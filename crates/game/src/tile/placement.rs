use common::coords::Cell;
use strum::Display;

use super::{Tile, TileKind, TileMapLayer, TileMapLayerKind, TileMapLayerRefs, TilePoolIndex, sets::TileDef, wall, water};
use crate::{
    debug,
    pathfind::{self, NodeKind as PathNodeKind},
};

// ----------------------------------------------
// Tile placement error handling
// ----------------------------------------------

pub enum Placement {}
pub enum Clearing {}

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

#[derive(Clone)]
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
    CannotAffordMaterials,
    RequiresProximity(PathNodeKind), // Required node proximity violated.
    RequiresDyke,                    // A sluice gate away from every dyke.
    RequiresWall,                    // A gatehouse not in a wall: between two pieces, on one or in a gap.
    Obstruction(&'static TileDef),   // Tile obstructing the placement.
    // Terrain height (docs/plans/terrain-height.md, §7):
    UnevenGround,       // A footprint on a ramp, or across levels.
    BesideHigherGround, // A building beside ground more than a level higher at its side (Q4).
    RaisedGround,       // Water, a canal, or a dock or wharf away from the ground at level 0.
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
    use crate::tile::elevation::{ElevationMap, Shape};

    // Where the ground's height doesn't take a tile (docs/plans/terrain-height.md, §7): a building's,
    // a field's, a prop's or a lot's footprint must be flat at one level; a building mustn't stand
    // beside ground more than a level above it at its sides (Q4); water and canals stay flat at level
    // 0 (Q3), and a dyke on flat ground. Roads may climb ramps (road::build_segment keeps them on
    // ground that joins). A flat map takes anything.
    pub fn is_placement_on_heights_valid(
        layers: TileMapLayerRefs,
        elevation: &ElevationMap,
        target_cell: Cell,
        tile_def_to_place: &'static TileDef,
    ) -> Result<(), TilePlacementErr> {
        if elevation.is_flat() || !target_cell.is_valid() {
            return Ok(());
        }
        let cells = tile_def_to_place.cell_range(target_cell);

        if tile_def_to_place.is(TileKind::Unit) {
            return Ok(()); // Walkers stand anywhere, ramps included; ships keep to water.
        }

        if tile_def_to_place.is(TileKind::Terrain) {
            let path_kind = tile_def_to_place.path_kind;
            if path_kind.is_water() {
                if cells.iter().any(|cell| elevation.is_raised(cell)) {
                    return err!(Placement::RaisedGround, "Water stays at the ground's lowest level.");
                }
            } else if (tile_def_to_place.blocks_floods || path_kind.is_vacant_lot())
                && cells.iter().any(|cell| elevation.shape(cell) != Shape::Flat)
            {
                return err!(Placement::UnevenGround, "'{}' needs flat ground.", tile_def_to_place.name);
            }
            return Ok(());
        }

        if water::is_port_or_wharf(tile_def_to_place) {
            // On the water, which is at level 0.
            if cells.iter().any(|cell| elevation.is_raised(cell)) {
                return err!(
                    Placement::RaisedGround,
                    "'{}' stands on the water, at the ground's lowest level.",
                    tile_def_to_place.name
                );
            }
            return Ok(());
        }

        // Every footprint cell flat, at one level.
        let level = elevation.level(target_cell);
        if cells.iter().any(|cell| !elevation.is_flat_at(cell, level)) {
            return err!(Placement::UnevenGround, "'{}' needs flat ground at one level.", tile_def_to_place.name);
        }

        if tile_def_to_place.is(TileKind::Building) {
            // Not beside ground more than a level above it at its sides (Q4): the cells beside its
            // footprint's left and right corners, on its row, sort with it.
            let left = Cell::new(cells.start.x - 1, cells.end.y + 1);
            let right = Cell::new(cells.end.x + 1, cells.start.y - 1);
            for side in [left, right] {
                if elevation.is_cell_within_bounds(side) && elevation.centre_height(side) > f32::from(level) + 1.0 {
                    return err!(
                        Placement::BesideHigherGround,
                        "'{}' can't stand beside ground more than a level above it.",
                        tile_def_to_place.name
                    );
                }
            }

            // A sluice gate's dyke on its own level.
            if tile_def_to_place.requires_dyke {
                let mut is_beside_dyke = false;
                pathfind::for_each_surrounding_cell(cells, |cell| {
                    is_beside_dyke = layers
                        .get(TileMapLayerKind::Terrain)
                        .try_tile(cell)
                        .is_some_and(|tile| tile.tile_def().blocks_floods && elevation.is_flat_at(cell, level));
                    !is_beside_dyke
                });
                if !is_beside_dyke {
                    return err!(
                        Placement::RequiresDyke,
                        "'{}' must be placed beside a dyke on its own level.",
                        tile_def_to_place.name
                    );
                }
            }
        }

        Ok(())
    }

    pub fn is_placement_on_terrain_valid(
        layers: TileMapLayerRefs,
        target_cell: Cell,
        tile_def_to_place: &'static TileDef,
    ) -> Result<(), TilePlacementErr> {
        debug_assert!(tile_def_to_place.is_valid());

        if !target_cell.is_valid() {
            return err!(Placement::CellOutOfBounds, "Invalid cell!");
        }

        if tile_def_to_place.is(TileKind::Object) {
            if water::is_port_or_wharf(tile_def_to_place) {
                let cell_range = tile_def_to_place.cell_range(target_cell);

                // Ports/wharfs must be contained withing water tiles (not canals)...
                for cell in &cell_range {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell)
                        && (!tile.path_kind().is_water() || water::is_canal_def(tile.tile_def()))
                    {
                        return err!(
                            Placement::RequiresProximity(PathNodeKind::Water),
                            "Building must be placed near the water edge."
                        );
                    }
                }

                // But also neighboring some kind of land tile, which means
                // they are only placeable at a water edge tile boundary.
                let mut is_near_land = false;
                pathfind::for_each_surrounding_cell(cell_range, |cell| {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell)
                        && !tile.path_kind().is_water()
                    {
                        is_near_land = true;
                        return false; // done
                    }
                    true // continue
                });

                if !is_near_land {
                    return err!(
                        Placement::RequiresProximity(PathNodeKind::Water),
                        "Building must be placed near the water edge."
                    );
                }
            } else {
                let has_proximity_requirements = !tile_def_to_place.required_proximity.is_empty();
                let mut found_proximity_requirements = false;

                for cell in &tile_def_to_place.cell_range(target_cell) {
                    if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell) {
                        let path_kind = tile.path_kind();

                        if tile_def_to_place.is(TileKind::Object)
                            && tile_def_to_place.flying_object
                            && !path_kind.is_flying_object_placeable()
                        {
                            return err!(
                                Placement::Obstruction(tile.tile_def()),
                                "Cannot place flying object '{}' over terrain tile '{}'.",
                                tile_def_to_place.name,
                                tile.name()
                            );
                        } else if tile_def_to_place.is(TileKind::Unit) && tile_def_to_place.sails {
                            // A ship sails on open water only: never land or a canal.
                            if !path_kind.is_water() || water::is_canal_def(tile.tile_def()) {
                                return err!(
                                    Placement::Obstruction(tile.tile_def()),
                                    "Cannot place ship '{}' over terrain tile '{}'.",
                                    tile_def_to_place.name,
                                    tile.name()
                                );
                            }
                        } else if tile_def_to_place.is(TileKind::Unit)
                            && !tile_def_to_place.flying_object
                            && !path_kind.is_unit_placeable()
                        {
                            return err!(
                                Placement::Obstruction(tile.tile_def()),
                                "Cannot place unit '{}' over terrain tile '{}'.",
                                tile_def_to_place.name,
                                tile.name()
                            );
                        } else if tile_def_to_place
                            .is(TileKind::Rocks | TileKind::Vegetation | TileKind::Field | TileKind::Wall)
                            && !path_kind.is_object_placeable()
                        {
                            return err!(
                                Placement::Obstruction(tile.tile_def()),
                                "Cannot place object prop '{}' over terrain tile '{}'.",
                                tile_def_to_place.name,
                                tile.name()
                            );
                        } else if tile_def_to_place.is(TileKind::Building) && !path_kind.is_object_placeable() {
                            // A house on its lot; a gatehouse on the road where it crosses a wall.
                            let can_place_building = (path_kind.is_vacant_lot() && tile_def_to_place.is_house())
                                || (tile_def_to_place.requires_wall
                                    && path_kind.is_road()
                                    && !tile.tile_def().blocks_floods);

                            if !can_place_building {
                                return err!(
                                    Placement::Obstruction(tile.tile_def()),
                                    "Cannot place building '{}' over terrain tile '{}'.",
                                    tile_def_to_place.name,
                                    tile.name()
                                );
                            }
                        }

                        // Tile must be placed near water/rocks/etc.
                        if has_proximity_requirements && !found_proximity_requirements {
                            let neighbors = layers.get(TileMapLayerKind::Terrain).tile_neighbors(cell, false);
                            let is_near = neighbors
                                .iter()
                                .flatten()
                                .any(|neighbor| neighbor.path_kind().intersects(tile_def_to_place.required_proximity));
                            found_proximity_requirements = is_near;
                        }

                        // Check requirements again in the objects layer.
                        if has_proximity_requirements && !found_proximity_requirements {
                            let neighbors = layers.get(TileMapLayerKind::Objects).tile_neighbors(cell, false);
                            let is_near = neighbors
                                .iter()
                                .flatten()
                                .any(|neighbor| neighbor.path_kind().intersects(tile_def_to_place.required_proximity));
                            found_proximity_requirements = is_near;
                        }
                    }
                }

                if has_proximity_requirements && !found_proximity_requirements {
                    return err!(
                        Placement::RequiresProximity(tile_def_to_place.required_proximity),
                        "Building must be placed near {}.",
                        tile_def_to_place.required_proximity
                    );
                }

                // A gatehouse stands in a wall (crate::defence, tile::wall): a wall piece on both
                // sides of it along one axis, on a piece of the wall (which makes way for it as
                // it's placed), a gap of open land or a road through a gap (not a dyke).
                if tile_def_to_place.requires_wall {
                    let terrain = layers.get(TileMapLayerKind::Terrain);
                    let objects = layers.get(TileMapLayerKind::Objects);
                    let is_wall =
                        |cell: Cell| objects.try_tile(cell).is_some_and(|tile| wall::tier_of(tile.tile_def()).is_some());
                    let ground_takes_it = terrain.try_tile(target_cell).is_some_and(|tile| {
                        let path_kind = tile.path_kind();
                        path_kind.is_object_placeable() || (path_kind.is_road() && !tile.tile_def().blocks_floods)
                    });
                    let free_or_wall = objects.try_tile(target_cell).is_none() || is_wall(target_cell);
                    let (x, y) = (target_cell.x, target_cell.y);
                    let in_wall = (is_wall(Cell::new(x + 1, y)) && is_wall(Cell::new(x - 1, y)))
                        || (is_wall(Cell::new(x, y + 1)) && is_wall(Cell::new(x, y - 1)));
                    if !ground_takes_it || !free_or_wall || !in_wall {
                        return err!(
                            Placement::RequiresWall,
                            "'{}' must be placed in a wall: on a piece with a piece on both sides of it.",
                            tile_def_to_place.name
                        );
                    }
                }

                // A sluice gate stands beside a dyke (crate::farms): a terrain tile next to its
                // footprint, four neighbours, that blocks floods.
                if tile_def_to_place.requires_dyke {
                    let mut is_beside_dyke = false;
                    pathfind::for_each_surrounding_cell(tile_def_to_place.cell_range(target_cell), |cell| {
                        is_beside_dyke = layers
                            .get(TileMapLayerKind::Terrain)
                            .try_tile(cell)
                            .is_some_and(|tile| tile.tile_def().blocks_floods);
                        !is_beside_dyke // Stop at the first.
                    });
                    if !is_beside_dyke {
                        return err!(Placement::RequiresDyke, "'{}' must be placed beside a dyke.", tile_def_to_place.name);
                    }
                }
            }
        } else if tile_def_to_place.path_kind.is_vacant_lot() {
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell)
                && tile.path_kind().intersects(
                    PathNodeKind::Road | PathNodeKind::Water | PathNodeKind::Building | PathNodeKind::SettlersSpawnPoint,
                )
            {
                return err!(
                    Placement::Obstruction(tile.tile_def()),
                    "Cannot place vacant lot over terrain tile '{}'.",
                    tile.name()
                );
            }

            // Objects layer mut be empty.
            if let Some(object) = layers.get(TileMapLayerKind::Objects).try_tile(target_cell) {
                return err!(
                    Placement::Obstruction(object.tile_def()),
                    "Cannot place vacant lot here! Cell already occupied by an object."
                );
            }
        } else if tile_def_to_place.path_kind.intersects(PathNodeKind::Road | PathNodeKind::SettlersSpawnPoint) {
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell)
                && tile.path_kind().is_water()
            {
                return err!(
                    Placement::Obstruction(tile.tile_def()),
                    "Cannot place road over terrain tile '{}'.",
                    tile.name()
                );
            }
        } else if tile_def_to_place.path_kind.intersects(PathNodeKind::EmptyLand | PathNodeKind::Water) {
            // Land/water can only be placed over other land/water tiles.
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(target_cell)
                && !tile.path_kind().intersects(PathNodeKind::EmptyLand | PathNodeKind::Water)
            {
                return err!(
                    Placement::Obstruction(tile.tile_def()),
                    "Cannot place '{}' tile over terrain tile '{}'.",
                    tile_def_to_place.name,
                    tile.name()
                );
            }

            // Cannot place water under existing objects.
            if tile_def_to_place.path_kind.is_water()
                && let Some(object) = layers.get(TileMapLayerKind::Objects).try_tile(target_cell)
            {
                return err!(
                    Placement::Obstruction(object.tile_def()),
                    "Cannot place water tile here! Cell already occupied by an object."
                );
            }
        }

        Ok(())
    }

    pub fn try_place_tile_in_layer<'tile_map>(
        layer: &'tile_map mut TileMapLayer,
        target_cell: Cell,
        tile_def_to_place: &'static TileDef,
    ) -> Result<(&'tile_map mut Tile, usize), TilePlacementErr> {
        debug_assert!(tile_def_to_place.is_valid());
        debug_assert!(tile_def_to_place.layer_kind() == layer.kind());

        if !layer.is_cell_within_bounds(target_cell) {
            return err!(
                Placement::CellOutOfBounds,
                "'{}' - {}: Target cell {target_cell} is out of bounds",
                tile_def_to_place.name,
                layer.kind()
            );
        }

        let mut allow_stacking = false;

        if tile_def_to_place.is(TileKind::Terrain) {
            debug_assert!(layer.kind() == TileMapLayerKind::Terrain);

            // Terrain tiles are always allowed to replace existing tiles,
            // so first clear the cell in case there's already a tile there.
            if let Some(existing_tile) = layer.try_tile(target_cell) {
                // Avoid any work if we already have the same terrain tile.
                if existing_tile.tile_def().hash == tile_def_to_place.hash {
                    return err!(
                        Placement::Obstruction(existing_tile.tile_def()),
                        "Cell {target_cell} already contains '{}'",
                        tile_def_to_place.name
                    );
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
                return err!(
                    Placement::CellOutOfBounds,
                    "'{}' - {}: Target cell {cell} for this tile falls outside of the map bounds",
                    tile_def_to_place.name,
                    layer.kind()
                );
            }

            if allow_stacking {
                if let Some(existing_tile) = layer.try_tile(cell)
                    && !existing_tile.is(TileKind::Unit)
                {
                    return err!(
                        Placement::Obstruction(existing_tile.tile_def()),
                        "'{}' - {}: Target cell {cell} for this unit is already occupied by '{}'",
                        tile_def_to_place.name,
                        layer.kind(),
                        debug::tile_name_at(cell, layer.kind())
                    );
                }
            } else if let Some(existing_tile) = layer.try_tile(cell) {
                return err!(
                    Placement::Obstruction(existing_tile.tile_def()),
                    "'{}' - {}: Target cell {cell} for this tile is already occupied by '{}'",
                    tile_def_to_place.name,
                    layer.kind(),
                    debug::tile_name_at(cell, layer.kind())
                );
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

    pub fn try_clear_tile_from_layer(
        layer: &mut TileMapLayer,
        target_cell: Cell,
    ) -> Result<&'static TileDef, TileClearingErr> {
        if !layer.is_cell_within_bounds(target_cell) {
            return err!(Clearing::CellOutOfBounds, "{}: Target cell {} is out of bounds", layer.kind(), target_cell);
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

    pub fn try_clear_tile_from_layer_by_index(
        layer: &mut TileMapLayer,
        target_index: TilePoolIndex,
        target_cell: Cell,
    ) -> Result<&'static TileDef, TileClearingErr> {
        if !layer.is_cell_within_bounds(target_cell) {
            return err!(Clearing::CellOutOfBounds, "{}: Target cell {} is out of bounds", layer.kind(), target_cell);
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
