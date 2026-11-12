use common::{
    callback::Callback,
    coords::{Cell, CellRange},
    time::Seconds,
};
use game::{
    building::{
        Building,
        BuildingKind,
        BuildingKindAndId,
        BuildingTileInfo,
        BuildingVisitResult,
        HouseUpgradeDirection,
        config::BuildingConfigs,
        construction::ConstructionMode,
        destruction::{self, Destruction},
        PatrolVisits,
        risks::{self, BuildingRisks, MAX_RISK, RiskKind, RiskRates, RiskSummary},
    },
    campaign::config::MissionRisksDef,
    cheats,
    config::GameConfigs,
    debug::{game_object_debug::GameObjectDebugVarRef, preset_maps},
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{SimCmdQueue, SimContextMut, resources::ResourceKind, risks::CityRisks},
    tile::{
        TileFlags,
        TileKind,
        TileMapLayerKind,
        minimap::{
            MinimapLens,
            lens::{self, LensContext, LensData},
            style::MinimapStyle,
        },
        rubble,
    },
    config::RiskConfigs,
    undo_redo,
    world::city_stock::CityStock,
    unit::{
        Unit,
        UnitTaskHelper,
        config::UnitConfigKey,
        task::{
            UnitPatrolPathRecord,
            UnitTaskDespawn,
            Emergency,
            UnitTaskPatrolCompletionCallback,
            UnitTaskPatrolState,
            UnitTaskRandomizedPatrol,
            UnitTaskRespond,
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
        test_utils::test_fn!(test_set_clamps_risks),
        test_utils::test_fn!(test_jitter_is_in_range_and_stable),
        test_utils::test_fn!(test_a_well_slows_the_fire_risk),
        test_utils::test_fn!(test_house_rates_follow_the_level),
        // What stops it:
        test_utils::test_fn!(test_sites_build_up_no_risk),
        test_utils::test_fn!(test_disabled_risks_and_the_cheat_stop_build_up),
        test_utils::test_fn!(test_mission_risks_block),
        // Visits:
        test_utils::test_fn!(test_visits_reset_their_risk),
        // Fire and collapse:
        test_utils::test_fn!(test_a_fire_risk_at_the_top_starts_a_fire),
        test_utils::test_fn!(test_a_fire_burns_down_and_leaves_rubble),
        test_utils::test_fn!(test_a_fire_spreads_to_its_neighbours),
        test_utils::test_fn!(test_a_fire_watcher_puts_out_a_fire),
        test_utils::test_fn!(test_fires_wait_while_risks_are_off),
        test_utils::test_fn!(test_a_collapse_leaves_rubble_at_once),
        // Rubble:
        test_utils::test_fn!(test_every_footprint_leaves_a_whole_pile),
        test_utils::test_fn!(test_rubble_piles_follow_their_neighbours),
        test_utils::test_fn!(test_a_burnt_house_leaves_rubble_on_its_lots),
        // Destroyed buildings:
        test_utils::test_fn!(test_commands_for_a_destroyed_building_are_skipped),
        test_utils::test_fn!(test_a_burnt_yard_loses_its_goods_at_once),
        test_utils::test_fn!(test_a_runner_comes_back_empty_from_a_collapsed_granary),
        test_utils::test_fn!(test_a_requested_collapse_waits_for_the_next_frame),
        // The fire watch and the builders' guild:
        test_utils::test_fn!(test_fire_watch_and_guild_configs),
        test_utils::test_fn!(test_their_patrols_visit_every_building),
        test_utils::test_fn!(test_a_fire_watch_sends_a_walker_to_a_fire),
        test_utils::test_fn!(test_a_fire_out_of_reach_gets_no_walker),
        test_utils::test_fn!(test_a_fire_watcher_goes_on_to_the_next_fire),
        test_utils::test_fn!(test_a_fire_watcher_walks_off_the_road_to_a_fire),
        // Disease:
        test_utils::test_fn!(test_disease_builds_up_with_a_houses_needs),
        test_utils::test_fn!(test_the_outbreak_chance),
        test_utils::test_fn!(test_a_house_at_risk_falls_sick),
        test_utils::test_fn!(test_a_medics_patrol_resets_the_disease_risk),
        test_utils::test_fn!(test_a_sick_house_loses_residents_after_the_grace_period),
        test_utils::test_fn!(test_an_apothecary_halves_the_deaths),
        test_utils::test_fn!(test_a_sick_house_spreads_disease_to_the_houses_around_it),
        test_utils::test_fn!(test_a_medic_cures_an_outbreak),
        test_utils::test_fn!(test_a_hospital_sends_a_medic_to_an_outbreak),
        test_utils::test_fn!(test_an_empty_sick_house_ends_its_outbreak),
        test_utils::test_fn!(test_a_sick_house_takes_no_one_in_and_doesnt_grow),
        test_utils::test_fn!(test_a_sick_house_that_downgrades_stays_sick),
        test_utils::test_fn!(test_disease_can_be_off_while_risks_are_on),
        test_utils::test_fn!(test_the_cheat_stops_disease),
        // Summary and lens:
        test_utils::test_fn!(test_the_risk_summary),
        test_utils::test_fn!(test_the_risk_lens),
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

fn burn_secs() -> Seconds {
    GameConfigs::get().sim.risks.burn_secs
}

fn secs(env: &mut TestEnvironment, count: Seconds) {
    test_utils::tick_n(env, (count / TICK_SECS).round() as usize, TICK_SECS);
}

fn exists(env: &TestEnvironment, handle: BuildingKindAndId) -> bool {
    env.world.find_building(handle.kind, handle.id).is_some()
}

fn is_burning(env: &TestEnvironment, handle: BuildingKindAndId) -> bool {
    building(env, handle).is_burning()
}

// Sets a building's risk to the top and runs a step, which sets off what that risk brings.
fn top_out(env: &mut TestEnvironment, handle: BuildingKindAndId, kind: RiskKind) {
    test_utils::find_building_mut(env, handle).risks_mut().set(kind, MAX_RISK);
    test_utils::tick(env, TICK_SECS);
}

fn building_tile_flags(env: &TestEnvironment, cell: Cell) -> TileFlags {
    env.tile_map.find_tile(cell, TileKind::Building).expect("a building tile").flags()
}

// Queues `cause`'s destruction of the building and runs it, as a world step's commands do.
fn destroy(env: &mut TestEnvironment, handle: BuildingKindAndId, cause: Destruction) {
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    cmds.destroy_building(handle, cause);
    cmds.execute(&context);
}

// The rubble's variation name at `cell`, or None without rubble.
fn rubble_at(env: &TestEnvironment, cell: Cell) -> Option<&'static str> {
    env.tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Objects)
        .filter(|tile| rubble::is_rubble_def(tile.tile_def()))
        .map(|tile| tile.variation_name())
}

// The variation rubble at `cell` should have when `pile` holds the cells with rubble: a digit
// per side, north (+x), east (-y), south (-x), west (+y).
fn expected_rubble(pile: &[Cell], cell: Cell) -> String {
    [(1, 0), (0, -1), (-1, 0), (0, 1)]
        .iter()
        .map(|(dx, dy)| if pile.contains(&Cell::new(cell.x + dx, cell.y + dy)) { '1' } else { '0' })
        .collect()
}

fn cells_of(range: CellRange) -> Vec<Cell> {
    (&range).into_iter().collect()
}

// Every cell of `pile` has rubble whose variation matches its neighbours.
fn assert_pile(env: &TestEnvironment, pile: &[Cell], what: &str) {
    for cell in pile {
        let expected = expected_rubble(pile, *cell);
        assert_eq!(rubble_at(env, *cell), Some(expected.as_str()), "{what}: rubble at {cell}");
    }
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

// A risk is kept from 0 to MAX_RISK, whatever it's set to.
fn test_set_clamps_risks() {
    let configs = &GameConfigs::get().sim.risks;
    let mut risks = BuildingRisks::new(Cell::new(3, 4), BuildingKind::StorageYard, configs);
    risks.set(RiskKind::Collapse, 250.0);
    assert_eq!(risks.get(RiskKind::Collapse), MAX_RISK, "set clamps to the top");
    risks.set(RiskKind::Collapse, -3.0);
    assert_eq!(risks.get(RiskKind::Collapse), 0.0, "and to zero");
    risks.add(RiskKind::Fire, MAX_RISK * 3.0);
    assert_eq!(risks.get(RiskKind::Fire), MAX_RISK, "add stops at the top");
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
    assert!(risks.is_disease_enabled(), "disease is on unless the block says otherwise");

    let no_disease: MissionRisksDef = serde_json::from_str(r#"{ "disease": false }"#).expect("a valid risks block");
    risks.start_mission(&no_disease, configs);
    assert_eq!(risks.is_enabled(), configs.enabled, "fires and collapses keep the default");
    assert!(!risks.is_disease_enabled());

    let default: MissionRisksDef = serde_json::from_str("{}").unwrap();
    risks.start_mission(&default, configs);
    assert_eq!(risks.is_enabled(), configs.enabled);
    assert!(risks.is_disease_enabled());

    assert!(serde_json::from_str::<MissionRisksDef>(r#"{ "enable": false }"#).is_err(), "unknown fields are refused");
}

// ----------------------------------------------
// Visits
// ----------------------------------------------

fn patrol_test_callback(_context: &SimContextMut, _building: &mut Building, _unit: &mut Unit) {}

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
// Fire and collapse
// ----------------------------------------------

// A fire risk at the top sets the building on fire: drawn burning, not working, holding goods
// it can't hand out or take, and announced.
fn test_a_fire_risk_at_the_top_starts_a_fire() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
        assert_eq!(test_utils::find_building_mut(&mut env, yard).receive_resources(ResourceKind::Wood, 8), 8);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 8);

        top_out(&mut env, yard, RiskKind::Fire);

        let b = building(&env, yard);
        assert!(b.is_burning(), "the yard should be on fire");
        assert!(!b.is_operational(), "a burning building doesn't work");
        assert_close(b.risks().burning().unwrap().secs_left, burn_secs(), "a fire burns for burn_secs");
        assert_eq!(b.available_resources(ResourceKind::Wood), 0, "a burning yard hands out nothing");
        assert_eq!(b.receivable_resources(ResourceKind::Wood), 0, "and takes nothing");
        assert_eq!(b.stock().iter().find(|item| item.kind == ResourceKind::Wood).map(|item| item.count), Some(8));
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 0, "construction can't take from it");
        assert!(building_tile_flags(&env, Cell::new(20, 11)).intersects(TileFlags::Burning));
        assert_eq!(env.tile_map.minimap().alert_count(), 1, "a fire raises a minimap alert");
        assert_eq!(risks::take_notices(), vec!["Fire! The storage yard is burning.".to_string()]);
    });
}

