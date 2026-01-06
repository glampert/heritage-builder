#![allow(clippy::enum_variant_names)]

use bitflags::{bitflags, Flags};
use enum_dispatch::enum_dispatch;
use paste::paste;
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::{EnumCount, IntoDiscriminant};
use strum_macros::{Display, EnumCount, EnumDiscriminants, EnumIter};

use house::HouseBuilding;
use producer::ProducerBuilding;
use service::ServiceBuilding;
use storage::StorageBuilding;
use config::{BuildingConfig, BuildingConfigs};

use super::{
    sim::{
        Query,
        debug::DebugUiMode,
        resources::{
            Population, ResourceKind, ResourceKinds, ResourceStock,
            ServiceKind, StockItem, Workers, RESOURCE_KIND_COUNT,
        },
    },
    unit::{patrol::Patrol, runner::Runner, Unit},
    undo_redo::GameObjectSavedState,
    world::{
        stats::WorldStats,
        debug::GameObjectDebugOptions,
        object::{GameObject, GenerationalIndex},
    },
};
use crate::{
    bitflags_with_display,
    engine::time::UpdateTimer,
    game::config::GameConfigs,
    ui::{UiSystem, UiStaticVar},
    log,
    pathfind::{self, NodeKind as PathNodeKind},
    save::PostLoadContext,
    tile::{
        sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY},
        Tile, TileFlags, TileGameObjectHandle, TileKind,
        TileMap, TileMapLayerKind,
    },
    utils::{
        coords::{Cell, CellRange, WorldToScreenTransform},
        hash::StringHash,
        mem, Color,
    },
};

pub mod config;
pub use house::HouseLevel;

mod house;
mod house_upgrade;
mod producer;
mod service;
mod storage;

// ----------------------------------------------
// BuildingKind
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct BuildingKind: u32 {
        // Archetype: House
        const House          = 1 << 0;

        // Archetype: Producer
        const Farm           = 1 << 1;
        const FishingWharf   = 1 << 2;
        const Factory        = 1 << 3;
        const Mine           = 1 << 4;
        const Lumberyard     = 1 << 5;

        // Archetype: Storage
        const Granary        = 1 << 6;
        const StorageYard    = 1 << 7;

        // Archetype: Service
        const SmallWell      = 1 << 8;
        const LargeWell      = 1 << 9;
        const Market         = 1 << 10;
        const TaxOffice      = 1 << 11;
        const Shrine         = 1 << 12;
        const Temple         = 1 << 13;
        const Citadel        = 1 << 14;
        const GovernorPalace = 1 << 15;
        const PoliceStation  = 1 << 16;
        const Theater        = 1 << 17;
        const University     = 1 << 18;
        const Apothecary     = 1 << 19;
        const Hospital       = 1 << 20;
        const Garden         = 1 << 21;
    }
}

impl BuildingKind {
    #[inline]
    pub const fn is_single_building(self) -> bool {
        self.bits().count_ones() == 1
    }

    #[inline]
    pub const fn count() -> usize {
        Self::FLAGS.len()
    }

    #[inline]
    pub const fn producer_count() -> usize {
        Self::producers().bits().count_ones() as usize
    }

    #[inline]
    pub const fn producers() -> Self {
        Self::from_bits_retain(Self::Farm.bits()
                               | Self::FishingWharf.bits()
                               | Self::Factory.bits()
                               | Self::Mine.bits()
                               | Self::Lumberyard.bits())
    }

    #[inline]
    pub const fn storage_count() -> usize {
        Self::storage().bits().count_ones() as usize
    }

    #[inline]
    pub const fn storage() -> Self {
        Self::from_bits_retain(Self::Granary.bits() | Self::StorageYard.bits())
    }

    #[inline]
    pub const fn services_count() -> usize {
        Self::services().bits().count_ones() as usize
    }

    #[inline]
    pub const fn services() -> Self {
        Self::from_bits_retain(Self::SmallWell.bits()
                               | Self::LargeWell.bits()
                               | Self::Market.bits()
                               | Self::TaxOffice.bits()
                               | Self::Shrine.bits()
                               | Self::Temple.bits()
                               | Self::Citadel.bits()
                               | Self::GovernorPalace.bits()
                               | Self::PoliceStation.bits()
                               | Self::Theater.bits()
                               | Self::University.bits()
                               | Self::Apothecary.bits()
                               | Self::Hospital.bits()
                               | Self::Garden.bits())
    }

    #[inline]
    pub const fn treasury() -> Self {
        Self::from_bits_retain(Self::TaxOffice.bits())
    }

