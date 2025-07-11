use slab::Slab;
use rand::Rng;
use bitflags::{bitflags, Flags};
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    utils::{
        hash::StringHash,
        coords::CellRange
    },
    tile::{
        sets::{TileDef, TileKind, OBJECTS_BUILDINGS_CATEGORY},
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
    map_cells: CellRange,
    configs: &'config BuildingConfigs,
    archetype: BuildingArchetype<'config>,
}

impl<'config> Building<'config> {
    pub fn new(name: &'config str,
               kind: BuildingKind,
               map_cells: CellRange,
               configs: &'config BuildingConfigs,
               archetype: BuildingArchetype<'config>) -> Self {
        Self {
            name: name,
            kind: kind,
            map_cells: map_cells,
            configs: configs,
            archetype: archetype
        }
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: f32) {
        let mut update_ctx = 
            BuildingUpdateContext::new(self.name,
                                       self.kind,
                                       self.archetype_kind(),
                                       self.map_cells,
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

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if ui.collapsing_header("Properties##_building_properties", imgui::TreeNodeFlags::empty()) {
            ui.text(format!("Name............: '{}'", self.name));
            ui.text(format!("Kind............: {}", self.kind));
            ui.text(format!("Archetype.......: {}", self.archetype_kind()));
            ui.text(format!("Cells...........: [{},{}; {},{}]",
                self.map_cells.start.x,
                self.map_cells.start.y,
                self.map_cells.end.x,
                self.map_cells.end.y));
        }

        self.archetype.draw_debug_ui(ui_sys);
    }
}

// ----------------------------------------------
// BuildingList
// ----------------------------------------------

pub struct BuildingList<'config> {
    archetype_kind: BuildingArchetypeKind,
    buildings: Slab<Building<'config>>, // All share the same archetype.
}

impl<'config> BuildingList<'config> {
    pub fn new(archetype_kind: BuildingArchetypeKind) -> Self {
        Self {
            archetype_kind: archetype_kind,
            buildings: Slab::new(),
        }
    }

    pub fn clear(&mut self) {
        self.buildings.clear();
    }

    #[inline]
    pub fn try_get(&self, index: usize, archetype_kind: BuildingArchetypeKind) -> Option<&Building<'config>> {
        if archetype_kind != self.archetype_kind {
            return None;
        }
        self.buildings.get(index)
    }

    #[inline]
    pub fn try_get_mut(&mut self, index: usize, archetype_kind: BuildingArchetypeKind) -> Option<&mut Building<'config>> {
        if archetype_kind != self.archetype_kind {
            return None;
        }
        self.buildings.get_mut(index)
    }

    #[inline]
    pub fn add(&mut self, building: Building<'config>) -> usize {
        debug_assert!(building.archetype_kind() == self.archetype_kind);
        self.buildings.insert(building)
    }

    #[inline]
    pub fn remove(&mut self, index: usize, archetype_kind: BuildingArchetypeKind) -> bool {
        if archetype_kind != self.archetype_kind {
            return false;
        }
        if self.buildings.try_remove(index).is_none() {
            return false;
        }
        true
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: f32) {
        for (_, building) in &mut self.buildings {
            building.update(query, delta_time_secs);
        }
    }
}

// ----------------------------------------------
// BuildingKind
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct BuildingKind: u32 {
        // Archetype: House
        const House       = 1 << 0;

        // Archetype: Producer
        const Farm        = 1 << 1;

        // Archetype: Storage
        const Granary     = 1 << 2;
        const StorageYard = 1 << 3;

        // Archetype: Service
        const WellSmall   = 1 << 4;
        const WellBig     = 1 << 5;
        const Market      = 1 << 6;
    }
}

impl BuildingKind {
    #[inline] pub const fn count() -> usize { Self::FLAGS.len() }

