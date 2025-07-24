use slab::Slab;
use rand::Rng;
use bitflags::{bitflags, Flags};
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};
use proc_macros::DrawDebugUi;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    utils::{
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
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
            name,
            kind,
            map_cells,
            configs,
            archetype,
        }
    }

    #[inline]
    pub fn kind(&self) -> BuildingKind {
        self.kind
    }

    #[inline]
    pub fn archetype_kind(&self) -> BuildingArchetypeKind {
        self.archetype.discriminant()
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: Seconds) {
        let mut context =
            BuildingContext::new(self.name,
                                 self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.configs,
                                 query);

        self.archetype.update(&mut context, delta_time_secs);
    }

    pub fn set_cell_range(&mut self, new_map_cells: CellRange) {
        debug_assert!(new_map_cells.is_valid());
        self.map_cells = new_map_cells;
    }

    pub fn draw_debug_ui(&mut self, query: &mut Query<'config, '_, '_, '_>, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if ui.collapsing_header("Properties##_building_properties", imgui::TreeNodeFlags::empty()) {
            #[derive(DrawDebugUi)]
            struct DrawDebugUiVariables<'a> {
                name: &'a str,
                kind: BuildingKind,
                archetype: BuildingArchetypeKind,
                cells: CellRange,
            }
            let debug_vars = DrawDebugUiVariables {
                name: self.name,
                kind: self.kind,
                archetype: self.archetype_kind(),
                cells: self.map_cells,
            };
            debug_vars.draw_debug_ui(ui_sys);
        }

        let mut context =
            BuildingContext::new(self.name,
                                 self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.configs,
                                 query);

        self.archetype.draw_debug_ui(&mut context, ui_sys);
    }

    pub fn draw_debug_popups(&mut self,
                             query: &mut Query<'config, '_, '_, '_>,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange,
                             delta_time_secs: Seconds,
                             show_popup_messages: bool) {

        let context =
            BuildingContext::new(self.name,
                                 self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.configs,
                                 query);

        self.archetype.draw_debug_popups(
            &context,
            ui_sys,
            transform,
            visible_range,
            delta_time_secs,
            show_popup_messages);
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
    #[inline]
    pub fn new(archetype_kind: BuildingArchetypeKind) -> Self {
        Self {
            archetype_kind,
            buildings: Slab::new(),
        }
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
    pub fn try_get(&self, index: usize) -> Option<&Building<'config>> {
        self.buildings.get(index)
    }

    #[inline]
    pub fn try_get_mut(&mut self, index: usize) -> Option<&mut Building<'config>> {
        self.buildings.get_mut(index)
    }

    #[inline]
    pub fn add(&mut self, building: Building<'config>) -> usize {
        debug_assert!(building.archetype_kind() == self.archetype_kind);
        self.buildings.insert(building)
    }

    #[inline]
    pub fn remove(&mut self, index: usize) -> bool {
        if self.buildings.try_remove(index).is_none() {
            return false;
        }
        true
    }

    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(usize, &Building<'config>) -> bool
    {
        for (index, building) in &self.buildings {
            let should_continue = visitor_fn(index, building);
            if !should_continue {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(usize, &mut Building<'config>) -> bool
    {
        for (index, building) in &mut self.buildings {
            let should_continue = visitor_fn(index, building);
            if !should_continue {
                break;
            }
        }
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: Seconds) {
        for (_, building) in &mut self.buildings {
            debug_assert!(building.archetype_kind() == self.archetype_kind);
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
    #[inline] pub const fn producers() -> Self {
        Self::from_bits_retain(
            Self::Farm.bits()
        )
    }

    #[inline] pub const fn storage_count() -> usize { Self::storage().bits().count_ones() as usize }
    #[inline] pub const fn storage() -> Self {
        Self::from_bits_retain(
            Self::Granary.bits() |
            Self::StorageYard.bits()
        )
    }

    #[inline] pub const fn services_count() -> usize { Self::services().bits().count_ones() as usize }
    #[inline] pub const fn services() -> Self {
        Self::from_bits_retain(
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
    #[inline]
    fn new_producer(state: producer::ProducerBuilding<'config>) -> Self {
        Self::Producer(state)
    }

    #[inline]
    fn new_storage(state: storage::StorageBuilding<'config>) -> Self {
        Self::Storage(state)
    }

    #[inline]
    fn new_service(state: service::ServiceBuilding<'config>) -> Self {
        Self::Service(state)
    }

    #[inline]
    fn new_house(state: house::HouseBuilding<'config>) -> Self {
        Self::House(state)
    }

    #[inline]
    fn as_producer_mut(&mut self) -> &mut producer::ProducerBuilding<'config> {
        match self {
            Self::Producer(state) => state,
            _ => panic!("Building archetype is not Producer!")
        }
    }

    #[inline]
    fn as_storage_mut(&mut self) -> &mut storage::StorageBuilding<'config> {
        match self {
            Self::Storage(state) => state,
            _ => panic!("Building archetype is not Storage!")
        }
    }

    #[inline]
    fn as_service_mut(&mut self) -> &mut service::ServiceBuilding<'config> {
        match self {
            Self::Service(state) => state,
            _ => panic!("Building archetype is not Service!")
        }
    }

    #[inline]
    fn as_house_mut(&mut self) -> &mut house::HouseBuilding<'config> {
        match self {
            Self::House(state) => state,
            _ => panic!("Building archetype is not House!")
        }
    }

    #[inline]
    fn update(&mut self, context: &mut BuildingContext<'config, '_, '_, '_, '_>, delta_time_secs: Seconds) {
        match self {
            Self::Producer(state) => {
                state.update(context, delta_time_secs);
            }
            Self::Storage(state) => {
                state.update(context, delta_time_secs);
            }
            Self::Service(state) => {
                state.update(context, delta_time_secs);
            }
            Self::House(state) => {
                state.update(context, delta_time_secs);
            }
        }
    }

    fn draw_debug_ui(&mut self, context: &mut BuildingContext<'config, '_, '_, '_, '_>, ui_sys: &UiSystem) {
        match self {
            Self::Producer(state) => {
                state.draw_debug_ui(context, ui_sys);
            }
            Self::Storage(state) => {
                state.draw_debug_ui(context, ui_sys);
            }
            Self::Service(state) => {
                state.draw_debug_ui(context, ui_sys);
            }
            Self::House(state) => {
                state.draw_debug_ui(context, ui_sys);
            }
        }
    }

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext<'config, '_, '_, '_, '_>,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange,
                         delta_time_secs: Seconds,
                         show_popup_messages: bool) {
        match self {
            Self::Producer(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
            }
            Self::Storage(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
            }
            Self::Service(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
            }
            Self::House(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
            }
        }
    }
}