    #[inline]
    pub fn from_game_object_handle(handle: TileGameObjectHandle) -> Self {
        Self::from_bits(handle.kind())
            .expect("TileGameObjectHandle does not contain a valid BuildingKind enum value!")
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
            panic!("Unknown archetype for building kind: {}", self);
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
            pub fn [<as_ $derived_mod>](&self) -> &$derived_struct {
                match self.archetype() {
                    BuildingArchetype::$derived_struct(inner) => inner,
                    _ => panic!("Building archetype is not {}!", stringify!($derived_struct))
                }
            }
            #[inline]
            pub fn [<as_ $derived_mod _mut>](&mut self) -> &mut $derived_struct {
                match self.archetype_mut() {
                    BuildingArchetype::$derived_struct(inner) => inner,
                    _ => panic!("Building archetype is not {}!", stringify!($derived_struct))
                }
            }
        }
    };
}

// ----------------------------------------------
// Building
// ----------------------------------------------

pub type BuildingId = GenerationalIndex;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Building {
    id: BuildingId,
    map_cells: CellRange,
    road_link: mem::Mutable<Cell>,
    kind: BuildingKind,
    workers_update_timer: UpdateTimer,
    archetype: Option<BuildingArchetype>,
}

impl GameObject for Building {
    // ----------------------
    // GameObject Interface:
    // ----------------------

    #[inline]
    fn id(&self) -> BuildingId {
        self.id
    }

    fn update(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        // Refresh cached road link cell.
        self.update_road_link(query);

        if self.workers_update_timer.tick(query.delta_time_secs()).should_update() {
            self.update_workers(query);
        }

        let context = self.new_context(query);
        self.archetype_mut().update(&context);
    }

    fn tally(&self, stats: &mut WorldStats) {
        if !self.is_spawned() {
            return;
        }

        if let Some(population) = self.archetype().population() {
            stats.population.total += population.count();
        }

        if let Some(workers) = self.archetype().workers() {
            stats.workers.total += workers.count();

            if let Some(worker_pool) = workers.as_household_worker_pool() {
                stats.population.employed += worker_pool.employed_count();
                stats.population.unemployed += worker_pool.unemployed_count();
            } else if let Some(employer) = workers.as_employer() {
                stats.workers.min_required += employer.min_employees();
                stats.workers.max_employed += employer.max_employees();

                if employer.is_below_min_required() {
                    stats.workers.buildings_below_min += 1;
                }
                if !employer.is_at_max_capacity() {
                    stats.workers.buildings_below_max += 1;
                }
            }
        }

        self.archetype().tally(stats, self.kind);
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        debug_assert!(self.is_spawned());

        self.workers_update_timer.post_load(GameConfigs::get().sim.workers_update_frequency_secs);

        let kind = self.kind();
        debug_assert!(kind.is_single_building());

        let tile =
            context.tile_map()
                   .find_tile(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
                   .unwrap();
        debug_assert!(tile.is_valid());

        self.archetype_mut().post_load(context, kind, tile);
    }

    fn undo_redo_record(&self) -> Option<Box<dyn GameObjectSavedState>> {
        self.archetype().undo_redo_record()
    }

    fn undo_redo_apply(&mut self, state: &dyn GameObjectSavedState) {
        self.archetype_mut().undo_redo_apply(state);

        // Force a workers update right after this.
        self.workers_update_timer.force_update();
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem, mode: DebugUiMode) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                let context = self.new_context(query);
                self.draw_debug_ui_overview(&context, ui_sys);
            }
            DebugUiMode::Detailed => {
                let ui = ui_sys.ui();
                if ui.collapsing_header("Building", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    let context = self.new_context(query);
                    self.draw_debug_ui_detailed(&context, ui_sys);
                    ui.unindent_by(10.0);
                }
            }
        }
    }

    fn draw_debug_popups(&mut self,
                         query: &Query,
                         ui_sys: &UiSystem,
                         transform: WorldToScreenTransform,
                         visible_range: CellRange) {
        debug_assert!(self.is_spawned());

        let tile =
            query.find_tile(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
                 .unwrap();

        self.archetype_mut().debug_options().draw_popup_messages(tile,
                                                                 ui_sys,
                                                                 transform,
                                                                 visible_range,
                                                                 query.delta_time_secs());
    }
}

impl Building {
    // ----------------------
    // Spawning / Despawning:
    // ----------------------

