use common::coords::Cell;
use game::{
    building::{
        BuildingKind,
        BuildingKindAndId,
        destruction::{self, Destruction},
    },
    campaign::config::MissionGoal,
    config::GameConfigs,
    debug::game_object_debug::GameObjectDebugVarRef,
    ratings::{
        self,
        CityRatings,
        DriverDef,
        DriverKind,
        FailureReason,
        FailureSwitches,
        MAX_RATING,
        RatingConfigs,
        RatingKind,
        RealmRatingsDef,
        config::NO_FESTIVAL_DRIVER,
    },
    realm::Realm,
    sim::{
        SimCmdQueue,
        calendar::CalendarDate,
        faith::FestivalRecord,
        finance::TaxLevel,
        ratings::{DriverValue, Rating},
        resources::ResourceKind,
        risks::{CityRisks, Disaster},
        trade::TradeRequest,
    },
};
use strum::IntoEnumIterator;

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the four ratings
// (docs/plans/ratings.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Ratings", &[
        // Data:
        test_utils::test_fn!(test_every_realm_loads_without_errors),
        test_utils::test_fn!(test_validation_finds_errors),
        // State:
        test_utils::test_fn!(test_a_new_game_is_unrated_and_cant_be_lost),
        test_utils::test_fn!(test_a_rating_is_its_drivers_sum_clamped),
        test_utils::test_fn!(test_events_fade),
        test_utils::test_fn!(test_ratings_round_trip_through_a_save),
        test_utils::test_fn!(test_the_disasters_log),
        // Drivers:
        test_utils::test_fn!(test_an_empty_city_rates_low),
        test_utils::test_fn!(test_housing_levels_and_employment),
        test_utils::test_fn!(test_coverage_counts_the_residents_in_reach),
        test_utils::test_fn!(test_festivals_count_fade_and_are_missed),
        test_utils::test_fn!(test_requests_raise_and_lower_legitimacy),
        test_utils::test_fn!(test_fair_taxes_and_calm_streets),
        test_utils::test_fn!(test_disasters_lower_security),
        test_utils::test_fn!(test_floods_count_only_while_the_river_floods),
        test_utils::test_fn!(test_each_realm_names_its_own_drivers),
        test_utils::test_fn!(test_a_month_recomputes_the_ratings),
        // Goals:
        test_utils::test_fn!(test_the_goals_read_the_ratings_and_houses),
        test_utils::test_fn!(test_a_court_request_asked_again_counts_again),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn realm_def(realm: Realm) -> &'static RealmRatingsDef {
    RatingConfigs::get().for_realm(realm).unwrap_or_else(|| panic!("{realm} should have a ratings file"))
}

fn recompute(env: &mut TestEnvironment) {
    env.world.refresh_stats(env.sim.stats_sources());
    let context = env.new_sim_context(0.0);
    ratings::recompute(&context);
}

fn driver(env: &TestEnvironment, kind: RatingKind, id: &str) -> Option<i32> {
    env.sim.ratings().get(kind).drivers.iter().find(|driver| driver.id == id).map(|driver| driver.value)
}

fn now(env: &TestEnvironment) -> u32 {
    env.sim.calendar().ordinal_month()
}

// The calendar moved on `months` months, as many month starts would (without their events).
fn advance_months(env: &mut TestEnvironment, months: u32) {
    let configs = &GameConfigs::get().sim;
    let ordinal = now(env) + months;
    let date = CalendarDate { year: ordinal / 12, month: ordinal % 12 + 1, day: 1 };
    env.sim.calendar_mut().set_date(date, configs.days_per_month, configs.seconds_per_month).unwrap();
    assert_eq!(now(env), ordinal);
}

