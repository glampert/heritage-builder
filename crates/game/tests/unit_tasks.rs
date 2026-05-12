use common::{time::Seconds, callback::Callback, coords::Cell, mem::SingleThreadStatic};
use game::{
    building::{BuildingKind, BuildingKindAndId},
    debug::{game_object_debug::GameObjectDebugVarRef, preset_maps},
    pathfind::{NodeKind as PathNodeKind, SearchResult},
    sim::{resources::ResourceKind, SimContext},
    unit::{
        UnitId,
        config::UnitConfigKey,
        navigation::UnitNavGoal,
        task::{
            UnitTaskArg, UnitTaskArgs, UnitTaskDeliverToStorage, UnitTaskDespawn,
            UnitTaskDespawnWithCallback, UnitTaskFetchFromStorage, UnitTaskFollowPath,
            UnitTaskPostDespawnCallback,
        },
    },
};

mod test_utils;
use test_utils::{
    TestEnvironment,
    assign_task, clear_terrain, find_building, find_building_id, find_building_mut,
    find_unit, find_unit_by_config, place_road, spawn_unit, tick, tick_until, unit_exists,
};

// ----------------------------------------------
// Integration tests for UnitTask archetypes
// ----------------------------------------------
//
// Coverage map (one archetype per section):
//   - UnitTaskDespawn / UnitTaskDespawnWithCallback
//   - UnitTaskFollowPath
//   - UnitTaskDeliverToStorage
//   - UnitTaskFetchFromStorage         -- pending TODOs
//   - UnitTaskHarvestWood              -- placeholder
//   - UnitTaskSettler                  -- placeholder
//   - UnitTaskRandomizedPatrol         -- placeholder
//
// Placeholder tests are registered so they show up in the output and serve
// as a concrete landing pad once the underlying recovery/scenario work is
// done (see the TODO comments in each).

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

// ---- Placeholders for known-broken recovery paths in fetch.rs ----
//
// These track source-level TODOs (crates/game/src/unit/task/fetch.rs).
// Each pending test body narrates the expected future behavior so that
// when the fix lands, the test body itself is the spec.

// Covers fetch.rs:178-181 -- unit finishes pickup, then loses its path
// back to the origin building. Today the task logs a warning, clears the
// inventory, and ends. Expected future behavior: unit reroutes to an
// alternate storage and deposits.
fn test_fetch_recovers_by_shipping_back_to_storage_when_origin_unreachable() {
    // TODO(crates/game/src/unit/task/fetch.rs:178-181): implement once
    // the recovery path lands in source. Until then this passes trivially.
    println!("(pending fix for fetch.rs:178)");
}

// Covers fetch.rs:246-250 and fetch.rs:298-302 -- unit returns to origin
// but the deferred unload fails (e.g. inventory full, building destroyed
// before callback). Today the task clears the unit's inventory. Expected
// future behavior: ship surplus to a storage.
fn test_fetch_recovers_when_unload_at_origin_fails() {
    // TODO(crates/game/src/unit/task/fetch.rs:246 + 298): implement once
    // the recovery path lands.
    println!("(pending fix for fetch.rs:246,298)");
}

// Covers fetch.rs:332-336 -- origin building is despawned while the unit
// is returning with the fetched resource. Today the task clears the
// inventory and ends. Expected future behavior: route to another storage.
fn test_fetch_recovers_when_origin_destroyed_mid_return() {
    // TODO(crates/game/src/unit/task/fetch.rs:332-336): implement once
    // the recovery path lands.
    println!("(pending fix for fetch.rs:332)");
}

// ----------------------------------------------
// UnitTaskHarvestWood
// ----------------------------------------------

// TODO: New preset maps needed:
// - 1 lumberyard, 1 storage yard / connecting road between, a few tree props
// - 2 lumberyard + 2 tree props

// Verifies the off-road traversable flags added in harvest.rs:212-220 --
// harvester should reach a tree placed on EmptyLand without a road.
// Preset map setup similar to test_harvest_claims_tree_then_returns_wood: LumberYard + tree prop.
fn test_harvest_traverses_off_road() {
    // TODO: scenario pending. Needs a tree on EmptyLand/Vegetation with
    // no road between it and the origin building.
    println!("(scenario pending: off-road tree setup)");
}

