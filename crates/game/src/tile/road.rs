use std::{iter::Rev, ops::RangeInclusive, sync::LazyLock};

use common::{coords::Cell, hash::StrHashPair};

use crate::{
    pathfind::NodeKind as PathNodeKind,
    tile::{
        TileFlags,
        TileKind,
        TileMap,
        TileMapLayerKind,
        sets::{OBJECTS_WALLS_CATEGORY, TERRAIN_LAND_CATEGORY, TERRAIN_WATER_CATEGORY, TileDef, TileSets},
        wall,
        water,
    },
};

// ----------------------------------------------
// RoadKind
// ----------------------------------------------

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum RoadKind {
    #[default]
    Dirt,
    Paved,
    // A dyke (crate::floods): an embankment drawn as roads are, a road along its crest, that
    // stops the river's flood (TileDef::blocks_floods).
    Dyke,
    // A canal (crate::farms::irrigation): drawn as roads are, but a water tile with the water's
    // shores (tile::water), on open land only. Not a road: nothing walks it.
    Canal,
    // The city's walls in their three tiers (crate::defence, tile::wall): drawn as roads are, but
    // object tiles that sort with the buildings, on open land only. No one walks them.
    Palisade,
    Rampart,
    CityWall,
}

impl RoadKind {
    pub const ALL: [RoadKind; 7] = [
        RoadKind::Dirt,
        RoadKind::Paved,
        RoadKind::Dyke,
        RoadKind::Canal,
        RoadKind::Palisade,
        RoadKind::Rampart,
        RoadKind::CityWall,
    ];

    // A wall's tier, None for a road, dyke or canal.
    pub const fn wall_tier(self) -> Option<crate::sim::defence::WallTier> {
        use crate::sim::defence::WallTier;
        match self {
            RoadKind::Palisade => Some(WallTier::Palisade),
            RoadKind::Rampart => Some(WallTier::Rampart),
            RoadKind::CityWall => Some(WallTier::CityWall),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_wall(self) -> bool {
        self.wall_tier().is_some()
    }
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
        RoadKind::Dirt => StrHashPair::from_str("dirt_road"),
        RoadKind::Paved => StrHashPair::from_str("paved_road"),
        RoadKind::Dyke => StrHashPair::from_str("dyke"),
        RoadKind::Canal => water::CANAL,
        RoadKind::Palisade => StrHashPair::from_str("palisade"),
        RoadKind::Rampart => StrHashPair::from_str("rampart"),
        RoadKind::CityWall => StrHashPair::from_str("city_wall"),
    }
}

#[inline]
pub fn tile_def(kind: RoadKind) -> &'static TileDef {
    match kind {
        RoadKind::Dirt => &DIRT_ROAD_TILE_DEF,
        RoadKind::Paved => &PAVED_ROAD_TILE_DEF,
        RoadKind::Dyke => &DYKE_TILE_DEF,
        RoadKind::Canal => &CANAL_TILE_DEF,
        RoadKind::Palisade => &PALISADE_TILE_DEF,
        RoadKind::Rampart => &RAMPART_TILE_DEF,
        RoadKind::CityWall => &CITY_WALL_TILE_DEF,
    }
}

#[inline]
pub fn kind(tile_def: &'static TileDef) -> RoadKind {
    if tile_def.hash == DIRT_ROAD_TILE_DEF.hash {
        RoadKind::Dirt
    } else if tile_def.hash == PAVED_ROAD_TILE_DEF.hash {
        RoadKind::Paved
    } else if tile_def.hash == DYKE_TILE_DEF.hash {
        RoadKind::Dyke
    } else if tile_def.hash == CANAL_TILE_DEF.hash {
        RoadKind::Canal
    } else if let Some(kind) = try_kind(tile_def) {
        kind
    } else {
        unimplemented!()
    }
}

// The road kind of a tile def drawn as a segment (a road, a dyke, a canal, a wall), None for any other.
pub fn try_kind(tile_def: &TileDef) -> Option<RoadKind> {
    if !tile_def.is(TileKind::Terrain | TileKind::Wall) {
        return None;
    }
    RoadKind::ALL.into_iter().find(|kind| tile_name(*kind).hash == tile_def.hash)
}

static DIRT_ROAD_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get()
        .find_tile_def_by_hash(TileMapLayerKind::Terrain, TERRAIN_LAND_CATEGORY.hash, tile_name(RoadKind::Dirt).hash)
        .expect("Failed to find dirt road tile!")
});

static PAVED_ROAD_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get()
        .find_tile_def_by_hash(TileMapLayerKind::Terrain, TERRAIN_LAND_CATEGORY.hash, tile_name(RoadKind::Paved).hash)
        .expect("Failed to find paved road tile!")
});

static DYKE_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get()
        .find_tile_def_by_hash(TileMapLayerKind::Terrain, TERRAIN_LAND_CATEGORY.hash, tile_name(RoadKind::Dyke).hash)
        .expect("Failed to find the dyke tile!")
});

