#![allow(clippy::while_let_on_iterator)]

use core::iter;
use core::slice;
use bitvec::vec::BitVec;
use strum::IntoDiscriminant;

use crate::{
    imgui_ui::UiSystem,
    utils::coords::{
        Cell,
        CellRange,
        WorldToScreenTransform
    },
    tile::{
        Tile,
        TileKind,
        TileMap,
        TileMapLayerKind,
        TileGameStateHandle,
        sets::{TileDef, OBJECTS_UNITS_CATEGORY}
    },
    game::{
        constants::*,
        building::{
            self,
            HouseLevel,
            Building,
            BuildingKind,
            BuildingArchetypeKind,
            config::BuildingConfigs,
            BUILDING_ARCHETYPE_COUNT
        },
        unit::{
            Unit,
            config::{UnitConfigs, UnitConfigKey}
        }
    }
};

use super::{
    Query,
    resources::{ResourceKind, ResourceStock}
};

// ----------------------------------------------
// World
// ----------------------------------------------

// Holds the world state and provides queries.
pub struct World<'config> {
    stats: WorldStats,

    // One spawn pool per building archetype.
    // Iteration yields only *spawned* buildings.
    building_spawn_pools: [(BuildingArchetypeKind, SpawnPool<Building<'config>>); BUILDING_ARCHETYPE_COUNT],
    building_configs: &'config BuildingConfigs,

    // All units, spawned and despawned.
    // Iteration yields only *spawned* units.
    unit_spawn_pool: SpawnPool<Unit<'config>>,
    unit_configs: &'config UnitConfigs,
}

impl<'config> World<'config> {
    pub fn new(building_configs: &'config BuildingConfigs, unit_configs: &'config UnitConfigs) -> Self {
        Self {
            stats: WorldStats::new(),
            // Buildings:
            building_spawn_pools: [
                (BuildingArchetypeKind::ProducerBuilding, SpawnPool::new(PRODUCER_BUILDINGS_POOL_CAPACITY)),
                (BuildingArchetypeKind::StorageBuilding,  SpawnPool::new(STORAGE_BUILDINGS_POOL_CAPACITY)),
                (BuildingArchetypeKind::ServiceBuilding,  SpawnPool::new(SERVICE_BUILDINGS_POOL_CAPACITY)),
                (BuildingArchetypeKind::HouseBuilding,    SpawnPool::new(HOUSE_BUILDINGS_POOL_CAPACITY)),
            ],
            building_configs,
            // Units:
            unit_spawn_pool: SpawnPool::new(UNIT_SPAWN_POOL_CAPACITY),
            unit_configs,
        }
    }

    pub fn building_configs(&self) -> &'config BuildingConfigs {
        self.building_configs
    }

    pub fn unit_configs(&self) -> &'config UnitConfigs {
        self.unit_configs
    }

    pub fn reset(&mut self, query: &Query) {
        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            buildings.clear(query,
                |building, query| {
                    debug_assert!(building.archetype_kind() == *archetype_kind);
                    building.despawned(query)
                });
        }

        self.unit_spawn_pool.clear(query,
            |unit, query| {
                unit.despawned(query);
            });
    }

    pub fn update_unit_navigation(&mut self, query: &Query) {
        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update_navigation(query);
        } 
    }

    pub fn update(&mut self, query: &Query<'config, '_>) {
        self.stats.reset();

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update(query);
            unit.tally(&mut self.stats);
        }

        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == *archetype_kind);
                building.update(query);
                building.tally(&mut self.stats);
            }
        }
    }

    // ----------------------
    // Buildings API:
    // ----------------------

    pub fn try_spawn_building_with_tile_def(&mut self,
                                            query: &Query,
                                            tile_base_cell: Cell,
                                            tile_def: &TileDef) -> Result<&mut Building<'config>, String> {

        debug_assert!(tile_base_cell.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Building));

        // Allocate & place a Tile:
        match query.tile_map().try_place_tile(tile_base_cell, tile_def) {
            Ok(tile) => {
                // Instantiate new Building:
                match building::config::instantiate(tile, self.building_configs) {
                    Ok((building_kind, building_archetype)) => {
                        let archetype_kind = building_archetype.discriminant();
                        let buildings = self.buildings_pool_mut(archetype_kind);

                        let building = buildings.spawn(query,
                            |building, query, id| {
                                building.spawned(query, id, building_kind, tile.cell_range(), building_archetype);
                            });
                        debug_assert!(building.is_spawned());

                        // Store building index and kind so we can refer back to it from the Tile instance.
                        tile.set_game_state_handle(
                            TileGameStateHandle::new_building(
                                building.id().index(),
                                building_kind.bits()
                            ));

                        Ok(building)
                    },
                    Err(err) => {
                        Err(format!("Failed to instantiate Building at cell {} with TileDef '{}': {err}",
                                    tile_base_cell, tile_def.name))
                    }
                }
            },
            Err(err) => {
                Err(format!("Failed to place Building tile at cell {} with TileDef '{}': {}",
                            tile_base_cell, tile_def.name, err))
            }
        }
    }

    pub fn despawn_building(&mut self, query: &Query, building: &mut Building<'config>) -> Result<(), String> {
        let tile_base_cell = building.base_cell();
        debug_assert!(tile_base_cell.is_valid());

        let tile_map = query.tile_map();

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Building)
            .ok_or("Building should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Building tile '{}' {} should have a valid game state!", tile.name(), tile_base_cell));
        }

        // Remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        let pool_index = game_state.index();
        let building_kind = BuildingKind::from_game_state_handle(game_state);
        let archetype_kind = building_kind.archetype_kind();
        let buildings = self.buildings_pool_mut(archetype_kind);

        debug_assert!(pool_index == building.id().index());

        // Put the building instance back into the spawn pool.
        buildings.despawn(building, query,
            |building, query| {
                building.despawned(query);
            });

        Ok(())
    }

    #[inline]
    pub fn despawn_building_at_cell(&mut self, query: &Query<'config, '_>, tile_base_cell: Cell) -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        let building =
            query.world().find_building_for_cell_mut(tile_base_cell, query.tile_map())
                .expect("Tile cell does not contain a Building!");

        self.despawn_building(query, building)
    }

    #[inline]
    pub fn find_building(&self, kind: BuildingKind, id: BuildingId) -> Option<&Building<'config>> {
        let buildings = self.buildings_pool(kind.archetype_kind());
        buildings.try_get(id)
    }

    #[inline]
    pub fn find_building_mut(&mut self, kind: BuildingKind, id: BuildingId) -> Option<&mut Building<'config>> {
        let buildings = self.buildings_pool_mut(kind.archetype_kind());
        buildings.try_get_mut(id)
    }

    #[inline]
    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let pool_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_pool(archetype_kind);
            return buildings.try_get_at(pool_index); // NOTE: Does not perform generation check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let pool_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_pool_mut(archetype_kind);
            return buildings.try_get_at_mut(pool_index); // NOTE: Does not perform generation check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Building<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Building | TileKind::Blocker) {
            return self.find_building_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell_mut(&mut self, cell: Cell, tile_map: &TileMap) -> Option<&mut Building<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Building | TileKind::Blocker) {
            return self.find_building_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_by_name(&self, name: &str, kind: BuildingKind) -> Option<&Building<'config>> {
        self.buildings_pool(kind.archetype_kind())
            .iter()
            .find(|building| building.name() == name && building.is(kind))
    }

    #[inline]
    pub fn find_building_by_name_mut(&mut self, name: &str, kind: BuildingKind) -> Option<&mut Building<'config>> {
        self.buildings_pool_mut(kind.archetype_kind())
            .iter_mut()
            .find(|building| building.name() == name && building.is(kind))
    }

    // Iterates *all* buildings of a kind in the world, in unspecified order.
    // Visitor function should return true to continue iterating or false to stop.
    // `building_kinds` can be a combination of ORed BuildingKind flags.
    #[inline]
    pub fn for_each_building<F>(&self, building_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&Building<'config>) -> bool
    {
        let buildings = self.buildings_pool(building_kinds.archetype_kind());
        for building in buildings.iter() {
            if building.is(building_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_building_mut<F>(&mut self, building_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&mut Building<'config>) -> bool
    {
        let buildings = self.buildings_pool_mut(building_kinds.archetype_kind());
        for building in buildings.iter_mut() {
            if building.is(building_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    #[inline]
    fn buildings_pool(&self, archetype_kind: BuildingArchetypeKind) -> &SpawnPool<Building<'config>> {
        let (pool_archetype, buildings) = &self.building_spawn_pools[archetype_kind as usize];
        debug_assert!(archetype_kind == *pool_archetype);
        buildings
    }

    #[inline]
    fn buildings_pool_mut(&mut self, archetype_kind: BuildingArchetypeKind) -> &mut SpawnPool<Building<'config>> {
        let (pool_archetype, buildings) = &mut self.building_spawn_pools[archetype_kind as usize];
        debug_assert!(archetype_kind == *pool_archetype);
        buildings
    }

    // ----------------------
    // Buildings debug:
    // ----------------------

    pub fn draw_building_debug_popups(&mut self,
                                      query: &Query,
                                      ui_sys: &UiSystem,
                                      transform: &WorldToScreenTransform,
                                      visible_range: CellRange) {

        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == *archetype_kind);
                building.draw_debug_popups(
                    query,
                    ui_sys,
                    transform,
                    visible_range);
            };
        }
    }

    pub fn draw_building_debug_ui(&mut self,
                                  query: &Query<'config, '_>,
                                  ui_sys: &UiSystem,
                                  selected_cell: Cell) {

        let tile = match query.tile_map.find_tile(selected_cell,
                                                             TileMapLayerKind::Objects,
                                                             TileKind::Building) {
            Some(tile) => tile,
            None => return,
        };

        if let Some(building) = self.find_building_for_tile_mut(tile) {
            building.draw_debug_ui(query, ui_sys);
        }
    }

    // ----------------------
    // Units API:
    // ----------------------

    pub fn try_spawn_unit_with_config(&mut self,
                                      query: &Query,
                                      unit_origin: Cell,
                                      unit_config_key: UnitConfigKey) -> Result<&mut Unit<'config>, String> {

        debug_assert!(unit_origin.is_valid());
        debug_assert!(unit_config_key.is_valid());

        let config = self.unit_configs.find_config_by_hash(unit_config_key.hash);

        // Find TileDef:
        if let Some(tile_def) = query.tile_sets().find_tile_def_by_hash(
            TileMapLayerKind::Objects,
            OBJECTS_UNITS_CATEGORY.hash,
            config.tile_def_name_hash) {
            // Allocate & place a Tile:
            match query.tile_map().try_place_tile(unit_origin, tile_def) {
                Ok(tile) => {
                    // Spawn unit:
                    let unit = self.unit_spawn_pool.spawn(query,
                        |unit, _query, id| {
                            unit.spawned(tile, config, id);
                        });
                    debug_assert!(unit.is_spawned());

                    // Store unit index so we can refer back to it from the Tile instance.
                    tile.set_game_state_handle(
                        TileGameStateHandle::new_unit(
                            unit.id().index(),
                            unit.id().generation()
                        ));

                    Ok(unit)
                },
                Err(err) => {
                    Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                                unit_origin, tile_def.name, err))
                }
            }
        } else {
            Err(format!("Failed to spawn Unit at cell {} with config '{}': Cannot find TileDef '{}'!",
                        unit_origin, unit_config_key.string, config.tile_def_name))
        }
    }

    pub fn try_spawn_unit_with_tile_def(&mut self,
                                        query: &Query,
                                        unit_origin: Cell,
                                        tile_def: &TileDef) -> Result<&mut Unit<'config>, String> {

        debug_assert!(unit_origin.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Unit));

        // Allocate & place a Tile:
        match query.tile_map().try_place_tile(unit_origin, tile_def) {
            Ok(tile) => {
                let config = self.unit_configs.find_config_by_hash(tile_def.hash);

                // Spawn unit:
                let unit = self.unit_spawn_pool.spawn(query,
                    |unit, _query, id| {
                        unit.spawned(tile, config, id);
                    });
                debug_assert!(unit.is_spawned());

                // Store unit index so we can refer back to it from the Tile instance.
                tile.set_game_state_handle(
                    TileGameStateHandle::new_unit(
                        unit.id().index(),
                        unit.id().generation()
                    ));

                Ok(unit)
            },
            Err(err) => {
                Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                            unit_origin, tile_def.name, err))
            }
        }
    }

    pub fn despawn_unit(&mut self, query: &Query, unit: &mut Unit<'config>) -> Result<(), String> {
        debug_assert!(unit.is_spawned());
        let tile_map = query.tile_map();

        let tile_cell = unit.cell();
        debug_assert!(tile_cell.is_valid());

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .ok_or("Unit should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Unit tile '{}' {} should have a valid game state!", tile.name(), tile_cell));
        }

        debug_assert!(game_state.index() == unit.id().index());
        debug_assert!(game_state.generation() == unit.id().generation());

        // First remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_cell, TileMapLayerKind::Objects)?;

        // Put the unit instance back into the spawn pool.
        self.unit_spawn_pool.despawn(unit, query,
            |unit, query| {
                unit.despawned(query);
            });

        Ok(())
    }

    #[inline]
    pub fn despawn_unit_at_cell(&mut self, query: &Query<'config, '_>, tile_base_cell: Cell) -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        let unit =
            query.world().find_unit_for_cell_mut(tile_base_cell, query.tile_map())
                .expect("Tile cell does not contain a Unit!");

        self.despawn_unit(query, unit)
    }

    #[inline]
    pub fn find_unit(&self, id: UnitId) -> Option<&Unit<'config>> {
        self.unit_spawn_pool.try_get(id)
    }

    #[inline]
    pub fn find_unit_mut(&mut self, id: UnitId) -> Option<&mut Unit<'config>> {
        self.unit_spawn_pool.try_get_mut(id)
    }

    #[inline]
    pub fn find_unit_for_tile(&self, tile: &Tile) -> Option<&Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let id = UnitId::new(game_state.generation(), game_state.index());
            return self.unit_spawn_pool.try_get(id);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let id = UnitId::new(game_state.generation(), game_state.index());
            return self.unit_spawn_pool.try_get_mut(id);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Unit<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Unit) {
            return self.find_unit_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_cell_mut(&mut self, cell: Cell, tile_map: &mut TileMap) -> Option<&mut Unit<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Unit) {
            return self.find_unit_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_unit_by_name(&self, name: &str) -> Option<&Unit<'config>> {
        self.unit_spawn_pool
            .iter()
            .find(|unit| unit.name() == name)
    }

    #[inline]
    pub fn find_unit_by_name_mut(&mut self, name: &str) -> Option<&mut Unit<'config>> {
        self.unit_spawn_pool
            .iter_mut()
            .find(|unit| unit.name() == name)
    }

    #[inline]
    pub fn for_each_unit<F>(&self, mut visitor_fn: F)
        where F: FnMut(&Unit<'config>) -> bool
    {
        for unit in self.unit_spawn_pool.iter() {
            if !visitor_fn(unit) {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_unit_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(&mut Unit<'config>) -> bool
    {
        for unit in self.unit_spawn_pool.iter_mut() {
            if !visitor_fn(unit) {
                break;
            }
        }
    }

    // ----------------------
    // Units debug:
    // ----------------------

    pub fn draw_unit_debug_popups(&mut self,
                                  query: &Query,
                                  ui_sys: &UiSystem,
                                  transform: &WorldToScreenTransform,
                                  visible_range: CellRange) {

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.draw_debug_popups(
                query,
                ui_sys,
                transform,
                visible_range);
        };
    }

    pub fn draw_unit_debug_ui(&mut self,
                              query: &Query<'config, '_>,
                              ui_sys: &UiSystem,
                              selected_cell: Cell) {

        let tile = match query.tile_map.find_tile(selected_cell,
                                                             TileMapLayerKind::Objects,
                                                             TileKind::Unit) {
            Some(tile) => tile,
            None => return,
        };

        if let Some(unit) = self.find_unit_for_tile_mut(tile) {
            unit.draw_debug_ui(query, ui_sys);
        }
    }

    // ----------------------
    // World debug:
    // ----------------------

    pub fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if let Some(_tab_bar) = ui.tab_bar("World Stats Tab Bar") {
            self.stats.draw_debug_ui(ui_sys);
        }
    }
}

// ----------------------------------------------
// GenerationalIndex
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenerationalIndex {
    generation: u32,
    index: u32, // Index into spawn pool; u32::MAX = invalid.
}

impl GenerationalIndex {
    #[inline]
    pub fn new(generation: u32, index: usize) -> Self {
        // Reserved value for invalid.
        debug_assert!(generation < u32::MAX);
        debug_assert!(index < u32::MAX as usize);
        Self {
            generation,
            index: index.try_into().expect("Index cannot fit into u32!"),
        }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self {
            generation: u32::MAX,
            index: u32::MAX,
        }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.generation < u32::MAX && self.index < u32::MAX
    }

    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }

    #[inline]
    pub fn index(self) -> usize {
        self.index as usize
    }
}

impl Default for GenerationalIndex {
    #[inline]
    fn default() -> Self {
        Self::invalid()
    }
}

impl std::fmt::Display for GenerationalIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "[{},{}]", self.generation, self.index)
        } else {
            write!(f, "[invalid]")
        }
    }
}

