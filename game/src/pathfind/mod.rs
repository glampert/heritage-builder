#![allow(clippy::too_many_arguments)]
#![allow(clippy::nonminimal_bool)]

use arrayvec::ArrayVec;
use bitflags::bitflags;
use serde::Deserialize;
use priority_queue::PriorityQueue;
use rand::Rng;
use std::{cmp::Reverse, ops::{Index, IndexMut}, hash::{Hash, Hasher, DefaultHasher}};

use crate::{
    bitflags_with_display,
    utils::{
        Size,
        coords::{Cell, CellRange}
    },
    tile::{
        TileKind,
        TileFlags,
        TileMap,
        TileMapLayerKind
    }
};

#[cfg(test)]
mod tests;

// Useful references and reading material:
//  https://gabrielgambetta.com/generic-search.html
//  https://www.redblobgames.com/pathfinding/a-star/introduction.html
//  https://www.redblobgames.com/pathfinding/a-star/implementation.html
//  https://www.redblobgames.com/pathfinding/grids/algorithms.html

// ----------------------------------------------
// NodeKind
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
    pub struct NodeKind: u8 {
        const Dirt               = 1 << 0;
        const Road               = 1 << 1;
        const Water              = 1 << 2;
        const Building           = 1 << 3;
        const BuildingRoadLink   = 1 << 4;
        const BuildingAccess     = 1 << 5;
        const VacantLot          = 1 << 6;
        const SettlersSpawnPoint = 1 << 7;
    }
}

impl NodeKind {
    #[inline]
    pub const fn is_single_kind(self) -> bool {
        self.bits().count_ones() == 1
    }
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Road // Most units will only navigate on roads.
    }
}

// ----------------------------------------------
// Node
// ----------------------------------------------

type NodeCost = i32;
const NODE_COST_ZERO: NodeCost = 0;
const NODE_COST_INFINITE: NodeCost = NodeCost::MAX;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub cell: Cell,
}

impl Node {
    #[inline]
    pub const fn new(cell: Cell) -> Self {
        Self { cell }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self { cell: Cell::invalid() }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.cell.is_valid()
    }

    // 4 neighbor cells of this node's cell.
    #[inline]
    pub fn neighbors(self) -> [Node; 4] {
        [
            Node::new(Cell::new(self.cell.x + 1, self.cell.y)), // right
            Node::new(Cell::new(self.cell.x - 1, self.cell.y)), // left
            Node::new(Cell::new(self.cell.x, self.cell.y + 1)), // top
            Node::new(Cell::new(self.cell.x, self.cell.y - 1)), // bottom
        ]
    }

    #[inline]
    pub fn manhattan_distance(self, other: Node) -> i32 {
        (self.cell.x - other.cell.x).abs() + (self.cell.y - other.cell.y).abs()
    }
}

// ----------------------------------------------
// Grid
// ----------------------------------------------

// 2D grid of nodes. For each Node in the grid stores a generic payload.
// Grid can be indexed with `grid[node]`.
struct Grid<T> {
    size: Size,
    nodes: Vec<T>, // WxH nodes.
}

impl<T> Grid<T> {
    #[inline]
    fn new(size: Size, nodes: Vec<T>) -> Self {
        Self { size, nodes }
    }

    #[inline]
    fn node_payload(&self, node: Node) -> &T {
        let index = self.node_to_grid_index(node)
            .unwrap_or_else(|| panic!("Unexpected invalid grid node: {:?}", node));
        &self.nodes[index]
    }

    #[inline]
    fn node_payload_mut(&mut self, node: Node) -> &mut T {
        let index = self.node_to_grid_index(node)
            .unwrap_or_else(|| panic!("Unexpected invalid grid node: {:?}", node));
        &mut self.nodes[index]
    }

    #[inline]
    fn node_to_grid_index(&self, node: Node) -> Option<usize> {
        if !self.is_node_within_bounds(node) {
            return None;
        }
        let index = node.cell.x + (node.cell.y * self.size.width);
        Some(index as usize)
    }

