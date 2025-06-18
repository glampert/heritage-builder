use std::time::{self};
use rand::SeedableRng;
use rand_pcg::Pcg64;

use crate::{
    tile::{
        map::{Tile, TileMap, TileMapLayerKind},
        sets::{TileDef, TileKind, TileSets}
    },
    utils::{
        coords::{Cell, CellRange},
        hash::StringHash
    }
};

use super::{
    building::BuildingKind
};

pub mod resources;
pub mod world;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

const DEFAULT_RANDOM_SEED: u64 = 0xCAFE0CAFE0CAFE03;
pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

const DEFAULT_SIM_UPDATE_FREQUENCY_SECS: f32 = 0.5;

pub struct Simulation {
    update_frequency_secs: f32,
    time_since_last_update_secs: f32,
    rng: RandomGenerator,
}

impl Simulation {
    pub fn new() -> Self {
        Self {
            update_frequency_secs: DEFAULT_SIM_UPDATE_FREQUENCY_SECS,
            time_since_last_update_secs: 0.0,
            rng: RandomGenerator::seed_from_u64(DEFAULT_RANDOM_SEED),
        }
    }

    pub fn update<'tile_map, 'tile_sets>(&mut self,
                                         world: &mut world::World,
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
                         category_name_hash: StringHash,
                         tile_def_name_hash: StringHash) -> Option<&'tile_sets TileDef> {
        self.tile_sets.find_tile_def_by_hash(layer, category_name_hash, tile_def_name_hash)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer: TileMapLayerKind,
                     tile_kinds: TileKind) -> Option<&Tile<'tile_sets>> {

        self.tile_map.find_tile(cell, layer, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&mut self,
                         cell: Cell,
                         layer: TileMapLayerKind,
                         tile_kinds: TileKind) -> Option<&mut Tile<'tile_sets>> {

        self.tile_map.find_tile_mut(cell, layer, tile_kinds)
    }

    pub fn is_near_building(&self, start_cells: CellRange, kind: BuildingKind, radius_in_cells: i32) -> bool {
        debug_assert!(start_cells.is_valid());
        debug_assert!(radius_in_cells > 0);

        let search_range = {
            let start_x = start_cells.start.x - radius_in_cells;
            let start_y = start_cells.start.y - radius_in_cells;
            let end_x   = start_cells.end.x   + radius_in_cells;
            let end_y   = start_cells.end.y   + radius_in_cells;
            CellRange::new(Cell::new(start_x, start_y), Cell::new(end_x, end_y))  
        };

        for search_cell in &search_range {
            if let Some(search_tile) =
                self.tile_map.find_tile(search_cell, TileMapLayerKind::Objects, TileKind::Building) {
                let game_state = search_tile.game_state_handle();
                if game_state.is_valid() {
                    let building_kind = BuildingKind::from_game_state_handle(game_state);
                    if building_kind == kind {
                        return true;
                    }
                }
            }
        }

        false
    }
}
