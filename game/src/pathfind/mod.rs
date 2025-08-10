use arrayvec::ArrayVec;
use bitflags::bitflags;
use serde::Deserialize;
use priority_queue::PriorityQueue;
use std::{cmp::Reverse, ops::{Index, IndexMut}};

use crate::{
    bitflags_with_display,
    utils::{
        Size,
        coords::Cell
    },
    tile::{
        TileKind,
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
        const Ground = 1 << 0;
        const Road   = 1 << 1;
        const Water  = 1 << 2;
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
    fn neighbors(self) -> [Node; 4] {
        [
            Node::new(Cell::new(self.cell.x + 1, self.cell.y)), // right
            Node::new(Cell::new(self.cell.x - 1, self.cell.y)), // left
            Node::new(Cell::new(self.cell.x, self.cell.y + 1)), // top
            Node::new(Cell::new(self.cell.x, self.cell.y - 1)), // bottom
        ]
    }

    #[inline]
    fn manhattan_distance(self, other: Node) -> i32 {
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
        debug_assert!(node_kind.bits().count_ones() == 1, "Expected single node kind flag!");
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
        // Any building or prop is considered non-traversable
        // and is not added to the graph.
        tile_map.for_each_tile(TileMapLayerKind::Terrain, TileKind::Terrain,
            |tile| {
                let node = Node::new(tile.base_cell());
                let blocker_kinds =
                    TileKind::Blocker  |
                    TileKind::Building |
                    TileKind::Prop     |
                    TileKind::Vegetation;

                // If there's no building/prop over this cell, set it's path kind.
                if !tile_map.has_tile(node.cell, TileMapLayerKind::Objects, blocker_kinds) {
                    let path_kind = tile.tile_def().path_kind;
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
    fn neighbors(&self, node: Node, wanted_node_kinds: NodeKind) -> ArrayVec<Node, 4> {
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
    #[inline]
    pub fn new() -> Self { Self }
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
            first_run: true,
        }
    }

    // A* graph search.
    // Only nodes of `traversable_node_kinds` will be considered by the search.
    // Anything else is assumed not traversable and ignored.
    #[must_use]
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

    // Finds any destination within the given max distance.
    // Path endpoint can be up to start+distance nodes.
    #[must_use]
    pub fn find_waypoint(&mut self,
                         graph: &Graph,
                         heuristic: &impl Heuristic,
                         traversable_node_kinds: NodeKind,
                         start: Node,
                         max_distance: i32) -> SearchResult {

        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(max_distance > 0);

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start node is invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let mut last_node_explored = Node::invalid();
        let mut last_node_explored_dist_from_start = NODE_COST_INFINITE;

        while let Some((current, _)) = self.frontier.pop() {
            last_node_explored = current;
            last_node_explored_dist_from_start = current.manhattan_distance(start);

            if last_node_explored_dist_from_start >= max_distance {
                // We've explored far enough, stop here.
                return self.reconstruct_path(start, current);
            }

            let neighbors = graph.neighbors(current, traversable_node_kinds);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    let priority = new_cost; // No estimate cost (no explicit goal), same as Dijkstra's search.
                    self.frontier.push(neighbor, Reverse(priority));

                    // Remember how we got here so we can backtrack.
                    self.came_from[neighbor] = current;
                }
            }
        }

        // If we've reached the end we never found a path >= max_distance, but a shorter path may still exit.
        if last_node_explored.is_valid() {
            debug_assert!(last_node_explored_dist_from_start < max_distance);
            return self.reconstruct_path(start, last_node_explored);
        }

        SearchResult::PathNotFound
    }

    fn reset(&mut self, start: Node) {
        if !self.first_run {
            // If we're reusing the Search instance, reset these to defaults.
            self.path.clear();
            self.frontier.clear();
            self.came_from.fill(Node::invalid());
            self.cost_so_far.fill(NODE_COST_INFINITE);
        }
        self.first_run = false;

        self.frontier.push(start, Reverse(NODE_COST_ZERO));
        self.came_from[start] = start;
        self.cost_so_far[start] = NODE_COST_ZERO;
    }

    fn reconstruct_path(&mut self, start: Node, goal: Node) -> SearchResult {
        if !self.came_from[goal].is_valid() {
            return SearchResult::PathNotFound;
        }

        debug_assert!(self.path.is_empty());

        let mut current = goal;
        while current != start {
            self.path.push(current);
            current = self.came_from[current];
        }

        self.path.push(start);
        self.path.reverse();

        SearchResult::PathFound(&self.path)
    }
}