    #[inline]
    fn is_node_within_bounds(&self, node: Node) -> bool {
         if (node.cell.x < 0 || node.cell.x >= self.size.width) ||
            (node.cell.y < 0 || node.cell.y >= self.size.height) {
            return false;
        }
        true
    }

    #[inline]
    fn fill(&mut self, value: T) where T: Clone {
        self.nodes.fill(value);
    }
}

// Immutable indexing
impl<T> Index<Node> for Grid<T> {
    type Output = T;

    #[inline]
    fn index(&self, node: Node) -> &Self::Output {
        self.node_payload(node)
    }
}

// Mutable indexing
impl<T> IndexMut<Node> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, node: Node) -> &mut Self::Output {
        self.node_payload_mut(node)
    }
}

// ----------------------------------------------
// Graph
// ----------------------------------------------

// Our graph is just a 2D grid of Nodes (Cells).
pub struct Graph {
    grid: Grid<NodeKind>, // WxH nodes.
}

impl Graph {
    pub fn with_empty_grid(grid_size: Size) -> Self {
        debug_assert!(grid_size.is_valid());
        let node_count = (grid_size.width * grid_size.height) as usize;
        Self { grid: Grid::new(grid_size, vec![NodeKind::empty(); node_count]) }
    }

    pub fn with_node_kind(grid_size: Size, node_kind: NodeKind) -> Self {
        debug_assert!(grid_size.is_valid());
        debug_assert!(node_kind.is_single_kind(), "Expected single node kind flag!");
        let node_count = (grid_size.width * grid_size.height) as usize;
        Self { grid: Grid::new(grid_size, vec![node_kind; node_count]) }
    }

    pub fn with_node_grid(grid_size: Size, nodes: Vec<NodeKind>) -> Self {
        debug_assert!(grid_size.is_valid());
        debug_assert!(nodes.len() == (grid_size.width * grid_size.height) as usize);
        Self { grid: Grid::new(grid_size, nodes) }
    }

    pub fn from_tile_map(tile_map: &TileMap) -> Self {
        let mut graph = Self::with_empty_grid(tile_map.size_in_cells());
        graph.rebuild_from_tile_map(tile_map, false);
        graph
    }

    pub fn rebuild_from_tile_map(&mut self, tile_map: &TileMap, full_reset_to_empty: bool) {
        // We assume size hasn't changed.
        debug_assert!(self.grid_size() == tile_map.size_in_cells());

        if full_reset_to_empty {
            self.grid.fill(NodeKind::empty());
        }

        // Construct our search graph from the terrain tiles.
        // Any building or prop is considered non-traversable.
        // Building tiles are handled specially since we need
        // then for building searches.
        tile_map.for_each_tile(TileMapLayerKind::Terrain, TileKind::Terrain,
            |tile| {
                let node = Node::new(tile.base_cell());
                let blocker_kinds =
                    TileKind::Building |
                    TileKind::Blocker  |
                    TileKind::Prop     |
                    TileKind::Vegetation;

                if let Some(blocker_tile) = tile_map.find_tile(node.cell, TileMapLayerKind::Objects, blocker_kinds) {
                    if blocker_tile.is(TileKind::Building | TileKind::Blocker) {
                        // Buildings have a node kind for building searches, but they are not traversable.
                        self.grid[node] = NodeKind::Building;

                        for_each_surrounding_cell(blocker_tile.cell_range(), |cell| {
                            if !tile_map.has_tile(cell, TileMapLayerKind::Objects, blocker_kinds) &&
                                tile_map.is_cell_within_bounds(cell) {
                                self.grid[Node::new(cell)] |= NodeKind::BuildingAccess;
                            }
                            true
                        });
                    }
                    // Else leave it empty.
                } else {
                    // If there's no blocker over this cell, set its path kind.
                    let mut path_kind = tile.path_kind();
                    if tile.has_flags(TileFlags::BuildingRoadLink) {
                        path_kind |= NodeKind::BuildingRoadLink;
                    }
                    self.grid[node] = path_kind;
                }
            });
    }

