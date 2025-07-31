use rand::{distr::uniform::{SampleRange, SampleUniform}, Rng, SeedableRng};
use rand_pcg::Pcg64;

use crate::{
    imgui_ui::UiSystem,
    pathfind::{
        Graph,
        Search,
        SearchResult,
        AStarUniformCostHeuristic,
        NodeKind as PathNodeKind,
        Node,
    },
    utils::{
        coords::{Cell, CellRange},
        hash::StringHash,
        UnsafeWeakRef,
        Seconds
    },
    tile::{
        map::{Tile, TileMap, TileMapLayerKind},
        sets::{TileDef, TileKind, TileSets}
    }
};

use super::{
    sim::world::World,
    unit::config::UnitConfigs,
    building::{
        Building,
        BuildingKind,
        config::BuildingConfigs
    }
};

pub mod debug;
pub mod resources;
pub mod world;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

const DEFAULT_RANDOM_SEED: u64 = 0xCAFE0CAFE0CAFE03;
pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

const DEFAULT_SIM_UPDATE_FREQUENCY_SECS: Seconds = 0.5;

pub struct Simulation<'config> {
    update_timer: UpdateTimer,
    rng: RandomGenerator,

    // Path finding:
    graph: Graph,
    search: Search,

    building_configs: &'config BuildingConfigs,
    unit_configs: &'config UnitConfigs,
}

impl<'config> Simulation<'config> {
    pub fn new(tile_map: &TileMap,
               building_configs: &'config BuildingConfigs,
               unit_configs: &'config UnitConfigs) -> Self {
        Self {
            update_timer: UpdateTimer::new(DEFAULT_SIM_UPDATE_FREQUENCY_SECS),
            rng: RandomGenerator::seed_from_u64(DEFAULT_RANDOM_SEED),
            graph: Graph::from_tile_map(tile_map),
            search: Search::with_grid_size(tile_map.size_in_cells()),
            building_configs,
            unit_configs,
        }
    }

    pub fn update<'tile_sets>(&mut self,
                              world: &mut World<'config>,
                              tile_map: &mut TileMap<'tile_sets>,
                              tile_sets: &'tile_sets TileSets,
                              delta_time_secs: Seconds) {

        // Rebuild the search graph once every frame so any
        // add/remove tile changes will be reflected on the graph.
        self.graph.rebuild_from_tile_map(tile_map, true);

        let query = Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            world,
            tile_map,
            tile_sets,
            self.building_configs,
            self.unit_configs);

        // Units movement needs to be smooth, so it updates every frame.
        world.update_unit_navigation(&query, delta_time_secs);

        // Fixed step update.
        let world_update_delta_time_secs = self.update_timer.time_since_last_secs();
        if self.update_timer.tick(delta_time_secs).should_update() {
            world.update(&query, world_update_delta_time_secs);
        }
    }

    // ----------------------
    // Debug:
    // ----------------------

    // Buildings:
    pub fn draw_building_debug_popups(&mut self,
                                      context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                      visible_range: CellRange,
                                      show_popup_messages: bool) {

        let query = Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            context.world,
            context.tile_map,
            context.tile_sets,
            self.building_configs,
            self.unit_configs);

        context.world.draw_building_debug_popups(
            &query,
            context.ui_sys,
            &context.transform,
            visible_range,
            context.delta_time_secs,
            show_popup_messages);
    }

    pub fn draw_building_debug_ui(&mut self,
                                  context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                  selected_cell: Cell) {

        let query = Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            context.world,
            context.tile_map,
            context.tile_sets,
            self.building_configs,
            self.unit_configs);

        context.world.draw_building_debug_ui(
            &query,
            context.ui_sys,
            selected_cell);
    }

    // Units:
    pub fn draw_unit_debug_popups(&mut self,
                                  context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                  visible_range: CellRange,
                                  show_popup_messages: bool) {

        let query = Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            context.world,
            context.tile_map,
            context.tile_sets,
            self.building_configs,
            self.unit_configs);

        context.world.draw_unit_debug_popups(
            &query,
            context.ui_sys,
            &context.transform,
            visible_range,
            context.delta_time_secs,
            show_popup_messages);
    }

    pub fn draw_unit_debug_ui(&mut self,
                              context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                              selected_cell: Cell) {

        let query = Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            context.world,
            context.tile_map,
            context.tile_sets,
            self.building_configs,
            self.unit_configs);

        context.world.draw_unit_debug_ui(
            &query,
            context.ui_sys,
            selected_cell);
    }
}

