use common::coords::Cell;
use game::{
    campaign::config::MissionFinanceDef,
    building::{BuildingKind, construction::ConstructionMode},
    cheats,
    menu::{PlaceOrClearResult, TilePlacement},
    config::GameConfigs,
    sim::{
        calendar::{CalendarDate, CalendarEvent},
        finance::{self, Finance, LedgerCategory, TaxLevel},
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
        // Wages and upkeep:
        test_utils::test_fn!(test_month_end_pays_wages_and_upkeep),
        test_utils::test_fn!(test_sites_pay_no_upkeep),
        test_utils::test_fn!(test_months_charge_in_their_year),
        // Debt:
        test_utils::test_fn!(test_debt_blocks_gold_placements),
        test_utils::test_fn!(test_wages_run_the_city_into_debt),
        // Missions:
        test_utils::test_fn!(test_mission_start_sets_the_finances),
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

// The calendar events of a world step that crosses into `month` (and into a new year in January).
fn month_events(month: u32) -> Vec<CalendarEvent> {
    let mut events = vec![CalendarEvent::NewMonth(CalendarDate { year: 2, month, day: 1 })];
    if month == 1 {
        events.push(CalendarEvent::NewYear(2));
    }
    events
}

// Runs the month-end charges as a world step does, with `employed` workers in the stats.
fn end_month(env: &mut TestEnvironment, month: u32, employed: u32) {
    env.world.refresh_stats(env.sim.finance());
    env.world.stats_mut().population.employed = employed;
    let (world, tile_map) = (&env.world, &env.tile_map);
    env.sim.finance_mut().on_calendar_events(&month_events(month), world, tile_map);
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
// Wages and upkeep
// ----------------------------------------------

// A month's end charges a twelfth of the yearly wages and upkeep, recorded by category.
fn test_month_end_pays_wages_and_upkeep() {
    let mut env = TestEnvironment::new();
    test_utils::spawn_building(&mut env, Cell::new(4, 4), "tax_office");
    let upkeep = test_utils::find_building_def("tax_office").upkeep;
    assert!(upkeep > 0, "the tax office should have an upkeep");
    assert_eq!(finance::yearly_upkeep(&env.world, &env.tile_map), u64::from(upkeep));

    env.sim.finance_mut().set_wage(6);
    env.sim.finance_mut().set_balance_unrecorded(1_000);

    // 24 workers at 6 a year: 144 a year, 12 a month.
    for month in 2..=4 {
        end_month(&mut env, month, 24);
    }

    let accounts = finance(&env).this_year();
    assert_eq!(accounts.total(LedgerCategory::Wages), 36);
    assert_eq!(accounts.total(LedgerCategory::Upkeep), i64::from(upkeep * 3 / 12));
    assert_eq!(finance(&env).balance(), 1_000 - accounts.spending());
}

// A construction site costs no upkeep until it's finished.
fn test_sites_pay_no_upkeep() {
    cheats::get_mut().instant_construction = false;
    let mut env = TestEnvironment::new();

    let cell = Cell::new(6, 6);
    assert!(place(&mut env, cell, "tax_office", ConstructionMode::Site).is_ok());
    let site = env.world.find_building_for_cell(cell, &env.tile_map).expect("a site");
    assert!(site.is_under_construction());
    assert_eq!(finance::yearly_upkeep(&env.world, &env.tile_map), 0);

    test_utils::spawn_building(&mut env, Cell::new(12, 12), "tax_office");
    assert_eq!(finance::yearly_upkeep(&env.world, &env.tile_map), u64::from(test_utils::find_building_def("tax_office").upkeep));

    cheats::get_mut().instant_construction = true;
    undo_redo::clear();
}

// December's charges count in the year that ends, before January starts the new accounts.
fn test_months_charge_in_their_year() {
    let mut env = TestEnvironment::new();
    env.sim.finance_mut().set_wage(12);
    env.sim.finance_mut().set_balance_unrecorded(500);

    // 10 workers at 12 a year cost 10 a month.
    end_month(&mut env, 12, 10);
    end_month(&mut env, 1, 10);

    assert_eq!(finance(&env).last_year().total(LedgerCategory::Wages), 20);
    assert_eq!(finance(&env).this_year().total(LedgerCategory::Wages), 0);
    assert_eq!(finance(&env).this_year().start_balance(), 480);
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

// Wages keep being paid with an empty treasury: the balance goes negative, the debt clock
// counts the months, and the debt is overdue at the limit. Income brings it back.
fn test_wages_run_the_city_into_debt() {
    let mut env = TestEnvironment::new();
    let limit = GameConfigs::get().sim.finance.debt_limit_months;
    assert!(limit > 1, "the test needs a debt limit of a few months");
    env.sim.finance_mut().set_wage(12);
    env.sim.finance_mut().set_balance_unrecorded(0);
    let _ = env.sim.finance_mut().take_notices();

    // 10 workers at 12 a year cost 10 a month.
    for month in 2..2 + limit {
        end_month(&mut env, month % 12 + 1, 10);
    }
    assert_eq!(finance(&env).balance(), -10 * i64::from(limit));
    assert_eq!(finance(&env).months_in_debt(), limit);
    assert!(finance(&env).is_debt_overdue(&GameConfigs::get().sim.finance));

    let notices = env.sim.finance_mut().take_notices();
    assert_eq!(notices.first().map(String::as_str), Some("The treasury is in debt: -10 gold."));
    assert!(notices.last().is_some_and(|notice| notice.contains("overdue")), "{notices:?}");

    env.sim.finance_mut().earn(LedgerCategory::Taxes, 1_000);
    end_month(&mut env, 6, 10);
    assert_eq!(finance(&env).months_in_debt(), 0);
    assert_eq!(env.sim.finance_mut().take_notices(), vec!["The treasury is out of debt.".to_string()]);
}

// ----------------------------------------------
// Missions
// ----------------------------------------------

// A mission's finance block starts a new treasury: its gold, tax level and wage, fresh accounts
// and no debt. A block without starting gold keeps the map's balance.
fn test_mission_start_sets_the_finances() {
    let configs = &GameConfigs::get().sim.finance;
    let mut env = TestEnvironment::new();
    env.sim.finance_mut().set_balance_unrecorded(-40);
    env.sim.finance_mut().spend(LedgerCategory::Wages, 10);
    end_month(&mut env, 3, 0);
    assert!(finance(&env).months_in_debt() > 0);

    let mission: MissionFinanceDef =
        serde_json::from_str(r#"{ "starting_gold": 800, "tax_level": "Low", "wage": 4 }"#).expect("a valid finance block");
    env.sim.finance_mut().start_mission(&mission, configs);

    assert_eq!(finance(&env).balance(), 800);
    assert!(finance(&env).tax_level() == TaxLevel::Low);
    assert_eq!(finance(&env).wage(), 4);
    assert_eq!(finance(&env).months_in_debt(), 0);
    assert_eq!(finance(&env).this_year().spending(), 0);
    assert_eq!(finance(&env).this_year().start_balance(), 800);

    // Only a tax level: the balance stays, the wage is the default.
    let mission: MissionFinanceDef = serde_json::from_str(r#"{ "tax_level": "High" }"#).unwrap();
    env.sim.finance_mut().start_mission(&mission, configs);
    assert_eq!(finance(&env).balance(), 800);
    assert_eq!(finance(&env).wage(), configs.default_wage);

    // A misspelt field is refused rather than ignored.
    assert!(serde_json::from_str::<MissionFinanceDef>(r#"{ "starting_golds": 1 }"#).is_err());
}
