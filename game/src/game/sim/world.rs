use slab::Slab;
use bitvec::vec::BitVec;
use core::iter::{self};
use core::slice::{self};

use crate::{
    imgui_ui::UiSystem,
    tile::sets::TileKind,
    utils::{
        Seconds,
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
        }
    },
    tile::{
        sets::{
            TileSets,
            TileDef,
            OBJECTS_UNITS_CATEGORY
        },
        map::{
            Tile,
            TileMap,
            TileMapLayerKind,
            GameStateHandle
        }
    },
    game:: {
        building::{
            self,
            Building,
            BuildingKind,
            BuildingArchetypeKind,
            config::BuildingConfigs,
            BUILDING_ARCHETYPE_COUNT
        },
        unit::{
            Unit,
            config::{
                UnitConfig,
                UnitConfigs,
                UnitConfigKey
            }
        }
    }
};

use super::{
    Query
};

// ----------------------------------------------
// World
// ----------------------------------------------

// Holds the world state and provides queries.
pub struct World<'config> {
    // One list per building archetype.
    building_lists: [BuildingList<'config>; BUILDING_ARCHETYPE_COUNT],
    building_configs: &'config BuildingConfigs,
    building_generation_count: u32,

    // All units, spawned ones and despawned ones waiting to be recycled.
    // List iteration yields only *spawned* units.
    unit_spawn_pool: UnitSpawnPool<'config>,
    unit_configs: &'config UnitConfigs,
    unit_generation_count: u32,
}

impl<'config> World<'config> {
    pub fn new(building_configs: &'config BuildingConfigs, unit_configs: &'config UnitConfigs) -> Self {
        Self {
            // Buildings:
            building_lists: [
                BuildingList::new(BuildingArchetypeKind::Producer, 32),
                BuildingList::new(BuildingArchetypeKind::Storage,  32),
                BuildingList::new(BuildingArchetypeKind::Service,  128),
                BuildingList::new(BuildingArchetypeKind::House,    256),
            ],
            building_configs,
            building_generation_count: 0,
            // Units:
            unit_spawn_pool: UnitSpawnPool::new(256),
            unit_configs,
            unit_generation_count: 0,
        }
    }

    pub fn reset(&mut self) {
        for buildings in &mut self.building_lists {
            buildings.clear();
        }

        self.unit_spawn_pool.clear();
    }

