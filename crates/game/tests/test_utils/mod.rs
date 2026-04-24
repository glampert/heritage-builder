// NOTE: Allow for the whole crate.
#![allow(dead_code)]

use common::{coords::Cell, time::Seconds, Size};
use engine::{log, render::texture::TextureCache};
use game::{
    cheats,
    config::GameConfigs,
    world::World,
    prop::{config::PropConfigs, PropId},
    sim::{
        commands::{self, SimCmdQueue, SpawnQueryResult, SpawnReadyResult},
        SimCmds, SimContext, Simulation,
    },
    tile::{
        sets::{TileDef, TileSets, OBJECTS_BUILDINGS_CATEGORY, OBJECTS_VEGETATION_CATEGORY, TERRAIN_LAND_CATEGORY},
        TileMap, TileMapLayerKind,
    },
    building::{
        config::BuildingConfigs,
        BuildingKindAndId,
    },
    unit::{
        config::{UnitConfigKey, UnitConfigs},
        task::{UnitTask, UnitTaskArchetype, UnitTaskId},
        Unit, UnitId,
    },
};

// ----------------------------------------------
// Custom Test Runner / Harness
// ----------------------------------------------

// One-time environment setup. Must be called on the main thread
// before any tests, since the game uses SingleThreadStatic globals
// that assert access from the thread that initialized them.
fn setup(test_suite_name: &str) {
    log::info!("----------------------------------------");
    log::info!("Starting Test Suite: {}", test_suite_name);
    log::info!("----------------------------------------");

    log::set_level(log::Level::Warning);

    GameConfigs::load();
    UnitConfigs::load();
    PropConfigs::load();
    BuildingConfigs::load();

    cheats::initialize();
    cheats::get_mut().ignore_tile_cost = true; // So we can spawn anything...
    cheats::get_mut().ignore_worker_requirements = true; // So producer/storage buildings accept deliveries without staffing.

    let mut tex_cache = TextureCache::default();
    let skip_loading_textures = true;
    TileSets::load(&mut tex_cache, false, false, skip_loading_textures);
}

fn print_passed() {
    let colors = ("\x1b[32m", "\x1b[0m"); // green
    println!("{}ok{}", colors.0, colors.1);
}

fn print_failed() {
    let colors = ("\x1b[31m", "\x1b[0m"); // red
    println!("{}FAILED{}", colors.0, colors.1);
}

// Runs all tests sequentially on the calling thread.
// Calls setup() once before running any tests.
pub fn run_tests(test_suite_name: &str, tests: &[(&str, fn())]) {
    setup(test_suite_name);

    let mut passed = 0;
    let mut failed = 0;

    for (name, test_fn) in tests {
        print!("test {name} ... ");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test_fn));
        match result {
            Ok(()) => { print_passed(); passed += 1; }
            Err(_) => { print_failed(); failed += 1; }
        }
    }

    println!("\ntest result: {} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

macro_rules! test_fn {
    ($f:ident) => {
        (stringify!($f), $f)
    };
}

pub(crate) use test_fn;

// ----------------------------------------------
// TestEnvironment
// ----------------------------------------------

// Bundles the TileMap / World / Simulation triple that most tests need.
// Lives on the main thread (SingleThreadStatic globals constrain this).
pub struct TestEnvironment {
    pub tile_map: TileMap,
    pub world: World,
    pub sim: Simulation,
}

impl TestEnvironment {
    pub const DEFAULT_MAP_SIZE: Size = Size::new(32, 32);

    // Default to a "small enough to iterate quickly, big enough to plan paths" tick.
    // Individual tests can pass a different delta to tick() if they need to advance
    // a timer quickly (e.g. harvest timers that are in tens of seconds).
    pub const TICK_DELTA_SECS: Seconds = 0.1;

    pub fn new() -> Self {
        Self::with_map_size(Self::DEFAULT_MAP_SIZE)
    }

    pub fn with_map_size(size: Size) -> Self {
        Self {
            tile_map: TileMap::new(size, None),
            world: World::new(),
            sim: Simulation::new(size, GameConfigs::get()),
        }
    }

    pub fn new_sim_context(&mut self, delta_time_secs: Seconds) -> SimContext {
        self.sim.new_sim_context(delta_time_secs, &mut self.tile_map, &mut self.world)
    }
}

// ----------------------------------------------
// Tile-def lookup helpers
// ----------------------------------------------

pub fn find_terrain_def(name: &str) -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_name(TileMapLayerKind::Terrain, TERRAIN_LAND_CATEGORY.string, name)
        .unwrap_or_else(|| panic!("Missing terrain tile def '{name}'"))
}