    pub fn spawned(&mut self,
                   query: &Query,
                   id: BuildingId,
                   kind: BuildingKind,
                   map_cells: CellRange,
                   archetype: BuildingArchetype) {
        debug_assert!(!self.is_spawned());
        debug_assert!(id.is_valid());
        debug_assert!(kind.is_single_building());
        debug_assert!(map_cells.is_valid());

        self.id = id;
        self.map_cells = map_cells;
        self.kind = kind;
        self.workers_update_timer = UpdateTimer::new(GameConfigs::get().sim.workers_update_frequency_secs);
        self.archetype = Some(archetype);

        self.update_road_link(query);

        let context = self.new_context(query);
        self.archetype_mut().spawned(&context);
    }

    pub fn despawned(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        self.remove_all_workers(query);
        self.remove_all_population(query);
        self.clear_road_link(query.tile_map());

        let context = self.new_context(query);
        self.archetype_mut().despawned(&context);

        self.id = BuildingId::default();
        self.map_cells = CellRange::default();
        self.kind = BuildingKind::default();
        self.workers_update_timer = UpdateTimer::default();
        self.archetype = None;
    }

    #[inline]
    fn new_context<'world>(&self, query: &'world Query) -> BuildingContext<'world> {
        BuildingContext::new(self.id,
                             self.map_cells,
                             self.road_link(query),
                             self.kind,
                             self.archetype_kind(),
                             query)
    }

    #[inline]
    fn archetype(&self) -> &BuildingArchetype {
        self.archetype.as_ref().unwrap()
    }

    #[inline]
    fn archetype_mut(&mut self) -> &mut BuildingArchetype {
        self.archetype.as_mut().unwrap()
    }

    // ----------------------
    // Utilities:
    // ----------------------

    building_type_casts! { producer, ProducerBuilding } // as_producer()
    building_type_casts! { storage,  StorageBuilding  } // as_storage()
    building_type_casts! { service,  ServiceBuilding  } // as_service()
    building_type_casts! { house,    HouseBuilding    } // as_house()

    #[inline]
    pub fn name(&self) -> &str {
        self.archetype().name()
    }

    #[inline]
    pub fn configs(&self) -> &dyn BuildingConfig {
        self.archetype().configs()
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
    pub fn kind_and_id(&self) -> BuildingKindAndId {
        BuildingKindAndId { kind: self.kind, id: self.id }
    }

    #[inline]
    pub fn tile_info(&self, query: &Query) -> BuildingTileInfo {
        BuildingTileInfo { road_link: self.road_link(query).unwrap_or_default(), /* We may or may not be connected to a road. */
                           base_cell: self.base_cell() }
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
        self.archetype().discriminant()
    }

    pub fn visited_by(&mut self, unit: &mut Unit, query: &Query) {
        debug_assert!(self.is_spawned());
        let context = self.new_context(query);
        self.archetype_mut().visited_by(unit, &context);
    }

    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        debug_assert!(self.is_spawned());
        if self.base_cell() == destination_cell {
            return true;
        }

        if tile_map.try_move_tile(self.base_cell(), destination_cell, TileMapLayerKind::Objects) {
            let tile = tile_map.find_tile_mut(destination_cell,
                                              TileMapLayerKind::Objects,
                                              TileKind::Building)
                               .unwrap();

            debug_assert!(tile.base_cell() == destination_cell);
            self.map_cells = tile.cell_range();
            return true;
        }

        false
    }

    pub fn set_random_variation(&self, query: &Query) {
        let context = self.new_context(query);
        context.set_random_building_variation();
    }

    // ----------------------
    // Building Resources:
    // ----------------------

    #[inline]
    pub fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        debug_assert!(self.is_spawned());
        self.archetype().available_resources(kind)
    }

    #[inline]
    pub fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        debug_assert!(self.is_spawned());
        self.archetype().receivable_resources(kind)
    }

    #[inline]
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        debug_assert!(self.is_spawned());
        self.archetype_mut().receive_resources(kind, count)
    }

    #[inline]
    pub fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        debug_assert!(self.is_spawned());
        self.archetype_mut().remove_resources(kind, count)
    }

    // ----------------------
    // Patrol/Runner Units:
    // ----------------------

    #[inline]
    pub fn active_patrol(&mut self) -> Option<&mut Patrol> {
        self.archetype_mut().active_patrol()
    }

    #[inline]
    pub fn active_runner(&mut self) -> Option<&mut Runner> {
        self.archetype_mut().active_runner()
    }

    // ----------------------
    // Population:
    // ----------------------

    #[inline]
    pub fn population(&self) -> Option<Population> {
        self.archetype().population()
    }

    #[inline]
    pub fn population_count(&self) -> u32 {
        self.archetype().population().map_or(0, |population| population.count())
    }

    #[inline]
    pub fn population_is_maxed(&self) -> bool {
        self.archetype().population().is_none_or(|population| population.is_max())
    }

    #[inline]
    pub fn add_population(&mut self, query: &Query, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        let context = self.new_context(query);
        self.archetype_mut().add_population(&context, count)
    }

    #[inline]
    pub fn remove_population(&mut self, query: &Query, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        let context = self.new_context(query);
        self.archetype_mut().remove_population(&context, count)
    }

    #[inline]
    fn remove_all_population(&mut self, query: &Query) {
        self.remove_population(query, self.population_count());
    }

    // ----------------------
    // Workers:
    // ----------------------

    #[inline]
    pub fn workers(&self) -> Option<&Workers> {
        self.archetype().workers()
    }

    #[inline]
    pub fn workers_count(&self) -> u32 {
        self.archetype().workers().map_or(0, |workers| workers.count())
    }

    #[inline]
    pub fn workers_is_maxed(&self) -> bool {
        self.archetype().workers().is_none_or(|workers| workers.is_max())
    }

    // These return the amount added/removed, which can be <= the `count` parameter.
    pub fn add_workers(&mut self, count: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(self.is_spawned());
        let mut workers_added = 0;
        if count != 0 && !self.workers_is_maxed() {
            if let Some(workers) = self.archetype_mut().workers_mut() {
                workers_added = workers.add(count, source);
            }

            if workers_added != 0 {
                self.archetype_mut()
                    .debug_options()
                    .popup_msg_color_string(Color::cyan(),
                                            format!("+{workers_added} workers").into());
            }
        }
        workers_added
    }

    pub fn remove_workers(&mut self, count: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(self.is_spawned());
        let mut workers_removed = 0;
        if count != 0 && self.workers_count() != 0 {
            if let Some(workers) = self.archetype_mut().workers_mut() {
                workers_removed = workers.remove(count, source);
            }

            if workers_removed != 0 {
                self.archetype_mut()
                    .debug_options()
                    .popup_msg_color_string(Color::magenta(),
                                            format!("-{workers_removed} workers").into());
            }
        }
        workers_removed
    }

    fn remove_all_workers(&mut self, query: &Query) {
        if let Some(workers) = self.archetype().workers() {
            if let Some(employer) = workers.as_employer() {
                employer.for_each_employee_household(query.world(), |household, employee_count| {
                            // Put worker back into the house's worker pool.
                            household.add_workers(employee_count, self.kind_and_id());
                            true
                        });
            } else if let Some(household) = workers.as_household_worker_pool() {
                household.for_each_employer(query.world(), |employer, employed_count| {
                             // Tell employer workers are no longer available from this household.
                             employer.remove_workers(employed_count, self.kind_and_id());
                             true
                         });
            } else {
                panic!("Unhandled Workers kind!");
            }
        }

        if let Some(workers) = self.archetype_mut().workers_mut() {
            workers.clear();
        }
    }

    fn update_workers(&mut self, query: &Query) {
        if !self.is_linked_to_road(query) {
            return;
        }

        // Search for employees if we're an Employer and not already at max capacity.
        if let Some(workers) = self.archetype().workers() {
            if let Some(employer) = workers.as_employer() {
                if !employer.is_at_max_capacity() {
                    if let Some(house) = self.find_house_with_available_workers(query) {
                        let workers_available = house.workers_count();
                        let workers_added =
                            self.add_workers(workers_available, house.kind_and_id());
                        let workers_removed =
                            house.remove_workers(workers_added, self.kind_and_id());
                        debug_assert!(workers_added == workers_removed);
                    }
                }
            }
        }
    }

    fn find_house_with_available_workers<'world>(&self,
                                                 query: &'world Query)
                                                 -> Option<&'world mut Building> {
        let workers_search_radius = GameConfigs::get().sim.workers_search_radius;
        debug_assert!(workers_search_radius > 0);

        let result = query.find_nearest_buildings(self.road_link(query).unwrap(),
                                                  BuildingKind::House,
                                                  PathNodeKind::Road,
                                                  Some(workers_search_radius),
                                                  |house, _path| {
                                                      if house.workers_count() != 0 {
                                                          return false; // Accept and stop search.
                                                      }
                                                      true // Continue searching.
                                                  });

        if let Some((house, _path)) = result {
            debug_assert!(house.is(BuildingKind::House));
            debug_assert!(house.workers_count() != 0);
            return Some(house);
        }

        None
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
        debug_assert!(self.is_spawned());

        if self.road_link.is_valid() {
            return Some(*self.road_link.as_ref());
        }

        // Lazily cache the road link cell on demand:
        if let Some(road_link) = query.find_nearest_road_link(self.cell_range()) {
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

    pub fn find_road_link_tile<'world>(&self, query: &'world Query) -> Option<&'world mut Tile> {
        if let Some(road_link) = self.road_link(query) {
            return Self::find_road_link_tile_for_cell(query, road_link);
        }
        None
    }

    fn find_road_link_tile_for_cell(query: &Query, road_link: Cell) -> Option<&mut Tile> {
        query.find_tile_mut(road_link, TileMapLayerKind::Terrain, TileKind::Terrain)
    }

    fn update_road_link(&mut self, query: &Query) {
        if let Some(new_road_link) = query.find_nearest_road_link(self.cell_range()) {
            debug_assert!(new_road_link.is_valid());

            if new_road_link != *self.road_link.as_ref() && self.road_link.is_valid() {
                // Clear previous underlying tile flag:
                if let Some(prev_road_link_tile) =
                    Self::find_road_link_tile_for_cell(query, *self.road_link.as_ref())
                {
                    prev_road_link_tile.set_flags(TileFlags::BuildingRoadLink, false);
                }
            }

            // Set new underlying tile flag:
            if let Some(new_road_link_tile) =
                Self::find_road_link_tile_for_cell(query, new_road_link)
            {
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
            if let Some(road_link_tile) =
                tile_map.try_tile_from_layer_mut(*road_link, TileMapLayerKind::Terrain)
            {
                road_link_tile.set_flags(TileFlags::BuildingRoadLink, false);
            }
        }
        *road_link = Cell::invalid();
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        ProducerBuilding::register_callbacks();
        StorageBuilding::register_callbacks();
        ServiceBuilding::register_callbacks();
        HouseBuilding::register_callbacks();
    }

    // ----------------------
    // Building Debug UI:
    // ----------------------

    fn draw_debug_ui_overview(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let color_bullet_bool = |label: &str, value: bool| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            if value {
                ui.text("yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        let color_bullet_text = |label: &str, value: &str| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            ui.text_colored(Color::red().to_array(), value);
        };

        ui.set_window_font_scale(1.2);
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.base_cell()));
        ui.set_window_font_scale(1.0);

        color_bullet_bool("Linked to road", self.is_linked_to_road(context.query));

        if self.archetype_kind() == BuildingArchetypeKind::HouseBuilding {
            let house = self.as_house();
            ui.bullet_text(format!("Tax: (generated: {}, avail: {})",
                                   house.tax_generated(),
                                   house.tax_available()));

            if !house.level().is_max() {
                let upgrade_requirements = house.upgrade_requirements(context);
                let has_required_resources = upgrade_requirements.has_required_resources();
                let has_required_services = upgrade_requirements.has_required_services();

                color_bullet_bool("Has resources to upgrade", has_required_resources);
                if !has_required_resources {
                    color_bullet_text("Missing", &upgrade_requirements.resources_missing().to_string());
                }

                color_bullet_bool("Has services to upgrade", has_required_services);
                if !has_required_services {
                    color_bullet_text("Missing", &upgrade_requirements.services_missing().to_string());
                }

                color_bullet_bool("Has room to upgrade", house.is_upgrade_available(context));
            } else {
                ui.bullet_text("Max house level reached");
            }
        } else {
            color_bullet_bool("Is operational", self.archetype().is_operational());
            color_bullet_bool("Has resources", self.archetype().has_min_required_resources());

            if self.archetype().has_stock() {
                color_bullet_bool("Stock is full", self.archetype().is_stock_full());
            }
        }

        if let Some(population) = self.archetype().population() {
            ui.bullet_text(format!("Population: {} (max: {})",
                                   population.count(),
                                   population.max()));
        }

        if let Some(workers) = self.archetype().workers() {
            if let Some(worker_pool) = workers.as_household_worker_pool() {
                ui.bullet_text(format!("Workers: {} (employed: {}, unemployed: {})",
                                       worker_pool.total_workers(),
                                       worker_pool.employed_count(),
                                       worker_pool.unemployed_count()));
            } else if let Some(employer) = workers.as_employer() {
                color_bullet_bool("Has min workers", self.archetype().has_min_required_workers());
                color_bullet_bool("Has all workers", employer.is_at_max_capacity());
                if employer.is_below_min_required() {
                    ui.bullet_text("Workers:");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(),
                                    format!("{}", employer.employee_count()));
                    ui.same_line();
                    ui.text(format!("(min: {}, max: {})",
                                    employer.min_employees(),
                                    employer.max_employees()));
                } else {
                    ui.bullet_text(format!("Workers: {} (min: {}, max: {})",
                                           employer.employee_count(),
                                           employer.min_employees(),
                                           employer.max_employees()));
                }
            }
        }
    }

    fn draw_debug_ui_detailed(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

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
                kind: self.kind(),
                archetype: self.archetype_kind(),
                cells: self.cell_range(),
                road_link: self.road_link(context.query).unwrap_or_default(),
                id: self.id()
            };
            debug_vars.draw_debug_ui(ui_sys);
        }

        self.configs().draw_debug_ui(ui_sys);

        if let Some(population) = self.archetype().population() {
            if ui.collapsing_header("Population", imgui::TreeNodeFlags::empty()) {
                population.draw_debug_ui(ui_sys);

                if ui.button("Increase Population (+1)") {
                    self.archetype_mut().add_population(context, 1);
                }

                if ui.button("Evict Resident (-1)") {
                    self.archetype_mut().remove_population(context, 1);
                }

                if ui.button("Evict All Residents") {
                    self.remove_all_population(context.query);
                }
            }
        }

        if let Some(workers) = self.archetype().workers() {
            if ui.collapsing_header("Workers", imgui::TreeNodeFlags::empty()) {
                let is_household_worker_pool = workers.is_household_worker_pool();
                let is_employer = workers.is_employer();

                workers.draw_debug_ui(context.query.world(), ui_sys);
                ui.separator();

                let source = {
                    static BUILDING_KIND_IDX: UiStaticVar<usize> = UiStaticVar::new(0);
                    static BUILDING_GEN: UiStaticVar<u32> = UiStaticVar::new(0);
                    static BUILDING_ID: UiStaticVar<usize> = UiStaticVar::new(0);

                    if is_household_worker_pool {
                        ui.text("Select Employer:");
                    } else {
                        ui.text("Select Worker Household:");
                    }

                    let kind = {
                        if is_employer {
                            // Employers only source workers from houses.
                            BUILDING_KIND_IDX.set(0);
                            ui.combo_simple_string("Kind", BUILDING_KIND_IDX.as_mut(), &["House"]);
                            BuildingKind::House
                        } else {
                            let mut building_kind_names: SmallVec<[&'static str; BuildingKind::count()]> = SmallVec::new();
                            for kind in BuildingKind::FLAGS {
                                if *kind.value() != BuildingKind::House {
                                    building_kind_names.push(kind.name());
                                }
                            }
                            ui.combo_simple_string("Kind",
                                                   BUILDING_KIND_IDX.as_mut(),
                                                   &building_kind_names);
                            *BuildingKind::FLAGS[*BUILDING_KIND_IDX + 1].value() // We've skipped House @ [0]
                        }
                    };

                    ui.input_scalar("Gen", BUILDING_GEN.as_mut()).step(1).build();
                    ui.input_scalar("Idx", BUILDING_ID.as_mut()).step(1).build();

                    BuildingKindAndId { kind, id: BuildingId::new(*BUILDING_GEN, *BUILDING_ID) }
                };

                if ui.button("Add Worker (+1)") && !self.workers_is_maxed() {
                    if let Some(building) =
                        context.query.world().find_building_mut(source.kind, source.id)
                    {
                        let removed_count = building.remove_workers(1, self.kind_and_id());
                        let added_count = self.add_workers(removed_count, source);
                        debug_assert!(removed_count == added_count);
                    } else {
                        log::error!("Add Worker: Invalid source building id!");
                    }
                }

                if ui.button("Remove Worker (-1)") && self.workers_count() != 0 {
                    if let Some(building) =
                        context.query.world().find_building_mut(source.kind, source.id)
                    {
                        let added_count = building.add_workers(1, self.kind_and_id());
                        let removed_count = self.remove_workers(added_count, source);
                        debug_assert!(added_count == removed_count);
                    } else {
                        log::error!("Remove Worker: Invalid source building id!");
                    }
                }

                if ui.button("Remove All Workers") {
                    self.remove_all_workers(context.query);
                }

                if is_employer {
                    // Only employers need to search for workers.
                    self.workers_update_timer.draw_debug_ui("Update", 0, ui_sys);
                }
            }
        }

        if ui.collapsing_header("Access", imgui::TreeNodeFlags::empty()) {
            if self.is_linked_to_road(context.query) {
                ui.text_colored(Color::green().to_array(), "Has road access.");
            } else {
                ui.text_colored(Color::red().to_array(), "No road access!");
            }

            ui.text(format!("Road Link Tile : {}",
                            self.road_link(context.query).unwrap_or_default()));

            let mut show_road_link = self.is_showing_road_link_debug(context.query);
            if ui.checkbox("Show Road Link", &mut show_road_link) {
                self.set_show_road_link_debug(context.query, show_road_link);
            }

            if ui.button("Highlight Access Tiles") {
                pathfind::highlight_building_access_tiles(context.query.tile_map(),
                                                          self.cell_range());
            }
        }

        self.archetype_mut().debug_options().draw_debug_ui(ui_sys);
        self.archetype_mut().draw_debug_ui(context, ui_sys);
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
 - Produces a resource/consumer good (farm, fishing wharf, factory) or raw material (mine, lumberyard).
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
#[derive(Clone, EnumDiscriminants, Serialize, Deserialize)]
#[strum_discriminants(repr(u32),
                      name(BuildingArchetypeKind),
                      derive(Display, EnumCount, EnumIter, PartialOrd, Ord, Serialize, Deserialize))]
