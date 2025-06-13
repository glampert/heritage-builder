use std::fmt;
use rand::Rng;
use bitflags::bitflags;
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};

use crate::{
    bitflags_with_display,
    utils::Cell,
    imgui_ui::UiSystem,
    tile::{
        sets::{TileDef, TileKind},
        map::{GameStateHandle, Tile, TileMapLayerKind}
    }
};

use super::{
    sim::Query
};

use config::{
    BuildingConfigs
};

pub mod producer;
pub mod storage;
pub mod service;
pub mod house;
pub mod config;

/*
-----------------------
  Building Archetypes  
-----------------------

* Population Building (AKA House/Household):
 - Consumes resources (water, food, goods, etc).
 - Needs access to certain services in the neighborhood.
 - Adds a population number (workers).
 - Pays tax (income).
 - Can evolve/expand (more population capacity).
 - Only evolves if it has required resources and services.

* Producer Building:
 - Produces a resource/consumer good (farm, fishing wharf, factory) or raw material (mine, logging camp).
 - Uses workers (min, max workers needed). Production output depends on number of workers.
 - May need other raw materials to function (factory needs wood, metal, etc).
 - Needs Storage Buildings to store production.

* Storage Building:
 - Stores production from Producer Buildings (granary, storage yard).
 - Uses workers (min, max workers needed).

* Service Building:
 - Uses workers (min, max workers needed).
 - May consume resources (food, goods, etc) from storage (e.g.: a Market).
 - Provides services to neighborhood.
*/

// ----------------------------------------------
// Building
// ----------------------------------------------

pub struct Building<'config> {
    name: &'config str,
    kind: BuildingKind,
    map_cell: Cell,
    configs: &'config BuildingConfigs,
    archetype: BuildingArchetype<'config>,
}

impl<'config> Building<'config> {
    pub fn new(name: &'config str,
               kind: BuildingKind,
               map_cell: Cell,
               configs: &'config BuildingConfigs,
               archetype: BuildingArchetype<'config>) -> Self {
        Self {
            name: name,
            kind: kind,
            map_cell: map_cell,
            configs: configs,
            archetype: archetype
        }
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query, delta_time_secs: f32) {
        let mut update_ctx = 
            BuildingUpdateContext::new(self.name,
                                       self.kind,
                                       self.archetype_kind(),
                                       self.map_cell,
                                       self.configs,
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

    pub fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if !ui.collapsing_header("Properties##_building_props", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        ui.text(format!("Name..............: '{}'", self.name));
        ui.text(format!("Kind..............: {}", self.kind));
        ui.text(format!("Archetype.........: {}", self.archetype_kind()));
        ui.text(format!("Cell..............: {},{}", self.map_cell.x, self.map_cell.y));

        self.archetype.draw_debug_ui(ui_sys);
    }
}

// ----------------------------------------------
// BuildingList
// ----------------------------------------------

pub struct BuildingList<'config> {
    archetype_kind: BuildingArchetypeKind,
    buildings: Vec<Building<'config>>, // All share the same archetype.
}

impl<'config> BuildingList<'config> {
    pub fn new(archetype_kind: BuildingArchetypeKind) -> Self {
        Self {
            archetype_kind: archetype_kind,
            buildings: Vec::new(),
        }
    }

    #[inline]
    pub fn try_get(&self, index: usize, archetype_kind: BuildingArchetypeKind) -> Option<&Building<'config>> {
        if index >= self.buildings.len() {
            return None;
        }
        if archetype_kind != self.archetype_kind {
            return None;
        }
        Some(&self.buildings[index])
    }

    #[inline]
    pub fn add(&mut self, building: Building<'config>) -> usize {
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

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct BuildingKind: i32 {
        // Archetype: House
        const House     = 1 << 0;

        // Archetype: Producer

        // Archetype: Storage

        // Archetype: Service
        const WellSmall = 1 << 1;
        const WellBig   = 1 << 2;
        const Market    = 1 << 3;
    }
}

impl BuildingKind {
    #[inline]
    pub fn from_game_state_handle(handle: GameStateHandle) -> Self {
        BuildingKind::from_bits(handle.kind())
            .expect("GameStateHandle does not contain a valid BuildingKind enum value!")
    }