pub fn find_building_def(name: &str) -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_name(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.string, name)
        .unwrap_or_else(|| panic!("Missing building tile def '{name}'"))
}

pub fn find_vegetation_def(name: &str) -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_name(TileMapLayerKind::Objects, OBJECTS_VEGETATION_CATEGORY.string, name)
        .unwrap_or_else(|| panic!("Missing vegetation tile def '{name}'"))
}

// ----------------------------------------------
// Scenario builders (SimCmds-backed)
// ----------------------------------------------

// All scenario builders go through the real `SimCmds` spawn pipeline
// (the same path the game uses) so that graph/minimap updates, tile
// handles, and generational ids are all kept consistent.

pub fn place_terrain_by_def(env: &mut TestEnvironment, cell: Cell, tile_def: &'static TileDef) {
    let mut cmds = SimCmds::default();
    let promise = cmds.spawn_tile_with_tile_def_promise(cell, tile_def, commands::no_tile_callback());
    execute_cmds(env, &mut cmds);

    match cmds.query_promise(promise) {
        SpawnQueryResult::Ready(SpawnReadyResult::Tile(_, _)) => {}
        other => panic!("place_terrain_by_def({cell}, '{}') failed: {other}", tile_def.name),
    }
}

pub fn place_terrain(env: &mut TestEnvironment, cell: Cell, name: &str) {
    place_terrain_by_def(env, cell, find_terrain_def(name));
}

// Fill every cell of the map with the named terrain. Useful for tests
// that need pathable land everywhere (harvest off-road, patrol, etc.).
pub fn fill_terrain(env: &mut TestEnvironment, name: &str) {
    let tile_def = find_terrain_def(name);
    let size = env.tile_map.size_in_cells();
    for y in 0..size.height {
        for x in 0..size.width {
            place_terrain_by_def(env, Cell::new(x, y), tile_def);
        }
    }
}

// Place a straight run of road cells (for the common "connect A to B" scenario).
pub fn place_road(env: &mut TestEnvironment, cells: &[Cell]) {
    let road_def = find_terrain_def("dirt_road");
    for cell in cells {
        place_terrain_by_def(env, *cell, road_def);
    }
}

pub fn spawn_unit(env: &mut TestEnvironment, origin: Cell, config: UnitConfigKey) -> UnitId {
    let mut cmds = SimCmds::default();
    let promise = cmds.spawn_unit_with_config_promise(origin, config, commands::no_object_callback());
    execute_cmds(env, &mut cmds);

    match cmds.query_promise(promise) {
        SpawnQueryResult::Ready(SpawnReadyResult::GameObject(id)) => id,
        other => panic!("spawn_unit({origin}, {config:?}) failed: {other}"),
    }
}

