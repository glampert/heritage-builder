use arrayvec::ArrayVec;
use common::{time::Seconds, callback::Callback, coords::Cell, mem::SingleThreadStatic};
use game::{
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    debug::{game_object_debug::GameObjectDebugVarRef, preset_maps},
    pathfind::{NodeKind as PathNodeKind, SearchResult},
    sim::{
        resources::{ResourceKind, ShoppingList, StockItem},
        SimCmds, SimCmdQueue, SimContext,
    },
    system::settlers::Settler,
    tile::TileKind,
    unit::{
        Unit,
        UnitId,
        config::UnitConfigKey,
        navigation::UnitNavGoal,
        task::{
            UnitPatrolPathRecord, UnitTaskArg, UnitTaskArgs, UnitTaskDeliverToStorage,
            UnitTaskDespawn, UnitTaskDespawnWithCallback, UnitTaskFetchFromStorage,
            UnitTaskFetchState, UnitTaskFollowPath, UnitTaskHarvestWood,
            UnitTaskPatrolCompletionCallback, UnitTaskPatrolState,
            UnitTaskPostDespawnCallback, UnitTaskRandomizedPatrol,
        },
    },
};

mod test_utils;
use test_utils::{
    TestEnvironment,
    assign_task, clear_terrain, despawn_building, find_building, find_building_id,
    find_building_mut, find_unit, find_unit_by_config, place_road, spawn_unit, tick,
    tick_until, unit_exists,
};

// ----------------------------------------------
// Integration tests for UnitTask archetypes
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Unit Tasks", &[
        // UnitTaskDespawn
        test_utils::test_fn!(test_despawn_removes_unit_from_world),

        // UnitTaskDespawnWithCallback
        test_utils::test_fn!(test_despawn_with_callback_fires_callback),
        test_utils::test_fn!(test_despawn_with_callback_passes_extra_arg),

        // UnitTaskFollowPath
        test_utils::test_fn!(test_follow_path_reaches_goal),
        test_utils::test_fn!(test_follow_path_chains_to_completion_task),

        // UnitTaskDeliverToStorage
        test_utils::test_fn!(test_deliver_transfers_resources_to_storage),
        test_utils::test_fn!(test_deliver_producer_fallback_when_no_storage),
        test_utils::test_fn!(test_delivery_path_blocked_recovery),

        // UnitTaskFetchFromStorage
        test_utils::test_fn!(test_fetch_picks_up_and_returns_with_resource),
        test_utils::test_fn!(test_fetch_path_blocked_recovery),
        test_utils::test_fn!(test_fetch_path_blocked_no_recovery),
        test_utils::test_fn!(test_fetch_recovers_by_shipping_back_to_storage_when_origin_unreachable),
        test_utils::test_fn!(test_fetch_recovers_when_unload_at_origin_fails),
        test_utils::test_fn!(test_fetch_recovers_when_origin_destroyed_mid_return),

        // UnitTaskHarvestWood
        test_utils::test_fn!(test_harvest_traverses_off_road),
        test_utils::test_fn!(test_harvest_claims_tree_then_returns_wood),
        test_utils::test_fn!(test_harvest_reroutes_when_tree_already_claimed),

        // UnitTaskSettler
        test_utils::test_fn!(test_settler_prefers_vacant_lot),
        test_utils::test_fn!(test_settler_falls_back_to_house_when_no_lot),
        test_utils::test_fn!(test_settler_returns_to_spawn_when_no_settlement),

        // UnitTaskRandomizedPatrol
        test_utils::test_fn!(test_patrol_leaves_and_returns_to_origin),
        test_utils::test_fn!(test_patrol_visits_target_buildings),
        test_utils::test_fn!(test_patrol_respects_max_distance),
    ]);
}

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

// Assign a bare UnitTaskDespawn and confirm the unit is gone after one tick.
// Exercises the TerminateAndDespawn -> despawn_unit_with_id path in
// UnitTaskManager::run_unit_tasks and the SimCmds despawn execute step.
fn test_despawn_removes_unit_from_world() {
    let mut env = TestEnvironment::new();
    let unit_id = spawn_unit(&mut env, Cell::new(5, 5), UnitConfigKey::Peasant);
    assert!(unit_exists(&env, unit_id));

    let task_id = assign_task(&mut env, unit_id, UnitTaskDespawn);
    assert!(task_id.is_valid());

    // A single tick should run the task, queue the despawn, and flush it.
    tick(&mut env, TestEnvironment::TICK_DELTA_SECS);
    assert!(!unit_exists(&env, unit_id), "unit should be despawned after one tick");
}

// ----------------------------------------------
// UnitTaskDespawnWithCallback
// ----------------------------------------------

// The Callback<fn(..)> machinery round-trips through a global registry keyed
// on the fn pointer's name. We register the test callbacks once here.
static DESPAWN_CALLBACK_FIRED: SingleThreadStatic<bool> = SingleThreadStatic::new(false);
static DESPAWN_CALLBACK_ARG: SingleThreadStatic<i32> = SingleThreadStatic::new(0);
static DESPAWN_CALLBACK_PREV_CELL: SingleThreadStatic<Cell> = SingleThreadStatic::new(Cell::invalid());

fn despawn_test_callback(
    _cmds: &mut game::sim::SimCmds,
    _context: &SimContext,
    unit_prev_cell: Cell,
    _unit_prev_goal: Option<UnitNavGoal>,
    extra_args: &[UnitTaskArg],
) {
    DESPAWN_CALLBACK_FIRED.set(true);
    DESPAWN_CALLBACK_PREV_CELL.set(unit_prev_cell);
    if let Some(arg) = extra_args.first() {
        DESPAWN_CALLBACK_ARG.set(arg.as_i32());
    }
}

// Reset the shared callback observables for a fresh assertion.
fn reset_despawn_callback_observables() {
    DESPAWN_CALLBACK_FIRED.set(false);
    DESPAWN_CALLBACK_ARG.set(0);
    DESPAWN_CALLBACK_PREV_CELL.set(Cell::invalid());
}

fn register_despawn_test_callback() -> Callback<UnitTaskPostDespawnCallback> {
    // `register` is idempotent for the same fn pointer; subsequent calls
    // inside a single test run return the same handle.
    common::callback::register!(despawn_test_callback)
}

// Assign UnitTaskDespawnWithCallback, verify the callback fires after the unit despawns.
fn test_despawn_with_callback_fires_callback() {
    reset_despawn_callback_observables();

    let mut env = TestEnvironment::new();
    let origin = Cell::new(4, 4);
    let unit_id = spawn_unit(&mut env, origin, UnitConfigKey::Peasant);

    let cb = register_despawn_test_callback();
    let task = UnitTaskDespawnWithCallback {
        post_despawn_callback: cb,
        callback_extra_args: UnitTaskArgs::empty(),
    };
    assign_task(&mut env, unit_id, task);

    tick(&mut env, TestEnvironment::TICK_DELTA_SECS);

    assert!(!unit_exists(&env, unit_id));
    assert!(*DESPAWN_CALLBACK_FIRED, "post-despawn callback should have fired");
    assert_eq!(*DESPAWN_CALLBACK_PREV_CELL, origin, "callback received unit's previous cell");
}

