use strum_macros::Display;
use super::{water, sets::TileDef, Tile, TileKind, TileMapLayer, TileMapLayerRefs, TileMapLayerKind, TilePoolIndex};
use crate::{debug, pathfind::{self, NodeKind as PathNodeKind}, utils::coords::Cell};

// ----------------------------------------------
// Tile placement error handling
// ----------------------------------------------

pub enum Placement {}
pub enum Clearing  {}

pub trait TileErrorDomain {
    type Err;
    type Reason;
}

impl TileErrorDomain for Placement {
    type Err = TilePlacementErr;
    type Reason = TilePlacementErrReason;
}

impl TileErrorDomain for Clearing {
    type Err = TileClearingErr;
    type Reason = TileClearingErrReason;
}

// Usage: `err!(Placement::InvalidLocation, "message")`
macro_rules! err {
    ($domain:ident :: $variant:ident $(($($args:expr),* $(,)?))?, $($fmt:tt)*) => {{
        type InnerErrT    = < $domain as $crate::tile::placement::TileErrorDomain >::Err;
        type InnerReasonT = < $domain as $crate::tile::placement::TileErrorDomain >::Reason;
        Err(InnerErrT {
            reason: InnerReasonT::$variant $(($($args),*))?,
            message: format!($($fmt)*),
        })
    }};
}

// Usage: `err_reason!(Placement, err.reason, "message")`
macro_rules! err_reason {
    ($domain:ident, $reason:expr, $($fmt:tt)*) => {{
        type InnerErrT = < $domain as $crate::tile::placement::TileErrorDomain >::Err;
        Err(InnerErrT {
            reason: $reason,
            message: format!($($fmt)*),
        })
    }};
}

pub(crate) use err;
pub(crate) use err_reason;

// ----------------------------------------------
// TilePlacementErr
// ----------------------------------------------

pub struct TilePlacementErr {
    pub reason: TilePlacementErrReason,
    pub message: String,
}

