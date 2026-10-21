use arrayvec::ArrayString;
use common::{
    coords::Cell,
    hash::{self, StrHashPair},
};

use super::{TileKind, TileMap, TileMapLayerKind, sets::TileDef};

// ----------------------------------------------
// Water Tile Transitions
// ----------------------------------------------

// A water tile draws the shore of the land around it. Its variation is picked from which of its
// eight neighbors are land whenever the terrain next to it changes. The four sides come first, as
// the digits of the variation names: "0001" is land to the west, "0010" south, "0100" east, "1000"
// north; "1010" has land on both the north and the south sides, with a channel between the two
// shores, and "1111" on every side. A diagonal counts only when both sides next to it are water,
// since the shore of a land side covers it otherwise. The diagonals name the `_corner` variations:
// "ne_corner", "ne_se_corner", "1000_sw_corner", "1100_sw_corner", ... One exception: a tile with
// a single land side also lists the diagonal next to that side when a one-cell channel enters the
// tile beside the land, i.e. when the water side between the two is bordered by land at both of
// its diagonals ("0010_nw_sw_corner": land south, a channel entering from the west between the
// north-west and south-west land). There the shore bends from the channel's bank into the wider
// water. The hand-made sprites cover one side, two adjacent sides and one diagonal;
// crates/tools/water-transitions/compose.py composes the other 42 combinations from them.
//
// Directions: north is +x, the upper-right edge of the cell on screen; east is -y, the lower-right.

// Sides, as the digits of a variation name, from the last.
const WEST_BIT:  usize = 1 << 0; // 0001
const SOUTH_BIT: usize = 1 << 1; // 0010
const EAST_BIT:  usize = 1 << 2; // 0100
const NORTH_BIT: usize = 1 << 3; // 1000

// Diagonals, in the order a variation name lists them.
const NE_BIT: usize = 1 << 0;
const NW_BIT: usize = 1 << 1;
const SE_BIT: usize = 1 << 2;
const SW_BIT: usize = 1 << 3;

const DIAGONAL_NAMES: [(usize, &str); 4] = [(NE_BIT, "ne"), (NW_BIT, "nw"), (SE_BIT, "se"), (SW_BIT, "sw")];

// Open water, the variation every water tile starts with.
const OPEN_WATER_VARIATION: usize = 0;

// Name of the variation that draws a water tile's shores, e.g. "1010" or "1000_se_sw_corner".
pub type ShoreVariationName = ArrayString<32>;

// The land around a water cell.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ShoreNeighbors {
    pub sides: usize,     // NORTH_BIT | EAST_BIT | SOUTH_BIT | WEST_BIT.
    pub diagonals: usize, // NE_BIT | NW_BIT | SE_BIT | SW_BIT: those not next to a land side, plus `attached`.
    pub attached: usize,  // The diagonal next to the one land side where a channel enters beside it.
}

impl ShoreNeighbors {
    pub fn variation_name(self) -> ShoreVariationName {
        let mut name = ShoreVariationName::new();

        if self.sides != 0 || self.diagonals == 0 {
            for bit in [NORTH_BIT, EAST_BIT, SOUTH_BIT, WEST_BIT] {
                name.push(if (self.sides & bit) != 0 { '1' } else { '0' });
            }
        }

        if self.diagonals != 0 {
            for (bit, diagonal) in DIAGONAL_NAMES {
                if (self.diagonals & bit) != 0 {
                    if !name.is_empty() {
                        name.push('_');
                    }
                    name.push_str(diagonal);
                }
            }
            name.push_str("_corner");
        }

        name
    }
}

// Is the terrain at `cell` land? Cells past the map's edge count as water, so a shore
// ends at the edge of the map instead of running along it.
#[inline]
fn is_land(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
        .is_some_and(|tile| tile.path_kind().is_land())
}