// A house with `population` residents that holds its level, people and stock.
fn house_with(env: &mut TestEnvironment, cell: Cell, tile: &str, population: u32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, cell, tile);
    for name in ["freeze_upgrade_update", "freeze_population_update", "freeze_stock_update", "freeze_deprivation"] {
        let mut value = true;
        let ok = test_utils::find_building_mut(env, house)
            .debug_options()
            .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
        assert!(ok, "houses have a '{name}' debug option");
    }
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    let added = context.find_building_mut(house.kind, house.id).unwrap().add_population(cmds, &context, population);
    cmds.execute(&context);
    assert_eq!(added, population, "the house should take {population}");
    house
}

fn def(id: &str, driver: DriverKind, max: i32, min: i32) -> DriverDef {
    DriverDef { id: id.into(), driver, max, min }
}

// A file with one valid driver per rating.
fn minimal_def() -> RealmRatingsDef {
    RealmRatingsDef {
        prosperity: vec![def("housing", DriverKind::HousingLevels {}, 40, 0)],
        culture: vec![def("temples", DriverKind::FaithCoverage {}, 35, 0)],
        security: vec![def("safety", DriverKind::RiskSafety {}, 30, 0)],
        legitimacy: vec![def("favour", DriverKind::Favour {}, 20, 0)],
    }
}

// ----------------------------------------------
// Data
// ----------------------------------------------

fn test_every_realm_loads_without_errors() {
    let configs = RatingConfigs::get();
    assert_eq!(configs.validate(), Vec::<String>::new());
    assert_eq!(configs.missing_text_keys(), Vec::<String>::new());
    for realm in Realm::iter() {
        let def = realm_def(realm);
        for kind in RatingKind::iter() {
            assert!(!def.drivers(kind).is_empty(), "{realm}'s {kind} has drivers");
            let caps: i32 = def.drivers(kind).iter().map(|driver| driver.max).sum();
            assert_eq!(caps, MAX_RATING as i32, "{realm}'s {kind}: a city that has everything rates 100");
        }
    }
    // Only Đại Long's Mandate counts the river's floods (GDD §6: flood control).
    let floods = |realm| realm_def(realm).legitimacy.iter().any(|driver| matches!(driver.driver, DriverKind::Floods { .. }));
    assert!(floods(Realm::DaiLong));
    assert!(!floods(Realm::Kambara) && !floods(Realm::Tampadesa));
}

