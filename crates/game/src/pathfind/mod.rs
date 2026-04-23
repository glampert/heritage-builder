#![allow(clippy::too_many_arguments)]
#![allow(clippy::nonminimal_bool)]

use std::{
    cmp::Reverse,
    hash::{DefaultHasher, Hash, Hasher},
    ops::{Index, IndexMut},
};
use rand::Rng;
use arrayvec::ArrayVec;
use priority_queue::PriorityQueue;
use serde::{Deserialize, Serialize};

use common::{
    Size,
    Color,
    bitflags_with_display,
    coords::{Cell, CellRange},
};
use engine::ui::UiSystem;
use crate::tile::{TileFlags, TileKind, TileMapLayerKind, TileMap};

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
    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct NodeKind: u16 {
        const EmptyLand          = 1 << 0;
        const Road               = 1 << 1;
        const Water              = 1 << 2;
        const Building           = 1 << 3;
        const BuildingAccess     = 1 << 4;
        const BuildingRoadLink   = 1 << 5;
        const VacantLot          = 1 << 6;
        const SettlersSpawnPoint = 1 << 7;
        const Rocks              = 1 << 8;
        const Vegetation         = 1 << 9;
        const HarvestableTree    = 1 << 10;
    }
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Road // Most units will only navigate on roads.
    }
}

impl NodeKind {
    #[inline]
    pub const fn is_single_kind(self) -> bool {
        self.bits().count_ones() == 1
    }

    #[inline]
    pub fn is_road(self) -> bool {
        self.intersects(Self::Road)
    }

    #[inline]
    pub fn is_empty_land(self) -> bool {
        self.intersects(Self::EmptyLand)
    }

    #[inline]
    pub fn is_land(self) -> bool {
        // Anything besides water.
        !self.intersects(Self::Water)
    }

    #[inline]
    pub fn is_water(self) -> bool {
        self.intersects(Self::Water)
    }

    #[inline]
    pub fn is_building(self) -> bool {
        self.intersects(Self::Building)
    }

    #[inline]
    pub fn is_vacant_lot(self) -> bool {
        self.intersects(Self::VacantLot)
    }

    #[inline]
    pub fn is_rocks(self) -> bool {
        self.intersects(Self::Rocks)
    }

    #[inline]
    pub fn is_vegetation(self) -> bool {
        self.intersects(Self::Vegetation)
    }

    #[inline]
    pub fn is_harvestable_tree(self) -> bool {
        self.intersects(Self::HarvestableTree)
    }

    #[inline]
    pub fn is_prop(self) -> bool {
        self.intersects(Self::Rocks | Self::Vegetation)
    }

    #[inline]
    pub fn is_harvestable_prop(self) -> bool {
        self.is_harvestable_tree()
    }

    #[inline]
    pub fn is_unit_placeable(self) -> bool {
        self.intersects(Self::EmptyLand | Self::Road | Self::VacantLot | Self::SettlersSpawnPoint)
    }

    #[inline]
    pub fn is_flying_object_placeable(self) -> bool {
        self.intersects(Self::Water
                      | Self::EmptyLand
                      | Self::Road
                      | Self::VacantLot
                      | Self::SettlersSpawnPoint
                      | Self::Building
                      | Self::Vegetation
                      | Self::HarvestableTree
                      | Self::Rocks)
    }

    #[inline]
    pub fn is_object_placeable(self) -> bool {
        self.intersects(Self::EmptyLand)
    }

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        macro_rules! node_kind_ui_checkbox {
            ($ui:ident, $node_kind:ident, $flag_name:ident) => {
                let mut value = $node_kind.intersects(NodeKind::$flag_name);
                $ui.checkbox(stringify!($flag_name), &mut value);
                $node_kind.set(NodeKind::$flag_name, value);
            };
        }