pub enum BuildingArchetype {
    ProducerBuilding(ProducerBuilding),
    StorageBuilding(StorageBuilding),
    ServiceBuilding(ServiceBuilding),
    HouseBuilding(HouseBuilding),
}

pub const BUILDING_ARCHETYPE_COUNT: usize = BuildingArchetypeKind::COUNT;

// ----------------------------------------------
// BuildingBehavior
// ----------------------------------------------

// Common behavior for all Building archetypes.
#[enum_dispatch(BuildingArchetype)]
trait BuildingBehavior {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str;
    fn configs(&self) -> &dyn BuildingConfig;

    fn spawned(&mut self, _context: &BuildingContext) {}
    fn despawned(&mut self, _context: &BuildingContext) {}

    fn update(&mut self, context: &BuildingContext);
    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContext);

    fn post_load(&mut self, context: &PostLoadContext, kind: BuildingKind, tile: &Tile);

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn has_stock(&self) -> bool;
    fn is_stock_full(&self) -> bool;
    fn has_min_required_resources(&self) -> bool {
        true
    }

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

    // Add to the world resource counts.
    fn tally(&self, stats: &mut WorldStats, kind: BuildingKind);

    // ----------------------
    // Patrol/Runner Units:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> {
        None
    }

    fn active_runner(&mut self) -> Option<&mut Runner> {
        None
    }

    // ----------------------
    // Population:
    // ----------------------

    fn population(&self) -> Option<Population> {
        None
    }

    // These return the amount added/removed, which can be <= the `count` parameter.
    fn add_population(&mut self, _context: &BuildingContext, _count: u32) -> u32 {
        0
    }

    fn remove_population(&mut self, _context: &BuildingContext, _count: u32) -> u32 {
        0
    }

    // ----------------------
    // Workers:
    // ----------------------

    fn workers(&self) -> Option<&Workers> {
        None
    }

    fn workers_mut(&mut self) -> Option<&mut Workers> {
        None
    }

    fn is_operational(&self) -> bool {
        self.has_min_required_workers() && self.has_min_required_resources()
    }

    fn has_min_required_workers(&self) -> bool {
        true
    }

    // ----------------------
    // Undo/Redo:
    // ----------------------

    fn undo_redo_record(&self) -> Option<Box<dyn GameObjectSavedState>> {
        None
    }

    fn undo_redo_apply(&mut self, _state: &dyn GameObjectSavedState) {
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions;
    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem);
}

