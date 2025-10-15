use std::iter::Rev;
use std::ops::RangeInclusive;
use std::sync::LazyLock;

use crate::{
    pathfind::NodeKind as PathNodeKind,
    utils::{coords::Cell, hash},
    tile::{
        TileKind, TileFlags, TileMap, TileMapLayerKind,
        sets::{TileDef, TileSets, TERRAIN_GROUND_CATEGORY},
    },
};

#[derive(Default)]
pub struct RoadSegment {
    pub path: Vec<Cell>,
    pub is_valid: bool,
}

impl RoadSegment {
    fn valid(path: Vec<Cell>) -> Self {
        Self { path, is_valid: true }
    }

    fn invalid(path: Vec<Cell>) -> Self {
        Self { path, is_valid: false }
    }

    pub fn clear(&mut self) {
        self.path.clear();
        self.is_valid = false;
    }

    pub fn cost(&self) -> u32 {
        // All road tiles have the same cost.
        (self.path.len() as u32) * tile_def().cost
    }
}

#[inline]
pub fn tile_def() -> &'static TileDef {
    &ROAD_TILE_DEF
}

static ROAD_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get().find_tile_def_by_hash(
        TileMapLayerKind::Terrain,
        TERRAIN_GROUND_CATEGORY.hash,
        hash::fnv1a_from_str("stone_path"))
        .expect("Failed to find road tile 'stone_path'!")
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

fn can_place_road(cell: Cell, tile_map: &TileMap) -> bool {
    if !tile_map.is_cell_within_bounds(cell) {
        return false;
    }

    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
        if tile.kind().intersects(TileKind::Building | TileKind::Blocker | TileKind::Prop | TileKind::Vegetation) {
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

fn is_road(cell: Cell, tile_map: &TileMap) -> bool {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().intersects(PathNodeKind::Road) {
            return true;
        }
    }
    false
}

fn is_path_valid(path: &[Cell], tile_map: &TileMap) -> bool {
    path.iter().all(|cell| can_place_road(*cell, tile_map))
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

pub fn build_segment(start: Cell, end: Cell, tile_map: &TileMap) -> RoadSegment {
    if start == end {
        // One cell segment.
        let path = vec![start];
        let is_valid = is_path_valid(&path, tile_map);
        return RoadSegment { path, is_valid };
    }

    // Horizontal-first path:
    let hv_path = horizontal_vertical_path(start, end);
    let hv_valid = is_path_valid(&hv_path, tile_map);

    // Vertical-first path:
    let vh_path = vertical_horizontal_path(start, end);
    let vh_valid = is_path_valid(&vh_path, tile_map);

    // Diagonal zigzag path:
    let zigzag_path = zigzag_path(start, end);
    let zigzag_valid = is_path_valid(&zigzag_path, tile_map);

    match (hv_valid, vh_valid, zigzag_valid) {
        (true, false, false) | (true, false, true) => RoadSegment::valid(hv_path), // Favor straight roads.
        (false, true, false) | (false, true, true) => RoadSegment::valid(vh_path), // Favor straight roads.
        (true, true, false)  | (true, true, true)  => {
            // Prefer the one with fewer existing roads (so it expands less).
            // Favor straight roads (ignore zigzag diagonals).
            let hv_existing = hv_path.iter().filter(|cell| is_road(**cell, tile_map)).count();
            let vh_existing = vh_path.iter().filter(|cell| is_road(**cell, tile_map)).count();
            if hv_existing <= vh_existing {
                RoadSegment::valid(hv_path)
            } else {
                RoadSegment::valid(vh_path)
            }
        },
        // Only valid path is a zigzag diagonal.
        (false, false, true)  => RoadSegment::valid(zigzag_path),
        // All blocked. Return an invalidated horiz-vert path.
        (false, false, false) => RoadSegment::invalid(hv_path),
    }
}

pub fn mark_tiles(tile_map: &mut TileMap, segment: &RoadSegment, highlight: bool, valid_placement: bool) {
    if highlight {
        for cell in &segment.path {
            if let Some(tile) = tile_map.try_tile_from_layer_mut(*cell, TileMapLayerKind::Terrain) {
                if valid_placement {
                    tile.set_flags(TileFlags::RoadPlacement | TileFlags::Highlighted, true);
                } else {
                    tile.set_flags(TileFlags::RoadPlacement | TileFlags::Invalidated, true);
                }
            }
        }
    } else {
        for cell in &segment.path {
            if let Some(tile) = tile_map.try_tile_from_layer_mut(*cell, TileMapLayerKind::Terrain) {
                tile.set_flags(
                    TileFlags::RoadPlacement |
                    TileFlags::Highlighted |
                    TileFlags::Invalidated,
                    false);
            }
        }
    }
}
