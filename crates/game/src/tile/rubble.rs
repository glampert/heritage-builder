// Rubble: what a building that burns down or collapses leaves (docs/plans/city-risks.md).
//
// A pile is built from 1x1 pieces, as water shores and road junctions are. Each rubble tile's
// variation names which of its four sides border more rubble, as digits in the order north,
// east, south, west, where 1 means rubble that way: `1111` is the middle of a pile, `0110` a
// corner, `1010` a strip and `0000` a heap on its own. North is +x and east is -y, as for roads
// (road::junction_mask).
//
// The tile spawns (Spawner, SimCmds) don't pick neighbours' variations, so whatever places or
// clears rubble calls update_variations: place_pile after a destruction, and TilePlacement's
// place and clear.

use common::{
    coords::{Cell, CellRange},
    hash::{self, StrHashPair},
};
use engine::log;

use super::{
    TileKind,
    TileMap,
    TileMapLayerKind,
    sets::{OBJECTS_ROCKS_CATEGORY, TileDef, TileSets},
};
use crate::{sim::SimContext, world::object::Spawner};

pub const RUBBLE_TILE: StrHashPair = StrHashPair::from_str("rubble");

// North, east, south, west: the neighbour each digit of a variation name looks at.
const SIDES: [(i32, i32); 4] = [(1, 0), (0, -1), (-1, 0), (0, 1)];

pub fn rubble_tile_def() -> Option<&'static TileDef> {
    TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Objects, OBJECTS_ROCKS_CATEGORY.hash, RUBBLE_TILE.hash)
}

#[inline]
pub fn is_rubble_def(tile_def: &TileDef) -> bool {
    tile_def.hash == RUBBLE_TILE.hash && tile_def.is(TileKind::Rocks)
}

#[inline]
pub fn is_rubble(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).is_some_and(|tile| is_rubble_def(tile.tile_def()))
}

// The variation name for the rubble at `cell`: a digit per side, north, east, south, west.
pub fn variation_name(tile_map: &TileMap, cell: Cell) -> String {
    SIDES
        .iter()
        .map(|(dx, dy)| if is_rubble(tile_map, Cell::new(cell.x + dx, cell.y + dy)) { '1' } else { '0' })
        .collect()
}

// Picks the variation of every rubble tile in `area` and the ring of cells around it, from its
// neighbours. Call it after placing or clearing rubble anywhere in `area`.
pub fn update_variations(tile_map: &mut TileMap, area: CellRange) {
    let ring = CellRange::new(Cell::new(area.start.x - 1, area.start.y - 1), Cell::new(area.end.x + 1, area.end.y + 1));
    for cell in &ring {
        if !is_rubble(tile_map, cell) {
            continue;
        }

        let name = variation_name(tile_map, cell);
        let name_hash = hash::fnv1a_from_str(&name);

        if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects) {
            let variations = &tile.tile_def().variations;
            // Every mask has a piece; a tile set without one shows the lone heap, the first.
            let index = variations.iter().position(|variation| variation.hash == name_hash).unwrap_or(0);
            tile.set_variation_index(index);
        }
    }
}

// Covers each empty cell of `area` with rubble and joins it to any rubble around it. Returns how
// many cells got rubble. A cell something else still stands on is skipped and logged.
pub fn place_pile(context: &SimContext, area: CellRange) -> usize {
    let Some(rubble_def) = rubble_tile_def() else {
        log::error!(log::channel!("risks"), "No '{}' tile def in the objects tile set: no rubble placed.", RUBBLE_TILE.string);
        return 0;
    };

    let spawner = Spawner::new(context);
    let mut placed = 0;

    for cell in &area {
        let result = spawner.try_spawn_tile_with_def(cell, rubble_def);
        if let crate::world::object::SpawnerResult::Err(err) = result {
            log::warning!(log::channel!("risks"), "No rubble at {cell}: {}", err.message);
        } else {
            placed += 1;
        }
    }

    update_variations(context.tile_map_mut(), area);
    placed
}