pub type BuildingId = GenerationalIndex;
pub type UnitId = GenerationalIndex;

// ----------------------------------------------
// GameObject
// ----------------------------------------------

pub trait GameObject<'config> {
    fn id(&self) -> GenerationalIndex;

    #[inline]
    fn is_spawned(&self) -> bool {
        self.id().is_valid()
    }

    fn update(&mut self, query: &Query<'config, '_>);
    fn tally(&self, stats: &mut WorldStats);
}

// ----------------------------------------------
// SpawnPool
// ----------------------------------------------

struct SpawnPool<T> {
    instances: Vec<T>,
    spawned: BitVec,
    generation: u32,
}

struct SpawnPoolIter<'a, T> {
    instances: iter::Enumerate<slice::Iter<'a, T>>,
    spawned: &'a BitVec,
}

struct SpawnPoolIterMut<'a, T> {
    instances: iter::Enumerate<slice::IterMut<'a, T>>,
    spawned: &'a BitVec,
}

impl<'a, T> Iterator for SpawnPoolIter<'a, T> {
    type Item = &'a T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yields only *spawned* instances.
        while let Some((index, instance)) = self.instances.next() {
            if self.spawned[index] {
                return Some(instance);
            }
        }
        None
    }
}

impl<T> iter::FusedIterator for SpawnPoolIter<'_, T> {}

