use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::{log, ui::UiSystem};

use super::{
    PathFindResult,
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    find_delivery_candidate,
    find_storage_fetch_candidate,
    invoke_completion_callback_deferred,
    visit_destination_deferred,
    with_task,
};
use crate::{
    debug,
    pathfind::SearchResult,
    tile::TileMapLayerKind,
    unit::{Unit, navigation::UnitNavGoal},
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    sim::{SimContext, resources::ShoppingList},
};

// ----------------------------------------------
// UnitTaskFetchFromStorage
// ----------------------------------------------

pub type UnitTaskFetchCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskFetchState {
    // Looking for a storage that has one of the wanted resources.
    #[default]
    Searching,

    // Walking to the chosen storage.
    MovingToStorage,

    // At the storage, waiting for the deferred fetch visit to resolve.
    VisitingStorage,

    // Walking back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    DeliveringToOrigin,

    // Recovery: the origin is unreachable, so the cargo is being routed
    // to any storage that will take it.
    RoutingSurplus,

    // At a recovery storage, waiting for the deferred surplus unload.
    UnloadingSurplus,

    // Terminal state.
    Done,
}

// Fetch goods from a storage building.
// Storage -> Producer | Storage -> Storage
#[derive(Serialize, Deserialize)]
pub struct UnitTaskFetchFromStorage {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Resources to fetch:
    pub storage_buildings_accepted: BuildingKind,

    // Will fetch at most *one* of these. This is a list of desired options.
    pub resources_to_fetch: ShoppingList,

    // Called on the origin building once the unit has returned with resources.
    // `|context, origin_building, runner_unit|`
    pub completion_callback: Callback<UnitTaskFetchCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    pub state: UnitTaskFetchState,

    // Outcome of a deferred building visit (`VisitingStorage` / `UnloadingSurplus`):
    // `Some(true)` good, `Some(false)` retry elsewhere.
    #[serde(skip)]
    pub visit_outcome: Option<bool>,

    // Set by the deferred completion callback; consumed by `DeliveringToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,
}

impl UnitTaskFetchFromStorage {
    fn try_find_goal(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let origin_cell = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        for resource_to_fetch in self.resources_to_fetch.iter() {
            debug_assert!(resource_to_fetch.kind.is_single_resource());

            let path_find_result = find_storage_fetch_candidate(
                ctx.sim_context,
                self.origin_building.kind,
                origin_cell,
                traversable_node_kinds,
                self.storage_buildings_accepted,
                resource_to_fetch.kind,
            );

            if let PathFindResult::Success { path, goal } = path_find_result {
                ctx.unit.move_to_goal(path, goal);
                return true;
            }
            // Else no path or Storage building found. Try the next resource.
        }

        false
    }

    fn try_return_to_origin(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;

        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::info!(log::channel!("task"), "Origin building no longer valid; TaskFetchFromStorage will attempt surplus recovery.");
            return false;
        }