// ----------------------------------------------
// BuildingKindAndId / BuildingTileInfo
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildingKindAndId {
    pub kind: BuildingKind,
    pub id: BuildingId,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
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

pub struct BuildingContext<'world> {
    pub id: BuildingId,
    pub map_cells: mem::Mutable<CellRange>,
    pub road_link: Option<Cell>,
    pub kind: BuildingKind,
    pub archetype_kind: BuildingArchetypeKind,
    pub query: &'world Query,
}

impl<'world> BuildingContext<'world> {
    fn new(id: BuildingId,
           map_cells: CellRange,
           road_link: Option<Cell>,
           kind: BuildingKind,
           archetype_kind: BuildingArchetypeKind,
           query: &'world Query)
           -> Self {
        Self { id, map_cells: mem::Mutable::new(map_cells), road_link, kind, archetype_kind, query }
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.map_cells.start
    }

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        *self.map_cells.as_ref()
    }

    #[inline]
    pub fn kind_and_id(&self) -> BuildingKindAndId {
        BuildingKindAndId { kind: self.kind, id: self.id }
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
    pub fn find_tile_def(&self, tile_def_name_hash: StringHash) -> Option<&'static TileDef> {
        self.query.find_tile_def(TileMapLayerKind::Objects,
                                 OBJECTS_BUILDINGS_CATEGORY.hash,
                                 tile_def_name_hash)
    }