    #[inline]
    pub fn archetype_kind(self) -> BuildingArchetypeKind {
        if self.intersects(BuildingKind::House) {
            BuildingArchetypeKind::House
        } else if self.intersects(BuildingKind::WellSmall | BuildingKind::WellBig | BuildingKind::Market) {
            BuildingArchetypeKind::Service
        } else {
            panic!("Unknown archetype for building kind: {:?}", self);
        }
    }
}

// ----------------------------------------------
// BuildingArchetype / BuildingArchetypeKind
// ----------------------------------------------

#[derive(EnumDiscriminants)]
#[strum_discriminants(repr(u32))]
#[strum_discriminants(name(BuildingArchetypeKind))]
#[strum_discriminants(derive(Display, EnumCount, EnumIter))]
pub enum BuildingArchetype<'config> {
    Producer(producer::ProducerState<'config>),
    Storage(storage::StorageState<'config>),
    Service(service::ServiceState<'config>),
    House(house::HouseState<'config>),
}

pub const BUILDING_ARCHETYPE_COUNT: usize = BuildingArchetypeKind::COUNT;

impl<'config> BuildingArchetype<'config> {
    fn new_producer(state: producer::ProducerState<'config>) -> Self {
        BuildingArchetype::Producer(state)
    }

    fn new_storage(state: storage::StorageState<'config>) -> Self {
        BuildingArchetype::Storage(state)
    }

    fn new_service(state: service::ServiceState<'config>) -> Self {
        BuildingArchetype::Service(state)
    }

    fn new_house(state: house::HouseState<'config>) -> Self {
        BuildingArchetype::House(state)
    }

    #[inline]
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
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
            BuildingArchetype::House(state) => {
                state.update(update_ctx, delta_time_secs);
            }
        }
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        match self {
            BuildingArchetype::Producer(state) => {
                state.draw_debug_ui(ui_sys);
            }
            BuildingArchetype::Storage(state) => {
                state.draw_debug_ui(ui_sys);
            }
            BuildingArchetype::Service(state) => {
                state.draw_debug_ui(ui_sys);
            }
            BuildingArchetype::House(state) => {
                state.draw_debug_ui(ui_sys);
            }
        }
    }
}

// ----------------------------------------------
// BuildingUpdateContext
// ----------------------------------------------

pub struct BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    name: &'config str,
    kind: BuildingKind,
    archetype_kind: BuildingArchetypeKind,
    map_cell: Cell,
    configs: &'config BuildingConfigs,
    query: &'query mut Query<'sim, 'tile_map, 'tile_sets>,
}

impl<'config, 'query, 'sim, 'tile_map, 'tile_sets> BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn new(name: &'config str,
           kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           map_cell: Cell,
           configs: &'config BuildingConfigs,
           query: &'query mut Query<'sim, 'tile_map, 'tile_sets>) -> Self {
        Self {
            name: name,
            kind: kind,
            archetype_kind: archetype_kind,
            map_cell: map_cell,
            configs: configs,
            query: query
        }
    }

    #[inline]
    fn find_tile_def(&self, category_name: &str, tile_def_name: &str) -> Option<&'tile_sets TileDef> {
        self.query.find_tile_def(TileMapLayerKind::Buildings, category_name, tile_def_name)
    }

    #[inline]
    fn find_tile(&self) -> &Tile {
        self.query.find_tile(self.map_cell, TileMapLayerKind::Buildings, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn find_tile_mut(&mut self) -> &mut Tile<'tile_sets> {
        self.query.find_tile_mut(self.map_cell, TileMapLayerKind::Buildings, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn is_near_building(&self, kind: BuildingKind, radius_in_cells: i32) -> bool {
        self.query.is_near_building(self.map_cell, kind, radius_in_cells)
    }

    #[inline]
    fn set_random_building_variation(&mut self) {
        let variation_count = self.find_tile().variation_count();
        if variation_count > 1 {
            let rand_variation_index = self.query.rng.random_range(0..variation_count);
            self.find_tile_mut().set_variation_index(rand_variation_index);
        }
    }

    #[inline]
    fn has_access_to_service(&self, service_kind: BuildingKind) -> bool {
        let config = self.configs.find::<service::ServiceConfig>(service_kind);
        self.query.is_near_building(self.map_cell, service_kind, config.effect_radius)
    }
}

impl<'config, 'query, 'sim, 'tile_map, 'tile_sets> fmt::Display for BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Building '{}' ({:?}|{:?}) [{},{}]",
               self.name,
               self.archetype_kind,
               self.kind,
               self.map_cell.x,
               self.map_cell.y)
    }
}
