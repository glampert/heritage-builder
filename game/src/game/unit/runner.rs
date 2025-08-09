use crate::{
    utils::coords::Cell,
    game::{
        building::{
            BuildingKind,
            BuildingContext
        },
        sim::{
            Query,
            world::UnitId,
            resources::{ShoppingList, ResourceKind}
        }
    }
};

use super::{
    Unit,
    config::{self},
    task::{
        UnitTask,
        UnitTaskArchetype,
        UnitTaskDespawn,
        UnitTaskDeliverToStorage,
        UnitTaskFetchFromStorage,
        UnitTaskCompletionCallback
    }
};

// ----------------------------------------------
// Runner Unit helper
// ----------------------------------------------

#[derive(Default)]
pub struct Runner {
    unit_id: UnitId,
    failed_to_spawn: bool,
}

impl Runner {
    #[inline]
    pub fn reset(&mut self) {
        self.unit_id = UnitId::default();
        self.failed_to_spawn = false;
    }

    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.unit_id.is_valid()
    }

    #[inline]
    pub fn failed_to_spawn(&self) -> bool {
        self.failed_to_spawn
    }

    #[inline]
    pub fn unit_id(&self) -> UnitId {
        self.unit_id
    }

    #[inline]
    pub fn try_unit<'config>(&self, query: &'config Query) -> Option<&'config Unit<'config>> {
        if self.unit_id.is_valid() {
            query.world().find_unit(self.unit_id)
        } else {
            None
        }
    }

    #[inline]
    pub fn try_unit_mut<'config>(&mut self, query: &'config Query) -> Option<&'config mut Unit<'config>> {
        if self.unit_id.is_valid() {
            query.world().find_unit_mut(self.unit_id)
        } else {
            None
        }
    }

    #[inline]
    pub fn unit<'config>(&self, query: &'config Query) -> &'config Unit<'config> {
        self.try_unit(query).unwrap()
    }

    #[inline]
    pub fn unit_mut<'config>(&mut self, query: &'config Query) -> &'config mut Unit<'config> {
        self.try_unit_mut(query).unwrap()
    }

    #[inline]
    pub fn is_running_task<Task>(&self, query: &Query) -> bool
        where
            Task: UnitTask + 'static
    {
        self.try_unit(query).is_some_and(|unit| {
            unit.is_running_task::<Task>(query.task_manager())
        })
    }

    #[inline]
    pub fn try_spawn_with_task<Task>(&mut self,
                                     spawner_name: &str,
                                     query: &Query,
                                     unit_origin: Cell,
                                     task: Task) -> bool
        where
            Task: UnitTask,
            UnitTaskArchetype: From<Task>
    {
        debug_assert!(!self.is_spawned(), "Runner Unit already spawned! Call reset() first.");

        match Unit::try_spawn_with_task(query, unit_origin, config::UNIT_RUNNER, task) {
            Ok(unit) => {
                self.unit_id = unit.id();
                true
            },
            Err(err) => {
                eprintln!("{}: Failed to spawn Runner Unit at cell {}: {}", spawner_name, unit_origin, err);
                self.failed_to_spawn = true;
                false
            },
        }
    }

    #[inline]
    pub fn try_deliver_to_storage(&mut self,
                                  context: &BuildingContext,
                                  unit_origin: Cell,
                                  storage_buildings_accepted: BuildingKind,
                                  resource_kind_to_deliver: ResourceKind,
                                  resource_count: u32,
                                  completion_callback: Option<UnitTaskCompletionCallback>) -> bool {
        self.try_spawn_with_task(
            context.debug_name(),
            context.query,
            unit_origin,
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

    #[inline]
    pub fn try_fetch_from_storage(&mut self,
                                  context: &BuildingContext,
                                  unit_origin: Cell,
                                  storage_buildings_accepted: BuildingKind,
                                  resources_to_fetch: ShoppingList, // Will fetch at most *one* of these. This is a list of desired options.
                                  completion_callback: Option<UnitTaskCompletionCallback>) -> bool {
        self.try_spawn_with_task(
            context.debug_name(),
            context.query,
            unit_origin,
            UnitTaskFetchFromStorage {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                storage_buildings_accepted,
                resources_to_fetch,
                completion_callback,
                completion_task: context.query.task_manager().new_task(UnitTaskDespawn),
            }
        )
    }
}
