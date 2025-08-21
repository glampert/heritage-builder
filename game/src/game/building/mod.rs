#![allow(clippy::enum_variant_names)]

use paste::paste;
use bitflags::{bitflags, Flags};
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};
use enum_dispatch::enum_dispatch;
use proc_macros::DrawDebugUi;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    pathfind::{self},
    utils::{
        Color,
        UnsafeMutable,
        hash::StringHash,
        coords::{Cell, CellRange, WorldToScreenTransform}
    },
    tile::{
        Tile,
        TileKind,
        TileFlags,
        TileMap,
        TileMapLayerKind,
        TileGameStateHandle,
        sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY}
    }
};

use super::{
    unit::{
        Unit,
        patrol::Patrol,
        runner::Runner,
    },
    sim::{
        Query,
        world::BuildingId,
        resources::{
            ServiceKind,
            StockItem,
            ResourceKind,
            ResourceKinds,
            ResourceStock,
            RESOURCE_KIND_COUNT
        }
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
        const Factory     = 1 << 2;

        // Archetype: Storage
        const Granary     = 1 << 3;
        const StorageYard = 1 << 4;

        // Archetype: Service
        const WellSmall   = 1 << 5;
        const WellBig     = 1 << 6;
        const Market      = 1 << 7;
    }
}

impl BuildingKind {
    #[inline]
    pub const fn is_single_building(self) -> bool {
        self.bits().count_ones() == 1
    }

    #[inline] pub const fn count() -> usize { Self::FLAGS.len() }

    #[inline] pub const fn producer_count() -> usize { Self::producers().bits().count_ones() as usize }
    #[inline] pub const fn producers() -> Self {
        Self::from_bits_retain(
            Self::Farm.bits() |
            Self::Factory.bits()
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
    road_link: UnsafeMutable<Cell>,
    id: BuildingId,
    archetype: BuildingArchetype<'config>,
}

impl<'config> Building<'config> {
    // ----------------------
    // Creation/Placement:
    // ----------------------

    pub fn new(kind: BuildingKind,
               map_cells: CellRange,
               archetype: BuildingArchetype<'config>) -> Self {
        Self {
            kind,
            map_cells,
            // Road link cached on first access and refreshed every building update.
            road_link: UnsafeMutable::new(Cell::invalid()),
            // Id is set after construction by Building::placed().
            id: BuildingId::default(),
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
    pub fn removed(&mut self, tile_map: &mut TileMap) {
        debug_assert!(self.id.is_valid()); // Should be placed.

        self.id = BuildingId::default();
        self.map_cells = CellRange::default();

        self.clear_road_link(tile_map);
    }

    // ----------------------
    // Utilities:
    // ----------------------

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

    // ----------------------
    // World Update:
    // ----------------------

    #[inline]
    pub fn update(&mut self, query: &Query<'config, '_>) {
        debug_assert!(self.id.is_valid());

        // Refresh cached road link cell:
        self.update_road_link(query);

        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.road_link(query),
                                 self.id,
                                 query);

        self.archetype.update(&context);
    }

    #[inline]
    pub fn visited_by(&mut self, unit: &mut Unit, query: &Query<'config, '_>) {
        debug_assert!(self.id.is_valid());

        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.road_link(query),
                                 self.id,
                                 query);

        self.archetype.visited_by(unit, &context);
    }

    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        debug_assert!(self.id.is_valid());
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

    // ----------------------
    // Building Resources:
    // ----------------------

    #[inline]
    pub fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.archetype.available_resources(kind)
    }