// A building burns for burn_secs, then is gone, with rubble on every cell it stood on.
fn test_a_fire_burns_down_and_leaves_rubble() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
        let footprint = building(&env, yard).cell_range();

        top_out(&mut env, yard, RiskKind::Fire);
        secs(&mut env, burn_secs() - 2.0 * TICK_SECS);
        assert!(is_burning(&env, yard), "still burning just before burn_secs");

        secs(&mut env, 2.0 * TICK_SECS);
        assert!(!exists(&env, yard), "burnt down after burn_secs");
        assert_pile(&env, &cells_of(footprint), "a burnt-down 3x3");
        assert_eq!(rubble_at(&env, Cell::new(21, 12)), Some("1111"), "the middle of the pile");

        let notices = risks::take_notices();
        assert_eq!(notices.last().map(String::as_str), Some("The storage yard burnt down."));
    });
}

// A burning building adds spread_per_sec fire risk a second to each building next to it (not
// to one further off, or to one with no fire rate), until its neighbour catches fire too.
fn test_a_fire_spreads_to_its_neighbours() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let burning = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
        let next_door = test_utils::spawn_building(&mut env, Cell::new(11, 11), "house0");
        let across = test_utils::spawn_building(&mut env, Cell::new(14, 11), "house0");
        let well = test_utils::spawn_building(&mut env, Cell::new(10, 12), "small_well");
        assert_eq!(building(&env, well).configs().risk_rates().fire, 0.0, "the test needs a well that can't burn");
        assert!(!building(&env, well).can_catch_fire());

        top_out(&mut env, burning, RiskKind::Fire);
        let (next_door_before, across_before) = (risk(&env, next_door, RiskKind::Fire), risk(&env, across, RiskKind::Fire));

        let spread = GameConfigs::get().sim.risks.spread_per_sec;
        secs(&mut env, 4.0);
        let next_door_gain = risk(&env, next_door, RiskKind::Fire) - next_door_before;
        let own_rate_bound = 1.0; // Its own build-up over 4 s is far below a point.
        assert!(
            next_door_gain >= spread * 4.0 && next_door_gain <= spread * 4.0 + own_rate_bound,
            "next door gains spread_per_sec a second: {next_door_gain}"
        );
        assert!(risk(&env, across, RiskKind::Fire) - across_before < own_rate_bound, "a house further off gains none");
        assert_eq!(risk(&env, well, RiskKind::Fire), 0.0, "a well can't burn");

        // Next door catches fire while the first house still burns (tick_until panics otherwise).
        test_utils::tick_until(&mut env, (burn_secs() / TICK_SECS) as usize, TICK_SECS, |env| is_burning(env, next_door));
        assert!(exists(&env, burning), "next door should catch fire before the first burns down");
    });
}

// A fire watcher's visit puts a fire out: the building works again with no fire risk. Other
// walkers are turned away while it burns.
fn test_a_fire_watcher_puts_out_a_fire() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
        top_out(&mut env, house, RiskKind::Fire);
        assert!(is_burning(&env, house));

        let builder = patrol_walker(&mut env, house, BuildingKind::BuildersGuild);
        assert_eq!(visit(&mut env, house, builder), BuildingVisitResult::Refused, "a burning house takes no builder");

        let fire_watcher = patrol_walker(&mut env, house, BuildingKind::FireWatch);
        assert_eq!(visit(&mut env, house, fire_watcher), BuildingVisitResult::Accepted);
        assert!(!is_burning(&env, house), "the fire is out");
        assert_eq!(risk(&env, house, RiskKind::Fire), 0.0);
        assert!(!building_tile_flags(&env, Cell::new(10, 11)).intersects(TileFlags::Burning));
        assert_eq!(env.tile_map.minimap().alert_count(), 0, "the fire's alert is resolved");

        // The walkers' patrols name a fire watch and a guild that don't exist: done with them.
        for walker in [builder, fire_watcher] {
            despawn_walker(&mut env, walker);
        }

        secs(&mut env, burn_secs() + 2.0);
        assert!(exists(&env, house), "a fire put out doesn't burn the house down");
        assert_eq!(
            risks::take_notices(),
            vec!["Fire! The house is burning.".to_string(), "The fire at the house is out.".to_string()]
        );
    });
}

// While risks are off, a fire neither spreads nor burns down.
fn test_fires_wait_while_risks_are_off() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
        let next_door = test_utils::spawn_building(&mut env, Cell::new(11, 11), "house0");
        top_out(&mut env, house, RiskKind::Fire);
        let secs_left = building(&env, house).risks().burning().unwrap().secs_left;
        let next_door_fire = risk(&env, next_door, RiskKind::Fire);

        env.sim.risks_mut().set_enabled(false);
        secs(&mut env, burn_secs() * 2.0);
        assert!(is_burning(&env, house), "still burning");
        assert_eq!(building(&env, house).risks().burning().unwrap().secs_left, secs_left, "but not burning down");
        assert_eq!(risk(&env, next_door, RiskKind::Fire), next_door_fire, "or spreading");

        env.sim.risks_mut().set_enabled(true);
        secs(&mut env, burn_secs() + 2.0);
        assert!(!exists(&env, house), "it burns down once risks are on again");
    });
}

// A collapse risk at the top brings the building down in the same step.
fn test_a_collapse_leaves_rubble_at_once() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
        let footprint = building(&env, yard).cell_range();

        top_out(&mut env, yard, RiskKind::Collapse);
        assert!(!exists(&env, yard), "collapsed");
        assert_pile(&env, &cells_of(footprint), "a collapsed 3x3");
        assert_eq!(env.tile_map.minimap().alert_count(), 1, "a collapse raises a minimap alert");
        assert_eq!(risks::take_notices(), vec!["The storage yard collapsed.".to_string()]);

        // Clearing the rubble resolves it.
        let context = env.new_sim_context(0.0);
        let tile = context.try_tile_from_layer(Cell::new(20, 11), TileMapLayerKind::Objects).unwrap();
        assert!(matches!(TilePlacement::clear(&context, tile, false, false), PlaceOrClearResult::ClearedTile(_)));
        assert_eq!(env.tile_map.minimap().alert_count(), 0, "clearing the rubble at the alert resolves it");
    });
}

