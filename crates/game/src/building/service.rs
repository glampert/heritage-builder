use std::cmp::Reverse;
use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};
use strum::Display;

use common::{
    Color,
    callback::{self, Callback},
    coords::{Cell, CellRange},
    hash::{self, StringHash},
    time::{Seconds, UpdateTimer},
};
use engine::{log, ui::UiSystem};
use proc_macros::DrawDebugUi;

use super::{
    risks::RiskRates,
    Building,
    BuildingBehavior,
    BuildingContext,
    BuildingContextMut,
    BuildingKind,
    BuildingStock,
    config::{BuildingConfig, BuildingConfigs, building_config},
};
use crate::{
    cheats,
    tile::Tile,
    save_context::PostLoadContext,
    world::{object::GameObject, stats::WorldStats},
    undo_redo::{GameObjectSavedState, game_object_undo_redo_state},
    debug::game_object_debug::{GameObjectDebugOptions, debug_popup_msg_color, game_object_debug_options},
    sim::{
        SimCmds,
        SimContext, SimContextMut,
        finance::LedgerCategory,
        resources::{RESOURCE_KIND_COUNT, ResourceKind, ResourceKinds, ShoppingList, StockItem, Workers},
    },
    unit::{
        Unit,
        UnitTaskHelper,
        config::UnitConfigKey,
        patrol::*,
        runner::Runner,
        task::{
            UnitTaskDespawn,
            UnitTaskFetchCompletionCallback,
            UnitTaskFetchFromStorage,
            Emergency,
            UnitTaskRandomizedPatrol,
            UnitTaskRespond,
            UnitTaskRespondCompletionCallback,
            UnitTaskRespondState,
            find_nearest_emergency,
        },
    },
};

// ----------------------------------------------
// PatrolVisits
// ----------------------------------------------

// The buildings a service's patrol walker visits on its way.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
pub enum PatrolVisits {
    // Houses only: the services residents need.
    #[default]
    Houses,

    // Every building: fire watchers and builders, who look after any building's risks.
    AllBuildings,
}

impl PatrolVisits {
    #[inline]
    pub fn building_kinds(self) -> BuildingKind {
        match self {
            Self::Houses => BuildingKind::House,
            Self::AllBuildings => BuildingKind::all(),
        }
    }
}

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

#[derive(Clone, DrawDebugUi, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub kind: BuildingKind,

    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32, // How far our patrol unit can go.
    pub requires_road_access: bool,

    #[serde(default)]
    pub has_patrol_unit: bool,

    #[serde(default)]
    pub patrol_unit: UnitConfigKey,

    #[serde(default)]
    pub patrol_frequency_secs: Seconds,

    // Which buildings its patrol walker visits.
    #[serde(default)]
    #[debug_ui(format = "Patrol Visits : {}")]
    pub visits: PatrolVisits,

    #[serde(default)] // Optional if no `resources_required`.
    pub stock_update_frequency_secs: Seconds,

    // Kinds of resources required for the service to run, if any.
    #[serde(default)]
    pub resources_required: ResourceKinds,

    // Capacity for each resource kind it accepts.
    #[serde(default)]
    pub stock_capacity: u32,

    // How fast its fire and collapse risks build up, per calendar month (building::risks).
    #[serde(default)]
    #[debug_ui(nested)]
    pub risks: RiskRates,
}

impl Default for ServiceConfig {
    #[inline]
    fn default() -> Self {
        Self {
            kind: BuildingKind::SmallWell,
            name: "Small Well".into(),
            tile_def_name: "small_well".into(),
            tile_def_name_hash: hash::fnv1a_from_str("small_well"),
            min_workers: 0,
            max_workers: 0,
            effect_radius: 5,
            requires_road_access: false,
            has_patrol_unit: false,
            patrol_unit: UnitConfigKey::default(),
            patrol_frequency_secs: 0.0,
            visits: PatrolVisits::Houses,
            stock_update_frequency_secs: 0.0,
            resources_required: ResourceKinds::none(),
            stock_capacity: 0,
            risks: RiskRates::ZERO,
        }
    }
}

building_config! {
    ServiceConfig
}

// ----------------------------------------------
// ServiceDebug
// ----------------------------------------------

game_object_debug_options! {
    ServiceDebug,

    // Stops fetching resources from storage.
    freeze_stock_update: bool,

    // Stops sending out units on patrol.
    freeze_patrol: bool,
}

// ----------------------------------------------
// ServiceUndoRedoSavedState
// ----------------------------------------------

