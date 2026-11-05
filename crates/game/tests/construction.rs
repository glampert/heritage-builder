use common::coords::Cell;
use game::{
    cheats,
    menu::TilePlacement,
    sim::SimCmdQueue,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for building costs and
// construction (docs/plans/construction.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Construction", &[
        test_utils::test_fn!(test_deferred_spawns_are_free),
        test_utils::test_fn!(test_player_placement_charges_gold),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Runs `test` with the tile cost cheat off (the harness turns it on), then turns it back on.
fn with_tile_costs<F: FnOnce()>(test: F) {
    cheats::get_mut().ignore_tile_cost = false;
    test();
    cheats::get_mut().ignore_tile_cost = true;
}

// Gold the HUD and the affordability checks see: the world stats, recounted now.
fn gold(env: &mut TestEnvironment) -> u32 {
    env.world.refresh_stats(env.sim.treasury());
    env.world.stats().treasury.gold_units_total
}

// ----------------------------------------------
// Charging
// ----------------------------------------------

// A building spawned through SimCmds, as a settler's house is, costs nothing: only the
// player's placement charges.
fn test_deferred_spawns_are_free() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);

        let well_def = test_utils::find_building_def("small_well");
        assert!(well_def.cost > 0, "the test needs a building that costs gold");

        {
            let context = env.new_sim_context(0.0);
            let cmds = context.cmds_mut();
            cmds.spawn_building_with_tile_def_cb(Cell::new(4, 4), well_def, |_, result| {
                assert!(result.is_ok(), "the deferred spawn should succeed");
            });
            cmds.execute(&context);
        }

        assert!(env.world.find_building_for_cell(Cell::new(4, 4), &env.tile_map).is_some());
        assert_eq!(gold(&mut env), gold_before, "a deferred spawn shouldn't charge");
    });
}

// The player's placement path charges the tile's gold cost.
fn test_player_placement_charges_gold() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);

        let well_def = test_utils::find_building_def("small_well");
        {
            let context = env.new_sim_context(0.0);
            assert!(TilePlacement::place(&context, Cell::new(4, 4), well_def, true, false).is_ok());
        }

        assert_eq!(gold(&mut env), gold_before - well_def.cost);
    });
}
