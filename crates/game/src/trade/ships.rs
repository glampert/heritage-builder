// Docks and ships (docs/plans/trade.md, §5): a river or sea partner's trade ship sails in from the
// map's edge to the nearest working dock its water reaches, moors at its berth, and waits while the
// dock's dockers carry its goods to the stores (the city pays as each load comes off) and the city's
// exports back to it (the partner pays as each load goes on). The ship's voyage is its task
// (unit::task::UnitTaskShip); the dock keeps its berth and its dockers (DockBerth, on its
// ServiceBuilding).

use std::collections::VecDeque;

use common::{
    callback::{self, Callback},
    coords::Cell,
};
use engine::log;
use serde::{Deserialize, Serialize};

use super::{
    NotSent,
    affordable,
    caravan_cargo,
    export_offered,
    export_price,
    has_exports,
    holdings,
    import_price,
    import_wanted,
    is_trading_with,
    terms,
};
use crate::{
    building::{Building, BuildingContext, BuildingKind, BuildingKindAndId},
    config::GameConfigs,
    pathfind::{Node, NodeKind as PathNodeKind, Path},
    sim::{
        SimCmdQueue,
        SimCmds,
        SimContext,
        SimContextMut,
        finance::LedgerCategory,
        resources::{ResourceKind, ShoppingList, StockItem},
    },
    tile::water,
    unit::{
        Unit,
        UnitId,
        UnitSpawnState,
        UnitTaskHelper,
        config::UnitConfigKey,
        runner::Runner,
        task::{
            TRADE_STORES,
            UnitTaskDeliverToStorage,
            UnitTaskDeliveryCompletionCallback,
            UnitTaskDeliveryState,
            UnitTaskDespawn,
            UnitTaskFetchCompletionCallback,
            UnitTaskFetchFromStorage,
            UnitTaskFetchState,
            UnitTaskShip,
        },
    },
    world::{
        World,
        object::{GameObject, Spawner},
    },
};

// The most dockers a dock sends at once, whatever the rules say.
const MAX_DOCKERS: usize = 4;

// ----------------------------------------------
// The dock's berth and dockers
// ----------------------------------------------

// A dock's state (saved on its ServiceBuilding): the ship moored at its berth, and its dockers.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DockBerth {
    // The ship moored (or mooring) at the berth.
    pub ship: Option<UnitId>,
    pub dockers: Vec<Docker>,
}

// A docker slot: the runner out on an errand, and the errand.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Docker {
    pub runner: Runner,
    pub errand: Option<DockerErrand>,
}

// A docker's errand: a load off the ship to a store (unloading), or from a store onto it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DockerErrand {
    pub good: ResourceKind,
    pub count: u32,
    pub unloading: bool,
}

impl DockBerth {
    pub fn new() -> Self {
        let mut berth = Self::default();
        berth.fit_dockers();
        berth
    }

    // As many docker slots as the rules give (1 to MAX_DOCKERS); a slot out on an errand is kept.
    pub fn fit_dockers(&mut self) {
        let wanted = (GameConfigs::get().sim.trade.dockers as usize).clamp(1, MAX_DOCKERS);
        while self.dockers.len() < wanted {
            self.dockers.push(Docker::default());
        }
        while self.dockers.len() > wanted && self.dockers.last().is_some_and(|docker| docker.errand.is_none()) {
            self.dockers.pop();
        }
    }

    // How many dockers are out, or about to set out.
    pub fn dockers_out(&self) -> usize {
        self.dockers.iter().filter(|docker| docker.runner.is_spawned_or_pending_spawn()).count()
    }

    fn is_spawning(&self) -> bool {
        self.dockers.iter().any(|docker| docker.runner.is_pending_spawn())
    }

    // The goods of `good` out on errands one way.
    pub fn in_flight(&self, good: ResourceKind, unloading: bool) -> u32 {
        self.errands().filter(|errand| errand.good == good && errand.unloading == unloading).map(|errand| errand.count).sum()
    }

