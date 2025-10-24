use super::{sets::TileDef, TileMap, TileKind, TileMapLayerKind};
use crate::{pathfind::NodeKind as PathNodeKind, utils::{coords::Cell, hash::StrHashPair}};

// ----------------------------------------------
// Water Tile Transitions
// ----------------------------------------------

// Each bit represents whether there is land in that direction
// relative to the current tile (the one being placed).
const WEST_BIT:  usize = 1 << 0; // 0001
const SOUTH_BIT: usize = 1 << 1; // 0010
const EAST_BIT:  usize = 1 << 2; // 0100
const NORTH_BIT: usize = 1 << 3; // 1000

fn water_transition_mask(tile_map: &TileMap, cell: Cell) -> usize {
    let mut mask = 0;
    if is_land(tile_map, Cell::new(cell.x + 1, cell.y)) { mask |= NORTH_BIT; }
    if is_land(tile_map, Cell::new(cell.x - 1, cell.y)) { mask |= SOUTH_BIT; }
    if is_land(tile_map, Cell::new(cell.x, cell.y - 1)) { mask |= EAST_BIT;  }
    if is_land(tile_map, Cell::new(cell.x, cell.y + 1)) { mask |= WEST_BIT;  }
    mask
}

#[inline]
fn is_land(tile_map: &TileMap, cell: Cell) -> bool {
    !is_water(tile_map, cell)
}

#[inline]
fn is_water(tile_map: &TileMap, cell: Cell) -> bool {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().is_water() {
            return true;
        }
    }
    false
}

// Lookup table to handle invalid/unsupported combinations.
// If no transition is available we fallback to full water (var 0).
const FALLBACK_WATER_TRANSITION_VARIATION: usize = 0;
const WATER_TRANSITION_VARIATION_LOOKUP: [Option<usize>; 16] = [
    None,    // 0000 — surrounded by water
    Some(1), // 0001 — land to the west
    Some(2), // 0010 — land to the south
    Some(3), // 0011 — NE corner (land south & west)
    Some(4), // 0100 — land to the east
    None,    // 0101
    Some(5), // 0110 — NW corner (land south & east)
    None,    // 0111
    Some(6), // 1000 — land to the north
    Some(7), // 1001 — SE corner (land north & west)
    None,    // 1010
    None,    // 1011
    Some(8), // 1100 — SW corner (land north & east)
    None,    // 1101
    None,    // 1110
    None,    // 1111
];

// Outer corners for islands / L-shaped inner edges.
#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq)]
enum ShoreCorner {
    None,
    // These follow the last transition variation (`1100 — SW corner`, #8).
    SE = 9,  // se_corner
    SW = 10, // sw_corner
    NE = 11, // ne_corner
    NW = 12, // nw_corner
}

// Water transition mask + outer corner transition if any.
fn compute_full_water_transitions(tile_map: &TileMap, cell: Cell) -> (usize, ShoreCorner) {
    fn tile_at(tile_map: &TileMap, x: i32, y: i32) -> PathNodeKind {
        if let Some(tile) = tile_map.try_tile_from_layer(Cell::new(x, y), TileMapLayerKind::Terrain) {
            tile.path_kind()
        } else {
            PathNodeKind::empty()
        }
    }

    // Helper: does a given cell have at least one cardinal land neighbor (including self)?
    fn neighborhood_has_cardinal_land(tile_map: &TileMap, x: i32, y: i32) -> bool {
        tile_at(tile_map, x, y).is_land()
        || tile_at(tile_map, x + 1, y).is_land()
        || tile_at(tile_map, x - 1, y).is_land()
        || tile_at(tile_map, x, y - 1).is_land()
        || tile_at(tile_map, x, y + 1).is_land()
    }

    // Cardinal neighbors (N/S/E/W):
    let north = tile_at(tile_map, cell.x + 1, cell.y);
    let south = tile_at(tile_map, cell.x - 1, cell.y);
    let east  = tile_at(tile_map, cell.x, cell.y - 1);
    let west  = tile_at(tile_map, cell.x, cell.y + 1);

    // Diagonals:
    let ne = tile_at(tile_map, cell.x + 1, cell.y - 1);
    let nw = tile_at(tile_map, cell.x + 1, cell.y + 1);
    let se = tile_at(tile_map, cell.x - 1, cell.y - 1);
    let sw = tile_at(tile_map, cell.x - 1, cell.y + 1);

    // Base mask: which cardinals are land (so this water tile must show transition on those sides).
    let mut mask = 0;
    if north.is_land() { mask |= NORTH_BIT; }
    if south.is_land() { mask |= SOUTH_BIT; }
    if east.is_land()  { mask |= EAST_BIT;  }
    if west.is_land()  { mask |= WEST_BIT;  }

    // Decide outer corner — default none.
    let mut corner = ShoreCorner::None;

    // Condition pattern used for each corner:
    // 1) diagonal is land
    // 2) both adjacent cardinals (from the current tile) are water
    // 3) there is land context:
    //    either mask != 0 (current tile touches some cardinal land)
    //    OR the diagonal cell has at least one cardinal land neighbor (so it's not an isolated diagonal)

    // SE diagonal case: se at (x-1, y-1)
    // adjacent cardinals for current tile: north (x+1,y) and west (x,y+1) must be water
    if se.is_land()
        && north.is_water() && west.is_water()
        && (mask != 0 || neighborhood_has_cardinal_land(tile_map, cell.x - 1, cell.y - 1))
    {
        corner = ShoreCorner::SE;
    }

    // SW diagonal case: sw at (x-1, y+1)
    // adjacent: north (x+1,y) and east (x,y-1)
    if sw.is_land()
        && north.is_water() && east.is_water()
        && (mask != 0 || neighborhood_has_cardinal_land(tile_map, cell.x - 1, cell.y + 1))
    {
        corner = ShoreCorner::SW;
    }

    // NE diagonal case: ne at (x+1, y-1)
    // adjacent: south (x-1,y) and west (x,y+1)
    if ne.is_land()
        && south.is_water() && west.is_water()
        && (mask != 0 || neighborhood_has_cardinal_land(tile_map, cell.x + 1, cell.y - 1))
    {
        corner = ShoreCorner::NE;
    }

    // NW diagonal case: nw at (x+1, y+1)
    // adjacent: south (x-1,y) and east (x,y-1)
    if nw.is_land()
        && south.is_water() && east.is_water()
        && (mask != 0 || neighborhood_has_cardinal_land(tile_map, cell.x + 1, cell.y + 1))
    {
        corner = ShoreCorner::NW;
    }

    // If this tile is completely surrounded by water (mask == 0)
    // and no diagonal produced a corner (corner == None), keep None.

    (mask, corner)
}