    #[inline]
    pub fn set_node_kind(&mut self, node: Node, kind: NodeKind) {
        if self.grid.is_node_within_bounds(node) {
            self.grid[node] = kind;
        }
    }

    #[inline]
    pub fn node_kind(&self, node: Node) -> Option<NodeKind> {
        if self.grid.is_node_within_bounds(node) {
            return Some(self.grid[node]);
        }
        None
    }

    #[inline]
    pub fn grid_size(&self) -> Size {
        self.grid.size
    }

    #[inline]
    pub fn neighbors(&self, node: Node, wanted_node_kinds: NodeKind) -> ArrayVec<Node, 4> {
        let mut nodes = ArrayVec::new();
        for neighbor in node.neighbors() {
            if let Some(node_kind) = self.node_kind(neighbor) {
                if node_kind.intersects(wanted_node_kinds) {
                    nodes.push(neighbor);
                }
            }
        }
        nodes
    }

    #[inline]
    pub fn find_node_with_kinds(&self, kinds: NodeKind) -> Option<Node> {
        let width  = self.grid.size.width;
        let height = self.grid.size.height;

        for y in 0..height {
            for x in 0..width {
                let node = Node::new(Cell::new(x, y));
                if self.grid[node].intersects(kinds) {
                    return Some(node);
                }
            }
        }

        None
    }
}

// ----------------------------------------------
// Heuristic
// ----------------------------------------------

pub trait Heuristic {
    // Returns the estimated cost from `node` to `goal` node.
    // For grids this is typically the Manhattan Distance.
    fn estimate_cost_to_goal(&self, graph: &Graph, node: Node, goal: Node) -> NodeCost;

    // Returns the cost of moving `from` node `to` node, AKA the Edge Cost.
    fn movement_cost(&self, graph: &Graph, from: Node, to: Node) -> NodeCost;
}

// Uniform movement cost (movement_cost() always = 1).
pub struct AStarUniformCostHeuristic;

impl AStarUniformCostHeuristic {
    #[inline] pub fn new() -> Self { Self }
}

impl Heuristic for AStarUniformCostHeuristic {
    #[inline]
    fn estimate_cost_to_goal(&self, _graph: &Graph, node: Node, goal: Node) -> NodeCost {
        // Estimating 0 here would turn A* into Dijkstra's.
        node.manhattan_distance(goal)
    }

    #[inline]
    fn movement_cost(&self, _graph: &Graph, _from: Node, _to: Node) -> NodeCost {
        // Uniform movement cost.
        // Could be a dynamic cost based on terrain kind in the future.
        1
    }
}

// ----------------------------------------------
// Bias
// ----------------------------------------------

// Bias search towards a direction.
pub trait Bias {
    #[inline] fn cost_for(&self, _start: Node, _node: Node) -> f32 { 0.0 } // unbiased default.
}

pub struct Unbiased;

impl Unbiased {
    #[inline] pub fn new() -> Self { Self }
}

impl Bias for Unbiased {}

pub struct RandomDirectionalBias {
    dir_x: f32,
    dir_y: f32,
    strength: f32,
}

impl RandomDirectionalBias {
    pub fn new<R: Rng>(rng: &mut R, min: f32, max: f32) -> Self {
        debug_assert!(min <= max);
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let strength = rng.random_range(min..max);
        Self {
            dir_x: angle.cos(),
            dir_y: angle.sin(),
            strength,
        }
    }
}

impl Bias for RandomDirectionalBias {
    fn cost_for(&self, start: Node, node: Node) -> f32 {
        let dx = (node.cell.x - start.cell.x) as f32;
        let dy = (node.cell.y - start.cell.y) as f32;

        // Alignment with preferred direction.
        let alignment = (dx * self.dir_x) + (dy * self.dir_y);

        // Subtract a small amount if aligned (makes it cheaper).
        -alignment * self.strength
    }
}

// ----------------------------------------------
// PathFilter
// ----------------------------------------------