impl<'a, T> Iterator for SpawnPoolIterMut<'a, T> {
    type Item = &'a mut T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yields only *spawned* instances.
        while let Some((index, instance)) = self.instances.next() {
            if self.spawned[index] {
                return Some(instance);
            }
        }
        None
    }
}

impl<T> iter::FusedIterator for SpawnPoolIterMut<'_, T> {}

impl<'config, T> SpawnPool<T>
    where T: GameObject<'config> + Clone + Default
{
    fn new(capacity: usize) -> Self {
        let default_instance = T::default();
        Self {
            instances: vec![default_instance; capacity],
            spawned: BitVec::repeat(false, capacity),
            generation: 0,
        }
    }

    fn clear<F>(&mut self, query: &Query, on_despawned_fn: F)
        where F: Fn(&mut T, &Query)
    {
        debug_assert!(self.is_valid());

        for instance in self.iter_mut() {
            on_despawned_fn(instance, query);
        }

        self.instances.fill(T::default());
        self.spawned.fill(false);
    }

    fn spawn<F>(&mut self, query: &Query, on_spawned_fn: F) -> &mut T
        where F: FnOnce(&mut T, &Query, GenerationalIndex)
    {
        debug_assert!(self.is_valid());

        let generation = self.generation;
        self.generation += 1;

        // Try find a free slot to reuse:
        if let Some(recycled_index) = self.spawned.first_zero() {
            let recycled_instance = &mut self.instances[recycled_index];

            debug_assert!(!recycled_instance.is_spawned());
            on_spawned_fn(recycled_instance, query, GenerationalIndex::new(generation, recycled_index));

            self.spawned.set(recycled_index, true);

            return recycled_instance;
        }

        // Need to instantiate a new one.
        let new_index = self.instances.len();
        let mut new_instance = T::default();

        debug_assert!(!new_instance.is_spawned());
        on_spawned_fn(&mut new_instance, query, GenerationalIndex::new(generation, new_index));

        self.instances.push(new_instance);
        self.spawned.push(true);

        &mut self.instances[new_index]
    }

    fn despawn<F>(&mut self, instance: &mut T, query: &Query, on_despawned_fn: F)
        where F: FnOnce(&mut T, &Query)
    {
        debug_assert!(self.is_valid());
        debug_assert!(instance.is_spawned());

        let index = instance.id().index();
        debug_assert!(self.spawned[index]);
        debug_assert!(std::ptr::eq(&self.instances[index], instance)); // Ensure addresses are the same.

        on_despawned_fn(instance, query);
        self.spawned.set(index, false);
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.instances.len() == self.spawned.len()
    }

    #[inline]
    fn iter(&self) -> SpawnPoolIter<'_, T> {
        SpawnPoolIter {
            instances: self.instances.iter().enumerate(),
            spawned: &self.spawned,
        }
    }

    #[inline]
    fn iter_mut(&mut self) -> SpawnPoolIterMut<'_, T> {
        SpawnPoolIterMut {
            instances: self.instances.iter_mut().enumerate(),
            spawned: &self.spawned,
        }
    }

    #[inline]
    fn try_get(&self, id: GenerationalIndex) -> Option<&T> {
        debug_assert!(self.is_valid());

        if !id.is_valid() {
            return None;
        }

        let index = id.index();
        if !self.spawned[index] {
            return None;
        }

        let instance = &self.instances[index];
        debug_assert!(instance.is_spawned());

        if instance.id().generation != id.generation() {
            return None;
        }

        Some(instance)
    }

    #[inline]
    fn try_get_mut(&mut self, id: GenerationalIndex) -> Option<&mut T> {
        debug_assert!(self.is_valid());

        if !id.is_valid() {
            return None;
        }

        let index = id.index();
        if !self.spawned[index] {
            return None;
        }

        let instance = &mut self.instances[index];
        debug_assert!(instance.is_spawned());

        if instance.id().generation != id.generation() {
            return None;
        }

        Some(instance)
    }

    #[inline]
    fn try_get_at(&self, index: usize) -> Option<&T> {
        debug_assert!(self.is_valid());

        if !self.spawned[index] {
            return None;
        }

        let instance = &self.instances[index];
        debug_assert!(instance.is_spawned());
        Some(instance)
    }

    #[inline]
    fn try_get_at_mut(&mut self, index: usize) -> Option<&mut T> {
        debug_assert!(self.is_valid());

        if !self.spawned[index] {
            return None;
        }

        let instance = &mut self.instances[index];
        debug_assert!(instance.is_spawned());
        Some(instance)
    }
}