fn test_validation_finds_errors() {
    assert_eq!(minimal_def().validate(), Vec::<String>::new());

    let mut bad = minimal_def();
    bad.culture.clear();
    bad.prosperity.push(def("housing", DriverKind::HousingLevels {}, 70, 0)); // Twice, and caps 110.
    bad.security.push(def("Bad-Id", DriverKind::ServiceCoverage { service: BuildingKind::House }, 10, 0));
    bad.legitimacy.push(def("taxes", DriverKind::FairTaxes { by_level: [0; 5] }, -1, 3));
    bad.legitimacy.push(def("favour_again", DriverKind::Favour {}, 10, -5)); // Favour can't go negative.
    bad.prosperity.push(def("employment", DriverKind::Employment { free_share: 1.0 }, 0, 0));
    let problems = bad.validate();
    let has = |text: &str| problems.iter().any(|problem| problem.contains(text));
    assert!(has("culture has no drivers"), "{problems:?}");
    assert!(has("'housing' is listed twice"), "{problems:?}");
    assert!(has("reads the same (housing_levels)"), "{problems:?}");
    assert!(has("caps add up to 110"), "{problems:?}");
    assert!(has("'Bad-Id': its id must be"), "{problems:?}");
    assert!(has("one service building kind"), "{problems:?}");
    assert!(has("its max must be 0 or more"), "{problems:?}");
    assert!(has("its min must be 0 or less"), "{problems:?}");
    assert!(has("reads the same (favour)"), "{problems:?}");
    assert!(has("a favour driver can't go below 0"), "{problems:?}");
    assert!(has("free_share must be from 0 up to 1"), "{problems:?}");

    // Two coverage drivers of different services are two drivers.
    let mut two = minimal_def();
    two.culture.push(def("theatre", DriverKind::ServiceCoverage { service: BuildingKind::Theater }, 25, 0));
    two.culture.push(def("schools", DriverKind::ServiceCoverage { service: BuildingKind::University }, 15, 0));
    assert_eq!(two.validate(), Vec::<String>::new());

    // The data spells drivers by kind, with their parameters.
    let parsed: DriverDef = serde_json::from_str(
        r#"{"id": "theatre", "driver": {"kind": "service_coverage", "service": "Theater"}, "max": 25}"#,
    )
    .unwrap();
    assert!(parsed.driver == DriverKind::ServiceCoverage { service: BuildingKind::Theater });
    for driver in [r#"{"kind": "favour", "extra": 1}"#, r#"{"kind": "treasury", "per_resident": 1, "extra": 1}"#] {
        let json = format!(r#"{{"id": "x", "driver": {driver}, "max": 1}}"#);
        assert!(serde_json::from_str::<DriverDef>(&json).is_err(), "unknown parameters are refused: {driver}");
    }
}

// ----------------------------------------------
// State
// ----------------------------------------------

fn test_a_new_game_is_unrated_and_cant_be_lost() {
    let ratings = CityRatings::new();
    assert!(!ratings.is_rated());
    for kind in RatingKind::iter() {
        assert_eq!(ratings.value(kind), 0);
        assert!(ratings.get(kind).drivers.is_empty());
    }
    assert_eq!(ratings.fails_on(), FailureSwitches::OFF);
    assert_eq!(ratings.failure(), None);

    let mut ratings = CityRatings::new();
    ratings.set_rated_month(3);
    ratings.set_failure(Some(FailureReason::Debt));
    ratings.spoil("no_debt");
    ratings.start(FailureSwitches::ON);
    assert!(!ratings.is_rated(), "a new city starts unrated");
    assert_eq!(ratings.failure(), None);
    assert!(ratings.spoiled().is_empty());
    assert_eq!(ratings.fails_on(), FailureSwitches::ON);

    ratings.set_value(RatingKind::Culture, 250);
    assert_eq!(ratings.value(RatingKind::Culture), MAX_RATING, "held to 100");
}

fn test_a_rating_is_its_drivers_sum_clamped() {
    let value = |values: &[i32]| {
        let drivers = values.iter().map(|value| DriverValue { id: "x".into(), value: *value }).collect();
        Rating::from_drivers(drivers).value
    };
    assert_eq!(value(&[12, 8, 9, 5]), 34, "the mockup's Prosperity adds up");
    assert_eq!(value(&[60, 60]), 100);
    assert_eq!(value(&[10, -30]), 0);
    assert_eq!(value(&[]), 0);
}

fn test_events_fade() {
    assert_eq!(ratings::fade(0, 12), 1.0, "in full in its month");
    assert_eq!(ratings::fade(6, 12), 0.5);
    assert_eq!(ratings::fade(12, 12), 0.0, "gone after its months");
    assert_eq!(ratings::fade(30, 12), 0.0);
    assert_eq!(ratings::fade(0, 0), 0.0);
}

fn test_ratings_round_trip_through_a_save() {
    let mut ratings = CityRatings::new();
    ratings.start(FailureSwitches { debt: true, legitimacy: false });
    ratings.set(
        RatingKind::Culture,
        Rating::from_drivers(vec![DriverValue { id: "temples".into(), value: 11 }, DriverValue {
            id: NO_FESTIVAL_DRIVER.into(),
            value: -2,
        }]),
    );
    ratings.set_rated_month(25);
    ratings.set_legitimacy_low_months(4);
    ratings.spoil("never_breached");

    let saved = serde_json::to_value(&ratings).unwrap();
    let mut keys: Vec<&str> = saved.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["fails_on", "failure", "legitimacy_low_months", "rated_month", "ratings", "spoiled"]);
    assert_eq!(saved["ratings"]["culture"]["value"], 9, "saved by rating name");
    assert_eq!(saved["ratings"]["culture"]["drivers"][1]["id"], NO_FESTIVAL_DRIVER);

    let loaded: CityRatings = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded.value(RatingKind::Culture), 9);
    assert_eq!(loaded.rated_month(), Some(25));
    assert_eq!(loaded.fails_on(), FailureSwitches { debt: true, legitimacy: false });
    assert_eq!(loaded.legitimacy_low_months(), 4);
    assert!(loaded.is_spoiled("never_breached"));

    // A save from before ratings (v16, migrated) is unrated.
    let migrated = serde_json::json!({
        "ratings": {
            "prosperity": {"value": 0, "drivers": []}, "culture": {"value": 0, "drivers": []},
            "security": {"value": 0, "drivers": []}, "legitimacy": {"value": 0, "drivers": []}
        },
        "rated_month": null,
        "fails_on": {"debt": true, "legitimacy": true},
        "legitimacy_low_months": 0,
        "failure": null,
        "spoiled": []
    });
    let loaded: CityRatings = serde_json::from_value(migrated).unwrap();
    assert!(!loaded.is_rated());
}