        let ui = ui_sys.ui();
        node_kind_ui_checkbox!(ui, self, EmptyLand);
        node_kind_ui_checkbox!(ui, self, Road);
        node_kind_ui_checkbox!(ui, self, Water);
        node_kind_ui_checkbox!(ui, self, Building);
        node_kind_ui_checkbox!(ui, self, BuildingRoadLink);
        node_kind_ui_checkbox!(ui, self, BuildingAccess);
        node_kind_ui_checkbox!(ui, self, VacantLot);
        node_kind_ui_checkbox!(ui, self, SettlersSpawnPoint);
        node_kind_ui_checkbox!(ui, self, Rocks);
        node_kind_ui_checkbox!(ui, self, Vegetation);
        node_kind_ui_checkbox!(ui, self, HarvestableTree);
    }

    pub fn debug_color(self) -> Color {
        macro_rules! map_to_color {
            ($node_kind:ident, $flag_name:ident, $color:expr) => {
                if $node_kind.intersects(NodeKind::$flag_name) {
                    return $color;
                }
            };
        }

        // NOTE: If multiple flags are set, higher on this list will match first and win.
        map_to_color!(self, BuildingAccess,     Color::new(0.50, 0.50, 0.50, 1.0)); // light gray
        map_to_color!(self, BuildingRoadLink,   Color::new(1.00, 0.00, 0.00, 1.0)); // red
        map_to_color!(self, Building,           Color::new(0.66, 0.23, 0.74, 1.0)); // purple
        map_to_color!(self, VacantLot,          Color::new(0.00, 0.90, 0.90, 1.0)); // cyan
        map_to_color!(self, SettlersSpawnPoint, Color::new(0.66, 0.13, 0.13, 1.0)); // dark red
        map_to_color!(self, Rocks,              Color::new(0.20, 0.20, 0.20, 1.0)); // dark gray
        map_to_color!(self, HarvestableTree,    Color::new(0.10, 0.85, 0.15, 1.0)); // green
        map_to_color!(self, Vegetation,         Color::new(0.00, 0.45, 0.00, 1.0)); // dark green
        map_to_color!(self, Road,               Color::new(0.60, 0.40, 0.30, 1.0)); // brown
        map_to_color!(self, EmptyLand,          Color::new(0.82, 0.88, 0.07, 1.0)); // bright yellow
        map_to_color!(self, Water,              Color::new(0.11, 0.39, 0.45, 1.0)); // dark blue

        Color::black() // fallback: NodeKind::empty()
    }
}

// ----------------------------------------------
// Node
// ----------------------------------------------

type NodeCost = i32;
const NODE_COST_ZERO: NodeCost = 0;
const NODE_COST_INFINITE: NodeCost = NodeCost::MAX;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Default, Serialize, Deserialize)]
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
        let index = self.node_to_grid_index(node).unwrap_or_else(|| panic!("Unexpected invalid grid node: {:?}", node));
        &self.nodes[index]
    }

    #[inline]
    fn node_payload_mut(&mut self, node: Node) -> &mut T {
        let index = self.node_to_grid_index(node).unwrap_or_else(|| panic!("Unexpected invalid grid node: {:?}", node));
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
        if (node.cell.x < 0 || node.cell.x >= self.size.width) || (node.cell.y < 0 || node.cell.y >= self.size.height) {
            return false;
        }
        true
    }

    #[inline]
    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
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
// GraphUpdateAction
// ----------------------------------------------

pub enum GraphUpdateAction {
    TilePlaced(CellRange, TileMapLayerKind, NodeKind),
    TileCleared(CellRange, TileMapLayerKind, TileKind),
    TileDefEdited(CellRange, TileMapLayerKind, TileKind, NodeKind),
    TileMoved(CellRange, CellRange, TileMapLayerKind, TileKind, NodeKind),
    TileFlagsChanged(CellRange, TileFlags, NodeKind),
}

// ----------------------------------------------
// Graph
// ----------------------------------------------

// Our search graph is just a 2D grid of Nodes (Cells).
#[derive(Default)]
pub struct Graph {
    grid: Grid<NodeKind>,               // WxH nodes grid.
    vacant_lots: usize,                 // VacantLot count.
    settlers_spawn_point: Option<Node>, // Cached SettlersSpawnPoint for fast query.
}

impl Graph {
    pub fn with_empty_grid(grid_size: Size) -> Self {
        debug_assert!(grid_size.is_valid());
        let node_count = (grid_size.width * grid_size.height) as usize;
        Self {
            grid: Grid::new(grid_size, vec![NodeKind::empty(); node_count]),
            vacant_lots: 0,
            settlers_spawn_point: None,
        }
    }

