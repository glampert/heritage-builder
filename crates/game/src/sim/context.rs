#![allow(clippy::too_many_arguments)]

use common::{
    coords::{Cell, CellRange},
    hash::StringHash,
    mem::RawPtr,
    time::Seconds,
};
use engine::log;
use rand::{
    Rng,
    distr::uniform::{SampleRange, SampleUniform},
};
use smallvec::SmallVec;

use super::{GlobalTreasury, RandomGenerator};
use crate::{
    building::{Building, BuildingKind},
    pathfind::{
        self,
        AStarUniformCostHeuristic,
        Bias,
        DefaultPathFilter,
        Graph,
        Node,
        NodeKind as PathNodeKind,
        Path,
        PathFilter,
        Search,
        SearchResult,
        Unbiased,
    },
    tile::{
        Tile,
        TileKind,
        TileMap,
        TileMapLayerKind,
        sets::{TileDef, TileSets},
    },
    unit::task::UnitTaskManager,
    world::World,
};

// ----------------------------------------------
// SimContext
// ----------------------------------------------

pub struct SimContext {
    // SAFETY: SimContext is a local variable in the Simulation::update() stack,
    // so none of the pointers stored here will persist or leak outside the
    // update call stack. Storing raw pointers here makes things easier
    // since SimContext is only a container of references to external objects,
    // so we don't want any of these lifetimes to be associated with the
    // SimContext's lifetime. This also allows us to pass immutable SimContext refs.

    // Random generator:
    rng: RawPtr<RandomGenerator>,

    // Path finding:
    graph: RawPtr<Graph>,
    search: RawPtr<Search>,

    // Unit tasks:
    task_manager: RawPtr<UnitTaskManager>,

    // World & Tile Map:
    world: RawPtr<World>,
    tile_map: RawPtr<TileMap>,

    treasury: RawPtr<GlobalTreasury>,
    delta_time_secs: Seconds,

    // True if the world is being reset/destroyed.
    is_world_teardown: bool,
}

impl SimContext {
    #[inline]
    pub fn new(
        rng: &mut RandomGenerator,
        graph: &mut Graph,
        search: &mut Search,
        task_manager: &mut UnitTaskManager,
        world: &mut World,
        tile_map: &mut TileMap,
        treasury: &mut GlobalTreasury,
        delta_time_secs: Seconds,
        is_world_teardown: bool,
    ) -> Self {
        Self {
            rng: RawPtr::from_ref(rng),
            graph: RawPtr::from_ref(graph),
            search: RawPtr::from_ref(search),
            task_manager: RawPtr::from_ref(task_manager),
            world: RawPtr::from_ref(world),
            tile_map: RawPtr::from_ref(tile_map),
            treasury: RawPtr::from_ref(treasury),
            delta_time_secs,
            is_world_teardown,
        }
    }

    // Internal.
    #[inline(always)]
    fn search_mut(&self) -> &mut Search {
        self.search.mut_ref_cast()
    }

    // ----------------------
    // Public API:
    // ----------------------

    #[inline(always)]
    pub fn is_world_teardown(&self) -> bool {
        self.is_world_teardown
    }

    #[inline(always)]
    pub fn delta_time_secs(&self) -> Seconds {
        self.delta_time_secs
    }

    #[inline(always)]
    pub fn task_manager(&self) -> &UnitTaskManager {
        &self.task_manager
    }

    #[inline(always)]
    pub fn task_manager_mut(&self) -> &mut UnitTaskManager {
        self.task_manager.mut_ref_cast()
    }

    #[inline(always)]
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    #[inline(always)]
    pub fn graph_mut(&self) -> &mut Graph {
        self.graph.mut_ref_cast()
    }

