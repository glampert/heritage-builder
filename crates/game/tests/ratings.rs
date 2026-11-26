use common::coords::Cell;
use game::{
    building::{
        BuildingKind,
        BuildingKindAndId,
        destruction::{self, Destruction},
    },
    campaign::{
        self,
        CampaignPrompt,
        config::{DistinctionCondition, MissionDistinctionDef, MissionGoal, MissionRequirements},
    },
    config::GameConfigs,
    custom_game::{self, ActiveCustomGame, ChosenMap, CustomGameSettings, Difficulty, GoalSetDef, MapConfigs},
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
        distinctions::{self, DistinctionState},
        failure::{self, FailureNotice, MissionTerms},
    },
    realm::Realm,
    sim::{
        SimCmdQueue,
        calendar::CalendarDate,
        faith::FestivalRecord,
        finance::TaxLevel,
        floods::FloodRecord,
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
        // Failure:
        test_utils::test_fn!(test_debt_loses_the_mission),
        test_utils::test_fn!(test_a_year_of_low_legitimacy_recalls_the_governor),
        test_utils::test_fn!(test_a_time_limit_runs_out),
        test_utils::test_fn!(test_each_realm_words_its_failures),
        // Distinctions:
        test_utils::test_fn!(test_each_distinction_condition),
        test_utils::test_fn!(test_distinctions_spoil_month_by_month),
        // Custom games:
        test_utils::test_fn!(test_the_shipped_goal_sets_load),
        test_utils::test_fn!(test_a_custom_games_goals_and_failure),
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

// ----------------------------------------------
// Failure (Phase 3)
// ----------------------------------------------

fn terms(time_limit_months: Option<u32>) -> MissionTerms {
    MissionTerms { completed: false, time_limit_months, goals_met: false }
}

fn check(env: &mut TestEnvironment, terms: Option<MissionTerms>) -> Option<FailureReason> {
    let context = env.new_sim_context(0.0);
    failure::check_failure(&context, terms.as_ref())
}

// A mission that can be lost both ways, its Legitimacy well above the mark.
fn mission_env() -> TestEnvironment {
    let mut env = TestEnvironment::new();
    env.sim.ratings_mut().start(FailureSwitches::ON);
    env.sim.ratings_mut().set_value(RatingKind::Legitimacy, 50);
    failure::take_notices();
    env
}

// The debt running out its months loses the mission, unless the mission is finished, its debt's
// failure is off, or it's outside a mission; once lost, nothing more happens.
fn test_debt_loses_the_mission() {
    let mut env = mission_env();
    let limit = GameConfigs::get().sim.finance.debt_limit_months;
    env.sim.finance_mut().set_balance_unrecorded(-100);
    env.sim.finance_mut().set_months_in_debt(limit - 1);
    assert_eq!(check(&mut env, Some(terms(None))), None, "a month to go");

    env.sim.finance_mut().set_months_in_debt(limit);
    assert_eq!(check(&mut env, None), None, "outside a mission");
    assert_eq!(check(&mut env, Some(MissionTerms { completed: true, ..terms(None) })), None, "finished");
    env.sim.ratings_mut().set_fails_on(FailureSwitches { debt: false, legitimacy: true });
    assert_eq!(check(&mut env, Some(terms(None))), None, "debt's failure off");
    assert_eq!(env.sim.ratings().failure(), None);

    env.sim.ratings_mut().set_fails_on(FailureSwitches::ON);
    assert_eq!(check(&mut env, Some(terms(None))), Some(FailureReason::Debt));
    assert_eq!(env.sim.ratings().failure(), Some(FailureReason::Debt));
    assert_eq!(check(&mut env, Some(terms(None))), None, "lost once");
    assert!(failure::take_notices().is_empty(), "the debt's warnings are Finance's");
}

// Legitimacy below the mark at recall_months month-ends in a row recalls the governor; a month
// above it stops the clock. Warned of when it starts and in its last three months.
fn test_a_year_of_low_legitimacy_recalls_the_governor() {
    let mut env = mission_env();
    let rules = &GameConfigs::get().sim.ratings;
    let (below, months) = (rules.recall_below, rules.recall_months);
    let low_months = |env: &TestEnvironment| env.sim.ratings().legitimacy_low_months();

    env.sim.ratings_mut().set_value(RatingKind::Legitimacy, below - 1);
    assert_eq!(check(&mut env, Some(terms(None))), None);
    assert_eq!(low_months(&env), 1);
    assert_eq!(failure::take_notices(), [FailureNotice::LegitimacyLow { months_left: months - 1, started: true }]);

    // A month at the mark stops the clock.
    env.sim.ratings_mut().set_value(RatingKind::Legitimacy, below);
    assert_eq!(check(&mut env, Some(terms(None))), None);
    assert_eq!(low_months(&env), 0);

    env.sim.ratings_mut().set_value(RatingKind::Legitimacy, below - 1);
    for _ in 1..months - 3 {
        assert_eq!(check(&mut env, Some(terms(None))), None);
    }
    assert_eq!(failure::take_notices().len(), 1, "only as it starts");
    for _ in 0..3 {
        assert_eq!(check(&mut env, Some(terms(None))), None);
    }
    assert_eq!(
        failure::take_notices(),
        [3, 2, 1].map(|left| FailureNotice::LegitimacyLow { months_left: left, started: false })
    );
    assert_eq!(check(&mut env, Some(terms(None))), Some(FailureReason::Legitimacy));
    assert_eq!(low_months(&env), months);

    // Without Legitimacy's failure the clock doesn't run.
    env.sim.ratings_mut().start(FailureSwitches { debt: true, legitimacy: false });
    for _ in 0..months + 1 {
        assert_eq!(check(&mut env, Some(terms(None))), None);
    }
    assert_eq!(low_months(&env), 0);
    assert!(failure::take_notices().is_empty());

    // The warning's words, the realm's name for the rating.
    let notice = FailureNotice::LegitimacyLow { months_left: 11, started: true };
    let (message, _) = failure::notice_message(&notice, Realm::DaiLong);
    assert_eq!(message.kind, game::messages::MessageKind::Warning);
    assert!(message.target == Some(game::messages::MessageTarget::CityPage("ratings")));
    assert_eq!(message.text, "The Mandate has fallen below 10. If it stays there for 11 months, the mission is lost.");
}

// The goals unmet when the time limit's months have passed lose the mission; met, it isn't lost.
// Warned of with 12, 6, 3 and 1 months left.
fn test_a_time_limit_runs_out() {
    let mut env = mission_env();
    let limit = env.sim.calendar().months_elapsed() + 12;
    let mut warned = Vec::new();
    for _ in 0..12 {
        assert_eq!(check(&mut env, Some(terms(Some(limit)))), None);
        warned.extend(failure::take_notices());
        advance_months(&mut env, 1);
    }
    let expected: Vec<FailureNotice> =
        failure::TIME_WARNINGS.iter().map(|&months_left| FailureNotice::TimeLeft { months_left }).collect();
    assert_eq!(warned, expected);
    let text = failure::notice_message(&expected[3], Realm::DaiLong).0.text;
    assert_eq!(text, "1 month left to meet the mission's goals.");

    assert_eq!(check(&mut env, Some(MissionTerms { goals_met: true, ..terms(Some(limit)) })), None, "met in time");
    assert_eq!(check(&mut env, Some(terms(None))), None, "no limit");
    assert_eq!(check(&mut env, Some(terms(Some(limit)))), Some(FailureReason::Time));
}

// Each realm words each failure its own way (the failure screen's `failure.<realm>.<reason>`).
fn test_each_realm_words_its_failures() {
    use engine::ui::text::{self, UiTextCategory};
    for realm in Realm::iter() {
        for reason in FailureReason::iter() {
            let key = format!("failure.{realm}.{reason}");
            assert!(text::peek(UiTextCategory::Realm, &key, None).1.is_some(), "en.json lacks Realm '{key}'");
        }
    }
    assert_eq!(engine::ui::text::get(UiTextCategory::Realm, "failure.dailong.legitimacy"), "The court has recalled you");
}

// ----------------------------------------------
// Distinctions (Phase 4)
// ----------------------------------------------

fn distinction(id: &str, condition: DistinctionCondition) -> MissionDistinctionDef {
    MissionDistinctionDef { id: id.into(), condition, stele_line: String::new() }
}

fn broken(env: &mut TestEnvironment, condition: &DistinctionCondition) -> bool {
    let context = env.new_sim_context(0.0);
    distinctions::is_broken(condition, &context)
}

fn earned(env: &mut TestEnvironment, distinction: &MissionDistinctionDef) -> bool {
    let context = env.new_sim_context(0.0);
    distinctions::is_earned(distinction, &context)
}

// Each form: a goal is read as the goals are met; the others break for good from the mission's
// start, never before it.
fn test_each_distinction_condition() {
    let mut env = mission_env();
    advance_months(&mut env, 2);
    let start = distinctions::mission_start_month(&env.new_sim_context(0.0));
    assert_eq!(start + 2, now(&env), "two months into the mission");

    // A goal, met or not; never broken.
    let culture =
        distinction("culture", DistinctionCondition::Goal(MissionGoal::Rating { rating: RatingKind::Culture, min: 60 }));
    env.world.stats_mut().ratings.values[RatingKind::Culture as usize] = 59;
    assert!(!earned(&mut env, &culture));
    env.world.stats_mut().ratings.values[RatingKind::Culture as usize] = 60;
    assert!(earned(&mut env, &culture));
    assert!(!broken(&mut env, &culture.condition));

    // A month's end in debt.
    let no_debt = DistinctionCondition::NoDebt;
    assert!(!broken(&mut env, &no_debt));
    env.sim.finance_mut().set_balance_unrecorded(-10);
    assert!(!broken(&mut env, &no_debt), "in debt, but not at a month's end yet");
    env.sim.finance_mut().set_months_in_debt(1);
    assert!(broken(&mut env, &no_debt));
    env.sim.finance_mut().set_months_in_debt(0);
    env.sim.finance_mut().set_balance_unrecorded(100);

    // A breach since the mission's start.
    let never_breached = DistinctionCondition::NeverBreached;
    let record = |ordinal_month: u32, breaches: u32| FloodRecord {
        ordinal_month,
        reach: 1,
        severity: 1.0,
        cells: 0,
        buildings_damaged: 0,
        fields_flooded: 0,
        breaches,
    };
    env.sim.floods_mut().begin_flood(Vec::new(), 1.0, 0, record(start - 1, 2));
    env.sim.floods_mut().begin_flood(Vec::new(), 1.0, 0, record(start, 0));
    assert!(!broken(&mut env, &never_breached), "a breach before the mission, a flood held back in it");
    env.sim.floods_mut().begin_flood(Vec::new(), 1.0, 0, record(start + 1, 1));
    assert!(broken(&mut env, &never_breached));

    // A house lost since the mission's start (another building doesn't count).
    let no_house_lost = DistinctionCondition::NoHouseLost;
    env.sim.risks_mut().record_disaster(Disaster::BurntDown { house: true }, start - 1);
    env.sim.risks_mut().record_disaster(Disaster::Collapsed { house: false }, start + 1);
    assert!(!broken(&mut env, &no_house_lost));
    env.sim.risks_mut().record_disaster(Disaster::Collapsed { house: true }, start + 2);
    assert!(broken(&mut env, &no_house_lost));

    // The good out of storage while people live in the city.
    let rice = DistinctionCondition::NeverShortOf { good: ResourceKind::Rice };
    env.world.stats_mut().population.total = 0;
    assert!(!broken(&mut env, &rice), "no one to go short");
    env.world.stats_mut().population.total = 5;
    assert!(broken(&mut env, &rice));
    env.world.stats_mut().add_storage_yard_resources(ResourceKind::Rice, 4);
    assert!(!broken(&mut env, &rice));

    // Within so many months of the start.
    let within = DistinctionCondition::Within { months: 3 };
    assert!(!broken(&mut env, &within), "two months in");
    advance_months(&mut env, 1);
    assert!(broken(&mut env, &within), "three months in");

    // A spoiled distinction isn't earned, even once its condition holds again.
    let no_debt = distinction("no_debt", DistinctionCondition::NoDebt);
    assert!(earned(&mut env, &no_debt));
    env.sim.ratings_mut().spoil("no_debt");
    assert!(!earned(&mut env, &no_debt));
}

// A month's start spoils the "never" forms broken then, kept with the game; not a goal, and nothing
// outside a mission, once it's finished or once it's lost. Each one's state follows.
fn test_distinctions_spoil_month_by_month() {
    let mut env = mission_env();
    let list = [
        distinction("no_debt", DistinctionCondition::NoDebt),
        distinction("culture", DistinctionCondition::Goal(MissionGoal::Rating { rating: RatingKind::Culture, min: 99 })),
        distinction("quick", DistinctionCondition::Within { months: 1 }),
    ];
    let check = |env: &mut TestEnvironment, terms: Option<MissionTerms>| {
        let context = env.new_sim_context(0.0);
        distinctions::check_spoiled(&context, &list, terms.as_ref())
    };
    let state = |env: &TestEnvironment, index: usize, completed: bool, earned: &[String]| {
        distinctions::state(&list[index], env.sim.ratings(), completed, earned)
    };

    assert!(check(&mut env, Some(terms(None))).is_empty(), "nothing broken");
    assert_eq!(state(&env, 0, false, &[]), DistinctionState::Open);

    env.sim.finance_mut().set_months_in_debt(1);
    advance_months(&mut env, 1);
    assert!(check(&mut env, None).is_empty(), "outside a mission");
    assert!(check(&mut env, Some(MissionTerms { completed: true, ..terms(None) })).is_empty(), "finished");
    env.sim.ratings_mut().set_failure(Some(FailureReason::Debt));
    assert!(check(&mut env, Some(terms(None))).is_empty(), "lost");
    env.sim.ratings_mut().set_failure(None);

    assert_eq!(check(&mut env, Some(terms(None))), ["no_debt", "quick"]);
    assert!(check(&mut env, Some(terms(None))).is_empty(), "spoiled once");
    assert_eq!(env.sim.ratings().spoiled().iter().collect::<Vec<_>>(), ["no_debt", "quick"]);
    assert_eq!(state(&env, 0, false, &[]), DistinctionState::Spoiled);
    assert_eq!(state(&env, 1, false, &[]), DistinctionState::Open, "a goal waits for the end");

    // Kept with the game.
    let saved = serde_json::to_value(env.sim.ratings()).unwrap();
    let loaded: CityRatings = serde_json::from_value(saved).unwrap();
    assert!(loaded.is_spoiled("no_debt") && loaded.is_spoiled("quick") && !loaded.is_spoiled("culture"));

    // Once finished: earned (in this play or an earlier one) or missed.
    let earned = ["no_debt".to_string()];
    assert_eq!(state(&env, 0, true, &earned), DistinctionState::Earned);
    assert_eq!(state(&env, 1, true, &earned), DistinctionState::Missed);
    assert_eq!(DistinctionState::Spoiled.to_string(), "spoiled");
}

// ----------------------------------------------
// Custom games (Phase 5)
// ----------------------------------------------

fn map_configs() -> &'static MapConfigs {
    if !MapConfigs::is_initialized() {
        MapConfigs::load();
    }
    MapConfigs::get()
}