fn wall_tile_def(kind: RoadKind) -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_hash(TileMapLayerKind::Objects, OBJECTS_WALLS_CATEGORY.hash, tile_name(kind).hash)
        .unwrap_or_else(|| panic!("Failed to find the {kind:?} wall tile!"))
}

static PALISADE_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| wall_tile_def(RoadKind::Palisade));
static RAMPART_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| wall_tile_def(RoadKind::Rampart));
static CITY_WALL_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| wall_tile_def(RoadKind::CityWall));

static CANAL_TILE_DEF: LazyLock<&'static TileDef> = LazyLock::new(|| {
    TileSets::get()
        .find_tile_def_by_hash(TileMapLayerKind::Terrain, TERRAIN_WATER_CATEGORY.hash, tile_name(RoadKind::Canal).hash)
        .expect("Failed to find the canal tile!")
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
    if a <= b { RangeInclusiveIter::Forward(a..=b) } else { RangeInclusiveIter::Backward((b..=a).rev()) }
}

fn can_place_road(tile_map: &TileMap, cell: Cell, kind: RoadKind) -> bool {
    if !tile_map.is_cell_within_bounds(cell) {
        return false;
    }

    // Heights (docs/plans/terrain-height.md, §7): a canal is water, flat at level 0; a dyke stands on
    // flat ground; a road may run up a ramp (is_path_valid keeps it on ground that joins).
    let elevation = tile_map.elevation();
    match kind {
        RoadKind::Canal if elevation.is_raised(cell) => return false,
        RoadKind::Dyke if elevation.shape(cell).is_ramp() => return false,
        _ if kind.is_wall() && elevation.shape(cell).is_ramp() => return false,
        _ => {}
    }

    // A wall goes on open land, or over a wall piece already there (a stronger tier replaces it).
    if kind.is_wall() {
        if let Some(object) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects)
            && wall::tier_of(object.tile_def()).is_none()
        {
            return false;
        }
        return tile_map
            .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
            .is_some_and(|tile| tile.path_kind() == PathNodeKind::EmptyLand);
    }

    // A canal goes on open land with nothing on it (not a road, a dyke, a lot or a field), or
    // over a canal already there.
    if kind == RoadKind::Canal {
        if tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).is_some() {
            return false;
        }
        return tile_map
            .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
            .is_some_and(|tile| water::is_canal_def(tile.tile_def()) || tile.path_kind() == PathNodeKind::EmptyLand);
    }

    // A road runs through a gatehouse (a road lies under it already, tile::wall), never over a wall:
    // only a gatehouse opens one.
    if matches!(kind, RoadKind::Dirt | RoadKind::Paved) && wall::is_gatehouse(tile_map, cell) {
        return true;
    }

    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects)
        && tile
            .kind()
            .intersects(TileKind::Building | TileKind::Blocker | TileKind::Rocks | TileKind::Vegetation | TileKind::Wall)
    {
        return false;
    }

    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain)
        && tile.path_kind().intersects(PathNodeKind::Water | PathNodeKind::VacantLot)
    {
        return false;
    }

    true
}

fn is_road(tile_map: &TileMap, cell: Cell) -> bool {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain)
        && (tile.path_kind().is_road() || tile.has_flags(ROAD_PREVIEW_FLAGS))
    {
        return true;
    }
    false
}

// Every cell takes the road, and each step joins the next by height: no step down a cliff, and onto a
// ramp only along its way (ElevationMap::joins).
fn is_path_valid(tile_map: &TileMap, path: &[Cell], kind: RoadKind) -> bool {
    let elevation = tile_map.elevation();
    path.iter().all(|cell| can_place_road(tile_map, *cell, kind))
        && path.windows(2).all(|step| elevation.joins(step[0], step[1]))
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
        let is_valid = is_path_valid(tile_map, &path, kind);
        return RoadSegment { path, kind, is_valid };
    }

    // Horizontal-first path:
    let hv_path = horizontal_vertical_path(start, end);
    let hv_valid = is_path_valid(tile_map, &hv_path, kind);

    // Vertical-first path:
    let vh_path = vertical_horizontal_path(start, end);
    let vh_valid = is_path_valid(tile_map, &vh_path, kind);

    // Diagonal zigzag path:
    let zigzag_path = zigzag_path(start, end);
    let zigzag_valid = is_path_valid(tile_map, &zigzag_path, kind);

    match (hv_valid, vh_valid, zigzag_valid) {
        (true, false, false) | (true, false, true) => RoadSegment::valid(hv_path, kind), // Favor straight roads.
        (false, true, false) | (false, true, true) => RoadSegment::valid(vh_path, kind), // Favor straight roads.
        (true, true, false) | (true, true, true) => {
            // Prefer the one with fewer existing roads (so it expands less).
            // Favor straight roads (ignore zigzag diagonals).
            let hv_existing = hv_path.iter().filter(|cell| is_road(tile_map, **cell)).count();
            let vh_existing = vh_path.iter().filter(|cell| is_road(tile_map, **cell)).count();
            if hv_existing <= vh_existing { RoadSegment::valid(hv_path, kind) } else { RoadSegment::valid(vh_path, kind) }
        }
        // Only valid path is a zigzag diagonal.
        (false, false, true) => RoadSegment::valid(zigzag_path, kind),
        // All blocked. Return an invalidated horiz-vert path.
        (false, false, false) => RoadSegment::invalid(hv_path, kind),
    }
}