fn test_the_disasters_log() {
    let mut risks = CityRisks::new(&GameConfigs::get().sim.risks);
    risks.record_disaster(Disaster::BurntDown { house: true }, 10);
    risks.record_disaster(Disaster::Collapsed { house: false }, 10);
    risks.add_disease_deaths(3, 10);
    risks.add_disease_deaths(0, 11); // Nothing lost: no month kept.
    risks.record_disaster(Disaster::Collapsed { house: true }, 12);
    let months = risks.disasters();
    assert_eq!(months.len(), 2);
    assert_eq!((months[0].ordinal_month, months[0].burnt, months[0].collapsed), (10, 1, 1));
    assert_eq!((months[0].houses_lost, months[0].deaths, months[0].buildings_lost()), (1, 3, 2));
    assert_eq!((months[1].ordinal_month, months[1].collapsed, months[1].houses_lost), (12, 1, 1));
    assert_eq!(risks.disease_deaths(), 3);

    let saved = serde_json::to_value(&risks).unwrap();
    assert_eq!(saved["disasters"][0]["burnt"], 1);
    let loaded: CityRisks = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded.disasters(), risks.disasters());

    // A building destroyed by fire or collapse is logged in its month.
    let mut env = TestEnvironment::new();
    let yard = test_utils::spawn_building(&mut env, Cell::new(10, 10), "storage_yard");
    let month = now(&env);
    {
        let context = env.new_sim_context(0.0);
        assert!(destruction::destroy_building(&context, yard, Destruction::Collapsed));
    }
    let months = env.sim.risks().disasters();
    assert_eq!(months.len(), 1);
    assert_eq!((months[0].ordinal_month, months[0].collapsed, months[0].houses_lost), (month, 1, 0));
}

// ----------------------------------------------
// Drivers
// ----------------------------------------------

fn test_an_empty_city_rates_low() {
    let mut env = TestEnvironment::new();
    env.sim.finance_mut().set_balance_unrecorded(0);
    recompute(&mut env);
    let ratings = env.sim.ratings();
    assert!(ratings.is_rated());
    assert_eq!(ratings.rated_month(), Some(now(&env)));
    for kind in [RatingKind::Prosperity, RatingKind::Culture, RatingKind::Security] {
        assert_eq!(ratings.value(kind), 0, "{kind}: nothing to rate");
    }
    // Normal taxes are fair, and the patrons start at half favour.
    assert_eq!(driver(&env, RatingKind::Legitimacy, "fair_taxes"), Some(10));
    assert_eq!(driver(&env, RatingKind::Legitimacy, "favour"), Some(10));
    assert_eq!(env.sim.ratings().value(RatingKind::Legitimacy), 20);
    // Drivers the game doesn't offer are left out: no trade, no requests, no floods.
    assert_eq!(driver(&env, RatingKind::Prosperity, "trade_income"), None);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), None);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "floods"), None);
    // Offered drivers are listed in the file's order, zero or not.
    let ids: Vec<&str> =
        env.sim.ratings().get(RatingKind::Culture).drivers.iter().map(|driver| driver.id.as_str()).collect();
    assert_eq!(ids, ["temples", "theatre", "schools", "festivals"]);
}

