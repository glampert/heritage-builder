use strum::IntoEnumIterator;

use crate::{
    utils::{
        Vec2,
        coords::{
            Cell,
            WorldToScreenTransform
        }
    }
};

use super::{
    sets::{TileDef, TileKind},
    map::{Tile, TileMap, TileMapLayer, TileMapLayerKind}
};

// ----------------------------------------------
// Tile placements helpers
// ----------------------------------------------

/*
pub fn cells_overlap(lhs_cells: &TileFootprintList, rhs_cells: &TileFootprintList) -> bool {
    for lhs_cell in lhs_cells {
        for rhs_cell in rhs_cells {
            if lhs_cell == rhs_cell {
                return true;
            }
        }
    }
    false
}
*/

pub fn try_place_tile_in_layer<'tile_sets>(layer: &mut TileMapLayer<'tile_sets>,
                                           target_cell: Cell,
                                           tile_def_to_place: &'tile_sets TileDef) -> bool {

    debug_assert!(tile_def_to_place.is_valid());
    debug_assert!(tile_def_to_place.layer_kind() == layer.kind());

    // TODO: overlap checks !!!

    // Place base tile.
    if !layer.insert_tile(target_cell, tile_def_to_place) {
        return false;
    }

    // Check if we have to place any child blockers too for larger tiles.
    if tile_def_to_place.has_multi_cell_footprint() {
        let cell_range = tile_def_to_place.calc_footprint_cells(target_cell);
        layer.insert_blocker_tiles(cell_range, target_cell);
    }

    /*
    // Overlap checks for buildings:
    if tile_def_to_place.is_building() {
        debug_assert!(layer_kind == TileMapLayerKind::Buildings);

        // Building -> unit overlap check:
        if tile_map.has_tile(target_cell, TileMapLayerKind::Units, TileKind::Unit) {
            return false;
        }

        // Check for building overlap:
        if tile_map.has_tile(target_cell, layer_kind, TileKind::Building | TileKind::Blocker) {
            let current_footprint =
                Tile::calc_exact_footprint_cells(target_cell, tile_map.layer(layer_kind));

            let target_footprint =
                tile_def_to_place.calc_footprint_cells(target_cell);

            if cells_overlap(&current_footprint, &target_footprint) {
                return false; // Cannot place building here.
            }
        }

        // Multi-tile building?
        if tile_def_to_place.has_multi_cell_footprint() {
            let target_footprint = tile_def_to_place.calc_footprint_cells(target_cell);

            // Check if placement is allowed:
            for footprint_cell in &target_footprint {
                if !tile_map.is_cell_within_bounds(*footprint_cell) {
                    // If any cell would fall outside of the map bounds we won't place.
                    return false;
                }

                if tile_map.has_tile(*footprint_cell, layer_kind, TileKind::Building | TileKind::Blocker) {
                    return false; // Cannot place building here.
                }

                // Building blocker -> unit overlap check:
                if tile_map.has_tile(*footprint_cell, TileMapLayerKind::Units, TileKind::Unit) {
                    return false; // Cannot place building here.
                }
            }

            let owner_flags = tile_def_to_place.tile_flags();

            for footprint_cell in target_footprint {
                if footprint_cell != target_cell {
                    if let Some(current_tile) = tile_map.try_tile_from_layer_mut(footprint_cell, layer_kind) {
                        current_tile.set_as_blocker(target_cell, owner_flags);
                    }
                }
            }
        }
    }
    // Unit -> building overlap check:
    else if tile_def_to_place.is_unit() {
        debug_assert!(layer_kind == TileMapLayerKind::Units);

        // Check overlap with buildings:
        if tile_map.has_tile(target_cell, TileMapLayerKind::Buildings,
            TileKind::Building | TileKind::Blocker) {

            return false; // Can't place unit over building or building blocker cell.
        }
    }

    if let Some(current_tile) = tile_map.try_tile_from_layer_mut(target_cell, layer_kind) {
        current_tile.reset_def(tile_def_to_place);
        return true; // Tile placed successfully.
    }
    */

    false // Nothing placed.
}

pub fn try_clear_tile_from_layer<'tile_sets>(layer: &mut TileMapLayer<'tile_sets>,
                                             target_cell: Cell) -> bool {

    if let Some(tile) = layer.try_tile(target_cell) {
        // Make sure we clear the base tile + any child blockers.
        for cell in &tile.cell_range() {
            let did_remove_tile = layer.remove_tile(cell);
            assert!(did_remove_tile);
        }
        true
    } else {
        // Already empty.
        false
    }
}

pub fn try_place_tile_at_cursor<'tile_sets>(tile_map: &mut TileMap<'tile_sets>,
                                            cursor_screen_pos: Vec2,
                                            transform: &WorldToScreenTransform,
                                            tile_def_to_place: &'tile_sets TileDef) -> bool {

    debug_assert!(transform.is_valid());
    debug_assert!(tile_def_to_place.is_valid());

    let layer_kind = tile_def_to_place.layer_kind();
    let layer = tile_map.layer_mut(layer_kind);

    let target_cell = layer.find_exact_cell_for_point(
        cursor_screen_pos,
        transform);

    return try_place_tile_in_layer(layer, target_cell, tile_def_to_place);
}

pub fn try_clear_tile_at_cursor<'tile_sets>(tile_map: &mut TileMap<'tile_sets>,
                                            cursor_screen_pos: Vec2,
                                            transform: &WorldToScreenTransform) -> bool {

    debug_assert!(transform.is_valid());

    // Clear the topmost layer tile under the target cell.
    for layer_kind in TileMapLayerKind::iter().rev() {
        let layer = tile_map.layer_mut(layer_kind);

        let target_cell = layer.find_exact_cell_for_point(
            cursor_screen_pos,
            transform);

        if try_clear_tile_from_layer(layer, target_cell) {
            return true;
        }
    }

    false // Nothing removed.
}
