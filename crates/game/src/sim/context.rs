#![allow(clippy::too_many_arguments)]

use std::{marker::PhantomData, ops::Deref};

use smallvec::SmallVec;
use rand::{
    Rng,
    distr::uniform::{SampleRange, SampleUniform},
};

use common::{
    Size,
    Vec2,
    coords::{Cell, CellRange, WorldToScreenTransform},
    mem::{self, RawPtr},
    hash::StringHash,
    time::Seconds,
};
use engine::log;

use super::{CityRisks, Finance, GameCalendar, RandomGenerator, SimCmds};
use crate::{
    world::{World, object::GameObject},
    building::{Building, BuildingId, BuildingKind},
    unit::{Unit, UnitId, task::UnitTaskManager},
    prop::{Prop, PropId},
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
        TilePoolIndex,
        minimap::Minimap,
        placement::{TileClearingErr, TilePlacementErr},
        sets::{TileDef, TileSets},
    },
};

// ----------------------------------------------
// SimContext / SimContextMut
// ----------------------------------------------

// What the simulation's code sees of the game while it runs: the world, the tile map, the
// finances, the calendar, the risks, the path search, the unit tasks and the RNG.
//
// SimContext is read-only: the world step's update (World::update, the game systems' update and
// calendar events) gets one, so whatever would change the world, the tile map or the
// simulation's state has to go through SimCmds. SimContextMut adds the mutable API and the
// command queue; everything else (SimCmds execution, unit navigation, spawning, the player's
// edits, debug UI) gets one. A SimContextMut derefs to a SimContext, so read-only helpers take
// `&SimContext` and serve both.
//
// Shared resources: the RNG, the path search and the unit task manager are mutated through a
// read-only context too. Updates roll the RNG and allocate unit tasks, and path queries reuse
// the search's buffers; none of that is a world or tile map edit.
//
// SAFETY: the contexts hold raw pointers, since the world's update iterates its game objects
// mutably while they read the world through the context (and spawning goes through the world
// the context points to). The `'game` lifetime ties a context to the borrows it was made from,
// so it can't outlive them: Simulation::new_sim_context borrows its parts for `'game`, and only
// the Simulation builds contexts over parts it is using itself (`from_raw`).
pub struct SimContext<'game> {
    // Random generator:
    rng: RawPtr<RandomGenerator>,

    // Path finding:
    search: RawPtr<Search>,

    // Unit tasks:
    task_manager: RawPtr<UnitTaskManager>,

    // World & Tile Map:
    world: RawPtr<World>,
    tile_map: RawPtr<TileMap>,

    // The city's treasury and accounts:
    finance: RawPtr<Finance>,

    // Date and season:
    calendar: RawPtr<GameCalendar>,

    // Whether fire and collapse risks build up:
    risks: RawPtr<CityRisks>,

    // Update delta time.
    delta_time_secs: Seconds,

    // True if the world is being reset/destroyed.
    is_world_teardown: bool,

    _borrow: PhantomData<&'game mut ()>,
}

pub struct SimContextMut<'game> {
    base: SimContext<'game>,

    // Deferred sim command queue:
    cmds: RawPtr<SimCmds>,
}

impl<'game> Deref for SimContextMut<'game> {
    type Target = SimContext<'game>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

// The parts of the simulation a context points to.
pub(super) struct SimContextParts<'a> {
    pub rng: &'a mut RandomGenerator,
    pub search: &'a mut Search,
    pub task_manager: &'a mut UnitTaskManager,
    pub world: &'a mut World,
    pub tile_map: &'a mut TileMap,
    pub finance: &'a mut Finance,
    pub calendar: &'a mut GameCalendar,
    pub risks: &'a mut CityRisks,
    pub cmds: &'a mut SimCmds,
}

impl<'game> SimContext<'game> {
    // ----------------------
    // Internal:
    // ----------------------