fn test_housing_levels_and_employment() {
    let mut env = TestEnvironment::new();
    let house = house_with(&mut env, Cell::new(5, 5), "house2", 6);
    recompute(&mut env);
    let level = test_utils::find_building(&env, house).as_house().level() as usize;
    let top = (game::building::HouseLevel::count() - 1) as f32;
    let expected = (40.0 * level as f32 / top).round() as i32;
    assert_eq!(driver(&env, RatingKind::Prosperity, "housing"), Some(expected), "level {level} of {top}");

    // A treasury of 10 gold a resident is full marks; a debt of that much the floor.
    env.sim.finance_mut().set_balance_unrecorded(60);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Prosperity, "treasury"), Some(15));
    env.sim.finance_mut().set_balance_unrecorded(30);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Prosperity, "treasury"), Some(8));
    env.sim.finance_mut().set_balance_unrecorded(-60);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Prosperity, "treasury"), Some(-15));

    // No one works yet: employment and wages count nothing.
    assert_eq!(driver(&env, RatingKind::Prosperity, "employment"), Some(0));
    assert_eq!(driver(&env, RatingKind::Prosperity, "wages"), Some(0));
}

fn test_coverage_counts_the_residents_in_reach() {
    let mut env = TestEnvironment::new();
    // A shrine (reach 10) beside the first house (6 residents) and far from the second (12).
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    house_with(&mut env, Cell::new(30, 5), "house2", 12);
    test_utils::spawn_building(&mut env, Cell::new(9, 5), "shrine");
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "temples"), Some((35.0_f32 * 6.0 / 18.0).round() as i32));
    assert_eq!(driver(&env, RatingKind::Culture, "theatre"), Some(0));

    // A theater (reach 40) takes both.
    test_utils::spawn_building(&mut env, Cell::new(15, 12), "theater");
    test_utils::spawn_building(&mut env, Cell::new(15, 20), "police_station");
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "theatre"), Some(25));
    assert_eq!(driver(&env, RatingKind::Security, "police"), Some(35));
    assert_eq!(driver(&env, RatingKind::Culture, "schools"), Some(0));
}

fn test_festivals_count_fade_and_are_missed() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    let held = now(&env);
    env.sim.faith_mut().record_festival(FestivalRecord {
        festival: "floodwater_offering".into(),
        scale: "grand".into(),
        ordinal_month: held,
        culture: 6.0,
        legitimacy: 3.0,
    });
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "festivals"), Some(6), "in full in its month");
    assert_eq!(driver(&env, RatingKind::Legitimacy, "festivals"), Some(3));

    advance_months(&mut env, 6);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "festivals"), Some(3), "half gone after half a year");

    // A year without one: "No festival this year".
    advance_months(&mut env, 6);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "festivals"), None);
    assert_eq!(driver(&env, RatingKind::Culture, NO_FESTIVAL_DRIVER), Some(-2));
    assert_eq!(driver(&env, RatingKind::Legitimacy, "festivals"), Some(0));

    // Not while festivals are off for the mission.
    env.sim.faith_mut().set_festivals_enabled(false);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Culture, "festivals"), Some(0));
}

fn request(id: &str, legitimacy: i32, opened: u32, due: u32) -> TradeRequest {
    TradeRequest {
        id: id.into(),
        from: "hoa_thanh".into(),
        good: ResourceKind::Pottery,
        amount: 30,
        opened,
        due,
        legitimacy,
        relation: 0,
        again: 0,
    }
}

