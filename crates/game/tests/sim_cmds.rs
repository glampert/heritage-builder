use common::{coords::Cell, Size};
use game::{
    world::World,
    config::GameConfigs,
    unit::config::UnitConfigKey,
    debug::utils::DebugSimContextBuilder,
    sim::commands::{SimCmds, SpawnQueryResult, SpawnReadyResult},
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
    ]);
}

struct TestEnvironment {
    tile_map: TileMap,
    world: World,
}

impl TestEnvironment {
    const MAP_SIZE_IN_CELLS: Size = Size::new(32, 32);

    fn new() -> Self {
        Self {
            tile_map: TileMap::new(Self::MAP_SIZE_IN_CELLS, None),
            world: World::new(),
        }
    }

    fn context_builder(&mut self) -> DebugSimContextBuilder<'_> {
        DebugSimContextBuilder::new(
            &mut self.world,
            &mut self.tile_map,
            Self::MAP_SIZE_IN_CELLS,
            GameConfigs::get(),
        )
    }
}

fn test_sim_cmd_queue_spawning() {
    let mut test_env = TestEnvironment::new();
    let mut context_builder = test_env.context_builder();
    let context = context_builder.new_sim_context();

    let mut cmds = SimCmds::new();

    // Push spawn commands:
    let mut tile_promise = cmds.spawn_tile_with_tile_def(
        Cell::new(0, 0),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Terrain,
            TERRAIN_LAND_CATEGORY.string,
            "grass",
        ).unwrap(),
    );

    let mut building_promise = cmds.spawn_building_with_tile_def(
        Cell::new(1, 1),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_BUILDINGS_CATEGORY.string,
            "small_well",
        ).unwrap(),
    );

    let mut unit_promise = cmds.spawn_unit_with_config(
        Cell::new(2, 2),
        UnitConfigKey::Peasant,
    );

    let mut prop_promise = cmds.spawn_prop_with_tile_def(
        Cell::new(3, 3),
        TileSets::get().find_tile_def_by_name(
            TileMapLayerKind::Objects,
            OBJECTS_VEGETATION_CATEGORY.string,
            "tree",
        ).unwrap(),
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

    // Ready check should also return false for all.
    assert!(!cmds.is_promise_ready(&tile_promise));
    assert!(!cmds.is_promise_ready(&building_promise));
    assert!(!cmds.is_promise_ready(&unit_promise));
    assert!(!cmds.is_promise_ready(&prop_promise));

    // Execute commands:
    assert!(!cmds.is_empty());
    cmds.execute(&context);
    assert!(cmds.is_empty());

    // All should have been executed.
    assert!(cmds.is_promise_ready(&tile_promise));
    assert!(cmds.is_promise_ready(&building_promise));
    assert!(cmds.is_promise_ready(&unit_promise));
    assert!(cmds.is_promise_ready(&prop_promise));

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
    let mut context_builder = test_env.context_builder();
    let context = context_builder.new_sim_context();

    let mut cmds = SimCmds::new();

    let mut unit_promise = cmds.spawn_unit_with_config(
        Cell::new(999, 999), // Out of bounds cell - must fail.
        UnitConfigKey::Peasant,
    );

    // Check pending commands return pending promises:
    unit_promise = match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Pending(promise) => promise,
        err => panic!("Expected pending SpawnPromise, got {err} instead."),
    };

    // Ready check should also return false.
    assert!(!cmds.is_promise_ready(&unit_promise));

    // Execute commands:
    assert!(!cmds.is_empty());
    cmds.execute(&context);
    assert!(cmds.is_empty());

    // Should have been executed.
    assert!(cmds.is_promise_ready(&unit_promise));

    // Expect a tile placement error.
    match cmds.query_promise(unit_promise) {
        SpawnQueryResult::Failed(err) => {
            assert!(matches!(err.reason, TilePlacementErrReason::CellOutOfBounds));
        }
        res => panic!("Expected failed SpawnPromise, got {res} instead."),
    }
}
