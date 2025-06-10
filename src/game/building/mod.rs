use std::fmt;
use rand::Rng;
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumIter, EnumDiscriminants};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    utils::{Cell},
    tile::{
        sets::{TileKind, TileDef},
        map::{Tile, TileMapLayerKind}
    }
};

use super::{
    sim::{Query}
};

pub mod producer;
pub mod storage;
pub mod service;
pub mod household;
pub mod create;

use producer::ProducerState;
use storage::StorageState;
use service::ServiceState;
use household::HouseholdState;

/*
-----------------------
  Building Archetypes  
-----------------------

* Population Building (Household):
 - Consumes resources (water, food, goods, etc).
 - Needs certain services in the neighborhood.
 - Adds a population number (workers).
 - Pays tax (income).
 - Can evolve/expand (more population capacity).
 - Only evolves if it has required resources and services.

* Production Building:
 - Produces a resource (farm, fishing wharf, factory).
 - Uses workers (min, max workers needed). Production output depends on number of workers.
 - May need a raw material (factory needs wood, metal, etc).
 - Needs Storage Buildings to store production.

* Storage Building:
 - Stores production from Production Buildings (granary, storage yard).
 - Uses workers (min, max workers needed).

* Service Building:
 - Uses workers (min, max workers needed).
 - May consume resources (food, goods, etc) from storage (e.g.: Market).
 - Provides services to neighborhood.
*/

// ----------------------------------------------
// Building
// ----------------------------------------------

pub struct Building {
    name: String,
    map_cell: Cell,
    kind: BuildingKind,
    archetype: BuildingArchetype,
}

impl Building {
    pub fn new(name: String, map_cell: Cell, kind: BuildingKind, archetype: BuildingArchetype) -> Self {
        Self {
            name: name,
            map_cell: map_cell,
            kind: kind,
            archetype: archetype
        }
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query, delta_time_secs: f32) {
        let mut update_ctx = 
            BuildingUpdateContext::new(&self.name,
                                       self.map_cell,
                                       self.archetype_kind(),
                                       query);

        self.archetype.update(&mut update_ctx, delta_time_secs);
    }

    #[inline]
    pub fn kind(&self) -> BuildingKind {
        self.kind
    }

    #[inline]
    pub fn archetype_kind(&self) -> BuildingArchetypeKind {
        self.archetype.discriminant()
    }
}

// ----------------------------------------------
// BuildingList
// ----------------------------------------------

pub struct BuildingList {
    archetype_kind: BuildingArchetypeKind,
    buildings: Vec<Building>, // All share the same archetype.
}

impl BuildingList {
    pub fn new(archetype_kind: BuildingArchetypeKind) -> Self {
        Self {
            archetype_kind: archetype_kind,
            buildings: Vec::new(),
        }
    }

    pub fn add(&mut self, building: Building) -> usize {
        debug_assert!(building.archetype_kind() == self.archetype_kind);
        self.buildings.push(building);
        self.buildings.len() - 1
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query, delta_time_secs: f32) {
        for building in &mut self.buildings {
            building.update(query, delta_time_secs);
        }
    }
}

// ----------------------------------------------
// BuildingKind
// ----------------------------------------------

#[repr(i32)]
#[derive(Copy, Clone, Debug, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum BuildingKind {
    // Archetype: Producer

    // Archetype: Storage

    // Archetype: Service
    Well,
    Market,

    // Archetype: Household
    Household,
}

// ----------------------------------------------
// BuildingArchetype / BuildingArchetypeKind
// ----------------------------------------------

#[derive(EnumDiscriminants)]
#[strum_discriminants(repr(u32))]
#[strum_discriminants(name(BuildingArchetypeKind))]
#[strum_discriminants(derive(Display, EnumCount, EnumIter))]
pub enum BuildingArchetype {
    Producer(ProducerState),
    Storage(StorageState),
    Service(ServiceState),
    Household(HouseholdState),
}

pub const BUILDING_ARCHETYPE_COUNT: usize = BuildingArchetypeKind::COUNT;

impl BuildingArchetype {
    pub fn new_producer(state: ProducerState) -> Self {
        BuildingArchetype::Producer(state)
    }

