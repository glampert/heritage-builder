// The city's walls (docs/plans/defence.md, §1): pieces drawn with the road tool, in three tiers.
//
// A wall piece is a plain object tile of the `walls` category (TileKind::Wall, path kind Wall: no one
// walks it), anchored as a 1x1 building so it sorts with the buildings and walkers around it. Its
// variation is its junction mask, as a road's is (road::junction_mask: north, east, south, west, a
// bit each, the variation index the mask read as a number): the sides that join another piece of any
// tier, a gatehouse, or a piece being drawn there, over ground that joins.
//
// A gatehouse stands in a wall (TileDef::requires_wall): on a cell with a wall piece on both sides
// along one axis, a piece of the wall itself (which makes way for it: TilePlacement::place), a gap
// of open land or a road through a gap. A road lies under it (laid with it where none ran), which
// runs through its passage both ways (gate_passage, road::junction_mask), so the roads on either
// side of the wall join it. Its variation is its walls' tier and their axis, `<tier>_ns` (walls to
// its north and south) or `<tier>_ew`, picked as it's placed; with walls of two tiers, the weaker's.

use common::{coords::Cell, hash};

use super::{
    TileFlags,
    TileKind,
    TileMap,
    TileMapLayerKind,
    road::{self, RoadKind},
    sets::{OBJECTS_WALLS_CATEGORY, TileDef, TileSets},
};
use crate::sim::defence::WallTier;

// The wall placement preview flags (tile::road), one per tier.
pub const WALL_PREVIEW_FLAGS: TileFlags =
    TileFlags::PalisadePlacement.union(TileFlags::RampartPlacement).union(TileFlags::CityWallPlacement);

// The cracks drawn over a damaged piece (tile::rendering; the overlays category).
pub const WALL_CRACKS_TILE: common::hash::StrHashPair = common::hash::StrHashPair::from_str("wall_cracks");

// A tier's tile def, if the tile sets have it.
pub fn tile_def(tier: WallTier) -> Option<&'static TileDef> {
    TileSets::get().try_find_tile_def_by_hash(
        TileMapLayerKind::Objects,
        OBJECTS_WALLS_CATEGORY.hash,
        hash::fnv1a_from_str(tier.name()),
    )
}

// The tier of a wall piece's tile def, None for any other tile.
pub fn tier_of(tile_def: &TileDef) -> Option<WallTier> {
    if !tile_def.is(TileKind::Wall) {
        return None;
    }
    WallTier::ALL.into_iter().find(|tier| hash::fnv1a_from_str(tier.name()) == tile_def.hash)
}

// The tier of the wall piece at `cell`, if any.
pub fn tier_at(tile_map: &TileMap, cell: Cell) -> Option<WallTier> {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).and_then(|tile| tier_of(tile.tile_def()))
}

#[inline]
pub fn is_wall(tile_map: &TileMap, cell: Cell) -> bool {
    tier_at(tile_map, cell).is_some()
}

// A gatehouse at `cell` (a building whose def requires a wall).
pub fn is_gatehouse(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Objects)
        .is_some_and(|tile| tile.is(TileKind::Building) && tile.tile_def().requires_wall)
}

// The wall pieces on the map, each cell and its tier (CityDefence::sync_walls).
pub fn wall_cells(tile_map: &TileMap) -> Vec<(Cell, WallTier)> {
    let mut cells = Vec::new();
    tile_map.for_each_tile(TileKind::Wall, |_, tile| {
        if let Some(tier) = tier_of(tile.tile_def()) {
            cells.push((tile.base_cell(), tier));
        }
    });
    cells
}

// Whether a piece at `cell` joins `neighbour`: another piece, a gatehouse or a piece being drawn
// there (its preview), over ground that joins.
fn joins(tile_map: &TileMap, cell: Cell, neighbour: Cell) -> bool {
    let wall_there = is_wall(tile_map, neighbour)
        || is_gatehouse(tile_map, neighbour)
        || tile_map
            .try_tile_from_layer(neighbour, TileMapLayerKind::Terrain)
            .is_some_and(|tile| tile.has_flags(WALL_PREVIEW_FLAGS));
    wall_there && tile_map.elevation().joins(cell, neighbour)
}

