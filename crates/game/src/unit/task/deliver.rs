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
        find_delivery_candidate,
        invoke_completion_callback_immediate,
        visit_destination_deferred,
    },
};
use crate::{
    debug,
    tile::TileMapLayerKind,
    sim::{SimCmds, SimContext, resources::ResourceKind},
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    unit::Unit,
};

// ----------------------------------------------
// UnitTaskDeliverToStorage
// ----------------------------------------------

pub type UnitTaskDeliveryCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskDeliveryState {
    #[default]
    Idle,
    MovingToGoal,
    PendingBuildingVisit,
    Completed,
}

// Deliver goods to a storage building.
// Producer -> Storage | Storage -> Storage | Producer -> Producer (fallback)
#[derive(Serialize, Deserialize)]
pub struct UnitTaskDeliverToStorage {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Resources to deliver:
    pub storage_buildings_accepted: BuildingKind,
    pub resource_kind_to_deliver: ResourceKind,
    pub resource_count: u32,

    // Called on the origin building once resources are delivered.
    // `|context, origin_building, runner_unit|`
    pub completion_callback: Callback<UnitTaskDeliveryCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Optional fallback if we are not able to deliver to a Storage.
    // E.g.: Deliver directly to a Producer building instead.
    pub allow_producer_fallback: bool,

    // Current internal task state. Should start as Idle.
    // Deserialize uses Default if missing to retain backwards compatibility with older save files.
    #[serde(default)]
    pub internal_state: UnitTaskDeliveryState,
}

impl UnitTaskDeliverToStorage {
    fn try_find_goal(&mut self, unit: &mut Unit, context: &SimContext) {
        let origin_kind = self.origin_building.kind;
        let origin_base_cell = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        // Prefer delivering to a storage building.
        let mut path_find_result = find_delivery_candidate(
            context,
            origin_kind,
            origin_base_cell,
            traversable_node_kinds,
            self.storage_buildings_accepted,
            self.resource_kind_to_deliver,
        );

        if path_find_result.not_found() && self.allow_producer_fallback {
            // Find any producer that can take our resources as fallback.
            path_find_result = find_delivery_candidate(
                context,
                origin_kind,
                origin_base_cell,
                traversable_node_kinds,
                BuildingKind::producers(),
                self.resource_kind_to_deliver,
            );
        }

        if let PathFindResult::Success { path, goal } = path_find_result {
            unit.move_to_goal(path, goal);
            self.internal_state = UnitTaskDeliveryState::MovingToGoal;
        }
        // Else no path or Storage/Producer building found. Try again later.
    }
}

impl UnitTask for UnitTaskDeliverToStorage {
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
        debug_assert!(self.resource_kind_to_deliver.is_single_resource());
        debug_assert!(self.resource_count != 0);
        debug_assert_eq!(self.internal_state, UnitTaskDeliveryState::Idle);

        // Give the unit the resources we want to deliver:
        let received_count = unit.receive_resources(self.resource_kind_to_deliver, self.resource_count);
        debug_assert!(received_count == self.resource_count);

        self.try_find_goal(unit, context);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        match self.internal_state {
            UnitTaskDeliveryState::Idle | UnitTaskDeliveryState::MovingToGoal => {
                // If we have a goal we're already moving somewhere,
                // otherwise we may need to pathfind again.
                if unit.goal().is_none() {
                    self.try_find_goal(unit, context);
                }
            }
            UnitTaskDeliveryState::PendingBuildingVisit => {
                // Wait for building visited callback to be invoked.
                return UnitTaskState::Running;
            }
            UnitTaskDeliveryState::Completed => {
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
        if self.internal_state == UnitTaskDeliveryState::MovingToGoal || unit.goal().is_some() {
            let destination_exists = visit_destination_deferred(unit, cmds, context, |context, _building, unit, _result| {
                let task = unit.current_task_as_mut::<Self>(context.task_manager_mut())
                    .expect("Expected unit to be running UnitTaskDeliverToStorage!");

                debug_assert_eq!(task.internal_state, UnitTaskDeliveryState::PendingBuildingVisit);

                // If we've delivered our goods, we're done. Otherwise we were not able
                // to offload everything, so we'll retry with another building later.
                if unit.inventory_is_empty() {
                    if task.completion_callback.is_valid() {
                        // NOTE: Invoke callback immediately; we are already inside a deferred callback.
                        invoke_completion_callback_immediate(
                            unit,
                            context,
                            task.origin_building.kind,
                            task.origin_building.id,
                            task.completion_callback.get(),
                        );
                    }

                    task.internal_state = UnitTaskDeliveryState::Completed;
                } else {
                    task.internal_state = UnitTaskDeliveryState::Idle;
                }
            });

            if destination_exists {
                // Building visitation is deferred, so we must wait for it to complete.
                self.internal_state = UnitTaskDeliveryState::PendingBuildingVisit;
            } else {
                // Destination building no longer valid (might have been destroyed).
                self.internal_state = UnitTaskDeliveryState::Idle;
            }

            // Wait for PendingBuildingVisit.
            unit.follow_path(None);
            UnitTaskResult::Retry
        } else {
            if self.internal_state != UnitTaskDeliveryState::Completed {
                log::error!(
                    log::channel!("task"),
                    "Unexpected UnitTaskDeliverToStorage state. Expected Completed, found: {:?}",
                    self.internal_state,
                );
            }

            UnitTaskResult::completed_with(&mut self.completion_task)
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
        ui.text(format!("Resource Kind To Deliver   : {}", self.resource_kind_to_deliver));
        ui.text(format!("Resource Count             : {}", self.resource_count));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
        ui.text(format!("Allow Producer Fallback    : {}", self.allow_producer_fallback));
    }
}