game_object_undo_redo_state! {
    ServiceUndoRedoSavedState,

    service_stock: ServiceStock,
}

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct ServiceBuilding {
    #[serde(skip)]
    config: Option<&'static ServiceConfig>,

    workers: Workers,

    // Stock of the resources this service requires, if any. (Tax offices hold no gold: their
    // collectors pay it into the city treasury.)
    pub(crate) service_stock: ServiceStock,

    // Runner Unit we may send out to fetch resources from storage. A fire watch, which has no
    // stock, sends a fire watcher to a fire with it instead (send_out_to_fire).
    pub(crate) runner: Runner,
    pub(crate) patrol: Patrol,            // Unit we may send out on patrol to provide the service.
    pub(crate) patrol_timer: UpdateTimer, // Min time before we can send out a new patrol unit.

    #[serde(skip)]
    debug: ServiceDebug,
}

// ----------------------------------------------
// BuildingBehavior for ServiceBuilding
// ----------------------------------------------

impl BuildingBehavior for ServiceBuilding {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &'static str {
        &self.config.unwrap().name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.config.unwrap()
    }

    fn despawned(&mut self, context: &BuildingContextMut) {
        self.runner.discard_spawn_promise(context.sim_ctx.cmds_mut());
        self.patrol.discard_spawn_promise(context.sim_ctx.cmds_mut());
    }

    fn update(&mut self, cmds: &mut SimCmds, context: &BuildingContext) {
        debug_assert!(self.config.is_some());

        self.runner.update(cmds);
        self.patrol.update(cmds);

        let delta_time_secs = context.sim_ctx.delta_time_secs();
        let has_min_required_workers = self.has_min_required_workers();
        let has_stock_requirements = self.service_stock.is_stock_and_requires_resources();
        let has_patrol_unit = self.has_patrol_unit();

        // Under-staffed services run slower: stock procurement and patrol dispatch
        // advance proportionally to how staffed the building is. The hard
        // min-worker gate still halts them entirely below the minimum.
        let service_delta_secs = delta_time_secs * self.work_efficiency();

        // Procure resources from storage periodically if we need them.
        if has_stock_requirements && has_min_required_workers && !self.debug.freeze_stock_update() {
            if let ServiceStock::Stock { update_timer, .. } = &mut self.service_stock {
                if update_timer.tick(service_delta_secs).should_update() {
                    self.stock_update(cmds, context);
                }
            }
        }

        if has_patrol_unit
            && has_min_required_workers
            && !self.debug.freeze_patrol()
            && self.patrol_timer.tick(service_delta_secs).should_update()
        {
            self.send_out_patrol_unit(cmds, context);
        }

        // A fire watch answers fires, and a hospital outbreaks, as well as patrolling
        // (freeze_patrol leaves this be).
        if has_min_required_workers {
            if context.kind == BuildingKind::FireWatch {
                self.send_out_responder(cmds, context, Emergency::Fire);
            } else if context.kind == BuildingKind::Hospital {
                self.send_out_responder(cmds, context, Emergency::Outbreak);
            }
        }
    }

    fn pre_save(&mut self, cmds: &mut SimCmds) {
        self.runner.pre_save(cmds);
        self.patrol.pre_save(cmds);
    }

    fn post_save(&mut self) {
        self.runner.post_save();
        self.patrol.post_save();
    }

    fn post_load(&mut self, _context: &mut PostLoadContext, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind.intersects(BuildingKind::services()));

        self.runner.post_load();
        self.patrol.post_load();

        let config = BuildingConfigs::get().find_service_config(kind);

        if let ServiceStock::Stock { update_timer, stock } = &mut self.service_stock {
            update_timer.post_load(config.stock_update_frequency_secs);
            stock.refresh_from_config(config.resources_required.combined(), config.stock_capacity);
        }

        self.patrol_timer.post_load(config.patrol_frequency_secs);
        self.config = Some(config);
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn has_stock(&self) -> bool {
        matches!(&self.service_stock, ServiceStock::Stock { .. })
    }

    fn is_stock_full(&self) -> bool {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => stock.is_full(),
            ServiceStock::None => false,
        }
    }

    fn stock(&self) -> ArrayVec<StockItem, RESOURCE_KIND_COUNT> {
        let mut items = ArrayVec::new();

        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => {
                stock.for_each(|_, item| {
                    items.push(*item);
                });
            }
            ServiceStock::None => {}
        }

        items
    }

    fn has_min_required_resources(&self) -> bool {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => stock.accepts_any() && !stock.is_empty(),
            ServiceStock::None => true,
        }
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => {
                if self.has_min_required_workers() {
                    return stock.available_resources(kind);
                }
            }
            ServiceStock::None => {}
        }
        0
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => {
                if self.has_min_required_workers() {
                    return stock.receivable_resources(kind);
                }
            }
            ServiceStock::None => {}
        }
        0
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let has_min_required_workers = self.has_min_required_workers();
            match &mut self.service_stock {
                ServiceStock::Stock { stock, .. } => {
                    if has_min_required_workers {
                        let received_count = stock.receive_resources(kind, count);
                        self.debug.log_resources_gained(kind, received_count);
                        return received_count;
                    }
                }
                ServiceStock::None => {}
            }
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let has_min_required_workers = self.has_min_required_workers();
            match &mut self.service_stock {
                ServiceStock::Stock { stock, .. } => {
                    if has_min_required_workers {
                        let removed_count = stock.remove_resources(kind, count);
                        self.debug.log_resources_lost(kind, removed_count);
                        return removed_count;
                    }
                }
                ServiceStock::None => {}
            }
        }
        0
    }

    fn tally(&self, stats: &mut WorldStats, kind: BuildingKind) {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => {
                if kind.intersects(BuildingKind::Market) {
                    stock.for_each(|_, item| {
                        stats.add_market_resources(item.kind, item.count);
                    });
                } else {
                    stock.for_each(|_, item| {
                        stats.add_service_resources(item.kind, item.count);
                    });
                }
            }
            ServiceStock::None => {}
        }
    }

    // ----------------------
    // Patrol/Runner/Workers:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> {
        Some(&mut self.patrol)
    }

    fn active_runner(&mut self) -> Option<&mut Runner> {
        Some(&mut self.runner)
    }

    fn workers(&self) -> Option<&Workers> {
        Some(&self.workers)
    }

    fn workers_mut(&mut self) -> Option<&mut Workers> {
        Some(&mut self.workers)
    }

    #[inline]
    fn has_min_required_workers(&self) -> bool {
        if cheats::get().ignore_worker_requirements {
            return true;
        }
        self.workers.as_employer().unwrap().has_min_required()
    }

    // ----------------------
    // Undo/Redo:
    // ----------------------

    fn undo_redo_record(&self) -> Option<Box<dyn GameObjectSavedState>> {
        ServiceUndoRedoSavedState::new_state(ServiceUndoRedoSavedState {
            service_stock: self.service_stock.clone(),
        })
    }

    fn undo_redo_apply(&mut self, state: &dyn GameObjectSavedState) {
        let saved_state = ServiceUndoRedoSavedState::downcast(state);

        // NOTE: Only stock is preserved on undo/redo. Runners/patrols and workers are reset.
        match &mut self.service_stock {
            ServiceStock::Stock { stock, .. } => {
                *stock = saved_state.service_stock.as_stock().clone();
            }
            ServiceStock::None => {
                debug_assert!(matches!(saved_state.service_stock, ServiceStock::None));
            }
        }
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, cmds: &mut SimCmds, context: &BuildingContextMut, ui_sys: &UiSystem) {
        // Debug-UI drawing lives in `crate::debug::building`.
        self.draw_debug_ui_dispatch(cmds, context, ui_sys);
    }
}

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

