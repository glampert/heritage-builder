use std::iter::Rev;
use std::ops::RangeInclusive;
use std::sync::LazyLock;

use crate::{
    pathfind::NodeKind as PathNodeKind,
    utils::{coords::Cell, hash::StrHashPair},
    tile::{
        TileKind, TileFlags, TileMap, TileMapLayerKind,
        sets::{TileDef, TileSets, TERRAIN_GROUND_CATEGORY},
    },
};

// ----------------------------------------------
// RoadKind
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub enum RoadKind {
    #[default]
    Dirt,
    Paved,
}

// ----------------------------------------------
// RoadSegment
// ----------------------------------------------

#[derive(Default)]
pub struct RoadSegment {
    pub path: Vec<Cell>,
    pub kind: RoadKind,
    pub is_valid: bool,
}

impl RoadSegment {
    #[inline]
    fn valid(path: Vec<Cell>, kind: RoadKind) -> Self {
        Self { path, kind, is_valid: true }
    }

    #[inline]
    fn invalid(path: Vec<Cell>, kind: RoadKind) -> Self {
        Self { path, kind, is_valid: false }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.path.clear();
        self.is_valid = false;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    #[inline]
    pub fn cost(&self) -> u32 {
        // All road tiles have the same cost.
        (self.path.len() as u32) * self.tile_def().cost
    }

    #[inline]
    pub fn tile_def(&self) -> &'static TileDef {
        self::tile_def(self.kind)
    }
}

// ----------------------------------------------
// Road Placement API
// ----------------------------------------------

#[inline]
pub const fn tile_name(kind: RoadKind) -> StrHashPair {
    match kind {
        RoadKind::Dirt  => StrHashPair::from_str("dirt_road"),
        RoadKind::Paved => StrHashPair::from_str("paved_road"),
    }
}

#[inline]
pub fn tile_def(kind: RoadKind) -> &'static TileDef {
    match kind {
        RoadKind::Dirt  => &DIRT_ROAD_TILE_DEF,
        RoadKind::Paved => &PAVED_ROAD_TILE_DEF,
    }
}

static DIRT_ROAD_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get().find_tile_def_by_hash(
        TileMapLayerKind::Terrain,
        TERRAIN_GROUND_CATEGORY.hash,
        tile_name(RoadKind::Dirt).hash)
            .expect("Failed to find dirt road tile!")
});

static PAVED_ROAD_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get().find_tile_def_by_hash(
        TileMapLayerKind::Terrain,
        TERRAIN_GROUND_CATEGORY.hash,
        tile_name(RoadKind::Paved).hash)
            .expect("Failed to find paved road tile!")
});

enum RangeInclusiveIter {
    Forward(RangeInclusive<i32>),
    Backward(Rev<RangeInclusive<i32>>),
}

impl Iterator for RangeInclusiveIter {
    type Item = i32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RangeInclusiveIter::Forward(r) => r.next(),
            RangeInclusiveIter::Backward(r) => r.next(),
        }
    }
}

#[inline]
fn range_inclusive(a: i32, b: i32) -> RangeInclusiveIter {
    if a <= b {
        RangeInclusiveIter::Forward(a..=b)
    } else {
        RangeInclusiveIter::Backward((b..=a).rev())
    }
}

fn can_place_road(tile_map: &TileMap, cell: Cell) -> bool {
    if !tile_map.is_cell_within_bounds(cell) {
        return false;
    }

    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
        if tile.kind().intersects(TileKind::Building | TileKind::Blocker | TileKind::Rocks | TileKind::Vegetation) {
            return false;
        }
    }

    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().intersects(PathNodeKind::Water | PathNodeKind::VacantLot) {
            return false;
        }
    }

    true
}

fn is_road(tile_map: &TileMap, cell: Cell) -> bool {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().is_road() ||
           tile.has_flags(TileFlags::DirtRoadPlacement | TileFlags::PavedRoadPlacement) {
            return true;
        }
    }
    false
}

fn is_path_valid(tile_map: &TileMap, path: &[Cell]) -> bool {
    path.iter().all(|cell| can_place_road(tile_map, *cell))
}

fn horizontal_vertical_path(start: Cell, end: Cell) -> Vec<Cell> {
    let mut path_hv = Vec::new();

    // Horizontal leg:
    for x in range_inclusive(start.x, end.x) {
        path_hv.push(Cell { x, y: start.y });
    }

    // Vertical leg (skip the first, since it’s the corner).
    for y in range_inclusive(start.y, end.y).skip(1) {
        path_hv.push(Cell { x: end.x, y });
    }

    path_hv
}

fn vertical_horizontal_path(start: Cell, end: Cell) -> Vec<Cell> {
    let mut path_vh = Vec::new();

    for y in range_inclusive(start.y, end.y) {
        path_vh.push(Cell { x: start.x, y });
    }

    // Horizontal leg (skip corner).
    for x in range_inclusive(start.x, end.x).skip(1) {
        path_vh.push(Cell { x, y: end.y });
    }

    path_vh
}