// ----------------------------------------------
// Rubble
// ----------------------------------------------

// Buildings 1x1 to 5x5 leave a pile over their whole footprint: corners, sides and middles.
fn test_every_footprint_leaves_a_whole_pile() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let buildings = [
        ("house0", Cell::new(2, 11)),
        ("market", Cell::new(4, 11)),
        ("storage_yard", Cell::new(7, 11)),
        ("governor_palace", Cell::new(11, 11)),
        ("citadel", Cell::new(16, 11)),
    ];

    for (size, (name, cell)) in buildings.iter().enumerate() {
        let handle = test_utils::spawn_building(&mut env, *cell, name);
        let footprint = building(&env, handle).cell_range();
        assert_eq!(footprint.size().width as usize, size + 1, "{name} should be {0}x{0}", size + 1);

        destroy(&mut env, handle, Destruction::Collapsed);
        assert!(!exists(&env, handle));
        assert_pile(&env, &cells_of(footprint), name);
    }

    assert_eq!(rubble_at(&env, Cell::new(2, 11)), Some("0000"), "a 1x1 leaves a lone heap");
    // The 2x2's four corners. North is +x and west +y, so the base cell's rubble runs north and west.
    assert_eq!(rubble_at(&env, Cell::new(4, 11)), Some("1001"));
    assert_eq!(rubble_at(&env, Cell::new(5, 11)), Some("0011"));
    assert_eq!(rubble_at(&env, Cell::new(5, 12)), Some("0110"));
    assert_eq!(rubble_at(&env, Cell::new(4, 12)), Some("1100"));
    assert_eq!(rubble_at(&env, Cell::new(18, 13)), Some("1111"), "the 5x5's middle");
}

// Clearing part of a pile reshapes the pieces around the gap, and piles side by side join.
fn test_rubble_piles_follow_their_neighbours() {
    let mut env = TestEnvironment::new();
    let first = CellRange::new(Cell::new(10, 10), Cell::new(12, 12));
    let second = CellRange::new(Cell::new(13, 10), Cell::new(14, 11));
    {
        let context = env.new_sim_context(0.0);
        assert_eq!(rubble::place_pile(&context, first), 9);
        assert_eq!(rubble::place_pile(&context, second), 4);
    }

    let mut pile: Vec<Cell> = cells_of(first).into_iter().chain(cells_of(second)).collect();
    assert_pile(&env, &pile, "two piles side by side join");
    assert_eq!(rubble_at(&env, Cell::new(12, 10)), Some("1011"), "the first pile's edge now runs into the second");

    // Clear the first pile's middle and a corner, as the player does.
    for cell in [Cell::new(11, 11), Cell::new(10, 10)] {
        let context = env.new_sim_context(0.0);
        let tile = context.try_tile_from_layer(cell, TileMapLayerKind::Objects).unwrap();
        assert!(matches!(TilePlacement::clear(&context, tile, false, true), PlaceOrClearResult::ClearedTile(_)));
        pile.retain(|c| *c != cell);
    }
    assert_eq!(rubble_at(&env, Cell::new(11, 11)), None);
    assert_pile(&env, &pile, "after clearing the middle and a corner");
    assert_eq!(rubble_at(&env, Cell::new(11, 10)), Some("1000"), "an end, with the middle and the corner gone");

    // Undoing the last clear puts the corner back, joined to the pile again.
    {
        let context = env.new_sim_context(0.0);
        undo_redo::undo(&context);
    }
    pile.push(Cell::new(10, 10));
    assert_pile(&env, &pile, "after undoing the corner's clear");
    undo_redo::clear();
}

// A house stands on vacant lots, where nothing else can be placed. When it burns down, every
// lot goes back to the land it was laid on, grass or dirt, under a whole pile of rubble, and
// clearing the rubble leaves bare land, not a lot.
fn test_a_burnt_house_leaves_rubble_on_its_lots() {
    with_risks(|| {
        let mut env = TestEnvironment::with_terrain(TestEnvironment::DEFAULT_MAP_SIZE, test_utils::find_terrain_def("grass"));
        road(&mut env);
        let lots = CellRange::new(Cell::new(20, 11), Cell::new(21, 12));
        let on_dirt = Cell::new(21, 12);
        test_utils::place_terrain(&mut env, on_dirt, "dirt");
        for cell in &lots {
            test_utils::place_tile_as_player(&mut env, cell, test_utils::find_terrain_def("vacant_lot"));
        }
        let terrain = |env: &TestEnvironment, cell: Cell| {
            let tile = env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).expect("terrain");
            (tile.name().to_string(), tile.variation_name())
        };
        assert_eq!(terrain(&env, on_dirt), ("vacant_lot".to_string(), "dirt"), "a lot laid on dirt");

        let house = test_utils::spawn_building(&mut env, lots.start, "house1");
        assert!(cells_of(building(&env, house).cell_range()) == cells_of(lots), "a 2x2 house on the four lots");

        top_out(&mut env, house, RiskKind::Fire);
        secs(&mut env, burn_secs() + TICK_SECS);
        assert!(!exists(&env, house), "burnt down");
        assert_pile(&env, &cells_of(lots), "a burnt-down house on lots");

        for cell in &lots {
            let land = if cell == on_dirt { "dirt" } else { "grass" };
            assert_eq!(terrain(&env, cell).0, land, "the lot at {cell} is {land} again");
        }

        let context = env.new_sim_context(0.0);
        let tile = context.try_tile_from_layer(lots.start, TileMapLayerKind::Objects).unwrap();
        assert!(matches!(TilePlacement::clear(&context, tile, false, false), PlaceOrClearResult::ClearedTile(_)));
        assert_eq!(rubble_at(&env, lots.start), None);
        assert_eq!(terrain(&env, lots.start).0, "grass", "bare land under the cleared rubble");
    });
}

// ----------------------------------------------
// Destroyed buildings
// ----------------------------------------------

// A destruction runs after the other commands of its step, so a visit queued after it still
// finds the building; commands queued for it once it's gone are skipped, not a panic.
fn test_commands_for_a_destroyed_building_are_skipped() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
    test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Fire, 60.0);
    let fire_watcher = patrol_walker(&mut env, house, BuildingKind::FireWatch);

    {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.destroy_building(house, Destruction::Collapsed);
        let visited = std::rc::Rc::new(std::cell::Cell::new(false));
        let visited_in_cmd = visited.clone();
        cmds.visit_building_with_completion(house, fire_watcher, move |_context, building, _unit, result| {
            assert_eq!(result, BuildingVisitResult::Accepted);
            assert_eq!(building.risks().get(RiskKind::Fire), 0.0);
            visited_in_cmd.set(true);
        });
        cmds.execute(&context);
        assert!(visited.get(), "the visit ran before the destruction");
    }
    assert!(!exists(&env, house));
    assert_eq!(rubble_at(&env, Cell::new(10, 11)), Some("0000"));

    {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.visit_building(house, fire_watcher);
        cmds.defer_building_update(house, |_context, _building| panic!("a deferred update ran for a building that's gone"));
        cmds.destroy_building(house, Destruction::BurntDown);
        cmds.execute(&context);
    }

    // A plain despawn isn't delayed: a visit after it in the same queue is skipped.
    let other = test_utils::spawn_building(&mut env, Cell::new(14, 11), "house0");
    {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.despawn_building_with_id(other);
        cmds.visit_building(other, fire_watcher);
        cmds.execute(&context);
    }

    despawn_walker(&mut env, fire_watcher);
}