    pub fn update_unit_navigation(&mut self, query: &Query<'config, '_>, delta_time_secs: Seconds) {
        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update_navigation(query, delta_time_secs);
        } 
    }

    pub fn update(&mut self, query: &Query<'config, '_>, delta_time_secs: Seconds) {
        for buildings in &mut self.building_lists {
            let list_archetype = buildings.archetype_kind();
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == list_archetype);
                building.update(query, delta_time_secs);
            }
        }

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update(query, delta_time_secs);
        }
    }

    // Increment generation count, return previous.
    #[inline]
    fn next_building_generation(&mut self) -> u32 {
        let generation = self.building_generation_count;
        self.building_generation_count += 1;
        generation
    }

    #[inline]
    fn next_unit_generation(&mut self) -> u32 {
        let generation = self.unit_generation_count;
        self.unit_generation_count += 1;
        generation
    }

    // ----------------------
    // Buildings API:
    // ----------------------

    pub fn try_spawn_building_with_tile_def<'tile_sets>(&mut self,
                                                        tile_map: &mut TileMap<'tile_sets>,
                                                        target_cell: Cell,
                                                        tile_def: &'tile_sets TileDef) -> Result<&mut Building<'config>, String> {
        debug_assert!(target_cell.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Building));

        // Allocate & place a Tile:
        match tile_map.try_place_tile(target_cell, tile_def) {
            Ok(tile) => {
                // Increment generation count:
                let generation = self.next_building_generation();

                // Instantiate new Building:
                if let Some(building) = building::config::instantiate(tile, self.building_configs) {
                    let building_kind = building.kind();
                    let archetype_kind = building.archetype_kind();
                    let buildings = self.buildings_list_mut(archetype_kind);

                    let (list_index, instance) = buildings.add_instance(building);

                    // Update tile & building handles:
                    instance.placed(GenerationalIndex::new(generation, list_index));
                    tile.set_game_state_handle(GameStateHandle::new_building(list_index, building_kind.bits()));

                    Ok(instance)
                } else {
                    Err(format!("Failed to instantiate Building at cell {} with TileDef '{}'.",
                                target_cell, tile_def.name))
                }
            },
            Err(err) => {
                Err(format!("Failed to place Building tile at cell {} with TileDef '{}': {}",
                            target_cell, tile_def.name, err))
            }
        }
    }

    pub fn despawn_building(&mut self, tile_map: &mut TileMap, building: &mut Building<'config>) -> Result<(), String> {
        let tile_base_cell = building.base_cell();
        debug_assert!(tile_base_cell.is_valid());

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Building)
            .ok_or("Building should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Building tile '{}' {} should have a valid game state!", tile.name(), tile_base_cell));
        }

        // Remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        let list_index = game_state.index();
        let building_kind = BuildingKind::from_game_state_handle(game_state);
        let archetype_kind = building_kind.archetype_kind();
        let buildings = self.buildings_list_mut(archetype_kind);

        debug_assert!(list_index == building.id().index());

        // Remove the building instance:
        buildings.remove_instance_at(list_index).map_err(|err| {
            format!("Failed to remove Building index [{}], cell {}: {}", list_index, tile_base_cell, err)
        })
    }

    pub fn despawn_building_at_cell(&mut self, tile_map: &mut TileMap, tile_base_cell: Cell) -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Building)
            .ok_or("Building should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Building tile '{}' {} should have a valid game state!", tile.name(), tile_base_cell));
        }

        // Remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        let list_index = game_state.index();
        let building_kind = BuildingKind::from_game_state_handle(game_state);
        let archetype_kind = building_kind.archetype_kind();
        let buildings = self.buildings_list_mut(archetype_kind);

        // Remove the building instance:
        buildings.remove_instance_at(list_index).map_err(|err| {
            format!("Failed to remove Building index [{}], cell {}: {}", list_index, tile_base_cell, err)
        })
    }

    #[inline]
    pub fn find_building(&self, kind: BuildingKind, id: GenerationalIndex) -> Option<&Building<'config>> {
        let buildings = self.buildings_list(kind.archetype_kind());
        buildings.try_get(id)
    }

    #[inline]
    pub fn find_building_mut(&mut self, kind: BuildingKind, id: GenerationalIndex) -> Option<&mut Building<'config>> {
        let buildings = self.buildings_list_mut(kind.archetype_kind());
        buildings.try_get_mut(id)
    }

    #[inline]
    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let list_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_list(archetype_kind);
            return buildings.try_get_at(list_index); // NOTE: Does not perform generation check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let list_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_list_mut(archetype_kind);
            return buildings.try_get_at_mut(list_index); // NOTE: Does not perform generation check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Building<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Building) {
            return self.find_building_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell_mut(&mut self, cell: Cell, tile_map: &TileMap) -> Option<&mut Building<'config>> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Building) {
            return self.find_building_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_by_name(&self, name: &str, archetype_kind: BuildingArchetypeKind) -> Option<&Building<'config>> {
        self.buildings_list(archetype_kind)
            .iter()
            .find(|building| building.name() == name)
    }

    #[inline]
    pub fn find_building_by_name_mut(&mut self, name: &str, archetype_kind: BuildingArchetypeKind) -> Option<&mut Building<'config>> {
        self.buildings_list_mut(archetype_kind)
            .iter_mut()
            .find(|building| building.name() == name)
    }

    #[inline]
    pub fn buildings_list(&self, archetype_kind: BuildingArchetypeKind) -> &BuildingList<'config> {
        let buildings = &self.building_lists[archetype_kind as usize];
        debug_assert!(buildings.archetype_kind() == archetype_kind);
        buildings
    }

    #[inline]
    pub fn buildings_list_mut(&mut self, archetype_kind: BuildingArchetypeKind) -> &mut BuildingList<'config> {
        let buildings = &mut self.building_lists[archetype_kind as usize];
        debug_assert!(buildings.archetype_kind() == archetype_kind);
        buildings
    }

    // ----------------------
    // Buildings debug:
    // ----------------------

    pub fn draw_building_debug_popups(&mut self,
                                      query: &Query<'config, '_>,
                                      ui_sys: &UiSystem,
                                      transform: &WorldToScreenTransform,
                                      visible_range: CellRange,
                                      delta_time_secs: Seconds) {

        for buildings in &mut self.building_lists {
            let list_archetype = buildings.archetype_kind();
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == list_archetype);
                building.draw_debug_popups(
                    query,
                    ui_sys,
                    transform,
                    visible_range,
                    delta_time_secs);
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

    pub fn try_spawn_unit_with_config<'tile_sets>(&mut self,
                                                  tile_map: &mut TileMap<'tile_sets>,
                                                  tile_sets: &'tile_sets TileSets,
                                                  target_cell: Cell,
                                                  unit_config_key: UnitConfigKey) -> Result<&mut Unit<'config>, String> {
        debug_assert!(target_cell.is_valid());
        debug_assert!(unit_config_key.is_valid());

        let config = self.unit_configs.find_config_by_hash(unit_config_key.hash);

        // Find TileDef:
        if let Some(tile_def) = tile_sets.find_tile_def_by_hash(
            TileMapLayerKind::Objects,
            OBJECTS_UNITS_CATEGORY.hash,
            config.tile_def_name_hash) {
            // Allocate & place a Tile:
            match tile_map.try_place_tile(target_cell, tile_def) {
                Ok(tile) => {
                    // Increment generation count:
                    let generation = self.next_unit_generation();

                    // Spawn unit:
                    let unit = self.unit_spawn_pool.spawn_instance(tile, config, generation);
                    debug_assert!(unit.is_spawned());

                    // Store unit index so we can refer back to it from the Tile instance.
                    tile.set_game_state_handle(GameStateHandle::new_unit(unit.id().index(), unit.id().generation()));
                    Ok(unit)
                },
                Err(err) => {
                    Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                                target_cell, tile_def.name, err))
                }
            }
        } else {
            Err(format!("Failed to spawn Unit at cell {} with config '{}': Cannot find TileDef '{}'!",
                        target_cell, unit_config_key.string, config.tile_def_name))
        }
    }

    pub fn try_spawn_unit_with_tile_def<'tile_sets>(&mut self,
                                                    tile_map: &mut TileMap<'tile_sets>,
                                                    target_cell: Cell,
                                                    tile_def: &'tile_sets TileDef) -> Result<&mut Unit<'config>, String> {
        debug_assert!(target_cell.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Unit));

        // Allocate & place a Tile:
        match tile_map.try_place_tile(target_cell, tile_def) {
            Ok(tile) => {
                let config = self.unit_configs.find_config_by_hash(tile_def.hash);

                // Increment generation count:
                let generation = self.next_unit_generation();

                // Spawn unit:
                let unit = self.unit_spawn_pool.spawn_instance(tile, config, generation);
                debug_assert!(unit.is_spawned());

                // Store unit index so we can refer back to it from the Tile instance.
                tile.set_game_state_handle(GameStateHandle::new_unit(unit.id().index(), unit.id().generation()));
                Ok(unit)
            },
            Err(err) => {
                Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                            target_cell, tile_def.name, err))
            }
        }
    }

    pub fn despawn_unit(&mut self, tile_map: &mut TileMap, unit: &mut Unit) -> Result<(), String> {
        debug_assert!(unit.is_spawned());

        let tile_base_cell = unit.cell();
        debug_assert!(tile_base_cell.is_valid());

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .ok_or("Unit should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Unit tile '{}' {} should have a valid game state!", tile.name(), tile_base_cell));
        }

        debug_assert!(game_state.index() == unit.id().index());
        debug_assert!(game_state.generation() == unit.id().generation());

        // First remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        // Put the unit instance back into the spawn pool.
        self.unit_spawn_pool.despawn_instance(unit);
        Ok(())
    }

    pub fn despawn_unit_at_cell(&mut self, tile_map: &mut TileMap, tile_base_cell: Cell) -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .ok_or("Unit should have an associated Tile in the TileMap!")?;

        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            return Err(format!("Unit tile '{}' {} should have a valid game state!", tile.name(), tile_base_cell));
        }

        let unit = self.unit_spawn_pool.try_get(GenerationalIndex::new(game_state.generation(), game_state.index()))
            .ok_or("Unit tile GameStateHandle is invalid!")?;

        debug_assert!(game_state.index() == unit.id().index());
        debug_assert!(game_state.generation() == unit.id().generation());

        // First remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        // Put the unit instance back into the spawn pool.
        self.unit_spawn_pool.despawn_by_id(unit.id());
        Ok(())
    }

    #[inline]
    pub fn find_unit(&self, id: GenerationalIndex) -> Option<&Unit<'config>> {
        self.unit_spawn_pool.try_get(id)
    }

    #[inline]
    pub fn find_unit_mut(&mut self, id: GenerationalIndex) -> Option<&mut Unit<'config>> {
        self.unit_spawn_pool.try_get_mut(id)
    }

    #[inline]
    pub fn find_unit_for_tile(&self, tile: &Tile) -> Option<&Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let id = GenerationalIndex::new(game_state.generation(), game_state.index());
            return self.unit_spawn_pool.try_get(id);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let id = GenerationalIndex::new(game_state.generation(), game_state.index());
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

    // ----------------------
    // Units debug:
    // ----------------------

    pub fn draw_unit_debug_popups(&mut self,
                                  query: &Query<'config, '_>,
                                  ui_sys: &UiSystem,
                                  transform: &WorldToScreenTransform,
                                  visible_range: CellRange,
                                  delta_time_secs: Seconds) {

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.draw_debug_popups(
                query,
                ui_sys,
                transform,
                visible_range,
                delta_time_secs);
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
}