    pub fn new_storage(state: StorageState) -> Self {
        BuildingArchetype::Storage(state)
    }

    pub fn new_service(state: ServiceState) -> Self {
        BuildingArchetype::Service(state)
    }

    pub fn new_household(state: HouseholdState) -> Self {
        BuildingArchetype::Household(state)
    }

    #[inline]
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext, delta_time_secs: f32) {
        match self {
            BuildingArchetype::Producer(state) => {
                state.update(update_ctx, delta_time_secs);
            }
            BuildingArchetype::Storage(state) => {
                state.update(update_ctx, delta_time_secs);
            }
            BuildingArchetype::Service(state) => {
                state.update(update_ctx, delta_time_secs);
            }
            BuildingArchetype::Household(state) => {
                state.update(update_ctx, delta_time_secs);
            }
        }
    }
}

// ----------------------------------------------
// BuildingUpdateContext
// ----------------------------------------------

pub struct BuildingUpdateContext<'building, 'query, 'sim, 'tile_map, 'tile_sets> {
    name: &'building str,
    map_cell: Cell,
    archetype_kind: BuildingArchetypeKind,
    query: &'query mut Query<'sim, 'tile_map, 'tile_sets>,
}

impl<'building, 'query, 'sim, 'tile_map, 'tile_sets> BuildingUpdateContext<'building, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn new(name: &'building str,
           map_cell: Cell,
           archetype_kind: BuildingArchetypeKind,
           query: &'query mut Query<'sim, 'tile_map, 'tile_sets>) -> Self {
        Self {
            name: name,
            map_cell: map_cell,
            archetype_kind: archetype_kind,
            query: query
        }
    }

    #[inline]
    pub fn find_tile_def(&self, category_name: &str, tile_name: &str) -> Option<&'tile_sets TileDef> {
        self.query.find_tile_def(TileMapLayerKind::Buildings, category_name, tile_name)
    }

    #[inline]
    pub fn find_tile(&self) -> &Tile {
        self.query.find_tile(self.map_cell, TileMapLayerKind::Buildings, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    pub fn find_tile_mut(&mut self) -> &mut Tile<'tile_sets> {
        self.query.find_tile_mut(self.map_cell, TileMapLayerKind::Buildings, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    pub fn set_random_variation(&mut self) {
        let variation_count = self.find_tile().variation_count();
        if variation_count > 1 {
            let rand_variation_index = self.query.rng.random_range(0..variation_count);
            self.find_tile_mut().set_variation_index(rand_variation_index);
        }
    }

    pub fn try_replace_tile(&mut self, tile_to_place: &'tile_sets TileDef) -> bool {
        // Replaces the give tile if the placement is valid,
        // fails and leaves the map unchanged otherwise.

        // First check if we have space to place this tile.
        let footprint = tile_to_place.calc_footprint_cells(self.map_cell);
        for footprint_cell in footprint {
            if footprint_cell == self.map_cell {
                continue;
            }

            if let Some(tile) =
                self.query.tile_map.try_tile_from_layer(footprint_cell, TileMapLayerKind::Buildings) {
                if tile.is_building() || tile.is_blocker() {
                    // Cannot expand here.
                    return false;
                }
            }
        }

        // Now we must clear the previous tile.
        if !self.query.tile_map.try_place_tile_in_layer(self.map_cell, TileMapLayerKind::Buildings, TileDef::empty()) {
            eprintln!("Failed to clear previous tile! This is unexpected...");
            return false;
        }

        // And place the new one.
        if !self.query.tile_map.try_place_tile_in_layer(self.map_cell, TileMapLayerKind::Buildings, tile_to_place) {
            eprintln!("Failed to place new tile! This is unexpected...");
            return false;
        }

        true
    }
}

impl<'building, 'query, 'sim, 'tile_map, 'tile_sets> fmt::Display for BuildingUpdateContext<'building, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Building '{}' ({:?}) [{},{}]",
               self.name,
               self.archetype_kind,
               self.map_cell.x,
               self.map_cell.y)
    }
}