pub trait PathFilter {
    // Accept or refuse the given path. If the path is accepted, search terminates.
    // If not paths are accepted, PathFilter::choose will be called at the end to
    // optionally choose a fallback.
    #[inline]
    fn accepts(&mut self, _index: usize, _path: &Path, _goal: Node) -> bool {
        true // accept all.
    }

    // Optionally shuffles neighboring nodes to randomize Search::find_waypoints.
    #[inline]
    fn shuffle(&mut self, _nodes: &mut [Node]) {
        // no shuffling.
    }

    // true  = If accept() rejects all paths Search still tries to return a fallback.
    // false = If accept() rejects all paths Search returns PathNotFound.
    const TAKE_FALLBACK_PATH: bool = false;

    // Choose a fallback node from the list or None. This is called once by
    // Search::find_waypoints if no other path was accepted by the filter (when TAKE_FALLBACK_PATH=true).
    #[inline]
    fn choose_fallback(&mut self, _nodes: &[Node]) -> Option<Node> {
        None
    }
}

// Default no-op filter.
pub struct DefaultPathFilter;

impl DefaultPathFilter {
    #[inline] pub fn new() -> Self { Self }
}

impl PathFilter for DefaultPathFilter {}

// ----------------------------------------------
// PathHistory
// ----------------------------------------------

const PATH_HISTORY_MAX_SIZE: usize = 4;

#[derive(Clone, Default)]
pub struct PathHistory {
    hashes: ArrayVec<u64, PATH_HISTORY_MAX_SIZE>,
}

impl PathHistory {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    #[inline]
    pub fn add_path(&mut self, path: &Path) {
        let path_hash = Self::hash_path(path);

        // Add unique entries only.
        for prev_hash in &self.hashes {
            if *prev_hash == path_hash {
                return;
            }
        }

        if self.hashes.len() == PATH_HISTORY_MAX_SIZE {
            // Pop front:
            self.hashes.swap_remove(0);
        }

        self.hashes.push(path_hash);
    }

    #[inline]
    pub fn has_path(&self, path: &Path) -> bool {
        if !self.hashes.is_empty() {
            let path_hash = Self::hash_path(path);
            for prev_hash in &self.hashes {
                if *prev_hash == path_hash {
                    return true;
                }
            }
        }
        false
    }

    #[inline]
    pub fn is_last_path_hash(&self, path_hash: u64) -> bool {
        if let Some(last_hash) = self.hashes.last() {
            if *last_hash == path_hash {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn hash_path(path: &Path) -> u64 {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    pub fn hash_path_reverse(path: &Path) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write_usize(path.len()); // Vec::hash includes the length.
        for node in path.iter().rev() {
            node.hash(&mut hasher);
        }
        hasher.finish()
    }
}

impl std::fmt::Display for PathHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[")?;
        let mut first = true;
        for hash in &self.hashes {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{:x}", *hash)?;
            first = false
        }
        write!(f, "]")
    }
}

// ----------------------------------------------
// Search
// ----------------------------------------------

pub type Path = Vec<Node>;

pub enum SearchResult<'search> {
    PathFound(&'search Path),
    PathNotFound,
}

impl SearchResult<'_> {
    #[inline] fn found(&self)     -> bool { matches!(self, Self::PathFound(_)) }
    #[inline] fn not_found(&self) -> bool { matches!(self, Self::PathNotFound) }
}

pub struct Search {
    // Reconstructed path when SearchResult == PathFound, empty otherwise.
    path: Path,

    // PriorityQueue sorts highest priority first by default,
    // but we want nodes with smallest cost first, so reverse
    // the cost order.
    frontier: PriorityQueue<Node, Reverse<NodeCost>>,
    came_from: Grid<Node>,
    cost_so_far: Grid<NodeCost>,

    // Scratchpad for find_waypoints.
    possible_waypoints: Vec<Node>,

    first_run: bool,
}

impl Search {
    pub fn with_graph(graph: &Graph) -> Self {
        Self::with_grid_size(graph.grid_size())
    }