// maps.json's goal sets all pass their checks (MapConfigs::post_load leaves out any that don't).
fn test_the_shipped_goal_sets_load() {
    let configs = map_configs();
    let ids: Vec<&str> = configs.goal_sets.iter().map(|set| set.id.as_str()).collect();
    assert_eq!(ids, ["thriving_town", "city_of_culture"]);
    let town = configs.goal_set("thriving_town").unwrap();
    assert_eq!(town.display_name(), "A thriving town");
    assert_eq!(town.requirements.rating_goal(RatingKind::Prosperity), Some(40));
    assert!(town.requirements.problems().is_empty());
}

// A blank custom game in Đại Long on the goal set `goal_set`, with the Failure switch `failure`.
fn custom_settings(goal_set: Option<&str>, failure: bool) -> CustomGameSettings {
    CustomGameSettings {
        realm: Realm::DaiLong,
        map_id: "blank".into(),
        map: ChosenMap::Blank { width: 64, height: 96, terrain: game::tile::sets::PresetTiles::Dirt },
        calendar: custom_game::starting_dates(Realm::DaiLong).into_iter().next().unwrap(),
        starting_gold: 2500,
        difficulty: Difficulty::Hard,
        fires_and_collapses: false,
        fast_construction: true,
        seasons_and_floods: false,
        raids: false,
        goal_set: goal_set.map(str::to_string),
        failure,
    }
}

