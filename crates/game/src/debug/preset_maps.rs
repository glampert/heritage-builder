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
    // Same shape as `terrain_tiles` / `building_tiles`. Cells default to `X` (empty).
    // Empty slice means "no props on this preset".
    prop_tiles: &'static [i32],
    // Cells flagged with `TileFlags::SettlersSpawnPoint` after terrain placement.
    settler_spawn_points: &'static [Cell],
    enable_cheats_fn: Option<fn(&mut Cheats)>,
}

// TERRAIN:
const G: i32 = 0; // grass
const D: i32 = 1; // dirt
const R: i32 = 2; // dirt road
const V: i32 = 3; // vacant_lot
const TERRAIN_TILE_NAMES: [&str; 4] = [
    "grass",
    "dirt",
    road::tile_name(road::RoadKind::Dirt).string,
    "vacant_lot",
];

// BUILDINGS:
const X: i32 = -1; // empty (dummy value, also used by prop_tiles)
const H: i32 = 0;  // house0
const W: i32 = 1;  // small_well
const B: i32 = 2;  // large_well
const M: i32 = 3;  // market
const F: i32 = 4;  // rice_farm
const S: i32 = 5;  // granary
const Y: i32 = 6;  // storage_yard
const A: i32 = 7;  // distillery
const L: i32 = 8;  // lumberyard
const BUILDING_TILE_NAMES: [&str; 9] = [
    "house0",
    "small_well",
    "large_well",
    "market",
    "rice_farm",
    "granary",
    "storage_yard",
    "distillery",
    "lumberyard",
];

