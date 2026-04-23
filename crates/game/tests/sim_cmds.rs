use common::{mem::SingleThreadStatic, coords::Cell, Size};
use game::{
    world::World,
    config::GameConfigs,
    unit::config::UnitConfigKey,
    sim::{Simulation, SimContext, SimCmds, commands::{SpawnQueryResult, SpawnReadyResult}},
    tile::{
        TileMap, TileMapLayerKind, placement::TilePlacementErrReason,
        sets::{TileSets, TERRAIN_LAND_CATEGORY, OBJECTS_BUILDINGS_CATEGORY, OBJECTS_VEGETATION_CATEGORY},
    },
};

mod test_utils;

// ----------------------------------------------
// Integration tests for SimCmds
// ----------------------------------------------

fn main() {
    test_utils::run_tests(&[
        test_utils::test_fn!(test_sim_cmd_queue_spawning),
        test_utils::test_fn!(test_sim_cmd_queue_spawn_failure),
        test_utils::test_fn!(test_sim_cmd_queue_spawning_with_callbacks),
    ]);
}

struct TestEnvironment {
    tile_map: TileMap,
    world: World,
    sim: Simulation,
}

impl TestEnvironment {
    const MAP_SIZE_IN_CELLS: Size = Size::new(32, 32);

    fn new() -> Self {
        Self {
            tile_map: TileMap::new(Self::MAP_SIZE_IN_CELLS, None),
            world: World::new(),
            sim: Simulation::new(Self::MAP_SIZE_IN_CELLS, GameConfigs::get()),
        }
    }

    fn new_sim_context(&mut self) -> SimContext {
        self.sim.new_sim_context(0.0, &mut self.tile_map, &mut self.world)
    }
}

fn test_sim_cmd_queue_spawning() {
    let mut test_env = TestEnvironment::new();
    let context = test_env.new_sim_context();
    let mut cmds = SimCmds::default();

    // Push spawn commands:
    let mut tile_promise = cmds.spawn_tile_with_tile_def_promise(
        Cell::new(0, 0),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Terrain,
            TERRAIN_LAND_CATEGORY.string,
            "grass",
        ).unwrap(),
        SimCmds::no_tile_callback(),
    );

    let mut building_promise = cmds.spawn_building_with_tile_def_promise(
        Cell::new(1, 1),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_BUILDINGS_CATEGORY.string,
            "small_well",
        ).unwrap(),
        SimCmds::no_object_callback(),
    );

    let mut unit_promise = cmds.spawn_unit_with_config_promise(
        Cell::new(2, 2),
        UnitConfigKey::Peasant,
        SimCmds::no_object_callback(),
    );

    let mut prop_promise = cmds.spawn_prop_with_tile_def_promise(
        Cell::new(3, 3),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_VEGETATION_CATEGORY.string,
            "tree",
        ).unwrap(),
        SimCmds::no_object_callback(),
    );

    // Check pending commands return pending promises:
    tile_promise = match cmds.query_promise(tile_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    building_promise = match cmds.query_promise(building_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    unit_promise = match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    prop_promise = match cmds.query_promise(prop_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    // Resolved check should also return false for all.
    assert!(!cmds.is_promise_resolved(&tile_promise));
    assert!(!cmds.is_promise_resolved(&building_promise));
    assert!(!cmds.is_promise_resolved(&unit_promise));
    assert!(!cmds.is_promise_resolved(&prop_promise));

    // Execute commands:
    assert!(!cmds.is_empty());
    cmds.execute(&context);
    assert!(cmds.is_empty());

    // All should have been executed.
    assert!(cmds.is_promise_resolved(&tile_promise));
    assert!(cmds.is_promise_resolved(&building_promise));
    assert!(cmds.is_promise_resolved(&unit_promise));
    assert!(cmds.is_promise_resolved(&prop_promise));

    // Query promise results:
    let tile_result = match cmds.query_promise(tile_promise) {
        SpawnQueryResult::Ready(result) => result,
        err => panic!("Expected ready SpawnPromise, got {err} instead."),
    };

    let building_result = match cmds.query_promise(building_promise) {
        SpawnQueryResult::Ready(result) => result,
        err => panic!("Expected ready SpawnPromise, got {err} instead."),
    };

    let unit_result = match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Ready(result) => result,
        err => panic!("Expected ready SpawnPromise, got {err} instead."),
    };

    let prop_result = match cmds.query_promise(prop_promise) {
        SpawnQueryResult::Ready(result) => result,
        err => panic!("Expected ready SpawnPromise, got {err} instead."),
    };

    match tile_result {
        SpawnReadyResult::Tile(cell, _layer) => assert!(cell.is_valid()),
        _ => panic!("Expected SpawnReadyResult::Tile!"),
    }

    match building_result {
        SpawnReadyResult::GameObject(id) => assert!(id.is_valid()),
        _ => panic!("Expected SpawnReadyResult::GameObject!"),
    }

    match unit_result {
        SpawnReadyResult::GameObject(id) => assert!(id.is_valid()),
        _ => panic!("Expected SpawnReadyResult::GameObject!"),
    }

    match prop_result {
        SpawnReadyResult::GameObject(id) => assert!(id.is_valid()),
        _ => panic!("Expected SpawnReadyResult::GameObject!"),
    }
}

fn test_sim_cmd_queue_spawn_failure() {
    let mut test_env = TestEnvironment::new();
    let context = test_env.new_sim_context();
    let mut cmds = SimCmds::default();

    let mut unit_promise = cmds.spawn_unit_with_config_promise(
        Cell::new(999, 999), // Out of bounds cell - must fail.
        UnitConfigKey::Peasant,
        SimCmds::no_object_callback(),
    );

    // Check pending commands return pending promises:
    unit_promise = match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    // Resolved check should also return false.
    assert!(!cmds.is_promise_resolved(&unit_promise));

    // Execute commands:
    assert!(!cmds.is_empty());
    cmds.execute(&context);
    assert!(cmds.is_empty());

    // Should have been executed.
    assert!(cmds.is_promise_resolved(&unit_promise));

    // Expect a tile placement error.
    match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Failed(err) => {
            assert!(matches!(err.reason, TilePlacementErrReason::CellOutOfBounds));
        }
        res => panic!("Expected failed SpawnPromise, got {res} instead."),
    }
}

