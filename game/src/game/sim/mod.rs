#![allow(clippy::too_many_arguments)]

use rand::{distr::uniform::{SampleRange, SampleUniform}, Rng, SeedableRng};
use rand_pcg::Pcg64;
use smallvec::SmallVec;

use serde::{
    Serialize,
    Deserialize
};

use crate::{
    log,
    save::*,
    imgui_ui::UiSystem,
    pathfind::{
        self,
        Node,
        NodeKind as PathNodeKind,
        Graph,
        Search,
        SearchResult,
        Path,
        Bias,
        Unbiased,
        PathFilter,
        AStarUniformCostHeuristic
    },
    utils::{
        Seconds,
        UnsafeWeakRef,
        hash::StringHash,
        coords::{Cell, CellRange}
    },
    tile::{
        Tile,
        TileKind,
        TileMap,
        TileMapLayerKind,
        sets::{TileDef, TileSets}
    }
};

use super::{
    constants::*,
    world::World,
    system::GameSystems,
    unit::{config::UnitConfigs, task::UnitTaskManager},
    building::{
        Building,
        BuildingKind,
        config::BuildingConfigs
    }
};

pub mod debug;
pub mod resources;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Simulation<'config> {
    update_timer: UpdateTimer,
    rng: RandomGenerator,
    task_manager: UnitTaskManager,

    // Path finding:
    graph: Graph,
    #[serde(skip)] search: Search,

    // Configs:
    #[serde(skip)] building_configs: Option<&'config BuildingConfigs>,
    #[serde(skip)] unit_configs: Option<&'config UnitConfigs>,
}

impl<'config> Simulation<'config> {
    pub fn new(tile_map: &TileMap,
               building_configs: &'config BuildingConfigs,
               unit_configs: &'config UnitConfigs) -> Self {
        Self {
            update_timer: UpdateTimer::new(SIM_UPDATE_FREQUENCY_SECS),
            rng: RandomGenerator::seed_from_u64(SIM_DEFAULT_RANDOM_SEED),
            task_manager: UnitTaskManager::new(UNIT_TASK_POOL_CAPACITY),
            graph: Graph::from_tile_map(tile_map),
            search: Search::with_grid_size(tile_map.size_in_cells()),
            building_configs: Some(building_configs),
            unit_configs: Some(unit_configs),
        }
    }