pub fn shore_neighbors(tile_map: &TileMap, cell: Cell) -> ShoreNeighbors {
    let land = |dx: i32, dy: i32| is_land(tile_map, Cell::new(cell.x + dx, cell.y + dy));

    let north = land(1, 0);
    let south = land(-1, 0);
    let east  = land(0, -1);
    let west  = land(0, 1);

    let mut sides = 0;
    if north { sides |= NORTH_BIT; }
    if south { sides |= SOUTH_BIT; }
    if east  { sides |= EAST_BIT;  }
    if west  { sides |= WEST_BIT;  }

    let ne = land(1, -1);
    let nw = land(1, 1);
    let se = land(-1, -1);
    let sw = land(-1, 1);

    // A diagonal shows only when the two sides next to it are water.
    let mut diagonals = 0;
    if !north && !east && ne { diagonals |= NE_BIT; }
    if !north && !west && nw { diagonals |= NW_BIT; }
    if !south && !east && se { diagonals |= SE_BIT; }
    if !south && !west && sw { diagonals |= SW_BIT; }

    // With a single land side, a channel can enter the tile beside it: the water side next to
    // the land then has land at both of its diagonals. The diagonal next to the land side shows
    // too, and the shore bends into the channel's bank. Not when the far diagonals of both water
    // sides are land: that tile is a T junction, with a channel on each side already.
    let mut attached = 0;
    match sides {
        NORTH_BIT => {
            if se && ne && !sw { attached |= NE_BIT; }
            if sw && nw && !se { attached |= NW_BIT; }
        }
        SOUTH_BIT => {
            if ne && se && !nw { attached |= SE_BIT; }
            if nw && sw && !ne { attached |= SW_BIT; }
        }
        EAST_BIT => {
            if nw && ne && !sw { attached |= NE_BIT; }
            if sw && se && !nw { attached |= SE_BIT; }
        }
        WEST_BIT => {
            if ne && nw && !se { attached |= NW_BIT; }
            if se && sw && !ne { attached |= SW_BIT; }
        }
        _ => {}
    }

    ShoreNeighbors { sides, diagonals: diagonals | attached, attached }
}

// The variation of `tile_def` that draws these shores. A tile set without the variation for an
// entering channel falls back to the plain corner, one without that to the sides alone, and one
// without that to open water.
fn find_shore_variation(tile_def: &TileDef, neighbors: ShoreNeighbors) -> usize {
    let find = |sides: usize, diagonals: usize| {
        let name = ShoreNeighbors { sides, diagonals, attached: 0 }.variation_name();
        let hash = hash::fnv1a_from_str(&name);
        tile_def.variations.iter().position(|variation| variation.hash == hash)
    };

    let mut fallbacks = [
        Some((neighbors.sides, neighbors.diagonals)),
        (neighbors.attached != 0).then_some((neighbors.sides, neighbors.diagonals & !neighbors.attached)),
        (neighbors.diagonals != 0).then_some((neighbors.sides, 0)),
    ]
    .into_iter()
    .flatten();

    fallbacks.find_map(|(sides, diagonals)| find(sides, diagonals)).unwrap_or(OPEN_WATER_VARIATION)
}

fn update_tile_transitions(tile_map: &mut TileMap, cell: Cell) {
    let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) else {
        return;
    };

    if !tile.path_kind().is_water() {
        return;
    }

    let variation_index = find_shore_variation(tile.tile_def(), shore_neighbors(tile_map, cell));

    if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
        tile.set_variation_index(variation_index);
    }
}

// Picks the shores drawn by the water tile at `cell`, if it is one, and by the water tiles around
// it, once the terrain at `cell` has changed. A water tile's shores depend on its eight neighbors
// alone, so the ring around the changed cell holds every tile that can change.
pub fn update_transitions(tile_map: &mut TileMap, cell: Cell) {
    for y in -1..=1 {
        for x in -1..=1 {
            update_tile_transitions(tile_map, Cell::new(cell.x + x, cell.y + y));
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
    if tile_def.is(TileKind::Building) && tile_def.hash == FISHING_WHARF.hash {
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

// Which sides of `cell` have land, as NORTH_BIT | EAST_BIT | SOUTH_BIT | WEST_BIT.
fn water_transition_mask(tile_map: &TileMap, cell: Cell) -> usize {
    shore_neighbors(tile_map, cell).sides
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
