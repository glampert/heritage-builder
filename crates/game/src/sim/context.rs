#![allow(clippy::too_many_arguments)]

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

use super::{GlobalTreasury, RandomGenerator, SimCmds};
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
    search: RawPtr<Search>,

    // Unit tasks:
    task_manager: RawPtr<UnitTaskManager>,

    // World & Tile Map:
    world: RawPtr<World>,
    tile_map: RawPtr<TileMap>,

    // World resource stats:
    treasury: RawPtr<GlobalTreasury>,

    // Deferred sim command queue:
    cmds: RawPtr<SimCmds>,

    // Update delta time.
    delta_time_secs: Seconds,

    // True if the world is being reset/destroyed.
    is_world_teardown: bool,

    // True if the context is in "read-only" mode (mutable methods are not allowed to be called).
    is_read_only: bool,
}

impl SimContext {
    // ----------------------
    // Internal:
    // ----------------------

    #[inline]
    pub(super) fn new(
        rng: &mut RandomGenerator,
        search: &mut Search,
        task_manager: &mut UnitTaskManager,
        world: &mut World,
        tile_map: &mut TileMap,
        treasury: &mut GlobalTreasury,
        cmds: &mut SimCmds,
        delta_time_secs: Seconds,
        is_world_teardown: bool,
        is_read_only: bool,
    ) -> Self {
        Self {
            rng: RawPtr::from_ref(rng),
            search: RawPtr::from_ref(search),
            task_manager: RawPtr::from_ref(task_manager),
            world: RawPtr::from_ref(world),
            tile_map: RawPtr::from_ref(tile_map),
            treasury: RawPtr::from_ref(treasury),
            cmds: RawPtr::from_ref(cmds),
            delta_time_secs,
            is_world_teardown,
            is_read_only,
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
    pub fn treasury(&self) -> &GlobalTreasury {
        &self.treasury
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
            context: &'game SimContext,
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
            |_building, _path| {
                false // Stop iterating, we'll take the first match.
            },
        ).is_some()
    }

    // ----------------------
    // Mutable API:
    // ----------------------

    #[inline(always)]
    pub fn task_manager_mut(&self) -> &mut UnitTaskManager {
        // NOTE: Allow accessing mutable UnitTaskManager on a read-only context.
        // UnitTaskManager is a shared global resource used to allocate and assign
        // unit tasks, so we do not track it as a world or tile map modification.
        self.task_manager.mut_ref_cast()
    }

    #[inline(always)]
    pub fn cmds_mut(&self) -> &mut SimCmds {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.cmds.mut_ref_cast()
    }

    #[inline(always)]
    pub fn world_mut(&self) -> &mut World {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world.mut_ref_cast()
    }

    #[inline(always)]
    pub fn tile_map_mut(&self) -> &mut TileMap {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map.mut_ref_cast()
    }

    #[inline(always)]
    pub fn treasury_mut(&self) -> &mut GlobalTreasury {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.treasury.mut_ref_cast()
    }

    #[inline]
    pub fn find_tile_mut(&self, cell: Cell, tile_kinds: TileKind) -> Option<&mut Tile> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().find_tile_mut(cell, tile_kinds)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&self, cell: Cell, layer_kind: TileMapLayerKind) -> Option<&mut Tile> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().try_tile_from_layer_mut(cell, layer_kind)
    }

    #[inline]
    pub fn tile_at_index_mut(&self, index: TilePoolIndex, layer_kind: TileMapLayerKind) -> &mut Tile {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().tile_at_index_mut(index, layer_kind)
    }

    #[inline]
    pub fn minimap_mut(&self) -> &mut Minimap {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().minimap_mut()
    }

    #[inline]
    pub fn try_place_tile(
        &self,
        target_cell: Cell,
        tile_def_to_place: &'static TileDef,
    ) -> Result<&mut Tile, TilePlacementErr> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().try_place_tile(target_cell, tile_def_to_place)
    }

    #[inline]
    pub fn try_clear_tile_from_layer(
        &self,
        target_cell: Cell,
        layer_kind: TileMapLayerKind,
    ) -> Result<(), TileClearingErr> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().try_clear_tile_from_layer(target_cell, layer_kind)
    }

    #[inline]
    pub fn visit_next_tiles_mut<F>(&self, tile: &Tile, visitor_fn: F)
    where
        F: FnMut(&mut Tile),
    {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.tile_map_mut().visit_next_tiles_mut(tile, visitor_fn);
    }

    // ----------------------
    // World API (mutable):
    // ----------------------

    #[inline]
    pub fn find_building_mut(&self, kind: BuildingKind, id: BuildingId) -> Option<&mut Building> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_building_mut(kind, id)
    }

    #[inline]
    pub fn find_building_for_cell_mut(&self, cell: Cell) -> Option<&mut Building> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_building_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_building_for_tile_mut(&self, tile: &Tile) -> Option<&mut Building> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_building_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_unit_mut(&self, id: UnitId) -> Option<&mut Unit> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_unit_mut(id)
    }

    #[inline]
    pub fn find_unit_for_cell_mut(&self, cell: Cell) -> Option<&mut Unit> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_unit_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_unit_for_tile_mut(&self, tile: &Tile) -> Option<&mut Unit> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_unit_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_prop_mut(&self, id: PropId) -> Option<&mut Prop> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_prop_mut(id)
    }

    #[inline]
    pub fn find_prop_for_cell_mut(&self, cell: Cell) -> Option<&mut Prop> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_prop_for_cell_mut(cell, self.tile_map())
    }

    #[inline]
    pub fn find_prop_for_tile_mut(&self, tile: &Tile) -> Option<&mut Prop> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
        self.world_mut().find_prop_for_tile_mut(tile)
    }

    #[inline]
    pub fn find_game_object_for_tile_mut(&self, tile: &Tile) -> Option<&mut dyn GameObject> {
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");
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
        debug_assert!(!self.is_read_only, "Called mutable method on a read-only SimContext!");

        // Reuse non mutable find_nearest_buildings().
        self.find_nearest_buildings(start, building_kinds, traversable_node_kinds, max_distance, visitor_fn)
            .map(|(building, path)| (mem::mut_ref_cast(building), path))
    }
}