fn test_requests_raise_and_lower_legitimacy() {
    let mut env = TestEnvironment::new();
    let start = now(&env);
    assert!(env.sim.trade_mut().open_request(request("pottery", 8, start, start + 6), start));
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), Some(0), "offered once the game has requests");

    assert!(env.sim.trade_mut().meet_request("pottery", start).is_some());
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), Some(8), "met: its legitimacy");

    // A failed one takes away 1.5 times its legitimacy.
    assert!(env.sim.trade_mut().open_request(request("bricks", 10, start, start + 2), start));
    advance_months(&mut env, 2);
    let month = now(&env);
    let (failed, _) = env.sim.trade_mut().on_new_month(month);
    assert_eq!(failed.len(), 1);
    recompute(&mut env);
    let met = 8.0 * ratings::fade(2, 24);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), Some((met - 15.0_f32).round() as i32));

    // Remembered for two years.
    advance_months(&mut env, 24);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), Some(0));
}

fn test_fair_taxes_and_calm_streets() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    let by_level = [16, 14, 10, -5, -15];
    for level in TaxLevel::iter() {
        env.sim.finance_mut().set_tax_level(level);
        recompute(&mut env);
        assert_eq!(driver(&env, RatingKind::Legitimacy, "fair_taxes"), Some(by_level[level.index()]), "{level}");
    }

    // A house without food (the test's houses hold no stock) makes the streets restless.
    env.sim.finance_mut().set_tax_level(TaxLevel::Normal);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Security, "calm_streets"), Some(0), "every resident hungry");
}

fn test_disasters_lower_security() {
    let mut env = TestEnvironment::new();
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Security, "disasters"), Some(0));

    let month = now(&env);
    env.sim.risks_mut().record_disaster(Disaster::BurntDown { house: true }, month);
    env.sim.risks_mut().record_disaster(Disaster::Collapsed { house: false }, month);
    env.sim.risks_mut().add_disease_deaths(2, month);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Security, "disasters"), Some(-(6 + 4 + 2)));

    advance_months(&mut env, 6);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Security, "disasters"), Some(-6), "half forgotten");

    // Never below its floor.
    let month = now(&env);
    for _ in 0..20 {
        env.sim.risks_mut().record_disaster(Disaster::BurntDown { house: true }, month);
    }
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Security, "disasters"), Some(-30));
}

fn test_floods_count_only_while_the_river_floods() {
    let mut env = TestEnvironment::new();
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "floods"), None, "a river that doesn't flood");

    let rules = &GameConfigs::get().sim.floods;
    env.sim.floods_mut().set_enabled(true, rules, &env.tile_map);
    recompute(&mut env);
    assert_eq!(driver(&env, RatingKind::Legitimacy, "floods"), Some(0), "no flood yet");
}

fn test_each_realm_names_its_own_drivers() {
    assert_eq!(ratings::driver_name(Realm::DaiLong, "theatre"), "Theatre");
    assert_eq!(ratings::driver_name(Realm::Kambara, "theatre"), "Dance halls");
    assert_eq!(ratings::driver_name(Realm::Tampadesa, "theatre"), "Pwè theatre");
    assert_eq!(ratings::driver_name(Realm::Tampadesa, "housing"), "Housing levels", "the general name");
    assert_eq!(ratings::rating_name(Realm::DaiLong, RatingKind::Legitimacy), "The Mandate");
    assert_eq!(ratings::rating_name(Realm::Tampadesa, RatingKind::Legitimacy), "Merit");
    assert_eq!(ratings::rating_name(Realm::Kambara, RatingKind::Culture), "Culture");
}

fn test_a_month_recomputes_the_ratings() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    test_utils::spawn_building(&mut env, Cell::new(9, 5), "shrine");
    env.world.refresh_stats(env.sim.stats_sources());
    assert!(!env.sim.ratings().is_rated());
    let date = env.sim.calendar().date(30, 60.0);
    {
        let context = env.new_sim_context(0.0);
        ratings::on_new_month(&context, date);
    }
    assert!(env.sim.ratings().is_rated());
    assert_eq!(driver(&env, RatingKind::Culture, "temples"), Some(35));
}