fn update_tile_transitions(tile_map: &mut TileMap, cell: Cell) {
    let (mask, corner) = compute_full_water_transitions(tile_map, cell);
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().is_water() {
            if let Some(variation_index) = WATER_TRANSITION_VARIATION_LOOKUP[mask] {
                tile.set_variation_index(variation_index);
            } else {
                // Outer corner?
                if corner != ShoreCorner::None {
                    tile.set_variation_index(corner as usize);
                } else {
                    // Fully surrounded by water.
                    tile.set_variation_index(FALLBACK_WATER_TRANSITION_VARIATION);
                }
            }
        }
    }
}

fn update_neighboring_transitions(tile_map: &mut TileMap, cell: Cell) {
    for y in -1..=1 {
        for x in -1..=1 {
            let cell = Cell::new(cell.x + x, cell.y + y);
            if is_water(tile_map, cell) {
                update_tile_transitions(tile_map, cell);
            }
        }
    }
}

pub fn update_transitions(tile_map: &mut TileMap, cell: Cell) {
    update_tile_transitions(tile_map, cell);
    update_neighboring_transitions(tile_map, cell);
}

// ----------------------------------------------
// Ports and Wharfs
// ----------------------------------------------

const FISHING_WHARF: StrHashPair = StrHashPair::from_str("fishing_wharf");

const PORT_VARIATION_NORTH_BANK: usize = 0;
const PORT_VARIATION_SOUTH_BANK: usize = 1;
const PORT_VARIATION_EAST_BANK:  usize = 2;
const PORT_VARIATION_WEST_BANK:  usize = 3;

// Port-like buildings, e.g. fishing wharf, port, etc.
// These buildings must be placed over a water edge tile.
#[inline]
pub fn is_port_or_wharf(tile_def: &TileDef) -> bool {
    if tile_def.is(TileKind::Building)
        && tile_def.hash == FISHING_WHARF.hash {
        return true;
    }
    false
}

pub fn update_port_wharf_orientation(tile_map: &mut TileMap, cell: Cell) {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
        debug_assert!(is_port_or_wharf(tile.tile_def()));
        for tile_cell in &tile.cell_range() {
            if try_set_port_wharf_variation(tile_map, tile_cell) {
                return;
            }
        }
    }
    panic!("Couldn't find suitable port/wharf variation!");
}

fn try_set_port_wharf_variation(tile_map: &mut TileMap, cell: Cell) -> bool {
    let mask = water_transition_mask(tile_map, cell);
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects) {
        if (mask & NORTH_BIT) != 0 {
            tile.set_variation_index(PORT_VARIATION_NORTH_BANK);
            return true;
        } else if (mask & SOUTH_BIT) != 0 {
            tile.set_variation_index(PORT_VARIATION_SOUTH_BANK);
            return true;
        } else if (mask & EAST_BIT) != 0 {
            tile.set_variation_index(PORT_VARIATION_EAST_BANK);
            return true;
        } else if (mask & WEST_BIT) != 0 {
            tile.set_variation_index(PORT_VARIATION_WEST_BANK);
            return true;
        }
    }
    false
}
