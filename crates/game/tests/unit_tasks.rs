use common::{callback::Callback, coords::Cell, mem::SingleThreadStatic};
use game::{
    pathfind::{NodeKind as PathNodeKind, SearchResult},
    sim::SimContext,
    unit::{
        config::UnitConfigKey,
        navigation::UnitNavGoal,
        task::{
            UnitTaskArg, UnitTaskArgs, UnitTaskDespawn, UnitTaskDespawnWithCallback,
            UnitTaskFollowPath, UnitTaskPostDespawnCallback,
        },
    },
};

mod test_utils;
use test_utils::{
    TestEnvironment,
    assign_task, place_road, spawn_unit, tick, tick_until, find_unit, unit_exists,
};

// ----------------------------------------------
// Integration tests for UnitTask archetypes
// ----------------------------------------------
//
// Coverage map (one archetype per section):
//   - UnitTaskDespawn / UnitTaskDespawnWithCallback
//   - UnitTaskFollowPath
//   - UnitTaskDeliverToStorage         -- placeholder, see section
//   - UnitTaskFetchFromStorage         -- placeholder + source TODOs
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

        // UnitTaskFetchFromStorage
        test_utils::test_fn!(test_fetch_picks_up_and_returns_with_resource),
        test_utils::test_fn!(test_fetch_recovers_by_shipping_back_to_storage_when_origin_unreachable),
        test_utils::test_fn!(test_fetch_recovers_when_unload_at_origin_fails),
        test_utils::test_fn!(test_fetch_recovers_when_origin_destroyed_mid_return),

        // UnitTaskHarvestWood
        test_utils::test_fn!(test_harvest_claims_tree_then_returns_wood),
        test_utils::test_fn!(test_harvest_reroutes_when_tree_already_claimed),
        test_utils::test_fn!(test_harvest_traverses_off_road),

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
    *DESPAWN_CALLBACK_FIRED.as_mut() = true;
    *DESPAWN_CALLBACK_PREV_CELL.as_mut() = unit_prev_cell;
    if let Some(arg) = extra_args.first() {
        *DESPAWN_CALLBACK_ARG.as_mut() = arg.as_i32();
    }
}