#[derive(Copy, Clone, Display)]
pub enum TilePlacementErrReason {
    EmptyMap,
    CellOutOfBounds,
    SpawnFailed,
    CannotAffordCost,
    RequiresProximity(PathNodeKind), // Required node proximity violated.
    Obstruction(&'static TileDef),   // Tile obstructing the placement.
}

// ----------------------------------------------
// TileClearingErr
// ----------------------------------------------

pub struct TileClearingErr {
    pub reason: TileClearingErrReason,
    pub message: String,
}

#[derive(Copy, Clone, Display)]
pub enum TileClearingErrReason {
    EmptyMap,
    EmptyCell,
    CellOutOfBounds,
    DespawnFailed,
}

// ----------------------------------------------
// TilePlacementOp
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum TilePlacementOp {
    Place(&'static TileDef),
    Invalidate(&'static TileDef),
    Clear,
    None,
}

// ----------------------------------------------
// Tile placement internal helpers
// ----------------------------------------------

pub mod internal {
    use super::*;

    pub fn is_placement_on_terrain_valid(layers: TileMapLayerRefs,
                                         target_cell: Cell,
                                         tile_def_to_place: &'static TileDef)
                                         -> Result<(), TilePlacementErr> {
        debug_assert!(tile_def_to_place.is_valid());

        if !target_cell.is_valid() {
            return err!(Placement::CellOutOfBounds, "Invalid cell!");
        }

        if tile_def_to_place.is(TileKind::Object) {
            if water::is_port_or_wharf(tile_def_to_place) {
                let cell_range = tile_def_to_place.cell_range(target_cell);

                // Ports/wharfs must be contained withing water tiles...
                for cell in &cell_range {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell) {
                        if !tile.path_kind().is_water() {
                            return err!(Placement::RequiresProximity(PathNodeKind::Water),
                                        "Building must be placed near the water edge.");
                        }
                    }
                }

                // But also neighboring some kind of land tile, which means
                // they are only placeable at a water edge tile boundary.
                let mut is_near_land = false;
                pathfind::for_each_surrounding_cell(cell_range, |cell| {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell) {
                        if !tile.path_kind().is_water() {
                            is_near_land = true;
                            return false; // done
                        }
                    }
                    true // continue
                });

                if !is_near_land {
                    return err!(Placement::RequiresProximity(PathNodeKind::Water),
                                "Building must be placed near the water edge.");
                }
            } else {
                let has_proximity_requirements = !tile_def_to_place.required_proximity.is_empty();
                let mut found_proximity_requirements = false;

                for cell in &tile_def_to_place.cell_range(target_cell) {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell) {
                        let path_kind = tile.path_kind();

                        if tile_def_to_place.is(TileKind::Object) && tile_def_to_place.flying_object
                            && !path_kind.is_flying_object_placeable()
                        {
                            return err!(Placement::Obstruction(tile.tile_def()),
                                        "Cannot place flying object '{}' over terrain tile '{}'.",
                                        tile_def_to_place.name, tile.name());
                        } else if tile_def_to_place.is(TileKind::Unit) && !tile_def_to_place.flying_object
                            && !path_kind.is_unit_placeable()
                        {
                            return err!(Placement::Obstruction(tile.tile_def()),
                                        "Cannot place unit '{}' over terrain tile '{}'.",
                                        tile_def_to_place.name, tile.name());
                        } else if tile_def_to_place.is(TileKind::Rocks | TileKind::Vegetation)
                                && !path_kind.is_object_placeable()
                        {
                            return err!(Placement::Obstruction(tile.tile_def()),
                                        "Cannot place object prop '{}' over terrain tile '{}'.",
                                        tile_def_to_place.name, tile.name());
                        } else if tile_def_to_place.is(TileKind::Building)
                                && !path_kind.is_object_placeable()
                        {
                            let can_place_building =
                                path_kind.is_vacant_lot() && tile_def_to_place.is_house();

                            if !can_place_building {
                                return err!(Placement::Obstruction(tile.tile_def()),
                                            "Cannot place building '{}' over terrain tile '{}'.",
                                            tile_def_to_place.name, tile.name());
                            }
                        }

                        // Tile must be placed near water/rocks/etc.
                        if has_proximity_requirements && !found_proximity_requirements {
                            let neighbors =
                                layers.get(TileMapLayerKind::Terrain).tile_neighbors(cell, false);
                            let is_near = neighbors
                                .iter()
                                .flatten()
                                .any(|neighbor| neighbor.path_kind().intersects(tile_def_to_place.required_proximity));
                            found_proximity_requirements = is_near;
                        }

                        // Check requirements again in the objects layer.
                        if has_proximity_requirements && !found_proximity_requirements {
                            let neighbors =
                                layers.get(TileMapLayerKind::Objects).tile_neighbors(cell, false);
                            let is_near = neighbors
                                .iter()
                                .flatten()
                                .any(|neighbor| neighbor.path_kind().intersects(tile_def_to_place.required_proximity));
                            found_proximity_requirements = is_near;
                        }
                    }
                }

                if has_proximity_requirements && !found_proximity_requirements {
                    return err!(Placement::RequiresProximity(tile_def_to_place.required_proximity),
                                "Building must be placed near {}.",
                                tile_def_to_place.required_proximity);
                }
            }
        } else if tile_def_to_place.path_kind.is_vacant_lot() {
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell) {
                if tile.path_kind().intersects(PathNodeKind::Road
                                            | PathNodeKind::Water
                                            | PathNodeKind::Building
                                            | PathNodeKind::SettlersSpawnPoint)
                {
                    return err!(Placement::Obstruction(tile.tile_def()),
                                "Cannot place vacant lot over terrain tile '{}'.",
                                tile.name());
                }
            }

            // Objects layer mut be empty.
            if let Some(object) = layers.get(TileMapLayerKind::Objects).try_tile(target_cell) {
                return err!(Placement::Obstruction(object.tile_def()),
                            "Cannot place vacant lot here! Cell already occupied by an object.");
            }
        } else if tile_def_to_place.path_kind.intersects(PathNodeKind::Road | PathNodeKind::SettlersSpawnPoint) {
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell) {
                if tile.path_kind().is_water() {
                    return err!(Placement::Obstruction(tile.tile_def()),
                                "Cannot place road over terrain tile '{}'.",
                                tile.name());
                }
            }
        } else if tile_def_to_place.path_kind.intersects(PathNodeKind::EmptyLand | PathNodeKind::Water) {
            // Land/water can only be placed over other land/water tiles.
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell) {
                if !tile.path_kind().intersects(PathNodeKind::EmptyLand | PathNodeKind::Water) {
                    return err!(Placement::Obstruction(tile.tile_def()),
                                "Cannot place '{}' tile over terrain tile '{}'.",
                                tile_def_to_place.name, tile.name());
                }
            }