// ----------------------------------------------
// GenerationalIndex
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct GenerationalIndex {
    generation: u32,
    index: u32, // Index into pool/list; u32::MAX = invalid.
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
        write!(f, "[{},{}]", self.generation, self.index)
    }
}

// ----------------------------------------------
// BuildingList
// ----------------------------------------------

pub struct BuildingList<'config> {
    archetype_kind: BuildingArchetypeKind,
    buildings: Slab<Building<'config>>, // All share the same archetype.
}

pub struct BuildingListIter<'a, 'config> {
    inner: slab::Iter<'a, Building<'config>>,
}

pub struct BuildingListIterMut<'a, 'config> {
    inner: slab::IterMut<'a, Building<'config>>,
}

impl<'a, 'config> Iterator for BuildingListIter<'a, 'config> {
    type Item = &'a Building<'config>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, building)| building)
    }
}

impl<'a, 'config> Iterator for BuildingListIterMut<'a, 'config> {
    type Item = &'a mut Building<'config>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, building)| building)
    }
}

impl<'config> BuildingList<'config> {
    #[inline]
    pub fn new(archetype_kind: BuildingArchetypeKind, capacity: usize) -> Self {
        Self {
            archetype_kind,
            buildings: Slab::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn iter(&self) -> BuildingListIter<'_, 'config> {
        BuildingListIter { inner: self.buildings.iter() }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> BuildingListIterMut<'_, 'config> {
        BuildingListIterMut { inner: self.buildings.iter_mut() }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buildings.clear();
    }

    #[inline]
    pub fn archetype_kind(&self) -> BuildingArchetypeKind {
        self.archetype_kind
    }

    #[inline]
    pub fn try_get(&self, id: GenerationalIndex) -> Option<&Building<'config>> {
        debug_assert!(id.is_valid());
        let list_index = id.index();
        match self.buildings.get(list_index) {
            Some(building) => {
                if building.id().generation() == id.generation() {
                    Some(building)
                } else {
                    None
                }
            },
            None => None,
        }
    }

    #[inline]
    pub fn try_get_mut(&mut self, id: GenerationalIndex) -> Option<&mut Building<'config>> {
        debug_assert!(id.is_valid());
        let list_index = id.index();
        match self.buildings.get_mut(list_index) {
            Some(building) => {
                if building.id().generation() == id.generation() {
                    Some(building)
                } else {
                    None
                }
            },
            None => None,
        }        
    }

    #[inline]
    pub fn try_get_at(&self, index: usize) -> Option<&Building<'config>> {
        self.buildings.get(index)
    }