// Confirm that a single UnitTaskArg is threaded through to the callback untouched.
fn test_despawn_with_callback_passes_extra_arg() {
    reset_despawn_callback_observables();

    let mut env = TestEnvironment::new();
    let unit_id = spawn_unit(&mut env, Cell::new(6, 6), UnitConfigKey::Peasant);

    let cb = register_despawn_test_callback();
    let task = UnitTaskDespawnWithCallback {
        post_despawn_callback: cb,
        callback_extra_args: UnitTaskArgs::new(&[UnitTaskArg::I32(42)]),
    };
    assign_task(&mut env, unit_id, task);

    tick(&mut env, TestEnvironment::TICK_DELTA_SECS);

    assert!(*DESPAWN_CALLBACK_FIRED);
    assert_eq!(*DESPAWN_CALLBACK_ARG, 42, "callback should receive the extra arg verbatim");
}

// ----------------------------------------------
// UnitTaskFollowPath
// ----------------------------------------------

// Build a straight road between start and end, pathfind, and return an
// owned Path (Vec<Node>). Panics if pathfinding fails -- test setup bug.
fn straight_road_path(env: &mut TestEnvironment, start: Cell, end: Cell) -> game::pathfind::Path {
    // Road cells must form a contiguous run for pathfinding to connect them.
    let cells: Vec<Cell> = if start.x == end.x {
        let (lo, hi) = (start.y.min(end.y), start.y.max(end.y));
        (lo..=hi).map(|y| Cell::new(start.x, y)).collect()
    } else if start.y == end.y {
        let (lo, hi) = (start.x.min(end.x), start.x.max(end.x));
        (lo..=hi).map(|x| Cell::new(x, start.y)).collect()
    } else {
        panic!("straight_road_path only supports axis-aligned start/end");
    };
    place_road(env, &cells);

    let context = env.new_sim_context(0.0);
    match context.find_path(PathNodeKind::Road, start, end) {
        SearchResult::PathFound(path) => path.clone(),
        SearchResult::PathNotFound => panic!("pathfind({start} -> {end}) failed"),
    }
}

// Plain FollowPath: unit walks a road and reaches the last cell, then the
// task completes. We let many ticks run since unit movement is time-scaled
// by movement_speed (~1.66 tiles/sec for Peasant).
fn test_follow_path_reaches_goal() {
    let mut env = TestEnvironment::new();
    let start = Cell::new(3, 3);
    let end = Cell::new(3, 8);
    let path = straight_road_path(&mut env, start, end);

    let unit_id = spawn_unit(&mut env, start, UnitConfigKey::Peasant);
    let task = UnitTaskFollowPath {
        path,
        completion_callback: Callback::default(),
        completion_task: None,
        terminate_if_stuck: false,
    };
    assign_task(&mut env, unit_id, task);

    // ~6 tiles at ~1.66 tiles/sec ≈ 3.6s. Cap at 200 ticks (20 s) to be safe.
    let ticks = tick_until(&mut env, 200, TestEnvironment::TICK_DELTA_SECS, |env| {
        find_unit(env, unit_id).cell() == end
    });
    assert!(ticks < 200, "unit should have reached the goal well within 200 ticks");

    // The task clears its goal on completion (see follow_path.rs -- unit.follow_path(None)),
    // so we can't assert has_reached_goal() after the fact. We can, however, confirm the
    // task was consumed and no further task is scheduled.
    assert!(find_unit(&env, unit_id).current_task().is_none(), "completed task should be cleared");
}

// Chain FollowPath -> Despawn. The unit walks the path, then the despawn
// task takes over, then the unit is gone.
fn test_follow_path_chains_to_completion_task() {
    let mut env = TestEnvironment::new();
    let start = Cell::new(2, 2);
    let end = Cell::new(2, 6);
    let path = straight_road_path(&mut env, start, end);

    let unit_id = spawn_unit(&mut env, start, UnitConfigKey::Peasant);

    // Pre-allocate the follow-up despawn task and wire it in.
    let despawn_task_id = env.sim.task_manager_mut().new_task(UnitTaskDespawn)
        .expect("task pool full");

    let task = UnitTaskFollowPath {
        path,
        completion_callback: Callback::default(),
        completion_task: Some(despawn_task_id),
        terminate_if_stuck: false,
    };
    assign_task(&mut env, unit_id, task);

    let ticks = tick_until(&mut env, 200, TestEnvironment::TICK_DELTA_SECS, |env| {
        !unit_exists(env, unit_id)
    });
    assert!(ticks < 200, "chained despawn should finish within 200 ticks");
}

// ----------------------------------------------
// UnitTaskDeliverToStorage
// ----------------------------------------------

// Coarse delta lets the producer's 20s production timer fire in ~20 ticks
// without us having to fast-forward the timer through internal APIs.
const DELIVER_TICK_DELTA_SECS: Seconds = 1.0;

// Set a bool debug option (e.g. "freeze_harvesting" / "freeze_production") on a building's
// archetype-level debug options struct. Used to suppress the producer side-effects we don't
// want firing during a focused delivery test.
fn set_building_debug_bool(env: &mut TestEnvironment, handle: BuildingKindAndId, name: &str, mut value: bool) {
    let ok = find_building_mut(env, handle)
        .debug_options()
        .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
    assert!(ok, "Building {} does not expose debug option '{}'", handle.kind, name);
}

// Producer-output-stock seed: lumberyard / rice farm both expose `add_production_output_stock`
// on their ProducerBuilding archetype, but the Building::as_producer accessor is a pub fn.
fn seed_producer_output(env: &mut TestEnvironment, handle: BuildingKindAndId, kind: ResourceKind, count: u32) {
    let producer = find_building_mut(env, handle).as_producer_mut();
    let stored = producer.add_production_output_stock(kind, count);
    assert!(stored, "Producer {} refused to store {count} {kind}", handle.kind);
}

// Tick until the producer dispatches its Runner. Returns the runner's UnitId.
fn tick_until_runner_spawned(env: &mut TestEnvironment, max_ticks: usize) -> UnitId {
    tick_until(env, max_ticks, DELIVER_TICK_DELTA_SECS, |env| {
        find_unit_by_config(env, UnitConfigKey::Runner).is_some()
    });
    find_unit_by_config(env, UnitConfigKey::Runner)
        .expect("Runner should have been dispatched")
}

// Tick until the runner has finished its task chain and despawned. The chained
// UnitTaskDespawn that runs after the delivery completion frees both tasks and
// releases the spawn promise, so this drains the sim's task and promise pools
// and prevents leak panics on TestEnvironment drop.
fn tick_until_runner_despawned(env: &mut TestEnvironment, runner_id: UnitId, max_ticks: usize) {
    let ticks = tick_until(env, max_ticks, DELIVER_TICK_DELTA_SECS, |env| {
        !unit_exists(env, runner_id)
    });
    assert!(ticks < max_ticks, "runner should have despawned within {max_ticks} ticks");
}