    // The loads out to fill the ship.
    fn loads_in_flight(&self) -> u32 {
        self.errands().filter(|errand| !errand.unloading).map(|errand| errand.count).sum()
    }

    pub fn errands(&self) -> impl Iterator<Item = &DockerErrand> {
        self.dockers.iter().filter_map(|docker| docker.errand.as_ref())
    }

    // The slot of the docker `unit`, freed; its errand.
    fn free(&mut self, unit: UnitId) -> Option<DockerErrand> {
        let docker = self.dockers.iter_mut().find(|docker| docker.runner.is_spawned() && docker.runner.unit_id() == unit)?;
        docker.runner.reset();
        docker.errand.take()
    }

    pub fn pre_save(&mut self, cmds: &mut SimCmds) {
        for docker in &mut self.dockers {
            docker.runner.pre_save(cmds);
        }
    }

    pub fn post_save(&mut self) {
        for docker in &mut self.dockers {
            docker.runner.post_save();
        }
    }

    pub fn post_load(&mut self) {
        for docker in &mut self.dockers {
            docker.runner.post_load();
        }
        self.fit_dockers();
    }

    pub fn discard_spawn_promises(&mut self, cmds: &mut SimCmds) {
        for docker in &mut self.dockers {
            docker.runner.discard_spawn_promise(cmds);
        }
    }
}

pub fn register_callbacks() {
    let _: Callback<UnitTaskDeliveryCompletionCallback> = callback::register!(on_docker_unloaded);
    let _: Callback<UnitTaskFetchCompletionCallback> = callback::register!(on_docker_loaded);
}

// The berth of the dock `building`, if it's a dock.
pub fn berth_of(building: &Building) -> Option<&DockBerth> {
    if !building.is(BuildingKind::Dock) {
        return None;
    }
    building.as_service().dock.as_deref()
}

fn berth_of_mut(building: &mut Building) -> Option<&mut DockBerth> {
    if !building.is(BuildingKind::Dock) {
        return None;
    }
    building.as_service_mut().dock.as_deref_mut()
}

// The ship moored at `dock` (Some(None): none). None if the dock is gone.
pub fn berth_holder(context: &SimContext, dock: BuildingKindAndId) -> Option<Option<UnitId>> {
    let building = context.find_building(dock.kind, dock.id)?;
    berth_of(building).map(|berth| berth.ship)
}

// How many of the dock's dockers are out.
pub fn dockers_out(context: &SimContext, dock: BuildingKindAndId) -> usize {
    context.find_building(dock.kind, dock.id).and_then(berth_of).map_or(0, DockBerth::dockers_out)
}

// The ship `ship` claims the dock's berth: after the step, it holds it if no ship does.
pub fn claim_berth(cmds: &mut SimCmds, dock: BuildingKindAndId, ship: UnitId) {
    cmds.defer_building_update(dock, move |_context, building| {
        if let Some(berth) = berth_of_mut(building)
            && berth.ship.is_none()
        {
            berth.ship = Some(ship);
        }
    });
}

// The ship `ship` leaves the dock's berth.
pub fn release_berth(cmds: &mut SimCmds, dock: BuildingKindAndId, ship: UnitId) {
    cmds.defer_building_update(dock, move |_context, building| {
        if let Some(berth) = berth_of_mut(building)
            && berth.ship == Some(ship)
        {
            berth.ship = None;
        }
    });
}

// The ship moored at a dock, and its task.
pub fn moored_ship<'game>(context: &'game SimContext, berth: &DockBerth) -> Option<(&'game Unit, &'game UnitTaskShip)> {
    let unit = context.find_unit(berth.ship?)?;
    let task = unit.current_task_as::<UnitTaskShip>(context.task_manager())?;
    Some((unit, task))
}

// ----------------------------------------------
// The dockers' errands
// ----------------------------------------------