impl ServiceBuilding {
    pub fn new(config: &'static ServiceConfig) -> Self {
        Self {
            config: Some(config),
            workers: Workers::employer(config.min_workers, config.max_workers),
            service_stock: ServiceStock::new(config),
            runner: Runner::default(),
            patrol: Patrol::default(),
            patrol_timer: UpdateTimer::new(config.patrol_frequency_secs),
            debug: ServiceDebug::default(),
        }
    }

    pub fn register_callbacks() {
        let _: Callback<UnitTaskFetchCompletionCallback> = callback::register!(ServiceBuilding::on_resources_fetched);
        let _: Callback<PatrolCompletionCallback> = callback::register!(ServiceBuilding::on_patrol_completed);
        let _: Callback<UnitTaskRespondCompletionCallback> = callback::register!(ServiceBuilding::on_response_done);
    }

    // Fraction [0,1] the building runs at, based on how staffed it is relative to
    // max workers. The cheat forces full efficiency.
    #[inline]
    fn work_efficiency(&self) -> f32 {
        if cheats::get().ignore_worker_requirements {
            return 1.0;
        }
        self.workers.as_employer().unwrap().work_efficiency()
    }

    // ----------------------
    // Stock Update:
    // ----------------------

    fn stock_update(&mut self, cmds: &mut SimCmds, context: &BuildingContext) {
        if self.is_waiting_on_runner() {
            return; // A runner is already out fetching resources. Try again later.
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road. No stock update possible!
        };

        // Send out a runner:
        let storage_buildings_accepted = BuildingKind::storage(); // Search all storage building kinds.
        let resources_to_fetch = self.resource_fetch_list();
        if resources_to_fetch.is_empty() {
            return;
        }

        self.runner.try_fetch_from_storage(
            cmds,
            context,
            unit_origin,
            storage_buildings_accepted,
            resources_to_fetch,
            callback::create!(ServiceBuilding::on_resources_fetched),
        );
    }

