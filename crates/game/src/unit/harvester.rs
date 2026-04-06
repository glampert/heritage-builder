use common::{callback::Callback, coords::Cell, time::CountdownTimer};
use serde::{Deserialize, Serialize};

use super::{
    UnitId,
    UnitTaskHelper,
    config::UnitConfigKey,
    task::{UnitTaskDespawn, UnitTaskHarvestCompletionCallback, UnitTaskHarvestWood},
};
use crate::{building::BuildingContext, prop::PropId};

// ----------------------------------------------
// Harvester Unit helper
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Harvester {
    unit_id: UnitId,
    #[serde(skip)]
    failed_to_spawn: bool, // Debug flag; not serialized.
}

impl UnitTaskHelper for Harvester {
    #[inline]
    fn reset(&mut self) {
        self.unit_id = UnitId::default();
        self.failed_to_spawn = false;
    }

    #[inline]
    fn on_unit_spawn(&mut self, unit_id: UnitId, failed_to_spawn: bool) {
        self.unit_id = unit_id;
        self.failed_to_spawn = failed_to_spawn;
    }

    #[inline]
    fn unit_id(&self) -> UnitId {
        self.unit_id
    }

    #[inline]
    fn failed_to_spawn(&self) -> bool {
        self.failed_to_spawn
    }
}

impl Harvester {
    pub fn try_harvest_wood(
        &mut self,
        context: &BuildingContext,
        unit_origin: Cell,
        completion_callback: Callback<UnitTaskHarvestCompletionCallback>,
    ) -> bool {
        self.try_spawn_with_task(
            context.debug_name(),
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
            },
        )
    }
}