// The room the city's stores have for a good, the staffed ones that take it.
pub fn store_room(context: &SimContext, good: ResourceKind) -> u32 {
    let mut room = 0;
    context.world().for_each_building(TRADE_STORES, |store| {
        if store.is_operational() {
            room += store.receivable_resources(good);
        }
        true
    });
    room
}

// The next errand a dock's docker could run for its ship: first a load the city wants off it (below
// its import limit, within the partner's amounts, the stores' room and what the treasury pays), then
// a load of the city's exports onto it (above their limit, within its room). The loads already out
// count against each.
pub fn next_errand(context: &SimContext, berth: &DockBerth, ship: &UnitTaskShip) -> Option<DockerErrand> {
    if !is_trading_with(context, &ship.partner) {
        return None;
    }
    let trade = context.trade();
    let partner = trade.partner(&ship.partner)?;
    let terms = terms(trade, &ship.partner)?;
    let held = holdings(context);
    let faith_scale = context.faith().trade_scale();
    let load = GameConfigs::get().sim.trade.docker_load.max(1);

    for good in &terms.sells {
        let kind = good.good;
        if ship.carries(kind) == 0 {
            continue;
        }
        let out = berth.in_flight(kind, true);
        let count = load
            .min(ship.carries(kind))
            .min(import_wanted(partner, good, &held).saturating_sub(out))
            .min(store_room(context, kind).saturating_sub(out))
            .min(affordable(context, import_price(faith_scale, partner, good)));
        if count != 0 {
            return Some(DockerErrand { good: kind, count, unloading: true });
        }
    }

    let room = ship.room().saturating_sub(berth.loads_in_flight());
    for good in &terms.buys {
        let kind = good.good;
        let count = load.min(room).min(export_offered(partner, good, &held).saturating_sub(berth.in_flight(kind, false)));
        if count != 0 {
            return Some(DockerErrand { good: kind, count, unloading: false });
        }
    }
    None
}

// Whether a dock's dockers have something left to do for its ship.
pub fn has_errand(context: &SimContext, dock: BuildingKindAndId, ship: &UnitTaskShip) -> bool {
    let Some(berth) = context.find_building(dock.kind, dock.id).and_then(berth_of) else {
        return false;
    };
    next_errand(context, berth, ship).is_some()
}

// A dock's update: it frees its berth when its ship is gone, and while a ship is moored and it's
// staffed, sends a docker on the next errand whenever one is free.
pub fn update_dock(berth: &mut DockBerth, cmds: &mut SimCmds, context: &BuildingContext, staffed: bool) {
    let sim = context.sim_ctx;
    for docker in &mut berth.dockers {
        docker.runner.update(cmds);
        // A docker that failed to set out, or is gone without coming back, frees its slot.
        let gone = match docker.runner.spawn_state() {
            UnitSpawnState::Failed => true,
            UnitSpawnState::Spawned(_) => docker.runner.try_unit(sim).is_none(),
            _ => false,
        };
        if gone {
            docker.runner.reset();
            docker.errand = None;
        }
    }

    // The ship moored here is gone, or bound elsewhere: the berth is free.
    if let Some(ship) = berth.ship {
        let here = sim
            .find_unit(ship)
            .and_then(|unit| unit.current_task_as::<UnitTaskShip>(sim.task_manager()))
            .is_some_and(|task| task.dock == Some(context.kind_and_id()));
        if !here {
            berth.ship = None;
        }
    }

    // Send a docker: one at a time, each once the one before has set out (so the next errand counts it).
    if !staffed || berth.is_spawning() {
        return;
    }
    let Some((_, ship)) = moored_ship(sim, berth) else {
        return;
    };
    if !ship.is_moored_at(context.kind_and_id()) {
        return;
    }
    let Some(slot) = berth.dockers.iter().position(|docker| !docker.runner.is_spawned_or_pending_spawn()) else {
        return;
    };
    let Some(errand) = next_errand(sim, berth, ship) else {
        return;
    };
    let ship_id = berth.ship.unwrap();
    let city = ship.partner.clone();
    send_docker(&mut berth.dockers[slot], cmds, context, errand, ship_id, city);
}