    #[inline]
    fn is_stock_empty(&self) -> bool {
        match &self.service_stock {
            ServiceStock::Stock { stock, .. } => stock.is_empty(),
            ServiceStock::None => true,
        }
    }

    #[inline]
    pub(crate) fn is_waiting_on_runner(&self) -> bool {
        self.runner.is_spawned_or_pending_spawn()
    }

    #[inline]
    pub(crate) fn is_runner_fetching_resources(&self, context: &SimContext) -> bool {
        self.runner.is_running_task::<UnitTaskFetchFromStorage>(context)
    }

    fn on_resources_fetched(context: &SimContextMut, this_building: &mut Building, runner_unit: &mut Unit) {
        let this_service = this_building.as_service_mut();

        debug_assert!(this_service.is_runner_fetching_resources(context), "No Runner was sent out by this building!");
        debug_assert!(this_service.runner.unit_id() == runner_unit.id());

        // Try unload cargo. Any surplus stays in the runner's inventory -- the fetch
        // task's recovery path will ship it to a storage that can accept it.
        if let Some(item) = runner_unit.peek_inventory() {
            debug_assert!(item.count <= this_service.service_stock.as_stock().capacity_for(item.kind));

            let received_count = this_service.receive_resources(item.kind, item.count);
            if received_count != 0 {
                let removed_count = runner_unit.remove_resources(item.kind, received_count);
                debug_assert!(removed_count == received_count);
            }
        }
        // Else the runner failed to fetch the wanted resources and came back home empty handed.

        this_service.runner.reset();
        debug_popup_msg_color!(this_service.debug, Color::cyan(), "Fetch Task complete");
    }

    fn resource_fetch_list(&self) -> ShoppingList {
        let mut list = ShoppingList::default();
        let stock = self.service_stock.as_stock();

        // Only goods there's room for: a runner sent for a full one would bring nothing back.
        stock.for_each(|index, item| {
            debug_assert!(item.count <= stock.capacity_at(index), "{item}");
            let room = stock.capacity_at(index) - item.count;
            if room != 0 {
                list.push(StockItem { kind: item.kind, count: room });
            }
        });

        // Items with the highest capacity first.
        list.sort_by_key(|item: &StockItem| Reverse(item.count));
        list
    }

    // ----------------------
    // Patrol Update:
    // ----------------------

    fn send_out_patrol_unit(&mut self, cmds: &mut SimCmds, context: &BuildingContext) {
        if self.is_waiting_on_patrol() {
            return; // A patrol unit is already out. Try again later.
        }

        if context.kind == BuildingKind::Market && self.is_stock_empty() {
            // Markets will only send out a patrol if there are goods in stock.
            return;
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road!
        };

        let config = self.config.unwrap();

        // Visit houses, or every building (config.visits):
        self.patrol.start_randomized_patrol(
            cmds,
            context,
            unit_origin,
            config.patrol_unit,
            config.effect_radius,
            Some(config.visits.building_kinds()),
            callback::create!(ServiceBuilding::on_patrol_completed),
            None,
        );
    }

    #[inline]
    pub(crate) fn has_patrol_unit(&self) -> bool {
        self.config.unwrap().has_patrol_unit
    }

    #[inline]
    pub(crate) fn is_waiting_on_patrol(&self) -> bool {
        self.patrol.is_spawned_or_pending_spawn()
    }

    fn on_patrol_completed(context: &SimContextMut, this_building: &mut Building, patrol_unit: &mut Unit) {
        let this_building_kind = this_building.kind();
        let this_service = this_building.as_service_mut();

        debug_assert!(this_service.patrol.unit_id() == patrol_unit.id());
        debug_assert!(
            this_service.patrol.is_running_task::<UnitTaskRandomizedPatrol>(context),
            "No Patrol was sent out by this building!"
        );

        if let Some(item) = patrol_unit.peek_inventory() {
            // Only tax collector patrols bring back resources: the gold goes straight into the
            // city treasury, as tax income. Tax offices hold no gold of their own.
            if this_building_kind.intersects(BuildingKind::TaxOffice) && item.kind == ResourceKind::Gold {
                let tax_collected = patrol_unit.remove_resources(item.kind, item.count);
                context.finance_mut().earn(LedgerCategory::Taxes, tax_collected);

                debug_popup_msg_color!(this_service.debug, Color::yellow(), "Tax collected +{tax_collected}");
            } else {
                log::error!(
                    log::channel!("unit"),
                    "Patrol unit inventory has {} which {} - '{}' cannot receive!",
                    item,
                    this_building_kind,
                    this_service.name()
                );
            }

            patrol_unit.clear_inventory();
        }

        this_service.patrol.reset();
        debug_popup_msg_color!(this_service.debug, Color::magenta(), "Patrol complete");
    }
}

