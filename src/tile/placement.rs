use strum::IntoEnumIterator;

use crate::{
    utils::{Cell2D, Point2D, WorldToScreenTransform}
};

use super::{
    def::{self, TileDef, TileKind},
    map::{self, Tile, TileMap, TileMapLayerKind}
};

// ----------------------------------------------
// Tile placements helpers
// ----------------------------------------------

pub fn try_place_tile_in_layer<'a>(tile_map: &mut TileMap<'a>,
                                   kind: TileMapLayerKind,
                                   target_cell: Cell2D,
                                   tile_to_place: &'a TileDef) -> bool {

    debug_assert!(tile_map.is_cell_within_bounds(target_cell));
    debug_assert!(tile_to_place.is_empty() == false);
    debug_assert!(map::tile_kind_to_layer(tile_to_place.kind) == kind);

    // Overlap checks for buildings:
    if tile_to_place.is_building() {
        debug_assert!(kind == TileMapLayerKind::Buildings);

        // Building -> unit overlap check:
        if tile_map.has_tile(target_cell, TileMapLayerKind::Units, &[TileKind::Unit]) {
            return false;
        }

        // Check for building overlap:
        if tile_map.has_tile(target_cell, kind, &[TileKind::Building, TileKind::Blocker]) {
            let current_footprint =
                Tile::calc_exact_footprint_cells(target_cell, tile_map.layer(kind));

            let target_footprint =
                tile_to_place.calc_footprint_cells(target_cell);

            if def::cells_overlap(&current_footprint, &target_footprint) {
                return false; // Cannot place building here.
            }
        }

        // Multi-tile building?
        if tile_to_place.has_multi_cell_footprint() {
            let target_footprint = tile_to_place.calc_footprint_cells(target_cell);

            // Check if placement is allowed:
            for footprint_cell in &target_footprint {
                if !tile_map.is_cell_within_bounds(*footprint_cell) {
                    // If any cell would fall outside of the map bounds we won't place.
                    return false;
                }

                if tile_map.has_tile(*footprint_cell, kind, &[TileKind::Building, TileKind::Blocker]) {
                    return false; // Cannot place building here.
                }

                // Building blocker -> unit overlap check:
                if tile_map.has_tile(*footprint_cell, TileMapLayerKind::Units, &[TileKind::Unit]) {
                    return false; // Cannot place building here.
                }
            }

            let owner_flags = tile_to_place.tile_flags();

            for footprint_cell in target_footprint {
                if footprint_cell != target_cell {
                    if let Some(current_tile) = tile_map.try_tile_from_layer_mut(footprint_cell, kind) {
                        current_tile.set_as_blocker(target_cell, owner_flags);
                    }
                }
            }
        }
    }
    // Unit -> building overlap check:
    else if tile_to_place.is_unit() {
        debug_assert!(kind == TileMapLayerKind::Units);

        // Check overlap with buildings:
        if tile_map.has_tile(target_cell, TileMapLayerKind::Buildings,
            &[TileKind::Building, TileKind::Blocker]) {

            return false; // Can't place unit over building or building blocker cell.
        }
    }

    if let Some(current_tile) = tile_map.try_tile_from_layer_mut(target_cell, kind) {
        current_tile.set_def(tile_to_place);
        return true; // Tile placed successfully.
    }

    false // Nothing placed.
}

pub fn try_clear_tile_from_layer<'a>(tile_map: &mut TileMap<'a>,
                                     kind: TileMapLayerKind,
                                     target_cell: Cell2D) -> bool {

    debug_assert!(tile_map.is_cell_within_bounds(target_cell));

    // Tile removal/clearing: Handle removing multi-tile buildings.
    if tile_map.has_tile(target_cell, kind, &[TileKind::Building, TileKind::Blocker]) {
        let target_footprint =
            Tile::calc_exact_footprint_cells(target_cell, tile_map.layer(kind));

        for footprint_cell in target_footprint {
            if footprint_cell != target_cell {
                if let Some(current_tile) = tile_map.try_tile_from_layer_mut(footprint_cell, kind) {
                    current_tile.set_as_empty();
                }
            }
        }
    }

    if let Some(current_tile) = tile_map.try_tile_from_layer_mut(target_cell, kind) {
        current_tile.set_as_empty();
        return true; // Tile placed successfully.
    }

    false // Nothing placed.
}

pub fn try_place_tile_at_cursor<'a>(tile_map: &mut TileMap<'a>,
                                    cursor_screen_pos: Point2D,
                                    transform: &WorldToScreenTransform,
                                    tile_to_place: &'a TileDef) -> bool {

    debug_assert!(tile_to_place.is_empty() == false);

    let layer_kind = map::tile_kind_to_layer(tile_to_place.kind);

    let target_cell = tile_map.find_exact_cell_for_point(
        layer_kind,
        cursor_screen_pos,
        transform);

    if tile_map.is_cell_within_bounds(target_cell) {
        return try_place_tile_in_layer(tile_map, layer_kind, target_cell, tile_to_place);
    }

    false // Nothing placed.
}

pub fn try_clear_tile_at_cursor<'a>(tile_map: &mut TileMap<'a>,
                                    cursor_screen_pos: Point2D,
                                    transform: &WorldToScreenTransform) -> bool {

    // If placing an empty tile we will actually clear the topmost layer under that cell.
    for layer_kind in TileMapLayerKind::iter().rev() {

        let target_cell = tile_map.find_exact_cell_for_point(
            layer_kind,
            cursor_screen_pos,
            transform);

        if tile_map.is_cell_within_bounds(target_cell) {
            if let Some(existing_tile) = tile_map.try_tile_from_layer(target_cell, layer_kind) {
                if !existing_tile.is_empty() {
                    return try_clear_tile_from_layer(tile_map, layer_kind, target_cell);
                }
            }
        }      
    }

    false // Nothing placed.
}