fn zigzag_path(start: Cell, end: Cell) -> Vec<Cell> {
    let dx = (end.x - start.x).signum();
    let dy = (end.y - start.y).signum();
    let mut x = start.x;
    let mut y = start.y;
    let mut path = Vec::new();

    while x != end.x || y != end.y {
        // Alternate between x and y movement for a stair-like diagonal.
        if (x - end.x).abs() > (y - end.y).abs() {
            x += dx;
        } else {
            y += dy;
        }
        path.push(Cell::new(x, y));
    }

    path
}

pub fn build_segment(tile_map: &TileMap, start: Cell, end: Cell, kind: RoadKind) -> RoadSegment {
    if start == end {
        // One cell segment.
        let path = vec![start];
        let is_valid = is_path_valid(tile_map, &path);
        return RoadSegment { path, kind, is_valid };
    }

    // Horizontal-first path:
    let hv_path = horizontal_vertical_path(start, end);
    let hv_valid = is_path_valid(tile_map, &hv_path);

    // Vertical-first path:
    let vh_path = vertical_horizontal_path(start, end);
    let vh_valid = is_path_valid(tile_map, &vh_path);

    // Diagonal zigzag path:
    let zigzag_path = zigzag_path(start, end);
    let zigzag_valid = is_path_valid(tile_map, &zigzag_path);

    match (hv_valid, vh_valid, zigzag_valid) {
        (true, false, false) | (true, false, true) => RoadSegment::valid(hv_path, kind), // Favor straight roads.
        (false, true, false) | (false, true, true) => RoadSegment::valid(vh_path, kind), // Favor straight roads.
        (true, true, false)  | (true, true, true)  => {
            // Prefer the one with fewer existing roads (so it expands less).
            // Favor straight roads (ignore zigzag diagonals).
            let hv_existing = hv_path.iter().filter(|cell| is_road(tile_map, **cell)).count();
            let vh_existing = vh_path.iter().filter(|cell| is_road(tile_map, **cell)).count();
            if hv_existing <= vh_existing {
                RoadSegment::valid(hv_path, kind)
            } else {
                RoadSegment::valid(vh_path, kind)
            }
        },
        // Only valid path is a zigzag diagonal.
        (false, false, true)  => RoadSegment::valid(zigzag_path, kind),
        // All blocked. Return an invalidated horiz-vert path.
        (false, false, false) => RoadSegment::invalid(hv_path, kind),
    }
}

pub fn mark_tiles(tile_map: &mut TileMap, segment: &RoadSegment, highlight: bool, valid_placement: bool) {
    let road_placement_flag = match segment.kind {
        RoadKind::Dirt  => TileFlags::DirtRoadPlacement,
        RoadKind::Paved => TileFlags::PavedRoadPlacement,
    };

    if highlight {
        for cell in &segment.path {
            if let Some(tile) = tile_map.try_tile_from_layer_mut(*cell, TileMapLayerKind::Terrain) {
                if valid_placement {
                    tile.set_flags(road_placement_flag | TileFlags::Highlighted, true);
                } else {
                    tile.set_flags(road_placement_flag | TileFlags::Invalidated, true);
                }
            }
        }
    } else {
        for cell in &segment.path {
            if let Some(tile) = tile_map.try_tile_from_layer_mut(*cell, TileMapLayerKind::Terrain) {
                tile.set_flags(
                    road_placement_flag |
                    TileFlags::Highlighted |
                    TileFlags::Invalidated,
                    false);
            }
        }
    }
}

// ----------------------------------------------
// Road Junctions Mask Reference (N,E,S,W)
// ----------------------------------------------

// Bit order (rightmost bit is the start)
//
// W = bit 0
// S = bit 1
// E = bit 2
// N = bit 3
// mask = N|E|S|W
//
// 0000 -> no connection
// 0001 -> connects west
// 0010 -> connects south
// 0100 -> connects east
// 1000 -> connects north
//
// Each mask maps to one of the 16 possible junctions for a road tile.
// The road tile has one variation for each of the junction combinations.

const WEST_BIT:  usize = 1 << 0;
const SOUTH_BIT: usize = 1 << 1;
const EAST_BIT:  usize = 1 << 2;
const NORTH_BIT: usize = 1 << 3;

pub fn junction_mask(tile_map: &TileMap, cell: Cell) -> usize {
    let mut mask = 0;
    if is_road(tile_map, Cell::new(cell.x + 1, cell.y)) { mask |= NORTH_BIT; }
    if is_road(tile_map, Cell::new(cell.x - 1, cell.y)) { mask |= SOUTH_BIT; }
    if is_road(tile_map, Cell::new(cell.x, cell.y - 1)) { mask |= EAST_BIT;  }
    if is_road(tile_map, Cell::new(cell.x, cell.y + 1)) { mask |= WEST_BIT;  }
    mask
}

pub fn update_junctions(tile_map: &mut TileMap, cell: Cell) {
    update_tile_junction(tile_map, cell);
    update_neighboring_junctions(tile_map, cell);
}

fn update_tile_junction(tile_map: &mut TileMap, cell: Cell) {
    let variation_index = junction_mask(tile_map, cell);
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().is_road() {
            tile.set_variation_index(variation_index);
        }
    }
}

fn update_neighboring_junctions(tile_map: &mut TileMap, cell: Cell) {
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let cell = Cell::new(cell.x + dx, cell.y + dy);
        if is_road(tile_map, cell) {
            update_tile_junction(tile_map, cell);
        }
    }
}