// A storage yard that burns down takes its goods with it: the world stats and the city stock
// stop counting them at once, no gold moves, and the notice lists them.
fn test_a_burnt_yard_loses_its_goods_at_once() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let burnt = test_utils::spawn_building(&mut env, Cell::new(6, 11), "storage_yard");
        let kept = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
        for (yard, wood, bricks) in [(burnt, 20, 12), (kept, 4, 0)] {
            let b = test_utils::find_building_mut(&mut env, yard);
            assert_eq!(b.receive_resources(ResourceKind::Wood, wood), wood);
            assert_eq!(b.receive_resources(ResourceKind::Bricks, bricks), bricks);
        }
        test_utils::tick(&mut env, TICK_SECS);
        let stats_wood = |env: &TestEnvironment| env.world.stats().resources.storage_yards.count(ResourceKind::Wood);
        assert_eq!(stats_wood(&env), 24);

        top_out(&mut env, burnt, RiskKind::Fire);
        let finance_before = serde_json::to_value(env.sim.finance()).unwrap();

        // Run the fire's last step by hand, to read the stats right after its commands.
        secs(&mut env, burn_secs() - TICK_SECS);
        assert!(is_burning(&env, burnt));
        {
            let context = env.new_sim_context(TICK_SECS);
            let cmds = context.cmds_mut();
            context.world_mut().update(cmds, &context);
            cmds.execute(&context);
        }
        assert!(!exists(&env, burnt), "burnt down");

        let check = |env: &TestEnvironment, when: &str| {
            assert_eq!(stats_wood(env), 4, "{when}: the stats count the kept yard's wood only");
            assert_eq!(env.world.stats().resources.all.count(ResourceKind::Bricks), 0, "{when}: no bricks left");
            assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 4, "{when}: the city stock agrees");
            assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 0, "{when}");
        };
        check(&env, "at once");
        test_utils::tick(&mut env, TICK_SECS);
        check(&env, "after the next step");
        assert_eq!(serde_json::to_value(env.sim.finance()).unwrap(), finance_before, "no gold moves");

        let notice = risks::take_notices().pop().unwrap();
        assert!(notice.starts_with("The storage yard burnt down. Lost: "), "{notice}");
        assert!(notice.contains("20 wood") && notice.contains("12 bricks"), "{notice}");
    });
}

// A market's runner on its way to a granary that collapses comes back empty.
fn test_a_runner_comes_back_empty_from_a_collapsed_granary() {
    const TICK: Seconds = 1.0;
    let mut env = TestEnvironment::with_preset_map(preset_maps::PRESET_1_MARKET_1_GRANARY);
    let market = test_utils::find_building_id(&env, BuildingKind::Market);
    let granary = test_utils::find_building_id(&env, BuildingKind::Granary);

    let freeze = |env: &mut TestEnvironment, name: &str| {
        let mut value = true;
        let ok = test_utils::find_building_mut(env, market)
            .debug_options()
            .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
        assert!(ok, "the market has no '{name}' debug option");
    };
    freeze(&mut env, "freeze_patrol");
    assert_eq!(test_utils::find_building_mut(&mut env, granary).receive_resources(ResourceKind::Rice, 4), 4);

    test_utils::tick_until(&mut env, 30, TICK, |env| test_utils::find_unit_by_config(env, UnitConfigKey::Runner).is_some());
    let runner = test_utils::find_unit_by_config(&env, UnitConfigKey::Runner).unwrap();
    freeze(&mut env, "freeze_stock_update");
    test_utils::tick_n(&mut env, 2, TICK);

    destroy(&mut env, granary, Destruction::Collapsed);
    assert!(!exists(&env, granary));

    let ticks = test_utils::tick_until(&mut env, 200, TICK, |env| !test_utils::unit_exists(env, runner));
    assert!(ticks < 200, "the runner should come home and despawn");
    assert_eq!(building(&env, market).available_resources(ResourceKind::Rice), 0, "it brought nothing back");
}

// The developer inspector's Collapse button can't despawn the building it's drawing: it asks, and
// the collapse happens when the requests run (Simulation::update, at the start of a frame).
fn test_a_requested_collapse_waits_for_the_next_frame() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");

    destruction::request(house, Destruction::Collapsed);
    assert!(destruction::has_requests());
    assert!(exists(&env, house), "nothing happens until the requests run");

    let context = env.new_sim_context(0.0);
    destruction::run_requests(&context);
    assert!(!destruction::has_requests());
    assert!(!exists(&env, house));
    assert_eq!(rubble_at(&env, Cell::new(10, 11)), Some("0000"));
}

// ----------------------------------------------
// The fire watch and the builders' guild
// ----------------------------------------------

// Sets one of a building's bool debug options, e.g. "freeze_patrol".
fn set_debug_bool(env: &mut TestEnvironment, handle: BuildingKindAndId, name: &str, mut value: bool) {
    let ok = test_utils::find_building_mut(env, handle)
        .debug_options()
        .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
    assert!(ok, "{} has no '{name}' debug option", handle.kind);
}

// What a walker was sent to, if he's a responder (UnitTaskRespond).
fn responder_emergency(env: &TestEnvironment, unit_id: game::unit::UnitId) -> Option<Emergency> {
    test_utils::find_unit(env, unit_id)
        .current_task_as::<UnitTaskRespond>(env.sim.task_manager())
        .map(|task| task.emergency)
}

fn fire_response_is_out(env: &mut TestEnvironment, handle: BuildingKindAndId) -> bool {
    test_utils::find_building_mut(env, handle).active_runner().unwrap().is_spawned_or_pending_spawn()
}

// The fire watch sends fire watchers, the builders' guild peasants, and both patrol every
// building, while other services still visit houses only.
fn test_fire_watch_and_guild_configs() {
    let configs = BuildingConfigs::get();
    let fire_watch = configs.find_service_config(BuildingKind::FireWatch);
    assert_eq!(fire_watch.patrol_unit, UnitConfigKey::FireWatcher);
    assert_eq!(fire_watch.visits, PatrolVisits::AllBuildings);
    assert!(fire_watch.has_patrol_unit);

    let guild = configs.find_service_config(BuildingKind::BuildersGuild);
    assert_eq!(guild.patrol_unit, UnitConfigKey::Peasant, "a builder is a peasant from the guild");
    assert_eq!(guild.visits, PatrolVisits::AllBuildings);

    let police = configs.find_service_config(BuildingKind::PoliceStation);
    assert_eq!(police.visits, PatrolVisits::Houses);
    assert!(PatrolVisits::Houses.building_kinds() == BuildingKind::House);
    assert!(PatrolVisits::AllBuildings.building_kinds().contains(BuildingKind::StorageYard | BuildingKind::House));
}

// A patrol from either building visits a storage yard by the road it walks, and resets the risk
// it looks after. The yard sits across the road from the patrol's home, so the walker passes it
// at least on coming home.
fn test_their_patrols_visit_every_building() {
    for (home, kind, walker, risk_kind) in [
        ("fire_watch", BuildingKind::FireWatch, UnitConfigKey::FireWatcher, RiskKind::Fire),
        ("builders_guild", BuildingKind::BuildersGuild, UnitConfigKey::Peasant, RiskKind::Collapse),
    ] {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let service = test_utils::spawn_building(&mut env, Cell::new(10, 11), home);
        assert!(service.kind == kind, "{home} should be a {kind}");
        let road_link = building(&env, service).road_link().expect("road-linked");
        // Three cells north of the road, its south edge on the road link's cell row.
        let yard = test_utils::spawn_building(&mut env, Cell::new(road_link.x - 1, 7), "storage_yard");
        assert!(building(&env, yard).cell_range().contains(Cell::new(road_link.x, 9)), "the yard should touch the road link");
        test_utils::find_building_mut(&mut env, yard).risks_mut().set(risk_kind, 50.0);

        test_utils::tick_until(&mut env, 200, TICK_SECS, |env| test_utils::find_unit_by_config(env, walker).is_some());
        let walker_id = test_utils::find_unit_by_config(&env, walker).unwrap();
        {
            let unit = test_utils::find_unit(&env, walker_id);
            let task = unit.current_task_as::<UnitTaskRandomizedPatrol>(env.sim.task_manager()).expect("a patrol");
            assert!(task.buildings_to_visit == Some(BuildingKind::all()), "{home}'s patrol visits every building");
        }

        // One patrol is enough: the walker that's out finishes it, and no other sets out.
        set_debug_bool(&mut env, service, "freeze_patrol", true);
        test_utils::tick_until(&mut env, 600, TICK_SECS, |env| !test_utils::unit_exists(env, walker_id));
        assert_eq!(risk(&env, yard, risk_kind), 0.0, "{home}'s walker resets the yard's {risk_kind} risk");
    }
}