// Reset the shared callback observables for a fresh assertion.
fn reset_despawn_callback_observables() {
    *DESPAWN_CALLBACK_FIRED.as_mut() = false;
    *DESPAWN_CALLBACK_ARG.as_mut() = 0;
    *DESPAWN_CALLBACK_PREV_CELL.as_mut() = Cell::invalid();
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

// Deliver requires: an origin producer building, a storage building with
// free capacity, a connecting road, and an inventory seeded on the unit.
// That's a full vertical slice of the world — buildings + road links +
// inventories + completion callbacks. Deferred until the task-level
// harness gains a `spawn_linked_producer_and_storage(...)` convenience.
//
// Registered so the scenario is visible in test output. The end-to-end
// assertions are left as TODOs rather than faked out.
fn test_deliver_transfers_resources_to_storage() {
    // TODO: requires producer + storage + road-link setup beyond what the
    // current scenario builders provide. Exercise:
    //   1. spawn producer (origin), storage (destination), connecting road
    //   2. seed unit inventory with N wood
    //   3. assign UnitTaskDeliverToStorage with resource_kind = Wood, count = N
    //   4. tick until state == Completed
    //   5. assert storage received N wood and completion callback fired on origin
    println!("(scenario pending: producer/storage/road-link setup)");
}

// As above, with `allow_producer_fallback = true` and no storage on the map.
// The delivery should be accepted by a compatible producer building instead.
fn test_deliver_producer_fallback_when_no_storage() {
    // TODO: once the harness grows a producer/storage setup, parametrise
    // to drop the storage building and set allow_producer_fallback = true,
    // then assert the delivery landed in the producer.
    println!("(scenario pending: producer fallback setup)");
}

// ----------------------------------------------
// UnitTaskFetchFromStorage
// ----------------------------------------------

fn test_fetch_picks_up_and_returns_with_resource() {
    // TODO: scenario pending. Needs origin consumer + stocked storage +
    // road-link. Expected state chain:
    //   MovingToGoal -> PendingBuildingVisit -> ReturningToOrigin
    //   -> PendingCompletionCallback -> Completed
    // Assert origin inventory grew and storage inventory shrank by N.
    println!("(scenario pending: consumer/storage/road-link setup)");
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

// Harvest needs an origin building with a road link, a tree prop within
// pathfinding range, and enough tick budget to cover WOOD_HARVEST_TIME_INTERVAL
// (20s in harvest.rs) -- feasible but requires real building placement to
// produce a valid road_link, so registered as a scenario pending.
fn test_harvest_claims_tree_then_returns_wood() {
    // TODO: scenario pending. Needs a LumberYard-like origin + tree prop
    // + road-link. Expected state chain:
    //   Running -> PendingHarvest (after harvest_timer elapses)
    //   -> PendingCompletionCallback -> Completed
    // Assert unit carries 1..=WOOD_HARVEST_MAX_AMOUNT wood during return,
    // and that it's deposited at the origin building.
    println!("(scenario pending: harvester/tree/road-link setup)");
}

// Exercises the reroute branch at harvest.rs:323-365 -- a second harvester
// arriving at a tree already claimed by another unit picks a different one.
fn test_harvest_reroutes_when_tree_already_claimed() {
    // TODO: scenario pending. Needs 2x harvester origin + 2x tree props +
    // deterministic turn ordering so the claim race is observable.
    println!("(scenario pending: multi-harvester tree-claim setup)");
}

// Verifies the off-road traversable flags added in harvest.rs:212-220 --
// harvester should reach a tree placed on EmptyLand without a road.
fn test_harvest_traverses_off_road() {
    // TODO: scenario pending. Needs a tree on EmptyLand/Vegetation with
    // no road between it and the origin building.
    println!("(scenario pending: off-road tree setup)");
}

// ----------------------------------------------
// UnitTaskSettler
// ----------------------------------------------

// Settler prefers a vacant lot over a house (with `fallback_to_houses_with_room`
// enabled). Exercises the priority order in settler.rs:164-172.
fn test_settler_prefers_vacant_lot() {
    // TODO: scenario pending. Needs vacant lot terrain + a house with room +
    // a spawn point + settler-traversable terrain between them.
    println!("(scenario pending: settler/vacant-lot/house setup)");
}

// Settler falls back to a house when no vacant lot is available.
fn test_settler_falls_back_to_house_when_no_lot() {
    // TODO: scenario pending.
    println!("(scenario pending: settler fallback-to-house setup)");
}

// Settler returns to its spawn point (exits) when no settlement is available
// and `return_to_spawn_point_if_failed = true`.
fn test_settler_returns_to_spawn_when_no_settlement() {
    // TODO: scenario pending.
    println!("(scenario pending: settler exit-on-failure setup)");
}

// ----------------------------------------------
// UnitTaskRandomizedPatrol
// ----------------------------------------------

// Unit leaves its origin, wanders within `max_distance`, and returns.
fn test_patrol_leaves_and_returns_to_origin() {
    // TODO: scenario pending. Needs a patrol origin building (with road_link)
    // and a connected road network >= max_distance cells.
    println!("(scenario pending: patrol origin + road network setup)");
}

// Unit visits target buildings along its route when `buildings_to_visit` is set.
fn test_patrol_visits_target_buildings() {
    // TODO: scenario pending. Needs the above plus target building(s) of
    // the specified kind adjacent to the patrol route.
    println!("(scenario pending: patrol target-building setup)");
}

// No waypoint chosen should be farther than `max_distance` cells from origin.
fn test_patrol_respects_max_distance() {
    // TODO: scenario pending. Track unit's cell at each tick, assert
    // max |cell - origin| <= max_distance.
    println!("(scenario pending: patrol max_distance instrumentation)");
}
