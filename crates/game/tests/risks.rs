use common::{callback::Callback, coords::Cell, time::Seconds};
use game::{
    building::{
        Building,
        BuildingKind,
        BuildingKindAndId,
        BuildingTileInfo,
        BuildingVisitResult,
        construction::ConstructionMode,
        risks::{BuildingRisks, MAX_RISK, RiskKind, RiskRates},
    },
    campaign::config::MissionRisksDef,
    cheats,
    config::GameConfigs,
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{SimCmdQueue, SimContext, risks::CityRisks},
    unit::{
        Unit,
        config::UnitConfigKey,
        task::{
            UnitPatrolPathRecord,
            UnitTaskDespawn,
            UnitTaskPatrolCompletionCallback,
            UnitTaskPatrolState,
            UnitTaskRandomizedPatrol,
        },
    },
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for fire and collapse risk
// (docs/plans/city-risks.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Risks", &[
        // Build-up:
        test_utils::test_fn!(test_risks_build_up_at_the_config_rate),
        test_utils::test_fn!(test_risks_stop_at_the_top),
        test_utils::test_fn!(test_jitter_is_in_range_and_stable),
        test_utils::test_fn!(test_a_well_slows_the_fire_risk),
        test_utils::test_fn!(test_house_rates_follow_the_level),
        // What stops it:
        test_utils::test_fn!(test_sites_build_up_no_risk),
        test_utils::test_fn!(test_disabled_risks_and_the_cheat_stop_build_up),
        test_utils::test_fn!(test_mission_risks_block),
        // Visits:
        test_utils::test_fn!(test_visits_reset_their_risk),
        // Saves:
        test_utils::test_fn!(test_risks_round_trip_through_a_save),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

const TICK_SECS: Seconds = 0.5;

// Runs `test` with risks building up (the harness's `no_city_risks` cheat off).
fn with_risks<F: FnOnce()>(test: F) {
    cheats::get_mut().no_city_risks = false;
    test();
    cheats::get_mut().no_city_risks = true;
}

fn seconds_per_month() -> Seconds {
    GameConfigs::get().sim.seconds_per_month
}

fn months(env: &mut TestEnvironment, count: f32) {
    let ticks = (count * seconds_per_month() / TICK_SECS).round() as usize;
    test_utils::tick_n(env, ticks, TICK_SECS);
}

fn building(env: &TestEnvironment, handle: BuildingKindAndId) -> &Building {
    test_utils::find_building(env, handle)
}

fn risk(env: &TestEnvironment, handle: BuildingKindAndId, kind: RiskKind) -> f32 {
    building(env, handle).risks().get(kind)
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() <= 0.01 + expected.abs() * 1e-3, "{what}: expected {expected}, got {actual}");
}

// A road along y = 10, across the map, for buildings to link to.
fn road(env: &mut TestEnvironment) {
    let cells: Vec<Cell> = (1..31).map(|x| Cell::new(x, 10)).collect();
    test_utils::place_road(env, &cells);
}

// ----------------------------------------------
// Build-up
// ----------------------------------------------

// A month adds the building's rate (config times its jitter) to each risk.
fn test_risks_build_up_at_the_config_rate() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");

        let config_rates = *building(&env, yard).configs().risk_rates();
        assert!(config_rates.fire > 0.0 && config_rates.collapse > 0.0, "the test needs a storage yard with risks");

        months(&mut env, 1.0);

        let b = building(&env, yard);
        assert!(!b.risks().is_near_water(), "no wells on this map");
        let jitter = b.risks().jitter();
        assert_close(b.risk_rates().fire, config_rates.fire * jitter, "fire rate");
        assert_close(risk(&env, yard, RiskKind::Fire), config_rates.fire * jitter, "fire after a month");
        assert_close(risk(&env, yard, RiskKind::Collapse), config_rates.collapse * jitter, "collapse after a month");

        months(&mut env, 2.0);
        assert_close(risk(&env, yard, RiskKind::Fire), 3.0 * config_rates.fire * jitter, "fire after three months");
    });
}