// The junction mask of a wall piece at `cell` (its variation index).
pub fn junction_mask(tile_map: &TileMap, cell: Cell) -> usize {
    road::mask_of(cell, |neighbour| joins(tile_map, cell, neighbour))
}

// Picks the variation of the piece at `cell` and of the pieces beside it, and turns a gatehouse
// beside it to its walls. Call it after placing or clearing a piece or a gatehouse there.
pub fn update_variations(tile_map: &mut TileMap, cell: Cell) {
    for cell in [
        cell,
        Cell::new(cell.x + 1, cell.y),
        Cell::new(cell.x - 1, cell.y),
        Cell::new(cell.x, cell.y - 1),
        Cell::new(cell.x, cell.y + 1),
    ] {
        if is_wall(tile_map, cell) {
            let mask = junction_mask(tile_map, cell);
            if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects) {
                tile.set_variation_index(mask);
            }
        } else if is_gatehouse(tile_map, cell) {
            orient_gatehouse(tile_map, cell);
        }
    }
}

// The two cells a gatehouse's passage opens onto, across its wall: east and west of an `_ns` gate
// (its walls north and south of it), north and south of an `_ew` one. None where no gatehouse
// stands.
pub fn gate_passage(tile_map: &TileMap, cell: Cell) -> Option<[Cell; 2]> {
    let tile = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects)?;
    if !tile.is(TileKind::Building) || !tile.tile_def().requires_wall {
        return None;
    }
    Some(passage_cells(cell, tile.variation_name().ends_with("_ns")))
}

fn passage_cells(cell: Cell, walls_ns: bool) -> [Cell; 2] {
    if walls_ns {
        [Cell::new(cell.x, cell.y - 1), Cell::new(cell.x, cell.y + 1)]
    } else {
        [Cell::new(cell.x - 1, cell.y), Cell::new(cell.x + 1, cell.y)]
    }
}

// The road laid under a gatehouse at `cell` where none runs: paved where a paved road meets its
// passage, else dirt.
pub fn gate_road(tile_map: &TileMap, cell: Cell) -> RoadKind {
    let walls_ns = crossing(tile_map, cell).is_none_or(|(_, axis)| axis == "ns");
    let paved = road::tile_def(RoadKind::Paved);
    let meets_paved = passage_cells(cell, walls_ns).into_iter().any(|cell| {
        tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.tile_def().hash == paved.hash)
    });
    if meets_paved { RoadKind::Paved } else { RoadKind::Dirt }
}

// Which way a wall crosses `cell` and its tier: a piece north and south of it (`ns`) or east and
// west (`ew`), the weaker's tier where they differ; None without a piece on both sides of one axis.
// North is +x on the grid and east is -y, as for roads.
pub fn crossing(tile_map: &TileMap, cell: Cell) -> Option<(WallTier, &'static str)> {
    let pair = |a: Cell, b: Cell| Some(tier_at(tile_map, a)?.min(tier_at(tile_map, b)?));
    if let Some(tier) = pair(Cell::new(cell.x + 1, cell.y), Cell::new(cell.x - 1, cell.y)) {
        return Some((tier, "ns"));
    }
    pair(Cell::new(cell.x, cell.y - 1), Cell::new(cell.x, cell.y + 1)).map(|tier| (tier, "ew"))
}

// Turns the gatehouse at `cell` to its walls: the variation `<tier>_<axis>`, by name.
pub fn orient_gatehouse(tile_map: &mut TileMap, cell: Cell) {
    let Some((tier, axis)) = crossing(tile_map, cell) else {
        return; // Its walls are gone: it keeps its look.
    };
    let name_hash = hash::fnv1a_from_str(&format!("{}_{axis}", tier.name()));
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects)
        && let Some(index) = tile.tile_def().variations.iter().position(|variation| variation.hash == name_hash)
    {
        tile.set_variation_index(index);
    }
}