impl ServiceBuilding {
    // ----------------------
    // Emergencies (fire watch, hospital):
    // ----------------------

    // Sends a walker straight to the nearest building with `emergency` within the building's
    // effect radius, over any land (RESPONDER_NODE_KINDS), unless one is out already: a fire
    // watcher to a fire, a medic to an outbreak. He goes on to any other such emergency near it
    // before coming home. The patrol walker goes on patrolling.
    fn send_out_responder(&mut self, cmds: &mut SimCmds, context: &BuildingContext, emergency: Emergency) {
        if self.runner.is_spawned_or_pending_spawn() {
            return; // Out at one already (a fire watch and a hospital fetch no goods).
        }

        let Some(road_link) = context.road_link else {
            return;
        };

        let radius = self.config.unwrap().effect_radius;
        let sim_ctx = context.sim_ctx;

        // A cheap look first: a path search only when there's an emergency near enough.
        let mut nearby = false;
        sim_ctx.world().for_each_building_of_any_kind(|building| {
            nearby |= emergency.is_at(building) && cells_from(road_link, building.cell_range()) <= radius;
            !nearby
        });
        if !nearby {
            return;
        }

        // The nearest one within the radius that a walker can walk up to.
        let Some((target, _path)) = find_nearest_emergency(sim_ctx, emergency, road_link, radius, None) else {
            return;
        };

        let task = UnitTaskRespond {
            emergency,
            origin_building: context.kind_and_id(),
            origin_building_tile: context.tile_info(),
            target: target.kind_and_id(),
            target_tile: target.tile_info(),
            completion_callback: callback::create!(ServiceBuilding::on_response_done),
            completion_task: sim_ctx.task_manager_mut().new_task(UnitTaskDespawn::default()),
            state: UnitTaskRespondState::default(),
            completion_callback_done: false,
        };

        log::info!(
            log::channel!("risks"),
            "{} at {} sends a {} to the {} at {}.",
            self.name(),
            context.base_cell(),
            emergency.responder_name(),
            emergency.name(),
            target.base_cell()
        );
        self.runner.try_spawn_with_task(context.debug_name(), cmds, sim_ctx, road_link, self.config.unwrap().patrol_unit, task);
    }

    fn on_response_done(context: &SimContextMut, this_building: &mut Building, responder: &mut Unit) {
        let this_service = this_building.as_service_mut();

        debug_assert!(this_service.runner.unit_id() == responder.id());
        debug_assert!(this_service.runner.is_running_task::<UnitTaskRespond>(context), "No responder was sent out by this building!");

        this_service.runner.reset();
        debug_popup_msg_color!(this_service.debug, Color::magenta(), "Back from an emergency");
    }
}

// How many cells from `cell` to the nearest cell of `area` (Manhattan), 0 inside it.
fn cells_from(cell: Cell, area: CellRange) -> i32 {
    let dx = (area.start.x - cell.x).max(cell.x - area.end.x).max(0);
    let dy = (area.start.y - cell.y).max(cell.y - area.end.y).max(0);
    dx + dy
}

// ----------------------------------------------
// ServiceStock
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum ServiceStock {
    None,
    Stock {
        update_timer: UpdateTimer,
        stock: BuildingStock, // Current local stock of resources required (if any).
    },
}

impl ServiceStock {
    fn new(config: &'static ServiceConfig) -> Self {
        if !config.resources_required.is_empty() && config.stock_capacity != 0 {
            Self::Stock {
                update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
                stock: BuildingStock::with_accepted_list_and_capacity(&config.resources_required, config.stock_capacity),
            }
        } else {
            Self::None
        }
    }

    pub(crate) fn is_stock_and_requires_resources(&self) -> bool {
        match self {
            Self::Stock { stock, .. } => stock.accepts_any(),
            _ => false,
        }
    }

    fn as_stock(&self) -> &BuildingStock {
        match self {
            Self::Stock { stock, .. } => stock,
            _ => panic!("Not a BuildingStock!"),
        }
    }
}

