#![allow(clippy::too_many_arguments)]

use common::{callback::Callback, coords::Cell};
use serde::{Deserialize, Serialize};

use super::{
    Unit,
    UnitId,
    UnitTaskHelper,
    UnitSpawnState,
    SpawnedUnitWithTask,
    config::UnitConfigKey,
    task::{
        UnitTaskDespawn,
        UnitTaskDeliverToStorage,
        UnitTaskDeliveryCompletionCallback,
        UnitTaskDeliveryState,
        UnitTaskFetchFromStorage,
        UnitTaskFetchCompletionCallback,
        UnitTaskFetchState,
    },
};
use crate::{
    building::{BuildingContext, BuildingKind},
    sim::{resources::{ResourceKind, ShoppingList}, commands::{SimCmds, SpawnPromise}},
};

// ----------------------------------------------
// Runner Unit helper
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Runner {
    #[serde(flatten)] // Preserve backwards compatibility with old save files. Previously `unit_id: UnitId`.
    unit: SpawnedUnitWithTask,
}

impl UnitTaskHelper for Runner {
    #[inline]
    fn reset(&mut self) {
        self.unit.reset();
    }

    #[inline]
    fn get_pending_promise(&mut self) -> Option<SpawnPromise<Unit>> {
        self.unit.get_pending_promise()
    }

    #[inline]
    fn set_spawn_state(&mut self, state: UnitSpawnState) {
        self.unit.set_spawn_state(state);
    }

    #[inline]
    fn spawn_state(&self) -> &UnitSpawnState {
        self.unit.spawn_state()
    }

    #[inline]
    fn unit_id(&self) -> UnitId {
        self.unit.unit_id()
    }
}

impl Runner {
    pub fn try_deliver_to_storage(
        &mut self,
        cmds: &mut SimCmds,
        context: &BuildingContext,
        unit_origin: Cell,
        storage_buildings_accepted: BuildingKind,
        resource_kind_to_deliver: ResourceKind,
        resource_count: u32,
        completion_callback: Callback<UnitTaskDeliveryCompletionCallback>,
    ) {
        self.try_spawn_with_task(
            context.debug_name(),
            cmds,
            context.sim_ctx,
            unit_origin,
            UnitConfigKey::Runner,
            UnitTaskDeliverToStorage {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                storage_buildings_accepted,
                resource_kind_to_deliver,
                resource_count,
                completion_callback,
                completion_task: context.sim_ctx.task_manager_mut().new_task(UnitTaskDespawn),
                // If we can't find a Storage that will take our goods, try delivering directly to other Producers.
                allow_producer_fallback: true,
                internal_state: UnitTaskDeliveryState::default(),
            },
        );
    }

    pub fn try_fetch_from_storage(
        &mut self,
        cmds: &mut SimCmds,
        context: &BuildingContext,
        unit_origin: Cell,
        storage_buildings_accepted: BuildingKind,
        resources_to_fetch: ShoppingList, // Will fetch at most *one* of these. This is a list of desired options.
        completion_callback: Callback<UnitTaskFetchCompletionCallback>,
    ) {
        self.try_spawn_with_task(
            context.debug_name(),
            cmds,
            context.sim_ctx,
            unit_origin,
            UnitConfigKey::Runner,
            UnitTaskFetchFromStorage {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                storage_buildings_accepted,
                resources_to_fetch,
                completion_callback,
                completion_task: context.sim_ctx.task_manager_mut().new_task(UnitTaskDespawn),
                internal_state: UnitTaskFetchState::default(),
            },
        );
    }
}