// A fire watch sends a fire watcher to a fire within its radius, who puts it out before it burns
// down and comes home. Its patrol walker is held back, so the fire is the dispatch's.
fn test_a_fire_watch_sends_a_walker_to_a_fire() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let fire_watch = test_utils::spawn_building(&mut env, Cell::new(5, 11), "fire_watch");
        set_debug_bool(&mut env, fire_watch, "freeze_patrol", true);
        let house = test_utils::spawn_building(&mut env, Cell::new(20, 11), "house0");

        top_out(&mut env, house, RiskKind::Fire);
        assert!(is_burning(&env, house));
        test_utils::tick(&mut env, TICK_SECS);
        assert!(fire_response_is_out(&mut env, fire_watch), "the fire watch sends someone");

        test_utils::tick_until(&mut env, (burn_secs() / TICK_SECS) as usize, TICK_SECS, |env| !is_burning(env, house));
        assert!(exists(&env, house), "the fire is out before the house burns down");
        assert_eq!(risk(&env, house, RiskKind::Fire), 0.0);
        let walker = test_utils::find_unit_by_config(&env, UnitConfigKey::FireWatcher).expect("the fire watcher");
        assert_eq!(
            responder_emergency(&env, walker),
            Some(Emergency::Fire),
            "the fire watcher who put it out was sent to it"
        );

        test_utils::tick_until(&mut env, 200, TICK_SECS, |env| !test_utils::unit_exists(env, walker));
        assert!(!fire_response_is_out(&mut env, fire_watch), "back home, ready for the next fire");
        let notices = risks::take_notices();
        assert_eq!(notices.last().map(String::as_str), Some("The fire at the house is out."));
    });
}

// A fire further than the fire watch's effect radius gets no one.
fn test_a_fire_out_of_reach_gets_no_walker() {
    with_risks(|| {
        let mut env = TestEnvironment::with_map_size(common::Size::new(48, 24));
        let cells: Vec<Cell> = (1..47).map(|x| Cell::new(x, 10)).collect();
        test_utils::place_road(&mut env, &cells);
        let fire_watch = test_utils::spawn_building(&mut env, Cell::new(2, 11), "fire_watch");
        set_debug_bool(&mut env, fire_watch, "freeze_patrol", true);
        let radius = BuildingConfigs::get().find_service_config(BuildingKind::FireWatch).effect_radius;
        let house = test_utils::spawn_building(&mut env, Cell::new(2 + radius + 4, 11), "house0");

        top_out(&mut env, house, RiskKind::Fire);
        test_utils::tick_n(&mut env, 4, TICK_SECS);
        assert!(!fire_response_is_out(&mut env, fire_watch), "too far");
        assert!(is_burning(&env, house));
    });
}

// A fire watcher who puts out one fire goes on to the next within his fire watch's reach before
// coming home, so one walker puts out both.
fn test_a_fire_watcher_goes_on_to_the_next_fire() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let fire_watch = test_utils::spawn_building(&mut env, Cell::new(5, 11), "fire_watch");
        set_debug_bool(&mut env, fire_watch, "freeze_patrol", true);
        let near = test_utils::spawn_building(&mut env, Cell::new(16, 11), "house0");
        let far = test_utils::spawn_building(&mut env, Cell::new(22, 11), "house0");

        for house in [near, far] {
            test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Fire, MAX_RISK);
        }
        test_utils::tick(&mut env, TICK_SECS);
        assert!(is_burning(&env, near) && is_burning(&env, far));
        test_utils::tick(&mut env, TICK_SECS);
        let walker = test_utils::find_unit_by_config(&env, UnitConfigKey::FireWatcher).expect("a fire watcher sent out");

        test_utils::tick_until(&mut env, (burn_secs() / TICK_SECS) as usize, TICK_SECS, |env| !is_burning(env, near));
        assert!(exists(&env, near), "the nearer fire is out first");
        assert!(is_burning(&env, far), "the other still burns");
        assert!(test_utils::unit_exists(&env, walker), "the walker didn't go home");

        test_utils::tick_until(&mut env, (burn_secs() / TICK_SECS) as usize, TICK_SECS, |env| !is_burning(env, far));
        assert!(exists(&env, far), "the walker put out the second fire before it burnt down");
        assert!(test_utils::unit_exists(&env, walker), "the same walker");
        assert_eq!(test_utils::find_unit_by_config(&env, UnitConfigKey::FireWatcher), Some(walker), "and no other");

        test_utils::tick_until(&mut env, 200, TICK_SECS, |env| !test_utils::unit_exists(env, walker));
        assert!(!fire_response_is_out(&mut env, fire_watch), "back home once no fire is left");
        let notices = risks::take_notices();
        assert_eq!(notices.iter().filter(|notice| notice.as_str() == "The fire at the house is out.").count(), 2);
    });
}