// The flags a segment's preview sets on its cells, one per kind (the renderer draws that kind's
// piece over them).
pub const ROAD_PLACEMENT_FLAGS: TileFlags =
    ROAD_PREVIEW_FLAGS.union(TileFlags::CanalPlacement).union(wall::WALL_PREVIEW_FLAGS);

// The previews of roads and dykes, which roads' junctions join (a canal's doesn't).
const ROAD_PREVIEW_FLAGS: TileFlags =
    TileFlags::DirtRoadPlacement.union(TileFlags::PavedRoadPlacement).union(TileFlags::DykePlacement);

pub const fn placement_flag(kind: RoadKind) -> TileFlags {
    match kind {
        RoadKind::Dirt => TileFlags::DirtRoadPlacement,
        RoadKind::Paved => TileFlags::PavedRoadPlacement,
        RoadKind::Dyke => TileFlags::DykePlacement,
        RoadKind::Canal => TileFlags::CanalPlacement,
        RoadKind::Palisade => TileFlags::PalisadePlacement,
        RoadKind::Rampart => TileFlags::RampartPlacement,
        RoadKind::CityWall => TileFlags::CityWallPlacement,
    }
}

// The kind a cell's preview flags name, if any.
pub fn placement_kind(flags: TileFlags) -> Option<RoadKind> {
    RoadKind::ALL.into_iter().find(|kind| flags.intersects(placement_flag(*kind)))
}

pub fn mark_tiles(tile_map: &mut TileMap, segment: &RoadSegment, highlight: bool, valid_placement: bool) {
    let road_placement_flag = placement_flag(segment.kind);

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
                tile.set_flags(road_placement_flag | TileFlags::Highlighted | TileFlags::Invalidated, false);
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

const WEST_BIT: usize = 1 << 0;
const SOUTH_BIT: usize = 1 << 1;
const EAST_BIT: usize = 1 << 2;
const NORTH_BIT: usize = 1 << 3;

// The road under a gatehouse runs through its passage both ways, road beyond it or not
// (wall::gate_passage); the roads beside it join it as they join any road.
pub fn junction_mask(tile_map: &TileMap, cell: Cell) -> usize {
    let elevation = tile_map.elevation();
    let passage = wall::gate_passage(tile_map, cell);
    mask_of(cell, |neighbour| {
        (is_road(tile_map, neighbour) || passage.is_some_and(|passage| passage.contains(&neighbour)))
            && elevation.joins(cell, neighbour)
    })
}

// A dyke's pieces join other dykes only: its embankment stands at full height where it joins, so
// a flat road beside it would leave its cut end open. A road still joins a dyke, running up to the
// foot of its bank.
pub fn dyke_junction_mask(tile_map: &TileMap, cell: Cell) -> usize {
    let elevation = tile_map.elevation();
    mask_of(cell, |neighbour| is_dyke_or_placing_one(tile_map, neighbour) && elevation.joins(cell, neighbour))
}

// The junction mask of a road tile of `kind` at the cell (a wall piece's, for a wall).
pub fn junction_mask_for(tile_map: &TileMap, cell: Cell, kind: RoadKind) -> usize {
    if kind.is_wall() {
        wall::junction_mask(tile_map, cell)
    } else if kind == RoadKind::Dyke {
        dyke_junction_mask(tile_map, cell)
    } else {
        junction_mask(tile_map, cell)
    }
}

pub(crate) fn mask_of(cell: Cell, joins: impl Fn(Cell) -> bool) -> usize {
    let mut mask = 0;
    if joins(Cell::new(cell.x + 1, cell.y)) {
        mask |= NORTH_BIT;
    }
    if joins(Cell::new(cell.x - 1, cell.y)) {
        mask |= SOUTH_BIT;
    }
    if joins(Cell::new(cell.x, cell.y - 1)) {
        mask |= EAST_BIT;
    }
    if joins(Cell::new(cell.x, cell.y + 1)) {
        mask |= WEST_BIT;
    }
    mask
}

fn is_dyke_or_placing_one(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
        .is_some_and(|tile| tile.tile_def().blocks_floods || tile.has_flags(TileFlags::DykePlacement))
}

pub fn update_junctions(tile_map: &mut TileMap, cell: Cell) {
    update_tile_junction(tile_map, cell);
    update_neighboring_junctions(tile_map, cell);
}

fn update_tile_junction(tile_map: &mut TileMap, cell: Cell) {
    let is_dyke =
        tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.tile_def().blocks_floods);
    let variation_index = if is_dyke { dyke_junction_mask(tile_map, cell) } else { junction_mask(tile_map, cell) };
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain)
        && tile.path_kind().is_road()
    {
        tile.set_variation_index(variation_index);
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