// ----------------------------------------------
// Goals (Phase 2)
// ----------------------------------------------

// The world stats count the ratings (as recomputed, or set by hand until the next recompute) and the
// finished houses by level, for the Rating and HousesAtLevel goals.
fn test_the_goals_read_the_ratings_and_houses() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house0", 2);
    house_with(&mut env, Cell::new(9, 5), "house2", 4);
    recompute(&mut env);
    let culture = env.sim.ratings().value(RatingKind::Culture);
    let stats = env.world.stats();
    assert_eq!(stats.ratings.value(RatingKind::Culture), culture);
    assert_eq!(stats.houses.at_level[0], 1);
    assert_eq!(stats.houses.at_level[2], 1);
    assert_eq!(stats.houses.at_or_above(1), 2);
    assert_eq!(stats.houses.at_or_above(3), 1, "house2 is the player's level 3");
    assert!(MissionGoal::HousesAtLevel { level: 3, count: 1 }.is_met(stats));
    assert!(!MissionGoal::HousesAtLevel { level: 4, count: 1 }.is_met(stats));

    env.sim.ratings_mut().set_value(RatingKind::Culture, 55);
    env.world.refresh_stats(env.sim.stats_sources());
    assert!(MissionGoal::Rating { rating: RatingKind::Culture, min: 55 }.is_met(env.world.stats()));
    assert!(!MissionGoal::Rating { rating: RatingKind::Culture, min: 56 }.is_met(env.world.stats()));

    // A world step tallies them too.
    test_utils::tick(&mut env, 0.5);
    assert_eq!(env.world.stats().ratings.value(RatingKind::Culture), 55);
}

// A court request asked again after failing (trade::requests::ask_again): for more, later, with as
// many months to send it; it's logged like any other, so failing it too takes away its Legitimacy
// again.
fn test_a_court_request_asked_again_counts_again() {
    let mut env = TestEnvironment::new();
    let start = now(&env);
    let first = request("court_pottery", 6, start, start + 2);
    let again = first.asked_again(start + 2, 3, 1.25);
    assert_eq!(again.id, "court_pottery_again_1");
    assert_eq!((again.amount, again.opened, again.due, again.legitimacy, again.again), (38, start + 5, start + 7, 6, 1));
    let third = again.asked_again(start + 7, 3, 1.25);
    assert_eq!((third.id.as_str(), third.amount, third.again), ("court_pottery_again_2", 48, 2));
    assert_eq!(third.first_id(), "court_pottery");

    // The first fails; the court asks again three months later.
    assert!(env.sim.trade_mut().open_request(first, start));
    advance_months(&mut env, 2);
    assert_eq!(env.sim.trade_mut().on_new_month(start + 2).0.len(), 1);
    assert!(env.sim.trade_mut().schedule(again.clone()));
    assert!(!env.sim.trade_mut().schedule(again), "known already");

    advance_months(&mut env, 3);
    let (failed, opened) = env.sim.trade_mut().on_new_month(start + 5);
    assert!(failed.is_empty());
    assert_eq!(opened.iter().map(|request| request.id.as_str()).collect::<Vec<_>>(), ["court_pottery_again_1"]);

    advance_months(&mut env, 2);
    assert_eq!(env.sim.trade_mut().on_new_month(start + 7).0.len(), 1, "unmet again");
    recompute(&mut env);
    let lost = -6.0 * 1.5 * (ratings::fade(5, 24) + ratings::fade(0, 24));
    assert_eq!(driver(&env, RatingKind::Legitimacy, "requests"), Some(lost.round() as i32));
    assert_eq!(env.sim.trade().requests_failed(), 2);
}