    #[inline(always)]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[inline(always)]
    pub fn world_mut(&self) -> &mut World {
        self.world.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_map(&self) -> &TileMap {
        &self.tile_map
    }

    #[inline(always)]
    pub fn tile_map_mut(&self) -> &mut TileMap {
        self.tile_map.mut_ref_cast()
    }

    #[inline(always)]
    pub fn treasury(&self) -> &GlobalTreasury {
        &self.treasury
    }

    #[inline(always)]
    pub fn treasury_mut(&self) -> &mut GlobalTreasury {
        self.treasury.mut_ref_cast()
    }

    #[inline(always)]
    pub fn rng_mut(&self) -> &mut RandomGenerator {
        self.rng.mut_ref_cast()
    }

    #[inline(always)]
    pub fn random_range<T, R>(&self, range: R) -> T
    where
        T: SampleUniform,
        R: SampleRange<T>,
    {
        self.rng_mut().random_range(range)
    }

    #[inline]
    pub fn find_tile_def(
        &self,
        layer: TileMapLayerKind,
        category_name_hash: StringHash,
        tile_def_name_hash: StringHash,
    ) -> Option<&'static TileDef> {
        TileSets::get().find_tile_def_by_hash(layer, category_name_hash, tile_def_name_hash)
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell, layer: TileMapLayerKind, tile_kinds: TileKind) -> Option<&Tile> {
        self.tile_map().find_tile(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&self, cell: Cell, layer: TileMapLayerKind, tile_kinds: TileKind) -> Option<&mut Tile> {
        self.tile_map_mut().find_tile_mut(cell, layer, tile_kinds)
    }

    // ----------------------
    // World Searches:
    // ----------------------

    #[inline]
    pub fn find_nearest_road_link(&self, start_cells: CellRange) -> Option<Cell> {
        pathfind::find_nearest_road_link(self.graph(), start_cells)
    }

    #[inline]
    pub fn find_path(&self, traversable_node_kinds: PathNodeKind, start: Cell, goal: Cell) -> SearchResult<'_> {
        self.search_mut().find_path(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            traversable_node_kinds,
            Node::new(start),
            Node::new(goal),
        )
    }

    #[inline]
    pub fn find_paths<Filter>(
        &self,
        path_filter: &mut Filter,
        max_paths: usize,
        traversable_node_kinds: PathNodeKind,
        start: Cell,
        goal: Cell,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        self.search_mut().find_paths(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            path_filter,
            max_paths,
            traversable_node_kinds,
            Node::new(start),
            Node::new(goal),
        )
    }

    #[inline]
    pub fn find_waypoints<Filter>(
        &self,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: PathNodeKind,
        start: Cell,
        max_distance: i32,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        self.search_mut().find_waypoints(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            bias,
            path_filter,
            traversable_node_kinds,
            Node::new(start),
            max_distance,
        )
    }