// A fire watcher walks over land to a building with no road, and home again.
fn test_a_fire_watcher_walks_off_the_road_to_a_fire() {
    with_risks(|| {
        let mut env = TestEnvironment::with_terrain(TestEnvironment::DEFAULT_MAP_SIZE, test_utils::find_terrain_def("grass"));
        road(&mut env);
        let fire_watch = test_utils::spawn_building(&mut env, Cell::new(5, 11), "fire_watch");
        set_debug_bool(&mut env, fire_watch, "freeze_patrol", true);
        // Six cells off the road, with nothing but grass in between.
        let house = test_utils::spawn_building(&mut env, Cell::new(12, 16), "house0");
        assert!(building(&env, house).road_link().is_none(), "no road reaches it");

        top_out(&mut env, house, RiskKind::Fire);
        test_utils::tick(&mut env, TICK_SECS);
        assert!(fire_response_is_out(&mut env, fire_watch), "the fire watch sends someone");
        let walker = test_utils::find_unit_by_config(&env, UnitConfigKey::FireWatcher).expect("the fire watcher");

        test_utils::tick_until(&mut env, (burn_secs() / TICK_SECS) as usize, TICK_SECS, |env| !is_burning(env, house));
        assert!(exists(&env, house), "the fire is out before the house burns down");

        test_utils::tick_until(&mut env, 200, TICK_SECS, |env| !test_utils::unit_exists(env, walker));
        assert!(!fire_response_is_out(&mut env, fire_watch), "he found his way home");
    });
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
    assert_eq!(keys, [
        "burning",
        "collapse",
        "disease",
        "disease_check_timer",
        "fire",
        "has_food",
        "near_apothecary",
        "near_water",
        "outbreak",
        "water_check_timer",
    ]);

    let mut loaded: BuildingRisks = serde_json::from_value(saved).unwrap();
    loaded.post_load(cell, BuildingKind::House, configs);
    assert_eq!(loaded.get(RiskKind::Fire), 42.5);
    assert_eq!(loaded.get(RiskKind::Collapse), 17.25);
    assert!(loaded.is_near_water());
    assert!(!loaded.is_burning());
    assert_eq!(loaded.jitter(), risks.jitter());

    let rates = RiskRates { fire: 8.0, collapse: 4.0, disease: 3.0 };
    assert_eq!(loaded.effective_rates(&rates, configs).fire, risks.effective_rates(&rates, configs).fire);

    // An outbreak is saved with how long it has run, when the next deaths come, the share that
    // die then, and the deaths so far.
    let mut sick = BuildingRisks::new(cell, BuildingKind::House, configs);
    sick.set_disease_needs(true, false);
    sick.start_outbreak(configs);
    sick.outbreak_mut().unwrap().secs = 41.5;
    sick.outbreak_mut().unwrap().deaths = 3;
    let mut loaded: BuildingRisks = serde_json::from_value(serde_json::to_value(&sick).unwrap()).unwrap();
    loaded.post_load(cell, BuildingKind::House, configs);
    let outbreak = loaded.outbreak().expect("still sick");
    assert_eq!((outbreak.secs, outbreak.deaths), (41.5, 3));
    assert_eq!(outbreak.next_death_secs, configs.death_grace_secs);
    assert_eq!(loaded.get(RiskKind::Disease), MAX_RISK);
    assert!(loaded.is_near_apothecary() && !loaded.has_food());

    // A fire is saved with the time it has left.
    risks.start_burning(12.5);
    let mut loaded: BuildingRisks = serde_json::from_value(serde_json::to_value(&risks).unwrap()).unwrap();
    loaded.post_load(cell, BuildingKind::House, configs);
    assert_eq!(loaded.burning().map(|burning| burning.secs_left), Some(12.5));
    assert_eq!(loaded.get(RiskKind::Fire), MAX_RISK);

    let city: CityRisks = serde_json::from_str(r#"{ "enabled": false, "disease": false, "disease_deaths": 3 }"#).unwrap();
    assert!(!city.is_enabled());
    assert!(!city.is_disease_enabled());
    assert_eq!(city.disease_deaths(), 3);
    assert!(serde_json::from_str::<CityRisks>(r#"{ "enabled": false }"#).is_err(), "a v7 city's risks need migrating");
}

// ----------------------------------------------
// Disease
// ----------------------------------------------

// Runs `test` with risk configs changed by `change`, then puts them back.
fn with_risk_configs<C: FnOnce(&mut RiskConfigs), F: FnOnce()>(change: C, test: F) {
    let saved = serde_json::to_value(&GameConfigs::get().sim.risks).unwrap();
    change(&mut GameConfigs::get_mut().sim.risks);
    test();
    GameConfigs::get_mut().sim.risks = serde_json::from_value(saved).unwrap();
}

fn risk_configs() -> &'static RiskConfigs {
    &GameConfigs::get().sim.risks
}

// Sets one of a house's debug freezes, so only the risks change it.
fn freeze_house(env: &mut TestEnvironment, house: BuildingKindAndId) {
    for name in ["freeze_upgrade_update", "freeze_population_update", "freeze_stock_update", "freeze_deprivation"] {
        set_debug_bool(env, house, name, true);
    }
}

// A house of `tile` at `cell` with `population` residents, frozen but for its risks.
fn house_with(env: &mut TestEnvironment, cell: Cell, tile: &str, population: u32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, cell, tile);
    freeze_house(env, house);
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    let added = context.find_building_mut(house.kind, house.id).unwrap().add_population(cmds, &context, population);
    cmds.execute(&context);
    assert_eq!(added, population, "the house should take {population}");
    house
}

fn population(env: &TestEnvironment, house: BuildingKindAndId) -> u32 {
    building(env, house).population_count()
}

fn is_sick(env: &TestEnvironment, house: BuildingKindAndId) -> bool {
    building(env, house).has_outbreak()
}

// Makes a house sick at once, as risks.infect does.
fn infect(env: &mut TestEnvironment, house: BuildingKindAndId) {
    let context = env.new_sim_context(0.0);
    let building = context.find_building_mut(house.kind, house.id).unwrap();
    assert!(building.start_outbreak_now(&context), "the house should fall sick");
}

// A house's disease risk builds up at its level's rate times its jitter and how full it is,
// three times as fast without water or food, and half as fast again with an apothecary.
fn test_disease_builds_up_with_a_houses_needs() {
    with_risk_configs(|configs| configs.outbreak_chance_at_max = 0.0, || with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);
        let empty = house_with(&mut env, Cell::new(24, 11), "house2", 0);

        let base = building(&env, house).configs().risk_rates().disease;
        assert!(base > 0.0, "a level 2 house falls sick");
        assert!(*building(&env, house).configs().risk_rates() == *building(&env, empty).configs().risk_rates());
        let jitter = building(&env, house).risks().jitter();

        months(&mut env, 1.0);
        let b = building(&env, house);
        assert!(!b.risks().is_near_water() && !b.risks().has_food() && !b.risks().is_near_apothecary());
        assert_close(b.risk_rates().disease, base * jitter * 3.0, "rate without water or food");
        assert_close(risk(&env, house, RiskKind::Disease), base * jitter * 3.0, "disease after a month without either");
        assert_eq!(risk(&env, empty, RiskKind::Disease), 0.0, "an empty house builds up none");

        // Food, a well and an apothecary beside it: found on the next check.
        test_utils::find_building_mut(&mut env, house).receive_resources(ResourceKind::Rice, 5);
        test_utils::spawn_building(&mut env, Cell::new(18, 11), "small_well");
        test_utils::spawn_building(&mut env, Cell::new(23, 11), "apothecary");
        secs(&mut env, risk_configs().water_check_secs + 1.0);
        let b = building(&env, house);
        assert!(b.risks().is_near_water() && b.risks().has_food() && b.risks().is_near_apothecary(), "needs met");
        let cared_for = base * jitter * risk_configs().apothecary_scale;
        assert_close(b.risk_rates().disease, cared_for, "rate with water, food and an apothecary");

        test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Disease, 0.0);
        months(&mut env, 1.0);
        assert_close(risk(&env, house, RiskKind::Disease), cared_for, "disease after a month with all three");

        // Half full, half the rate.
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        context.find_building_mut(house.kind, house.id).unwrap().remove_population(cmds, &context, 6);
        cmds.execute(&context);
        assert_close(building(&env, house).risk_rates().disease, cared_for * 0.5, "half full");
        while let Some(settler) = test_utils::find_unit_by_config(&env, UnitConfigKey::Settler) {
            despawn_walker(&mut env, settler);
        }

        // Other buildings never fall sick.
        let yard = test_utils::spawn_building(&mut env, Cell::new(4, 11), "storage_yard");
        months(&mut env, 1.0);
        assert_eq!(risk(&env, yard, RiskKind::Disease), 0.0);
        assert!(!building(&env, yard).can_fall_sick());
    }));
}

// The chance per roll: the monthly chance, outbreak_chance_at_max times (risk / 100) squared,
// spread over a month of rolls.
fn test_the_outbreak_chance() {
    let configs = risk_configs();
    let month = seconds_per_month();
    let check = configs.disease_check_secs;
    assert_close(risks::outbreak_chance(MAX_RISK, month, configs), configs.outbreak_chance_at_max, "a month at the top");
    assert_close(risks::outbreak_chance(50.0, month, configs), configs.outbreak_chance_at_max * 0.25, "a month at 50");
    assert_eq!(risks::outbreak_chance(0.0, check, configs), 0.0, "no risk, no chance");

    // A month of rolls adds up to the monthly chance.
    let per_roll = risks::outbreak_chance(MAX_RISK, check, configs);
    let rolls = month / check;
    assert_close(1.0 - (1.0 - per_roll).powf(rolls), configs.outbreak_chance_at_max, "a month of rolls");
    assert!(per_roll < configs.outbreak_chance_at_max);
}

// With a sure chance, a house at the top falls sick on its next roll: drawn sick, announced,
// with an alert. An empty one doesn't.
fn test_a_house_at_risk_falls_sick() {
    with_risk_configs(|configs| configs.outbreak_chance_at_max = 1.0, || with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);
        let empty = house_with(&mut env, Cell::new(24, 11), "house2", 0);
        for handle in [house, empty] {
            test_utils::find_building_mut(&mut env, handle).risks_mut().set(RiskKind::Disease, MAX_RISK);
        }

        test_utils::tick_until(&mut env, (risk_configs().disease_check_secs / TICK_SECS) as usize + 2, TICK_SECS, |env| {
            is_sick(env, house)
        });
        assert!(is_sick(&env, house), "the house fell sick on its roll");
        test_utils::tick(&mut env, TICK_SECS); // The step's commands set its tile.
        assert!(building_tile_flags(&env, Cell::new(20, 11)).intersects(TileFlags::Diseased));
        assert!(!is_sick(&env, empty), "no one to fall sick");
        assert!(!building(&env, empty).can_fall_sick());
        assert_eq!(risks::take_notices(), ["Sickness has broken out in a house."]);
        assert_eq!(population(&env, house), 12, "no one dies in the grace period");
    }));
}

// A medic on patrol from a hospital resets a house's disease risk, and only that.
fn test_a_medics_patrol_resets_the_disease_risk() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let house = test_utils::spawn_building(&mut env, Cell::new(10, 11), "house0");
    {
        let risks = test_utils::find_building_mut(&mut env, house).risks_mut();
        risks.set(RiskKind::Disease, 70.0);
        risks.set(RiskKind::Fire, 60.0);
    }
    let medic = patrol_walker(&mut env, house, BuildingKind::Hospital);
    assert_eq!(visit(&mut env, house, medic), BuildingVisitResult::Accepted);
    assert_eq!(risk(&env, house, RiskKind::Disease), 0.0, "a medic resets the disease risk");
    assert_eq!(risk(&env, house, RiskKind::Fire), 60.0, "but not the fire risk");
    despawn_walker(&mut env, medic);
}