fn send_docker(
    docker: &mut Docker,
    cmds: &mut SimCmds,
    context: &BuildingContext,
    errand: DockerErrand,
    ship: UnitId,
    city: String,
) {
    let sim = context.sim_ctx;
    let Some(origin) = context.road_link else {
        return;
    };
    let completion_task = sim.task_manager_mut().new_task(UnitTaskDespawn::default());
    if errand.unloading {
        // The docker takes the load off the ship as it sets out, and the city pays for it then
        // (take_off_ship); then it carries it to the nearest store that takes it.
        let task = UnitTaskDeliverToStorage {
            origin_building: context.kind_and_id(),
            origin_building_tile: context.tile_info(),
            storage_buildings_accepted: TRADE_STORES,
            resource_kind_to_deliver: errand.good,
            resource_count: errand.count,
            completion_callback: callback::create!(on_docker_unloaded),
            completion_task,
            allow_producer_fallback: false,
            state: UnitTaskDeliveryState::default(),
            visit_delivered: None,
        };
        let promise = Unit::try_spawn_with_task_deferred_promise(
            cmds,
            sim,
            origin,
            UnitConfigKey::Runner,
            task,
            move |context, result| match result {
                Ok(unit) => take_off_ship(context, ship, &city, errand, unit),
                Err(err) => log::error!(log::channel!("trade"), "A docker couldn't set out: {}", err.message),
            },
        );
        docker.runner.set_spawn_state(UnitSpawnState::Pending(promise));
    } else {
        // The docker fetches a load from the nearest store that holds it and brings it back; it goes
        // onto the ship as it arrives (on_docker_loaded).
        let task = UnitTaskFetchFromStorage {
            origin_building: context.kind_and_id(),
            origin_building_tile: context.tile_info(),
            storage_buildings_accepted: TRADE_STORES,
            resources_to_fetch: ShoppingList::from_items(&[StockItem { kind: errand.good, count: errand.count }]),
            completion_callback: callback::create!(on_docker_loaded),
            completion_task,
            state: UnitTaskFetchState::default(),
            visit_outcome: None,
            completion_callback_done: false,
        };
        docker.runner.try_spawn_with_task(context.debug_name(), cmds, sim, origin, UnitConfigKey::Runner, task);
    }
    docker.errand = Some(errand);
    log::info!(
        log::channel!("trade"),
        "A docker sets out to {} {} {} for a ship from '{}'.",
        if errand.unloading { "unload" } else { "load" },
        errand.count,
        errand.good,
        ship_partner_name(sim, ship)
    );
}

fn ship_partner_name(context: &SimContext, ship: UnitId) -> String {
    context
        .find_unit(ship)
        .and_then(|unit| unit.current_task_as::<UnitTaskShip>(context.task_manager()))
        .map_or_else(String::new, |task| task.partner.clone())
}

// A docker sets out with a load off the ship: the ship gives it up, and the city pays for it
// (`Imports`), counted toward the partner's amount this year. If the ship has less (or is gone), the
// docker carries only what it took.
fn take_off_ship(context: &SimContextMut, ship: UnitId, city: &str, errand: DockerErrand, docker: &mut Unit) {
    let kind = errand.good;
    let taken = (|| {
        let task = context.find_unit_mut(ship)?.current_task_as_mut::<UnitTaskShip>(context.task_manager_mut())?;
        let terms = terms(context.trade(), city)?;
        let good = *terms.sells(kind)?;
        let faith_scale = context.faith().trade_scale();
        let trade = context.trade_mut();
        let partner = trade.partner_mut(city)?;
        let taken = errand.count.min(task.carries(kind));
        let price = import_price(faith_scale, partner, &good);
        task.cargo[kind.index()] -= taken;
        task.sold[kind.index()] += taken;
        partner.sold[kind.index()] += taken;
        let gold = taken * price;
        context.finance_mut().spend(LedgerCategory::Imports, gold);
        task.paid += gold;
        Some(taken)
    })()
    .unwrap_or(0);
    if taken < errand.count {
        docker.remove_resources(kind, errand.count - taken);
    }
}