    pub fn with_node_kind(grid_size: Size, node_kind: NodeKind) -> Self {
        debug_assert!(grid_size.is_valid());
        debug_assert!(node_kind.is_single_kind(), "Expected single node kind flag!");
        debug_assert!(!node_kind.intersects(NodeKind::SettlersSpawnPoint), "SettlersSpawnPoint cannot be specified here!");

        let node_count = (grid_size.width * grid_size.height) as usize;
        Self {
            grid: Grid::new(grid_size, vec![node_kind; node_count]),
            vacant_lots: if node_kind.intersects(NodeKind::VacantLot) { node_count } else { 0 },
            settlers_spawn_point: None,
        }
    }

    pub fn with_node_grid(grid_size: Size, nodes: Vec<NodeKind>) -> Self {
        debug_assert!(grid_size.is_valid());
        debug_assert!(nodes.len() == (grid_size.width * grid_size.height) as usize);

        let mut vacant_lots = 0;
        let mut settlers_spawn_point = None;

        for y in 0..grid_size.height {
            for x in 0..grid_size.width {
                let node_kind = nodes[(x + (y * grid_size.width)) as usize];
                if node_kind.intersects(NodeKind::VacantLot) {
                    vacant_lots += 1;
                }
                if node_kind.intersects(NodeKind::SettlersSpawnPoint) {
                    debug_assert!(settlers_spawn_point.is_none(), "Cannot have multiple SettlersSpawnPoint nodes!");
                    settlers_spawn_point = Some(Node::new(Cell::new(x, y)));
                }
            }
        }

        Self {
            grid: Grid::new(grid_size, nodes),
            vacant_lots,
            settlers_spawn_point,
        }
    }

    pub fn from_tile_map(tile_map: &TileMap) -> Self {
        if tile_map.size_in_cells().is_valid() {
            let mut graph = Self::with_empty_grid(tile_map.size_in_cells());
            graph.rebuild_from_tile_map(tile_map);
            graph
        } else {
            Self::default()
        }
    }

    pub fn clear(&mut self) {
        self.grid.fill(NodeKind::empty());
        self.vacant_lots = 0;
        self.settlers_spawn_point = None;
    }

    pub fn rebuild_from_tile_map(&mut self, tile_map: &TileMap) {
        // We assume size hasn't changed.
        debug_assert_eq!(self.grid_size(), tile_map.size_in_cells());

        // Terrain layer:
        tile_map.for_each_tile(TileKind::Terrain, |tile_map, tile| {
            self.update(tile_map, GraphUpdateAction::TilePlaced(
                tile.cell_range(),
                TileMapLayerKind::Terrain,
                tile.path_kind(),
            ));
        });

        // Objects layer:
        tile_map.for_each_tile(Self::OBJECT_KINDS, |tile_map, tile| {
            self.update(tile_map, GraphUpdateAction::TilePlaced(
                tile.cell_range(),
                TileMapLayerKind::Objects,
                tile.path_kind(),
            ));
        });
    }