// PROPS (Objects layer, vegetation category):
const T: i32 = 0; // tree
const PROP_TILE_NAMES: [&str; 1] = [
    "tree",
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
    prop_tiles: &[],
    settler_spawn_points: &[],
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
    prop_tiles: &[],
    settler_spawn_points: &[],
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
    prop_tiles: &[],
    settler_spawn_points: &[],
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
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 lumberyard + 1 storage yard, ring road for connectivity (delivery test scenarios).
pub const PRESET_1_LUMBERYARD_1_STORAGE_YARD: usize = 4;
const PRESET_TILES_4: PresetTiles = PresetTiles {
    preset_name: "[4] - 1 lumberyard, 1 storage yard | 9x9",
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
        X,X,X,X,X,X,X,X,X,
        X,L,X,X,X,X,X,X,X, // lumberyard 2x2 at (1,2)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,Y,X,X,X,X, // storage_yard 3x3 at (4,5)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 rice farm + 1 distillery, ring road (no storage on map).
// Used to verify producer-fallback delivery (farm -> distillery accepts the rice harvest as raw input).
pub const PRESET_1_FARM_1_DISTILLERY: usize = 5;
const PRESET_TILES_5: PresetTiles = PresetTiles {
    preset_name: "[5] - 1 rice farm, 1 distillery | 9x9",
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
        X,F,X,X,X,X,X,X,X, // rice_farm 3x3 at (1,1)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,A,X,X, // distillery 2x2 at (6,5)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 market + 1 granary, ring road (fetch test scenarios).
pub const PRESET_1_MARKET_1_GRANARY: usize = 6;
const PRESET_TILES_6: PresetTiles = PresetTiles {
    preset_name: "[6] - 1 market, 1 granary | 9x9",
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
        X,M,X,X,X,X,X,X,X, // market 2x2 at (1,1)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,S,X,X,X,X, // granary 3x3 at (4,5)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 lumberyard + 1 storage yard + a few tree props (harvest test scenarios).
// Trees are placed on grass (off-road) to also exercise the off-road traversal flags.
pub const PRESET_1_LUMBERYARD_1_STORAGE_YARD_WITH_TREES: usize = 7;
const PRESET_TILES_7: PresetTiles = PresetTiles {
    preset_name: "[7] - 1 lumberyard, 1 storage yard, trees | 12x12",
    map_size_in_cells: Size::new(12, 12),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R,R,R,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,L,X,X,X,X,X,X,X,X,X,X, // lumberyard 2x2 at (1,1)
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,Y,X,X,X, // storage_yard 3x3 at (8,8)
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,T,X,X,X,X,X,X,
        X,X,X,X,X,X,T,X,X,X,X,X,
        X,X,X,X,T,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 2 lumberyards + 2 trees, roughly equidistant. Used to exercise the
// claim-race reroute branch (a second harvester arriving at a tree
// already claimed should pick the other one).
pub const PRESET_2_LUMBERYARDS_2_TREES: usize = 8;
const PRESET_TILES_8: PresetTiles = PresetTiles {
    preset_name: "[8] - 2 lumberyards, 2 trees | 12x12",
    map_size_in_cells: Size::new(12, 12),
    terrain_tiles: &[
        R,R,R,R,R,R,R,R,R,R,R,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,R,R,R,R,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,L,X,X,X,X,X,X,X,L,X,X, // lumberyards 2x2 at (1,1) and (9,1)
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,T,X,X,X,X,X,X, // tree at (5,5)
        X,X,X,X,X,X,T,X,X,X,X,X, // tree at (6,6)
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// EmptyLand (grass) terrain + 1 vacant lot + 1 spawn point + 1 house0.
// Settler should prefer the vacant lot over the existing house.
pub const PRESET_SETTLER_VACANT_LOT_AND_HOUSE: usize = 9;
const PRESET_TILES_9: PresetTiles = PresetTiles {
    preset_name: "[9] - settler: vacant lot + house0 | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        G,G,G,G,G,G,G,G,G, // spawn point flagged at (4,0) below
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,V,G,G,G,G, // vacant_lot at (4,3)
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,H,X,X,X,X, // house0 at (4,7)
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[Cell::new(4, 0)],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// EmptyLand (grass) terrain + 1 spawn point + 1 house0 (no vacant lot).
// Settler should fall back to the existing house.
pub const PRESET_SETTLER_HOUSE_ONLY: usize = 10;
const PRESET_TILES_10: PresetTiles = PresetTiles {
    preset_name: "[10] - settler: house0 only | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        G,G,G,G,G,G,G,G,G, // spawn point flagged at (4,0) below
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,H,X,X,X,X, // house0 at (4,4)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[Cell::new(4, 0)],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// EmptyLand (grass) terrain + 1 spawn point only (no vacant lots, no houses).
// Settler should fail to settle and walk back to the spawn point to despawn.
pub const PRESET_SETTLER_SPAWN_POINT_ONLY: usize = 11;
const PRESET_TILES_11: PresetTiles = PresetTiles {
    preset_name: "[11] - settler: spawn point only | 9x9",
    map_size_in_cells: Size::new(9, 9),
    terrain_tiles: &[
        G,G,G,G,G,G,G,G,G, // spawn point flagged at (4,0) below
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,G,G,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[Cell::new(4, 0)],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// Patrol crossroads: N-S road at col 7, E-W road at row 7. Market 2x2 at (8,8)
// sits adjacent to the intersection. Roads are long enough for max_distance tests.
pub const PRESET_PATROL_CROSSROADS_MARKET: usize = 12;
const PRESET_TILES_12: PresetTiles = PresetTiles {
    preset_name: "[12] - patrol: crossroads + market | 15x15",
    map_size_in_cells: Size::new(15, 15),
    terrain_tiles: &[
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        R,R,R,R,R,R,R,R,R,R,R,R,R,R,R,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,M,X,X,X,X,X,X, // market 2x2 at (8,8)
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// Same crossroads as above + house0s along the road for visit-target tests.
pub const PRESET_PATROL_CROSSROADS_MARKET_WITH_HOUSES: usize = 13;
const PRESET_TILES_13: PresetTiles = PresetTiles {
    preset_name: "[13] - patrol: crossroads + market + houses | 15x15",
    map_size_in_cells: Size::new(15, 15),
    terrain_tiles: &[
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        R,R,R,R,R,R,R,R,R,R,R,R,R,R,R,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
        G,G,G,G,G,G,G,R,G,G,G,G,G,G,G,
    ],
    building_tiles: &[
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X, // houses bracketing N arm of road
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X,
        X,H,X,H,X,H,X,X,X,H,X,H,X,H,X, // houses bracketing W & E arms of road row 6
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,M,X,X,X,X,X,X, // market 2x2 at (8,8)
        X,H,X,H,X,H,X,X,X,X,X,H,X,H,X, // houses bracketing W & E arms of road row 9
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,H,X,H,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

// 1 market + 1 granary (source) + 1 storage yard (recovery sink).
// Used by the fetch-recovery tests: the market dispatches a runner to fetch
// from the granary; when the runner can't deliver back to the market, the
// storage yard accepts the surplus. All three buildings are road-linked via
// the ring road so the runner can reach any of them.
pub const PRESET_1_MARKET_1_GRANARY_1_STORAGE_YARD: usize = 14;
const PRESET_TILES_14: PresetTiles = PresetTiles {
    preset_name: "[14] - 1 market, 1 granary (src), 1 storage yard (sink) | 9x9",
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
        X,M,X,X,X,S,X,X,X, // market 2x2 at (1,1); granary 3x3 at (5,1)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,Y,X,X,X, // storage_yard 3x3 at (5,5)
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
        X,X,X,X,X,X,X,X,X,
    ],
    prop_tiles: &[],
    settler_spawn_points: &[],
    enable_cheats_fn: Some(|cheats| {
        cheats.ignore_worker_requirements = true
    })
};

const PRESET_TILES: [&PresetTiles; 15] = [
    &PRESET_TILES_0,
    &PRESET_TILES_1,
    &PRESET_TILES_2,
    &PRESET_TILES_3,
    &PRESET_TILES_4,
    &PRESET_TILES_5,
    &PRESET_TILES_6,
    &PRESET_TILES_7,
    &PRESET_TILES_8,
    &PRESET_TILES_9,
    &PRESET_TILES_10,
    &PRESET_TILES_11,
    &PRESET_TILES_12,
    &PRESET_TILES_13,
    &PRESET_TILES_14,
];

const PRESET_TILE_MAP_COUNT: usize = PRESET_TILES.len();

fn find_terrain_tile(tile_id: i32) -> Option<&'static TileDef> {
    if tile_id < 0 {
        return None;
    }
    let tile_name = TERRAIN_TILE_NAMES[tile_id as usize];
    TileSets::get().find_tile_def_by_name(
        TileMapLayerKind::Terrain,
        tile::sets::TERRAIN_LAND_CATEGORY.string,
        tile_name,
    )
}

fn find_building_tile(tile_id: i32) -> Option<&'static TileDef> {
    if tile_id < 0 {
        return None;
    }
    let tile_name = BUILDING_TILE_NAMES[tile_id as usize];
    TileSets::get().find_tile_def_by_name(
        TileMapLayerKind::Objects,
        tile::sets::OBJECTS_BUILDINGS_CATEGORY.string,
        tile_name,
    )
}

fn find_prop_tile(tile_id: i32) -> Option<&'static TileDef> {
    if tile_id < 0 {
        return None;
    }
    let tile_name = PROP_TILE_NAMES[tile_id as usize];
    TileSets::get().find_tile_def_by_name(
        TileMapLayerKind::Objects,
        tile::sets::OBJECTS_VEGETATION_CATEGORY.string,
        tile_name,
    )
}

fn build_tile_map(preset: &'static PresetTiles, world: &mut World) -> TileMap {
    let map_size_in_cells = preset.map_size_in_cells;

    let tile_count = (map_size_in_cells.width * map_size_in_cells.height) as usize;
    debug_assert!(preset.terrain_tiles.len() == tile_count);
    debug_assert!(preset.building_tiles.len() == tile_count);
    debug_assert!(preset.prop_tiles.is_empty() || preset.prop_tiles.len() == tile_count);

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
            if let Some(tile_def) = find_terrain_tile(tile_id) {
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
            if let Some(tile_def) = find_building_tile(tile_id) {
                if let Err(err) = spawner.try_spawn_building_with_tile_def(Cell::new(x, y), tile_def) {
                    log::error!(log::channel!("debug"), "Preset: Failed to place Building tile: {} - {}", err.reason, err.message);
                }
            }
        }
    }

    // Props (Objects, vegetation category):
    if !preset.prop_tiles.is_empty() {
        for y in 0..map_size_in_cells.height {
            for x in 0..map_size_in_cells.width {
                let tile_id = preset.prop_tiles[(x + (y * map_size_in_cells.width)) as usize];
                if let Some(tile_def) = find_prop_tile(tile_id) {
                    match spawner.try_spawn_tile_with_def(Cell::new(x, y), tile_def) {
                        SpawnerResult::Prop(_) => {}
                        SpawnerResult::Err(err) => {
                            log::error!(log::channel!("debug"), "Preset: Failed to place Prop tile: {} - {}", err.reason, err.message);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    // Settler spawn points: flag the underlying terrain tiles. The graph picks
    // up `PathNodeKind::SettlersSpawnPoint` from the flag during the recalc.
    for &spawn_point in preset.settler_spawn_points {
        tile_map.set_tile_flags(
            spawn_point,
            tile::TileKind::Terrain,
            TileFlags::SettlersSpawnPoint,
            true,
        );
    }

    super::utils::refresh_cached_tile_visuals(&mut tile_map);
    tile_map
}