// A docker delivered its load off the ship to a store: its slot is free.
fn on_docker_unloaded(_context: &SimContextMut, dock: &mut Building, docker: &mut Unit) {
    if let Some(berth) = berth_of_mut(dock) {
        berth.free(docker.id());
    }
}

// A docker came back to the dock with a load for the ship: it goes on, as far as the ship has room
// and the partner buys this year, and the partner pays for it (`Exports`). What doesn't go on (the
// ship gone or full) stays with the docker, who takes it back to a store (the fetch task's recovery).
fn on_docker_loaded(context: &SimContextMut, dock: &mut Building, docker: &mut Unit) {
    let dock_id = dock.kind_and_id();
    let Some(berth) = berth_of_mut(dock) else {
        return;
    };
    berth.free(docker.id());
    let Some(item) = docker.peek_inventory() else {
        return; // Came back empty-handed.
    };
    let Some(ship) = berth.ship else {
        return;
    };
    let kind = item.kind;
    let loaded = (|| {
        let task = context.find_unit_mut(ship)?.current_task_as_mut::<UnitTaskShip>(context.task_manager_mut())?;
        if !task.is_moored_at(dock_id) {
            return None;
        }
        let city = task.partner.clone();
        let terms = terms(context.trade(), &city)?;
        let good = *terms.buys(kind)?;
        let faith_scale = context.faith().trade_scale();
        let trade = context.trade_mut();
        let partner = trade.partner_mut(&city)?;
        let left = good.per_year.saturating_sub(partner.bought[kind.index()]);
        let count = item.count.min(task.room()).min(left);
        let removed = docker.remove_resources(kind, count);
        task.bought[kind.index()] += removed;
        partner.bought[kind.index()] += removed;
        let gold = removed * export_price(faith_scale, partner, &good);
        context.finance_mut().earn(LedgerCategory::Exports, gold);
        task.earned += gold;
        Some(removed)
    })();
    if loaded.is_none_or(|count| count < item.count) {
        log::info!(
            log::channel!("trade"),
            "A docker brought {} {kind} back to a dock whose ship can't take them all.",
            item.count
        );
    }
}

// ----------------------------------------------
// Sailing: the water's paths
// ----------------------------------------------

// Whether a ship can sail on `cell`: open water (not a canal), with no building over it.
fn is_sailable(context: &SimContext, cell: Cell) -> bool {
    context.tile_map().is_cell_within_bounds(cell)
        && context.graph().node_kind(Node::new(cell)).is_some_and(|kind| kind.intersects(PathNodeKind::Water))
        && water::is_open_water(context.tile_map(), cell)
}

const SIDES: [(i32, i32); 4] = [(1, 0), (0, -1), (-1, 0), (0, 1)];

fn neighbours(cell: Cell) -> impl Iterator<Item = Cell> {
    SIDES.iter().map(move |(dx, dy)| Cell::new(cell.x + dx, cell.y + dy))
}

// A breadth-first search over the open water from `starts`, visiting each cell in order of distance
// until `visit` returns true; the path to that cell (from its start) if it does.
struct WaterSearch {
    width: i32,
    parents: Vec<u32>,
}

const UNVISITED: u32 = u32::MAX;
const ROOT: u32 = u32::MAX - 1;