    pub fn with_grid_size(grid_size: Size) -> Self {
        let node_count = (grid_size.width * grid_size.height) as usize;
        Self {
            path: Path::new(),
            frontier: PriorityQueue::new(),
            came_from: Grid::new(grid_size, vec![Node::invalid(); node_count]),
            cost_so_far: Grid::new(grid_size, vec![NODE_COST_INFINITE; node_count]),
            possible_waypoints: Vec::with_capacity(64),
            first_run: true,
        }
    }

    // A* graph search for the shortest path to goal.
    // Only nodes of `traversable_node_kinds` will be considered by the search.
    // Anything else is assumed not traversable and ignored.
    pub fn find_path(&mut self,
                     graph: &Graph,
                     heuristic: &impl Heuristic,
                     traversable_node_kinds: NodeKind,
                     start: Node,
                     goal: Node) -> SearchResult {

        debug_assert!(!traversable_node_kinds.is_empty());

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) ||
           !graph.node_kind(goal ).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start/end nodes are invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        while let Some((current, _)) = self.frontier.pop() {
            if current == goal {
                // Found a path! We're done.
                return self.reconstruct_path(start, goal);
            }

            let neighbors = graph.neighbors(current, traversable_node_kinds);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    let priority = new_cost + heuristic.estimate_cost_to_goal(graph, neighbor, goal);
                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        SearchResult::PathNotFound
    }

    // Searches for all paths leading to the goal.
    // Returns the first path which PathFilter accepts.
    pub fn find_paths<Filter>(&mut self,
                              graph: &Graph,
                              heuristic: &impl Heuristic,
                              path_filter: &mut Filter,
                              max_paths: usize,
                              traversable_node_kinds: NodeKind,
                              start: Node,
                              goal: Node) -> SearchResult
        where Filter: PathFilter
    {
        debug_assert!(!traversable_node_kinds.is_empty());

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) ||
           !graph.node_kind(goal ).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start/end nodes are invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            // Found a viable path.
            if current == goal {
                let valid_path = Self::try_reconstruct_path(&mut self.path, &self.came_from, start, goal);
                if valid_path && path_filter.accepts(paths_found, &self.path, goal) {
                    // Filter accepted this path, we're done.
                    return SearchResult::PathFound(&self.path);
                }

                // Else try a different path.
                self.path.clear();

                paths_found += 1;
                if paths_found >= max_paths {
                    break;
                }

                continue;
            }

            let neighbors = graph.neighbors(current, traversable_node_kinds);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                if neighbor == goal ||
                   self.cost_so_far[neighbor] == NODE_COST_INFINITE ||
                   new_cost < self.cost_so_far[neighbor] {

                    self.cost_so_far[neighbor] = new_cost;

                    let priority = new_cost + heuristic.estimate_cost_to_goal(graph, neighbor, goal);
                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        // There is at least one viable path but the filter predicate refused all paths. 
        if Filter::TAKE_FALLBACK_PATH && paths_found != 0 {
            return self.reconstruct_path(start, goal);
        }

        SearchResult::PathNotFound
    }

