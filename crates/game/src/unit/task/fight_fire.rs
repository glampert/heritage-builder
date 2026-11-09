use std::any::Any;
use serde::{Deserialize, Serialize};

use common::{callback::Callback, coords::Cell};
use engine::{log, ui::{DrawDebugUi, UiSystem}};
use proc_macros::DrawDebugUi;

use super::{
    UnitTaskOriginBuildingDebug,
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    invoke_completion_callback_deferred,
    with_task,
};
use crate::{
    building::{Building, BuildingKindAndId, BuildingTileInfo, config::BuildingConfigs},
    pathfind::{Node, NodeKind as PathNodeKind, Path, SearchResult},
    sim::{SimCmdQueue, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskFightFire
// ----------------------------------------------

// A fire watcher a fire watch sends straight to a fire (docs/plans/city-risks.md, §3): it walks
// over any land to the burning building, road or not, puts the fire out, and goes on to the
// nearest other fire within its fire watch's radius of where it stands, until none is left; then
// it goes home. Building::visited_by knows it by this task (building::risks::risk_reset_by).

pub type UnitTaskFightFireCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

// The ground a fire watcher crosses on a fire's call: any land, on the road or off it, but not
// water, and not through buildings, rubble, rocks or trees.
pub const FIRE_WATCHER_NODE_KINDS: PathNodeKind = PathNodeKind::from_bits_retain(
    PathNodeKind::EmptyLand.bits()
        | PathNodeKind::Road.bits()
        | PathNodeKind::VacantLot.bits()
        | PathNodeKind::SettlersSpawnPoint.bits(),
);

// The nearest building burning within `radius` cells of `start`, other than `except`, that a
// fire watcher can walk up to, and the way to the cell beside it where he'd stand.
pub fn find_nearest_fire<'ctx>(
    context: &'ctx SimContext,
    start: Cell,
    radius: i32,
    except: Option<BuildingKindAndId>,
) -> Option<(&'ctx Building, &'ctx Path)> {
    let mut fire = None;

    let result = context.find_path_next_to(FIRE_WATCHER_NODE_KINDS, start, Some(radius), |cell| {
        if !context.graph().node_kind(Node::new(cell)).is_some_and(PathNodeKind::is_building) {
            return false;
        }
        let burning = context
            .find_building_for_cell(cell)
            .filter(|building| building.is_burning() && Some(building.kind_and_id()) != except);
        if burning.is_some() {
            fire = burning; // The search stops here.
        }
        burning.is_some()
    });

    match result {
        SearchResult::PathFound(path) => fire.map(|building| (building, path)),
        SearchResult::PathNotFound => None,
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskFightFireState {
    // Walking to a cell next to the burning building.
    #[default]
    MovingToFire,

    // Walking back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    ReportingToOrigin,

    // Terminal state.
    Done,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskFightFire {
    // Origin building info: the fire watch.
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // The burning building the walker is on his way to. He stands on whichever cell next to it
    // he reaches first, so its road link (if it has one) doesn't matter.
    pub fire: BuildingKindAndId,
    pub fire_tile: BuildingTileInfo,

    // Called on the origin building once the walker is back.
    // `|context, origin_building, fire_watcher|`
    pub completion_callback: Callback<UnitTaskFightFireCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    pub state: UnitTaskFightFireState,

    // Set by the deferred completion callback; consumed by `ReportingToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,
}

impl UnitTaskFightFire {
    // The fire is still burning where it was when the walker set out.
    fn fire_is_burning(&self, sim_context: &SimContext) -> bool {
        sim_context.find_building(self.fire.kind, self.fire.id).is_some_and(Building::is_burning)
    }

    // How far from where he stands a fire watcher looks for the next fire: his fire watch's reach.
    fn search_radius(&self) -> i32 {
        BuildingConfigs::get().find_service_config(self.origin_building.kind).effect_radius
    }

    // Heads for `fire`, along `path`, which ends next to it.
    fn walk_to(&mut self, ctx: &mut UnitTaskContext, fire: &Building, path: &Path) {
        self.fire = fire.kind_and_id();
        self.fire_tile = fire.tile_info();

        // The goal is where the path ends, the cell he'll stand on, in place of a road link.
        let stand_on = BuildingTileInfo { road_link: path.last().unwrap().cell, base_cell: self.fire_tile.base_cell };
        let goal =
            UnitNavGoal::building(self.origin_building.kind, self.origin_building_tile.base_cell, self.fire.kind, stand_on);
        ctx.unit.move_to_goal(path, goal);
    }

    // Sets out for the fire, or finds another way after the path was blocked.
    fn try_route_to_fire(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;
        let Some(fire) = sim_context.find_building(self.fire.kind, self.fire.id) else {
            return false;
        };
        let footprint = fire.cell_range();

        match sim_context.find_path_next_to(FIRE_WATCHER_NODE_KINDS, ctx.unit.cell(), None, |cell| footprint.contains(cell)) {
            SearchResult::PathFound(path) => {
                self.walk_to(ctx, fire, path);
                true
            }
            SearchResult::PathNotFound => {
                log::info!(log::channel!("task"), "TaskFightFire: no way to the fire at {}.", self.fire_tile.base_cell);
                false
            }
        }
    }

    // Sets out for the nearest fire within reach of where he stands but `except`, the one he's
    // just put out (its visit hasn't run yet, so it's still burning).
    fn try_route_to_next_fire(&mut self, ctx: &mut UnitTaskContext, except: Option<BuildingKindAndId>) -> bool {
        let sim_context = ctx.sim_context;
        match find_nearest_fire(sim_context, ctx.unit.cell(), self.search_radius(), except) {
            Some((fire, path)) => {
                log::info!(log::channel!("risks"), "A fire watcher goes on to the fire at {}.", fire.base_cell());
                self.walk_to(ctx, fire, path);
                true
            }
            None => false,
        }
    }

    // On to the next fire, or home if there's none.
    fn next_fire_or_home(
        &mut self,
        ctx: &mut UnitTaskContext,
        except: Option<BuildingKindAndId>,
    ) -> UnitTaskTransition<UnitTaskFightFireState> {
        if self.try_route_to_next_fire(ctx, except) {
            UnitTaskTransition::Stay
        } else {
            UnitTaskTransition::Goto(UnitTaskFightFireState::ReturningToOrigin)
        }
    }

    // Queues the visit that puts the fire out (Building::visited_by). It runs with this step's
    // commands, before a burning down queued in the same step.
    fn fight_fire(&mut self, ctx: &mut UnitTaskContext) {
        if self.fire_is_burning(ctx.sim_context) {
            ctx.sim_cmds.visit_building(self.fire, ctx.unit.id());
        }
    }

    fn try_return_to_origin(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;

        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::info!(log::channel!("task"), "TaskFightFire: the fire watch is gone; the walker leaves.");
            return false;
        }

        let start = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        match sim_context.find_path(traversable_node_kinds, start, self.origin_building_tile.road_link) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                ctx.unit.move_to_goal(path, goal);
                true
            }
            SearchResult::PathNotFound => {
                log::info!(log::channel!("task"), "TaskFightFire: no road back to the fire watch; the walker leaves.");
                false
            }
        }
    }

    fn schedule_completion_callback(&mut self, ctx: &mut UnitTaskContext) -> bool {
        invoke_completion_callback_deferred(
            ctx.unit,
            ctx.sim_cmds,
            ctx.sim_context,
            self.origin_building.kind,
            self.origin_building.id,
            self.completion_callback.get(),
            |context, _building, unit| {
                with_task::<UnitTaskFightFire>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_moving_to_fire(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if ctx.unit.goal().is_none() {
            // Set out, or find another way after the path was blocked. A fire that's out (or
            // can't be reached) needs no one, but there may be another.
            if self.fire_is_burning(ctx.sim_context) && self.try_route_to_fire(ctx) {
                return UnitTaskTransition::Stay;
            }
            return self.next_fire_or_home(ctx, Some(self.fire));
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // A path that's just the cell he stands on (he's next to the fire already) gets here too.
        ctx.unit.follow_path(None);
        self.fight_fire(ctx);
        self.next_fire_or_home(ctx, Some(self.fire))
    }

    fn update_returning(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if ctx.unit.goal().is_none() {
            if ctx.unit.cell() == self.origin_building_tile.road_link {
                return self.report_to_origin(ctx);
            }
            if self.try_return_to_origin(ctx) {
                return UnitTaskTransition::Stay;
            }
            return UnitTaskTransition::Goto(UnitTaskFightFireState::Done);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        ctx.unit.follow_path(None);
        self.report_to_origin(ctx)
    }

    fn report_to_origin(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            UnitTaskTransition::Goto(UnitTaskFightFireState::ReportingToOrigin)
        } else {
            // No completion callback, or the fire watch is gone.
            UnitTaskTransition::Goto(UnitTaskFightFireState::Done)
        }
    }

    fn update_reporting(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if self.completion_callback_done {
            UnitTaskTransition::Goto(UnitTaskFightFireState::Done)
        } else {
            UnitTaskTransition::Stay
        }
    }
}

impl UnitTaskState for UnitTaskFightFireState {
    type Task = UnitTaskFightFire;

    fn update(self, task: &mut UnitTaskFightFire, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::MovingToFire      => task.update_moving_to_fire(ctx),
            Self::ReturningToOrigin => task.update_returning(ctx),
            Self::ReportingToOrigin => task.update_reporting(ctx),
            Self::Done              => UnitTaskTransition::Done,
        }
    }
}

impl UnitTask for UnitTaskFightFire {
    type State = UnitTaskFightFireState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert_eq!(ctx.unit.cell(), self.origin_building_tile.road_link); // We start at the fire watch's road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.fire.is_valid());

        // On a fire's call, fire watchers go off-road, there and back.
        let current_node_kinds = ctx.unit.traversable_node_kinds();
        ctx.unit.set_traversable_node_kinds(current_node_kinds | FIRE_WATCHER_NODE_KINDS);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }

    fn completion_task(&mut self) -> Option<UnitTaskId> {
        self.completion_task.take()
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, sim_context: &SimContext, ui_sys: &UiSystem) {
        #[derive(DrawDebugUi)]
        struct View {
            origin_building: UnitTaskOriginBuildingDebug,
            #[debug_ui(label = "Fire")]
            fire: UnitTaskOriginBuildingDebug,
            fire_is_burning: bool,
            #[debug_ui(debug)]
            state: UnitTaskFightFireState,
            has_completion_callback: bool,
            has_completion_task: bool,
        }
        View {
            origin_building: UnitTaskOriginBuildingDebug::new(self.origin_building, self.origin_building_tile),
            fire: UnitTaskOriginBuildingDebug::new(self.fire, self.fire_tile),
            fire_is_burning: self.fire_is_burning(sim_context),
            state: self.state,
            has_completion_callback: self.completion_callback.is_valid(),
            has_completion_task: self.completion_task.is_some(),
        }
        .draw_debug_ui(ui_sys);
    }
}