    // SAFETY: the caller picks `'game` and must keep every part alive and in place for it. The
    // parts' borrows end on return, so the caller may go on using them (the world's update
    // does); it must not move or drop them while the context lives.
    #[inline]
    pub(super) unsafe fn from_raw(parts: SimContextParts, delta_time_secs: Seconds, is_world_teardown: bool) -> Self {
        Self {
            rng: RawPtr::from_ref(parts.rng),
            search: RawPtr::from_ref(parts.search),
            task_manager: RawPtr::from_ref(parts.task_manager),
            world: RawPtr::from_ref(parts.world),
            tile_map: RawPtr::from_ref(parts.tile_map),
            finance: RawPtr::from_ref(parts.finance),
            calendar: RawPtr::from_ref(parts.calendar),
            risks: RawPtr::from_ref(parts.risks),
            delta_time_secs,
            is_world_teardown,
            _borrow: PhantomData,
        }
    }

    #[inline(always)]
    fn search_mut(&self) -> &mut Search {
        self.search.mut_ref_cast()
    }

    // ----------------------
    // Global Sim RNG:
    // ----------------------

    #[inline(always)]
    pub fn rng_mut(&self) -> &mut RandomGenerator {
        // NOTE: Mutable RNG access is granted even on read-only contexts.
        // The Sim RNG is a global shared resource that is not tracked as
        // a world or tile map modification.
        self.rng.mut_ref_cast()
    }

    #[inline]
    pub fn random_range<T, R>(&self, range: R) -> T
    where
        T: SampleUniform,
        R: SampleRange<T>,
    {
        self.rng_mut().random_range(range)
    }

    // ----------------------
    // Read-Only (const) API:
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
    pub fn graph(&self) -> &Graph {
        self.tile_map.graph()
    }

    #[inline(always)]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[inline(always)]
    pub fn tile_map(&self) -> &TileMap {
        &self.tile_map
    }

    #[inline(always)]
    pub fn finance(&self) -> &Finance {
        &self.finance
    }

    #[inline(always)]
    pub fn calendar(&self) -> &GameCalendar {
        &self.calendar
    }

    #[inline(always)]
    pub fn risks(&self) -> &CityRisks {
        &self.risks
    }