    #[inline]
    pub fn find_path_to_node(
        &self,
        bias: &impl Bias,
        traversable_node_kinds: PathNodeKind,
        start: Cell,
        goal_node_kinds: PathNodeKind,
    ) -> SearchResult<'_> {
        self.search_mut().find_path_to_node(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            bias,
            &mut DefaultPathFilter::new(),
            traversable_node_kinds,
            Node::new(start),
            goal_node_kinds,
        )
    }

    #[inline]
    pub fn find_paths_to_node<Filter>(
        &self,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: PathNodeKind,
        start: Cell,
        goal_node_kinds: PathNodeKind,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        self.search_mut().find_path_to_node(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            bias,
            path_filter,
            traversable_node_kinds,
            Node::new(start),
            goal_node_kinds,
        )
    }

    #[inline]
    pub fn find_nearest_buildings<F>(
        &self,
        start: Cell,
        building_kinds: BuildingKind,
        traversable_node_kinds: PathNodeKind,
        max_distance: Option<i32>,
        visitor_fn: F,
    ) -> Option<(&Building, &Path)>
    where
        F: FnMut(&Building, &Path) -> bool,
    {
        self.find_nearest_buildings_mut(start, building_kinds, traversable_node_kinds, max_distance, visitor_fn)
            .map(|(building, path)| (building as &Building, path))
    }

    pub fn find_nearest_buildings_mut<F>(
        &self,
        start: Cell,
        building_kinds: BuildingKind,
        traversable_node_kinds: PathNodeKind,
        max_distance: Option<i32>,
        visitor_fn: F,
    ) -> Option<(&mut Building, &Path)>
    where
        F: FnMut(&Building, &Path) -> bool,
    {
        debug_assert!(start.is_valid());
        debug_assert!(!building_kinds.is_empty());
        debug_assert!(!traversable_node_kinds.is_empty());

        if !self.graph().node_kind(Node::new(start)).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            log::error!(log::channel!("sim"), "Near building search: start cell {start} is not traversable!");
            return None;
        }

        struct BuildingPathFilter<'game, F> {
            context: &'game SimContext,
            building_kinds: BuildingKind,
            traversable_node_kinds: PathNodeKind,
            visitor_fn: F,
            result_building: Option<&'game mut Building>, // Search result.
            result_path: Option<RawPtr<Path>>,            // SAFETY: Saved for result debug validation only.
            visited_nodes: SmallVec<[Node; 32]>,
        }

        impl<F> PathFilter for BuildingPathFilter<'_, F>
        where
            F: FnMut(&Building, &Path) -> bool,
        {
            fn accepts(&mut self, _index: usize, path: &Path, goal: Node) -> bool {
                if self.visited_nodes.contains(&goal) {
                    // Already visited, continue searching.
                    return false;
                }

                debug_assert!(!path.is_empty());
                debug_assert!(path.last().unwrap().cell == goal.cell);

                let node_kind = self.context.graph().node_kind(goal).unwrap();
                debug_assert!(
                    node_kind.intersects(PathNodeKind::BuildingRoadLink | PathNodeKind::BuildingAccess),
                    "Unexpected PathNodeKind: {node_kind}"
                );

                let neighbors = self.context.graph().neighbors(goal, PathNodeKind::Building);
                for neighbor in neighbors {
                    if let Some(building) =
                        self.context.world_mut().find_building_for_cell_mut(neighbor.cell, self.context.tile_map())
                    {
                        if building.is(self.building_kinds) {
                            let mut accept_building = false;

                            // If we're looking for buildings connected to roads, check that this
                            // road link goal actually belongs to this
                            // building. Buildings can share the same road link tile.
                            if self.traversable_node_kinds.is_road() {
                                if building.road_link(self.context).is_some_and(|link| link == goal.cell) {
                                    accept_building = !(self.visitor_fn)(building, path);
                                }
                            } else {
                                accept_building = !(self.visitor_fn)(building, path);
                            }

                            if accept_building {
                                // Accept this path/goal pair and stop searching.
                                self.result_building = Some(building);
                                self.result_path = Some(RawPtr::from_ref(path));
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
            context: self,
            building_kinds,
            traversable_node_kinds,
            visitor_fn,
            result_building: None,
            result_path: None,
            visited_nodes: SmallVec::new(),
        };

        let result = self.search_mut().find_buildings(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            &Unbiased::new(),
            &mut building_filter,
            traversable_node_kinds,
            Node::new(start),
            max_distance.unwrap_or(i32::MAX),
        );

        match result {
            SearchResult::PathFound(path_found) => {
                debug_assert!(!path_found.is_empty());

                let result_building =
                    building_filter.result_building.expect("If we've found a path we should have found a building too!");

                let result_path = building_filter.result_path.expect("Path should be valid for SearchResult::PathFound!");

                debug_assert!(result_building.is(building_kinds));
                debug_assert!(result_path.as_ref() == path_found); // Must be the same.

                Some((result_building, path_found))
            }
            SearchResult::PathNotFound => None,
        }
    }

    pub fn is_near_building(
        &self,
        start: Cell, // -> Cell must be traversable!
        building_kinds: BuildingKind,
        connected_to_road_only: bool,
        effect_radius: i32,
    ) -> bool {
        debug_assert!(start.is_valid());
        debug_assert!(!building_kinds.is_empty());
        debug_assert!(effect_radius > 0);

        let traversable_node_kinds =
            { if connected_to_road_only { PathNodeKind::Road } else { PathNodeKind::EmptyLand | PathNodeKind::Road } };

        if !self.graph().node_kind(Node::new(start)).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
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
            },
        )
        .is_some()
    }
}
