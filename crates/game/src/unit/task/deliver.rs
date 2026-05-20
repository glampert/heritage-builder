use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::ui::UiSystem;

use super::{
    PathFindResult,
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    find_delivery_candidate,
    invoke_completion_callback_immediate,
    visit_destination_deferred,
    with_task,
};
use crate::{
    debug,
    tile::TileMapLayerKind,
    sim::{SimContext, resources::ResourceKind},
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    unit::Unit,
};

// ----------------------------------------------
// UnitTaskDeliverToStorage
// ----------------------------------------------

pub type UnitTaskDeliveryCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskDeliveryState {
    // Looking for a storage (or fallback producer) that can take the goods.
    #[default]
    Searching,

    // Walking to the chosen building.
    MovingToStorage,

    // At the building, waiting for the deferred visit to resolve.
    VisitingStorage,

    // Goods delivered; terminal state.
    Done,
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

    pub state: UnitTaskDeliveryState,

    // Outcome of the deferred storage visit, written by the visit callback and
    // consumed by `VisitingStorage`: `Some(true)` delivered, `Some(false)` refused.
    #[serde(skip)]
    pub visit_delivered: Option<bool>,
}

impl UnitTaskDeliverToStorage {
    fn try_find_goal(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let origin_kind = self.origin_building.kind;
        let origin_base_cell = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        // Prefer delivering to a storage building.
        let mut path_find_result = find_delivery_candidate(
            ctx.sim_context,
            origin_kind,
            origin_base_cell,
            traversable_node_kinds,
            self.storage_buildings_accepted,
            self.resource_kind_to_deliver,
        );

        if path_find_result.not_found() && self.allow_producer_fallback {
            // Find any producer that can take our resources as fallback.
            path_find_result = find_delivery_candidate(
                ctx.sim_context,
                origin_kind,
                origin_base_cell,
                traversable_node_kinds,
                BuildingKind::producers(),
                self.resource_kind_to_deliver,
            );
        }

        if let PathFindResult::Success { path, goal } = path_find_result {
            ctx.unit.move_to_goal(path, goal);
            true
        } else {
            // No path or Storage/Producer building found. Try again later.
            false
        }
    }

    // Schedules the deferred building visit.
    // Returns false if the destination building is no longer valid.
    fn try_visit_storage(&mut self, ctx: &mut UnitTaskContext) -> bool {
        visit_destination_deferred(ctx.unit, ctx.sim_cmds, ctx.sim_context, |context, _building, unit, _result| {
            with_task::<Self>(unit, context, |task, unit| {
                if unit.inventory_is_empty() {
                    // Delivered everything.
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
                    task.visit_delivered = Some(true);
                } else {
                    // Couldn't offload everything; retry with another building.
                    task.visit_delivered = Some(false);
                }
            });
        })
    }

    fn update_searching(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskDeliveryState> {
        if self.try_find_goal(ctx) {
            UnitTaskTransition::Goto(UnitTaskDeliveryState::MovingToStorage)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_moving_to_storage(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskDeliveryState> {
        if ctx.unit.goal().is_none() {
            // Lost the path to the storage; search again.
            return UnitTaskTransition::Goto(UnitTaskDeliveryState::Searching);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // Reached the storage building. Schedule the deferred visit.
        let scheduled = self.try_visit_storage(ctx);
        ctx.unit.follow_path(None);

        if scheduled {
            UnitTaskTransition::Goto(UnitTaskDeliveryState::VisitingStorage)
        } else {
            // Destination building no longer valid (might have been destroyed).
            UnitTaskTransition::Goto(UnitTaskDeliveryState::Searching)
        }
    }

    fn update_visiting_storage(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskDeliveryState> {
        match self.visit_delivered.take() {
            None        => UnitTaskTransition::Stay,                                   // Deferred visit not resolved yet.
            Some(true)  => UnitTaskTransition::Goto(UnitTaskDeliveryState::Done),      // Delivered.
            Some(false) => UnitTaskTransition::Goto(UnitTaskDeliveryState::Searching), // Refused; try elsewhere.
        }
    }
}

impl UnitTaskState for UnitTaskDeliveryState {
    type Task = UnitTaskDeliverToStorage;

    fn update(self, task: &mut UnitTaskDeliverToStorage, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::Searching       => task.update_searching(ctx),
            Self::MovingToStorage => task.update_moving_to_storage(ctx),
            Self::VisitingStorage => task.update_visiting_storage(ctx),
            Self::Done            => UnitTaskTransition::Done,
        }
    }
}

impl UnitTask for UnitTaskDeliverToStorage {
    type State = UnitTaskDeliveryState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert!(ctx.unit.inventory_is_empty());
        debug_assert!(ctx.unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(self.resource_kind_to_deliver.is_single_resource());
        debug_assert_ne!(self.resource_count, 0);

        // Give the unit the resources we want to deliver:
        let received_count = ctx.unit.receive_resources(self.resource_kind_to_deliver, self.resource_count);
        debug_assert_eq!(received_count, self.resource_count);
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
        ui.text(format!("Resource Kind To Deliver   : {}", self.resource_kind_to_deliver));
        ui.text(format!("Resource Count             : {}", self.resource_count));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
        ui.text(format!("Allow Producer Fallback    : {}", self.allow_producer_fallback));
    }
}