impl WaterSearch {
    fn run(context: &SimContext, starts: &[Cell], mut visit: impl FnMut(Cell) -> bool) -> Option<Path> {
        let size = context.map_size_in_cells();
        let mut search = Self { width: size.width, parents: vec![UNVISITED; (size.width * size.height).max(0) as usize] };
        let mut queue = VecDeque::new();
        for &start in starts {
            if let Some(index) = search.index(start)
                && search.parents[index] == UNVISITED
            {
                search.parents[index] = ROOT;
                queue.push_back(start);
            }
        }
        while let Some(cell) = queue.pop_front() {
            if visit(cell) {
                return Some(search.path_to(cell));
            }
            for next in neighbours(cell) {
                if let Some(index) = search.index(next)
                    && search.parents[index] == UNVISITED
                    && is_sailable(context, next)
                {
                    search.parents[index] = search.index(cell).unwrap() as u32;
                    queue.push_back(next);
                }
            }
        }
        None
    }

    fn index(&self, cell: Cell) -> Option<usize> {
        let height = self.parents.len() as i32 / self.width.max(1);
        (cell.x >= 0 && cell.y >= 0 && cell.x < self.width && cell.y < height)
            .then(|| (cell.y * self.width + cell.x) as usize)
    }

    fn path_to(&self, cell: Cell) -> Path {
        let mut path = vec![Node::new(cell)];
        let mut index = self.index(cell).unwrap();
        while self.parents[index] != ROOT {
            index = self.parents[index] as usize;
            path.push(Node::new(Cell::new(index as i32 % self.width, index as i32 / self.width)));
        }
        path.reverse();
        path
    }
}

// The path over open water from `start` to the nearest cell `is_goal` accepts.
pub fn find_water_path(context: &SimContext, start: Cell, is_goal: impl FnMut(Cell) -> bool) -> Option<Path> {
    WaterSearch::run(context, &[start], is_goal)
}

// The dock beside `cell` (four neighbours), if any.
fn dock_beside<'game>(context: &'game SimContext, cell: Cell) -> Option<&'game Building> {
    neighbours(cell).find_map(|next| context.find_building_for_cell(next).filter(|building| building.is(BuildingKind::Dock)))
}

// A dock ships trade at: finished, staffed and not burning.
fn is_working_dock(building: &Building) -> bool {
    building.is(BuildingKind::Dock) && building.is_operational()
}

// What a ship's search for a berth found.
pub enum BerthSearch {
    // The nearest working dock with a free berth: the dock, its berth and the way there.
    Free { dock: BuildingKindAndId, berth: Cell, path: Path },
    // Every working dock in reach is busy: the way to wait off the nearest.
    Busy { path: Path },
    // No working dock in reach.
    None,
}

// How far off a busy dock a ship waits, in cells.
const WAIT_OFF_CELLS: usize = 3;

// The nearest working dock the ship `me` at `start` can reach over open water whose berth is free (or
// already its own).
pub fn find_berth(context: &SimContext, start: Cell, me: UnitId) -> BerthSearch {
    let mut busy: Option<Path> = None;
    let mut found: Option<BuildingKindAndId> = None;
    let path = WaterSearch::run(context, &[start], |cell| {
        let Some(dock) = dock_beside(context, cell).filter(|dock| is_working_dock(dock)) else {
            return false;
        };
        let holder = berth_of(dock).and_then(|berth| berth.ship);
        if holder.is_none_or(|ship| ship == me) {
            found = Some(dock.kind_and_id());
            return true;
        }
        if busy.is_none() {
            // Remember the way to the nearest busy one, a few cells short of its berth.
            busy = find_water_path(context, start, |goal| goal == cell);
        }
        false
    });
    match (path, found) {
        (Some(path), Some(dock)) => BerthSearch::Free { dock, berth: path.last().unwrap().cell, path },
        _ => match busy {
            Some(mut path) => {
                let keep = path.len().saturating_sub(WAIT_OFF_CELLS).max(1);
                path.truncate(keep);
                BerthSearch::Busy { path }
            }
            None => BerthSearch::None,
        },
    }
}

