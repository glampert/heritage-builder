use arrayvec::ArrayVec;
use common::{Size, coords::Cell};
use engine::log;

use crate::{
    sim::Simulation,
    config::GameConfigs,
    cheats::{self, Cheats},
    world::{World, object::{Spawner, SpawnerResult}},
    tile::{
        self,
        TileFlags,
        TileMap,
        TileMapLayerKind,
        sets::{TileDef, TileSets},
        road,
    },
};

// ----------------------------------------------
// Preset TileMaps Public API
// ----------------------------------------------

pub fn preset_tile_maps_list() -> ArrayVec<&'static str, PRESET_TILE_MAP_COUNT> {
    PRESET_TILES.iter().map(|preset| preset.preset_name).collect()
}

pub fn create_preset_tile_map(world: &mut World, mut preset_number: usize) -> TileMap {
    preset_number = preset_number.min(PRESET_TILE_MAP_COUNT - 1);

    log::info!(log::channel!("debug"),
                "Creating debug tile map - PRESET: {} ...",
                preset_number);

    let preset = PRESET_TILES[preset_number];

    if let Some(enable_cheats_fn) = preset.enable_cheats_fn {
        enable_cheats_fn(cheats::get_mut());
    }

    build_tile_map(preset, world)
}

// ----------------------------------------------
// Built-in preset test TileMaps
// ----------------------------------------------

struct PresetTiles {
    preset_name: &'static str,
    map_size_in_cells: Size,
    terrain_tiles: &'static [i32],
    building_tiles: &'static [i32],
    enable_cheats_fn: Option<fn(&mut Cheats)>,
}

// TERRAIN:
const G: i32 = 0; // grass
const D: i32 = 1; // dirt
const R: i32 = 2; // dirt road
const TERRAIN_TILE_NAMES: [&str; 3] = [
    "grass",
    "dirt",
    road::tile_name(road::RoadKind::Dirt).string,
];

// BUILDINGS:
const X: i32 = -1; // empty (dummy value)
const H: i32 = 0;  // house0
const W: i32 = 1;  // small_well
const B: i32 = 2;  // large_well
const M: i32 = 3;  // market
const F: i32 = 4;  // rice_farm
const S: i32 = 5;  // granary
const Y: i32 = 6;  // storage_yard
const A: i32 = 7;  // distillery
const BUILDING_TILE_NAMES: [&str; 8] = [
    "house0",
    "small_well",
    "large_well",
    "market",
    "rice_farm",
    "granary",
    "storage_yard",
    "distillery",
];

