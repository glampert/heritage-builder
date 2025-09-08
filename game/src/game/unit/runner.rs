use crate::{
    utils::coords::Cell,
    game::{
        building::{
            Building,
            BuildingKind,
            BuildingContext
        },
        sim::{
            Query,
            resources::{ShoppingList, ResourceKind}
        }
    }
};

use super::{
    Unit,
    UnitId,
    UnitTaskHelper,
    config::{self},
    task::{
        UnitTaskDespawn,
        UnitTaskDeliverToStorage,
        UnitTaskFetchFromStorage
    }
};

// ----------------------------------------------
// Runner Unit helper
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct Runner {
    unit_id: UnitId,
    failed_to_spawn: bool,
}

impl UnitTaskHelper for Runner {
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

impl Runner {
    pub fn try_deliver_to_storage(&mut self,
                                  context: &BuildingContext,
                                  unit_origin: Cell,
                                  storage_buildings_accepted: BuildingKind,
                                  resource_kind_to_deliver: ResourceKind,
                                  resource_count: u32,
                                  completion_callback: Option<fn(&mut Building, &mut Unit, &Query)>) -> bool {
        self.try_spawn_with_task(
            context.debug_name(),
            context.query,
            unit_origin,
            config::UNIT_RUNNER,
            UnitTaskDeliverToStorage {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                storage_buildings_accepted,
                resource_kind_to_deliver,
                resource_count,
                completion_callback,
                completion_task: context.query.task_manager().new_task(UnitTaskDespawn),
                allow_producer_fallback: true, // If we can't find a Storage that will take our goods,
                                               // try delivering directly to other Producers.
            }
        )
    }

    pub fn try_fetch_from_storage(&mut self,
                                  context: &BuildingContext,
                                  unit_origin: Cell,
                                  storage_buildings_accepted: BuildingKind,
                                  resources_to_fetch: ShoppingList, // Will fetch at most *one* of these. This is a list of desired options.
                                  completion_callback: Option<fn(&mut Building, &mut Unit, &Query)>) -> bool {
        self.try_spawn_with_task(
            context.debug_name(),
            context.query,
            unit_origin,
            config::UNIT_RUNNER,
            UnitTaskFetchFromStorage {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                storage_buildings_accepted,
                resources_to_fetch,
                completion_callback,
                completion_task: context.query.task_manager().new_task(UnitTaskDespawn),
                is_returning_to_origin: false,
            }
        )
    }
}