// Lumberyard -> StorageYard: producer dispatches Runner, Runner walks the road
// network, storage receives the delivery, producer's output stock drained.
fn test_deliver_transfers_resources_to_storage() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_LUMBERYARD_1_STORAGE_YARD,
    );

    let lumberyard = find_building_id(&env, BuildingKind::Lumberyard);
    let storage = find_building_id(&env, BuildingKind::StorageYard);

    // Skip the lumberyard's harvester branch -- we seed the output stock directly.
    set_building_debug_bool(&mut env, lumberyard, "freeze_harvesting", true);

    const N: u32 = 4;
    seed_producer_output(&mut env, lumberyard, ResourceKind::Wood, N);
    assert_eq!(find_building(&env, lumberyard).available_resources(ResourceKind::Wood), N);

    // ~20s for the production timer to fire + a few ticks for the runner to traverse.
    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Producer's output stock is cleared as soon as the runner is dispatched
    // (the resources are handed over to the unit).
    assert_eq!(find_building(&env, lumberyard).available_resources(ResourceKind::Wood), 0);

    // Wait for the runner to finish the delivery: storage gains N wood.
    let ticks = tick_until(&mut env, 100, DELIVER_TICK_DELTA_SECS, |env| {
        find_building(env, storage).available_resources(ResourceKind::Wood) >= N
    });
    assert!(ticks < 100, "delivery should complete within 100 ticks");

    assert_eq!(find_building(&env, storage).available_resources(ResourceKind::Wood), N);
    assert_eq!(find_building(&env, lumberyard).available_resources(ResourceKind::Wood), 0);

    // Drain the runner's chained UnitTaskDespawn so the sim's task pool is empty
    // before TestEnvironment drops. Stop the producer from re-dispatching now
    // that the storage already accepted the delivery.
    set_building_debug_bool(&mut env, lumberyard, "freeze_storage_delivery", true);
    tick_until_runner_despawned(&mut env, runner_id, 20);
}

// Rice farm -> Distillery (producer fallback): no Granary on the map, so the
// runner's primary delivery search fails and the fallback routes to a producer
// that consumes Rice as a raw material (Distillery -> Wine).
fn test_deliver_producer_fallback_when_no_storage() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_FARM_1_DISTILLERY,
    );

    let rice_farm  = find_building_id(&env, BuildingKind::Farm);
    let distillery = find_building_id(&env, BuildingKind::Factory);

    // Pin the rice farm's output to exactly N -- with freeze_production set, the
    // production timer won't add more to the stock before delivery dispatch.
    set_building_debug_bool(&mut env, rice_farm, "freeze_production", true);

    const N: u32 = 4;
    seed_producer_output(&mut env, rice_farm, ResourceKind::Rice, N);

    // Distillery starts empty: capacity - receivable_resources gives current input stock.
    let initial_capacity = find_building(&env, distillery).receivable_resources(ResourceKind::Rice);
    assert!(initial_capacity >= N, "Distillery should have room for {N} Rice");

    let runner_id = tick_until_runner_spawned(&mut env, 30);
    assert_eq!(find_building(&env, rice_farm).available_resources(ResourceKind::Rice), 0);

    // Wait for the distillery's input stock to grow by N.
    let ticks = tick_until(&mut env, 100, DELIVER_TICK_DELTA_SECS, |env| {
        let now_capacity = find_building(env, distillery).receivable_resources(ResourceKind::Rice);
        initial_capacity - now_capacity >= N
    });
    assert!(ticks < 100, "producer fallback delivery should complete within 100 ticks");

    let final_capacity = find_building(&env, distillery).receivable_resources(ResourceKind::Rice);
    assert_eq!(initial_capacity - final_capacity, N, "distillery should have absorbed exactly {N} Rice units");
    assert_eq!(find_building(&env, rice_farm).available_resources(ResourceKind::Rice), 0);

    // Drain the runner's task chain so the pools are empty before drop.
    set_building_debug_bool(&mut env, rice_farm, "freeze_storage_delivery", true);
    tick_until_runner_despawned(&mut env, runner_id, 20);
}

// Lumberyard -> StorageYard with the road torn out mid-delivery. Runner should
// idle (still owns the task, but no goal/path), then recover once the road is
// restored and finish the delivery.
fn test_delivery_path_blocked_recovery() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_LUMBERYARD_1_STORAGE_YARD,
    );

    let lumberyard = find_building_id(&env, BuildingKind::Lumberyard);
    let storage = find_building_id(&env, BuildingKind::StorageYard);

    set_building_debug_bool(&mut env, lumberyard, "freeze_harvesting", true);

    const N: u32 = 3;
    seed_producer_output(&mut env, lumberyard, ResourceKind::Wood, N);

    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Let the runner advance a few cells along the ring road before we tear out
    // the storage's only road links. Storage_yard 3x3 at (4,5)..(6,7) has its
    // adjacent road tiles on the bottom ring road row (cols 4..=6, row 8).
    let storage_road_cells = [Cell::new(4, 8), Cell::new(5, 8), Cell::new(6, 8)];
    for _ in 0..3 {
        tick(&mut env, DELIVER_TICK_DELTA_SECS);
    }
    clear_terrain(&mut env, &storage_road_cells);

    // Tick until the runner detects its path is blocked. The runner walks toward
    // the storage's road link; nav goes PathBlocked when it tries to step onto the
    // first removed cell, after which the delivery task can't pathfind to storage
    // (the building is no longer road-linked) so it idles.
    let ticks_to_block = tick_until(&mut env, 60, DELIVER_TICK_DELTA_SECS, |env| {
        find_unit(env, runner_id).path_is_blocked()
    });
    assert!(ticks_to_block < 60, "runner should detect path blocked within 60 ticks");

    // Runner is still alive, still owns its delivery task, and the storage
    // hasn't received anything because no route exists.
    assert!(unit_exists(&env, runner_id));
    {
        let runner = find_unit(&env, runner_id);
        let task_manager = env.sim.task_manager();
        assert!(
            runner.is_running_task::<UnitTaskDeliverToStorage>(task_manager),
            "runner should still own the delivery task while idle",
        );
    }
    assert_eq!(find_building(&env, storage).available_resources(ResourceKind::Wood), 0);

    // Restore the road and the runner should re-route on its next task tick.
    place_road(&mut env, &storage_road_cells);

    let ticks = tick_until(&mut env, 100, DELIVER_TICK_DELTA_SECS, |env| {
        find_building(env, storage).available_resources(ResourceKind::Wood) >= N
    });
    assert!(ticks < 100, "delivery should complete within 100 ticks after road restored");

    assert_eq!(find_building(&env, storage).available_resources(ResourceKind::Wood), N);
    assert_eq!(find_building(&env, lumberyard).available_resources(ResourceKind::Wood), 0);

    // Drain the runner's task chain so the pools are empty before drop.
    set_building_debug_bool(&mut env, lumberyard, "freeze_storage_delivery", true);
    tick_until_runner_despawned(&mut env, runner_id, 20);
}