// No one dies for death_grace_secs. Then a share dies every death_interval_secs, a bigger one
// each time, until no one is left; the outbreak ends then, with its deaths told.
fn test_a_sick_house_loses_residents_after_the_grace_period() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);
        infect(&mut env, house);
        assert!(building_tile_flags(&env, Cell::new(20, 11)).intersects(TileFlags::Diseased), "drawn sick at once");
        let configs = risk_configs();

        secs(&mut env, configs.death_grace_secs - 1.0);
        assert_eq!(population(&env, house), 12, "no one dies in the grace period");

        // 10% of 12, then 15% of 11, 20% of 9, ..., up to 30%; at least one each time.
        let expected = [11, 9, 7, 5, 3, 2, 1];
        secs(&mut env, 1.5);
        assert_eq!(population(&env, house), expected[0], "the first deaths");
        for left in &expected[1..] {
            secs(&mut env, configs.death_interval_secs);
            assert_eq!(population(&env, house), *left, "deaths every {}s", configs.death_interval_secs);
            assert!(is_sick(&env, house));
        }

        secs(&mut env, configs.death_interval_secs);
        assert_eq!(population(&env, house), 0, "the last one dies");
        test_utils::tick(&mut env, TICK_SECS);
        assert!(!is_sick(&env, house), "with no one left, it's over");
        assert_eq!(risk(&env, house, RiskKind::Disease), MAX_RISK / 2.0, "the next residents start at some risk");
        assert!(!building_tile_flags(&env, Cell::new(20, 11)).intersects(TileFlags::Diseased));
        assert_eq!(env.sim.risks().disease_deaths(), 12);
        assert_eq!(risks::take_notices(), ["Sickness has broken out in a house.", "The sickness at the house is over. 12 people died."]);
        assert!(exists(&env, house), "the house stands, empty");
    });
}

// An apothecary in reach halves the share that die.
fn test_an_apothecary_halves_the_deaths() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house4", 25);
        test_utils::spawn_building(&mut env, Cell::new(25, 11), "apothecary");
        secs(&mut env, risk_configs().water_check_secs + 1.0);
        assert!(building(&env, house).risks().is_near_apothecary());

        infect(&mut env, house);
        secs(&mut env, risk_configs().death_grace_secs + 0.5);
        // 25 x 10% x 0.5 = 1.25: one dies, where 3 would without it (2.5, rounded up).
        assert_eq!(population(&env, house), 24);
        secs(&mut env, risk_configs().death_interval_secs);
        // 24 x 15% x 0.5 = 1.8: two.
        assert_eq!(population(&env, house), 22);
    });
}

// A sick house adds disease risk to every house within disease_spread_cells of it, and to
// nothing else.
fn test_a_sick_house_spreads_disease_to_the_houses_around_it() {
    with_risk_configs(|configs| configs.outbreak_chance_at_max = 0.0, || with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let sick = house_with(&mut env, Cell::new(20, 11), "house2", 12); // x 20..=21, y 11..=12
        let near = house_with(&mut env, Cell::new(23, 12), "house0", 0); // one cell between
        let far = house_with(&mut env, Cell::new(25, 11), "house0", 0); // three cells between
        let yard = test_utils::spawn_building(&mut env, Cell::new(16, 11), "storage_yard"); // touching
        infect(&mut env, sick);

        secs(&mut env, 10.0);
        let spread = risk_configs().disease_spread_per_sec * 10.0;
        assert_close(risk(&env, near, RiskKind::Disease), spread, "the house nearby");
        assert_eq!(risk(&env, far, RiskKind::Disease), 0.0, "out of reach");
        assert_eq!(risk(&env, yard, RiskKind::Disease), 0.0, "not a house");
    }));
}

// A medic's visit cures a sick house: no more deaths, no disease risk, drawn well again.
fn test_a_medic_cures_an_outbreak() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(10, 11), "house2", 12);
        infect(&mut env, house);
        secs(&mut env, risk_configs().death_grace_secs + 0.5);
        assert_eq!(population(&env, house), 11);

        let monk = patrol_walker(&mut env, house, BuildingKind::Temple);
        assert_eq!(visit(&mut env, house, monk), BuildingVisitResult::Refused, "a monk can't cure it");
        assert!(is_sick(&env, house));

        let medic = patrol_walker(&mut env, house, BuildingKind::Hospital);
        assert_eq!(visit(&mut env, house, medic), BuildingVisitResult::Accepted);
        assert!(!is_sick(&env, house), "cured");
        assert_eq!(risk(&env, house, RiskKind::Disease), 0.0);
        assert!(!building_tile_flags(&env, Cell::new(10, 11)).intersects(TileFlags::Diseased));
        for walker in [monk, medic] {
            despawn_walker(&mut env, walker);
        }

        secs(&mut env, risk_configs().death_interval_secs * 3.0);
        assert_eq!(population(&env, house), 11, "no one dies after the cure");
        assert_eq!(risks::take_notices(), ["Sickness has broken out in a house.", "The sickness at the house is over. 1 person died."]);
    });
}

// A hospital sends a medic to an outbreak within its radius, who cures it in the grace period
// and comes home. Its patrol is held back, so the cure is the dispatch's.
fn test_a_hospital_sends_a_medic_to_an_outbreak() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let hospital = test_utils::spawn_building(&mut env, Cell::new(5, 11), "hospital");
        set_debug_bool(&mut env, hospital, "freeze_patrol", true);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);

        infect(&mut env, house);
        test_utils::tick(&mut env, TICK_SECS);
        assert!(fire_response_is_out(&mut env, hospital), "the hospital sends someone");
        let medic = test_utils::find_unit_by_config(&env, UnitConfigKey::Medic).expect("a medic sent out");
        assert_eq!(responder_emergency(&env, medic), Some(Emergency::Outbreak));

        test_utils::tick_until(&mut env, (risk_configs().death_grace_secs / TICK_SECS) as usize, TICK_SECS, |env| {
            !is_sick(env, house)
        });
        assert!(!is_sick(&env, house), "cured before anyone died");
        assert_eq!(population(&env, house), 12);

        test_utils::tick_until(&mut env, 200, TICK_SECS, |env| !test_utils::unit_exists(env, medic));
        assert!(!fire_response_is_out(&mut env, hospital), "back home, ready for the next outbreak");
    });
}

// A sick house that empties for another reason (its residents leave) ends its outbreak too.
fn test_an_empty_sick_house_ends_its_outbreak() {
    with_risks(|| {
        let _ = risks::take_notices();
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house0", 2);
        infect(&mut env, house);

        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        context.find_building_mut(house.kind, house.id).unwrap().remove_population(cmds, &context, 2);
        cmds.execute(&context);
        test_utils::tick_n(&mut env, 2, TICK_SECS);

        assert!(!is_sick(&env, house), "no one left to be sick");
        assert_eq!(risk(&env, house, RiskKind::Disease), MAX_RISK / 2.0);
        // The residents who left, off to look for a home elsewhere.
        while let Some(settler) = test_utils::find_unit_by_config(&env, UnitConfigKey::Settler) {
            despawn_walker(&mut env, settler);
        }
        let notices = risks::take_notices();
        assert_eq!(notices.last().map(String::as_str), Some("The sickness at the house is over."), "no one died of it");
    });
}

// A sick house takes in no settlers, doesn't grow, and isn't merged into a neighbour; a healthy
// one beside it grows.
fn test_a_sick_house_takes_no_one_in_and_doesnt_grow() {
    with_risk_configs(|configs| configs.death_grace_secs = 100_000.0, || with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let sick = house_with(&mut env, Cell::new(20, 11), "house2", 4);
        let well = house_with(&mut env, Cell::new(24, 11), "house2", 4);
        for house in [sick, well] {
            set_debug_bool(&mut env, house, "freeze_population_update", false);
        }
        infect(&mut env, sick);

        // A healthy house would read this settler's task (he has none, so it would panic); the
        // sick one turns him away before that.
        let settler = test_utils::spawn_unit(&mut env, Cell::new(22, 10), UnitConfigKey::Settler);
        assert_eq!(visit(&mut env, sick, settler), BuildingVisitResult::Refused, "no one moves into a sick house");
        despawn_walker(&mut env, settler);
        let well_population = population(&env, well);

        months(&mut env, 5.0);
        assert_eq!(population(&env, sick), 4, "the sick house didn't grow");
        assert!(population(&env, well) > well_population, "the healthy one did");
    }));
}

// A sick house that downgrades keeps its outbreak, and its new tile is drawn sick too.
fn test_a_sick_house_that_downgrades_stays_sick() {
    with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 8);
        infect(&mut env, house);

        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.upgrade_house(house, HouseUpgradeDirection::Downgrade);
        cmds.execute(&context);

        assert!(building(&env, house).as_house().level() == game::building::HouseLevel::Level1, "downgraded");
        assert!(is_sick(&env, house), "still sick");
        let base_cell = building(&env, house).base_cell();
        assert!(building_tile_flags(&env, base_cell).intersects(TileFlags::Diseased), "the new tile shows it");
    });
}