    #[inline]
    pub fn new_query<'tile_sets>(&mut self,
                                 world: &mut World<'config>,
                                 tile_map: &mut TileMap<'tile_sets>,
                                 tile_sets: &'tile_sets TileSets,
                                 delta_time_secs: Seconds) -> Query<'config, 'tile_sets> {
        Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            &mut self.task_manager,
            world,
            tile_map,
            tile_sets,
            self.building_configs.unwrap(),
            self.unit_configs.unwrap(),
            delta_time_secs)
    }

    pub fn update<'tile_sets>(&mut self,
                              world: &mut World<'config>,
                              systems: &mut GameSystems,
                              tile_map: &mut TileMap<'tile_sets>,
                              tile_sets: &'tile_sets TileSets,
                              delta_time_secs: Seconds) {

        debug_assert!(self.building_configs.is_some());
        debug_assert!(self.unit_configs.is_some());

        // Rebuild the search graph once every frame so any
        // add/remove tile changes will be reflected on the graph.
        self.graph.rebuild_from_tile_map(tile_map, true);

        // Units movement needs to be smooth, so it updates every frame.
        {
            let query = self.new_query(world, tile_map, tile_sets, delta_time_secs);
            world.update_unit_navigation(&query);
        }

        // Fixed step world & systems update.
        {
            let world_update_delta_time_secs = self.update_timer.time_since_last_secs();
            if self.update_timer.tick(delta_time_secs).should_update() {
                let query = self.new_query(world, tile_map, tile_sets, world_update_delta_time_secs);
                world.update(&query);
                systems.update(&query);
            }
        }
    }

    pub fn reset<'tile_sets>(&mut self,
                             world: &mut World<'config>,
                             systems: &mut GameSystems,
                             tile_map: &mut TileMap<'tile_sets>,
                             tile_sets: &'tile_sets TileSets) {

        let query = self.new_query(world, tile_map, tile_sets, 0.0);
        world.reset(&query);
        systems.reset();
    }

    #[inline]
    pub fn building_configs(&self) -> &'config BuildingConfigs {
        self.building_configs.unwrap()
    }

    #[inline]
    pub fn unit_configs(&self) -> &'config UnitConfigs {
        self.unit_configs.unwrap()
    }

    #[inline]
    pub fn task_manager(&mut self) -> &mut UnitTaskManager {
        &mut self.task_manager
    }

    // ----------------------
    // Debug:
    // ----------------------

    // World:
    pub fn draw_world_debug_ui(&mut self, context: &mut debug::DebugContext) {
        context.world.draw_debug_ui(context.ui_sys);
    }

    // Game Systems:
    pub fn draw_game_systems_debug_ui(&mut self, context: &mut debug::DebugContext<'config, '_, '_, '_, '_>) {
        let query = self.new_query(
            context.world,
            context.tile_map,
            context.tile_sets,
            context.delta_time_secs);

        context.systems.draw_debug_ui(&query, context.ui_sys);
    }

    // Generic GameObjects:
    pub fn draw_game_object_debug_ui(&mut self,
                                     context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                     tile: &Tile,
                                     mode: debug::DebugUiMode) {

        if tile.is(TileKind::Building) {
            self.draw_building_debug_ui(context, tile, mode);
        } else if tile.is(TileKind::Unit) {
            self.draw_unit_debug_ui(context, tile, mode);
        }
    }

    pub fn draw_game_object_debug_popups(&mut self,
                                         context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                         visible_range: CellRange) {
    
        self.draw_building_debug_popups(context, visible_range);
        self.draw_unit_debug_popups(context, visible_range);
    }

    // Buildings:
    fn draw_building_debug_popups(&mut self,
                                  context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                                  visible_range: CellRange) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.tile_sets,
            context.delta_time_secs);

        context.world.draw_building_debug_popups(
            &query,
            context.ui_sys,
            &context.transform,
            visible_range);
    }

    fn draw_building_debug_ui(&mut self,
                              context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                              tile: &Tile,
                              mode: debug::DebugUiMode) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.tile_sets,
            context.delta_time_secs);

        context.world.draw_building_debug_ui(
            &query,
            context.ui_sys,
            tile,
            mode);
    }

    // Units:
    fn draw_unit_debug_popups(&mut self,
                              context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                              visible_range: CellRange) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.tile_sets,
            context.delta_time_secs);

        context.world.draw_unit_debug_popups(
            &query,
            context.ui_sys,
            &context.transform,
            visible_range);
    }

    fn draw_unit_debug_ui(&mut self,
                          context: &mut debug::DebugContext<'config, '_, '_, '_, '_>,
                          tile: &Tile,
                          mode: debug::DebugUiMode) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.tile_sets,
            context.delta_time_secs);

        context.world.draw_unit_debug_ui(
            &query,
            context.ui_sys,
            tile,
            mode);
    }
}

// ----------------------------------------------
// Save/Load
// ----------------------------------------------

impl Save for Simulation<'_> {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl<'config> Load<'_, '_, 'config> for Simulation<'config> {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext<'_, '_, 'config>) {
        self.search = Search::with_graph(&self.graph);

        self.building_configs = Some(context.building_configs);
        self.unit_configs = Some(context.unit_configs);

        self.task_manager.post_load();
    }
}

// ----------------------------------------------
// UpdateTimer
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
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

    task_manager: UnsafeWeakRef<UnitTaskManager>,

    // World & Tile Map:
    world: UnsafeWeakRef<World<'config>>,
    tile_map: UnsafeWeakRef<TileMap<'tile_sets>>,
    tile_sets: &'tile_sets TileSets,

    building_configs: &'config BuildingConfigs,
    unit_configs: &'config UnitConfigs,

    delta_time_secs: Seconds,
}