// ----------------------------------------------
// UnitTaskFetchFromStorage
// ----------------------------------------------

// Helper: seed a storage building (granary / storage_yard) so its
// `available_resources(kind)` returns `count`. The cheat
// `ignore_worker_requirements` (set in test_utils setup) makes this work
// even though the storage has no workers in tests.
//
// `Building::receive_resources` only fills one storage slot per call (slots
// are 4 wide on both granary and storage yard), so we loop until we've
// deposited `count`.
fn seed_storage(env: &mut TestEnvironment, handle: BuildingKindAndId, kind: ResourceKind, count: u32) {
    let mut remaining = count;
    while remaining > 0 {
        let received = find_building_mut(env, handle).receive_resources(kind, remaining);
        assert!(received != 0, "Storage {} refused to store {kind} ({remaining} remaining)", handle.kind);
        remaining -= received;
    }
}

// Market -> Granary fetch cycle: stock_update timer fires, runner is dispatched
// to the granary, picks up rice, returns home, callback deposits rice into the
// market's stock.
fn test_fetch_picks_up_and_returns_with_resource() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY,
    );

    let market  = find_building_id(&env, BuildingKind::Market);
    let granary = find_building_id(&env, BuildingKind::Granary);

    // Suppress the market's Vendor patrol unit -- we only care about the runner.
    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    // Seed the granary with rice so the market has something to fetch. The
    // granary's slot capacity is 4, and `StorageSlots::available_resources`
    // reports a single slot, so we keep N within one slot for clean assertions.
    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), N);

    // ~20s for the market's stock_update timer to fire and dispatch a runner.
    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Pin to a single dispatch -- the market would otherwise keep sending out
    // runners while there are empty slots for other resources, leaking tasks
    // when the test ends.
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    // Wait for the full fetch cycle: market receives rice from granary.
    let ticks = tick_until(&mut env, 200, DELIVER_TICK_DELTA_SECS, |env| {
        find_building(env, market).available_resources(ResourceKind::Rice) >= N
    });
    assert!(ticks < 200, "fetch should complete within 200 ticks");

    assert_eq!(find_building(&env, market).available_resources(ResourceKind::Rice), N);
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), 0);

    // Drain the runner's chained UnitTaskDespawn so the sim's task and promise
    // pools are empty before TestEnvironment drops.
    tick_until_runner_despawned(&mut env, runner_id, 30);
}

// Market dispatches its runner to fetch from the granary, we tear out the
// granary's nearest road link mid-route; with two other road links still on
// row 8 the granary remains reachable, so the fetch task's `try_find_goal`
// succeeds with a longer path and the runner reroutes around the ring road.
// Restoring the cleared cell lets the runner walk back via the short path on
// the return leg.
fn test_fetch_path_blocked_recovery() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY,
    );

    let market  = find_building_id(&env, BuildingKind::Market);
    let granary = find_building_id(&env, BuildingKind::Granary);

    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);

    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Pin to a single dispatch.
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    // Let the runner advance a few cells, then remove the granary's currently
    // assigned road link. With (5,8) and (6,8) still present, the granary
    // is still road-linked, so the task can find a fresh path the long way
    // around the ring road.
    let blocked_cell = Cell::new(4, 8);
    for _ in 0..5 {
        tick(&mut env, DELIVER_TICK_DELTA_SECS);
    }
    clear_terrain(&mut env, &[blocked_cell]);

    // Runner reroutes -- task stays in MovingToGoal with a new (longer) path
    // and the storage building remains reachable.
    assert!(unit_exists(&env, runner_id));
    {
        let runner = find_unit(&env, runner_id);
        let task_manager = env.sim.task_manager();
        assert!(
            runner.is_running_task::<UnitTaskFetchFromStorage>(task_manager),
            "runner should still own the fetch task",
        );
    }

    // Restore the short path. The runner may have already started rerouting
    // around the ring; restoration just guarantees the return trip is short.
    place_road(&mut env, &[blocked_cell]);

    let ticks = tick_until(&mut env, 300, DELIVER_TICK_DELTA_SECS, |env| {
        find_building(env, market).available_resources(ResourceKind::Rice) >= N
    });
    assert!(ticks < 300, "fetch should complete within 300 ticks despite reroute");

    assert_eq!(find_building(&env, market).available_resources(ResourceKind::Rice), N);
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), 0);

    tick_until_runner_despawned(&mut env, runner_id, 30);
}

// Same dispatch as the recovery test, but we tear out *all three* of the
// granary's road links. With the granary fully disconnected, `try_find_goal`
// can't find a candidate (storage filtered by `is_linked_to_road`), so the
// fetch task gives up and transitions to ReturningToOrigin. The runner walks
// home empty-handed and despawns.
fn test_fetch_path_blocked_no_recovery() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY,
    );

    let market  = find_building_id(&env, BuildingKind::Market);
    let granary = find_building_id(&env, BuildingKind::Granary);

    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);

    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Let the runner advance a few cells, then disconnect the granary entirely.
    let granary_road_cells = [Cell::new(4, 8), Cell::new(5, 8), Cell::new(6, 8)];
    for _ in 0..3 {
        tick(&mut env, DELIVER_TICK_DELTA_SECS);
    }
    clear_terrain(&mut env, &granary_road_cells);

    // Stop further stock_update dispatches -- once this runner gives up and
    // despawns, the market would otherwise dispatch another that gets stuck
    // the same way.
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    // Runner should head home empty and despawn within a generous window
    // (it has to detect path-blocked, walk back, then run the chained Despawn).
    tick_until_runner_despawned(&mut env, runner_id, 200);

    // Market got nothing; granary's stock is untouched.
    assert_eq!(find_building(&env, market).available_resources(ResourceKind::Rice), 0);
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), N);
}

// ---- Recovery paths in fetch.rs ----

// When a fetch runner cannot deliver its cargo back to its origin building
// (origin unreachable, destroyed, or refusing the delivery), the task routes
// the surplus to any storage that will accept it instead of dropping the cargo.

// Sum the rice held across all storage buildings on the map (used to assert
// "rice ended up *somewhere*" without depending on which storage the surplus
// recovery happened to pick).
fn total_rice_in_storages(env: &TestEnvironment) -> u32 {
    let mut total = 0u32;
    env.world.for_each_building(BuildingKind::storage(), |b| {
        total += b.available_resources(ResourceKind::Rice);
        true
    });
    total
}