// ----------------------------------------------
// UpdateTimer
// ----------------------------------------------

pub struct UpdateTimer {
    update_frequency_secs: Seconds,
    time_since_last_update_secs: Seconds,
}

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UpdateTimerResult {
    DoNotUpdate,
    ShouldUpdate,
}

impl UpdateTimerResult {
    #[inline]
    pub fn should_update(self) -> bool {
        self == UpdateTimerResult::ShouldUpdate
    }
}

impl UpdateTimer {
    #[inline]
    pub fn new(update_frequency_secs: Seconds) -> Self {
        Self {
            update_frequency_secs,
            time_since_last_update_secs: 0.0,
        }
    }

    #[inline]
    pub fn tick(&mut self, delta_time_secs: Seconds) -> UpdateTimerResult {
        if self.time_since_last_update_secs >= self.update_frequency_secs {
            // Reset the clock.
            self.time_since_last_update_secs = 0.0;
            UpdateTimerResult::ShouldUpdate
        } else {
            // Advance the clock.
            self.time_since_last_update_secs += delta_time_secs;
            UpdateTimerResult::DoNotUpdate
        }
    }

    #[inline]
    pub fn frequency_secs(&self) -> f32 {
        self.update_frequency_secs
    }

    #[inline]
    pub fn time_since_last_secs(&self) -> f32 {
        self.time_since_last_update_secs
    }

    pub fn draw_debug_ui(&mut self, label: &str, imgui_id: u32, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        ui.text(format!("{}:", label));

        ui.input_float(format!("Frequency (secs)##_timer_frequency_{}", imgui_id), &mut self.update_frequency_secs)
            .display_format("%.2f")
            .step(0.5)
            .build();

        ui.input_float(format!("Time since last##_last_update_{}", imgui_id), &mut self.time_since_last_update_secs)
            .display_format("%.2f")
            .read_only(true)
            .build();
    }
}

// ----------------------------------------------
// Query
// ----------------------------------------------

pub struct Query<'config, 'tile_sets> {
    // SAFETY: Queries are local variables in the Simulation::update() stack,
    // so none of the references stored here will persist or leak outside the
    // update call stack. Storing weak references here makes things easier
    // since Query is only a container of references to external objects,
    // so we don't want any of these lifetimes to be associated with the
    // Query's lifetime. It also allows us to pass immutable Query refs.

    // Random generator:
    rng: UnsafeWeakRef<RandomGenerator>,

    // Path finding:
    graph: UnsafeWeakRef<Graph>,
    search: UnsafeWeakRef<Search>,

    // World & Tile Map:
    world: UnsafeWeakRef<World<'config>>,
    tile_map: UnsafeWeakRef<TileMap<'tile_sets>>,
    tile_sets: &'tile_sets TileSets,

    building_configs: &'config BuildingConfigs,
    unit_configs: &'config UnitConfigs,
}

impl<'config, 'tile_sets> Query<'config, 'tile_sets> {
    #[allow(clippy::too_many_arguments)]
    fn new(rng: &mut RandomGenerator,
           graph: &mut Graph,
           search: &mut Search,
           world: &mut World<'config>,
           tile_map: &mut TileMap<'tile_sets>,
           tile_sets: &'tile_sets TileSets,
           building_configs: &'config BuildingConfigs,
           unit_configs: &'config UnitConfigs) -> Self {
        Self {
            rng: UnsafeWeakRef::new(rng),
            graph: UnsafeWeakRef::new(graph),
            search: UnsafeWeakRef::new(search),
            world: UnsafeWeakRef::new(world),
            tile_map: UnsafeWeakRef::new(tile_map),
            tile_sets,
            building_configs,
            unit_configs,
        }
    }

