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
        construction::ConstructionMode,
        destruction::Destruction,
        risks::{self, BuildingRisks, MAX_RISK, RiskKind, RiskRates},
    },
    campaign::config::MissionRisksDef,
    cheats,
    config::GameConfigs,
    debug::{game_object_debug::GameObjectDebugVarRef, preset_maps},
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{SimCmdQueue, SimContext, resources::ResourceKind, risks::CityRisks},
    tile::{TileFlags, TileKind, TileMapLayerKind, rubble},
    undo_redo,
    world::city_stock::CityStock,
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
        // Destroyed buildings:
        test_utils::test_fn!(test_commands_for_a_destroyed_building_are_skipped),
        test_utils::test_fn!(test_a_burnt_yard_loses_its_goods_at_once),
        test_utils::test_fn!(test_a_runner_comes_back_empty_from_a_collapsed_granary),
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

    // A fire is saved with the time it has left.
    risks.start_burning(12.5);
    let mut loaded: BuildingRisks = serde_json::from_value(serde_json::to_value(&risks).unwrap()).unwrap();
    loaded.post_load(cell, BuildingKind::House, configs);
    assert_eq!(loaded.burning().map(|burning| burning.secs_left), Some(12.5));
    assert_eq!(loaded.get(RiskKind::Fire), MAX_RISK);

    let city: CityRisks = serde_json::from_str(r#"{ "enabled": false }"#).unwrap();
    assert!(!city.is_enabled());
}