            // Cannot place water under existing objects.
            if tile_def_to_place.path_kind.is_water()
                && let Some(object) = layers.get(TileMapLayerKind::Objects).try_tile(target_cell)
            {
                return err!(Placement::Obstruction(object.tile_def()),
                            "Cannot place water tile here! Cell already occupied by an object.");
            }
        }

        Ok(())
    }

    pub fn try_place_tile_in_layer<'tile_map>(layer: &'tile_map mut TileMapLayer,
                                              target_cell: Cell,
                                              tile_def_to_place: &'static TileDef)
                                              -> Result<(&'tile_map mut Tile, usize), TilePlacementErr> {
        debug_assert!(tile_def_to_place.is_valid());
        debug_assert!(tile_def_to_place.layer_kind() == layer.kind());

        if !layer.is_cell_within_bounds(target_cell) {
            return err!(Placement::CellOutOfBounds,
                        "'{}' - {}: Target cell {target_cell} is out of bounds",
                        tile_def_to_place.name, layer.kind());
        }

        let mut allow_stacking = false;

        if tile_def_to_place.is(TileKind::Terrain) {
            debug_assert!(layer.kind() == TileMapLayerKind::Terrain);

            // Terrain tiles are always allowed to replace existing tiles,
            // so first clear the cell in case there's already a tile there.
            if let Some(existing_tile) = layer.try_tile(target_cell) {
                // Avoid any work if we already have the same terrain tile.
                if existing_tile.tile_def().hash == tile_def_to_place.hash {
                    return err!(Placement::Obstruction(existing_tile.tile_def()),
                                "Cell {target_cell} already contains '{}'",
                                tile_def_to_place.name);
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
                return err!(Placement::CellOutOfBounds,
                            "'{}' - {}: Target cell {cell} for this tile falls outside of the map bounds",
                            tile_def_to_place.name, layer.kind());
            }

            if allow_stacking {
                if let Some(existing_tile) = layer.try_tile(cell) {
                    if !existing_tile.is(TileKind::Unit) {
                        return err!(Placement::Obstruction(existing_tile.tile_def()),
                                    "'{}' - {}: Target cell {cell} for this unit is already occupied by '{}'",
                                    tile_def_to_place.name, layer.kind(), debug::tile_name_at(cell, layer.kind()));
                    }
                }
            } else if let Some(existing_tile) = layer.try_tile(cell) {
                return err!(Placement::Obstruction(existing_tile.tile_def()),
                            "'{}' - {}: Target cell {cell} for this tile is already occupied by '{}'",
                            tile_def_to_place.name, layer.kind(), debug::tile_name_at(cell, layer.kind()));
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
                                     -> Result<&'static TileDef, TileClearingErr> {
        if !layer.is_cell_within_bounds(target_cell) {
            return err!(Clearing::CellOutOfBounds,
                        "{}: Target cell {} is out of bounds",
                        layer.kind(), target_cell);
        }

        if let Some(tile) = layer.try_tile(target_cell) {
            let tile_def = tile.tile_def();

            // Make sure we clear the base tile + any child blockers.
            for cell in &tile.cell_range() {
                let did_remove_tile = layer.remove_tile(cell);
                assert!(did_remove_tile);
            }

            Ok(tile_def)
        } else {
            // Already empty.
            err!(Clearing::EmptyCell, "Cell {target_cell} in layer {} is already empty.", layer.kind())
        }
    }

    pub fn try_clear_tile_from_layer_by_index(layer: &mut TileMapLayer,
                                              target_index: TilePoolIndex,
                                              target_cell: Cell)
                                              -> Result<&'static TileDef, TileClearingErr> {
        if !layer.is_cell_within_bounds(target_cell) {
            return err!(Clearing::CellOutOfBounds,
                        "{}: Target cell {} is out of bounds",
                        layer.kind(), target_cell);
        }

        if let Some(tile) = layer.try_tile(target_cell) {
            // For now only Units are supported.
            debug_assert!(tile.is(TileKind::Unit));
            debug_assert!(!tile.occupies_multiple_cells());

            let tile_def = tile.tile_def();
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
                Ok(tile_def)
            } else {
                err!(Clearing::CellOutOfBounds, "Failed to find tile for index: {target_index:?}, cell: {target_cell}.")
            }
        } else {
            // Already empty.
            err!(Clearing::EmptyCell, "Cell {target_cell} in layer {} is already empty.", layer.kind())
        }
    }
}