// Harvest needs an origin building with a road link, a tree prop within
// pathfinding range, and enough tick budget to cover WOOD_HARVEST_TIME_INTERVAL
// (20s in harvest.rs - can be customized to a smaller value for testing via UnitTaskHarvestWood::set_harvest_time_interval).
fn test_harvest_claims_tree_then_returns_wood() {
    // TODO: scenario pending. Needs a LumberYard origin + tree prop + StorageYard.
    // Create preset tile map for this test (see NOTES below).
    // Expected state chain:
    //   Running -> PendingHarvest (after harvest_timer elapses)
    //   -> PendingCompletionCallback -> Completed
    // Assert unit carries 1..=WOOD_HARVEST_MAX_AMOUNT wood during return,
    // and that it's deposited at the origin building.
    // NOTES:
    // - Load a preset map containing a lumberyard, storage yard and tree prop.
    // - Assert that resources flow from tree -> harvester unit -> lumberyard -> storage yard.
    // - Tree harvestable amount decreases.
    // - Tick until we complete a full harvest cycle:
    //    lumberyard spawns unit -> unit harvests tree -> unit returns to lumberyard
    //      -> lumberyard dispatches delivery to storage -> storage receives wood.
    println!("(scenario pending: harvester/tree/road-link setup)");
}

// Exercises the reroute branch at harvest.rs:323-365 -- a second harvester
// arriving at a tree already claimed by another unit picks a different one.
fn test_harvest_reroutes_when_tree_already_claimed() {
    // TODO: scenario pending. Needs 2x harvester origin + 2x tree props +
    // deterministic turn ordering so the claim race is observable.
    println!("(scenario pending: multi-harvester tree-claim setup)");
}

// ----------------------------------------------
// UnitTaskSettler
// ----------------------------------------------

// TODO: New preset maps needed:
// - EmptyLand terrain with a vacant lot, settler spawn point and one house level 0.
// - EmptyLand terrain without any vacant lots, settler spawn point and one house level 0.
// - EmptyLand terrain without any vacant lots or houses, only a settler spawn point.

// Settler prefers a vacant lot over a house (with `fallback_to_houses_with_room`
// enabled). Exercises the priority order in settler.rs:164-172.
fn test_settler_prefers_vacant_lot() {
    // TODO: scenario pending. Needs vacant lot terrain + a house with room +
    // a spawn point + settler-traversable terrain between them.
    // - Create a new Preset Tile Map covering this setup: Spawn point, vacant lot, house level 0 without population (the default).
    println!("(scenario pending: settler/vacant-lot/house setup)");
}

// Settler falls back to a house when no vacant lot is available.
fn test_settler_falls_back_to_house_when_no_lot() {
    // TODO: scenario pending: Preset map with settler spawn point + house level 0 without population (the default).
    println!("(scenario pending: settler fallback-to-house setup)");
}

// Settler returns to its spawn point (exits) when no settlement is available
// and `return_to_spawn_point_if_failed = true` (the default behavior).
fn test_settler_returns_to_spawn_when_no_settlement() {
    // TODO: scenario pending.
    // - Spawn settler mid map; map is empty, containing only one settler spawn point.
    // - Wait for settler to exit and despawn.
    println!("(scenario pending: settler exit-on-failure setup)");
}

// ----------------------------------------------
// UnitTaskRandomizedPatrol
// ----------------------------------------------

// TODO: New preset maps needed:
// - Map with a road network going north-south and east-west, intersecting in the middle. Market building at the intersection.
//   Make the roads long enough so that we can test patrols with a max_distance of a few tiles and different patrol direction.
//
// - Expanded version of the above that also includes houses ("house0") along the path, to test building visitation.

// Unit leaves its origin, wanders within `max_distance`, and returns.
fn test_patrol_leaves_and_returns_to_origin() {
    // TODO: scenario pending. Needs a patrol origin building (e.g. Market)
    // and a connected road network >= max_distance cells. Suppress default patrol
    // unit by setting the debug option "freeze_patrol" and manually spawn a patrol
    // unit with custom params instead (see examples in unit/debug.rs).
    println!("(scenario pending: patrol origin + road network setup)");
}

// Unit visits target buildings along its route when `buildings_to_visit` is set.
fn test_patrol_visits_target_buildings() {
    // TODO: scenario pending. Needs the above plus target building(s) of
    // the specified kind adjacent to the patrol route (e.g. house level 0).
    //
    // NOTE:
    // - How can we verify that visitation has happened? By storing BuildingVisitResult on the task perhaps?
    println!("(scenario pending: patrol target-building setup)");
}

// No waypoint chosen should be farther than `max_distance` cells from origin.
fn test_patrol_respects_max_distance() {
    // TODO: scenario pending. Track unit's cell at each tick, assert
    // max |cell - origin| <= max_distance.
    println!("(scenario pending: patrol max_distance instrumentation)");
}