// ----------------------------------------------
// GlobalResourceCounts / WorldStats
// ----------------------------------------------

struct GlobalResourceCounts {
    // Combined sum of resources (all units + all buildings).
    all: ResourceStock,

    // Resources held by spawned units.
    units: ResourceStock,

    // Resources held by each kind of building.
    storage_yards: ResourceStock,
    granaries: ResourceStock,
    houses: ResourceStock,
    markets: ResourceStock,
    producers: ResourceStock,
    services: ResourceStock,
}

pub struct WorldStats {
    // Global counts:
    pub population: u32,
    pub workers: u32,

    // Housing stats:
    houses: u32,
    lowest_house_level: HouseLevel,
    highest_house_level: HouseLevel,

    // Global resource tally:
    resources: GlobalResourceCounts,
}

impl WorldStats {
    fn new() -> Self {
        Self {
            population: 0,
            workers: 0,
            houses: 0,
            lowest_house_level: HouseLevel::max(),
            highest_house_level: HouseLevel::min(),
            resources: GlobalResourceCounts {
                all: ResourceStock::accept_all(),
                units: ResourceStock::accept_all(),
                storage_yards: ResourceStock::accept_all(),
                granaries: ResourceStock::with_accepted_kinds(ResourceKind::foods()),
                houses: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                markets: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                producers: ResourceStock::accept_all(),
                services: ResourceStock::accept_all(),
            }
        }
    }