// With disease off for the game, houses build up no disease risk and don't fall sick, while
// fire still builds up.
fn test_disease_can_be_off_while_risks_are_on() {
    with_risk_configs(|configs| configs.outbreak_chance_at_max = 1.0, || with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);
        env.sim.risks_mut().set_disease_enabled(false);
        test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Disease, 60.0);

        months(&mut env, 1.0);
        assert_eq!(risk(&env, house, RiskKind::Disease), 60.0, "no build-up");
        assert!(!is_sick(&env, house), "no outbreak");
        assert!(risk(&env, house, RiskKind::Fire) > 0.0, "fire still builds up");

        env.sim.risks_mut().set_disease_enabled(true);
        test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Disease, MAX_RISK);
        secs(&mut env, risk_configs().disease_check_secs + 1.0);
        assert!(is_sick(&env, house), "on again");
    }));
}

// The cheat stops disease as it stops fires: no build-up and no roll, and an outbreak under way
// waits, with no deaths, until it's off again.
fn test_the_cheat_stops_disease() {
    with_risk_configs(|configs| configs.outbreak_chance_at_max = 1.0, || with_risks(|| {
        let mut env = TestEnvironment::new();
        road(&mut env);
        let house = house_with(&mut env, Cell::new(20, 11), "house2", 12);
        let sick = house_with(&mut env, Cell::new(24, 11), "house2", 12);
        test_utils::find_building_mut(&mut env, house).risks_mut().set(RiskKind::Disease, 60.0);
        infect(&mut env, sick);

        cheats::get_mut().no_city_risks = true;
        assert!(!env.sim.risks().is_disease_active());
        secs(&mut env, risk_configs().death_grace_secs + risk_configs().death_interval_secs * 2.0);
        assert_eq!(risk(&env, house, RiskKind::Disease), 60.0, "no build-up");
        assert!(!is_sick(&env, house), "no roll");
        assert!(is_sick(&env, sick), "the outbreak waits");
        assert_eq!(population(&env, sick), 12, "no one dies while it waits");

        cheats::get_mut().no_city_risks = false;
        secs(&mut env, risk_configs().death_grace_secs + 1.0);
        assert!(population(&env, sick) < 12, "deaths once the cheat is off");
    }));
}

// The summary behind risks.state and the World Debug window's Risks tab.
fn test_the_risk_summary() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let calm = test_utils::spawn_building(&mut env, Cell::new(14, 11), "storage_yard");
    let risky = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
    let burning = test_utils::spawn_building(&mut env, Cell::new(26, 11), "storage_yard");
    let house = house_with(&mut env, Cell::new(2, 11), "house2", 12);
    test_utils::find_building_mut(&mut env, calm).risks_mut().set(RiskKind::Collapse, 30.0);
    test_utils::find_building_mut(&mut env, risky).risks_mut().set(RiskKind::Fire, 80.0);
    test_utils::find_building_mut(&mut env, burning).risks_mut().start_burning(12.0);
    infect(&mut env, house);

    let summary = RiskSummary::of(&env.world, 3);
    assert_eq!(summary.buildings, 4);
    assert_eq!(summary.high(RiskKind::Fire), 2, "the risky yard and the burning one");
    assert_eq!(summary.high(RiskKind::Collapse), 0);
    assert_eq!(summary.high(RiskKind::Disease), 1, "the sick house");
    assert_eq!(summary.highest(RiskKind::Collapse), 30.0);
    assert_eq!(summary.highest(RiskKind::Fire), MAX_RISK);

    // Highest first, ties by cell (y, then x): the house's row before the burning yard's column.
    let riskiest: Vec<BuildingKindAndId> = summary.riskiest.iter().map(|entry| entry.kind_and_id).collect();
    assert!(riskiest == [house, burning, risky], "at most 3, the calm yard left out");
    assert_eq!(summary.fires.len(), 1);
    assert!(summary.fires[0].kind_and_id == burning);
    assert_eq!(summary.fires[0].burning.as_ref().unwrap().secs_left, 12.0);
    assert_eq!(summary.outbreaks.len(), 1);
    assert!(summary.outbreaks[0].kind_and_id == house);
    assert_eq!(summary.outbreaks[0].population, 12);
}

// The Risk lens colours each building by its highest risk, in that risk's colour (ties go to
// fire, then collapse), a burning one and a sick house in their own colours, and its heat field
// is that risk over the building's cells, fading out within a cell or two of them.
fn test_the_risk_lens() {
    assert!(MinimapLens::available().contains(&MinimapLens::Risk), "in the lens bar");

    let mut env = TestEnvironment::new();
    road(&mut env);
    let calm = test_utils::spawn_building(&mut env, Cell::new(14, 11), "storage_yard");
    let risky = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
    let burning = test_utils::spawn_building(&mut env, Cell::new(26, 11), "storage_yard");
    let house = house_with(&mut env, Cell::new(2, 11), "house2", 12);
    let tied = house_with(&mut env, Cell::new(6, 11), "house2", 12);
    let unwell = house_with(&mut env, Cell::new(9, 11), "house2", 12);
    test_utils::find_building_mut(&mut env, burning).risks_mut().start_burning(12.0);
    infect(&mut env, house);
    for (handle, fire, collapse, disease) in [(calm, 0.0, 0.0, 0.0), (risky, 20.0, 50.0, 0.0), (tied, 100.0, 100.0, 100.0), (unwell, 10.0, 0.0, 80.0)] {
        let risks = test_utils::find_building_mut(&mut env, handle).risks_mut();
        risks.set(RiskKind::Fire, fire);
        risks.set(RiskKind::Collapse, collapse);
        risks.set(RiskKind::Disease, disease);
    }

    let style = MinimapStyle::default();
    let mut data = LensData::default();
    let context = env.new_sim_context(0.0);
    let map_size = context.tile_map().size_in_cells();
    let lens_context = LensContext { sim: &context, features: context.tile_map().minimap().features(), map_size, style: &style };
    lens::provider(MinimapLens::Risk).expect("a provider").compute(&lens_context, &mut data);

    let colors = &style.lenses;
    let color_of = |handle: BuildingKindAndId| {
        let base = context.find_building(handle.kind, handle.id).unwrap().base_cell();
        *data.building_colors.get(&(base.y, base.x)).expect("a colour")
    };
    assert_eq!(color_of(calm), colors.risk_low.color(), "no risk");
    assert_eq!(color_of(burning), colors.risk_burning.color());
    assert_eq!(color_of(house), colors.risk_diseased.color());
    let low = colors.risk_low.color();
    let assert_mixed = |handle: BuildingKindAndId, top: common::Color, share: f32, what: &str| {
        let color = color_of(handle);
        for (actual, from, to) in [(color.r, low.r, top.r), (color.g, low.g, top.g), (color.b, low.b, top.b)] {
            assert!((actual - (from + (to - from) * share)).abs() < 1e-4, "{what}");
        }
    };
    assert_mixed(tied, colors.risk_fire.color(), 1.0, "a tie goes to fire");
    assert_mixed(risky, colors.risk_collapse.color(), 0.5, "half way to collapse");
    assert_mixed(unwell, colors.risk_disease.color(), 0.8, "most of the way to disease");

    let heat = |x: i32, y: i32| data.values[(x + y * map_size.width) as usize];
    let range = |handle: BuildingKindAndId| context.find_building(handle.kind, handle.id).unwrap().cell_range();
    let (calm_cells, risky_cells, burning_cells) = (range(calm), range(risky), range(burning));
    assert_eq!(heat(calm_cells.start.x, calm_cells.start.y), 0, "no risk, no heat");
    assert_eq!(heat(risky_cells.start.x, risky_cells.start.y), 128, "half the risk");
    assert_eq!(heat(risky_cells.end.x, risky_cells.end.y), 128, "over the footprint");
    let beside = heat(risky_cells.end.x + 1, risky_cells.start.y);
    assert!(beside > 0 && beside < 128, "fading beside it: {beside}");
    assert_eq!(heat(risky_cells.end.x + 2, risky_cells.start.y), 0, "gone two cells past it");
    assert_eq!(heat(burning_cells.start.x, burning_cells.start.y), 255, "a fire at the top");
    let legend: Vec<&str> = data.legend.iter().map(|(_, label)| *label).collect();
    assert_eq!(legend, ["Low risk", "Fire risk", "Collapse risk", "Disease risk", "On fire", "Sickness"]);
}