// A custom game's goal set is the game's goals (the HUD's, the rings' ticks); once they're met it's
// completed, once, with its debrief asked for, and can't be lost any more. With its Failure switch
// on it's lost as a mission is; off, never. Its record keeps how it started, so Try again starts it
// the same way, and a save's header keeps it all.
fn test_a_custom_games_goals_and_failure() {
    map_configs();
    campaign::initialize();
    campaign::reset();
    MapConfigs::get_mut().goal_sets.push(GoalSetDef {
        id: "test_culture".into(),
        name: "Test culture".into(),
        requirements: MissionRequirements {
            goals: vec![MissionGoal::Rating { rating: RatingKind::Culture, min: 30 }],
            time_limit_months: None,
        },
    });

    let mut env = mission_env();
    let settings = custom_settings(Some("test_culture"), true);
    custom_game::set_active(Some(ActiveCustomGame::from_settings(&settings)));
    assert_eq!(campaign::game_requirements().map(|requirements| requirements.goals.len()), Some(1));
    assert_eq!(ratings::mission_goal(RatingKind::Culture), Some(30));

    // Its terms: lost to debt as a mission is.
    {
        let context = env.new_sim_context(0.0);
        let terms = failure::mission_terms(&context).expect("a custom game with its Failure switch on has terms");
        assert_eq!(terms, MissionTerms { completed: false, time_limit_months: None, goals_met: false });
        assert!(!custom_game::tick(&context), "Culture is below 30");
    }

    // Met: completed once, its debrief asked for.
    env.sim.ratings_mut().set_value(RatingKind::Culture, 30);
    env.world.refresh_stats(env.sim.stats_sources());
    {
        let context = env.new_sim_context(0.0);
        assert!(custom_game::tick(&context));
        assert!(!custom_game::tick(&context), "once");
        assert!(failure::mission_terms(&context).unwrap().completed);
    }
    assert!(custom_game::active().unwrap().record.completed);
    assert_eq!(campaign::take_pending_prompt(), Some(CampaignPrompt::CustomGameComplete));
    let limit = GameConfigs::get().sim.finance.debt_limit_months;
    env.sim.finance_mut().set_balance_unrecorded(-100);
    env.sim.finance_mut().set_months_in_debt(limit);
    {
        let context = env.new_sim_context(0.0);
        let terms = failure::mission_terms(&context);
        assert_eq!(failure::check_failure(&context, terms.as_ref()), None, "its goals met, it can't be lost");
    }

    // Unmet, the same debt loses it; a lost game's goals met don't complete it.
    custom_game::set_active(Some(ActiveCustomGame::from_settings(&settings)));
    {
        let context = env.new_sim_context(0.0);
        let terms = failure::mission_terms(&context);
        assert_eq!(failure::check_failure(&context, terms.as_ref()), Some(FailureReason::Debt));
        assert!(!custom_game::tick(&context), "lost");
    }
    assert!(!custom_game::active().unwrap().record.completed);

    // With the Failure switch off it has no terms, and a Sandbox has no goals.
    custom_game::set_active(Some(ActiveCustomGame::from_settings(&custom_settings(None, false))));
    {
        let context = env.new_sim_context(0.0);
        assert_eq!(failure::mission_terms(&context), None);
        assert!(!custom_game::tick(&context));
    }
    assert!(campaign::game_requirements().is_none());
    assert_eq!(ratings::mission_goal(RatingKind::Culture), None);

    // Try again starts it as it started, from its record; so does a save's header's.
    let game = ActiveCustomGame::from_settings(&settings);
    assert_eq!(game.settings(), Some(settings.clone()));
    let record = serde_json::to_value(&game.record).unwrap();
    assert_eq!(record["goal_set"], "test_culture");
    assert_eq!(record["start"]["blank"]["terrain"], "dirt");
    let header = game::save_header::SaveHeader {
        realm: Some(Realm::DaiLong),
        custom_game: Some(serde_json::from_value(record).unwrap()),
        ..Default::default()
    };
    let loaded = ActiveCustomGame::from_header(&header).unwrap();
    assert_eq!(loaded.settings(), Some(settings));
    // A record from before goal sets keeps no start: Try again can't start it.
    let old = ActiveCustomGame { realm: Realm::DaiLong, record: Default::default() };
    assert_eq!(old.settings(), None);

    custom_game::end();
    campaign::reset();
    MapConfigs::get_mut().goal_sets.retain(|set| set.id != "test_culture");
}
