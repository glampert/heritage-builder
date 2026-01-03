use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::IntoDiscriminant;

use object::*;
use stats::*;

use crate::{
    log,
    save::*,
    ui::UiSystem,
    game::{
        constants::*,
        prop::{Prop, PropId, config::PropConfigs},
        sim::{
            debug::DebugUiMode, resources::GlobalTreasury, Query
        },
        unit::{
            config::{UnitConfigKey, UnitConfigs},
            Unit, UnitId,
        },
        building::{
            config::BuildingConfigs, Building, BuildingArchetypeKind,
            BuildingId, BuildingKind, BUILDING_ARCHETYPE_COUNT,
        },
    },
    tile::{
        sets::{TileDef, TileSets, OBJECTS_UNITS_CATEGORY},
        Tile, TileGameObjectHandle, TileKind, TileMap, TileMapLayerKind, TilePoolIndex,
    },
    utils::coords::{Cell, CellRange, WorldToScreenTransform},
};

pub mod debug;
pub mod object;
pub mod stats;

// ----------------------------------------------
// World
// ----------------------------------------------

// Holds the world state and provides queries.
#[derive(Serialize, Deserialize)]
pub struct World {
    #[serde(skip)]
    stats: WorldStats,

    // One spawn pool per building archetype.
    // Iteration yields only *spawned* buildings.
    building_spawn_pools: [(BuildingArchetypeKind, SpawnPool<Building>); BUILDING_ARCHETYPE_COUNT],

    // All units, spawned and despawned.
    // Iteration yields only *spawned* units.
    unit_spawn_pool: SpawnPool<Unit>,

    // All world props (e.g. trees).
    prop_spawn_pool: SpawnPool<Prop>,
}

impl World {
    pub fn new() -> Self {
        Self {
            // World Stats:
            stats: WorldStats::default(),
            // Buildings:
            building_spawn_pools: [(BuildingArchetypeKind::ProducerBuilding,
                                    SpawnPool::new(PRODUCER_BUILDINGS_POOL_CAPACITY,
                                                    INITIAL_GENERATION)),
                                    (BuildingArchetypeKind::StorageBuilding,
                                    SpawnPool::new(STORAGE_BUILDINGS_POOL_CAPACITY,
                                                    INITIAL_GENERATION)),
                                    (BuildingArchetypeKind::ServiceBuilding,
                                    SpawnPool::new(SERVICE_BUILDINGS_POOL_CAPACITY,
                                                    INITIAL_GENERATION)),
                                    (BuildingArchetypeKind::HouseBuilding,
                                    SpawnPool::new(HOUSE_BUILDINGS_POOL_CAPACITY,
                                                    INITIAL_GENERATION))],
            // Units:
            unit_spawn_pool: SpawnPool::new(UNIT_SPAWN_POOL_CAPACITY, INITIAL_GENERATION),
            // Props:
            prop_spawn_pool: SpawnPool::new(PROP_SPAWN_POOL_CAPACITY, INITIAL_GENERATION),
        }
    }

    pub fn reset(&mut self, query: &Query) {
        for (_, buildings) in &mut self.building_spawn_pools {
            buildings.clear(query, Building::despawned);
        }

        self.unit_spawn_pool.clear(query, Unit::despawned);
        self.prop_spawn_pool.clear(query, Prop::despawned);
    }