impl<'config, 'tile_sets> Query<'config, 'tile_sets> {
    fn new(rng: &mut RandomGenerator,
           graph: &mut Graph,
           search: &mut Search,
           task_manager: &mut UnitTaskManager,
           world: &mut World<'config>,
           tile_map: &mut TileMap<'tile_sets>,
           tile_sets: &'tile_sets TileSets,
           building_configs: &'config BuildingConfigs,
           unit_configs: &'config UnitConfigs,
           delta_time_secs: Seconds) -> Self {
        Self {
            rng: UnsafeWeakRef::new(rng),
            graph: UnsafeWeakRef::new(graph),
            search: UnsafeWeakRef::new(search),
            task_manager: UnsafeWeakRef::new(task_manager),
            world: UnsafeWeakRef::new(world),
            tile_map: UnsafeWeakRef::new(tile_map),
            tile_sets,
            building_configs,
            unit_configs,
            delta_time_secs,
        }
    }

    #[inline(always)]
    fn search(&self) -> &mut Search {
        self.search.mut_ref_cast()
    }

    // ----------------------
    // Public API:
    // ----------------------

    #[inline(always)]
    pub fn rng(&self) -> &mut RandomGenerator {
        self.rng.mut_ref_cast()
    }

    #[inline(always)]
    pub fn task_manager(&self) -> &mut UnitTaskManager {
        self.task_manager.mut_ref_cast()
    }

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
    pub fn random_range<T, R>(&self, range: R) -> T
        where T: SampleUniform,
              R: SampleRange<T>
    {
        self.rng().random_range(range)
    }

    #[inline(always)]
    pub fn delta_time_secs(&self) -> Seconds {
        self.delta_time_secs
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

    // ----------------------
    // World Searches:
    // ----------------------

    #[inline]
    pub fn find_nearest_road_link(&self, start_cells: CellRange) -> Option<Cell> {
        pathfind::find_nearest_road_link(self.graph(), start_cells)
    }

    #[inline]
    pub fn find_path(&self,
                     traversable_node_kinds: PathNodeKind,
                     start: Cell,
                     goal: Cell) -> SearchResult {

        self.search().find_path(self.graph(),
                                &AStarUniformCostHeuristic::new(),
                                traversable_node_kinds,
                                Node::new(start),
                                Node::new(goal))
    }

    #[inline]
    pub fn find_paths<Filter>(&self,
                              path_filter: &mut Filter,
                              max_paths: usize,
                              traversable_node_kinds: PathNodeKind,
                              start: Cell,
                              goal: Cell) -> SearchResult
        where Filter: PathFilter
    {
        self.search().find_paths(self.graph(),
                                 &AStarUniformCostHeuristic::new(),
                                 path_filter,
                                 max_paths,
                                 traversable_node_kinds,
                                 Node::new(start),
                                 Node::new(goal))
    }

    #[inline]
    pub fn find_waypoints<Filter>(&self,
                                  bias: &impl Bias,
                                  path_filter: &mut Filter,
                                  traversable_node_kinds: PathNodeKind,
                                  start: Cell,
                                  max_distance: i32) -> SearchResult
        where Filter: PathFilter
    {
        self.search().find_waypoints(self.graph(),
                                     &AStarUniformCostHeuristic::new(),
                                     bias,
                                     path_filter,
                                     traversable_node_kinds,
                                     Node::new(start),
                                     max_distance)
    }

    #[inline]
    pub fn find_path_to_node(&self,
                             bias: &impl Bias,
                             traversable_node_kinds: PathNodeKind,
                             start: Cell,
                             goal_node_kinds: PathNodeKind) -> SearchResult {

        self.search().find_path_to_node(self.graph(),
                                        &AStarUniformCostHeuristic::new(),
                                        bias,
                                        traversable_node_kinds,
                                        Node::new(start),
                                        goal_node_kinds)
    }

    pub fn find_nearest_buildings<F>(&self,
                                     start: Cell,
                                     building_kinds: BuildingKind,
                                     traversable_node_kinds: PathNodeKind,
                                     max_distance: Option<i32>,
                                     visitor_fn: F) -> Option<(&mut Building, &Path)>
        where F: FnMut(&Building, &Path) -> bool
    {
        debug_assert!(start.is_valid());
        debug_assert!(!building_kinds.is_empty());
        debug_assert!(!traversable_node_kinds.is_empty());

        if !self.graph().node_kind(Node::new(start))
            .is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            log::error!(log::channel!("sim"), "Near building search: start cell {start} is not traversable!");
            return None;
        }

        struct BuildingPathFilter<'sim, 'config, 'tile_sets, F> {
            query: &'sim Query<'config, 'tile_sets>,
            building_kinds: BuildingKind,
            traversable_node_kinds: PathNodeKind,
            visitor_fn: F,
            result_building: Option<&'sim mut Building<'config>>, // Search result.
            result_path: Option<UnsafeWeakRef<Path>>, // SAFETY: Saved for result debug validation only.
            visited_nodes: SmallVec<[Node; 32]>,
        }

