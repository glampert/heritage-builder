use common::{callback::Callback, coords::Cell, time::CountdownTimer};
use serde::{Deserialize, Serialize};

use super::{
    Unit,
    UnitId,
    UnitTaskHelper,
    UnitSpawnState,
    SpawnedUnitWithTask,
    config::UnitConfigKey,
    task::{UnitTaskDespawn, UnitTaskHarvestCompletionCallback, UnitTaskHarvestState, UnitTaskHarvestWood},
};
use crate::{
    prop::PropId,
    building::BuildingContext,
    sim::commands::{SimCmds, SpawnPromise},
};

// ----------------------------------------------
// Harvester Unit helper
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Harvester {
    #[serde(flatten)] // Preserve backwards compatibility with old save files. Previously `unit_id: UnitId`.
    unit: SpawnedUnitWithTask,
}

impl UnitTaskHelper for Harvester {
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

impl Harvester {
    pub fn try_harvest_wood(
        &mut self,
        cmds: &mut SimCmds,
        context: &BuildingContext,
        unit_origin: Cell,
        completion_callback: Callback<UnitTaskHarvestCompletionCallback>,
    ) {
        self.try_spawn_with_task(
            context.debug_name(),
            cmds,
            context.sim_ctx,
            unit_origin,
            UnitConfigKey::Peasant,
            UnitTaskHarvestWood {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                completion_callback,
                completion_task: context.sim_ctx.task_manager_mut().new_task(UnitTaskDespawn),
                harvest_timer: CountdownTimer::default(),
                harvest_target: PropId::default(),
                is_returning_to_origin: false,
                internal_state: UnitTaskHarvestState::default(),
            },
        );
    }
}
