use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::{log, ui::UiSystem};

use super::{
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    UnitTaskResult,
    UnitTaskState,
    common::{
        PathFindResult,
        find_storage_fetch_candidate,
        visit_destination_deferred,
        invoke_completion_callback_immediate,
        invoke_completion_callback_deferred,
    },
};
use crate::{
    debug,
    pathfind::SearchResult,
    tile::TileMapLayerKind,
    unit::{Unit, navigation::UnitNavGoal},
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    sim::{SimCmds, SimContext, resources::ShoppingList},
};

// ----------------------------------------------
// UnitTaskFetchFromStorage
// ----------------------------------------------

pub type UnitTaskFetchCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskFetchState {
    #[default]
    Idle,
    MovingToGoal,
    PendingBuildingVisit,
    ReturningToOrigin,
    Completed,
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
    pub resources_to_fetch: ShoppingList, // Will fetch at most *one* of these. This is a list of desired options.

    // Called on the origin building once the unit has returned with resources.
    // `|context, origin_building, runner_unit|`
    pub completion_callback: Callback<UnitTaskFetchCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Current internal task state. Should start as Idle.
    // Deserialize uses Default if missing to retain backwards compatibility with older save files.
    #[serde(default)]
    pub internal_state: UnitTaskFetchState,
}

impl UnitTaskFetchFromStorage {
    fn try_find_goal(&mut self, unit: &mut Unit, context: &SimContext) -> bool {
        let origin_cell = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        for resource_to_fetch in self.resources_to_fetch.iter() {
            debug_assert!(resource_to_fetch.kind.is_single_resource());

            let path_find_result = find_storage_fetch_candidate(
                context,
                self.origin_building.kind,
                origin_cell,
                traversable_node_kinds,
                self.storage_buildings_accepted,
                resource_to_fetch.kind,
            );

            if let PathFindResult::Success { path, goal } = path_find_result {
                unit.move_to_goal(path, goal);
                self.internal_state = UnitTaskFetchState::MovingToGoal;
                return true;
            }
            // Else no path or Storage building found. Try again.
        }

        false
    }

    fn try_return_to_origin(&mut self, unit: &mut Unit, context: &SimContext) -> bool {
        if context.world().find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::error!(log::channel!("task"), "Origin building is no longer valid! TaskFetchFromStorage will abort.");
            return false;
        }

        let start = unit.cell();
        let goal = self.origin_building_tile.road_link;
        let traversable_node_kinds = unit.traversable_node_kinds();

        match context.find_path(traversable_node_kinds, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                unit.move_to_goal(path, goal);
                self.internal_state = UnitTaskFetchState::ReturningToOrigin;
                true
            }
            SearchResult::PathNotFound => {
                log::error!(
                    log::channel!("task"),
                    "Origin building is no longer reachable! (no road access?) TaskFetchFromStorage will abort."
                );
                false
            }
        }
    }
}

impl UnitTask for UnitTaskFetchFromStorage {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn initialize(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.inventory_is_empty());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(!self.resources_to_fetch.is_empty());
        debug_assert_eq!(self.internal_state, UnitTaskFetchState::Idle);

        self.try_find_goal(unit, context);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        // If we have a goal we're already moving somewhere,
        // otherwise we may need to pathfind again.
        match self.internal_state {
            UnitTaskFetchState::Idle | UnitTaskFetchState::MovingToGoal => {
                if unit.goal().is_none() {
                    if !self.try_find_goal(unit, context) {
                        // No storage buildings available. Return home.
                        self.internal_state = UnitTaskFetchState::ReturningToOrigin;
                        return UnitTaskState::Running;
                    }
                }
            }
            UnitTaskFetchState::ReturningToOrigin => {
                if unit.goal().is_none() {
                    if !self.try_return_to_origin(unit, context) {
                        // TODO: We can recover from this and ship the resources back to storage.
                        log::error!(
                            log::channel!("TODO"),
                            "Aborting TaskFetchFromStorage. Unable to return to origin building..."
                        );
                        unit.clear_inventory();

                        if self.completion_callback.is_valid() {
                            invoke_completion_callback_deferred(
                                unit,
                                cmds,
                                context,
                                self.origin_building.kind,
                                self.origin_building.id,
                                self.completion_callback.get(),
                            );
                        }

                        debug_assert_ne!(self.internal_state, UnitTaskFetchState::Completed);
                        self.internal_state = UnitTaskFetchState::Completed;

                        return UnitTaskState::Completed;
                    }
                }
            }
            UnitTaskFetchState::PendingBuildingVisit => {
                // Wait for building visited callback to be invoked.
                return UnitTaskState::Running;
            }
            UnitTaskFetchState::Completed => {
                // Task completed or aborted.
                return UnitTaskState::Completed;
            }
        }