    pub fn update_unit_navigation(&mut self, query: &Query) {
        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update_navigation(query);
        }
    }

    pub fn update(&mut self, query: &Query) {
        self.stats.reset();

        query.treasury().tally(&mut self.stats);

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.update(query);
            unit.tally(&mut self.stats);
        }

        for prop in self.prop_spawn_pool.iter_mut() {
            prop.update(query);
            prop.tally(&mut self.stats);
        }

        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == *archetype_kind);
                building.update(query);
                building.tally(&mut self.stats);
            }
        }
    }

    #[inline]
    pub fn stats(&self) -> &WorldStats {
        &self.stats
    }

    #[inline]
    pub fn stats_mut(&mut self) -> &mut WorldStats {
        &mut self.stats
    }

    pub fn buildings_stats(&self) -> (usize,  usize) {
        let mut buildings_spawned = 0;
        let mut peak_buildings_spawned = 0;

        for (_, buildings) in &self.building_spawn_pools {
            buildings_spawned += buildings.spawned_count();
            peak_buildings_spawned += buildings.spawned_peak();
        }

        (buildings_spawned, peak_buildings_spawned)
    }

    pub fn units_stats(&self) -> (usize,  usize) {
        (self.unit_spawn_pool.spawned_count(), self.unit_spawn_pool.spawned_peak())
    }

    pub fn prop_stats(&self) -> (usize,  usize) {
        (self.prop_spawn_pool.spawned_count(), self.prop_spawn_pool.spawned_peak())
    }

    pub fn find_game_object_for_tile(&self, tile: &Tile) -> Option<&dyn GameObject> {
        if tile.is(TileKind::Building) {
            self.find_building_for_tile(tile).map(|building| building as &dyn GameObject)
        } else if tile.is(TileKind::Unit) {
            self.find_unit_for_tile(tile).map(|unit| unit as &dyn GameObject)
        } else if tile.is(TileKind::Prop) {
            self.find_prop_for_tile(tile).map(|prop| prop as &dyn GameObject)
        } else {
            None
        }
    }

    pub fn find_game_object_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut dyn GameObject> {
        if tile.is(TileKind::Building) {
            self.find_building_for_tile_mut(tile).map(|building| building as &mut dyn GameObject)
        } else if tile.is(TileKind::Unit) {
            self.find_unit_for_tile_mut(tile).map(|unit| unit as &mut dyn GameObject)
        } else if tile.is(TileKind::Prop) {
            self.find_prop_for_tile_mut(tile).map(|prop| prop as &mut dyn GameObject)
        } else {
            None
        }
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        Building::register_callbacks();
        Unit::register_callbacks();
        Prop::register_callbacks();
    }

    // ----------------------
    // Buildings API:
    // ----------------------

    pub fn try_spawn_building_with_tile_def(&mut self,
                                            query: &Query,
                                            tile_base_cell: Cell,
                                            tile_def: &'static TileDef)
                                            -> Result<&mut Building, String> {
        debug_assert!(tile_base_cell.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Building));

        // Allocate & place a Tile:
        match query.tile_map().try_place_tile(tile_base_cell, tile_def) {
            Ok(tile) => {
                // Instantiate new Building:
                match BuildingConfigs::get().new_building_archetype_for_tile_def(tile_def, query.rng()) {
                    Ok((building_kind, building_archetype)) => {
                        let archetype_kind = building_archetype.discriminant();
                        let buildings = self.buildings_pool_mut(archetype_kind);

                        let building = buildings.spawn(query,
                            |building, query, id| {
                                building.spawned(query, id, building_kind, tile.cell_range(), building_archetype);
                            });
                        debug_assert!(building.is_spawned());

                        // Store building index and kind so we can refer back to it from the Tile instance.
                        tile.set_game_object_handle(
                            TileGameObjectHandle::new_building(
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
            }
            Err(err) => {
                Err(format!("Failed to place Building tile at cell {} with TileDef '{}': {}",
                            tile_base_cell, tile_def.name, err))
            }
        }
    }

    pub fn despawn_building(&mut self,
                            query: &Query,
                            building: &mut Building)
                            -> Result<(), String> {
        let tile_base_cell = building.base_cell();
        debug_assert!(tile_base_cell.is_valid());

        let tile_map = query.tile_map();

        // Find and validate associated Tile:
        let tile =
            tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Building)
                    .ok_or("Building should have an associated Tile in the TileMap!")?;

        let game_object_handle = tile.game_object_handle();
        if !game_object_handle.is_valid() {
            return Err(format!("Building tile '{}' {} should have a valid TileGameObjectHandle!",
                               tile.name(),
                               tile_base_cell));
        }

        // Remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        let pool_index = game_object_handle.index();
        let building_kind = BuildingKind::from_game_object_handle(game_object_handle);
        let archetype_kind = building_kind.archetype_kind();
        let buildings = self.buildings_pool_mut(archetype_kind);

        debug_assert!(pool_index == building.id().index());

        // Put the building instance back into the spawn pool.
        buildings.despawn(building, query, Building::despawned);
        Ok(())
    }

    #[inline]
    pub fn despawn_building_at_cell(&mut self,
                                    query: &Query,
                                    tile_base_cell: Cell)
                                    -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        let building = query.world()
                            .find_building_for_cell_mut(tile_base_cell, query.tile_map())
                            .expect("Tile cell does not contain a Building!");

        self.despawn_building(query, building)
    }

    #[inline]
    pub fn find_building(&self, kind: BuildingKind, id: BuildingId) -> Option<&Building> {
        let buildings = self.buildings_pool(kind.archetype_kind());
        buildings.try_get(id)
    }

    #[inline]
    pub fn find_building_mut(&mut self,
                             kind: BuildingKind,
                             id: BuildingId)
                             -> Option<&mut Building> {
        let buildings = self.buildings_pool_mut(kind.archetype_kind());
        buildings.try_get_mut(id)
    }

    #[inline]
    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let pool_index = game_object_handle.index();
            let building_kind = BuildingKind::from_game_object_handle(game_object_handle);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_pool(archetype_kind);
            return buildings.try_get_at(pool_index); // NOTE: Does not perform
                                                     // generation check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Building> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let pool_index = game_object_handle.index();
            let building_kind = BuildingKind::from_game_object_handle(game_object_handle);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_pool_mut(archetype_kind);
            return buildings.try_get_at_mut(pool_index); // NOTE: Does not
                                                         // perform generation
                                                         // check.
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Building> {
        if let Some(tile) = tile_map.find_tile(cell,
                                               TileMapLayerKind::Objects,
                                               TileKind::Building | TileKind::Blocker)
        {
            return self.find_building_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_for_cell_mut(&mut self,
                                      cell: Cell,
                                      tile_map: &TileMap)
                                      -> Option<&mut Building> {
        if let Some(tile) = tile_map.find_tile(cell,
                                               TileMapLayerKind::Objects,
                                               TileKind::Building | TileKind::Blocker)
        {
            return self.find_building_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_building_by_name(&self, name: &str, kind: BuildingKind) -> Option<&Building> {
        self.buildings_pool(kind.archetype_kind())
            .iter()
            .find(|building| building.name() == name && building.is(kind))
    }

    #[inline]
    pub fn find_building_by_name_mut(&mut self,
                                     name: &str,
                                     kind: BuildingKind)
                                     -> Option<&mut Building> {
        self.buildings_pool_mut(kind.archetype_kind())
            .iter_mut()
            .find(|building| building.name() == name && building.is(kind))
    }

    // Iterates *all* buildings of a kind in the world, in unspecified order.
    // Visitor function should return true to continue iterating or false to stop.
    // `building_kinds` can be a combination of ORed BuildingKind flags.
    #[inline]
    pub fn for_each_building<F>(&self, building_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&Building) -> bool
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
        where F: FnMut(&mut Building) -> bool
    {
        let buildings = self.buildings_pool_mut(building_kinds.archetype_kind());
        for building in buildings.iter_mut() {
            if building.is(building_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    #[inline]
    fn buildings_pool(&self, archetype_kind: BuildingArchetypeKind) -> &SpawnPool<Building> {
        let (pool_archetype, buildings) = &self.building_spawn_pools[archetype_kind as usize];
        debug_assert!(archetype_kind == *pool_archetype);
        buildings
    }

    #[inline]
    fn buildings_pool_mut(&mut self,
                          archetype_kind: BuildingArchetypeKind)
                          -> &mut SpawnPool<Building> {
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
                                      transform: WorldToScreenTransform,
                                      visible_range: CellRange) {
        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == *archetype_kind);
                building.draw_debug_popups(query, ui_sys, transform, visible_range);
            }
        }
    }

    pub fn draw_building_debug_ui(&mut self,
                                  query: &Query,
                                  ui_sys: &UiSystem,
                                  tile: &Tile,
                                  mode: DebugUiMode) {
        if let Some(building) = self.find_building_for_tile_mut(tile) {
            building.draw_debug_ui(query, ui_sys, mode);
        }
    }

    // ----------------------
    // Units API:
    // ----------------------

    pub fn try_spawn_unit_with_config(&mut self,
                                      query: &Query,
                                      unit_origin: Cell,
                                      unit_config: UnitConfigKey)
                                      -> Result<&mut Unit, String> {
        debug_assert!(unit_origin.is_valid());

        let config = UnitConfigs::get().find_config_by_key(unit_config);

        // Find TileDef:
        if let Some(tile_def) = TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Objects,
                                                                      OBJECTS_UNITS_CATEGORY.hash,
                                                                      config.tile_def_name_hash)
        {
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
                    tile.set_game_object_handle(TileGameObjectHandle::new_unit(unit.id().index(),
                                                                               unit.id()
                                                                                   .generation()));

                    Ok(unit)
                }
                Err(err) => Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                                        unit_origin, tile_def.name, err)),
            }
        } else {
            Err(format!("Failed to spawn Unit at cell {} with config '{}': Cannot find TileDef '{}'!",
                        unit_origin, unit_config, config.tile_def_name))
        }
    }

    pub fn try_spawn_unit_with_tile_def(&mut self,
                                        query: &Query,
                                        unit_origin: Cell,
                                        tile_def: &'static TileDef)
                                        -> Result<&mut Unit, String> {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Unit));

        // Allocate & place a Tile:
        match query.tile_map().try_place_tile(unit_origin, tile_def) {
            Ok(tile) => {
                let config = UnitConfigs::get().find_config_by_hash(tile_def.hash, &tile_def.name);

                // Spawn unit:
                let unit = self.unit_spawn_pool.spawn(query,
                    |unit, _query, id| {
                        unit.spawned(tile, config, id);
                    });
                debug_assert!(unit.is_spawned());

                // Store unit index so we can refer back to it from the Tile instance.
                tile.set_game_object_handle(TileGameObjectHandle::new_unit(unit.id().index(),
                                                                           unit.id().generation()));

                Ok(unit)
            }
            Err(err) => Err(format!("Failed to spawn Unit at cell {} with TileDef '{}': {}",
                                    unit_origin, tile_def.name, err)),
        }
    }

    pub fn despawn_unit(&mut self, query: &Query, unit: &mut Unit) -> Result<(), String> {
        debug_assert!(unit.is_spawned());
        let tile_map = query.tile_map();

        let tile_cell = unit.cell();
        debug_assert!(tile_cell.is_valid());

        let mut tiles = SmallVec::<[(TileGameObjectHandle, TilePoolIndex, Cell); 10]>::new();

        // Find and validate associated Tile:
        let tile = tile_map.find_tile(tile_cell, TileMapLayerKind::Objects, TileKind::Unit)
                           .ok_or("Unit should have an associated Tile in the TileMap!")?;

        tiles.push((tile.game_object_handle(), tile.index(), tile.base_cell()));

        if tile.is_stacked() {
            tile_map.visit_next_tiles(tile, |next_tile| {
                        tiles.push((next_tile.game_object_handle(),
                                    next_tile.index(),
                                    next_tile.base_cell()));
                    });
        }

        for (game_object_handle, tile_index, cell) in &tiles {
            if !game_object_handle.is_valid() {
                return Err(format!("Unit tile '{}' {} should have a valid TileGameObjectHandle!",
                                   tile.name(),
                                   tile_cell));
            }

            if game_object_handle.index() == unit.id().index()
               && game_object_handle.generation() == unit.id().generation()
            {
                debug_assert!(unit.cell() == *cell);
                debug_assert!(unit.tile_index() == *tile_index);

                // First remove the associated Tile:
                tile_map.try_clear_tile_from_layer_by_index(*tile_index,
                                                            tile_cell,
                                                            TileMapLayerKind::Objects)?;

                // Put the unit instance back into the spawn pool.
                self.unit_spawn_pool.despawn(unit, query, Unit::despawned);
                return Ok(());
            }
        }

        if cfg!(debug_assertions) {
            log::error!("Failed to find tile for Unit '{}' @ {}, id: {}.",
                        unit.name(),
                        tile_cell,
                        unit.id());
            log::error!("--- Tiles @ {tile_cell} ---");

            for (game_object_handle, tile_index, cell) in &tiles {
                let id = UnitId::new(game_object_handle.generation(), game_object_handle.index());
                let unit = self.unit_spawn_pool.try_get_mut(id).unwrap();
                log::error!(" * Unit '{}': {game_object_handle:?}, {tile_index:?}, {cell:?}",
                            unit.name());
            }

            panic!("Failed to find tile for Unit '{}' @ {}, id: {}.",
                   unit.name(),
                   tile_cell,
                   unit.id());
        } else {
            Err(format!("Failed to find tile for Unit '{}' @ {}, id: {}.",
                        unit.name(),
                        tile_cell,
                        unit.id()))
        }
    }

    pub fn despawn_unit_at_cell(&mut self,
                                query: &Query,
                                tile_base_cell: Cell)
                                -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        let mut units = SmallVec::<[&mut Unit; 10]>::new();

        let tile = query.tile_map()
                        .find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Unit)
                        .ok_or("Tile cell does not contain a Unit!")?;

        let unit = query.world()
                        .find_unit_for_tile_mut(tile)
                        .ok_or("Unit tile does not have a valid TileGameObjectHandle!")?;

        units.push(unit);

        if tile.is_stacked() {
            query.tile_map().visit_next_tiles_mut(tile, |next_tile| {
                                let next_unit = query.world().find_unit_for_tile_mut(next_tile)
                    .expect("Next Unit tile does not have a valid TileGameObjectHandle!");
                                units.push(next_unit);
                            });
        }

        // This will take care of removing all tiles stacked at `tile_base_cell`.
        query.tile_map().try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        // Despawn all units at this cell.
        for unit in units {
            query.world().unit_spawn_pool.despawn(unit, query, Unit::despawned);
        }

        Ok(())
    }

    #[inline]
    pub fn find_unit(&self, id: UnitId) -> Option<&Unit> {
        self.unit_spawn_pool.try_get(id)
    }

    #[inline]
    pub fn find_unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.unit_spawn_pool.try_get_mut(id)
    }

    #[inline]
    pub fn find_unit_for_tile(&self, tile: &Tile) -> Option<&Unit> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let id = UnitId::new(game_object_handle.generation(), game_object_handle.index());
            return self.unit_spawn_pool.try_get(id);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Unit> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let id = UnitId::new(game_object_handle.generation(), game_object_handle.index());
            return self.unit_spawn_pool.try_get_mut(id);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Unit> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Unit) {
            return self.find_unit_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_unit_for_cell_mut(&mut self,
                                  cell: Cell,
                                  tile_map: &mut TileMap)
                                  -> Option<&mut Unit> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Unit) {
            return self.find_unit_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_unit_by_name(&self, name: &str) -> Option<&Unit> {
        self.unit_spawn_pool.iter().find(|unit| unit.name() == name)
    }

    #[inline]
    pub fn find_unit_by_name_mut(&mut self, name: &str) -> Option<&mut Unit> {
        self.unit_spawn_pool.iter_mut().find(|unit| unit.name() == name)
    }

    #[inline]
    pub fn for_each_unit<F>(&self, mut visitor_fn: F)
        where F: FnMut(&Unit) -> bool
    {
        for unit in self.unit_spawn_pool.iter() {
            if !visitor_fn(unit) {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_unit_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(&mut Unit) -> bool
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
                                  transform: WorldToScreenTransform,
                                  visible_range: CellRange) {
        for unit in self.unit_spawn_pool.iter_mut() {
            unit.draw_debug_popups(query, ui_sys, transform, visible_range);
        }
    }

    pub fn draw_unit_debug_ui(&mut self,
                              query: &Query,
                              ui_sys: &UiSystem,
                              tile: &Tile,
                              mode: DebugUiMode) {
        if let Some(unit) = self.find_unit_for_tile_mut(tile) {
            unit.draw_debug_ui(query, ui_sys, mode);
        }
    }

    // ----------------------
    // Props API:
    // ----------------------

    pub fn try_spawn_prop_with_tile_def(&mut self,
                                        query: &Query,
                                        prop_base_cell: Cell,
                                        tile_def: &'static TileDef)
                                        -> Result<&mut Prop, String> {
        debug_assert!(prop_base_cell.is_valid());
        debug_assert!(tile_def.is_valid());
        debug_assert!(tile_def.is(TileKind::Prop));

        // Allocate & place a Tile:
        match query.tile_map().try_place_tile(prop_base_cell, tile_def) {
            Ok(tile) => {
                let config = PropConfigs::get().find_config_by_hash(tile_def.hash, &tile_def.name);

                // Spawn prop:
                let prop = self.prop_spawn_pool.spawn(query,
                    |prop, _query, id| {
                        prop.spawned(tile, config, id);
                    });
                debug_assert!(prop.is_spawned());

                // Store prop index so we can refer back to it from the Tile instance.
                tile.set_game_object_handle(TileGameObjectHandle::new_prop(prop.id().index(),
                                                                           prop.id().generation()));

                Ok(prop)
            }
            Err(err) => Err(format!("Failed to spawn Prop at cell {} with TileDef '{}': {}",
                                    prop_base_cell, tile_def.name, err)),
        }
    }

    pub fn despawn_prop(&mut self,
                        query: &Query,
                        prop: &mut Prop)
                        -> Result<(), String> {
        let tile_base_cell = prop.cell();
        debug_assert!(tile_base_cell.is_valid());

        let tile_map = query.tile_map();

        // Find and validate associated Tile:
        let tile =
            tile_map.find_tile(tile_base_cell, TileMapLayerKind::Objects, TileKind::Prop)
                    .ok_or("Prop should have an associated Tile in the TileMap!")?;

        let game_object_handle = tile.game_object_handle();
        if !game_object_handle.is_valid() {
            return Err(format!("Prop tile '{}' {} should have a valid TileGameObjectHandle!",
                               tile.name(),
                               tile_base_cell));
        }

        debug_assert!(game_object_handle.index() == prop.id().index());
        debug_assert!(game_object_handle.generation() == prop.id().generation());

        // Remove the associated Tile:
        tile_map.try_clear_tile_from_layer(tile_base_cell, TileMapLayerKind::Objects)?;

        // Despawn prop instance:
        self.prop_spawn_pool.despawn(prop, query, Prop::despawned);
        Ok(())
    }

    #[inline]
    pub fn despawn_prop_at_cell(&mut self,
                                query: &Query,
                                tile_base_cell: Cell)
                                -> Result<(), String> {
        debug_assert!(tile_base_cell.is_valid());

        let prop = query.world()
                            .find_prop_for_cell_mut(tile_base_cell, query.tile_map())
                            .expect("Tile cell does not contain a Prop!");

        self.despawn_prop(query, prop)
    }

    #[inline]
    pub fn find_prop(&self, id: PropId) -> Option<&Prop> {
        self.prop_spawn_pool.try_get(id)
    }

    #[inline]
    pub fn find_prop_mut(&mut self, id: PropId) -> Option<&mut Prop> {
        self.prop_spawn_pool.try_get_mut(id)
    }

    #[inline]
    pub fn find_prop_for_tile(&self, tile: &Tile) -> Option<&Prop> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let id = PropId::new(game_object_handle.generation(), game_object_handle.index());
            return self.prop_spawn_pool.try_get(id);
        }
        None
    }

    #[inline]
    pub fn find_prop_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Prop> {
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            let id = PropId::new(game_object_handle.generation(), game_object_handle.index());
            return self.prop_spawn_pool.try_get_mut(id);
        }
        None
    }

    #[inline]
    pub fn find_prop_for_cell(&self, cell: Cell, tile_map: &TileMap) -> Option<&Prop> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Prop) {
            return self.find_prop_for_tile(tile);
        }
        None
    }

    #[inline]
    pub fn find_prop_for_cell_mut(&mut self,
                                  cell: Cell,
                                  tile_map: &mut TileMap)
                                  -> Option<&mut Prop> {
        if let Some(tile) = tile_map.find_tile(cell, TileMapLayerKind::Objects, TileKind::Prop) {
            return self.find_prop_for_tile_mut(tile);
        }
        None
    }

    #[inline]
    pub fn find_prop_by_name(&self, name: &str) -> Option<&Prop> {
        self.prop_spawn_pool.iter().find(|prop| prop.name() == name)
    }

    #[inline]
    pub fn find_prop_by_name_mut(&mut self, name: &str) -> Option<&mut Prop> {
        self.prop_spawn_pool.iter_mut().find(|prop| prop.name() == name)
    }

    #[inline]
    pub fn for_each_prop<F>(&self, mut visitor_fn: F)
        where F: FnMut(&Prop) -> bool
    {
        for prop in self.prop_spawn_pool.iter() {
            if !visitor_fn(prop) {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_prop_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(&mut Prop) -> bool
    {
        for prop in self.prop_spawn_pool.iter_mut() {
            if !visitor_fn(prop) {
                break;
            }
        }
    }

    // ----------------------
    // Props debug:
    // ----------------------

    pub fn draw_prop_debug_popups(&mut self,
                                  query: &Query,
                                  ui_sys: &UiSystem,
                                  transform: WorldToScreenTransform,
                                  visible_range: CellRange) {
        for prop in self.prop_spawn_pool.iter_mut() {
            prop.draw_debug_popups(query, ui_sys, transform, visible_range);
        }
    }

    pub fn draw_prop_debug_ui(&mut self,
                              query: &Query,
                              ui_sys: &UiSystem,
                              tile: &Tile,
                              mode: DebugUiMode) {
        if let Some(prop) = self.find_prop_for_tile_mut(tile) {
            prop.draw_debug_ui(query, ui_sys, mode);
        }
    }

    // ----------------------
    // World debug:
    // ----------------------

    pub fn draw_debug_ui(&self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if let Some(_tab_bar) = ui.tab_bar("World Stats Tab Bar") {
            self.stats.draw_debug_ui(treasury, ui_sys);
        }
    }
}

// ----------------------------------------------
// Save/Load for World
// ----------------------------------------------

impl Save for World {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load for World {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        self.stats.reset();

        for unit in self.unit_spawn_pool.iter_mut() {
            unit.post_load(context);
            unit.tally(&mut self.stats);
        }

        for prop in self.prop_spawn_pool.iter_mut() {
            prop.post_load(context);
            prop.tally(&mut self.stats);
        }

        for (archetype_kind, buildings) in &mut self.building_spawn_pools {
            for building in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == *archetype_kind);
                building.post_load(context);
                building.tally(&mut self.stats);
            }
        }
    }
}