    #[inline]
    pub fn find_tile(&self) -> &Tile {
        self.query
            .find_tile(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    pub fn find_tile_mut(&self) -> &mut Tile {
        self.query
            .find_tile_mut(self.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
            .expect("Building should have an associated Tile in the TileMap!")
    }

    #[inline]
    pub fn has_access_to_service(&self, service_kind: ServiceKind) -> bool {
        debug_assert!(service_kind.is_single_building());
        debug_assert!(service_kind.archetype_kind() == BuildingArchetypeKind::ServiceBuilding);

        if let Some(road_link) = self.road_link {
            let config = BuildingConfigs::get().find_service_config(service_kind);
            return self.query.is_near_building(road_link,
                                               service_kind,
                                               config.requires_road_access,
                                               config.effect_radius);
        }

        false
    }

    #[inline]
    pub fn set_random_building_variation(&self) {
        let tile = self.find_tile_mut();
        tile.set_random_variation_index(self.query.rng());
    }

    // Road link if valid, any unobstructed surrounding cell otherwise.
    pub fn road_link_or_building_access_tile(&self) -> Cell {
        if let Some(road_link) = self.road_link {
            if road_link.is_valid() {
                return road_link;
            }
        }

        let tile_map = self.query.tile_map();
        let mut access_cell = Cell::invalid();

        pathfind::for_each_surrounding_cell(self.cell_range(), |cell| {
            // Take any surrounding cell that is not obstructed by another object.
            if tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).is_none() {
                access_cell = cell;
                return false;
            }
            true
        });

        access_cell
    }
}

// ----------------------------------------------
// BuildingStock
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct BuildingStock {
    resources: ResourceStock,
    capacities: [u8; RESOURCE_KIND_COUNT],
}

impl BuildingStock {
    fn with_accepted_list_and_capacity(accepted_resources: &ResourceKinds, capacity: u32) -> Self {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        Self { resources: ResourceStock::with_accepted_list(accepted_resources),
               capacities: [capacity_u8; RESOURCE_KIND_COUNT] }
    }

    fn with_accepted_kinds_and_capacity(accepted_kinds: ResourceKind, capacity: u32) -> Self {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        Self { resources: ResourceStock::with_accepted_kinds(accepted_kinds),
               capacities: [capacity_u8; RESOURCE_KIND_COUNT] }
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.resources.count(kind)
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        if let Some((index, item)) = self.resources.find(kind) {
            debug_assert!(item.count <= self.capacity_at(index), "{item}");
            return self.capacity_at(index) - item.count;
        }
        0
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
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

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
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

    fn update_capacities(&mut self, capacity: u32) {
        let capacity_u8: u8 = capacity.try_into().expect("Stock capacity must be < 256");
        self.capacities = [capacity_u8; RESOURCE_KIND_COUNT];

        // Clamp any existing resources to the new capacity.
        self.resources.for_each_mut(|index, item| {
                          item.count = item.count.min(self.capacities[index] as u32);
                      });
    }

    fn merge(&mut self, other: &BuildingStock) -> bool {
        let mut success = true;

        other.for_each(|index, item| {
            let received_count = self.receive_resources(item.kind, item.count);
            if received_count != item.count {
                log::error!("Stock merge exceeds max capacity for {}. Capacity: {}, trying to merge: {}, merged only: {}",
                            item.kind, self.capacity_at(index), item.count, received_count);
                success = false;
            }
        });

        success
    }

    #[inline]
    fn has_any_of(&self, kinds: ResourceKind) -> bool {
        self.resources.has_any_of(kinds)
    }

    #[inline]
    fn capacity_for(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        if let Some((index, _)) = self.resources.find(kind) {
            return self.capacity_at(index);
        }
        0
    }

    #[inline]
    fn capacity_at(&self, index: usize) -> u32 {
        self.capacities[index] as u32
    }

    #[inline]
    fn accepts_any(&self) -> bool {
        self.resources.accepts_any()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    #[inline]
    fn is_full(&self) -> bool {
        let mut full_count = 0;
        self.resources.for_each(|index, item| {
                          let item_capacity = self.capacities[index] as u32;
                          if item.count >= item_capacity {
                              full_count += 1;
                          }
                      });
        full_count == self.resources.accepted_count()
    }

    #[inline]
    fn clear(&mut self) {
        self.resources.clear();
    }

    #[inline]
    fn fill(&mut self) {
        self.resources.for_each_mut(|index, item| {
                          item.count = self.capacities[index] as u32;
                      });
    }

    #[inline]
    fn for_each<F>(&self, visitor_fn: F)
        where F: FnMut(usize, &StockItem)
    {
        self.resources.for_each(visitor_fn);
    }

    fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
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