// ----------------------------------------------
// Internal helper macros
// ----------------------------------------------

macro_rules! make_context {
    ($self:ident, $delta_time_secs:expr, $tile_map:expr, $world:expr, $is_world_teardown:expr, $is_read_only:expr) => {
        $crate::sim::context::SimContext::new(
            &mut $self.rng,
            &mut $self.search,
            &mut $self.task_manager,
            $world,
            $tile_map,
            &mut $self.treasury,
            &mut $self.cmds,
            $delta_time_secs,
            $is_world_teardown,
            $is_read_only,
        )
    };
}

macro_rules! make_update_context_readonly {
    ($self:ident, $delta_time_secs:expr, $tile_map:expr, $world:expr) => {{
        const IS_WORLD_TEARDOWN: bool = false;
        const IS_READ_ONLY: bool = true;
        $crate::sim::context::make_context!($self, $delta_time_secs, $tile_map, $world, IS_WORLD_TEARDOWN, IS_READ_ONLY)
    }};
}

macro_rules! make_update_context_mut {
    ($self:ident, $delta_time_secs:expr, $tile_map:expr, $world:expr) => {{
        const IS_WORLD_TEARDOWN: bool = false;
        const IS_READ_ONLY: bool = false;
        $crate::sim::context::make_context!($self, $delta_time_secs, $tile_map, $world, IS_WORLD_TEARDOWN, IS_READ_ONLY)
    }};
}

macro_rules! make_world_reset_context {
    ($self:ident, $tile_map:expr, $world:expr) => {{
        const IS_WORLD_TEARDOWN: bool = true;
        const IS_READ_ONLY: bool = false;
        $crate::sim::context::make_context!($self, 0.0, $tile_map, $world, IS_WORLD_TEARDOWN, IS_READ_ONLY)
    }};
}

pub(super) use { make_context, make_update_context_readonly, make_update_context_mut, make_world_reset_context };
