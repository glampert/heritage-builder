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

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.cell)
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
                        // Restore the underlying Terrain's path_kind so the
                        // node reflects the true tile-map state (e.g. a
                        // VacantLot/Road/Water terrain re-emerges when the
                        // Object above is removed).
                        let terrain_kind = tile_map
                            .find_tile(cell, TileKind::Terrain)
                            .map(|t| t.path_kind())
                            .unwrap_or(NodeKind::EmptyLand);
                        graph.set_node_kind(Node::new(cell), terrain_kind);
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
    pub fn vacant_lot_nodes_count(&self) -> usize {
        self.vacant_lots
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
        let had_vacant_lot  = self.grid[node].intersects(NodeKind::VacantLot);
        let has_vacant_lot  = kind.intersects(NodeKind::VacantLot);

        let had_spawn_point = self.grid[node].intersects(NodeKind::SettlersSpawnPoint);
        let has_spawn_point = kind.intersects(NodeKind::SettlersSpawnPoint);

        self.grid[node] = kind; // NOTE: Override previous.

        match (had_vacant_lot, has_vacant_lot) {
            (false, true) => self.vacant_lots += 1,
            (true, false) => {
                debug_assert!(self.vacant_lots != 0, "Search Graph does not contain any VacantLot nodes!");
                self.vacant_lots -= 1;
            }
            _ => {}
        }

        // We maintain the invariant of a single SettlersSpawnPoint node in the graph.
        if has_spawn_point {
            // Moving the spawn: strip the flag from the previous cell so the grid
            // state matches the single-spawn invariant.
            if let Some(prev) = self.settlers_spawn_point {
                if prev != node {
                    self.grid[prev].remove(NodeKind::SettlersSpawnPoint);
                }
            }
            self.settlers_spawn_point = Some(node);
        } else if had_spawn_point {
            // Overwrote the current spawn cell with a non-spawn kind.
            self.settlers_spawn_point = None;
        }
    }

    #[inline]
    fn append_node_kind_internal(&mut self, node: Node, kind: NodeKind) {
        let had_vacant_lot = self.grid[node].intersects(NodeKind::VacantLot);

        self.grid[node] |= kind; // NOTE: OR instead of assigning.

        if kind.intersects(NodeKind::VacantLot) && !had_vacant_lot {
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
        let had_vacant_lot = self.grid[node].intersects(NodeKind::VacantLot);

        self.grid[node].remove(kind); // NOTE: Clear flag.

        if kind.intersects(NodeKind::VacantLot) && had_vacant_lot {
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
    pub fn found(&self) -> bool {
        matches!(self, Self::PathFound(_))
    }

    #[inline]
    pub fn not_found(&self) -> bool {
        matches!(self, Self::PathNotFound)
    }
}

// Per-cell payload tagged with a generation counter. Cells whose
// `generation` doesn't match the current `Search::generation` are
// treated as uninitialized, so `reset()` can bump the counter
// instead of doing an O(W*H) fill of the grids on every search.
#[derive(Copy, Clone, Default)]
struct Versioned<T: Default> {
    generation: u32,
    value: T,
}

#[derive(Default)]
pub struct Search {
    // Reconstructed path when SearchResult == PathFound, empty otherwise.
    path: Path,

    // PriorityQueue sorts highest priority first by default,
    // but we want nodes with smallest cost first, so reverse
    // the cost order.
    frontier: PriorityQueue<Node, Reverse<NodeCost>>,
    came_from: Grid<Versioned<Node>>,
    cost_so_far: Grid<Versioned<NodeCost>>,

    // Bumped by `reset()`; entries with a stale generation are
    // ignored, avoiding the per-search fill of `came_from`/`cost_so_far`.
    generation: u32,

    // Scratchpad for find_waypoints.
    possible_waypoints: Vec<Node>,
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
        // Grids are initialized with generation=0. `reset()` bumps the
        // current generation, so all entries start out "stale" and read
        // back as `INFINITE`/`Node::invalid()` without any explicit fill.
        Self {
            path: Path::new(),
            frontier: PriorityQueue::new(),
            came_from: Grid::new(grid_size, vec![Versioned::<Node>::default(); node_count]),
            cost_so_far: Grid::new(grid_size, vec![Versioned::<NodeCost>::default(); node_count]),
            generation: 0,
            possible_waypoints: Vec::with_capacity(64),
        }
    }

    // A* graph search for the shortest path to goal.
    // Only nodes of `traversable_node_kinds` will be considered by the search.
    // Anything else is assumed not traversable and ignored.
    #[inline]
    pub fn find_path(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        traversable_node_kinds: NodeKind,
        start: Node,
        goal: Node,
    ) -> SearchResult<'_> {
        self.find_paths_internal::<DefaultPathFilter, false>(
            graph,
            heuristic,
            &mut DefaultPathFilter::new(),
            1, // max_paths
            traversable_node_kinds,
            start,
            goal,
        )
    }

    // Searches for all paths leading to the goal.
    // Returns the first path which PathFilter accepts.
    #[inline]
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
        self.find_paths_internal::<Filter, true>(
            graph,
            heuristic,
            path_filter,
            max_paths,
            traversable_node_kinds,
            start,
            goal,
        )
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

        if !Self::validate_endpoints(graph, traversable_node_kinds, start, None) {
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
                let movement_cost = heuristic.movement_cost(graph, current, neighbor);

                if let Some(new_cost) = self.relax_neighbor(current, neighbor, movement_cost, false) {
                    // Apply optional directional bias. With no bias this is Dijkstra's
                    // search using only node cost (no heuristic / explicit goal).
                    let bias_amount = bias.cost_for(start, neighbor);
                    let priority = ((new_cost as f32) + bias_amount).round() as i32;
                    self.frontier.push(neighbor, Reverse(priority));
                }
            }
        }

        if self.possible_waypoints.is_empty() {
            return SearchResult::PathNotFound;
        }

        // Put most distant nodes first.
        self.possible_waypoints.sort_by_key(|node| Reverse(start.manhattan_distance(*node)));

        // NOTE: Can't use `try_accept_candidate` here because iterating
        // `self.possible_waypoints` would conflict with the `&mut self`
        // borrow it requires. Inline the same logic instead — the field
        // splits keep the borrow checker happy.
        for (index, node) in self.possible_waypoints.iter().enumerate() {
            let valid_path = Self::try_reconstruct_path(
                &mut self.path, &self.came_from, self.generation, start, *node);

            if valid_path && path_filter.accepts(index, &self.path, *node) {
                return SearchResult::PathFound(&self.path);
            }
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
        debug_assert!(max_distance > 0);

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

        self.find_path_to_node_internal::<Filter, true>(
            graph,
            heuristic,
            bias,
            path_filter,
            traversable_node_kinds,
            destination_kinds,
            destination_kinds, // also widen the traversal so we can stand on the destination tiles
            start,
            max_distance,
        )
    }

    // Find path to nodes matching any of the NodeKinds.
    #[inline]
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
        debug_assert!(!goal_node_kinds.is_empty() && goal_node_kinds.intersects(traversable_node_kinds));

        self.find_path_to_node_internal::<Filter, false>(
            graph,
            heuristic,
            bias,
            path_filter,
            traversable_node_kinds,
            goal_node_kinds,
            NodeKind::empty(), // no extra neighbor kinds
            start,
            0, // max_distance ignored when HAS_MAX_DIST = false
        )
    }

    // ----------------------
    // Internal:
    // ----------------------

    // A* graph search shared by `find_path` and `find_paths`.
    // `ALLOW_GOAL_REENTRY` controls whether the goal is force-relaxed during
    // neighbor expansion (so the goal can be popped multiple times to surface
    // alternative routes). It's a const generic so the per-neighbor branch
    // folds away in the `find_path` monomorphization.
    fn find_paths_internal<Filter, const ALLOW_GOAL_REENTRY: bool>(
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

        if !Self::validate_endpoints(graph, traversable_node_kinds, start, Some(goal)) {
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            if current == goal {
                if self.try_accept_candidate(path_filter, &mut paths_found, start, goal) {
                    return SearchResult::PathFound(&self.path);
                }
                if paths_found >= max_paths {
                    break;
                }
                continue;
            }

            let neighbors = graph.neighbors(current, traversable_node_kinds);

            for neighbor in neighbors {
                let movement_cost = heuristic.movement_cost(graph, current, neighbor);

                // Re-enqueue the goal so alternative routes can be explored.
                let force_relax = ALLOW_GOAL_REENTRY && neighbor == goal;
                if let Some(new_cost) = self.relax_neighbor(current, neighbor, movement_cost, force_relax) {
                    let priority = new_cost + heuristic.estimate_cost_to_goal(graph, neighbor, goal);
                    self.frontier.push(neighbor, Reverse(priority));
                }
            }
        }

        // There is at least one viable path but the filter predicate refused all paths.
        if Filter::TAKE_FALLBACK_PATH && paths_found != 0 {
            return self.reconstruct_path(start, goal);
        }

        SearchResult::PathNotFound
    }

    // Biased Dijkstra search shared by `find_buildings` and `find_path_to_node`.
    // `goal_kinds` is the destination predicate. `extra_neighbor_kinds` lets the
    // traversal step onto destination tiles even when they're not in
    // `traversable_node_kinds` (used by `find_buildings` to walk onto BuildingAccess
    // / BuildingRoadLink). `HAS_MAX_DIST` folds away the distance check in
    // monomorphizations that don't need it.
    fn find_path_to_node_internal<Filter, const HAS_MAX_DIST: bool>(
        &mut self,
        graph: &Graph,
        heuristic: &impl Heuristic,
        bias: &impl Bias,
        path_filter: &mut Filter,
        traversable_node_kinds: NodeKind,
        goal_kinds: NodeKind,
        extra_neighbor_kinds: NodeKind,
        start: Node,
        max_distance: i32,
    ) -> SearchResult<'_>
    where
        Filter: PathFilter,
    {
        debug_assert!(!traversable_node_kinds.is_empty());
        debug_assert!(!goal_kinds.is_empty());

        if !Self::validate_endpoints(graph, traversable_node_kinds, start, None) {
            return SearchResult::PathNotFound;
        }

        self.reset(start);

        let wanted_neighbor_kinds = traversable_node_kinds | extra_neighbor_kinds;
        let mut paths_found: usize = 0;

        while let Some((current, _)) = self.frontier.pop() {
            if HAS_MAX_DIST && current.manhattan_distance(start) > max_distance {
                continue;
            }

            let current_node_kind = graph.node_kind(current).unwrap();

            if current_node_kind.intersects(goal_kinds)
                && self.try_accept_candidate(path_filter, &mut paths_found, start, current)
            {
                return SearchResult::PathFound(&self.path);
            }

            let mut neighbors = graph.neighbors(current, wanted_neighbor_kinds);
            path_filter.shuffle(&mut neighbors);

            for neighbor in neighbors {
                let movement_cost = heuristic.movement_cost(graph, current, neighbor);

                if let Some(new_cost) = self.relax_neighbor(current, neighbor, movement_cost, false) {
                    // Apply optional directional bias. With no bias this is Dijkstra's
                    // search using only node cost (no heuristic / explicit goal).
                    let bias_amount = bias.cost_for(start, neighbor);
                    let priority = ((new_cost as f32) + bias_amount).round() as i32;
                    self.frontier.push(neighbor, Reverse(priority));
                }
            }
        }

        SearchResult::PathNotFound
    }

    fn reset(&mut self, start: Node) {
        self.path.clear();
        self.frontier.clear();
        self.possible_waypoints.clear();

        // Bump the generation so every existing cell becomes stale.
        // On rollover (every ~4 billion searches) do one real fill so
        // a wrap to 0 can't collide with the grids' initial gen=0 entries.
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.came_from.fill(Versioned::<Node>::default());
            self.cost_so_far.fill(Versioned::<NodeCost>::default());
            self.generation = 1;
        }

        self.frontier.push(start, Reverse(NODE_COST_ZERO));
        self.set_came_from(start, start);
        self.set_cost(start, NODE_COST_ZERO);
    }

    // Validates that `start` (and `goal`, if provided) are in-bounds and
    // traversable. Replaces the 5 copies of the same guard at the top of
    // each public search method.
    #[inline]
    fn validate_endpoints(
        graph: &Graph,
        traversable: NodeKind,
        start: Node,
        goal: Option<Node>,
    ) -> bool {
        if !graph.node_kind(start).is_some_and(|k| k.intersects(traversable)) {
            return false;
        }
        if let Some(goal) = goal {
            if !graph.node_kind(goal).is_some_and(|k| k.intersects(traversable)) {
                return false;
            }
        }
        true
    }

    // Read `cost_so_far[node]`, returning INFINITE for stale entries.
    #[inline]
    fn cost_at(&self, node: Node) -> NodeCost {
        let v = &self.cost_so_far[node];
        if v.generation == self.generation { v.value } else { NODE_COST_INFINITE }
    }

    #[inline]
    fn set_cost(&mut self, node: Node, value: NodeCost) {
        self.cost_so_far[node] = Versioned { generation: self.generation, value };
    }

    #[inline]
    fn set_came_from(&mut self, node: Node, value: Node) {
        self.came_from[node] = Versioned { generation: self.generation, value };
    }

    // Shared inner-loop body for all 5 search methods:
    // compute the candidate cost via `movement_cost`, and if `neighbor` is
    // either unvisited (stale entry), strictly cheaper, or `force_relax`,
    // update its bookkeeping and return the new cost so the caller can
    // compute and push its priority.
    //
    // `force_relax` is the `find_paths` "re-enqueue the goal to find
    // alternative routes" case — kept as a parameter so it doesn't leak
    // into the other methods.
    #[inline]
    fn relax_neighbor(
        &mut self,
        current: Node,
        neighbor: Node,
        movement_cost: NodeCost,
        force_relax: bool,
    ) -> Option<NodeCost> {
        let new_cost = self.cost_at(current) + movement_cost;
        let neighbor_cost = self.cost_at(neighbor);
        if force_relax || neighbor_cost == NODE_COST_INFINITE || new_cost < neighbor_cost {
            self.set_cost(neighbor, new_cost);
            self.set_came_from(neighbor, current);
            Some(new_cost)
        } else {
            None
        }
    }

    // Shared "candidate goal node" handling for the 4 methods that consult a
    // `PathFilter`. Reconstructs the path to `candidate` and asks the filter
    // to accept it.
    //
    // Returns `true` when the filter accepts (caller should return PathFound).
    // On reject, increments `paths_found` and clears the in-progress `path`
    // so the search can continue.
    #[inline]
    fn try_accept_candidate<F: PathFilter>(
        &mut self,
        filter: &mut F,
        paths_found: &mut usize,
        start: Node,
        candidate: Node,
    ) -> bool {
        let valid = Self::try_reconstruct_path(
            &mut self.path, &self.came_from, self.generation, start, candidate);

        if valid && filter.accepts(*paths_found, &self.path, candidate) {
            return true;
        }

        *paths_found += 1;
        self.path.clear();
        false
    }

    fn try_reconstruct_path(
        path: &mut Path,
        came_from: &Grid<Versioned<Node>>,
        generation: u32,
        start: Node,
        goal: Node,
    ) -> bool {
        debug_assert!(path.is_empty());

        let goal_entry = &came_from[goal];
        if goal_entry.generation != generation || !goal_entry.value.is_valid() {
            return false;
        }

        let mut current = goal;
        while current != start {
            path.push(current);
            let entry = &came_from[current];
            debug_assert!(entry.generation == generation);
            current = entry.value;
        }

        path.push(start);
        path.reverse();
        true
    }

    #[inline]
    fn reconstruct_path(&mut self, start: Node, goal: Node) -> SearchResult<'_> {
        if Self::try_reconstruct_path(&mut self.path, &self.came_from, self.generation, start, goal) {
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
