use paste::paste;
use bitflags::{bitflags, Flags};
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};
use enum_dispatch::enum_dispatch;
use proc_macros::DrawDebugUi;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    utils::{
        hash::StringHash,
        coords::{Cell, CellRange, WorldToScreenTransform}
    },
    tile::{
        Tile,
        TileKind,
        TileMap,
        TileMapLayerKind,
        TileGameStateHandle,
        sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY}
    }
};

use super::{
    unit::Unit,
    sim::{
        Query,
        world::BuildingId,
        resources::ResourceKind
    }
};

use producer::ProducerBuilding;
use storage::StorageBuilding;
use service::ServiceBuilding;
use house::HouseBuilding;

pub mod config;
mod producer;
mod storage;
mod service;
mod house;

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
// Helper Macros
// ----------------------------------------------

macro_rules! building_type_casts {
    ($derived_mod:ident, $derived_struct:ident) => {
        paste! {
            #[inline]
            pub fn [<as_ $derived_mod>](&self) -> &$derived_struct<'config> {
                match &self.archetype {
                    BuildingArchetype::$derived_struct(state) => state,
                    _ => panic!("Building archetype is not {}!", stringify!($derived_struct))
                }
            }
            #[inline]
            pub fn [<as_ $derived_mod _mut>](&mut self) -> &mut $derived_struct<'config> {
                match &mut self.archetype {
                    BuildingArchetype::$derived_struct(state) => state,
                    _ => panic!("Building archetype is not {}!", stringify!($derived_struct))
                }
            }
        }
    };
}

// ----------------------------------------------
// Building
// ----------------------------------------------

pub struct Building<'config> {
    kind: BuildingKind,
    map_cells: CellRange,
    id: BuildingId,
    archetype: BuildingArchetype<'config>,
}

impl<'config> Building<'config> {
    pub fn new(kind: BuildingKind,
               map_cells: CellRange,
               archetype: BuildingArchetype<'config>) -> Self {
        Self {
            kind,
            map_cells,
            id: BuildingId::default(), // Set after construction by Building::placed().
            archetype,
        }
    }

    #[inline]
    pub fn placed(&mut self, id: BuildingId) {
        debug_assert!(id.is_valid());
        debug_assert!(!self.id.is_valid());
        self.id = id;
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.archetype.name()
    }

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        self.map_cells
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.map_cells.start
    }

    #[inline]
    pub fn is(&self, kinds: BuildingKind) -> bool {
        self.kind.intersects(kinds)
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
    pub fn id(&self) -> BuildingId {
        self.id
    }

    building_type_casts! { producer, ProducerBuilding } // as_producer()
    building_type_casts! { storage,  StorageBuilding  } // as_storage()
    building_type_casts! { service,  ServiceBuilding  } // as_service()
    building_type_casts! { house,    HouseBuilding    } // as_house()

    #[inline]
    pub fn update(&mut self, query: &Query<'config, '_>) {
        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.id,
                                 query);

        self.archetype.update(&context);
    }

    #[inline]
    pub fn visited_by(&mut self, unit: &mut Unit, query: &Query<'config, '_>) {
        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.id,
                                 query);

        self.archetype.visited_by(unit, &context);
    }

    #[inline]
    pub fn receivable_amount(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        self.archetype.receivable_amount(kind)
    }

    #[inline]
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        self.archetype.receive_resources(kind, count)
    }

    #[inline]
    pub fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        self.archetype.give_resources(kind, count)
    }

    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        if tile_map.try_move_tile(self.base_cell(), destination_cell, TileMapLayerKind::Objects) {
            let tile = tile_map.find_tile_mut(
                destination_cell,
                TileMapLayerKind::Objects,
                TileKind::Building)
                .unwrap();

            debug_assert!(destination_cell == tile.base_cell());
            self.map_cells = tile.cell_range();
            return true;
        }
        false
    }

    pub fn draw_debug_ui(&mut self, query: &Query<'config, '_>, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if ui.collapsing_header("Properties##_building_properties", imgui::TreeNodeFlags::empty()) {
            #[derive(DrawDebugUi)]
            struct DrawDebugUiVariables<'a> {
                name: &'a str,
                kind: BuildingKind,
                archetype: BuildingArchetypeKind,
                cells: CellRange,
                id: BuildingId,
            }
            let debug_vars = DrawDebugUiVariables {
                name: self.name(),
                kind: self.kind,
                archetype: self.archetype_kind(),
                cells: self.map_cells,
                id: self.id,
            };
            debug_vars.draw_debug_ui(ui_sys);
        }

        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.id,
                                 query);

        self.archetype.draw_debug_ui(&context, ui_sys);
    }

    pub fn draw_debug_popups(&mut self,
                             query: &Query<'config, '_>,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange) {

        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.id,
                                 query);

        self.archetype.draw_debug_popups(
            &context,
            ui_sys,
            transform,
            visible_range);
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
    pub fn from_game_state_handle(handle: TileGameStateHandle) -> Self {
        Self::from_bits(handle.kind())
            .expect("TileGameStateHandle does not contain a valid BuildingKind enum value!")
    }

    #[inline]
    pub fn archetype_kind(self) -> BuildingArchetypeKind {
        if self.intersects(Self::producers()) {
            BuildingArchetypeKind::ProducerBuilding
        } else if self.intersects(Self::storage()) {
            BuildingArchetypeKind::StorageBuilding
        } else if self.intersects(Self::services()) {
            BuildingArchetypeKind::ServiceBuilding
        } else if self.intersects(Self::House) {
            BuildingArchetypeKind::HouseBuilding
        } else {
            panic!("Unknown archetype for building kind: {:?}", self);
        }
    }
}