pub fn spawn_building(env: &mut TestEnvironment, base_cell: Cell, name: &str) -> BuildingKindAndId {
    let tile_def = find_building_def(name);
    let mut cmds = SimCmds::default();
    let promise = cmds.spawn_building_with_tile_def_promise(base_cell, tile_def, commands::no_object_callback());
    execute_cmds(env, &mut cmds);

    match cmds.query_promise(promise) {
        SpawnQueryResult::Ready(SpawnReadyResult::GameObject(_id)) => {
            // SpawnReadyResult carries a bare GenerationalIndex; look the building
            // up by base cell to recover the `BuildingKindAndId` pair.
            let building = env.world
                .find_building_for_cell(base_cell, &env.tile_map)
                .unwrap_or_else(|| panic!("spawn_building: no building found at {base_cell} after spawn!"));
            building.kind_and_id()
        }
        other => panic!("spawn_building({base_cell}, '{name}') failed: {other}"),
    }
}

pub fn spawn_tree(env: &mut TestEnvironment, cell: Cell) -> PropId {
    let tile_def = find_vegetation_def("tree");
    let mut cmds = SimCmds::default();
    let promise = cmds.spawn_prop_with_tile_def_promise(cell, tile_def, commands::no_object_callback());
    execute_cmds(env, &mut cmds);

    match cmds.query_promise(promise) {
        SpawnQueryResult::Ready(SpawnReadyResult::GameObject(id)) => id,
        other => panic!("spawn_tree({cell}) failed: {other}"),
    }
}

// ----------------------------------------------
// Task assignment & tick driver
// ----------------------------------------------

// Allocate a task in the simulation task pool and assign it to a unit.
// Returns the task id (valid until the unit finishes the task or is despawned).
pub fn assign_task<T>(env: &mut TestEnvironment, unit_id: UnitId, task: T) -> UnitTaskId
where
    T: UnitTask,
    UnitTaskArchetype: From<T>,
{
    let task_manager = env.sim.task_manager_mut();

    let task_id = task_manager.new_task(task)
        .expect("UnitTaskManager::new_task returned None (pool full?)");

    let unit = env.world.find_unit_mut(unit_id)
        .unwrap_or_else(|| panic!("assign_task: unit {unit_id} not found"));

    unit.assign_task(task_manager, Some(task_id));

    task_id
}

// Advance one fixed-timestep tick: drive navigation + run tasks for every
// spawned unit/prop/building, then flush any deferred SimCmds the tasks
// produced. This intentionally bypasses Simulation::update / GameLoop,
// both of which require an Engine and GameSystems we don't have in tests.
pub fn tick(env: &mut TestEnvironment, delta_time_secs: Seconds) {
    let mut cmds = SimCmds::default();
    let context = env.new_sim_context(delta_time_secs);

    context.world_mut().update_unit_navigation(&context);
    context.world_mut().update(&mut cmds, &context);

    cmds.execute(&context);
}

pub fn tick_n(env: &mut TestEnvironment, count: usize, delta_time_secs: Seconds) {
    for _ in 0..count {
        tick(env, delta_time_secs);
    }
}

// Tick until `predicate(env)` returns true or `max_ticks` is exceeded.
// Returns the number of ticks consumed. Panics on timeout so tests
// don't silently hang.
pub fn tick_until<F>(env: &mut TestEnvironment, max_ticks: usize, delta_time_secs: Seconds, mut predicate: F) -> usize
where
    F: FnMut(&TestEnvironment) -> bool,
{
    for i in 0..max_ticks {
        if predicate(env) {
            return i;
        }
        tick(env, delta_time_secs);
    }

    if predicate(env) {
        return max_ticks;
    }

    panic!("tick_until exceeded max_ticks={max_ticks}");
}

// ----------------------------------------------
// Assertion helpers
// ----------------------------------------------

pub fn find_unit<'world>(env: &'world TestEnvironment, id: UnitId) -> &'world Unit {
    env.world.find_unit(id)
        .unwrap_or_else(|| panic!("Unit {id} is not spawned!"))
}

pub fn unit_exists(env: &TestEnvironment, id: UnitId) -> bool {
    env.world.find_unit(id).is_some()
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn execute_cmds(env: &mut TestEnvironment, cmds: &mut SimCmds) {
    let context = env.new_sim_context(0.0);
    cmds.execute(&context);
}