fn test_sim_cmd_queue_spawning_with_callbacks() {
    let mut test_env = TestEnvironment::new();
    let context = test_env.new_sim_context();
    let mut cmds = SimCmds::default();

    struct SpawnResults {
        tile_spawned: bool,
        building_spawned: bool,
        unit_spawned: bool,
        prop_spawned: bool,
        invalid_cell_spawn_failed: bool,
    }
    static SPAWN_RESULTS: SingleThreadStatic<SpawnResults> = SingleThreadStatic::new(
        SpawnResults {
            tile_spawned: false,
            building_spawned: false,
            unit_spawned: false,
            prop_spawned: false,
            invalid_cell_spawn_failed: false,
        }
    );

    // Push spawn commands:
    cmds.spawn_tile_with_tile_def_cb(
        Cell::new(0, 0),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Terrain,
            TERRAIN_LAND_CATEGORY.string,
            "grass",
        ).unwrap(),
        |_context, result| {
            assert!(result.is_ok());
            SPAWN_RESULTS.as_mut().tile_spawned = true;
        },
    );

    cmds.spawn_building_with_tile_def_cb(
        Cell::new(1, 1),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_BUILDINGS_CATEGORY.string,
            "small_well",
        ).unwrap(),
        |_context, result| {
            assert!(result.is_ok());
            SPAWN_RESULTS.as_mut().building_spawned = true;
        },
    );

    cmds.spawn_unit_with_config_cb(
        Cell::new(2, 2),
        UnitConfigKey::Peasant,
        |_context, result| {
            assert!(result.is_ok());
            SPAWN_RESULTS.as_mut().unit_spawned = true;
        },
    );

    cmds.spawn_prop_with_tile_def_cb(
        Cell::new(3, 3),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_VEGETATION_CATEGORY.string,
            "tree",
        ).unwrap(),
        |_context, result| {
            assert!(result.is_ok());
            SPAWN_RESULTS.as_mut().prop_spawned = true;
        },
    );

    // Push a command that will fail as well.
    cmds.spawn_unit_with_config_cb(
        Cell::new(999, 999), // Out of bounds cell - must fail.
        UnitConfigKey::Peasant,
        |_context, result| {
            assert!(result.is_err_and(|err| matches!(err.reason, TilePlacementErrReason::CellOutOfBounds)));
            SPAWN_RESULTS.as_mut().invalid_cell_spawn_failed = true;
        },
    );

    // Execute commands:
    assert!(!cmds.is_empty());
    cmds.execute(&context);
    assert!(cmds.is_empty());

    // Validate spawn results:
    {
        let results = SPAWN_RESULTS.as_ref();
        assert!(results.tile_spawned);
        assert!(results.building_spawned);
        assert!(results.unit_spawned);
        assert!(results.prop_spawned);
        assert!(results.invalid_cell_spawn_failed);
    }
}