    fn reset(&mut self) {
        // Reset all counts to zero.
        *self = Self::new();
    }

    pub fn add_unit_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.units.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_storage_yard_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.storage_yards.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_granary_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.granaries.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_house_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.houses.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_market_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.markets.add(kind, count);
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_producer_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.producers.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_service_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn update_house_level(&mut self, level: HouseLevel) {
        if level < self.lowest_house_level {
            self.lowest_house_level = level;
        }
        if level > self.highest_house_level {
            self.highest_house_level = level;
        }
        self.houses += 1;
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if let Some(_tab) = ui.tab_item("Population/Workers") {
            ui.text(format!("Population : {}", self.population));
            ui.text(format!("Workers    : {}", self.workers));

            ui.separator();

            if self.houses != 0 {
                ui.text("Housing:");
                ui.text(format!("Number Of Houses    : {}", self.houses));
                ui.text(format!("Lowest House Level  : {}", self.lowest_house_level  as u32));
                ui.text(format!("Highest House Level : {}", self.highest_house_level as u32));
            }
        }

        if let Some(_tab) = ui.tab_item("Resources") {
            let resources = &self.resources;
            resources.all.draw_debug_ui("All Resources", ui_sys);

            ui.separator();

            ui.text("In Storage:");
            resources.storage_yards.draw_debug_ui("Storage Yards", ui_sys);
            resources.granaries.draw_debug_ui("Granaries", ui_sys);

            ui.separator();

            ui.text("Buildings:");
            resources.houses.draw_debug_ui("Houses", ui_sys);
            resources.producers.draw_debug_ui("Producers", ui_sys);
            resources.services.draw_debug_ui("Services", ui_sys);

            ui.separator();

            ui.text("Other:");
            resources.units.draw_debug_ui("Units", ui_sys);
            resources.markets.draw_debug_ui("Markets", ui_sys);
        }
    }
}