// Covers fetch.rs's "unit finishes pickup, then loses its path back to origin"
// path: after pickup, the market's road link cells are torn out. The runner's
// return path is blocked, try_return_to_origin fails on the next task tick,
// and the recovery routes the surplus to a storage building instead.
fn test_fetch_recovers_by_shipping_back_to_storage_when_origin_unreachable() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY_1_STORAGE_YARD,
    );

    let market       = find_building_id(&env, BuildingKind::Market);
    let granary      = find_building_id(&env, BuildingKind::Granary);
    let storage_yard = find_building_id(&env, BuildingKind::StorageYard);

    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), N);

    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Pin to a single dispatch so the test doesn't leak follow-up runners.
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    // Wait until the runner has rice on board -- it has finished the deferred
    // visit at the granary and is now in the ReturningToOrigin state.
    let ticks = tick_until(&mut env, 200, DELIVER_TICK_DELTA_SECS, |env| {
        let unit = find_unit(env, runner_id);
        !unit.inventory_is_empty()
    });
    assert!(ticks < 200, "runner should have picked up rice within 200 ticks");
    assert_eq!(find_building(&env, granary).available_resources(ResourceKind::Rice), 0);

    // Tear out *every* road tile adjacent to the market so the runner's return
    // path is unrecoverable. (Market 2x2 at (1,1)-(2,2); its only road
    // neighbours are the top-left corner of the ring road.)
    let market_road_cells = [
        Cell::new(0, 1), Cell::new(0, 2),
        Cell::new(1, 0), Cell::new(2, 0),
    ];
    clear_terrain(&mut env, &market_road_cells);

    // Recovery: the runner reroutes to a storage that will accept rice
    // (granary or storage_yard -- whichever is nearest from where the path
    // blocked). After the surplus is deposited, the runner runs its chained
    // UnitTaskDespawn and despawns.
    tick_until_runner_despawned(&mut env, runner_id, 300);

    // Market got nothing -- its road link is gone.
    assert_eq!(find_building(&env, market).available_resources(ResourceKind::Rice), 0);

    // No rice was abandoned: it ended up in granary, storage_yard, or some mix.
    assert_eq!(
        total_rice_in_storages(&env), N,
        "surplus rice should have been deposited at a storage, not dropped",
    );

    let granary_rice = find_building(&env, granary).available_resources(ResourceKind::Rice);
    let storage_yard_rice = find_building(&env, storage_yard).available_resources(ResourceKind::Rice);
    assert_eq!(granary_rice + storage_yard_rice, N);
}

// Covers fetch.rs's "origin unload fails after return" paths. We manually
// assign a UnitTaskFetchFromStorage with no completion callback so the
// origin's deferred-unload step is skipped. The task then hits the recovery
// branch in completed() with a non-empty inventory and ships the surplus
// to a storage instead of dropping the cargo.
fn test_fetch_recovers_when_unload_at_origin_fails() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY_1_STORAGE_YARD,
    );

    let market       = find_building_id(&env, BuildingKind::Market);
    let granary      = find_building_id(&env, BuildingKind::Granary);
    let storage_yard = find_building_id(&env, BuildingKind::StorageYard);

    // Stop the market from dispatching its own runner -- we drive the fetch
    // ourselves with a custom task wired with no completion callback.
    set_building_debug_bool(&mut env, market, "freeze_patrol", true);
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);

    let (market_road_link, market_base_cell) = {
        let b = find_building(&env, market);
        (b.road_link().expect("market should be road-linked"), b.base_cell())
    };

    let unit_id = spawn_unit(&mut env, market_road_link, UnitConfigKey::Peasant);

    let despawn_task_id = env.sim.task_manager_mut().new_task(UnitTaskDespawn)
        .expect("task pool full");

    let task = UnitTaskFetchFromStorage {
        origin_building: market,
        origin_building_tile: BuildingTileInfo {
            road_link: market_road_link,
            base_cell: market_base_cell,
        },
        storage_buildings_accepted: BuildingKind::storage(),
        resources_to_fetch: ShoppingList::from_items(&[
            StockItem { kind: ResourceKind::Rice, count: N },
        ]),
        // No completion callback -- forces the task into the recovery branch
        // (no deferred unload at origin) after the unit reaches the market.
        completion_callback: Callback::default(),
        completion_task: Some(despawn_task_id),
        internal_state: UnitTaskFetchState::default(),
    };
    assign_task(&mut env, unit_id, task);

    // Walk to the granary, pick up, walk back to the market road link.
    // No completion callback fires, so completed() drops into the recovery
    // path and ships the rice to a storage that will accept it.
    tick_until(&mut env, 300, DELIVER_TICK_DELTA_SECS, |env| !unit_exists(env, unit_id));

    assert_eq!(find_building(&env, market).available_resources(ResourceKind::Rice), 0);
    assert_eq!(
        total_rice_in_storages(&env), N,
        "surplus rice should have been deposited at a storage, not dropped",
    );

    let granary_rice = find_building(&env, granary).available_resources(ResourceKind::Rice);
    let storage_yard_rice = find_building(&env, storage_yard).available_resources(ResourceKind::Rice);
    assert_eq!(granary_rice + storage_yard_rice, N);
}

// Covers fetch.rs's "origin destroyed mid-trip" path: the market is despawned
// while the runner is still walking toward the granary. On arrival the runner
// picks up the rice, try_return_to_origin fails (origin no longer exists),
// and the in-callback recovery routes the surplus to a storage instead.
fn test_fetch_recovers_when_origin_destroyed_mid_return() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_MARKET_1_GRANARY_1_STORAGE_YARD,
    );

    let market       = find_building_id(&env, BuildingKind::Market);
    let granary      = find_building_id(&env, BuildingKind::Granary);
    let storage_yard = find_building_id(&env, BuildingKind::StorageYard);

    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    const N: u32 = 4;
    seed_storage(&mut env, granary, ResourceKind::Rice, N);

    let runner_id = tick_until_runner_spawned(&mut env, 30);

    // Pin to a single dispatch.
    set_building_debug_bool(&mut env, market, "freeze_stock_update", true);

    // Let the runner take a few steps toward the granary, then despawn the
    // market. By the time the runner reaches the granary, picks up, and tries
    // to route home, the origin building is gone.
    for _ in 0..3 {
        tick(&mut env, DELIVER_TICK_DELTA_SECS);
    }
    despawn_building(&mut env, market);
    assert!(
        env.world.find_building(market.kind, market.id).is_none(),
        "market should be despawned before the runner reaches the granary",
    );

    // Recovery: the runner picks up at the granary, finds the market gone,
    // and routes the surplus to a storage instead of dropping it.
    tick_until_runner_despawned(&mut env, runner_id, 300);

    // No market remains; no rice was abandoned.
    assert!(env.world.find_building(market.kind, market.id).is_none());
    assert_eq!(
        total_rice_in_storages(&env), N,
        "surplus rice should have been deposited at a storage, not dropped",
    );

    let granary_rice = find_building(&env, granary).available_resources(ResourceKind::Rice);
    let storage_yard_rice = find_building(&env, storage_yard).available_resources(ResourceKind::Rice);
    assert_eq!(granary_rice + storage_yard_rice, N);
}

// ----------------------------------------------
// UnitTaskHarvestWood
// ----------------------------------------------

// 1s ticks: the lumberyard's 20s production timer fires in ~20 ticks, and a
// 1s harvest timer (set below) wraps up in one extra tick.
const HARVEST_TICK_DELTA_SECS: Seconds = 1.0;

