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
    map::{
        Tile,
        TileMap,
        TileMapLayer,
        TileMapLayerKind
    }
};

// ----------------------------------------------
// Tile placements helpers
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum PlacementOp<'tile_sets> {
    Place(&'tile_sets TileDef),
    Clear,
    None,
}

pub fn try_place_tile_in_layer<'tile_map, 'tile_sets>(layer: &'tile_map mut TileMapLayer<'tile_sets>,
                                                      target_cell: Cell,
                                                      tile_def_to_place: &'tile_sets TileDef) -> Option<&'tile_map mut Tile<'tile_sets>> {

    debug_assert!(tile_def_to_place.is_valid());
    debug_assert!(tile_def_to_place.layer_kind() == layer.kind());

    if !layer.is_cell_within_bounds(target_cell) {
        return None;
    }

    // Terrain tiles are always allowed to replace existing tiles,
    // so first clear the cell in case there's already a tile there.
    if tile_def_to_place.is(TileKind::Terrain) {
        debug_assert!(layer.kind() == TileMapLayerKind::Terrain);
        layer.remove_tile(target_cell);
    }

    // First check if the whole cell range is free:
    let cell_range = tile_def_to_place.calc_footprint_cells(target_cell);
    for cell in &cell_range {
        if !layer.is_cell_within_bounds(cell) {
            // One or more cells for this tile fall outside of the map.
            return None;
        }

        if layer.try_tile(cell).is_some() {
            // One of the cells for this tile is already occupied.
            return None;
        }
    }

    // Place base tile.
    let did_place_tile = layer.insert_tile(target_cell, tile_def_to_place);
    assert!(did_place_tile);

    // Check if we have to place any child blockers too for larger tiles.
    if tile_def_to_place.has_multi_cell_footprint() {
        let did_place_blocker = layer.insert_blocker_tiles(cell_range, target_cell);
        assert!(did_place_blocker);
    }

    // Placement successful.
    layer.try_tile_mut(target_cell)
}

pub fn try_clear_tile_from_layer(layer: &mut TileMapLayer,
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

pub fn try_place_tile_at_cursor<'tile_map, 'tile_sets>(tile_map: &'tile_map mut TileMap<'tile_sets>,
                                                       cursor_screen_pos: Vec2,
                                                       transform: &WorldToScreenTransform,
                                                       tile_def_to_place: &'tile_sets TileDef) -> Option<&'tile_map mut Tile<'tile_sets>> {

    debug_assert!(transform.is_valid());
    debug_assert!(tile_def_to_place.is_valid());

    let layer_kind = tile_def_to_place.layer_kind();
    let layer = tile_map.layer_mut(layer_kind);

    let target_cell = layer.find_exact_cell_for_point(
        cursor_screen_pos,
        transform);

    return try_place_tile_in_layer(layer, target_cell, tile_def_to_place);
}

pub fn try_clear_tile_at_cursor(tile_map: &mut TileMap,
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