    #[inline]
    pub fn try_get_at_mut(&mut self, index: usize) -> Option<&mut Building<'config>> {
        self.buildings.get_mut(index)
    }

    #[inline]
    pub fn add_instance(&mut self, building: Building<'config>) -> (usize, &mut Building<'config>) {
        debug_assert!(building.archetype_kind() == self.archetype_kind);
        let list_index = self.buildings.insert(building);
        (list_index, &mut self.buildings[list_index])
    }

    #[inline]
    pub fn remove_instance_at(&mut self, list_index: usize) -> Result<(), String> {
        if self.buildings.try_remove(list_index).is_none() {
            return Err(format!("BuildingList slot [{}] is already vacant!", list_index));
        }
        Ok(())
    }
}

// ----------------------------------------------
// UnitSpawnPool
// ----------------------------------------------

pub struct UnitSpawnPool<'config> {
    pool: Vec<Unit<'config>>,
    is_spawned_flags: BitVec,
}

pub struct UnitSpawnPoolIter<'a, 'config> {
    entries: iter::Enumerate<slice::Iter<'a, Unit<'config>>>,
    is_spawned_flags: &'a BitVec,
}

pub struct UnitSpawnPoolIterMut<'a, 'config> {
    entries: iter::Enumerate<slice::IterMut<'a, Unit<'config>>>,
    is_spawned_flags: &'a BitVec,
}