// Shrink the harvest timer (default 20s, harvest.rs) to a single tick.
// The static lingers across tests within the suite, but nothing else in this
// file exercises harvesting, so idempotent reapplication is fine.
fn set_short_harvest_interval() {
    UnitTaskHarvestWood::set_harvest_time_interval(1.0);
}

// Tick until a unit of the given config exists; return its id.
fn tick_until_unit_of_config_spawned(env: &mut TestEnvironment, config: UnitConfigKey, max_ticks: usize) -> UnitId {
    let ticks = tick_until(env, max_ticks, HARVEST_TICK_DELTA_SECS, |env| {
        find_unit_by_config(env, config).is_some()
    });
    assert!(ticks < max_ticks, "{config:?} should have spawned within {max_ticks} ticks");
    find_unit_by_config(env, config).expect("unit should be spawned")
}

// Drain in-flight units of `config` -- caller should set the appropriate
// freeze_* flags first or the producer will just dispatch another one.
fn tick_until_no_units_of_config(env: &mut TestEnvironment, config: UnitConfigKey, max_ticks: usize) {
    let ticks = tick_until(env, max_ticks, HARVEST_TICK_DELTA_SECS, |env| {
        find_unit_by_config(env, config).is_none()
    });
    assert!(ticks < max_ticks, "all {config:?} units should have despawned within {max_ticks} ticks");
}

// Stop the lumberyard from dispatching new harvesters/runners, then wait for
// any in-flight ones to finish. Leaves the sim's task/promise pools empty so
// TestEnvironment can drop without a leak panic.
fn drain_harvest_pipeline(env: &mut TestEnvironment, lumberyards: &[BuildingKindAndId], max_ticks: usize) {
    for &lumberyard in lumberyards {
        set_building_debug_bool(env, lumberyard, "freeze_harvesting", true);
        set_building_debug_bool(env, lumberyard, "freeze_storage_delivery", true);
    }
    tick_until_no_units_of_config(env, UnitConfigKey::Peasant, max_ticks);
    tick_until_no_units_of_config(env, UnitConfigKey::Runner, max_ticks);
}

// The harvester gains EmptyLand/VacantLot/SettlersSpawnPoint traversal in
// UnitTaskHarvestWood::initialize (harvest.rs). Preset 7 places its
// trees on grass with no road in between, so reaching a tree at all proves
// the off-road flags are in effect.
fn test_harvest_traverses_off_road() {
    set_short_harvest_interval();

    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_LUMBERYARD_1_STORAGE_YARD_WITH_TREES,
    );

    let lumberyard = find_building_id(&env, BuildingKind::Lumberyard);

    let harvester_id = tick_until_unit_of_config_spawned(&mut env, UnitConfigKey::Peasant, 30);

    // tick_until returns the tick *after* the unit was spawned, but the task
    // hasn't been ticked yet -- initialize() (which expands the traversable
    // kinds at harvest.rs and assigns a goal via try_find_goal) runs
    // on the first task update. Tick once more so initialization has happened
    // before we read state from the unit.
    tick(&mut env, HARVEST_TICK_DELTA_SECS);

    // After initialize() the unit traversable kinds must include EmptyLand --
    // the Peasant config defaults to Road-only.
    let traversable = find_unit(&env, harvester_id).traversable_node_kinds();
    assert!(
        traversable.intersects(PathNodeKind::EmptyLand),
        "harvester should have EmptyLand traversable kind, got {traversable}",
    );

    // The goal cell (a tree-adjacent neighbor, from harvest.rs) sits on
    // EmptyLand grass -- preset 7 has trees at (5,3)/(6,4)/(4,5), all interior
    // cells with no road in reach. If the off-road flags hadn't been added in
    // initialize(), pathfinding from the road link would have failed and the
    // goal would remain unset.
    let goal_cell = find_unit(&env, harvester_id)
        .goal()
        .expect("harvester should have a goal after task initialization")
        .destination_cell();
    let goal_path_kind = env.tile_map
        .find_tile(goal_cell, TileKind::Terrain)
        .expect("goal cell should have a terrain tile")
        .path_kind();
    assert!(
        goal_path_kind.intersects(PathNodeKind::EmptyLand),
        "harvester goal should be on grass (off-road), got {goal_path_kind}",
    );

    drain_harvest_pipeline(&mut env, &[lumberyard], 100);
}

// Full harvest cycle: harvester claims a tree, harvest_timer elapses, the
// deferred prop_update credits the unit with wood, the unit walks back, and
// `ProducerBuilding::on_resources_harvested` deposits the wood into the
// lumberyard's output stock. Also verifies the tree's harvestable amount drops.
fn test_harvest_claims_tree_then_returns_wood() {
    set_short_harvest_interval();

    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_1_LUMBERYARD_1_STORAGE_YARD_WITH_TREES,
    );

    let lumberyard = find_building_id(&env, BuildingKind::Lumberyard);
    assert_eq!(find_building(&env, lumberyard).available_resources(ResourceKind::Wood), 0);

    // Total harvestable wood across the three trees on the map before the cycle.
    let tree_cells = [Cell::new(5, 3), Cell::new(6, 4), Cell::new(4, 5)];
    let total_wood_pre: u32 = tree_cells
        .iter()
        .filter_map(|c| env.world.find_prop_for_cell(*c, &env.tile_map))
        .map(|p| p.harvestable_amount())
        .sum();

    let harvester_id = tick_until_unit_of_config_spawned(&mut env, UnitConfigKey::Peasant, 30);

    // Full cycle: walk -> claim -> harvest -> return -> deposit -> despawn.
    // The deposit (on_resources_harvested, producer.rs) happens before the
    // chained UnitTaskDespawn fires, so once the unit is gone the stock is non-zero.
    let _ = harvester_id; // keep ID for assertion ergonomics; presence already validated
    tick_until_no_units_of_config(&mut env, UnitConfigKey::Peasant, 100);

    let wood_in_stock = find_building(&env, lumberyard).available_resources(ResourceKind::Wood);
    assert!(wood_in_stock != 0, "lumberyard should have received wood, got {wood_in_stock}");

    // Harvest amount is random in 1..WOOD_HARVEST_MAX_AMOUNT (=5).
    assert!(wood_in_stock < 5, "single harvest should yield 1..=4 wood, got {wood_in_stock}");

    // Some tree on the map lost exactly `wood_in_stock` wood. (We can't pin
    // *which* tree without re-deriving the RNG path -- the harvester picks
    // randomly from accepting candidates, harvest.rs)
    let total_wood_post: u32 = tree_cells
        .iter()
        .filter_map(|c| env.world.find_prop_for_cell(*c, &env.tile_map))
        .map(|p| p.harvestable_amount())
        .sum();
    assert_eq!(
        total_wood_pre - total_wood_post, wood_in_stock,
        "wood removed from tree(s) should equal wood deposited in the lumberyard",
    );

    drain_harvest_pipeline(&mut env, &[lumberyard], 100);
}