// A risk stops at MAX_RISK: what happens then comes in Phase 1.
fn test_risks_stop_at_the_top() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
        test_utils::find_building_mut(&mut env, yard).risks_mut().set(RiskKind::Fire, MAX_RISK - 0.1);

        months(&mut env, 1.0);
        assert_eq!(risk(&env, yard, RiskKind::Fire), MAX_RISK);

        let risks = test_utils::find_building_mut(&mut env, yard).risks_mut();
        risks.set(RiskKind::Collapse, 250.0);
        assert_eq!(risks.get(RiskKind::Collapse), MAX_RISK, "set clamps to the top");
        risks.set(RiskKind::Collapse, -3.0);
        assert_eq!(risks.get(RiskKind::Collapse), 0.0, "and to zero");
    });
}

// Each building's jitter is within the configured spread, differs between cells, and is the
// same for the same building in the same place.
fn test_jitter_is_in_range_and_stable() {
    let spread = GameConfigs::get().sim.risks.rate_jitter;
    let mut env = TestEnvironment::new();

    let mut jitters = Vec::new();
    for i in 0..6 {
        let house = test_utils::spawn_building(&mut env, Cell::new(2 + 4 * i, 20), "house0");
        let jitter = building(&env, house).risks().jitter();
        assert!((1.0 - spread..=1.0 + spread).contains(&jitter), "jitter {jitter} out of range");
        jitters.push(jitter);
    }
    assert!(jitters.windows(2).any(|pair| pair[0] != pair[1]), "neighbours should get different jitters: {jitters:?}");

    let first = test_utils::find_building_id(&env, BuildingKind::House);
    let (cell, jitter) = (building(&env, first).base_cell(), building(&env, first).risks().jitter());
    test_utils::despawn_building(&mut env, first);
    let again = test_utils::spawn_building(&mut env, cell, "house0");
    assert_eq!(building(&env, again).risks().jitter(), jitter, "same cell and kind, same jitter");
}

// A building that a well reaches builds up fire risk at near_water_fire_scale of its rate; its
// collapse risk is unaffected.
fn test_a_well_slows_the_fire_risk() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let near = test_utils::spawn_building(&mut env, Cell::new(4, 11), "storage_yard");
        let far = test_utils::spawn_building(&mut env, Cell::new(24, 11), "storage_yard");
        test_utils::spawn_building(&mut env, Cell::new(8, 11), "small_well");

        months(&mut env, 1.0);

        let scale = GameConfigs::get().sim.risks.near_water_fire_scale;
        let (near_b, far_b) = (building(&env, near), building(&env, far));
        assert!(near_b.risks().is_near_water(), "the yard beside the well should have water");
        assert!(!far_b.risks().is_near_water(), "the yard across the map shouldn't");

        let config_rates = *near_b.configs().risk_rates();
        let near_jitter = near_b.risks().jitter();
        assert_close(near_b.risk_rates().fire, config_rates.fire * near_jitter * scale, "fire rate near water");
        assert_close(near_b.risk_rates().collapse, config_rates.collapse * near_jitter, "collapse rate near water");
        assert_close(risk(&env, near, RiskKind::Fire), config_rates.fire * near_jitter * scale, "fire near water");
        assert_close(risk(&env, far, RiskKind::Fire), config_rates.fire * far_b.risks().jitter(), "fire far from water");
    });
}

// A house's rates are its level's: thatch at the bottom burns faster than tile roofs higher up.
fn test_house_rates_follow_the_level() {
    let mut env = TestEnvironment::new();
    let low = test_utils::spawn_building(&mut env, Cell::new(4, 4), "house0");
    let high = test_utils::spawn_building(&mut env, Cell::new(8, 4), "house3");
    let (low_rates, high_rates) = (*building(&env, low).configs().risk_rates(), *building(&env, high).configs().risk_rates());
    assert!(low_rates.fire > high_rates.fire, "level 0 {} should burn faster than level 3 {}", low_rates.fire, high_rates.fire);
    assert!(high_rates.fire > 0.0 && high_rates.collapse > 0.0);
}

// ----------------------------------------------
// What stops it
// ----------------------------------------------