    #[inline] pub const fn producer_count() -> usize { Self::producers().bits().count_ones() as usize }
    #[inline] pub const fn producers() -> BuildingKind {
        BuildingKind::from_bits_retain(
            Self::Farm.bits()
        )
    }

    #[inline] pub const fn storage_count() -> usize { Self::storage().bits().count_ones() as usize }
    #[inline] pub const fn storage() -> BuildingKind {
        BuildingKind::from_bits_retain(
            Self::Granary.bits() |
            Self::StorageYard.bits()
        )
    }

    #[inline] pub const fn services_count() -> usize { Self::services().bits().count_ones() as usize }
    #[inline] pub const fn services() -> BuildingKind {
        BuildingKind::from_bits_retain(
            Self::WellSmall.bits() |
            Self::WellBig.bits() |
            Self::Market.bits()
        )
    }

    #[inline]
    pub fn from_game_state_handle(handle: GameStateHandle) -> Self {
        Self::from_bits(handle.kind())
            .expect("GameStateHandle does not contain a valid BuildingKind enum value!")
    }

    #[inline]
    pub fn archetype_kind(self) -> BuildingArchetypeKind {
        if self.intersects(Self::producers()) {
            BuildingArchetypeKind::Producer
        } else if self.intersects(Self::storage()) {
            BuildingArchetypeKind::Storage
        } else if self.intersects(Self::services()) {
            BuildingArchetypeKind::Service
        } else if self.intersects(Self::House) {
            BuildingArchetypeKind::House
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
    Producer(producer::ProducerBuilding<'config>),
    Storage(storage::StorageBuilding<'config>),
    Service(service::ServiceBuilding<'config>),
    House(house::HouseBuilding<'config>),
}

pub const BUILDING_ARCHETYPE_COUNT: usize = BuildingArchetypeKind::COUNT;

impl<'config> BuildingArchetype<'config> {
    fn new_producer(state: producer::ProducerBuilding<'config>) -> Self {
        Self::Producer(state)
    }

    fn new_storage(state: storage::StorageBuilding<'config>) -> Self {
        Self::Storage(state)
    }

    fn new_service(state: service::ServiceBuilding<'config>) -> Self {
        Self::Service(state)
    }

    fn new_house(state: house::HouseBuilding<'config>) -> Self {
        Self::House(state)
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

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
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

    #[inline]
    fn as_producer_mut(&mut self) -> &mut producer::ProducerBuilding<'config> {
        match self {
            BuildingArchetype::Producer(state) => state,
            _ => panic!("Building archetype is not Producer!")
        }
    }

    #[inline]
    fn as_storage_mut(&mut self) -> &mut storage::StorageBuilding<'config> {
        match self {
            BuildingArchetype::Storage(state) => state,
            _ => panic!("Building archetype is not Storage!")
        }
    }

    #[inline]
    fn as_service_mut(&mut self) -> &mut service::ServiceBuilding<'config> {
        match self {
            BuildingArchetype::Service(state) => state,
            _ => panic!("Building archetype is not Service!")
        }
    }

    #[inline]
    fn as_house_mut(&mut self) -> &mut house::HouseBuilding<'config> {
        match self {
            BuildingArchetype::House(state) => state,
            _ => panic!("Building archetype is not House!")
        }
    }

}

// ----------------------------------------------
// BuildingBehavior
// ----------------------------------------------

// Common behavior for all Building archetypes.
pub trait BuildingBehavior<'config> {
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32);
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem);
}

// ----------------------------------------------
// BuildingUpdateContext
// ----------------------------------------------

pub struct BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    name: &'config str,
    kind: BuildingKind,
    archetype_kind: BuildingArchetypeKind,
    map_cells: CellRange,
    configs: &'config BuildingConfigs,
    query: &'query mut Query<'config, 'sim, 'tile_map, 'tile_sets>,
}

impl<'config, 'query, 'sim, 'tile_map, 'tile_sets> BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn new(name: &'config str,
           kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           map_cells: CellRange,
           configs: &'config BuildingConfigs,
           query: &'query mut Query<'config, 'sim, 'tile_map, 'tile_sets>) -> Self {
        Self {
            name: name,
            kind: kind,
            archetype_kind: archetype_kind,
            map_cells: map_cells,
            configs: configs,
            query: query
        }
    }

    #[inline]
    fn find_tile_def(&self, tile_def_name_hash: StringHash) -> Option<&'tile_sets TileDef> {
        self.query.find_tile_def(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.hash, tile_def_name_hash)
    }

    #[inline]
    fn find_tile(&self) -> &Tile<'tile_sets> {
        self.query.find_tile(self.map_cells.start, TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn find_tile_mut(&mut self) -> &mut Tile<'tile_sets> {
        self.query.find_tile_mut(self.map_cells.start, TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
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
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::Service);
        let config = self.configs.find::<service::ServiceConfig>(service_kind);
        self.query.is_near_building(self.map_cells, service_kind, config.effect_radius)
    }

    fn find_nearest_service(&mut self, service_kind: BuildingKind) -> Option<&mut service::ServiceBuilding<'config>> {
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::Service);
        let config = self.configs.find::<service::ServiceConfig>(service_kind);

        if let Some(building) =
            self.query.find_nearest_building(self.map_cells, service_kind, config.effect_radius) {

            if building.archetype_kind() != BuildingArchetypeKind::Service {
                panic!("Building '{}' ({}|{}): Expected archetype to be Service!",
                       building.name, building.archetype_kind(), building.kind());
            }

            return Some(building.archetype.as_service_mut());
        }

        None
    }
}

impl<'config, 'query, 'sim, 'tile_map, 'tile_sets> std::fmt::Display for BuildingUpdateContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Building '{}' ({}|{}) [{},{}]",
               self.name,
               self.archetype_kind,
               self.kind,
               self.map_cells.start.x,
               self.map_cells.start.y)
    }
}