// Two lumberyards + two trees -> harvesters race for claims. Both end up with
// wood, which can only happen if (a) the path filter rejects already-claimed
// trees and/or (b) the reroute branch in harvest.rs reassigns a unit whose
// tree got claimed away by another harvester.
fn test_harvest_reroutes_when_tree_already_claimed() {
    set_short_harvest_interval();

    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_2_LUMBERYARDS_2_TREES,
    );

    // Collect both lumberyard handles.
    let mut lumberyard_ids = Vec::new();
    env.world.for_each_building(BuildingKind::Lumberyard, |b| {
        lumberyard_ids.push(b.kind_and_id());
        true
    });
    assert_eq!(lumberyard_ids.len(), 2, "preset should expose two lumberyards");

    // Tick until both lumberyards have at least one unit of wood.
    let ticks = tick_until(&mut env, 200, HARVEST_TICK_DELTA_SECS, |env| {
        lumberyard_ids
            .iter()
            .all(|id| find_building(env, *id).available_resources(ResourceKind::Wood) != 0)
    });
    assert!(ticks < 200, "both lumberyards should have wood within 200 ticks");

    for id in &lumberyard_ids {
        assert!(find_building(&env, *id).available_resources(ResourceKind::Wood) != 0);
    }

    drain_harvest_pipeline(&mut env, &lumberyard_ids, 100);
}

// ----------------------------------------------
// UnitTaskSettler
// ----------------------------------------------

// Coarser delta than the global default -- the settler has to walk a handful
// of grass cells and we don't want the tests churning for hundreds of ticks.
const SETTLER_TICK_DELTA_SECS: Seconds = 0.5;

// Spawn a settler via the helper Settler::try_spawn. Routes through a local
// SimCmds because the spawn command's on_spawned callback (and the chained task allocation)
// all settle within this execute pass -- no promise survives past the local cmds drop.
fn spawn_settler(env: &mut TestEnvironment, origin: Cell, population: u32) -> UnitId {
    let mut cmds = SimCmds::default();
    {
        let context = env.new_sim_context(0.0);
        Settler::try_spawn(&mut cmds, &context, origin, population);
        cmds.execute(&context);
    }
    find_unit_by_config(&env, UnitConfigKey::Settler)
        .expect("Settler should have been spawned")
}

// Count buildings of `kind` currently spawned in the world.
fn count_buildings(env: &TestEnvironment, kind: BuildingKind) -> usize {
    let mut count = 0;
    env.world.for_each_building(kind, |_| {
        count += 1;
        true
    });
    count
}

// Tick until the settler despawns. Drains the chained UnitTaskDespawn (and
// the spawn_building command that on_settled queues for vacant-lot goals)
// so the task / promise pools are empty before TestEnvironment drops.
fn tick_until_settler_despawned(env: &mut TestEnvironment, settler_id: UnitId, max_ticks: usize) {
    let ticks = tick_until(env, max_ticks, SETTLER_TICK_DELTA_SECS, |env| {
        !unit_exists(env, settler_id)
    });
    assert!(ticks < max_ticks, "settler should despawn within {max_ticks} ticks");
}

// Settler with a vacant lot in reach picks the lot over an existing house.
// After despawn, `Settler::on_settled` spawns a new house at the lot cell and
// seeds it with the settler's population.
fn test_settler_prefers_vacant_lot() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_SETTLER_VACANT_LOT_AND_HOUSE,
    );

    // NOTE: Cells assumed from the preset map above.
    let spawn_point     = Cell::new(4, 0);
    let vacant_lot_cell = Cell::new(4, 3);
    let original_house  = Cell::new(4, 7);

    // Sanity: a single (empty) house exists at the start.
    assert_eq!(count_buildings(&env, BuildingKind::House), 1);
    assert!(env.world.find_building_for_cell(original_house, &env.tile_map).is_some());

    let settler_id = spawn_settler(&mut env, spawn_point, 1);

    // Walk to the lot, complete the settler task, run the chained Despawn, and
    // let on_settled's queued building spawn flush. ~7 cells at 1.66 t/s + a
    // few ticks for command execution.
    tick_until_settler_despawned(&mut env, settler_id, 100);

    // A new house now sits on the vacant lot, populated by the settler.
    let new_house = env.world
        .find_building_for_cell(vacant_lot_cell, &env.tile_map)
        .expect("new house should be placed on the vacant lot");
    assert!(new_house.is(BuildingKind::House));
    assert_eq!(new_house.population_count(), 1, "settler should have populated the new house");

    // The pre-existing house is untouched.
    let original = env.world
        .find_building_for_cell(original_house, &env.tile_map)
        .expect("original house should still exist");
    assert_eq!(original.population_count(), 0);

    assert_eq!(count_buildings(&env, BuildingKind::House), 2);
}

// No vacant lot in reach -> settler falls back to the existing house and
// adds to its population. Exercises the `fallback_to_houses_with_room`
// branch in settler.rs plus HouseBuilding::visited_by_settler.
fn test_settler_falls_back_to_house_when_no_lot() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_SETTLER_HOUSE_ONLY,
    );

    // NOTE: Cells assumed from the preset map above.
    let spawn_point = Cell::new(4, 0);
    let house_cell  = Cell::new(4, 4);

    let house_id = find_building_id(&env, BuildingKind::House);
    assert_eq!(find_building(&env, house_id).population_count(), 0, "house starts empty");

    let settler_id = spawn_settler(&mut env, spawn_point, 1);

    tick_until_settler_despawned(&mut env, settler_id, 100);

    // No new building was created; the existing house gained the settler.
    assert_eq!(count_buildings(&env, BuildingKind::House), 1);
    let house = env.world
        .find_building_for_cell(house_cell, &env.tile_map)
        .expect("original house should still exist");
    assert_eq!(house.population_count(), 1, "house should have absorbed the settler");
}

// No vacant lot, no house -> settler routes back to its spawn point and despawns empty.
// Exercises the `return_to_spawn_point_if_failed` branch in settler.rs.
fn test_settler_returns_to_spawn_when_no_settlement() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_SETTLER_SPAWN_POINT_ONLY,
    );

    // Spawn the settler away from the spawn point so the return walk is
    // observable (and the test doesn't trivially pass on tick zero).
    let spawn_point    = Cell::new(4, 0);
    let settler_origin = Cell::new(4, 4);

    let settler_id = spawn_settler(&mut env, settler_origin, 1);

    tick_until_settler_despawned(&mut env, settler_id, 100);

    // Nothing was built; settler walked off the map via the spawn-point tile.
    assert_eq!(count_buildings(&env, BuildingKind::House), 0);

    // Sanity: the spawn point tile is still flagged for path-finding.
    let context = env.new_sim_context(0.0);
    assert!(
        context.graph().settlers_spawn_point().is_some_and(|node| node.cell == spawn_point),
        "spawn point should still be in the search graph",
    );
}

// ----------------------------------------------
// UnitTaskRandomizedPatrol
// ----------------------------------------------

// 0.5s ticks give the Peasant (1.66 t/s) enough granularity to advance by ~one
// cell per tick during the outbound + return legs.
const PATROL_TICK_DELTA_SECS: Seconds = 0.5;