    #[inline]
    pub fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.archetype.receivable_resources(kind)
    }

    #[inline]
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.archetype.receive_resources(kind, count)
    }

    #[inline]
    pub fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.archetype.remove_resources(kind, count)
    }

    // ----------------------
    // Patrol/Runner Units:
    // ----------------------

    #[inline]
    pub fn active_patrol(&mut self) -> Option<&mut Patrol> {
        self.archetype.active_patrol()
    }

    #[inline]
    pub fn active_runner(&mut self) -> Option<&mut Runner> {
        self.archetype.active_runner()
    }

    // ----------------------
    // Building Road Link:
    // ----------------------

    #[inline]
    pub fn is_linked_to_road(&self, query: &Query) -> bool {
        self.road_link(query).is_some()
    }

    #[inline]
    pub fn road_link(&self, query: &Query) -> Option<Cell> {
        if self.road_link.is_valid() {
            return Some(*self.road_link.as_ref());
        }

        // Lazily cache the road link cell on demand:
        if let Some(road_link) = query.find_nearest_road_link(self.map_cells) {
            // Cache road link cell:
            debug_assert!(road_link.is_valid());
            *self.road_link.as_mut() = road_link;

            // Set underlying tile flag:
            if let Some(road_link_tile) = Self::find_road_link_tile_for_cell(query, road_link) {
                road_link_tile.set_flags(TileFlags::BuildingRoadLink, true);
            }

            return Some(road_link);
        }

        None
    }

    pub fn is_showing_road_link_debug(&self, query: &Query) -> bool {
        if let Some(road_link_tile) = self.find_road_link_tile(query) {
            return road_link_tile.has_flags(TileFlags::DrawDebugBounds);
        }
        false
    }

    pub fn set_show_road_link_debug(&self, query: &Query, show: bool) {
        if let Some(road_link_tile) = self.find_road_link_tile(query) {
            road_link_tile.set_flags(TileFlags::DrawDebugBounds, show);
        }
    }

    pub fn find_road_link_tile<'a>(&self, query: &'a Query) -> Option<&'a mut Tile<'a>> {
        if let Some(road_link) = self.road_link(query) {
            return Self::find_road_link_tile_for_cell(query, road_link);
        }
        None
    }

    fn find_road_link_tile_for_cell<'a>(query: &'a Query, road_link: Cell) -> Option<&'a mut Tile<'a>> {
        query.find_tile_mut(road_link, TileMapLayerKind::Terrain, TileKind::Terrain)
    }

    fn update_road_link(&mut self, query: &Query) {
        if let Some(new_road_link) = query.find_nearest_road_link(self.map_cells) {
            debug_assert!(new_road_link.is_valid());

            if new_road_link != *self.road_link.as_ref() && self.road_link.is_valid() {
                // Clear previous underlying tile flag:
                if let Some(prev_road_link_tile) = Self::find_road_link_tile_for_cell(query, *self.road_link.as_ref()) {
                    prev_road_link_tile.set_flags(TileFlags::BuildingRoadLink, false);
                }
            }

            // Set new underlying tile flag:
            if let Some(new_road_link_tile) =  Self::find_road_link_tile_for_cell(query, new_road_link) {
                new_road_link_tile.set_flags(TileFlags::BuildingRoadLink, true);
            }

            *self.road_link.as_mut() = new_road_link;
        } else {
            // Building not connected to a road.
            *self.road_link.as_mut() = Cell::invalid();
        }
    }

    fn clear_road_link(&mut self, tile_map: &mut TileMap) {
        let road_link = self.road_link.as_mut();
        if road_link.is_valid() {
            if let Some(road_link_tile) = tile_map.try_tile_from_layer_mut(*road_link, TileMapLayerKind::Terrain) {
                road_link_tile.set_flags(TileFlags::BuildingRoadLink, false);
            }
        }
        *road_link = Cell::invalid();
    }

    // ----------------------
    // Building Debug:
    // ----------------------

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
                road_link: Cell,
                id: BuildingId,
            }
            let debug_vars = DrawDebugUiVariables {
                name: self.name(),
                kind: self.kind,
                archetype: self.archetype_kind(),
                cells: self.map_cells,
                road_link: self.road_link(query).unwrap_or_default(),
                id: self.id,
            };

            debug_vars.draw_debug_ui(ui_sys);
            ui.separator();

            if ui.button("Highlight Access Tiles") {
                pathfind::highlight_building_access_tiles(query.tile_map(), self.map_cells);
            }

            let mut show_road_link = self.is_showing_road_link_debug(query);
            if ui.checkbox("Show Road Link", &mut show_road_link) {
                self.set_show_road_link_debug(query, show_road_link);
            }

            if self.is_linked_to_road(query) {
                ui.text_colored(Color::green().to_array(), "Has road access.");
            } else {
                ui.text_colored(Color::red().to_array(), "No road access!");
            }
        }

        let context =
            BuildingContext::new(self.kind,
                                 self.archetype_kind(),
                                 self.map_cells,
                                 self.road_link(query),
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
                                 self.road_link(query),
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
// Building Archetypes  
// ----------------------------------------------
/*
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
#[enum_dispatch]
#[derive(EnumDiscriminants)]
#[strum_discriminants(repr(u32), name(BuildingArchetypeKind), derive(Display, EnumCount, EnumIter))]
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
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str;

    fn update(&mut self, context: &BuildingContext<'config, '_, '_>);

    fn visited_by(&mut self,
                  unit: &mut Unit,
                  context: &BuildingContext<'config, '_, '_>);

    // ----------------------
    // Resources/Stock:
    // ----------------------

    // How many resources of this kind do we currently hold?
    fn available_resources(&self, kind: ResourceKind) -> u32;

    // How many resources of this kind can we receive?
    fn receivable_resources(&self, kind: ResourceKind) -> u32;

    // Receive resources. Returns number of resources it was able
    // to accommodate, which can be less or equal to `count`.
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32;

    // Tries to relinquish up to `count` resources. Returns the number of
    // resources it was able to relinquish, which can be less or equal to `count`.
    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32;

    // ----------------------
    // Patrol/Runner Units:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> { None }
    fn active_runner(&mut self) -> Option<&mut Runner> { None }

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
        self.road_link.is_valid() && self.base_cell.is_valid()
    }
}

// ----------------------------------------------
// BuildingContext
// ----------------------------------------------

pub struct BuildingContext<'config, 'tile_sets, 'query> {
    kind: BuildingKind,
    archetype_kind: BuildingArchetypeKind,
    map_cells: CellRange,
    road_link: Option<Cell>,
    id: BuildingId,
    pub query: &'query Query<'config, 'tile_sets>,
}

impl<'config, 'tile_sets, 'query> BuildingContext<'config, 'tile_sets, 'query> {
    fn new(kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           map_cells: CellRange,
           road_link: Option<Cell>,
           id: BuildingId,
           query: &'query Query<'config, 'tile_sets>) -> Self {
        Self {
            kind,
            archetype_kind,
            map_cells,
            road_link,
            id,
            query,
        }
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.map_cells.start
    }

    #[inline]
    pub fn kind_and_id(&self) -> BuildingKindAndId {
        BuildingKindAndId {
            kind: self.kind,
            id: self.id,
        }
    }

    #[inline]
    pub fn tile_info(&self) -> BuildingTileInfo {
        BuildingTileInfo {
            road_link: self.road_link.unwrap_or_default(), // We may or may not be connected to a road.
            base_cell: self.base_cell(),
        }
    }

    #[inline]
    pub fn is_linked_to_road(&self) -> bool {
        self.road_link.is_some()
    }

    #[inline]
    pub fn debug_name(&self) -> &str {
        if cfg!(debug_assertions) {
            if let Some(building) = self.query.world().find_building(self.kind, self.id) {
                return building.name();
            }
        }
        "<unavailable>"
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
    fn has_access_to_service(&self, service_kind: ServiceKind) -> bool {
        debug_assert!(service_kind.is_single_building());
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::ServiceBuilding);

        if let Some(road_link) = self.road_link {
            let config = self.query.building_configs().find_service_config(service_kind);
            return self.query.is_near_building(road_link,
                                               service_kind,
                                               config.requires_road_access,
                                               config.effect_radius);
        }

        false
    }

    #[inline]
    fn set_random_building_variation(&self) {
        let tile = self.find_tile_mut();
        let variation_count = tile.variation_count();
        if variation_count > 1 {
            let rand_variation_index = self.query.random_range(0..variation_count);
            tile.set_variation_index(rand_variation_index);
        }
    }
}

// ----------------------------------------------
// BuildingStock
// ----------------------------------------------

pub struct BuildingStock {
    resources: ResourceStock,
    capacities: [u8; RESOURCE_KIND_COUNT],
}

impl BuildingStock {
    pub fn with_accepted_list_and_capacity(accepted_resources: &ResourceKinds, capacity: u32) -> Self {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        Self {
            resources: ResourceStock::with_accepted_list(accepted_resources),
            capacities: [capacity_u8; RESOURCE_KIND_COUNT],
        }
    }

    pub fn with_accepted_kinds_and_capacity(accepted_kinds: ResourceKind, capacity: u32) -> Self {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        Self {
            resources: ResourceStock::with_accepted_kinds(accepted_kinds),
            capacities: [capacity_u8; RESOURCE_KIND_COUNT],
        }
    }

    pub fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.resources.count(kind)
    }

    pub fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        if let Some((index, item)) = self.resources.find(kind) {
            debug_assert!(item.count <= self.capacity_at(index), "{item}");
            return self.capacity_at(index) - item.count;
        }
        0
    }

    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        if count != 0 {
            let capacity_left = self.receivable_resources(kind);
            if capacity_left != 0 {
                let add_count = count.min(capacity_left);
                self.resources.add(kind, add_count);
                return add_count;
            }
        }
        0
    }

    pub fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        if count != 0 {
            let available_count = self.available_resources(kind);
            if available_count != 0 {
                let remove_count = count.min(available_count);
                if self.resources.remove(kind, remove_count).is_some() {
                    return remove_count;
                }
            }
        }
        0
    }

    pub fn update_capacities(&mut self, capacity: u32) {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        self.capacities = [capacity_u8; RESOURCE_KIND_COUNT];

        // Clamp any existing resources to the new capacity.
        self.resources.for_each_mut(|index, item| {
            item.count = item.count.min(self.capacities[index] as u32);
        });
    }

    #[inline]
    pub fn has_any_of(&self, kinds: ResourceKind) -> bool {
        self.resources.has(kinds)
    }

    #[inline]
    pub fn capacity_for(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        if let Some((index, _)) = self.resources.find(kind) {
            return self.capacity_at(index);
        }
        0
    }

    #[inline]
    pub fn capacity_at(&self, index: usize) -> u32 {
        self.capacities[index] as u32
    }

    #[inline]
    pub fn accepts_any(&self) -> bool {
        self.resources.accepts_any()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.resources.clear();
    }

    #[inline]
    pub fn fill(&mut self) {
        self.resources.for_each_mut(|index, item| {
            item.count = self.capacities[index] as u32;
        });
    }

    #[inline]
    pub fn for_each<F>(&self, visitor_fn: F)
        where F: FnMut(usize, &StockItem)
    {
        self.resources.for_each(visitor_fn);
    }

    pub fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        self.resources.for_each_mut(|index, item| {
            let item_label = format!("{}##_stock_item_{}", item.kind, index);
            let item_capacity = self.capacities[index] as u32;

            if ui.input_scalar(item_label, &mut item.count).step(1).build() {
                item.count = item.count.min(item_capacity);
            }

            let capacity_left = item_capacity - item.count;
            let is_full = item.count >= item_capacity;

            ui.same_line();
            if is_full {
                ui.text_colored(Color::red().to_array(), "(full)");
            } else {
                ui.text(format!("({} left)", capacity_left));
            }
        });
    }
}