    #[inline]
    pub fn find_tile_def(
        &self,
        layer: TileMapLayerKind,
        category_name_hash: StringHash,
        tile_name_hash: StringHash,
    ) -> Option<&'static TileDef> {
        TileSets::get().find_tile_def_by_hash(layer, category_name_hash, tile_name_hash)
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell, tile_kinds: TileKind) -> Option<&Tile> {
        self.tile_map().find_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn try_tile_from_layer(&self, cell: Cell, layer_kind: TileMapLayerKind) -> Option<&Tile> {
        self.tile_map().try_tile_from_layer(cell, layer_kind)
    }

    #[inline]
    pub fn topmost_tile_at_cursor(&self, cursor_screen_pos: Vec2, transform: WorldToScreenTransform) -> Option<&Tile> {
        self.tile_map().topmost_tile_at_cursor(cursor_screen_pos, transform)
    }

    #[inline]
    pub fn tile_at_index(&self, index: TilePoolIndex, layer_kind: TileMapLayerKind) -> &Tile {
        self.tile_map().tile_at_index(index, layer_kind)
    }

    #[inline]
    pub fn map_size_in_cells(&self) -> Size {
        self.tile_map().size_in_cells()
    }

    // ----------------------
    // World API (read-only):
    // ----------------------

    #[inline]
    pub fn find_building(&self, kind: BuildingKind, id: BuildingId) -> Option<&Building> {
        self.world().find_building(kind, id)
    }

    #[inline]
    pub fn find_building_for_cell(&self, cell: Cell) -> Option<&Building> {
        self.world().find_building_for_cell(cell, self.tile_map())
    }

    #[inline]
    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building> {
        self.world().find_building_for_tile(tile)
    }

    #[inline]
    pub fn find_unit(&self, id: UnitId) -> Option<&Unit> {
        self.world().find_unit(id)
    }

    #[inline]
    pub fn find_unit_for_cell(&self, cell: Cell) -> Option<&Unit> {
        self.world().find_unit_for_cell(cell, self.tile_map())
    }

    #[inline]
    pub fn find_unit_for_tile(&self, tile: &Tile) -> Option<&Unit> {
        self.world().find_unit_for_tile(tile)
    }

    #[inline]
    pub fn find_prop(&self, id: PropId) -> Option<&Prop> {
        self.world().find_prop(id)
    }

    #[inline]
    pub fn find_prop_for_cell(&self, cell: Cell) -> Option<&Prop> {
        self.world().find_prop_for_cell(cell, self.tile_map())
    }

    #[inline]
    pub fn find_prop_for_tile(&self, tile: &Tile) -> Option<&Prop> {
        self.world().find_prop_for_tile(tile)
    }

    #[inline]
    pub fn find_game_object_for_tile(&self, tile: &Tile) -> Option<&dyn GameObject> {
        self.world().find_game_object_for_tile(tile)
    }

    // ----------------------
    // Pathfind (read-only):
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

    // The nearest cell next to a target, walking only `traversable_node_kinds`, within
    // `max_distance` of `start` if given (pathfind::Search::find_path_next_to).
    #[inline]
    pub fn find_path_next_to<F>(
        &self,
        traversable_node_kinds: PathNodeKind,
        start: Cell,
        max_distance: Option<i32>,
        mut is_target: F,
    ) -> SearchResult<'_>
    where
        F: FnMut(Cell) -> bool,
    {
        self.search_mut().find_path_next_to(
            self.graph(),
            &AStarUniformCostHeuristic::new(),
            traversable_node_kinds,
            Node::new(start),
            max_distance.unwrap_or(i32::MAX),
            |node| is_target(node.cell),
        )
    }

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
        debug_assert!(start.is_valid());
        debug_assert!(!building_kinds.is_empty());
        debug_assert!(!traversable_node_kinds.is_empty());

        if !self.graph().node_kind(Node::new(start)).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            log::error!(log::channel!("sim"), "Near building search: start cell {start} is not traversable!");
            return None;
        }

        struct BuildingPathFilter<'game, F> {
            context: &'game SimContext<'game>,
            building_kinds: BuildingKind,
            traversable_node_kinds: PathNodeKind,
            visitor_fn: F,
            result_building: Option<&'game Building>, // Search result.
            result_path: Option<RawPtr<Path>>,        // SAFETY: Saved for result debug validation only.
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
                    if let Some(building) = self.context.find_building_for_cell(neighbor.cell) {
                        if building.is(self.building_kinds) {
                            let mut accept_building = false;

                            // If we're looking for buildings connected to roads,
                            // check that this road link goal actually belongs to this
                            // building. Buildings can share the same road link tile.
                            if self.traversable_node_kinds.is_road() && !self.traversable_node_kinds.is_empty_land() {
                                if building.road_link().is_some_and(|link| link == goal.cell) {
                                    accept_building = !(self.visitor_fn)(building, path);
                                }
                            } else if self.traversable_node_kinds.is_empty_land() {
                                // We don't require road linked buildings.
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

                let result_path =
                    building_filter.result_path.expect("Path should be valid for SearchResult::PathFound!");

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

        let traversable_node_kinds = if connected_to_road_only {
            PathNodeKind::Road
        } else {
            PathNodeKind::EmptyLand | PathNodeKind::Road
        };

        if !self.graph().node_kind(Node::new(start)).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            log::error!(log::channel!("sim"), "Near building search: start cell {start} is not traversable!");
            return false;
        }

        self.find_nearest_buildings(
            start,
            building_kinds,
            traversable_node_kinds,
            Some(effect_radius),
            |building, _path| {
                // Stop at the first match; a construction site provides nothing yet.
                building.is_under_construction()
            },
        ).is_some()
    }

    // ----------------------
    // Unit tasks (shared):
    // ----------------------

    #[inline(always)]
    pub fn task_manager_mut(&self) -> &mut UnitTaskManager {
        // NOTE: Allow accessing mutable UnitTaskManager on a read-only context.
        // UnitTaskManager is a shared global resource used to allocate and assign
        // unit tasks, so we do not track it as a world or tile map modification.
        self.task_manager.mut_ref_cast()
    }

}

impl<'game> SimContextMut<'game> {
    // ----------------------
    // Internal:
    // ----------------------

    // SAFETY: as SimContext::from_raw.
    #[inline]
    pub(super) unsafe fn from_raw(parts: SimContextParts, delta_time_secs: Seconds, is_world_teardown: bool) -> Self {
        let cmds = RawPtr::from_ref(&*parts.cmds);
        Self { base: unsafe { SimContext::from_raw(parts, delta_time_secs, is_world_teardown) }, cmds }
    }