    // Finds any destination within the given max distance.
    // Path endpoint can be up to start+distance nodes.
    // Waypoint selection can optionally be biased and randomized to
    // produce different paths with the same start and max distance.
    pub fn find_waypoints<Filter>(&mut self,
                                  graph: &Graph,
                                  heuristic: &impl Heuristic,
                                  bias: &impl Bias,
                                  path_filter: &mut Filter,
                                  traversable_node_kinds: NodeKind,
                                  start: Node,
                                  max_distance: i32) -> SearchResult
        where Filter: PathFilter
    {
        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(max_distance > 0);

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start node is invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        debug_assert!(self.possible_waypoints.is_empty());

        while let Some((current, _)) = self.frontier.pop() {
            let dist_from_start = current.manhattan_distance(start);

            // Skip if already beyond allowed range.
            if dist_from_start > max_distance {
                continue;
            }

            // Keep track of all reachable nodes within the range.
            if current != start {
                self.possible_waypoints.push(current);
            }

            let mut neighbors = graph.neighbors(current, traversable_node_kinds);
            path_filter.shuffle(neighbors.as_mut_slice());

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic / explicit goal.
                    let bias_amount = bias.cost_for(start, neighbor);
                    let biased_priority = (new_cost as f32) + bias_amount;
                    let priority = biased_priority.round() as i32;

                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        if self.possible_waypoints.is_empty() {
            return SearchResult::PathNotFound;
        }

        // Put most distant nodes first.
        self.possible_waypoints.sort_by_key(|node| Reverse(start.manhattan_distance(*node)));

        for (index, node) in self.possible_waypoints.iter().enumerate() {
            let valid_path = Self::try_reconstruct_path(&mut self.path, &self.came_from, start, *node);
            if valid_path && path_filter.accepts(index, &self.path, *node) {
                // Filter accepted this path, we're done.
                return SearchResult::PathFound(&self.path);
            }

            // Else try a different path.
            self.path.clear();
        }

        if Filter::TAKE_FALLBACK_PATH {
            if let Some(node) = path_filter.choose_fallback(&self.possible_waypoints) {
                return self.reconstruct_path(start, node);
            }
        }

        SearchResult::PathNotFound
    }

    // Search for buildings within a max distance from a starting node.
    // Path filter is invoked for each building access tile or building
    // road link found, depending on the requested traversable node kinds.
    pub fn find_buildings<Filter>(&mut self,
                                  graph: &Graph,
                                  heuristic: &impl Heuristic,
                                  bias: &impl Bias,
                                  path_filter: &mut Filter,
                                  traversable_node_kinds: NodeKind,
                                  start: Node,
                                  max_distance: i32) -> SearchResult
        where Filter: PathFilter
    {
        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(max_distance > 0);

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start node is invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let mut destination_kinds = NodeKind::empty();
        if traversable_node_kinds.intersects(NodeKind::Road) {
            // Paved road paths:
            destination_kinds |= NodeKind::BuildingRoadLink;
        }
        if traversable_node_kinds.intersects(NodeKind::Dirt) {
            // Dirt paths:
            destination_kinds |= NodeKind::BuildingAccess;
        }

        debug_assert!(!destination_kinds.is_empty(), "Unsupported traversable node kinds: {}", traversable_node_kinds);
        let wanted_neighbor_kinds = traversable_node_kinds | destination_kinds;

        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            let dist_from_start = current.manhattan_distance(start);

            // Skip if already beyond allowed range.
            if dist_from_start > max_distance {
                continue;
            }

            if current != start {
                let current_node_kind = graph.node_kind(current).unwrap();

                // Found a possible building or its road link/access tile.
                if current_node_kind.intersects(destination_kinds) {
                    let valid_path = Self::try_reconstruct_path(&mut self.path, &self.came_from, start, current);
                    if valid_path && path_filter.accepts(paths_found, &self.path, current) {
                        // Filter accepted this path, we're done.
                        return SearchResult::PathFound(&self.path);
                    }

                    paths_found += 1;
                    self.path.clear(); // Else keep searching.
                }
            }

            let mut neighbors = graph.neighbors(current, wanted_neighbor_kinds);
            path_filter.shuffle(neighbors.as_mut_slice());

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic / explicit goal.
                    let bias_amount = bias.cost_for(start, neighbor);
                    let biased_priority = (new_cost as f32) + bias_amount;
                    let priority = biased_priority.round() as i32;

                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        SearchResult::PathNotFound
    }

    // Find path to first node matching any of the NodeKinds.
    pub fn find_path_to_node(&mut self,
                             graph: &Graph,
                             heuristic: &impl Heuristic,
                             bias: &impl Bias,
                             traversable_node_kinds: NodeKind,
                             start: Node,
                             goal_node_kinds: NodeKind) -> SearchResult {

        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(!goal_node_kinds.is_empty() && goal_node_kinds.intersects(traversable_node_kinds));

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start/end nodes are invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        while let Some((current, _)) = self.frontier.pop() {
            if current != start {
                let current_node_kind = graph.node_kind(current).unwrap();
                if current_node_kind.intersects(goal_node_kinds) {
                    // Found a desired goal node kind.
                    return self.reconstruct_path(start, current);
                }
            }

            let neighbors = graph.neighbors(current, traversable_node_kinds);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic / explicit goal.
                    let bias_amount = bias.cost_for(start, neighbor);
                    let biased_priority = (new_cost as f32) + bias_amount;
                    let priority = biased_priority.round() as i32;

                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        SearchResult::PathNotFound
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn reset(&mut self, start: Node) {
        if !self.first_run {
            // If we're reusing the Search instance, reset these to defaults.
            self.path.clear();
            self.frontier.clear();
            self.came_from.fill(Node::invalid());
            self.cost_so_far.fill(NODE_COST_INFINITE);
            self.possible_waypoints.clear();
        }
        self.first_run = false;

        self.frontier.push(start, Reverse(NODE_COST_ZERO));
        self.came_from[start] = start;
        self.cost_so_far[start] = NODE_COST_ZERO;
    }

    fn try_reconstruct_path(path: &mut Path, came_from: &Grid<Node>, start: Node, goal: Node) -> bool {
        debug_assert!(path.is_empty());

        if !came_from[goal].is_valid() {
            return false;
        }

        let mut current = goal;
        while current != start {
            path.push(current);
            current = came_from[current];
        }

        path.push(start);
        path.reverse();
        true
    }

    #[inline]
    fn reconstruct_path(&mut self, start: Node, goal: Node) -> SearchResult {
        if Self::try_reconstruct_path(&mut self.path, &self.came_from, start, goal) {
            SearchResult::PathFound(&self.path)
        } else {
            SearchResult::PathNotFound
        }
    }
}

// ----------------------------------------------
// Search Utilities
// ----------------------------------------------

pub fn find_nearest_road_link(graph: &Graph, start_cells: CellRange) -> Option<Cell> {
    let start_x = start_cells.start.x - 1;
    let start_y = start_cells.start.y - 1;
    let end_x   = start_cells.end.x   + 1;
    let end_y   = start_cells.end.y   + 1;
    let expanded_range = CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y));

    for cell in &expanded_range {
        // Skip diagonal corners.
        let is_corner =
            (cell.x == start_x && cell.y == start_y) ||
            (cell.x == start_x && cell.y == end_y)   ||
            (cell.x == end_x   && cell.y == start_y) ||
            (cell.x == end_x   && cell.y == end_y);

        if is_corner {
            continue;
        }

        if let Some(node_kind) = graph.node_kind(Node::new(cell)) {
            if node_kind.intersects(NodeKind::Road) {
                return Some(cell);
            }
        }
    }

    None
}

// Visits each cell around the center cell block, skipping the diagonal corners.
pub fn for_each_surrounding_cell(start_cells: CellRange, mut visitor_fn: impl FnMut(Cell) -> bool) {
    let start_x = start_cells.start.x - 1;
    let start_y = start_cells.start.y - 1;
    let end_x   = start_cells.end.x   + 1;
    let end_y   = start_cells.end.y   + 1;
    let expanded_range = CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y));

    for cell in &expanded_range {
        // Skip diagonal corners.
        let is_corner =
            (cell.x == start_x && cell.y == start_y) ||
            (cell.x == start_x && cell.y == end_y)   ||
            (cell.x == end_x   && cell.y == start_y) ||
            (cell.x == end_x   && cell.y == end_y);

        if is_corner {
            continue;
        }

        if !visitor_fn(cell) {
            return;
        }
    }
}

pub fn highlight_path_tiles(tile_map: &mut TileMap, path: &Path) {
    for node in path {
        if let Some(tile) = tile_map.try_tile_from_layer_mut(node.cell, TileMapLayerKind::Terrain) {
            tile.set_flags(TileFlags::Highlighted, true);
        }
    }
}

pub fn highlight_building_access_tiles(tile_map: &mut TileMap, start_cells: CellRange) {
    for_each_surrounding_cell(start_cells, |cell| {
        if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
            tile.set_flags(TileFlags::Invalidated, true);
        }
        true
    });
}
