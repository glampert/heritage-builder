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

fn water_transition_mask(cell: Cell, tile_map: &TileMap) -> usize {
    let mut mask = 0;
    if is_land(Cell::new(cell.x + 1, cell.y), tile_map) { mask |= NORTH_BIT; }
    if is_land(Cell::new(cell.x - 1, cell.y), tile_map) { mask |= SOUTH_BIT; }
    if is_land(Cell::new(cell.x, cell.y - 1), tile_map) { mask |= EAST_BIT;  }
    if is_land(Cell::new(cell.x, cell.y + 1), tile_map) { mask |= WEST_BIT;  }
    mask
}

#[inline]
fn is_land(cell: Cell, tile_map: &TileMap) -> bool {
    !is_water(cell, tile_map)
}

#[inline]
fn is_water(cell: Cell, tile_map: &TileMap) -> bool {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().intersects(PathNodeKind::Water) {
            return true;
        }
    }
    false
}

// Lookup table to handle invalid/unsupported combinations.
// If no transition is available we fallback to full water (var 0).
const FALLBACK_WATER_TRANSITION_VARIATION: usize = 0;
const WATER_TRANSITION_LOOKUP: [Option<usize>; 16] = [
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

pub fn update_transitions(cell: Cell, tile_map: &mut TileMap) {
    update_tile_transitions(cell, tile_map);
    update_neighboring_transitions(cell, tile_map);
}

fn update_tile_transitions(cell: Cell, tile_map: &mut TileMap) {
    let mask = water_transition_mask(cell, tile_map);
    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
        if tile.path_kind().intersects(PathNodeKind::Water) {
            if let Some(variation_index) = WATER_TRANSITION_LOOKUP[mask] {
                tile.set_variation_index(variation_index);
            } else {
                tile.set_variation_index(FALLBACK_WATER_TRANSITION_VARIATION);
            }
        }
    }
}

fn update_neighboring_transitions(cell: Cell, tile_map: &mut TileMap) {
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let cell = Cell::new(cell.x + dx, cell.y + dy);
        if is_water(cell, tile_map) {
            update_tile_transitions(cell, tile_map);
        }
    }
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

pub fn update_port_wharf_orientation(cell: Cell, tile_map: &mut TileMap) {
    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
        debug_assert!(is_port_or_wharf(tile.tile_def()));
        for tile_cell in &tile.cell_range() {
            if try_set_port_wharf_variation(tile_cell, tile_map) {
                return;
            }
        }
    }
    panic!("Couldn't find port variation!");
}

fn try_set_port_wharf_variation(cell: Cell, tile_map: &mut TileMap) -> bool {
    let mask = water_transition_mask(cell, tile_map);
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