        let start = ctx.unit.cell();
        let goal_cell = self.origin_building_tile.road_link;
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        match sim_context.find_path(traversable_node_kinds, start, goal_cell) {
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
                log::info!(
                    log::channel!("task"),
                    "Origin building unreachable (no road access); TaskFetchFromStorage will attempt surplus recovery."
                );
                false
            }
        }
    }

    // Recovery: look for any storage that will receive the surplus cargo.
    fn try_route_surplus_to_storage(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let Some(item) = ctx.unit.peek_inventory() else { return false; };
        let item_kind = item.kind;

        let origin_cell = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        let path_find_result = find_delivery_candidate(
            ctx.sim_context,
            self.origin_building.kind,
            origin_cell,
            traversable_node_kinds,
            self.storage_buildings_accepted,
            item_kind,
        );

        if let PathFindResult::Success { path, goal } = path_find_result {
            ctx.unit.move_to_goal(path, goal);
            true
        } else {
            false
        }
    }

    // Schedules the deferred storage visit (collect a resource).
    fn try_visit_storage(&mut self, ctx: &mut UnitTaskContext) -> bool {
        visit_destination_deferred(ctx.unit, ctx.sim_cmds, ctx.sim_context, |context, _building, unit, _result| {
            with_task::<Self>(unit, context, |task, unit| {
                // Did we collect a resource from this storage?
                task.visit_outcome = Some(!unit.inventory_is_empty());
            });
        })
    }

    // Schedules the deferred surplus unload visit.
    fn try_visit_for_surplus(&mut self, ctx: &mut UnitTaskContext) -> bool {
        visit_destination_deferred(ctx.unit, ctx.sim_cmds, ctx.sim_context, |context, _building, unit, _result| {
            with_task::<Self>(unit, context, |task, unit| {
                // Surplus fully deposited?
                task.visit_outcome = Some(unit.inventory_is_empty());
            });
        })
    }

    // Schedules the deferred completion callback on the origin building.
    fn schedule_completion_callback(&mut self, ctx: &mut UnitTaskContext) -> bool {
        invoke_completion_callback_deferred(
            ctx.unit,
            ctx.sim_cmds,
            ctx.sim_context,
            self.origin_building.kind,
            self.origin_building.id,
            self.completion_callback.get(),
            |context, _building, unit| {
                with_task::<Self>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_searching(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if self.try_find_goal(ctx) {
            UnitTaskTransition::Goto(UnitTaskFetchState::MovingToStorage)
        } else {
            // No storage has what we want; head home empty-handed.
            UnitTaskTransition::Goto(UnitTaskFetchState::ReturningToOrigin)
        }
    }

    fn update_moving_to_storage(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if ctx.unit.goal().is_none() {
            return UnitTaskTransition::Goto(UnitTaskFetchState::Searching);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        let scheduled = self.try_visit_storage(ctx);
        ctx.unit.follow_path(None);

        if scheduled {
            UnitTaskTransition::Goto(UnitTaskFetchState::VisitingStorage)
        } else {
            // Destination storage no longer valid; find another.
            UnitTaskTransition::Goto(UnitTaskFetchState::Searching)
        }
    }

    fn update_visiting_storage(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        match self.visit_outcome.take() {
            None        => UnitTaskTransition::Stay,                                        // Deferred visit not resolved yet.
            Some(true)  => UnitTaskTransition::Goto(UnitTaskFetchState::ReturningToOrigin), // Collected a resource.
            Some(false) => UnitTaskTransition::Goto(UnitTaskFetchState::Searching),         // Nothing here; try elsewhere.
        }
    }

    fn update_returning(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if ctx.unit.goal().is_none() {
            if self.try_return_to_origin(ctx) {
                return UnitTaskTransition::Stay;
            }

            // Origin unreachable; try to route the cargo to any storage.
            return UnitTaskTransition::Goto(UnitTaskFetchState::RoutingSurplus);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // Reached the origin building.
        ctx.unit.follow_path(None);

        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            UnitTaskTransition::Goto(UnitTaskFetchState::DeliveringToOrigin)
        } else if !ctx.unit.inventory_is_empty() {
            // No callback / origin gone, but still holding cargo: route it as surplus.
            UnitTaskTransition::Goto(UnitTaskFetchState::RoutingSurplus)
        } else {
            UnitTaskTransition::Goto(UnitTaskFetchState::Done)
        }
    }

    fn update_delivering(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if !self.completion_callback_done {
            return UnitTaskTransition::Stay;
        }

        // The origin building may not have taken the whole delivery.
        if !ctx.unit.inventory_is_empty() {
            UnitTaskTransition::Goto(UnitTaskFetchState::RoutingSurplus)
        } else {
            UnitTaskTransition::Goto(UnitTaskFetchState::Done)
        }
    }

    fn update_routing_surplus(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if ctx.unit.goal().is_none() {
            if self.try_route_surplus_to_storage(ctx) {
                return UnitTaskTransition::Stay;
            }

            // Nothing will take the surplus (or the unit is empty-handed); finish.
            if !ctx.unit.inventory_is_empty() {
                log::error!(
                    log::channel!("task"),
                    "Aborting TaskFetchFromStorage. No storage will accept the surplus cargo."
                );
                ctx.unit.clear_inventory();
            }

            return UnitTaskTransition::Goto(UnitTaskFetchState::Done);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // Reached the recovery storage. Schedule the unload visit.
        let scheduled = self.try_visit_for_surplus(ctx);
        ctx.unit.follow_path(None);

        if scheduled {
            UnitTaskTransition::Goto(UnitTaskFetchState::UnloadingSurplus)
        } else {
            // Destination disappeared; re-route.
            UnitTaskTransition::Goto(UnitTaskFetchState::RoutingSurplus)
        }
    }

    fn update_unloading_surplus(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        match self.visit_outcome.take() {
            None        => UnitTaskTransition::Stay,                                     // Deferred visit not resolved yet.
            Some(true)  => UnitTaskTransition::Goto(UnitTaskFetchState::Done),           // Surplus deposited.
            Some(false) => UnitTaskTransition::Goto(UnitTaskFetchState::RoutingSurplus), // Refused; try another storage.
        }
    }

    fn update_done(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFetchState> {
        if !ctx.unit.inventory_is_empty() {
            log::error!(log::channel!("task"), "TaskFetchFromStorage: dropping undeliverable cargo.");
            ctx.unit.clear_inventory();
        }
        UnitTaskTransition::Done
    }
}

impl UnitTaskState for UnitTaskFetchState {
    type Task = UnitTaskFetchFromStorage;

    fn update(self, task: &mut UnitTaskFetchFromStorage, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::Searching          => task.update_searching(ctx),
            Self::MovingToStorage    => task.update_moving_to_storage(ctx),
            Self::VisitingStorage    => task.update_visiting_storage(ctx),
            Self::ReturningToOrigin  => task.update_returning(ctx),
            Self::DeliveringToOrigin => task.update_delivering(ctx),
            Self::RoutingSurplus     => task.update_routing_surplus(ctx),
            Self::UnloadingSurplus   => task.update_unloading_surplus(ctx),
            Self::Done               => task.update_done(ctx),
        }
    }
}

impl UnitTask for UnitTaskFetchFromStorage {
    type State = UnitTaskFetchState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert!(ctx.unit.inventory_is_empty());
        debug_assert_eq!(ctx.unit.cell(), self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(!self.resources_to_fetch.is_empty());
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

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _sim_context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building            : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("State                      : {:?}", self.state));
        ui.separator();
        ui.text(format!("Storage Buildings Accepted : {}", self.storage_buildings_accepted));
        ui.text(format!("Resources To Fetch         : {}", self.resources_to_fetch));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
    }
}