// Whether `cell` is open water at the map's edge (a side beyond the playable area).
fn is_edge(context: &SimContext, cell: Cell) -> bool {
    neighbours(cell).any(|next| !context.tile_map().is_cell_within_playable_area(next))
}

// Where a partner's ships come in: its `entry` cell when it's open water, else the open water at the
// map's edge nearest a working dock (through the water that dock is on). None without a working dock
// whose water reaches the edge.
pub fn ship_entry(context: &SimContext, city: &str) -> Option<Cell> {
    let partner = context.trade().partner(city)?;
    if let Some([x, y]) = partner.def.entry {
        let cell = Cell::new(x, y);
        if is_sailable(context, cell) {
            return Some(cell);
        }
        log::warning!(
            log::channel!("trade"),
            "Trade partner '{city}': its entry {cell} isn't open water a ship can sail on; the map's edge stands in."
        );
    }
    let mut berths = Vec::new();
    context.world().for_each_building(BuildingKind::Dock, |dock| {
        if is_working_dock(dock) {
            crate::pathfind::for_each_surrounding_cell(dock.cell_range(), |cell| {
                if is_sailable(context, cell) {
                    berths.push(cell);
                }
                true
            });
        }
        true
    });
    if berths.is_empty() {
        return None;
    }
    let path = WaterSearch::run(context, &berths, |cell| is_edge(context, cell))?;
    path.last().map(|node| node.cell)
}

// Whether a working dock's berth is in reach of `start` over open water.
pub fn reaches_working_dock(context: &SimContext, start: Cell) -> bool {
    WaterSearch::run(context, &[start], |cell| dock_beside(context, cell).is_some_and(is_working_dock)).is_some()
}

// Whether the city has a finished dock (river and sea routes open only then).
pub fn has_dock(context: &SimContext) -> bool {
    world_has_dock(context.world())
}

pub fn world_has_dock(world: &World) -> bool {
    let mut any = false;
    world.for_each_building(BuildingKind::Dock, |dock| {
        any = !dock.is_under_construction();
        !any
    });
    any
}

// Sends a ship from `city` now, whatever its schedule (a river or sea partner).
pub fn send_ship(context: &SimContextMut, city: &str) -> Result<(), NotSent> {
    if !is_trading_with(context, city) {
        return Err(NotSent::NotTrading);
    }
    let trade = context.trade();
    let (Some(partner), Some(terms)) = (trade.partner(city), terms(trade, city)) else {
        return Err(NotSent::NotTrading);
    };
    // Nothing sails while no working dock is in reach of its entry.
    let Some(entry) = ship_entry(context, city).filter(|&entry| reaches_working_dock(context, entry)) else {
        return Err(NotSent::NoDock);
    };
    let rules = &GameConfigs::get().sim.trade;
    let held = holdings(context);
    let cargo = caravan_cargo(partner, &terms, &held, rules.ship_capacity);
    if cargo.iter().all(|&count| count == 0) && !has_exports(partner, &terms, &held) {
        return Err(NotSent::NothingToTrade);
    }
    // At once: this runs outside the world step (the month's start, automation), as a caravan does.
    match Spawner::new(context).try_spawn_unit_with_config(entry, UnitConfigKey::TradeShip) {
        Ok(unit) => {
            let task = UnitTaskShip::new(city, entry, cargo, rules.ship_capacity);
            let task_id = context.task_manager_mut().new_task(task);
            unit.assign_task(context.task_manager_mut(), task_id);
            log::info!(log::channel!("trade"), "A ship sets out from '{city}' at {entry}.");
            Ok(())
        }
        Err(err) => {
            log::error!(log::channel!("trade"), "A ship from '{city}' couldn't set out at {entry}: {}", err.message);
            Err(NotSent::NoEntry)
        }
    }
}
