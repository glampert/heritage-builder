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
    engine::time::{Seconds, UpdateTimer},
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
        mem,
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
    config::GameConfigs,
    world::World,
    system::GameSystems,
    unit::task::UnitTaskManager,
    building::{Building, BuildingKind}
};

pub mod debug;
pub mod resources;
use resources::GlobalTreasury;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Simulation {
    rng: RandomGenerator,
    update_timer: UpdateTimer,
    task_manager: UnitTaskManager,

    // Path finding:
    graph: Graph,
    #[serde(skip)]
    search: Search,

    treasury: GlobalTreasury,
}

impl Simulation {
    pub fn new(tile_map: &TileMap) -> Self {
        let configs = GameConfigs::get();
        Self {
            rng: RandomGenerator::seed_from_u64(configs.sim.random_seed),
            update_timer: UpdateTimer::new(configs.sim.update_frequency_secs),
            task_manager: UnitTaskManager::new(UNIT_TASK_POOL_CAPACITY),
            graph: Graph::from_tile_map(tile_map),
            search: Search::with_grid_size(tile_map.size_in_cells()),
            treasury: GlobalTreasury::new(configs.sim.starting_gold_units),
        }
    }

    #[inline]
    pub fn new_query(&mut self, world: &mut World, tile_map: &mut TileMap, delta_time_secs: Seconds) -> Query {
        Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            &mut self.task_manager,
            world,
            tile_map,
            &mut self.treasury,
            delta_time_secs)
    }

    #[inline]
    pub fn task_manager(&mut self) -> &mut UnitTaskManager {
        &mut self.task_manager
    }

    pub fn update(&mut self,
                  world: &mut World,
                  systems: &mut GameSystems,
                  tile_map: &mut TileMap,
                  delta_time_secs: Seconds) {

        // Rebuild the search graph once every frame so any
        // add/remove tile changes will be reflected on the graph.
        self.graph.rebuild_from_tile_map(tile_map, true);

        // Units movement needs to be smooth, so it updates every frame.
        {
            let query = self.new_query(world, tile_map, delta_time_secs);
            world.update_unit_navigation(&query);
        }

        // Fixed step world & systems update.
        {
            let world_update_delta_time_secs = self.update_timer.time_since_last_secs();
            if self.update_timer.tick(delta_time_secs).should_update() {
                let query = self.new_query(world, tile_map, world_update_delta_time_secs);
                world.update(&query);
                systems.update(&query);
            }
        }
    }

    pub fn reset(&mut self, world: &mut World, systems: &mut GameSystems, tile_map: &mut TileMap) {
        let query = self.new_query(world, tile_map, 0.0);
        world.reset(&query);
        systems.reset();
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        World::register_callbacks();
        GameSystems::register_callbacks();
    }

    // ----------------------
    // Debug:
    // ----------------------

    // World:
    pub fn draw_world_debug_ui(&mut self, context: &mut debug::DebugContext) {
        context.world.draw_debug_ui(&mut self.treasury, context.ui_sys);
    }

    // Game Systems:
    pub fn draw_game_systems_debug_ui(&mut self, context: &mut debug::DebugContext) {
        let query = self.new_query(
            context.world,
            context.tile_map,
            context.delta_time_secs);

        context.systems.draw_debug_ui(&query, context.ui_sys);
    }

    // Generic GameObjects:
    pub fn draw_game_object_debug_ui(&mut self,
                                     context: &mut debug::DebugContext,
                                     tile: &Tile,
                                     mode: debug::DebugUiMode) {

        if tile.is(TileKind::Building) {
            self.draw_building_debug_ui(context, tile, mode);
        } else if tile.is(TileKind::Unit) {
            self.draw_unit_debug_ui(context, tile, mode);
        }
    }

    pub fn draw_game_object_debug_popups(&mut self,
                                         context: &mut debug::DebugContext,
                                         visible_range: CellRange) {
    
        self.draw_building_debug_popups(context, visible_range);
        self.draw_unit_debug_popups(context, visible_range);
    }

    // Buildings:
    fn draw_building_debug_popups(&mut self,
                                  context: &mut debug::DebugContext,
                                  visible_range: CellRange) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.delta_time_secs);

        context.world.draw_building_debug_popups(
            &query,
            context.ui_sys,
            context.transform,
            visible_range);
    }

    fn draw_building_debug_ui(&mut self,
                              context: &mut debug::DebugContext,
                              tile: &Tile,
                              mode: debug::DebugUiMode) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.delta_time_secs);

        context.world.draw_building_debug_ui(
            &query,
            context.ui_sys,
            tile,
            mode);
    }

    // Units:
    fn draw_unit_debug_popups(&mut self,
                              context: &mut debug::DebugContext,
                              visible_range: CellRange) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.delta_time_secs);

        context.world.draw_unit_debug_popups(
            &query,
            context.ui_sys,
            context.transform,
            visible_range);
    }

    fn draw_unit_debug_ui(&mut self,
                          context: &mut debug::DebugContext,
                          tile: &Tile,
                          mode: debug::DebugUiMode) {

        let query = self.new_query(
            context.world,
            context.tile_map,
            context.delta_time_secs);

        context.world.draw_unit_debug_ui(
            &query,
            context.ui_sys,
            tile,
            mode);
    }
}