// A construction site builds up no risk until it's finished.
fn test_sites_build_up_no_risk() {
    with_risks(|| {
        cheats::get_mut().instant_construction = false;
        let mut env = TestEnvironment::new();
        road(&mut env);
        let tile_def = test_utils::find_building_def("storage_yard");
        let context = env.new_sim_context(0.0);
        let result = TilePlacement::place(&context, Cell::new(20, 11), tile_def, false, true, ConstructionMode::Site);
        assert!(!matches!(result, PlaceOrClearResult::Failed { .. }), "the site should be placed");
        let site = test_utils::find_building_id(&env, BuildingKind::StorageYard);
        assert!(building(&env, site).is_under_construction());

        test_utils::tick_n(&mut env, 10, TICK_SECS);
        assert!(building(&env, site).is_under_construction(), "still building");
        assert_eq!(risk(&env, site, RiskKind::Fire), 0.0);
        assert_eq!(risk(&env, site, RiskKind::Collapse), 0.0);
        cheats::get_mut().instant_construction = true;
    });
}

// Risks turned off for the game, or the cheat on, stop all build-up.
fn test_disabled_risks_and_the_cheat_stop_build_up() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");

        env.sim.risks_mut().set_enabled(false);
        assert!(!env.sim.risks().is_active());
        months(&mut env, 1.0);
        assert_eq!(risk(&env, yard, RiskKind::Fire), 0.0, "disabled");

        env.sim.risks_mut().set_enabled(true);
        cheats::get_mut().no_city_risks = true;
        assert!(!env.sim.risks().is_active());
        months(&mut env, 1.0);
        assert_eq!(risk(&env, yard, RiskKind::Fire), 0.0, "the cheat");

        cheats::get_mut().no_city_risks = false;
        assert!(env.sim.risks().is_active());
        months(&mut env, 1.0);
        assert!(risk(&env, yard, RiskKind::Fire) > 0.0, "on again");
    });
}

