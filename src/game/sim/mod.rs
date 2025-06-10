use std::time::{self};
use rand::SeedableRng;
use rand_pcg::Pcg64;

use crate::{
    utils::{Cell, Size},
    tile::{
        sets::{TileDef, TileKind, TileSets},
        map::{GameStateHandle, Tile, TileMap, TileMapLayerKind}
    }
};

use super::{
    building::{BuildingKind}
};

pub mod world;
use world::World;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

const DEFAULT_RANDOM_SEED: u64 = 0xCAFE0CAFE0CAFE03;
pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

pub struct Simulation {
    update_frequency_secs: f32,
    time_since_last_update_secs: f32,
    rng: RandomGenerator,
}

impl Simulation {
    pub fn new() -> Self {
        Self {
            update_frequency_secs: 0.5,
            time_since_last_update_secs: 0.0,
            rng: RandomGenerator::seed_from_u64(DEFAULT_RANDOM_SEED),
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

            let mut query = Query::new(&mut self.rng, tile_map, tile_sets);
            world.update(&mut query, delta_time_secs);

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

pub struct Query<'sim, 'tile_map, 'tile_sets> {
    pub rng: &'sim mut RandomGenerator,
    pub tile_sets: &'tile_sets TileSets,
    pub tile_map: &'tile_map mut TileMap<'tile_sets>,
}

impl<'sim, 'tile_map, 'tile_sets> Query<'sim, 'tile_map, 'tile_sets> {
    fn new(rng: &'sim mut RandomGenerator,
           tile_map: &'tile_map mut TileMap<'tile_sets>,
           tile_sets: &'tile_sets TileSets) -> Self {
        Self {
            rng: rng,
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
        // Buildings can occupy multiple cells; Find out how many to offset the start by.
        let mut end_offset = Size::zero();
        if let Some(start_tile) = self.tile_map.try_tile_from_layer(start_cell, TileMapLayerKind::Buildings) {
            let size = start_tile.size_in_cells();
            end_offset = Size::new(size.width - 1, size.height - 1);
        }

        for dx in -radius_in_cells..=(radius_in_cells + end_offset.width) {
            for dy in -radius_in_cells..=(radius_in_cells + end_offset.height) {
                if dx == 0 && dy == 0 {
                    continue; // Skip start_cell.
                }

                let search_cell = Cell::new(start_cell.x + dx, start_cell.y + dy);
                if let Some(search_tile) = self.tile_map.try_tile_from_layer(search_cell, TileMapLayerKind::Buildings) {
                    let mut game_state = GameStateHandle::invalid();

                    if search_tile.is_blocker() {
                        let owner_tile =
                            self.tile_map.try_tile_from_layer(search_tile.blocker_owner_cell(), TileMapLayerKind::Buildings).unwrap();
                        debug_assert!(owner_tile.is_building());
                        if owner_tile.cell != start_cell {
                            game_state = owner_tile.game_state;
                        }
                    } else if search_tile.is_building() {
                        game_state = search_tile.game_state;
                    }

                    if game_state.is_valid() {
                        let tile_building_kind =
                            BuildingKind::try_from(game_state.kind())
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
