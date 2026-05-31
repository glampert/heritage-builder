use common::{coords::Cell, Size};
use game::{
    building::BuildingKindAndId,
    debug::game_object_debug::GameObjectDebugVarRef,
    sim::{resources::ResourceKind, SimCmdQueue},
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for house resource consumption,
// occupancy scaling, deprivation downgrade and the
// worker-efficiency slowdown.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("House Consumption", &[
        test_utils::test_fn!(test_under_staffed_building_runs_at_reduced_efficiency),
        test_utils::test_fn!(test_house_consumption_scales_with_occupancy),
        test_utils::test_fn!(test_non_basic_deficiency_downgrades_house_immediately),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Current work efficiency [0,1] of an employer building, read straight from its
// Employer worker pool (independent of the ignore_worker_requirements cheat).
fn work_efficiency(env: &TestEnvironment, handle: BuildingKindAndId) -> f32 {
    test_utils::find_building(env, handle)
        .workers()
        .expect("building has a worker pool")
        .as_employer()
        .expect("building is an employer")
        .work_efficiency()
}

fn add_workers(env: &mut TestEnvironment, handle: BuildingKindAndId, count: u32, source: BuildingKindAndId) {
    let building = test_utils::find_building_mut(env, handle);
    let added = building.add_workers(count, source);
    assert_eq!(added, count, "expected to employ {count} workers");
}

// Toggle one of the house debug `freeze_*` flags via the public debug-options API.
fn set_house_freeze(env: &mut TestEnvironment, handle: BuildingKindAndId, option_name: &str, mut value: bool) {
    let building = test_utils::find_building_mut(env, handle);
    let applied = building
        .debug_options()
        .set_debug_option_by_name(option_name, GameObjectDebugVarRef::Bool(&mut value));
    assert!(applied, "debug option '{option_name}' should exist");
}

// Set a house's occupancy by routing through the real add_population path so the
// deferred worker adjustment runs too.
fn add_house_population(env: &mut TestEnvironment, handle: BuildingKindAndId, count: u32) {
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    let building = context
        .world_mut()
        .find_building_mut(handle.kind, handle.id)
        .expect("house should exist");
    building.add_population(cmds, &context, count);
    cmds.execute(&context);
}

fn give_rice(env: &mut TestEnvironment, handle: BuildingKindAndId, count: u32) -> u32 {
    let building = test_utils::find_building_mut(env, handle);
    building.receive_resources(ResourceKind::Rice, count)
}

fn rice_in_stock(env: &TestEnvironment, handle: BuildingKindAndId) -> u32 {
    let mut total = 0;
    for item in test_utils::find_building(env, handle).stock() {
        if item.kind == ResourceKind::Rice {
            total += item.count;
        }
    }
    total
}

fn house_level_name(env: &TestEnvironment, cell: Cell) -> &'static str {
    env.world
        .find_building_for_cell(cell, &env.tile_map)
        .expect("a house should occupy this cell")
        .name()
}

// ----------------------------------------------
// Worker efficiency
// ----------------------------------------------

// A producer staffed below its maximum runs at reduced efficiency, scaling
// linearly from its worker count toward the maximum. Buildings that need no
// workers (max == 0) are always fully efficient.
fn test_under_staffed_building_runs_at_reduced_efficiency() {
    let mut env = TestEnvironment::new();

    // Rice farm employs up to 4 workers (min 2).
    let farm = test_utils::spawn_building(&mut env, Cell::new(4, 4), "rice_farm");
    // A house is the only valid source of workers.
    let house = test_utils::spawn_building(&mut env, Cell::new(12, 12), "house0");

    assert_eq!(work_efficiency(&env, farm), 0.0, "no workers should be 0% efficient");

    add_workers(&mut env, farm, 2, house);
    assert!((work_efficiency(&env, farm) - 0.5).abs() < 1e-4, "2 of 4 workers should be 50% efficient");

    add_workers(&mut env, farm, 2, house);
    assert!((work_efficiency(&env, farm) - 1.0).abs() < 1e-4, "4 of 4 workers should be 100% efficient");

    // A small well needs no workers at all (min/max 0) and so is always full speed.
    let well = test_utils::spawn_building(&mut env, Cell::new(1, 1), "small_well");
    assert_eq!(work_efficiency(&env, well), 1.0, "a building needing no workers should be 100% efficient");
}

// ----------------------------------------------
// Per-resource, per-day, occupancy-scaled consumption
// ----------------------------------------------

// Houses consume food over time at a rate proportional to their occupancy: a
// fuller house burns through the same stockpile faster, and an empty house
// consumes nothing.
fn test_house_consumption_scales_with_occupancy() {
    let mut env = TestEnvironment::with_map_size(Size::new(10, 10));
    test_utils::fill_terrain(&mut env, "grass");

    // Three Level 1 houses (which require food) at increasing occupancy.
    let crowded = test_utils::spawn_building(&mut env, Cell::new(1, 1), "house1");
    let sparse  = test_utils::spawn_building(&mut env, Cell::new(4, 4), "house1");
    let empty   = test_utils::spawn_building(&mut env, Cell::new(7, 7), "house1");

    // Freeze upgrade (which would otherwise downgrade these service-less houses)
    // and population growth so occupancy stays fixed and the test is deterministic.
    for house in [crowded, sparse, empty] {
        set_house_freeze(&mut env, house, "freeze_upgrade_update", true);
        set_house_freeze(&mut env, house, "freeze_population_update", true);
    }

    const STARTING_RICE: u32 = 10;
    for house in [crowded, sparse, empty] {
        let received = give_rice(&mut env, house, STARTING_RICE);
        assert_eq!(received, STARTING_RICE, "house should accept a full stock of rice");
    }

    add_house_population(&mut env, crowded, 8);
    add_house_population(&mut env, sparse, 2);
    // `empty` stays at 0 residents.

    // Advance well past several stock-update intervals (160s each).
    test_utils::tick_n(&mut env, 50, 20.0);

    let consumed_crowded = STARTING_RICE - rice_in_stock(&env, crowded);
    let consumed_sparse = STARTING_RICE - rice_in_stock(&env, sparse);
    let consumed_empty = STARTING_RICE - rice_in_stock(&env, empty);

    assert_eq!(consumed_empty, 0, "an unoccupied house should consume nothing");
    assert!(consumed_sparse > 0, "an occupied house should consume food over time");
    assert!(
        consumed_crowded > consumed_sparse,
        "a fuller house ({consumed_crowded}) should consume more than a sparser one ({consumed_sparse})"
    );
}

// ----------------------------------------------
// Deprivation grace vs. immediate downgrade
// ----------------------------------------------

// A house missing a *non-basic* requirement (here: no Market access) still
// downgrades immediately rather than waiting out the deprivation grace window,
// which only covers basic needs (food/water).
fn test_non_basic_deficiency_downgrades_house_immediately() {
    let mut env = TestEnvironment::with_map_size(Size::new(12, 12));
    test_utils::fill_terrain(&mut env, "grass");

    let cell = Cell::new(5, 5);
    let _house = test_utils::spawn_building(&mut env, cell, "house1");
    assert_eq!(house_level_name(&env, cell), "House Level 1", "should spawn as Level 1");

    // No food, no nearby services -> the missing Market is a non-basic requirement,
    // so the house should downgrade on the next upgrade tick (every 10s) rather
    // than entering the slow deprivation/eviction path.
    test_utils::tick_n(&mut env, 6, 5.0); // ~30s

    assert_eq!(
        house_level_name(&env, cell),
        "House Level 0",
        "missing a non-basic requirement should downgrade the house immediately"
    );
}