// A mission's `risks` block turns risks off or on; left out, the default holds.
fn test_mission_risks_block() {
    let configs = &GameConfigs::get().sim.risks;
    let mut risks = CityRisks::new(configs);
    assert_eq!(risks.is_enabled(), configs.enabled);

    let off: MissionRisksDef = serde_json::from_str(r#"{ "enabled": false }"#).expect("a valid risks block");
    risks.start_mission(&off, configs);
    assert!(!risks.is_enabled());

    let default: MissionRisksDef = serde_json::from_str("{}").unwrap();
    risks.start_mission(&default, configs);
    assert_eq!(risks.is_enabled(), configs.enabled);

    assert!(serde_json::from_str::<MissionRisksDef>(r#"{ "enable": false }"#).is_err(), "unknown fields are refused");
}

// ----------------------------------------------
// Visits
// ----------------------------------------------

fn patrol_test_callback(_context: &SimContext, _building: &mut Building, _unit: &mut Unit) {}

fn register_patrol_test_callback() -> Callback<UnitTaskPatrolCompletionCallback> {
    common::callback::register!(patrol_test_callback)
}

// A walker on a patrol from a building of `origin_kind`. Only the patrol's origin matters to a
// visit, so the task borrows `home`'s place and id.
fn patrol_walker(env: &mut TestEnvironment, home: BuildingKindAndId, origin_kind: BuildingKind) -> game::unit::UnitId {
    let (road_link, base_cell) = {
        let b = building(env, home);
        (b.road_link().expect("home should be road-linked"), b.base_cell())
    };
    let unit_id = test_utils::spawn_unit(env, road_link, UnitConfigKey::Peasant);
    let completion_task = env.sim.task_manager_mut().new_task(UnitTaskDespawn::default());
    let task = UnitTaskRandomizedPatrol {
        origin_building: BuildingKindAndId { kind: origin_kind, id: home.id },
        origin_building_tile: BuildingTileInfo { road_link, base_cell },
        max_distance: 5,
        path_bias_min: 0.1,
        path_bias_max: 0.5,
        path_record: UnitPatrolPathRecord::default(),
        buildings_to_visit: Some(BuildingKind::House),
        completion_callback: register_patrol_test_callback(),
        completion_task,
        idle_countdown: None,
        state: UnitTaskPatrolState::default(),
        completion_callback_done: false,
        visited_buildings: None,
    };
    test_utils::assign_task(env, unit_id, task);
    unit_id
}

// Despawns a walker, which frees its tasks (the test environment checks none leak).
fn despawn_walker(env: &mut TestEnvironment, unit_id: game::unit::UnitId) {
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    cmds.despawn_unit_with_id(unit_id);
    cmds.execute(&context);
}

fn visit(env: &mut TestEnvironment, target: BuildingKindAndId, unit_id: game::unit::UnitId) -> BuildingVisitResult {
    let context = env.new_sim_context(0.0);
    let unit = context.find_unit_mut(unit_id).expect("the walker");
    let building = context.find_building_mut(target.kind, target.id).expect("the building");
    building.visited_by(unit, &context)
}

// A fire watcher's visit resets the fire risk only, a builder's the collapse risk only, and
// any other walker's neither.
fn test_visits_reset_their_risk() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
    let set_both = |env: &mut TestEnvironment| {
        let risks = test_utils::find_building_mut(env, house).risks_mut();
        risks.set(RiskKind::Fire, 60.0);
        risks.set(RiskKind::Collapse, 50.0);
    };

    set_both(&mut env);
    let fire_watcher = patrol_walker(&mut env, house, BuildingKind::FireWatch);
    assert_eq!(visit(&mut env, house, fire_watcher), BuildingVisitResult::Accepted);
    assert_eq!(risk(&env, house, RiskKind::Fire), 0.0, "a fire watcher resets the fire risk");
    assert_eq!(risk(&env, house, RiskKind::Collapse), 50.0, "but not the collapse risk");

    set_both(&mut env);
    let builder = patrol_walker(&mut env, house, BuildingKind::BuildersGuild);
    assert_eq!(visit(&mut env, house, builder), BuildingVisitResult::Accepted);
    assert_eq!(risk(&env, house, RiskKind::Collapse), 0.0, "a builder resets the collapse risk");
    assert_eq!(risk(&env, house, RiskKind::Fire), 60.0, "but not the fire risk");

    set_both(&mut env);
    let monk = patrol_walker(&mut env, house, BuildingKind::Temple);
    assert_eq!(visit(&mut env, house, monk), BuildingVisitResult::Refused, "a house has nothing for a temple's walker");
    assert_eq!(risk(&env, house, RiskKind::Fire), 60.0);
    assert_eq!(risk(&env, house, RiskKind::Collapse), 50.0);

    for walker in [fire_watcher, builder, monk] {
        despawn_walker(&mut env, walker);
    }
}

// ----------------------------------------------
// Saves
// ----------------------------------------------

// The risks are saved; the jitter isn't, and comes back the same on load.
fn test_risks_round_trip_through_a_save() {
    let configs = &GameConfigs::get().sim.risks;
    let cell = Cell::new(7, 9);
    let mut risks = BuildingRisks::new(cell, BuildingKind::House, configs);
    risks.set(RiskKind::Fire, 42.5);
    risks.set(RiskKind::Collapse, 17.25);
    risks.set_near_water(true);

    let saved = serde_json::to_value(&risks).unwrap();
    let mut keys: Vec<&str> = saved.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["burning", "collapse", "fire", "near_water", "water_check_timer"]);

    let mut loaded: BuildingRisks = serde_json::from_value(saved).unwrap();
    loaded.post_load(cell, BuildingKind::House, configs);
    assert_eq!(loaded.get(RiskKind::Fire), 42.5);
    assert_eq!(loaded.get(RiskKind::Collapse), 17.25);
    assert!(loaded.is_near_water());
    assert!(!loaded.is_burning());
    assert_eq!(loaded.jitter(), risks.jitter());

    let rates = RiskRates { fire: 8.0, collapse: 4.0 };
    assert_eq!(loaded.effective_rates(&rates, configs).fire, risks.effective_rates(&rates, configs).fire);

    let city: CityRisks = serde_json::from_str(r#"{ "enabled": false }"#).unwrap();
    assert!(!city.is_enabled());
}