// ----------------------------------------------
// Save/Load for Simulation
// ----------------------------------------------

impl Save for Simulation {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load for Simulation {
    fn pre_load(&mut self) {
        self.task_manager.pre_load();
    }

    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        self.search = Search::with_graph(&self.graph);
        self.update_timer.post_load(GameConfigs::get().sim.update_frequency_secs);
        self.task_manager.post_load();
    }
}

// ----------------------------------------------
// Query
// ----------------------------------------------

pub struct Query {
    // SAFETY: Queries are local variables in the Simulation::update() stack,
    // so none of the pointers stored here will persist or leak outside the
    // update call stack. Storing raw pointers here makes things easier
    // since Query is only a container of references to external objects,
    // so we don't want any of these lifetimes to be associated with the
    // Query's lifetime. It also allows us to pass immutable Query refs.

    // Random generator:
    rng: mem::RawPtr<RandomGenerator>,

    // Path finding:
    graph: mem::RawPtr<Graph>,
    search: mem::RawPtr<Search>,

    // Unit tasks:
    task_manager: mem::RawPtr<UnitTaskManager>,

    // World & Tile Map:
    world: mem::RawPtr<World>,
    tile_map: mem::RawPtr<TileMap>,

    treasury: mem::RawPtr<GlobalTreasury>,
    delta_time_secs: Seconds,
}

impl Query {
    fn new(rng: &mut RandomGenerator,
           graph: &mut Graph,
           search: &mut Search,
           task_manager: &mut UnitTaskManager,
           world: &mut World,
           tile_map: &mut TileMap,
           treasury: &mut GlobalTreasury,
           delta_time_secs: Seconds) -> Self {
        Self {
            rng: mem::RawPtr::from_ref(rng),
            graph: mem::RawPtr::from_ref(graph),
            search: mem::RawPtr::from_ref(search),
            task_manager: mem::RawPtr::from_ref(task_manager),
            world: mem::RawPtr::from_ref(world),
            tile_map: mem::RawPtr::from_ref(tile_map),
            treasury: mem::RawPtr::from_ref(treasury),
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
    pub fn world(&self) -> &mut World {
        self.world.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_map(&self) -> &mut TileMap {
        self.tile_map.mut_ref_cast()
    }

    #[inline(always)]
    pub fn random_range<T, R>(&self, range: R) -> T
        where T: SampleUniform,
              R: SampleRange<T>
    {
        self.rng().random_range(range)
    }

    #[inline(always)]
    pub fn treasury(&self) -> &mut GlobalTreasury {
        self.treasury.mut_ref_cast()
    }

    #[inline(always)]
    pub fn delta_time_secs(&self) -> Seconds {
        self.delta_time_secs
    }

    #[inline]
    pub fn find_tile_def(&self,
                         layer: TileMapLayerKind,
                         category_name_hash: StringHash,
                         tile_def_name_hash: StringHash) -> Option<&'static TileDef> {

        TileSets::get().find_tile_def_by_hash(layer, category_name_hash, tile_def_name_hash)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer: TileMapLayerKind,
                     tile_kinds: TileKind) -> Option<&Tile> {

        self.tile_map().find_tile(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&self,
                         cell: Cell,
                         layer: TileMapLayerKind,
                         tile_kinds: TileKind) -> Option<&mut Tile> {

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

        struct BuildingPathFilter<'world, F> {
            query: &'world Query,
            building_kinds: BuildingKind,
            traversable_node_kinds: PathNodeKind,
            visitor_fn: F,
            result_building: Option<&'world mut Building>, // Search result.
            result_path: Option<mem::RawPtr<Path>>, // SAFETY: Saved for result debug validation only.
            visited_nodes: SmallVec<[Node; 32]>,
        }

        impl<F> PathFilter for BuildingPathFilter<'_, F>
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
                                self.result_path = Some(mem::RawPtr::from_ref(path));
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