    pub fn update(&mut self, tile_map: &TileMap, action: GraphUpdateAction) {
        fn tile_placed(
            graph: &mut Graph,
            tile_map: &TileMap,
            cell_range: CellRange,
            layer_kind: TileMapLayerKind,
            path_kind: NodeKind,
        ) {
            match layer_kind {
                TileMapLayerKind::Terrain => {
                    // Terrain tiles always occupy a single cell.
                    graph.set_node_kind(Node::new(cell_range.start), path_kind);
                }
                TileMapLayerKind::Objects => {
                    for cell in &cell_range {
                        graph.set_node_kind(Node::new(cell), path_kind);
                    }

                    if path_kind.is_building() {
                        debug_assert!(layer_kind == TileMapLayerKind::Objects);

                        // Add surrounding building access nodes:
                        for_each_surrounding_cell(cell_range, |cell| {
                            if !tile_map.has_tile(cell, Graph::OBJECT_KINDS)
                                && tile_map.is_cell_within_bounds(cell)
                            {
                                graph.append_node_kind_internal(Node::new(cell), NodeKind::BuildingAccess);
                            }
                            true // Continue to next.
                        });
                    }
                }
            }
        }

        fn tile_cleared(
            graph: &mut Graph,
            tile_map: &TileMap,
            cell_range: CellRange,
            layer_kind: TileMapLayerKind,
            tile_kind: TileKind,
        ) {
            debug_assert!(layer_kind == tile_kind.layer_kind());

            match layer_kind {
                TileMapLayerKind::Terrain => {
                    // Terrain tiles always occupy a single cell.
                    graph.set_node_kind(Node::new(cell_range.start), NodeKind::empty());
                }
                TileMapLayerKind::Objects => {
                    for cell in &cell_range {
                        graph.set_node_kind(Node::new(cell), NodeKind::EmptyLand);
                    }

                    if tile_kind.intersects(TileKind::Building | TileKind::Blocker) {
                        // Clear surrounding building access nodes:
                        for_each_surrounding_cell(cell_range, |cell| {
                            if !tile_map.has_tile(cell, Graph::OBJECT_KINDS)
                                && tile_map.is_cell_within_bounds(cell)
                            {
                                graph.clear_node_kind_internal(Node::new(cell), NodeKind::BuildingAccess);
                            }
                            true // Continue to next.
                        });
                    }
                }
            }
        }

        match action {
            GraphUpdateAction::TilePlaced(cell_range, layer_kind, path_kind) => {
                tile_placed(self, tile_map, cell_range, layer_kind, path_kind);
            }
            GraphUpdateAction::TileCleared(cell_range, layer_kind, tile_kind) => {
                tile_cleared(self, tile_map, cell_range, layer_kind, tile_kind);
            }
            GraphUpdateAction::TileDefEdited(cell_range, layer_kind, tile_kind, path_kind) => {
                // NOTE: Simulate TileCleared followed by TilePlaced to refresh the graph state.
                tile_cleared(self, tile_map, cell_range, layer_kind, tile_kind);
                tile_placed(self,  tile_map, cell_range, layer_kind, path_kind);
            }
            GraphUpdateAction::TileMoved(from_cells, to_cells, layer_kind, tile_kind, path_kind) => {
                debug_assert!(from_cells.size() == to_cells.size());
                // NOTE: Same as TileCleared(from_cells) followed by TilePlaced(to_cells).
                tile_cleared(self, tile_map, from_cells, layer_kind, tile_kind);
                tile_placed(self,  tile_map, to_cells,   layer_kind, path_kind);
            }
            GraphUpdateAction::TileFlagsChanged(cell_range, new_flags, path_kind) => {
                if Self::tile_flags_affect_node_kind(new_flags) {
                    self.set_node_kind(Node::new(cell_range.start), path_kind);
                }
            }
        }
    }

    // True if the given TileFlags affect NodeKind flags and a Graph update should be performed.
    #[inline]
    pub fn tile_flags_affect_node_kind(flags: TileFlags) -> bool {
        flags.intersects(TileFlags::BuildingRoadLink | TileFlags::SettlersSpawnPoint)
    }