// UnitTaskRandomizedPatrol's completed() path expects a registered completion
// callback when the unit reaches origin (patrol.rs). Production code
// uses Patrol::on_randomized_patrol_completed, but that callback asserts the
// building owns a `Patrol` helper with state -- which we bypass when assigning
// the task manually. So we register a no-op here just to satisfy the callback
// invocation path; the test doesn't observe anything via this callback.
fn patrol_test_callback(_context: &SimContext, _building: &mut Building, _unit: &mut Unit) {
}

fn register_patrol_test_callback() -> Callback<UnitTaskPatrolCompletionCallback> {
    common::callback::register!(patrol_test_callback)
}

// Spawn a Peasant at the given building's road link and assign a manually-built
// UnitTaskRandomizedPatrol with a chained UnitTaskDespawn. Returns the unit's id.
fn spawn_patrol_from_building(
    env: &mut TestEnvironment,
    origin_building: BuildingKindAndId,
    max_distance: i32,
    buildings_to_visit: Option<BuildingKind>,
) -> (UnitId, Cell) {
    let (road_link, base_cell) = {
        let b = find_building(env, origin_building);
        (b.road_link().expect("origin building should be road-linked"), b.base_cell())
    };

    let unit_id = spawn_unit(env, road_link, UnitConfigKey::Peasant);

    let completion_task = env.sim.task_manager_mut().new_task(UnitTaskDespawn);

    let task = UnitTaskRandomizedPatrol {
        origin_building,
        origin_building_tile: BuildingTileInfo { road_link, base_cell },
        max_distance,
        path_bias_min: 0.1,
        path_bias_max: 0.5,
        path_record: UnitPatrolPathRecord::default(),
        buildings_to_visit,
        completion_callback: register_patrol_test_callback(),
        completion_task,
        idle_countdown: None,
        internal_state: UnitTaskPatrolState::default(),
        visited_buildings: ArrayVec::new(),
    };
    assign_task(env, unit_id, task);

    (unit_id, road_link)
}

// Patrol unit leaves its origin road link, wanders out to a waypoint, then
// returns home -- the chained UnitTaskDespawn fires once the patrol task ends.
fn test_patrol_leaves_and_returns_to_origin() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_PATROL_CROSSROADS_MARKET,
    );

    let market = find_building_id(&env, BuildingKind::Market);

    // Stop the market's default Vendor patrol from being dispatched concurrently.
    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    let (unit_id, origin) = spawn_patrol_from_building(&mut env, market, 10, None);

    // Unit should step off the origin within a handful of ticks.
    let ticks = tick_until(&mut env, 50, PATROL_TICK_DELTA_SECS, |env| {
        unit_exists(env, unit_id) && find_unit(env, unit_id).cell() != origin
    });
    assert!(ticks < 50, "unit should leave the origin road link within 50 ticks");

    // ...and despawn once it returns and the chained UnitTaskDespawn fires.
    let ticks = tick_until(&mut env, 200, PATROL_TICK_DELTA_SECS, |env| {
        !unit_exists(env, unit_id)
    });
    assert!(ticks < 200, "unit should return and despawn within 200 ticks");
}

// With buildings_to_visit set, the patrol must record at least one target
// building it queued a visit for while walking past it on the road. Preset 13
// places houses immediately off the crossroads so the unit can hardly avoid
// passing at least one BuildingRoadLink node next to a house.
fn test_patrol_visits_target_buildings() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_PATROL_CROSSROADS_MARKET_WITH_HOUSES,
    );

    let market = find_building_id(&env, BuildingKind::Market);
    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    let (unit_id, _) = spawn_patrol_from_building(
        &mut env,
        market,
        10,
        Some(BuildingKind::House),
    );

    // Tick until the patrol records at least one visit. We poll the task's
    // visited_buildings list (populated in patrol.rs each time the
    // unit stands on a BuildingRoadLink next to a House).
    let ticks = tick_until(&mut env, 100, PATROL_TICK_DELTA_SECS, |env| {
        if !unit_exists(env, unit_id) {
            return false;
        }
        let unit = find_unit(env, unit_id);
        let task = unit.current_task_as::<UnitTaskRandomizedPatrol>(env.sim.task_manager());
        task.is_some_and(|t| !t.visited_buildings.is_empty())
    });
    assert!(ticks < 100, "patrol should visit at least one target building within 100 ticks");

    // Every recorded visit should be a House -- the patrol filter must respect `buildings_to_visit`.
    let visits: Vec<BuildingKindAndId> = {
        let unit = find_unit(&env, unit_id);
        let task = unit.current_task_as::<UnitTaskRandomizedPatrol>(env.sim.task_manager())
            .expect("patrol should still own its task");
        task.visited_buildings.iter().copied().collect()
    };
    assert!(!visits.is_empty(), "expected at least one visited building");
    for v in &visits {
        assert!(
            v.kind == BuildingKind::House,
            "patrol visited a non-House building: kind={} id={}", v.kind, v.id,
        );
    }

    // Drain: let the patrol finish so the task pool is empty before drop.
    let ticks = tick_until(&mut env, 300, PATROL_TICK_DELTA_SECS, |env| {
        !unit_exists(env, unit_id)
    });
    assert!(ticks < 300, "patrol should despawn within 300 ticks");
}

// The patrol picks a waypoint within `max_distance` manhattan cells of the
// unit's current position (in pathfind/mod.rs), and then returns to origin,
// so at no point during the run should the unit be farther than
// `max_distance` from the origin road link.
fn test_patrol_respects_max_distance() {
    let mut env = TestEnvironment::with_preset_map(
        preset_maps::PRESET_PATROL_CROSSROADS_MARKET,
    );

    let market = find_building_id(&env, BuildingKind::Market);
    set_building_debug_bool(&mut env, market, "freeze_patrol", true);

    const MAX_DISTANCE: i32 = 6; // > PATROL_MIN_PREFERRED_PATH_LEN (=4)
    let (unit_id, origin) = spawn_patrol_from_building(&mut env, market, MAX_DISTANCE, None);

    // Sample the unit's cell each tick until it despawns. Predicate-style loop
    // so we can both bound the iteration count and observe the trajectory.
    let mut max_observed = 0i32;
    let max_ticks = 200;
    let mut ticks_used = 0;
    for _ in 0..max_ticks {
        if !unit_exists(&env, unit_id) {
            break;
        }
        let cell = find_unit(&env, unit_id).cell();
        max_observed = max_observed.max(cell.manhattan_distance(origin));
        tick(&mut env, PATROL_TICK_DELTA_SECS);
        ticks_used += 1;
    }

    assert!(
        ticks_used < max_ticks,
        "patrol should have despawned within {max_ticks} ticks (used {ticks_used})",
    );
    assert!(
        max_observed <= MAX_DISTANCE,
        "patrol drifted {max_observed} cells from origin (max_distance={MAX_DISTANCE})",
    );
    // Also confirm we actually saw movement -- otherwise the assertion is vacuous.
    assert!(max_observed > 0, "patrol should have moved at least one cell from origin");
}