        impl<F> PathFilter for BuildingPathFilter<'_, '_, '_, F>
            where F: FnMut(&Building, &Path) -> bool
        {
            fn accepts(&mut self, _index: usize, path: &Path, goal: Node) -> bool {
                if self.visited_nodes.iter().any(|node| *node == goal) {
                    // Already visited, continue searching.
                    return false;
                }

                debug_assert!(!path.is_empty());
                debug_assert!(path.last().unwrap().cell == goal.cell);

                let node_kind = self.query.graph().node_kind(goal).unwrap();
                debug_assert!(node_kind.intersects(PathNodeKind::BuildingRoadLink | PathNodeKind::BuildingAccess),
                              "Unexpected PathNodeKind: {}", node_kind);

                let neighbors = self.query.graph().neighbors(goal, PathNodeKind::Building);
                for neighbor in neighbors {
                    if let Some(building) = self.query.world().find_building_for_cell_mut(neighbor.cell, self.query.tile_map()) {
                        if building.is(self.building_kinds) {
                            let mut accept_building = false;

                            // If we're looking for buildings connected to roads, check that this road link
                            // goal actually belongs to this building. Buildings can share the same road link tile.
                            if self.traversable_node_kinds.intersects(PathNodeKind::Road) {
                                if building.road_link(self.query).is_some_and(|link| link == goal.cell) {
                                    accept_building = !(self.visitor_fn)(building, path);
                                }
                            } else {
                                accept_building = !(self.visitor_fn)(building, path);
                            }

                            if accept_building {
                                // Accept this path/goal pair and stop searching.
                                self.result_building = Some(building);
                                self.result_path = Some(UnsafeWeakRef::new(path));
                                return true;
                            }
                        }

                        self.visited_nodes.push(neighbor);
                    }
                }

                // Refuse path/goal pair and continue searching.
                false
            }
        }

        let mut building_filter = BuildingPathFilter {
            query: self,
            building_kinds,
            traversable_node_kinds,
            visitor_fn,
            result_building: None,
            result_path: None,
            visited_nodes: SmallVec::new()
        };

        let result =
            self.search().find_buildings(self.graph(),
                                         &AStarUniformCostHeuristic::new(),
                                         &Unbiased::new(),
                                         &mut building_filter,
                                         traversable_node_kinds,
                                         Node::new(start),
                                         max_distance.unwrap_or(i32::MAX));

        match result {
            SearchResult::PathFound(path_found) => {
                debug_assert!(!path_found.is_empty());

                let result_building = building_filter.result_building
                    .expect("If we've found a path we should have found a building too!");

                let result_path = building_filter.result_path
                    .expect("Path should be valid for SearchResult::PathFound!");

                debug_assert!(result_building.is(building_kinds));
                debug_assert!(result_path.as_ref() == path_found); // Must be the same.

                Some((result_building, path_found))
            },
            SearchResult::PathNotFound => None,
        }
    }

    pub fn is_near_building(&self,
                            start: Cell, // -> Cell must be traversable!
                            building_kinds: BuildingKind,
                            connected_to_road_only: bool,
                            effect_radius: i32) -> bool {

        debug_assert!(start.is_valid());
        debug_assert!(!building_kinds.is_empty());
        debug_assert!(effect_radius > 0);

        let traversable_node_kinds = {
            if connected_to_road_only {
                PathNodeKind::Road
            } else {
                PathNodeKind::Dirt | PathNodeKind::Road
            }
        };

        if !self.graph().node_kind(Node::new(start))
            .is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            log::error!(log::channel!("sim"), "Near building search: start cell {start} is not traversable!");
            return false;
        }

        self.find_nearest_buildings(
            start,
            building_kinds,
            traversable_node_kinds,
            Some(effect_radius),
            |_building, _path| {
                false // Stop iterating, we'll take the first match.
            }
        ).is_some()
    }
}
