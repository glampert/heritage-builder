use std::time::{self};

use crate::{
    utils::{Cell},
    tile::{
        sets::{TileKind, TileSets, TileDef},
        map::{Tile, TileMap, TileMapLayerKind}
    }
};

use super::{
    building::{BuildingKind}
};

pub mod world;
use world::World;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

pub struct Simulation {
    update_frequency_secs: f32,
    time_since_last_update_secs: f32,
}

impl Simulation {
    pub fn new() -> Self {
        Self {
            update_frequency_secs: 0.5,
            time_since_last_update_secs: 0.0,
        }
    }

    pub fn update<'tile_map, 'tile_sets>(&mut self,
                                         world: &mut World,
                                         tile_map: &'tile_map mut TileMap<'tile_sets>,
                                         tile_sets: &'tile_sets TileSets,
                                         delta_time: time::Duration) {

        if self.time_since_last_update_secs >= self.update_frequency_secs {

            // Fixed step update.
            let delta_time_secs = self.time_since_last_update_secs;

            world.update(tile_map, tile_sets, delta_time_secs);

            // Reset the clock.
            self.time_since_last_update_secs = 0.0;
        } else {
            // Advance the clock.
            self.time_since_last_update_secs += delta_time.as_secs_f32();
        }
    }
}

// ----------------------------------------------
// Query
// ----------------------------------------------

pub struct Query<'tile_map, 'tile_sets> {
    pub tile_sets: &'tile_sets TileSets,
    pub tile_map: &'tile_map mut TileMap<'tile_sets>,
}

impl<'tile_map, 'tile_sets> Query<'tile_map, 'tile_sets> {
    pub fn new(tile_map: &'tile_map mut TileMap<'tile_sets>, tile_sets: &'tile_sets TileSets) -> Self {
        Self {
            tile_sets: tile_sets,
            tile_map: tile_map,
        }
    }

    #[inline]
    pub fn find_tile_def(&self,
                         layer: TileMapLayerKind,
                         category_name: &str,
                         tile_name: &str) -> Option<&'tile_sets TileDef> {
        self.tile_sets.find_tile_by_name(layer, category_name, tile_name)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer: TileMapLayerKind,
                     tile_kinds: TileKind) -> Option<&Tile> {

        self.tile_map.find_tile(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&mut self,
                         cell: Cell,
                         layer: TileMapLayerKind,
                         tile_kinds: TileKind) -> Option<&mut Tile<'tile_sets>> {

        self.tile_map.find_tile_mut(cell, layer, tile_kinds)
    }

    pub fn is_near_building(&self, start_cell: Cell, kind: BuildingKind, radius_in_cells: i32) -> bool {
        for dx in -radius_in_cells..=radius_in_cells {
            for dy in -radius_in_cells..=radius_in_cells {
                if dx == 0 && dy == 0 {
                    continue; // Skip start_cell.
                }

                let search_cell = Cell::new(start_cell.x + dx, start_cell.y + dy);

                if let Some(tile) = self.tile_map.try_tile_from_layer(search_cell, TileMapLayerKind::Buildings) {
                    // FIXME: Need to handle multi-tile buildings here.
                    if tile.is_building() && tile.game_state.is_valid() {
                        let tile_building_kind =
                            BuildingKind::try_from(tile.game_state.kind())
                                .expect("GameStateHandle does not contain a valid BuildingKind enum value!");

                        if tile_building_kind == kind {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }
}
