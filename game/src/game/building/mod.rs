use bitflags::{bitflags, Flags};
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};
use proc_macros::DrawDebugUi;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    utils::{
        hash::StringHash,
        coords::{Cell, CellRange, WorldToScreenTransform}
    },
    tile::{
        sets::{TileDef, TileKind, OBJECTS_BUILDINGS_CATEGORY},
        map::{GameStateHandle, Tile, TileMap, TileMapLayerKind}
    }
};

use super::{
    unit::{self, Unit},
    sim::{
        Query,
        resources::ResourceKind,
        world::BuildingId,
    }
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

    #[inline]
    pub fn as_producer(&self) -> &producer::ProducerBuilding<'config> {
        self.archetype.as_producer()
    }

    #[inline]
    pub fn as_producer_mut(&mut self) -> &mut producer::ProducerBuilding<'config> {
        self.archetype.as_producer_mut()
    }

    #[inline]
    pub fn as_storage(&self) -> &storage::StorageBuilding<'config> {
        self.archetype.as_storage()
    }

    #[inline]
    pub fn as_storage_mut(&mut self) -> &mut storage::StorageBuilding<'config> {
        self.archetype.as_storage_mut()
    }

    #[inline]
    pub fn as_service(&self) -> &service::ServiceBuilding<'config> {
        self.archetype.as_service()
    }

    #[inline]
    pub fn as_service_mut(&mut self) -> &mut service::ServiceBuilding<'config> {
        self.archetype.as_service_mut()
    }

    #[inline]
    pub fn as_house(&self) -> &house::HouseBuilding<'config> {
        self.archetype.as_house()
    }

    #[inline]
    pub fn as_house_mut(&mut self) -> &mut house::HouseBuilding<'config> {
        self.archetype.as_house_mut()
    }

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
    fn as_producer(&self) -> &producer::ProducerBuilding<'config> {
        match self {
            Self::Producer(state) => state,
            _ => panic!("Building archetype is not Producer!")
        }
    }

    #[inline]
    fn as_producer_mut(&mut self) -> &mut producer::ProducerBuilding<'config> {
        match self {
            Self::Producer(state) => state,
            _ => panic!("Building archetype is not Producer!")
        }
    }

    #[inline]
    fn as_storage(&self) -> &storage::StorageBuilding<'config> {
        match self {
            Self::Storage(state) => state,
            _ => panic!("Building archetype is not Storage!")
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
    fn as_service(&self) -> &service::ServiceBuilding<'config> {
        match self {
            Self::Service(state) => state,
            _ => panic!("Building archetype is not Service!")
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
    fn as_house(&self) -> &house::HouseBuilding<'config> {
        match self {
            Self::House(state) => state,
            _ => panic!("Building archetype is not House!")
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
    fn name(&self) -> &str {
        match self {
            Self::Producer(state) => {
                state.name()
            }
            Self::Storage(state) => {
                state.name()
            }
            Self::Service(state) => {
                state.name()
            }
            Self::House(state) => {
                state.name()
            }
        }
    }

    #[inline]
    fn update(&mut self, context: &BuildingContext<'config, '_, '_>) {
        match self {
            Self::Producer(state) => {
                state.update(context);
            }
            Self::Storage(state) => {
                state.update(context);
            }
            Self::Service(state) => {
                state.update(context);
            }
            Self::House(state) => {
                state.update(context);
            }
        }
    }

    #[inline]
    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContext<'config, '_, '_>) {
        match self {
            Self::Producer(state) => {
                state.visited_by(unit, context);
            }
            Self::Storage(state) => {
                state.visited_by(unit, context);
            }
            Self::Service(state) => {
                state.visited_by(unit, context);
            }
            Self::House(state) => {
                state.visited_by(unit, context);
            }
        }
    }

    #[inline]
    fn receivable_amount(&self, kind: ResourceKind) -> u32 {
        match self {
            Self::Producer(state) => {
                state.receivable_amount(kind)
            }
            Self::Storage(state) => {
                state.receivable_amount(kind)
            }
            Self::Service(state) => {
                state.receivable_amount(kind)
            }
            Self::House(state) => {
                state.receivable_amount(kind)
            }
        }
    }

    #[inline]
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        match self {
            Self::Producer(state) => {
                state.receive_resources(kind, count)
            }
            Self::Storage(state) => {
                state.receive_resources(kind, count)
            }
            Self::Service(state) => {
                state.receive_resources(kind, count)
            }
            Self::House(state) => {
                state.receive_resources(kind, count)
            }
        }
    }

    #[inline]
    fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        match self {
            Self::Producer(state) => {
                state.give_resources(kind, count)
            }
            Self::Storage(state) => {
                state.give_resources(kind, count)
            }
            Self::Service(state) => {
                state.give_resources(kind, count)
            }
            Self::House(state) => {
                state.give_resources(kind, count)
            }
        }
    }

    fn draw_debug_ui(&mut self, context: &BuildingContext<'config, '_, '_>, ui_sys: &UiSystem) {
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
                         context: &BuildingContext<'config, '_, '_>,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange) {
        match self {
            Self::Producer(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range);
            }
            Self::Storage(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range);
            }
            Self::Service(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range);
            }
            Self::House(state) => {
                state.draw_debug_popups(context, ui_sys, transform, visible_range);
            }
        }
    }
}

// ----------------------------------------------
// BuildingBehavior
// ----------------------------------------------

// Common behavior for all Building archetypes.
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
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::Service);
        let config = self.query.building_configs().find_service_config(service_kind);
        self.query.is_near_building(self.map_cells, service_kind, config.effect_radius)
    }

    // `storage_kinds` can be a combination of ORed BuildingKind flags.
    fn for_each_storage<F>(&self, storage_kinds: BuildingKind, mut visitor_fn: F)
        where F: FnMut(&Building<'config>) -> bool
    {
        debug_assert!(storage_kinds.archetype_kind() == BuildingArchetypeKind::Storage);

        let world = self.query.world();
        let storage_buildings = world.buildings_list(BuildingArchetypeKind::Storage);

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
        debug_assert!(storage_kinds.archetype_kind() == BuildingArchetypeKind::Storage);

        let world = self.query.world();
        let storage_buildings = world.buildings_list_mut(BuildingArchetypeKind::Storage);

        for building in storage_buildings.iter_mut() {
            if building.kind().intersects(storage_kinds) && !visitor_fn(building) {
                break;
            }
        }
    }

    fn find_nearest_service_mut(&self, service_kind: BuildingKind) -> Option<&mut Building<'config>> {
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::Service);
        let config = self.query.building_configs().find_service_config(service_kind);

        if let Some(building) =
            self.query.find_nearest_building_mut(self.map_cells, service_kind, config.effect_radius) {
            if building.archetype_kind() != BuildingArchetypeKind::Service || building.kind() != service_kind {
                panic!("Building '{}' ({}|{}): Expected archetype to be Service ({service_kind})!",
                       building.name(), building.archetype_kind(), building.kind());
            }
            return Some(building);
        }
        None
    }
}