    #[inline]
    fn calc_search_radius(start_cells: CellRange, radius_in_cells: i32) -> CellRange {
        debug_assert!(start_cells.is_valid());
        debug_assert!(radius_in_cells > 0);
        let start_x = start_cells.start.x - radius_in_cells;
        let start_y = start_cells.start.y - radius_in_cells;
        let end_x   = start_cells.end.x   + radius_in_cells;
        let end_y   = start_cells.end.y   + radius_in_cells;
        CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y))
    }

    #[inline(always)]
    fn rng(&self) -> &mut RandomGenerator {
        self.rng.mut_ref_cast()
    }

    #[inline(always)]
    fn search(&self) -> &mut Search {
        self.search.mut_ref_cast()
    }

    // ----------------------
    // Public API:
    // ----------------------

    #[inline(always)]
    pub fn graph(&self) -> &mut Graph {
        self.graph.mut_ref_cast()
    }

    #[inline(always)]
    pub fn world(&self) -> &mut World<'config> {
        self.world.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_map(&self) -> &mut TileMap<'tile_sets> {
        self.tile_map.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_sets(&self) -> &'tile_sets TileSets {
        self.tile_sets
    }

    #[inline(always)]
    pub fn building_configs(&self) -> &'config BuildingConfigs {
        self.building_configs
    }

    #[inline(always)]
    pub fn unit_configs(&self) -> &'config UnitConfigs {
        self.unit_configs
    }

    #[inline(always)]
    pub fn random_in_range<T, R>(&self, range: R) -> T
        where T: SampleUniform,
              R: SampleRange<T>
    {
        self.rng().random_range(range)
    }

    #[inline]
    pub fn find_tile_def(&self,
                         layer: TileMapLayerKind,
                         category_name_hash: StringHash,
                         tile_def_name_hash: StringHash) -> Option<&'tile_sets TileDef> {

        self.tile_sets().find_tile_def_by_hash(layer, category_name_hash, tile_def_name_hash)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer: TileMapLayerKind,
                     tile_kinds: TileKind) -> Option<&Tile<'tile_sets>> {

        self.tile_map().find_tile(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&self,
                         cell: Cell,
                         layer: TileMapLayerKind,
                         tile_kinds: TileKind) -> Option<&mut Tile<'tile_sets>> {

        self.tile_map().find_tile_mut(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_path(&self, traversable_node_kinds: PathNodeKind, start: Cell, goal: Cell) -> SearchResult {
        self.search().find_path(self.graph(),
                                &AStarUniformCostHeuristic::new(),
                                traversable_node_kinds,
                                Node::new(start),
                                Node::new(goal))
    }

    pub fn find_nearest_road_link(&self, start_cells: CellRange) -> Cell {
        let start_x = start_cells.start.x - 1;
        let start_y = start_cells.start.y - 1;
        let end_x   = start_cells.end.x   + 1;
        let end_y   = start_cells.end.y   + 1;
        let expanded_range = CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y));

        for cell in &expanded_range {
            // Skip diagonal corners.
            #[allow(clippy::nonminimal_bool)] // Current code is more verbose but simple. Ignore this lint.
            let is_corner =
                (cell.x == start_x && cell.y == start_y) ||
                (cell.x == start_x && cell.y == end_y)   ||
                (cell.x == end_x   && cell.y == start_y) ||
                (cell.x == end_x   && cell.y == end_y);

            if !is_corner {
                if let Some(node_kind) = self.graph().node_kind(Node::new(cell)) {
                    if node_kind == PathNodeKind::Road {
                        return cell;
                    }
                }
            }
        }

        Cell::invalid()
    }

    pub fn is_near_building(&self,
                            start_cells: CellRange,
                            kind: BuildingKind,
                            radius_in_cells: i32) -> bool {

        let search_range = Self::calc_search_radius(start_cells, radius_in_cells);

        for search_cell in &search_range {
            if let Some(search_tile) =
                self.tile_map().find_tile(search_cell, TileMapLayerKind::Objects, TileKind::Building) {
                let game_state = search_tile.game_state_handle();
                if game_state.is_valid() {
                    let building_kind = BuildingKind::from_game_state_handle(game_state);
                    if building_kind == kind {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn find_nearest_building_mut(&self,
                                     start_cells: CellRange,
                                     kind: BuildingKind,
                                     radius_in_cells: i32) -> Option<&mut Building<'config>> {

        let world = self.world();
        let tile_map = self.tile_map();
        let search_range = Self::calc_search_radius(start_cells, radius_in_cells);

        for search_cell in &search_range {
            if let Some(search_tile) =
                tile_map.find_tile(search_cell, TileMapLayerKind::Objects, TileKind::Building) {
                let game_state = search_tile.game_state_handle();
                if game_state.is_valid() {
                    let building_kind = BuildingKind::from_game_state_handle(game_state);
                    if building_kind == kind {
                        return world.find_building_for_tile_mut(search_tile);
                    }
                }
            }
        }

        None
    }
}