// ----------------------------------------------
// BuildingArchetype / BuildingArchetypeKind
// ----------------------------------------------

#[enum_dispatch]
#[derive(EnumDiscriminants)]
#[strum_discriminants(repr(u32), name(BuildingArchetypeKind), derive(Display, EnumCount, EnumIter))]
#[allow(clippy::enum_variant_names)]
pub enum BuildingArchetype<'config> {
    ProducerBuilding(ProducerBuilding<'config>),
    StorageBuilding(StorageBuilding<'config>),
    ServiceBuilding(ServiceBuilding<'config>),
    HouseBuilding(HouseBuilding<'config>),
}

pub const BUILDING_ARCHETYPE_COUNT: usize = BuildingArchetypeKind::COUNT;

// ----------------------------------------------
// BuildingBehavior
// ----------------------------------------------

// Common behavior for all Building archetypes.
#[enum_dispatch(BuildingArchetype)]
pub trait BuildingBehavior<'config> {
    fn name(&self) -> &str;

    fn update(&mut self, context: &BuildingContext<'config, '_, '_>);

    fn visited_by(&mut self,
                  unit: &mut Unit,
                  context: &BuildingContext<'config, '_, '_>);

    // ----------------------
    // Resources / Stock:
    // ----------------------

    // How many resources of this kind can we receive currently?
    fn receivable_amount(&self, kind: ResourceKind) -> u32;

    // Returns number of resources it was able to accommodate,
    // which can be less or equal to `count`.
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32;

    // Tries to gives away up to `count` resources. Returns the number
    // of resources it was able to give, which can be less or equal to `count`.
    fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32;

    // ----------------------
    // Debug:
    // ----------------------

    fn draw_debug_ui(&mut self,
                     context: &BuildingContext<'config, '_, '_>,
                     ui_sys: &UiSystem);

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext<'config, '_, '_>,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange);
}

// ----------------------------------------------
// BuildingKindAndId / BuildingTileInfo
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct BuildingKindAndId {
    pub kind: BuildingKind,
    pub id: BuildingId,
}

#[derive(Copy, Clone)]
pub struct BuildingTileInfo {
    pub kind: BuildingKind,
    pub road_link: Cell,
    pub base_cell: Cell,
}

impl BuildingKindAndId {
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.kind.is_empty() && self.id.is_valid()
    }
}

impl BuildingTileInfo {
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.kind.is_empty() && self.road_link.is_valid() && self.base_cell.is_valid()
    }
}

// ----------------------------------------------
// BuildingContext
// ----------------------------------------------

pub struct BuildingContext<'config, 'tile_sets, 'query> {
    kind: BuildingKind,
    archetype_kind: BuildingArchetypeKind,
    map_cells: CellRange,
    id: BuildingId,
    query: &'query Query<'config, 'tile_sets>,
}

impl<'config, 'tile_sets, 'query> BuildingContext<'config, 'tile_sets, 'query> {
    fn new(kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           map_cells: CellRange,
           id: BuildingId,
           query: &'query Query<'config, 'tile_sets>) -> Self {
        Self {
            kind,
            archetype_kind,
            map_cells,
            id,
            query,
        }
    }

    #[inline]
    fn base_cell(&self) -> Cell {
        self.map_cells.start
    }

    #[inline]
    fn kind_and_id(&self) -> BuildingKindAndId {
        BuildingKindAndId {
            kind: self.kind,
            id: self.id,
        }
    }

    #[inline]
    fn tile_info(&self) -> BuildingTileInfo {
        BuildingTileInfo {
            kind: self.kind,
            road_link: self.find_nearest_road_link().unwrap_or_default(),
            base_cell: self.base_cell(),
        }
    }

    #[inline]
    fn find_tile_def(&self, tile_def_name_hash: StringHash) -> Option<&'tile_sets TileDef> {
        self.query.find_tile_def(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.hash, tile_def_name_hash)
    }

    #[inline]
    fn find_tile(&self) -> &Tile<'tile_sets> {
        self.query.find_tile(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn find_tile_mut(&self) -> &mut Tile<'tile_sets> {
        self.query.find_tile_mut(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn set_random_building_variation(&self) {
        let tile = self.find_tile_mut();
        let variation_count = tile.variation_count();
        if variation_count > 1 {
            let rand_variation_index = self.query.random_in_range(0..variation_count);
            tile.set_variation_index(rand_variation_index);
        }
    }

    #[inline]
    fn find_nearest_road_link(&self) -> Option<Cell> {
        self.query.find_nearest_road_link(self.map_cells)
    }

    #[inline]
    fn has_access_to_service(&self, service_kind: BuildingKind) -> bool {
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::ServiceBuilding);
        let config = self.query.building_configs().find_service_config(service_kind);
        self.query.is_near_building(self.map_cells, service_kind, config.effect_radius)
    }

    // `storage_kinds` can be a combination of ORed BuildingKind flags.
    fn for_each_storage<F>(&self, storage_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&Building<'config>) -> bool
    {
        debug_assert!(storage_kinds.archetype_kind() == BuildingArchetypeKind::StorageBuilding);

        let world = self.query.world();
        let storage_buildings = world.buildings_list(BuildingArchetypeKind::StorageBuilding);

        for building in storage_buildings.iter() {
            if building.kind().intersects(storage_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    // TODO: Get rid of mutable access to building here if possible!

    fn for_each_storage_mut<F>(&self, storage_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&mut Building<'config>) -> bool
    {
        debug_assert!(storage_kinds.archetype_kind() == BuildingArchetypeKind::StorageBuilding);

        let world = self.query.world();
        let storage_buildings = world.buildings_list_mut(BuildingArchetypeKind::StorageBuilding);

        for building in storage_buildings.iter_mut() {
            if building.kind().intersects(storage_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    fn find_nearest_service_mut(&self, service_kind: BuildingKind) -> Option<&mut Building<'config>> {
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::ServiceBuilding);
        let config = self.query.building_configs().find_service_config(service_kind);

        if let Some(building) =
            self.query.find_nearest_building_mut(self.map_cells, service_kind, config.effect_radius) {
            if building.archetype_kind() != BuildingArchetypeKind::ServiceBuilding || building.kind() != service_kind {
                panic!("Building '{}' ({}|{}): Expected archetype to be Service ({service_kind})!",
                       building.name(), building.archetype_kind(), building.kind());
            }
            return Some(building);
        }
        None
    }
}