        if unit.has_reached_goal() {
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        // NOTE: For backwards compatibility with old saves that didn't have internal_state.
        let goal_is_origin_building = |unit: &Unit| -> bool {
            unit.goal().is_some_and(|goal| {
                goal.is_building()
                    && goal.building_destination() == (self.origin_building.kind, self.origin_building_tile.base_cell)
            })
        };

        let task_completed = if self.internal_state == UnitTaskFetchState::ReturningToOrigin || goal_is_origin_building(unit) {
            // We've reached our origin building with the resources we were supposed to
            // fetch. Invoke the completion callback and end the task.
            if !unit.inventory_is_empty() {
                // If the unit inventory is not empty then we should have one of the items we were looking for.
                // If the inventory is empty it means we failed to find a storage building and are coming back
                // home empty handed.
                debug_assert!(self.resources_to_fetch.iter().any(|entry| entry.kind == unit.peek_inventory().unwrap().kind));
            }

            if self.completion_callback.is_valid() {
                invoke_completion_callback_deferred(
                    unit,
                    cmds,
                    context,
                    self.origin_building.kind,
                    self.origin_building.id,
                    self.completion_callback.get(),
                );
            }

            debug_assert_ne!(self.internal_state, UnitTaskFetchState::Completed);
            self.internal_state = UnitTaskFetchState::Completed;

            unit.follow_path(None);

            if !unit.inventory_is_empty() {
                // TODO: We can recover from this and ship the resources back to storage.
                log::error!(
                    log::channel!("TODO"),
                    "TaskFetchFromStorage: Failed to unload all resources. Src building destroyed?"
                );
                unit.clear_inventory();
            }

            true // Task completed.
        } else if self.internal_state == UnitTaskFetchState::MovingToGoal || (unit.goal().is_some() && !goal_is_origin_building(unit)) {
            // We've reached a destination to visit and attempt to fetch some resources.
            // We may fail and try again with another building or start returning to the origin.
            debug_assert!(unit.inventory_is_empty());

            let destination_exists = visit_destination_deferred(unit, cmds, context, |context, _building, unit, _result| {
                let task = unit.current_task_as_mut::<Self>(context.task_manager_mut())
                    .expect("Expected unit to be running UnitTaskFetchFromStorage!");

                debug_assert_eq!(task.internal_state, UnitTaskFetchState::PendingBuildingVisit);

                // If we've collected resources from the visited destination
                // we are done and can return to our origin building.
                if let Some(item) = unit.peek_inventory() {
                    debug_assert!(item.count != 0, "Expected nonzero item count: {item}");
                    debug_assert!(
                        task.resources_to_fetch.iter().any(|entry| entry.kind == item.kind),
                        "Expected to have item kind {}",
                        item.kind
                    );

                    // If we couldn't find a path back to the origin, maybe because the origin
                    // building was destroyed, we'll have to abort the task. Any
                    // resources collected will be lost.
                    if !task.try_return_to_origin(unit, context) {
                        // TODO: We can recover from this and ship the resources back to storage.
                        log::error!(
                            log::channel!("TODO"),
                            "Aborting TaskFetchFromStorage. Unable to return to origin building..."
                        );
                        unit.clear_inventory();

                        // NOTE: Invoke callback immediately; we are already inside a deferred callback.
                        if task.completion_callback.is_valid() {
                            invoke_completion_callback_immediate(
                                unit,
                                context,
                                task.origin_building.kind,
                                task.origin_building.id,
                                task.completion_callback.get(),
                            );
                        }

                        debug_assert_ne!(task.internal_state, UnitTaskFetchState::Completed);
                        task.internal_state = UnitTaskFetchState::Completed;
                    }

                    debug_assert!(matches!(
                        task.internal_state,
                        UnitTaskFetchState::ReturningToOrigin | UnitTaskFetchState::Completed
                    ));
                } else {
                    // Destination didn't have the resources we wanted. Try again elsewhere.
                    task.internal_state = UnitTaskFetchState::Idle;
                }
            });

            if destination_exists {
                // Building visitation is deferred, so we must wait for it to complete.
                self.internal_state = UnitTaskFetchState::PendingBuildingVisit;
            } else {
                // Destination building no longer valid (might have been destroyed).
                self.internal_state = UnitTaskFetchState::Idle;
            }

            unit.follow_path(None);

            false // Task pending.
        } else {
            if self.internal_state != UnitTaskFetchState::Completed {
                log::error!(
                    log::channel!("task"),
                    "Unexpected UnitTaskFetchFromStorage state. Expected Completed, found: {:?}",
                    self.internal_state,
                );
            }

            true // Task completed.
        };

        if task_completed {
            UnitTaskResult::completed_with(&mut self.completion_task)
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building            : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("Internal State             : {:?}", self.internal_state));
        ui.separator();
        ui.text(format!("Storage Buildings Accepted : {}", self.storage_buildings_accepted));
        ui.text(format!("Resources To Fetch         : {}", self.resources_to_fetch));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
    }
}