// Empty 9x9 map. Ring road around the whole map.
pub const PRESET_EMPTY_MAP_WITH_RING_ROAD: usize = 0;
const PRESET_TILES_0: PresetTiles = PresetTiles {
    preset_name: "[0] - empty | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X, // <-- start, tile zero is the leftmost (top-left)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 farm, 1 storage (granary)
pub const PRESET_1_FARM_1_GRANARY: usize = 1;
const PRESET_TILES_1: PresetTiles = PresetTiles {
    preset_name: "[1] - 1 farm, 1 granary | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,S,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,F,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 farm, 1 storage (granary), 1 house, 2 wells (big & small), 1 market
pub const PRESET_1_FARM_1_GRANARY_1_HOUSE_2_WELLS_1_MARKET: usize = 2;
const PRESET_TILES_2: PresetTiles = PresetTiles {
    preset_name: "[2] - 1 farm, 1 granary, 1 house, 2 wells, 1 market | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,R,
        R,R,G,G,R,R,R,R,R,
        R,G,G,G,R,G,G,G,R,
        R,G,G,G,R,G,G,G,R,
        R,G,G,G,R,G,G,G,R,
        R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,
        X,H,X,X,B,X,M,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,W,X,X,X,X,X,X,
        X,F,X,X,X,S,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 farm, 2 storages (granary, storage yard), 1 factory (distillery)
pub const PRESET_1_FARM_1_GRANARY_1_STORAGE_YARD_1_DISTILLERY: usize = 3;
const PRESET_TILES_3: PresetTiles = PresetTiles {
    preset_name: "[3] - 1 farm, 2 storages (G|Y), 1 distillery | 12x12",
    map_size_in_cells: Size::new(12, 12),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R,R,R,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,R,R,R,R,R,R,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,G,G,G,G,G,G,R,G,G,G,R,
        R,R,R,R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,A,X,X,X,X,X,X,S,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,F,X,X,X,X,X,X,Y,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

const PRESET_TILES: [&PresetTiles; 4] = [
    &PRESET_TILES_0,
    &PRESET_TILES_1,
    &PRESET_TILES_2,
    &PRESET_TILES_3,
];

const PRESET_TILE_MAP_COUNT: usize = PRESET_TILES.len();

fn find_tile(layer_kind: TileMapLayerKind, tile_id: i32) -> Option<&'static TileDef> {
    if tile_id < 0 {
        return None;
    }

    let category_name = match layer_kind {
        TileMapLayerKind::Terrain => tile::sets::TERRAIN_LAND_CATEGORY.string,
        TileMapLayerKind::Objects => tile::sets::OBJECTS_BUILDINGS_CATEGORY.string,
    };

    let tile_name = match layer_kind {
        TileMapLayerKind::Terrain => TERRAIN_TILE_NAMES[tile_id as usize],
        TileMapLayerKind::Objects => BUILDING_TILE_NAMES[tile_id as usize],
    };

    TileSets::get().find_tile_def_by_name(layer_kind, category_name, tile_name)
}

fn build_tile_map(preset: &'static PresetTiles, world: &mut World) -> TileMap {
    let map_size_in_cells = preset.map_size_in_cells;

    let tile_count = (map_size_in_cells.width * map_size_in_cells.height) as usize;
    debug_assert!(preset.terrain_tiles.len() == tile_count);
    debug_assert!(preset.building_tiles.len() == tile_count);

    let configs = GameConfigs::get();
    let mut tile_map = TileMap::new(map_size_in_cells, None);

    // Create a temp Simulation instance so we can create a SimContext to spawn the terrain and building tiles.
    let mut sim = Simulation::new(map_size_in_cells, configs);
    let context = sim.new_sim_context(0.0, &mut tile_map, world);

    let mut spawner = Spawner::new(&context);
    spawner.set_subtract_tile_cost(false);

    // Terrain:
    for y in 0..map_size_in_cells.height {
        for x in 0..map_size_in_cells.width {
            let tile_id = preset.terrain_tiles[(x + (y * map_size_in_cells.width)) as usize];
            if let Some(tile_def) = find_tile(TileMapLayerKind::Terrain, tile_id) {
                match spawner.try_spawn_tile_with_def(Cell::new(x, y), tile_def) {
                    SpawnerResult::Tile(tile) => {
                        // Set a random terrain tile variation:
                        if tile.has_flags(TileFlags::RandomizePlacement) {
                            tile.set_random_variation_index(context.rng_mut());
                        }
                    },
                    SpawnerResult::Err(err) => {
                        log::error!(log::channel!("debug"), "Preset: Failed to place Terrain tile: {} - {}", err.reason, err.message);
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    // Buildings (Objects):
    for y in 0..map_size_in_cells.height {
        for x in 0..map_size_in_cells.width {
            let tile_id = preset.building_tiles[(x + (y * map_size_in_cells.width)) as usize];
            if let Some(tile_def) = find_tile(TileMapLayerKind::Objects, tile_id) {
                if let Err(err) = spawner.try_spawn_building_with_tile_def(Cell::new(x, y), tile_def) {
                    log::error!(log::channel!("debug"), "Preset: Failed to place Building tile: {} - {}", err.reason, err.message);
                }
            }
        }
    }

    super::utils::refresh_cached_tile_visuals(&mut tile_map);
    tile_map
}
