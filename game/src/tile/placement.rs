use strum::IntoEnumIterator;

use super::{
    sets::TileDef, Tile, TileKind, TileMap, TileMapLayer, TileMapLayerKind, TilePoolIndex,
};
use crate::{
    debug,
    utils::{
        coords::{Cell, WorldToScreenTransform},
        Vec2,
    },
};

// ----------------------------------------------
// Tile placements helpers
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum PlacementOp {
    Place(&'static TileDef),
    Invalidate(&'static TileDef),
    Clear,
    None,
}

pub fn try_place_tile_in_layer<'tile_map>(layer: &'tile_map mut TileMapLayer,
                                          target_cell: Cell,
                                          tile_def_to_place: &'static TileDef)
                                          -> Result<(&'tile_map mut Tile, usize), String> {
    debug_assert!(tile_def_to_place.is_valid());
    debug_assert!(tile_def_to_place.layer_kind() == layer.kind());

    if !layer.is_cell_within_bounds(target_cell) {
        return Err(format!("'{}' - {}: Target cell {target_cell} is out of bounds",
                           tile_def_to_place.name,
                           layer.kind()));
    }

    let mut allow_stacking = false;

    if tile_def_to_place.is(TileKind::Terrain) {
        debug_assert!(layer.kind() == TileMapLayerKind::Terrain);

        // Terrain tiles are always allowed to replace existing tiles,
        // so first clear the cell in case there's already a tile there.
        if let Some(existing_tile) = layer.try_tile(target_cell) {
            // Avoid any work if we already have the same terrain tile.
            if existing_tile.tile_def().hash == tile_def_to_place.hash {
                return Err(format!("Cell {target_cell} already contains '{}'",
                                   tile_def_to_place.name));
            }

            layer.remove_tile(target_cell);
        }
    } else if tile_def_to_place.is(TileKind::Unit) {
        debug_assert!(layer.kind() == TileMapLayerKind::Objects);

        // Units are allowed to stack on top of each other so we
        // can support multiple units walking the same tile.
        allow_stacking = true;
    }

    // First check if the whole cell range is free:
    let cell_range = tile_def_to_place.cell_range(target_cell);
    for cell in &cell_range {
        if !layer.is_cell_within_bounds(cell) {
            return Err(format!("'{}' - {}: Target cell {cell} for this tile falls outside of the map bounds",
                               tile_def_to_place.name, layer.kind()));
        }

        if !allow_stacking && layer.try_tile(cell).is_some() {
            return Err(format!("'{}' - {}: Target cell {cell} for this tile is already occupied by '{}'",
                               tile_def_to_place.name, layer.kind(), debug::tile_name_at(cell, layer.kind())));
        }
    }

    // Place base tile.
    let did_place_tile = layer.insert_tile(target_cell, tile_def_to_place, allow_stacking);
    assert!(did_place_tile);

    // Check if we have to place any child blockers too for larger tiles.
    if tile_def_to_place.occupies_multiple_cells() {
        let did_place_blocker = layer.insert_blocker_tiles(cell_range, target_cell);
        assert!(did_place_blocker);
    }

    // Placement successful.
    let new_pool_capacity = layer.pool_capacity();
    let new_tile = layer.try_tile_mut(target_cell).unwrap();
    Ok((new_tile, new_pool_capacity))
}

pub fn try_clear_tile_from_layer(layer: &mut TileMapLayer,
                                 target_cell: Cell)
                                 -> Result<(), String> {
    if let Some(tile) = layer.try_tile(target_cell) {
        // Make sure we clear the base tile + any child blockers.
        for cell in &tile.cell_range() {
            let did_remove_tile = layer.remove_tile(cell);
            assert!(did_remove_tile);
        }
        Ok(())
    } else {
        // Already empty.
        Err(format!("Cell {target_cell} in layer {} is already empty.", layer.kind()))
    }
}

pub fn try_clear_tile_from_layer_by_index(layer: &mut TileMapLayer,
                                          target_index: TilePoolIndex,
                                          target_cell: Cell)
                                          -> Result<(), String> {
    if let Some(tile) = layer.try_tile(target_cell) {
        // For now only Units are supported.
        debug_assert!(tile.is(TileKind::Unit));
        debug_assert!(!tile.occupies_multiple_cells());

        let mut found_tile = false;

        // Find which tile in the stack we are removing:
        if tile.index() == target_index {
            found_tile = true;
        } else {
            layer.visit_next_tiles(tile.next_index, |next_tile| {
                     if next_tile.index() == target_index {
                         found_tile = true;
                     }
                 });
        }

        if found_tile {
            let did_remove_tile = layer.remove_tile_by_index(target_index, target_cell);
            assert!(did_remove_tile);
            Ok(())
        } else {
            Err(format!("Failed to find tile for index: {target_index:?}, cell: {target_cell}."))
        }
    } else {
        // Already empty.
        Err(format!("Cell {target_cell} in layer {} is already empty.", layer.kind()))
    }
}

pub fn try_place_tile_at_cursor<'tile_map>(tile_map: &'tile_map mut TileMap,
                                           cursor_screen_pos: Vec2,
                                           transform: WorldToScreenTransform,
                                           tile_def_to_place: &'static TileDef)
                                           -> Result<(&'tile_map mut Tile, usize), String> {
    debug_assert!(transform.is_valid());
    debug_assert!(tile_def_to_place.is_valid());

    let layer_kind = tile_def_to_place.layer_kind();
    let layer = tile_map.layer_mut(layer_kind);

    let target_cell = layer.find_exact_cell_for_point(cursor_screen_pos, transform);

    try_place_tile_in_layer(layer, target_cell, tile_def_to_place)
}

pub fn try_clear_tile_at_cursor(tile_map: &mut TileMap,
                                cursor_screen_pos: Vec2,
                                transform: WorldToScreenTransform)
                                -> Result<(), String> {
    debug_assert!(transform.is_valid());

    // Clear the topmost layer tile under the target cell.
    for layer_kind in TileMapLayerKind::iter().rev() {
        let layer = tile_map.layer_mut(layer_kind);

        let target_cell = layer.find_exact_cell_for_point(cursor_screen_pos, transform);

        match try_clear_tile_from_layer(layer, target_cell) {
            Ok(_) => return Ok(()),
            _ => continue,
        }
    }

    // Nothing removed.
    Err(format!("No tile found at cursor position {cursor_screen_pos}"))
}