    // ----------------------
    // Mutable API:
    // ----------------------

    #[inline(always)]
    pub fn cmds_mut(&self) -> &mut SimCmds {
        self.cmds.mut_ref_cast()
    }

    #[inline(always)]
    pub fn world_mut(&self) -> &mut World {
        self.world.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_map_mut(&self) -> &mut TileMap {
        self.tile_map.mut_ref_cast()
    }

    #[inline(always)]
    pub fn finance_mut(&self) -> &mut Finance {
        self.finance.mut_ref_cast()
    }

    #[inline]
    pub fn risks_mut(&self) -> &mut CityRisks {
        self.risks.mut_ref_cast()
    }

    #[inline]
    pub fn find_tile_mut(&self, cell: Cell, tile_kinds: TileKind) -> Option<&mut Tile> {
        self.tile_map_mut().find_tile_mut(cell, tile_kinds)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&self, cell: Cell, layer_kind: TileMapLayerKind) -> Option<&mut Tile> {
        self.tile_map_mut().try_tile_from_layer_mut(cell, layer_kind)
    }

    #[inline]
    pub fn tile_at_index_mut(&self, index: TilePoolIndex, layer_kind: TileMapLayerKind) -> &mut Tile {
        self.tile_map_mut().tile_at_index_mut(index, layer_kind)
    }

    #[inline]
    pub fn minimap_mut(&self) -> &mut Minimap {
        self.tile_map_mut().minimap_mut()
    }

    #[inline]
    pub fn try_place_tile(
        &self,
        target_cell: Cell,
        tile_def_to_place: &'static TileDef,
    ) -> Result<&mut Tile, TilePlacementErr> {
        self.tile_map_mut().try_place_tile(target_cell, tile_def_to_place)
    }

    #[inline]
    pub fn try_clear_tile_from_layer(
        &self,
        target_cell: Cell,
        layer_kind: TileMapLayerKind,
    ) -> Result<(), TileClearingErr> {
        self.tile_map_mut().try_clear_tile_from_layer(target_cell, layer_kind)
    }

    #[inline]
    pub fn visit_next_tiles_mut<F>(&self, tile: &Tile, visitor_fn: F)
    where
        F: FnMut(&mut Tile),
    {
        self.tile_map_mut().visit_next_tiles_mut(tile, visitor_fn);
    }

    // ----------------------
    // World API (mutable):
    // ----------------------

    #[inline]
    pub fn find_building_mut(&self, kind: BuildingKind, id: BuildingId) -> Option<&mut Building> {
        self.world_mut().find_building_mut(kind, id)
    }

    #[inline]
    pub fn find_building_for_cell_mut(&self, cell: Cell) -> Option<&mut Building> {
        self.world_mut().find_building_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_building_for_tile_mut(&self, tile: &Tile) -> Option<&mut Building> {
        self.world_mut().find_building_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_unit_mut(&self, id: UnitId) -> Option<&mut Unit> {
        self.world_mut().find_unit_mut(id)
    }

    #[inline]
    pub fn find_unit_for_cell_mut(&self, cell: Cell) -> Option<&mut Unit> {
        self.world_mut().find_unit_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_unit_for_tile_mut(&self, tile: &Tile) -> Option<&mut Unit> {
        self.world_mut().find_unit_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_prop_mut(&self, id: PropId) -> Option<&mut Prop> {
        self.world_mut().find_prop_mut(id)
    }

    #[inline]
    pub fn find_prop_for_cell_mut(&self, cell: Cell) -> Option<&mut Prop> {
        self.world_mut().find_prop_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_prop_for_tile_mut(&self, tile: &Tile) -> Option<&mut Prop> {
        self.world_mut().find_prop_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_game_object_for_tile_mut(&self, tile: &Tile) -> Option<&mut dyn GameObject> {
        self.world_mut().find_game_object_for_tile_mut(tile)
    }

    // ----------------------
    // Pathfind (mutable):
    // ----------------------

    #[inline]
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
        // Reuse non mutable find_nearest_buildings().
        self.find_nearest_buildings(start, building_kinds, traversable_node_kinds, max_distance, visitor_fn)
            .map(|(building, path)| (mem::mut_ref_cast(building), path))
    }
}