// ----------------------------------------------
// BuildingBehavior
// ----------------------------------------------

// Common behavior for all Building archetypes.
pub trait BuildingBehavior<'config> {
    fn update(&mut self,
              context: &mut BuildingContext<'config, '_, '_, '_, '_>,
              delta_time_secs: Seconds);

    fn draw_debug_ui(&mut self,
                     context: &mut BuildingContext<'config, '_, '_, '_, '_>,
                     ui_sys: &UiSystem);

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext<'config, '_, '_, '_, '_>,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange,
                         delta_time_secs: Seconds,
                         show_popup_messages: bool);
}

// ----------------------------------------------
// BuildingContext
// ----------------------------------------------

pub struct BuildingContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    name: &'config str,
    kind: BuildingKind,
    archetype_kind: BuildingArchetypeKind,
    map_cells: CellRange,
    configs: &'config BuildingConfigs,
    query: &'query mut Query<'config, 'sim, 'tile_map, 'tile_sets>,
}

impl<'config, 'query, 'sim, 'tile_map, 'tile_sets> BuildingContext<'config, 'query, 'sim, 'tile_map, 'tile_sets> {
    fn new(name: &'config str,
           kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           map_cells: CellRange,
           configs: &'config BuildingConfigs,
           query: &'query mut Query<'config, 'sim, 'tile_map, 'tile_sets>) -> Self {
        Self {
            name,
            kind,
            archetype_kind,
            map_cells,
            configs,
            query,
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
        let config = self.configs.find_service_config(service_kind);
        self.query.is_near_building(self.map_cells, service_kind, config.effect_radius)
    }

    fn find_nearest_service(&mut self, service_kind: BuildingKind) -> Option<&mut service::ServiceBuilding<'config>> {
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::Service);
        let config = self.configs.find_service_config(service_kind);

        if let Some(building) =
            self.query.find_nearest_building(self.map_cells, service_kind, config.effect_radius) {

            if building.archetype_kind() != BuildingArchetypeKind::Service || building.kind() != service_kind {
                panic!("Building '{}' ({}|{}): Expected archetype to be Service ({service_kind})!",
                       building.name, building.archetype_kind(), building.kind());
            }

            return Some(building.archetype.as_service_mut());
        }

        None
    }

    // `storage_kinds` can be a combination of ORed flags.
    fn for_each_storage<F>(&mut self, storage_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&mut storage::StorageBuilding<'config>) -> bool
    {
        debug_assert!(storage_kinds.archetype_kind() == BuildingArchetypeKind::Storage);

        let storage_buildings =
            self.query.world.buildings_list_mut(BuildingArchetypeKind::Storage);

        storage_buildings.for_each_mut(|_, building| {
            if building.kind().intersects(storage_kinds) {
                visitor_fn(building.archetype.as_storage_mut())
            } else {
                true // continue
            }
        });
    }
}

impl std::fmt::Display for BuildingContext<'_, '_, '_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Building '{}' ({}|{}) {}",
               self.name,
               self.archetype_kind,
               self.kind,
               self.map_cells.start)
    }
}