    #[inline]
    pub fn set_node_kind(&mut self, node: Node, kind: NodeKind) {
        if self.grid.is_node_within_bounds(node) {
            self.set_node_kind_internal(node, kind);
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
    pub fn has_vacant_lot_nodes(&self) -> bool {
        self.vacant_lots != 0
    }

    #[inline]
    pub fn settlers_spawn_point(&self) -> Option<Node> {
        self.settlers_spawn_point
    }

    #[inline]
    pub fn memory_usage_estimate(&self) -> usize {
        self.grid.nodes.len() * std::mem::size_of::<NodeKind>()
    }

    // ----------------------
    // Internal:
    // ----------------------

    // We construct our search graph from the terrain tiles.
    // Any building or prop is considered non-traversable.
    // Building tiles are handled specially since we need
    // then for building searches.
    const OBJECT_KINDS: TileKind = TileKind::from_bits_retain(
        TileKind::Building.bits() |
        TileKind::Blocker.bits()  |
        TileKind::Rocks.bits()    |
        TileKind::Vegetation.bits()
    );

    #[inline]
    fn set_node_kind_internal(&mut self, node: Node, kind: NodeKind) {
        self.grid[node] = kind; // NOTE: Override previous.

        if kind.intersects(NodeKind::VacantLot) {
            self.vacant_lots += 1;
        }

        // We can have a single SettlersSpawnPoint node.
        if kind.intersects(NodeKind::SettlersSpawnPoint) {
            debug_assert!(self.settlers_spawn_point.is_none(), "Cannot have multiple SettlersSpawnPoint nodes!");
            self.settlers_spawn_point = Some(node);
        }
    }

    #[inline]
    fn append_node_kind_internal(&mut self, node: Node, kind: NodeKind) {
        self.grid[node] |= kind; // NOTE: OR instead of assigning.

        if kind.intersects(NodeKind::VacantLot) {
            self.vacant_lots += 1;
        }

        // We can have a single SettlersSpawnPoint node.
        if kind.intersects(NodeKind::SettlersSpawnPoint) {
            debug_assert!(self.settlers_spawn_point.is_none(), "Cannot have multiple SettlersSpawnPoint nodes!");
            self.settlers_spawn_point = Some(node);
        }
    }

    #[inline]
    fn clear_node_kind_internal(&mut self, node: Node, kind: NodeKind) {
        self.grid[node].remove(kind); // NOTE: Clear flag.

        if kind.intersects(NodeKind::VacantLot) {
            debug_assert!(self.vacant_lots != 0, "Search Graph does not contain any VacantLot nodes!");
            self.vacant_lots -= 1;
        }

        // We can have a single SettlersSpawnPoint node.
        if kind.intersects(NodeKind::SettlersSpawnPoint) {
            self.settlers_spawn_point = None;
        }
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
    pub fn new() -> Self {
        Self
    }
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
    #[inline]
    fn cost_for(&self, _start: Node, _node: Node) -> f32 {
        0.0 // unbiased default.
    }
}

pub struct Unbiased;

impl Unbiased {
    #[inline]
    pub fn new() -> Self {
        Self
    }
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
        Self { dir_x: angle.cos(), dir_y: angle.sin(), strength }
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

    // true  = If accept() rejects all paths Search still tries to return a
    // fallback. false = If accept() rejects all paths Search returns
    // PathNotFound.
    const TAKE_FALLBACK_PATH: bool = false;

    // Choose a fallback node from the list or None. This is called once by
    // Search::find_waypoints if no other path was accepted by the filter (when
    // TAKE_FALLBACK_PATH=true).
    #[inline]
    fn choose_fallback(&mut self, _nodes: &[Node]) -> Option<Node> {
        None
    }
}

// Default no-op filter.
pub struct DefaultPathFilter;

impl DefaultPathFilter {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl PathFilter for DefaultPathFilter {}

// ----------------------------------------------
// PathHistory
// ----------------------------------------------

const PATH_HISTORY_MAX_SIZE: usize = 4;

#[derive(Clone, Default, Serialize, Deserialize)]
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
    #[inline]
    fn found(&self) -> bool {
        matches!(self, Self::PathFound(_))
    }
    #[inline]
    fn not_found(&self) -> bool {
        matches!(self, Self::PathNotFound)
    }
}

#[derive(Default)]
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
        if graph.grid_size().is_valid() {
            Self::with_grid_size(graph.grid_size())
        } else {
            Self::default()
        }
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
    pub fn find_path(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        traversable_node_kinds: NodeKind,
        start: Node,
        goal: Node,
    ) -> SearchResult<'_> {
        debug_assert!(!traversable_node_kinds.is_empty());

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds))
            || !graph.node_kind(goal).is_some_and(|kind| kind.intersects(traversable_node_kinds))
        {
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

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper
                // node to explore, we'll visit it.
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
    pub fn find_paths<Filter>(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        path_filter: &mut Filter,
        max_paths: usize,
        traversable_node_kinds: NodeKind,
        start: Node,
        goal: Node,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        debug_assert!(!traversable_node_kinds.is_empty());

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds))
            || !graph.node_kind(goal).is_some_and(|kind| kind.intersects(traversable_node_kinds))
        {
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

                if neighbor == goal
                    || self.cost_so_far[neighbor] == NODE_COST_INFINITE
                    || new_cost < self.cost_so_far[neighbor]
                {
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
    pub fn find_waypoints<Filter>(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: NodeKind,
        start: Node,
        max_distance: i32,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
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
            path_filter.shuffle(&mut neighbors);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper
                // node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic /
                    // explicit goal.
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
    pub fn find_buildings<Filter>(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: NodeKind,
        start: Node,
        max_distance: i32,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
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
        if traversable_node_kinds.intersects(NodeKind::EmptyLand) {
            // Empty land paths:
            destination_kinds |= NodeKind::BuildingAccess;
        }

        debug_assert!(!destination_kinds.is_empty(), "Unsupported traversable node kinds: {traversable_node_kinds}");

        let wanted_neighbor_kinds = traversable_node_kinds | destination_kinds;
        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            let dist_from_start = current.manhattan_distance(start);

            // Skip if already beyond allowed range.
            if dist_from_start > max_distance {
                continue;
            }

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

            let mut neighbors = graph.neighbors(current, wanted_neighbor_kinds);
            path_filter.shuffle(&mut neighbors);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper
                // node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic /
                    // explicit goal.
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

    // Find path to nodes matching any of the NodeKinds.
    pub fn find_path_to_node<Filter>(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: NodeKind,
        start: Node,
        goal_node_kinds: NodeKind,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(!goal_node_kinds.is_empty() && goal_node_kinds.intersects(traversable_node_kinds));

        if !graph.node_kind(start).is_some_and(|kind| kind.intersects(traversable_node_kinds)) {
            // Start node is invalid or not traversable!
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            let current_node_kind = graph.node_kind(current).unwrap();

            // Found a desired goal node kind:
            if current_node_kind.intersects(goal_node_kinds) {
                let valid_path = Self::try_reconstruct_path(&mut self.path, &self.came_from, start, current);
                if valid_path && path_filter.accepts(paths_found, &self.path, current) {
                    // Filter accepted this path, we're done.
                    return SearchResult::PathFound(&self.path);
                }

                paths_found += 1;
                self.path.clear(); // Else keep searching.
            }

            let mut neighbors = graph.neighbors(current, traversable_node_kinds);
            path_filter.shuffle(&mut neighbors);

            for neighbor in neighbors {
                let new_cost = self.cost_so_far[current] + heuristic.movement_cost(graph, current, neighbor);

                // If neighbor cost in INF, we haven't visited it yet, or if it is a cheaper
                // node to explore, we'll visit it.
                if self.cost_so_far[neighbor] == NODE_COST_INFINITE || new_cost < self.cost_so_far[neighbor] {
                    self.cost_so_far[neighbor] = new_cost;

                    // Apply optional directional bias:
                    // If no bias uses only node cost, i.e. Dijkstra's search, no heuristic /
                    // explicit goal.
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
    fn reconstruct_path(&mut self, start: Node, goal: Node) -> SearchResult<'_> {
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
    let end_x = start_cells.end.x + 1;
    let end_y = start_cells.end.y + 1;
    let expanded_range = CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y));

    for cell in &expanded_range {
        // Skip diagonal corners.
        let is_corner = (cell.x == start_x && cell.y == start_y)
            || (cell.x == start_x && cell.y == end_y)
            || (cell.x == end_x && cell.y == start_y)
            || (cell.x == end_x && cell.y == end_y);

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
    let end_x = start_cells.end.x + 1;
    let end_y = start_cells.end.y + 1;
    let expanded_range = CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y));

    for cell in &expanded_range {
        // Skip diagonal corners.
        let is_corner = (cell.x == start_x && cell.y == start_y)
            || (cell.x == start_x && cell.y == end_y)
            || (cell.x == end_x && cell.y == start_y)
            || (cell.x == end_x && cell.y == end_y);

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
        tile_map.set_tile_flags(node.cell, TileKind::Terrain, TileFlags::Highlighted, true);
    }
}

pub fn highlight_building_access_tiles(tile_map: &mut TileMap, start_cells: CellRange) {
    for_each_surrounding_cell(start_cells, |cell| {
        tile_map.set_tile_flags(cell, TileKind::Terrain, TileFlags::Invalidated, true);
        true
    });
}