impl<'a, 'config> Iterator for UnitSpawnPoolIter<'a, 'config> {
    type Item = &'a Unit<'config>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yield only *spawned* entries.
        for (index, entry) in &mut self.entries {
            if self.is_spawned_flags[index] {
                return Some(entry);
            }
        }
        None
    }
}

impl<'a, 'config> Iterator for UnitSpawnPoolIterMut<'a, 'config> {
    type Item = &'a mut Unit<'config>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yield only *spawned* entries.
        for (index, entry) in &mut self.entries {
            if self.is_spawned_flags[index] {
                return Some(entry);
            }
        }
        None
    }
}

impl<'config> UnitSpawnPool<'config> {
    #[inline]
    pub fn new(capacity: usize) -> Self {
        let despawned_unit = Unit::default();
        Self {
            pool: vec![despawned_unit; capacity],
            is_spawned_flags: BitVec::repeat(false, capacity),
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.pool.len() == self.is_spawned_flags.len()
    }

    #[inline]
    pub fn iter(&self) -> UnitSpawnPoolIter<'_, 'config> {
        UnitSpawnPoolIter {
            entries: self.pool.iter().enumerate(),
            is_spawned_flags: &self.is_spawned_flags,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> UnitSpawnPoolIterMut<'_, 'config> {
        UnitSpawnPoolIterMut {
            entries: self.pool.iter_mut().enumerate(),
            is_spawned_flags: &self.is_spawned_flags,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        debug_assert!(self.is_valid());

        for unit in self.iter_mut() {
            unit.despawned();
        }

        self.pool.fill(Unit::default());
        self.is_spawned_flags.fill(false);
    }

    #[inline]
    pub fn try_get(&self, id: GenerationalIndex) -> Option<&Unit<'config>> {
        debug_assert!(self.is_valid());
        debug_assert!(id.is_valid());

        let pool_index = id.index();
        if !self.is_spawned_flags[pool_index] {
            return None;
        }

        let unit = &self.pool[pool_index];
        debug_assert!(unit.is_spawned());

        if unit.id().generation != id.generation() {
            return None;
        }

        Some(unit)
    }

    #[inline]
    pub fn try_get_mut(&mut self, id: GenerationalIndex) -> Option<&mut Unit<'config>> {
        debug_assert!(self.is_valid());
        debug_assert!(id.is_valid());

        let pool_index = id.index();
        if !self.is_spawned_flags[pool_index] {
            return None;
        }

        let unit = &mut self.pool[pool_index];
        debug_assert!(unit.is_spawned());

        if unit.id().generation != id.generation() {
            return None;
        }

        Some(unit)
    }

    pub fn spawn_instance(&mut self, tile: &mut Tile, config: &'config UnitConfig, generation: u32) -> &mut Unit<'config> {
        debug_assert!(self.is_valid());

        // Try find a free slot to reuse:
        if let Some(recycled_pool_index) = self.is_spawned_flags.first_zero() {
            let recycled_unit = &mut self.pool[recycled_pool_index];
            debug_assert!(!recycled_unit.is_spawned());
            recycled_unit.spawned(tile, config, GenerationalIndex::new(generation, recycled_pool_index));
            self.is_spawned_flags.set(recycled_pool_index, true);
            return recycled_unit;
        }

        // Need to instantiate a new one.
        let new_pool_index = self.pool.len();
        self.pool.push(Unit::new(tile, config, GenerationalIndex::new(generation, new_pool_index)));
        self.is_spawned_flags.push(true);
        &mut self.pool[new_pool_index]
    }

    pub fn despawn_instance(&mut self, unit: &mut Unit) {
        debug_assert!(self.is_valid());
        debug_assert!(unit.is_spawned());

        let pool_index = unit.id().index();
        debug_assert!(self.is_spawned_flags[pool_index]);
        debug_assert!(std::ptr::eq(&self.pool[pool_index], unit)); // Ensure addresses are the same.

        unit.despawned();
        self.is_spawned_flags.set(pool_index, false);
    }

    pub fn despawn_by_id(&mut self, id: GenerationalIndex) {
        debug_assert!(self.is_valid());
        debug_assert!(id.is_valid());

        let pool_index = id.index();
        debug_assert!(self.is_spawned_flags[pool_index]);

        let unit = &mut self.pool[pool_index];
        debug_assert!(unit.is_spawned());
        debug_assert!(unit.id() == id);

        unit.despawned();
        self.is_spawned_flags.set(pool_index, false);
    }
}
