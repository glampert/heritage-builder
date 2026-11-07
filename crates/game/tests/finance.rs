use common::coords::Cell;
use game::{
    building::{BuildingKind, construction::ConstructionMode},
    cheats,
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{
        finance::{Finance, LedgerCategory},
        resources::ResourceKind,
    },
    undo_redo,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the city's finances
// (docs/plans/finance.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Finance", &[
        // One treasury:
        test_utils::test_fn!(test_tax_office_holds_no_gold),
        test_utils::test_fn!(test_stats_count_the_balance),
        // The ledger:
        test_utils::test_fn!(test_placement_records_construction),
        test_utils::test_fn!(test_cancelled_site_takes_its_cost_back),
        test_utils::test_fn!(test_cheats_are_not_recorded),
        // Debt:
        test_utils::test_fn!(test_debt_blocks_gold_placements),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Runs `test` with tile costs and construction sites on, as the player plays (the harness
// turns both cheats on).
fn as_player<F: FnOnce()>(test: F) {
    cheats::get_mut().ignore_tile_cost = false;
    cheats::get_mut().instant_construction = false;
    test();
    cheats::get_mut().instant_construction = true;
    cheats::get_mut().ignore_tile_cost = true;
}

fn finance(env: &TestEnvironment) -> &Finance {
    env.sim.finance()
}

fn place(env: &mut TestEnvironment, cell: Cell, name: &str, mode: ConstructionMode) -> PlaceOrClearResult {
    let tile_def = test_utils::find_building_def(name);
    let context = env.new_sim_context(0.0);
    TilePlacement::place(&context, cell, tile_def, true, true, mode)
}

fn clear(env: &mut TestEnvironment, cell: Cell) -> bool {
    let context = env.new_sim_context(0.0);
    let tile = context.find_tile(cell, game::tile::TileKind::Building).expect("a building to clear");
    TilePlacement::clear(&context, tile, false, true).is_ok()
}

// ----------------------------------------------
// One treasury
// ----------------------------------------------

// A tax office has no stock of its own: its collectors pay into the city treasury, so
// demolishing it can't lose any gold.
fn test_tax_office_holds_no_gold() {
    let mut env = TestEnvironment::new();
    let office = test_utils::spawn_building(&mut env, Cell::new(4, 4), "tax_office");
    assert!(office.kind == BuildingKind::TaxOffice);

    let building = test_utils::find_building_mut(&mut env, office);
    assert!(building.stock().is_empty(), "a tax office should hold nothing");
    assert_eq!(building.receivable_resources(ResourceKind::Gold), 0);
    assert_eq!(building.receive_resources(ResourceKind::Gold, 10), 0);
}

// The world stats' gold is the treasury's balance, debt included.
fn test_stats_count_the_balance() {
    let mut env = TestEnvironment::new();
    env.sim.finance_mut().set_balance_unrecorded(-25);
    env.world.refresh_stats(env.sim.finance());
    assert_eq!(env.world.stats().treasury.gold_units_total, -25);
}

// ----------------------------------------------
// The ledger
// ----------------------------------------------

// A player's placement charges construction in this year's accounts.
fn test_placement_records_construction() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let balance_before = finance(&env).balance();
        let cost = test_utils::find_building_def("small_well").cost;
        assert!(cost > 0, "the test needs a building that costs gold");

        assert!(place(&mut env, Cell::new(4, 4), "small_well", ConstructionMode::Finished).is_ok());

        assert_eq!(finance(&env).balance(), balance_before - i64::from(cost));
        assert_eq!(finance(&env).this_year().total(LedgerCategory::Construction), i64::from(cost));
        assert_eq!(finance(&env).this_year().spending(), i64::from(cost));
        undo_redo::clear();
    });
}

// Cancelling a construction site refunds it and takes it back off construction spending.
fn test_cancelled_site_takes_its_cost_back() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let balance_before = finance(&env).balance();
        let cell = Cell::new(6, 6);

        assert!(place(&mut env, cell, "small_well", ConstructionMode::Site).is_ok());
        assert!(finance(&env).this_year().total(LedgerCategory::Construction) > 0);
        assert!(clear(&mut env, cell));

        assert_eq!(finance(&env).balance(), balance_before);
        assert_eq!(finance(&env).this_year().total(LedgerCategory::Construction), 0);
        undo_redo::clear();
    });
}

// Cheats and debug tools change the balance, not the accounts.
fn test_cheats_are_not_recorded() {
    let mut env = TestEnvironment::new();
    let balance_before = finance(&env).balance();
    env.sim.finance_mut().add_unrecorded(500);

    assert_eq!(finance(&env).balance(), balance_before + 500);
    assert_eq!(finance(&env).this_year().income(), 0);
    assert_eq!(finance(&env).this_year().spending(), 0);
}

// ----------------------------------------------
// Debt
// ----------------------------------------------

// A treasury in debt can't pay for anything that costs gold, and says how much it lacks.
fn test_debt_blocks_gold_placements() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        env.sim.finance_mut().set_balance_unrecorded(-10);
        let cost = test_utils::find_building_def("small_well").cost;

        assert!(!finance(&env).can_afford(cost));
        assert_eq!(finance(&env).shortfall(cost), cost, "debt doesn't add to what's missing");
        assert!(matches!(place(&mut env, Cell::new(4, 4), "small_well", ConstructionMode::Finished), PlaceOrClearResult::Failed { .. }));
        assert_eq!(finance(&env).balance(), -10, "a refused placement charges nothing");

        // Back out of debt with enough for it, and it can be built.
        env.sim.finance_mut().set_balance_unrecorded(i64::from(cost));
        assert!(place(&mut env, Cell::new(4, 4), "small_well", ConstructionMode::Finished).is_ok());
        assert_eq!(finance(&env).balance(), 0);
        undo_redo::clear();
    });
}
