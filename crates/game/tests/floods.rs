use common::{Size, callback::Callback, coords::Cell};
use game::{
    building::{
        Building,
        BuildingKind,
        BuildingKindAndId,
        BuildingTileInfo,
        construction::ConstructionMode,
        risks::RiskKind,
    },
    campaign::config::{MissionFloodsDef, MissionGoal},
    cheats,
    config::{FloodRules, GameConfigs},
    debug::game_object_debug::GameObjectDebugVarRef,
    faith::events,
    farms::{self, FieldStage},
    floods::{self as river, FloodNotice},
    menu::{PlaceOrClearResult, TilePlacement},
    realm::Realm,
    sim::{
        SimCmdQueue,
        SimContextMut,
        calendar::CalendarDate,
        floods::{self, CityFloods},
        resources::ResourceKind,
    },
    tile::{
        TileFlags,
        TileMapLayerKind,
        elevation::{Dir, Half, Shape},
        minimap::{
            MinimapLens,
            lens::{self, LensContext, LensData},
            style::MinimapStyle,
        },
        road::{self, RoadKind},
        rubble,
        sets::{OBJECTS_FIELDS_CATEGORY, TileDef, TileSets},
        water,
    },
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
    world::stats::WorldStats,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the river's floods, the land's fertility, the dykes, the sluice gates and
// the canals (docs/plans/seasons-floods.md, Phases 0, 2, 3, 4 and 6).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Floods", &[
        test_utils::test_fn!(test_a_new_game_has_no_floods),
        test_utils::test_fn!(test_a_mission_turns_floods_on_and_lays_out_fertility),
        test_utils::test_fn!(test_a_mission_without_floods_keeps_no_fertility),
        test_utils::test_fn!(test_ponds_dont_flood),
        test_utils::test_fn!(test_a_mission_names_its_sources),
        test_utils::test_fn!(test_the_reach_fill),
        test_utils::test_fn!(test_the_reach_fill_stops_where_it_is_blocked),
        test_utils::test_fn!(test_turning_floods_on_and_off),
        test_utils::test_fn!(test_the_mission_block_is_checked),
        test_utils::test_fn!(test_the_state_round_trips_a_save),
        // The flood (Phase 2):
        test_utils::test_fn!(test_the_wet_season_says_the_river_is_rising),
        test_utils::test_fn!(test_the_flood_comes_at_its_month),
        test_utils::test_fn!(test_the_water_goes_after_its_months),
        test_utils::test_fn!(test_a_flood_damages_what_it_reaches),
        test_utils::test_fn!(test_no_damage_while_risks_are_off),
        test_utils::test_fn!(test_the_flood_takes_the_crops_and_the_harvest),
        test_utils::test_fn!(test_the_flood_leaves_silt),
        test_utils::test_fn!(test_a_severe_flood_and_a_gentle_one),
        test_utils::test_fn!(test_faith_sets_the_next_floods_severity),
        test_utils::test_fn!(test_nothing_new_on_land_under_water),
        test_utils::test_fn!(test_a_flood_now_and_the_water_going),
        test_utils::test_fn!(test_turning_floods_off_takes_the_water_away),
        test_utils::test_fn!(test_the_flood_lens),
        test_utils::test_fn!(test_a_flood_under_way_round_trips_a_save),
        // Dykes (Phase 3):
        test_utils::test_fn!(test_a_dyke_is_drawn_as_a_road),
        test_utils::test_fn!(test_dykes_hold_the_flood_back),
        test_utils::test_fn!(test_dykes_wear_month_by_month),
        test_utils::test_fn!(test_a_builder_walking_a_dyke_keeps_it_up),
        test_utils::test_fn!(test_a_worn_dyke_breaches),
        test_utils::test_fn!(test_the_floods_survived_goal),
        test_utils::test_fn!(test_the_flood_lens_shows_the_dykes),
        test_utils::test_fn!(test_dykes_round_trip_a_save),
        // Fertility and sluice gates (Phase 4):
        test_utils::test_fn!(test_a_harvest_wears_the_land),
        test_utils::test_fn!(test_a_sluice_gate_stands_beside_a_dyke),
        test_utils::test_fn!(test_a_sluice_gate_waters_the_dry_season),
        test_utils::test_fn!(test_an_unstaffed_sluice_gate_waters_nothing),
        test_utils::test_fn!(test_a_sluice_gate_lets_the_silt_in),
        test_utils::test_fn!(test_no_silt_where_the_water_doesnt_reach_the_gate),
        test_utils::test_fn!(test_the_fertility_lens),
        // Canals and irrigation as connected water (Phase 6):
        test_utils::test_fn!(test_a_canal_is_drawn_with_the_waters_shores),
        test_utils::test_fn!(test_water_flows_through_fields_and_canals),
        test_utils::test_fn!(test_a_sluice_gate_needs_water_across_its_dyke),
        test_utils::test_fn!(test_dry_canals_are_flagged),
        test_utils::test_fn!(test_an_open_canal_carries_the_flood),
        test_utils::test_fn!(test_the_flood_passes_over_a_canal_it_doesnt_come_from),
        test_utils::test_fn!(test_a_canal_never_makes_water_flood),
        // Terrain height (docs/plans/terrain-height.md, Stage 6):
        test_utils::test_fn!(test_floods_stay_below_their_level),
        test_utils::test_fn!(test_water_doesnt_climb_to_a_plateau),
        test_utils::test_fn!(test_a_sluice_gate_on_high_ground_draws_no_water),
        test_utils::test_fn!(test_a_farmsteads_reach_stops_at_a_cliff),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn rules() -> &'static FloodRules {
    &GameConfigs::get().sim.floods
}

// A 40x40 grass map (its playable area a diamond round (20, 20)) with a river two cells wide along
// x 20 and 21, and a pond of four cells at (30, 22), eight cells east of the river.
fn river_map() -> TestEnvironment {
    let mut env = TestEnvironment::with_terrain(Size::new(40, 40), test_utils::find_terrain_def("grass"));
    let water = test_utils::find_water_def();
    for y in 0..40 {
        for x in [20, 21] {
            if env.tile_map.is_cell_within_playable_area(Cell::new(x, y)) {
                test_utils::place_terrain_by_def(&mut env, Cell::new(x, y), water);
            }
        }
    }
    for cell in [Cell::new(30, 22), Cell::new(31, 22), Cell::new(30, 23), Cell::new(31, 23)] {
        test_utils::place_terrain_by_def(&mut env, cell, water);
    }
    env
}

fn on() -> MissionFloodsDef {
    MissionFloodsDef { enabled: Some(true), ..Default::default() }
}

// The river's seed: its first cell in row order.
fn river(env: &TestEnvironment) -> Cell {
    let seeds = floods::water_bodies(&env.tile_map, 30);
    assert_eq!(seeds.len(), 1, "{seeds:?}");
    assert_eq!(seeds[0].x, 20);
    seeds[0]
}

// Floods on, each year's reaching exactly `reach` cells from the river's shores.
fn flooding_map(reach: u32) -> TestEnvironment {
    let mut env = river_map();
    let mission = MissionFloodsDef { enabled: Some(true), reach: Some([reach, reach]), ..Default::default() };
    let tile_map = &env.tile_map;
    env.sim.floods_mut().start(true, &mission, rules(), tile_map);
    river::take_notices();
    env
}

// The start of `month` in year 1, as the simulation runs it: the river, then the fields.
fn month_start(env: &mut TestEnvironment, month: u32) {
    let date = CalendarDate { year: 1, month, day: 1 };
    let context = env.new_sim_context(0.0);
    river::on_new_month(&context, date);
    farms::on_new_month(&context, date);
}

fn months(env: &mut TestEnvironment, from: u32, to: u32) {
    for month in from..=to {
        month_start(env, month);
    }
}

fn is_flagged(env: &TestEnvironment, cell: Cell) -> bool {
    env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.has_flags(TileFlags::Flooded))
}

// Runs `test` with risks building up (the harness's `no_city_risks` cheat off).
fn with_risks<F: FnOnce()>(test: F) {
    cheats::get_mut().no_city_risks = false;
    test();
    cheats::get_mut().no_city_risks = true;
}

fn risk(env: &TestEnvironment, building: BuildingKindAndId, kind: RiskKind) -> f32 {
    test_utils::find_building(env, building).risks().get(kind)
}

fn field_def() -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_name(TileMapLayerKind::Objects, OBJECTS_FIELDS_CATEGORY.string, "rice_field")
        .expect("the rice field's tile def")
}

fn place(env: &mut TestEnvironment, cell: Cell, tile_def: &'static TileDef) -> PlaceOrClearResult {
    let context = env.new_sim_context(0.0);
    TilePlacement::place(&context, cell, tile_def, false, false, ConstructionMode::Finished)
}

fn farmstead(env: &mut TestEnvironment, cell: Cell) -> BuildingKindAndId {
    assert!(place(env, cell, test_utils::find_building_def("farmstead")).is_ok(), "a farmstead at {cell}");
    env.world.find_building_for_cell(cell, &env.tile_map).expect("the farmstead").kind_and_id()
}

fn field_stage(env: &TestEnvironment, cell: Cell) -> usize {
    env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).expect("a field").variation_index()
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

fn test_a_new_game_has_no_floods() {
    let env = river_map();
    let city_floods = env.sim.floods();
    assert!(!city_floods.is_enabled());
    assert!(city_floods.fertility().is_empty(), "no fertility kept");
    assert_eq!(city_floods.fertility_at(Cell::new(5, 20)), 50, "the neutral fertility");
    assert_eq!(city_floods.yield_factor_at(Cell::new(5, 20)), 1.0);
    assert_eq!(city_floods.profile().month, 7);
    assert_eq!(city_floods.profile().reach, [3, 6]);
    assert!(city_floods.flood().is_none());
    assert_eq!(city_floods.floods_survived(), 0);
}

fn test_a_mission_turns_floods_on_and_lays_out_fertility() {
    let mut env = river_map();
    let source = river(&env);
    let tile_map = &env.tile_map;
    env.sim.floods_mut().start(true, &on(), rules(), tile_map);
    let city_floods = env.sim.floods();
    assert!(city_floods.is_enabled());
    assert_eq!(city_floods.profile().sources, vec![source], "the river, not the pond");

    // Within the usual reach (6 cells from the river's shores) 70; beyond it 40.
    assert_eq!(city_floods.fertility_at(Cell::new(19, 20)), 70, "on the shore");
    assert_eq!(city_floods.fertility_at(Cell::new(14, 20)), 70, "6 cells west");
    assert_eq!(city_floods.fertility_at(Cell::new(13, 20)), 40, "7 cells west");
    assert_eq!(city_floods.fertility_at(Cell::new(27, 20)), 70, "6 cells east");
    assert_eq!(city_floods.fertility_at(Cell::new(28, 20)), 40, "7 cells east");
    assert_eq!(city_floods.fertility_at(Cell::new(29, 22)), 40, "by the pond");
    assert_eq!(city_floods.yield_factor_at(Cell::new(19, 20)), 1.2);
    assert_eq!(city_floods.yield_factor_at(Cell::new(29, 22)), 0.9);
}

fn test_a_mission_without_floods_keeps_no_fertility() {
    let mut env = river_map();
    let tile_map = &env.tile_map;
    let off = MissionFloodsDef { enabled: Some(false), ..Default::default() };
    env.sim.floods_mut().start(false, &off, rules(), tile_map);
    assert!(!env.sim.floods().is_enabled());
    assert!(env.sim.floods().fertility().is_empty());
    assert!(!env.sim.floods_mut().set_fertility(Cell::new(19, 20), 90), "nothing to set");
}

fn test_ponds_dont_flood() {
    let env = river_map();
    let source = river(&env);
    assert_eq!(floods::water_bodies(&env.tile_map, 4), vec![source, Cell::new(30, 22)]);
}

fn test_a_mission_names_its_sources() {
    let mut env = river_map();
    let tile_map = &env.tile_map;
    let mission = MissionFloodsDef {
        enabled: Some(true),
        sources: Some(vec![[30, 22], [5, 20]]),
        reach: Some([1, 2]),
        fertility: Some([90, 20]),
        ..Default::default()
    };
    env.sim.floods_mut().start(true, &mission, rules(), tile_map);
    let city_floods = env.sim.floods();
    assert_eq!(city_floods.profile().sources, vec![Cell::new(30, 22)], "(5, 20) is land: dropped");
    assert_eq!(city_floods.profile().reach, [1, 2]);
    assert_eq!(city_floods.fertility_at(Cell::new(28, 22)), 90, "2 cells from the pond");
    assert_eq!(city_floods.fertility_at(Cell::new(27, 22)), 20);
    assert_eq!(city_floods.fertility_at(Cell::new(19, 20)), 20, "the river doesn't flood");
}

fn test_the_reach_fill() {
    let env = river_map();
    let seeds = [river(&env)];
    let reached = floods::reach_fill(&env.tile_map, &seeds, 2, 0, |_| false);
    for cell in [Cell::new(18, 20), Cell::new(19, 20), Cell::new(22, 20), Cell::new(23, 20)] {
        assert!(reached.contains(&cell), "{cell}");
    }
    for cell in [Cell::new(17, 20), Cell::new(24, 20), Cell::new(20, 20)] {
        assert!(!reached.contains(&cell), "{cell}: past 2, or water");
    }
    assert!(reached[0].x == 19 || reached[0].x == 22, "nearest first: {}", reached[0]);
    assert!(floods::reach_fill(&env.tile_map, &seeds, 0, 0, |_| false).is_empty());
    assert!(floods::reach_fill(&env.tile_map, &[Cell::new(5, 20)], 3, 0, |_| false).is_empty(), "a land seed");
}

fn test_the_reach_fill_stops_where_it_is_blocked() {
    let env = river_map();
    // A wall along x = 18: the west bank floods one cell, the east bank three.
    let reached = floods::reach_fill(&env.tile_map, &[river(&env)], 3, 0, |cell| cell.x == 18);
    assert!(reached.contains(&Cell::new(19, 20)));
    assert!(!reached.contains(&Cell::new(18, 20)) && !reached.contains(&Cell::new(17, 20)));
    assert!(reached.contains(&Cell::new(24, 20)));
}

fn test_turning_floods_on_and_off() {
    let mut env = river_map();
    let tile_map = &env.tile_map;
    let mission = MissionFloodsDef { enabled: Some(true), reach: Some([2, 4]), ..Default::default() };
    env.sim.floods_mut().start(true, &mission, rules(), tile_map);
    env.sim.floods_mut().set_enabled(false, rules(), tile_map);
    assert!(!env.sim.floods().is_enabled());
    assert!(env.sim.floods().fertility().is_empty());
    assert_eq!(env.sim.floods().profile().reach, [2, 4], "keeps its flood");

    env.sim.floods_mut().set_enabled(true, rules(), tile_map);
    assert!(env.sim.floods().is_enabled());
    assert_eq!(env.sim.floods().fertility_at(Cell::new(25, 20)), 70, "laid out again, 4 cells east");
    assert_eq!(env.sim.floods().fertility_at(Cell::new(26, 20)), 40);
    assert!(env.sim.floods_mut().set_fertility(Cell::new(26, 20), 99));
    assert_eq!(env.sim.floods().fertility_at(Cell::new(26, 20)), 99);
    env.sim.floods_mut().set_next_severity(1.5);
    assert_eq!(env.sim.floods().next_severity(), 1.5);
}

fn test_the_mission_block_is_checked() {
    let wet = [5, 6, 7, 8, 9, 10];
    assert!(on().problems(&wet).is_empty());
    let bad = MissionFloodsDef {
        month: Some(2),
        reach: Some([4, 2]),
        months: Some(0),
        fertility: Some([120, 30]),
        ..Default::default()
    };
    assert_eq!(bad.problems(&wet).len(), 4, "{:?}", bad.problems(&wet));
    let zero = MissionFloodsDef { reach: Some([0, 3]), ..Default::default() };
    assert_eq!(zero.problems(&wet).len(), 1);
}

fn test_the_state_round_trips_a_save() {
    let mut env = river_map();
    let source = river(&env);
    let tile_map = &env.tile_map;
    env.sim.floods_mut().start(true, &on(), rules(), tile_map);
    env.sim.floods_mut().set_fertility(Cell::new(35, 20), 13);
    let json = serde_json::to_string(env.sim.floods()).unwrap();
    assert!(json.len() < 4_000, "the grid is saved as runs: {} bytes", json.len());
    let back: CityFloods = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    assert_eq!(back.fertility_at(Cell::new(35, 20)), 13);
    assert_eq!(back.fertility_at(Cell::new(19, 20)), 70);
    assert_eq!(back.profile().sources, vec![source]);
}

// ----------------------------------------------
// The flood (Phase 2)
// ----------------------------------------------

fn test_the_wet_season_says_the_river_is_rising() {
    let mut env = flooding_map(3);
    month_start(&mut env, 4);
    assert!(river::take_notices().is_empty(), "the dry season");
    month_start(&mut env, 5);
    let notices = river::take_notices();
    assert!(matches!(notices.as_slice(), [FloodNotice::RiverRising]), "{notices:?}");
    month_start(&mut env, 6);
    assert!(river::take_notices().is_empty(), "once, as the season begins");

    // A game whose river doesn't flood says nothing.
    let mut dry = river_map();
    month_start(&mut dry, 5);
    assert!(river::take_notices().is_empty());
}

fn test_the_flood_comes_at_its_month() {
    let mut env = flooding_map(3);
    months(&mut env, 5, 6);
    assert!(env.sim.floods().flood().is_none(), "not before its month");
    river::take_notices();

    month_start(&mut env, 7);
    let city_floods = env.sim.floods();
    let flood = city_floods.flood().expect("the flood in month 7");
    let expected = floods::reach_fill(&env.tile_map, &city_floods.profile().sources, 3, 0, |_| false);
    assert_eq!(flood.cells.len(), expected.len());
    assert_eq!(flood.months_left, 2);
    assert_eq!(flood.severity, 1.0);
    for cell in [Cell::new(17, 20), Cell::new(19, 20), Cell::new(22, 20), Cell::new(24, 20)] {
        assert!(city_floods.is_flooded(cell) && is_flagged(&env, cell), "{cell} under water");
    }
    for cell in [Cell::new(16, 20), Cell::new(25, 20)] {
        assert!(!city_floods.is_flooded(cell) && !is_flagged(&env, cell), "{cell}: past 3");
    }
    // The river's own cells beside the flood are drawn under it (their banks hidden), but aren't land
    // under water.
    assert!(!city_floods.is_flooded(Cell::new(20, 20)) && is_flagged(&env, Cell::new(20, 20)));

    let log = city_floods.log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].ordinal_month, 12 + 6, "year 1, month 7");
    assert_eq!(log[0].reach, 3);
    assert_eq!(log[0].cells as usize, expected.len());
    let notices = river::take_notices();
    assert!(matches!(notices.as_slice(), [FloodNotice::Flood { buildings_damaged: 0, .. }]), "{notices:?}");
}

fn test_the_water_goes_after_its_months() {
    let mut env = flooding_map(3);
    month_start(&mut env, 7);
    month_start(&mut env, 8);
    assert_eq!(env.sim.floods().flood().map(|flood| flood.months_left), Some(1), "still under water");
    assert!(is_flagged(&env, Cell::new(18, 20)));
    river::take_notices();

    month_start(&mut env, 9);
    assert!(env.sim.floods().flood().is_none(), "gone at the start of month 9");
    assert!(!is_flagged(&env, Cell::new(18, 20)));
    assert!(!is_flagged(&env, Cell::new(20, 20)), "the river's banks show again");
    let notices = river::take_notices();
    assert!(matches!(notices.as_slice(), [FloodNotice::Receded { fields: 0 }]), "{notices:?}");

    // One a year: the next comes in month 7 of year 2.
    month_start(&mut env, 10);
    assert!(env.sim.floods().flood().is_none());
    assert_eq!(env.sim.floods().log().len(), 1);
}

fn test_a_flood_damages_what_it_reaches() {
    with_risks(|| {
        let mut env = flooding_map(3);
        let near = test_utils::spawn_building(&mut env, Cell::new(17, 18), "house2");
        let far = test_utils::spawn_building(&mut env, Cell::new(10, 18), "house2");
        let (collapse, disease) = (risk(&env, near, RiskKind::Collapse), risk(&env, near, RiskKind::Disease));

        month_start(&mut env, 7);
        let rules = rules();
        assert_eq!(risk(&env, near, RiskKind::Collapse), collapse + rules.collapse_damage, "collapse risk");
        assert_eq!(risk(&env, near, RiskKind::Disease), disease + rules.disease_damage, "a house's disease risk");
        assert_eq!(risk(&env, far, RiskKind::Collapse), 0.0, "beyond the reach");
        assert_eq!(env.sim.floods().log()[0].buildings_damaged, 1);
        let notices = river::take_notices();
        assert!(matches!(notices.as_slice(), [FloodNotice::Flood { buildings_damaged: 1, .. }]), "{notices:?}");
    });
}

fn test_no_damage_while_risks_are_off() {
    let mut env = flooding_map(3);
    let near = test_utils::spawn_building(&mut env, Cell::new(17, 18), "house2");
    month_start(&mut env, 7);
    assert_eq!(risk(&env, near, RiskKind::Collapse), 0.0, "the harness's no_city_risks");
    assert_eq!(env.sim.floods().log()[0].buildings_damaged, 0);
    assert!(env.sim.floods().is_flooded(Cell::new(17, 18)), "the water comes all the same");
}

fn test_the_flood_takes_the_crops_and_the_harvest() {
    let mut env = flooding_map(3);
    // A farmstead west of the flood with a field in it and one out of it; another in the flood.
    let dry = farmstead(&mut env, Cell::new(12, 24));
    let wet = farmstead(&mut env, Cell::new(17, 14));
    for cell in [Cell::new(17, 24), Cell::new(14, 24)] {
        assert!(place(&mut env, cell, field_def()).is_ok(), "a field at {cell}");
    }
    months(&mut env, 5, 6);
    let sown = |env: &TestEnvironment, cell: Cell| {
        test_utils::find_building(env, dry)
            .as_producer()
            .fields()
            .iter()
            .any(|plot| plot.cell == cell && plot.crop.is_some())
    };
    assert!(sown(&env, Cell::new(17, 24)) && sown(&env, Cell::new(14, 24)), "both sown in month 5");
    let stored =
        test_utils::find_building_mut(&mut env, wet).as_producer_mut().add_production_output_stock(ResourceKind::Rice, 20);
    assert!(stored);

    month_start(&mut env, 7);
    assert!(!sown(&env, Cell::new(17, 24)), "the flood took its crop");
    assert_eq!(field_stage(&env, Cell::new(17, 24)), FieldStage::Fallow.variation());
    assert!(sown(&env, Cell::new(14, 24)), "the dry field grows on");
    assert_eq!(test_utils::find_building(&env, wet).available_resources(ResourceKind::Rice), 0, "its harvest lost");
    assert_eq!(env.sim.floods().log()[0].fields_flooded, 1);

    month_start(&mut env, 8);
    assert!(!sown(&env, Cell::new(17, 24)), "not while the water stays");
    month_start(&mut env, 9);
    assert!(sown(&env, Cell::new(17, 24)), "sown again once the water goes, still in the wet season");
    let notices = river::take_notices();
    assert!(notices.iter().any(|notice| matches!(notice, FloodNotice::Receded { fields: 1 })), "{notices:?}");
}

fn test_the_flood_leaves_silt() {
    let mut env = flooding_map(3);
    // The usual reach (3 here) starts at 70; beyond it 40.
    assert_eq!(env.sim.floods().fertility_at(Cell::new(18, 20)), 70);
    env.sim.floods_mut().set_fertility(Cell::new(17, 20), 50);
    month_start(&mut env, 7);
    assert_eq!(env.sim.floods().fertility_at(Cell::new(18, 20)), 100, "70 + 30, at most 100");
    assert_eq!(env.sim.floods().fertility_at(Cell::new(17, 20)), 80, "50 + 30");
    assert_eq!(env.sim.floods().fertility_at(Cell::new(16, 20)), 40, "beyond this flood");
}

fn test_a_severe_flood_and_a_gentle_one() {
    with_risks(|| {
        let mut env = flooding_map(3);
        let near = test_utils::spawn_building(&mut env, Cell::new(17, 18), "house2");
        env.sim.floods_mut().set_next_severity(1.5);
        month_start(&mut env, 7);
        let city_floods = env.sim.floods();
        assert!(city_floods.is_flooded(Cell::new(15, 20)), "a reach of 3 × 1.5, rounded: 5");
        assert!(!city_floods.is_flooded(Cell::new(14, 20)));
        assert_eq!(city_floods.log()[0].reach, 5);
        assert_eq!(city_floods.log()[0].severity, 1.5);
        assert_eq!(city_floods.next_severity(), 1.0, "the next one is ordinary");
        assert_eq!(risk(&env, near, RiskKind::Collapse), rules().collapse_damage * 1.5);

        months(&mut env, 8, 9);
        let gentle = test_utils::spawn_building(&mut env, Cell::new(22, 14), "house2");
        env.sim.floods_mut().set_next_severity(0.5);
        let context = env.new_sim_context(0.0);
        assert!(river::flood_now(&context, None));
        let city_floods = env.sim.floods();
        assert_eq!(city_floods.log()[1].reach, 3, "a gentle flood reaches as far");
        assert_eq!(risk(&env, gentle, RiskKind::Collapse), rules().collapse_damage * 0.5, "half the damage");
    });
}

fn test_faith_sets_the_next_floods_severity() {
    let mut env = flooding_map(3);
    env.sim.faith_mut().start(Realm::DaiLong, &GameConfigs::get().sim.faith);
    let context = env.new_sim_context(0.0);
    events::set_off_now(&context, "mountain_lord", "gentle_flood").expect("the gentle flood");
    assert_eq!(env.sim.floods().next_severity(), 0.5);
    let context = env.new_sim_context(0.0);
    events::set_off_now(&context, "water_lord", "severe_flood").expect("the severe flood");
    assert_eq!(env.sim.floods().next_severity(), 1.5, "the latest wins");

    let mut dry = river_map();
    dry.sim.faith_mut().start(Realm::DaiLong, &GameConfigs::get().sim.faith);
    let context = dry.new_sim_context(0.0);
    let error = events::set_off_now(&context, "mountain_lord", "gentle_flood").unwrap_err();
    assert!(error.contains("waits for floods"), "{error}");
}

fn test_nothing_new_on_land_under_water() {
    let mut env = flooding_map(3);
    let well = test_utils::find_building_def("small_well");
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, None));

    let refused = place(&mut env, Cell::new(18, 24), well);
    assert!(!refused.is_ok(), "a well under water");
    assert!(
        !place(&mut env, Cell::new(17, 25), test_utils::find_building_def("farmstead")).is_ok(),
        "a footprint half in it"
    );
    assert!(place(&mut env, Cell::new(10, 24), well).is_ok(), "dry land");

    let context = env.new_sim_context(0.0);
    assert!(river::recede(&context));
    assert!(place(&mut env, Cell::new(18, 24), well).is_ok(), "once the water goes");
}

fn test_a_flood_now_and_the_water_going() {
    let mut env = flooding_map(3);
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, Some(1)), "a flood of 1 cell");
    assert!(!river::flood_now(&context, None), "one at a time");
    let city_floods = env.sim.floods();
    assert!(city_floods.is_flooded(Cell::new(19, 20)) && !city_floods.is_flooded(Cell::new(18, 20)));
    assert_eq!(city_floods.log()[0].reach, 1);

    let context = env.new_sim_context(0.0);
    assert!(river::recede(&context));
    assert!(!river::recede(&context), "nothing left");
    assert!(!is_flagged(&env, Cell::new(19, 20)));

    let mut dry = river_map();
    let context = dry.new_sim_context(0.0);
    assert!(!river::flood_now(&context, None), "floods are off");
}

fn test_turning_floods_off_takes_the_water_away() {
    let mut env = flooding_map(3);
    month_start(&mut env, 7);
    river::take_notices();
    let context = env.new_sim_context(0.0);
    river::set_enabled(&context, false);
    assert!(env.sim.floods().flood().is_none());
    assert!(!is_flagged(&env, Cell::new(18, 20)));
    assert!(river::take_notices().is_empty(), "quietly");
}

fn test_the_flood_lens() {
    assert!(!MinimapLens::offered(false).contains(&MinimapLens::Flood), "not while the river doesn't flood");
    assert!(MinimapLens::offered(true).contains(&MinimapLens::Flood));

    // Floods of 2 to 6 cells; this one reaches 2.
    let mut env = river_map();
    let mission = MissionFloodsDef { enabled: Some(true), reach: Some([2, 6]), ..Default::default() };
    let tile_map = &env.tile_map;
    env.sim.floods_mut().start(true, &mission, rules(), tile_map);
    let under = test_utils::spawn_building(&mut env, Cell::new(18, 18), "house2");
    let at_risk = test_utils::spawn_building(&mut env, Cell::new(15, 24), "house2");
    let safe = test_utils::spawn_building(&mut env, Cell::new(8, 20), "house2");
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, Some(2)));

    let style = MinimapStyle::default();
    let mut data = LensData::default();
    let context = env.new_sim_context(0.0);
    let map_size = context.tile_map().size_in_cells();
    let lens_context =
        LensContext { sim: &context, features: context.tile_map().minimap().features(), map_size, style: &style };
    lens::provider(MinimapLens::Flood).expect("a provider").compute(&lens_context, &mut data);

    let value = |cell: Cell| data.values[(cell.y * map_size.width + cell.x) as usize];
    assert_eq!(value(Cell::new(19, 20)), u8::MAX, "under water");
    assert!(value(Cell::new(15, 20)) > 0 && value(Cell::new(15, 20)) < u8::MAX, "an ordinary flood's reach (6)");
    assert!(value(Cell::new(17, 20)) > value(Cell::new(15, 20)), "stronger by the water");
    assert_eq!(value(Cell::new(13, 20)), 0, "past it");

    let colour = |building: BuildingKindAndId| {
        let base = test_utils::find_building(&env, building).base_cell();
        data.building_colors.get(&(base.y, base.x)).copied()
    };
    assert_eq!(colour(under), Some(style.lenses.flood_under_water.color()));
    assert_eq!(colour(at_risk), Some(style.lenses.flood_at_risk.color()));
    assert_eq!(colour(safe), None);
    let legend: Vec<&str> = data.legend.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(legend, ["Under water", "Floods in an ordinary year", "At risk", "Dyke sound", "Dyke worn"]);
}

fn test_a_flood_under_way_round_trips_a_save() {
    let mut env = flooding_map(3);
    month_start(&mut env, 7);
    let json = serde_json::to_string(env.sim.floods()).unwrap();
    let back: CityFloods = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    assert!(back.is_flooded(Cell::new(18, 20)));
    assert!(!back.is_flooded(Cell::new(16, 20)));
    assert_eq!(back.flood().map(|flood| flood.months_left), Some(2));
    assert_eq!(back.log().len(), 1);
}

// ----------------------------------------------
// Dykes (Phase 3)
// ----------------------------------------------

fn dyke_def() -> &'static TileDef {
    road::tile_def(RoadKind::Dyke)
}

// Floods on, reaching `reach` cells, with a dyke along x = 17 the length of the playable area:
// the river's west bank (x 18 and 19) lies before it, the land west of x 17 behind it.
fn dyked_map(reach: u32) -> TestEnvironment {
    let mut env = flooding_map(reach);
    for y in 0..40 {
        let cell = Cell::new(17, y);
        if env.tile_map.is_cell_within_playable_area(cell) {
            assert!(place(&mut env, cell, dyke_def()).is_ok(), "a dyke at {cell}");
        }
    }
    let context = env.new_sim_context(0.0);
    context.floods_mut().sync_dykes(context.tile_map());
    env
}

fn variation_at(env: &TestEnvironment, cell: Cell) -> usize {
    env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).expect("terrain").variation_index()
}

fn test_a_dyke_is_drawn_as_a_road() {
    let mut env = flooding_map(3);
    let segment = road::build_segment(&env.tile_map, Cell::new(14, 18), Cell::new(14, 22), RoadKind::Dyke);
    assert!(segment.is_valid);
    let context = env.new_sim_context(0.0);
    TilePlacement::place_road_segment(&context, &segment, false).expect("the dyke");

    let tile = env.tile_map.try_tile_from_layer(Cell::new(14, 20), TileMapLayerKind::Terrain).unwrap();
    assert!(tile.tile_def().blocks_floods && tile.path_kind().is_road(), "a road that stops floods");
    // A run along y joins east (-y) and west (+y): mask 0101; its ends join one side each.
    assert_eq!(variation_at(&env, Cell::new(14, 20)), 0b0101);
    assert_eq!(variation_at(&env, Cell::new(14, 18)), 0b0001, "joins west only");
    assert_eq!(variation_at(&env, Cell::new(14, 22)), 0b0100, "joins east only");
    assert_eq!(env.sim.floods().dyke_wear().len(), 5, "counted as it's drawn");

    // Not on water.
    let wet = road::build_segment(&env.tile_map, Cell::new(19, 20), Cell::new(22, 20), RoadKind::Dyke);
    assert!(!wet.is_valid, "across the river");
}

fn test_dykes_hold_the_flood_back() {
    let mut env = dyked_map(4);
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, None));
    let city_floods = env.sim.floods();
    for cell in [Cell::new(19, 20), Cell::new(18, 20), Cell::new(22, 20), Cell::new(25, 20)] {
        assert!(city_floods.is_flooded(cell), "{cell}: before the dyke, or the east bank");
    }
    for cell in [Cell::new(17, 20), Cell::new(16, 20), Cell::new(15, 20)] {
        assert!(!city_floods.is_flooded(cell), "{cell}: the dyke and the land behind it");
    }
    assert_eq!(city_floods.log()[0].breaches, 0, "sound dykes hold");
    assert_eq!(city_floods.floods_survived(), 1);

    // The ordinary reach stops at them too.
    let reach = city_floods.ordinary_reach(&env.tile_map);
    assert!(reach.iter().any(|(cell, _)| *cell == Cell::new(18, 20)));
    assert!(!reach.iter().any(|(cell, _)| cell.x <= 17 && cell.y == 20), "nothing at or behind the dyke");
}

fn test_dykes_wear_month_by_month() {
    let mut env = dyked_map(3);
    let cell = Cell::new(17, 20);
    assert_eq!(env.sim.floods().wear_at(cell), 0.0, "new dykes are sound");
    let rules = rules();
    month_start(&mut env, 2);
    let first = env.sim.floods().wear_at(cell);
    let per_month = rules.wear_per_month;
    assert!(
        first >= per_month * (1.0 - rules.wear_jitter) - 1e-3 && first <= per_month * (1.0 + rules.wear_jitter) + 1e-3,
        "a month's wear, jittered: {first}"
    );
    month_start(&mut env, 3);
    assert!((env.sim.floods().wear_at(cell) - 2.0 * first).abs() < 1e-3, "the same each month for the same cell");
    for _ in 0..16 {
        month_start(&mut env, 3); // No flood month: these months only wear the dykes.
    }
    assert_eq!(env.sim.floods().wear_at(cell), floods::MAX_WEAR, "at most 100");
    assert!(env.sim.floods().worn_dykes(rules.worn_wear) > 0);

    // A builder's walk resets it.
    env.sim.floods_mut().builder_walked(cell, 30);
    assert_eq!(env.sim.floods().wear_at(cell), 0.0);
    assert_eq!(env.sim.floods().dyke(cell).and_then(|dyke| dyke.walked), Some(30));

    // Floods off: no records kept.
    let context = env.new_sim_context(0.0);
    river::set_enabled(&context, false);
    context.floods_mut().sync_dykes(context.tile_map());
    assert!(env.sim.floods().dyke_wear().is_empty());
}

fn patrol_test_callback(_context: &SimContextMut, _building: &mut Building, _unit: &mut Unit) {}

fn register_patrol_test_callback() -> Callback<UnitTaskPatrolCompletionCallback> {
    common::callback::register!(patrol_test_callback)
}

fn test_a_builder_walking_a_dyke_keeps_it_up() {
    let mut env = dyked_map(3);
    let cell = Cell::new(17, 20);
    env.sim.floods_mut().set_wear(cell, 70.0);
    let guild = test_utils::spawn_building(&mut env, Cell::new(10, 20), "builders_guild");

    // A builders' guild walker standing on the dyke, on its patrol.
    let walker = test_utils::spawn_unit(&mut env, cell, UnitConfigKey::Peasant);
    let completion_task = env.sim.task_manager_mut().new_task(UnitTaskDespawn::default());
    let task = UnitTaskRandomizedPatrol {
        origin_building: BuildingKindAndId { kind: BuildingKind::BuildersGuild, id: guild.id },
        origin_building_tile: BuildingTileInfo { road_link: cell, base_cell: Cell::new(10, 20) },
        max_distance: 8,
        path_bias_min: 0.1,
        path_bias_max: 0.5,
        path_record: UnitPatrolPathRecord::default(),
        buildings_to_visit: Some(BuildingKind::all()),
        completion_callback: register_patrol_test_callback(),
        completion_task,
        idle_countdown: None,
        state: UnitTaskPatrolState::default(),
        completion_callback_done: false,
        visited_buildings: None,
    };
    test_utils::assign_task(&mut env, walker, task);
    for _ in 0..4 {
        test_utils::tick(&mut env, 0.05);
    }
    assert_eq!(env.sim.floods().wear_at(cell), 0.0, "the builder walked it");
    assert!(env.sim.floods().dyke(cell).and_then(|dyke| dyke.walked).is_some());

    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    cmds.despawn_unit_with_id(walker);
    cmds.execute(&context);
}

fn test_a_worn_dyke_breaches() {
    let mut env = dyked_map(4);
    let breach = Cell::new(17, 20);
    env.sim.floods_mut().set_wear(breach, floods::MAX_WEAR);
    river::take_notices();
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, None));

    // The worn cell gave way: dirt under rubble, and the water through it to the reach left.
    let terrain = env.tile_map.try_tile_from_layer(breach, TileMapLayerKind::Terrain).unwrap();
    assert!(!terrain.tile_def().blocks_floods && terrain.tile_def().name == "dirt", "{}", terrain.tile_def().name);
    assert!(rubble::is_rubble(&env.tile_map, breach), "rubble on it");
    let city_floods = env.sim.floods();
    assert!(city_floods.is_flooded(breach), "the breach, 3 cells from the shore");
    assert!(city_floods.is_flooded(Cell::new(16, 20)), "the land behind it, the 4th");
    assert!(!city_floods.is_flooded(Cell::new(15, 20)), "past the reach");
    assert!(!city_floods.is_flooded(Cell::new(16, 22)), "beyond the breach's reach");
    assert!(city_floods.dyke(breach).is_none(), "its record gone");
    assert!(floods::is_dyke(&env.tile_map, Cell::new(17, 19)), "the sound sections stand");
    assert_eq!(variation_at(&env, Cell::new(17, 19)) & 0b0001, 0, "no longer joined west, to the breach");

    let record = &city_floods.log()[0];
    assert_eq!(record.breaches, 1);
    assert_eq!(city_floods.floods_survived(), 0);
    let notices = river::take_notices();
    assert!(notices.iter().any(|notice| matches!(notice, FloodNotice::Breached { breaches: 1 })), "{notices:?}");
}

fn test_the_floods_survived_goal() {
    let mut env = dyked_map(3);
    let goal = |min| MissionGoal::FloodsSurvived { min };
    let stats = |env: &TestEnvironment| {
        let mut stats = WorldStats::default();
        env.sim.floods().tally(&mut stats);
        stats
    };
    assert!(!goal(1).is_met(&stats(&env)));

    // One flood held back by the dykes.
    month_start(&mut env, 7);
    months(&mut env, 8, 9);
    assert_eq!(stats(&env).floods.floods, 1);
    assert!(goal(1).is_met(&stats(&env)));

    // One that breached a worn section doesn't count.
    env.sim.floods_mut().set_wear(Cell::new(17, 20), floods::MAX_WEAR);
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, Some(4)));
    let stats = stats(&env);
    assert_eq!((stats.floods.floods, stats.floods.survived), (2, 1));
    assert!(!goal(2).is_met(&stats));
}

fn test_the_flood_lens_shows_the_dykes() {
    let mut env = dyked_map(3);
    env.sim.floods_mut().set_wear(Cell::new(17, 22), floods::MAX_WEAR);
    let style = MinimapStyle::default();
    let mut data = LensData::default();
    let context = env.new_sim_context(0.0);
    let map_size = context.tile_map().size_in_cells();
    let lens_context =
        LensContext { sim: &context, features: context.tile_map().minimap().features(), map_size, style: &style };
    lens::provider(MinimapLens::Flood).expect("a provider").compute(&lens_context, &mut data);

    let mark = |cell: Cell| data.cell_colors.iter().find(|(at, _)| *at == cell).map(|(_, color)| *color);
    let near = |a: Option<common::Color>, b: common::Color| {
        a.is_some_and(|a| (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 1e-4)
    };
    assert!(near(mark(Cell::new(17, 20)), style.lenses.dyke_sound.color()), "a sound dyke");
    assert!(near(mark(Cell::new(17, 22)), style.lenses.dyke_worn.color()), "a worn one");
    assert!(mark(Cell::new(18, 20)).is_none(), "only dykes");
    let value = |cell: Cell| data.values[(cell.y * map_size.width + cell.x) as usize];
    assert!(value(Cell::new(18, 20)) > 0);
    assert_eq!(value(Cell::new(16, 20)), 0, "the reach stops at the dyke");
    let legend: Vec<&str> = data.legend.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(legend, ["Under water", "Floods in an ordinary year", "At risk", "Dyke sound", "Dyke worn"]);
}

fn test_dykes_round_trip_a_save() {
    let mut env = dyked_map(3);
    env.sim.floods_mut().set_wear(Cell::new(17, 20), 42.0);
    env.sim.floods_mut().builder_walked(Cell::new(17, 21), 17);
    let json = serde_json::to_string(env.sim.floods()).unwrap();
    let back: CityFloods = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    assert_eq!(back.wear_at(Cell::new(17, 20)), 42.0);
    assert_eq!(back.dyke(Cell::new(17, 21)).and_then(|dyke| dyke.walked), Some(17));
    assert_eq!(back.dyke_wear().len(), env.sim.floods().dyke_wear().len());
}

// ----------------------------------------------
// Fertility and sluice gates (Phase 4)
// ----------------------------------------------

fn gate_def() -> &'static TileDef {
    test_utils::find_building_def("sluice_gate")
}

// The start of the month `ordinal` months after year 1's first (0: month 1 of year 1), as the
// simulation runs it: the river, then the fields.
fn ordinal_month_start(env: &mut TestEnvironment, ordinal: u32) {
    let date = CalendarDate { year: 1 + ordinal / 12, month: ordinal % 12 + 1, day: 1 };
    let context = env.new_sim_context(0.0);
    river::on_new_month(&context, date);
    farms::on_new_month(&context, date);
}

fn sown(env: &TestEnvironment, farmstead: BuildingKindAndId, cell: Cell) -> bool {
    test_utils::find_building(env, farmstead)
        .as_producer()
        .fields()
        .iter()
        .any(|plot| plot.cell == cell && plot.crop.is_some())
}

fn rice_held(env: &TestEnvironment, farmstead: BuildingKindAndId) -> u32 {
    test_utils::find_building(env, farmstead).available_resources(ResourceKind::Rice)
}

// A canal drawn from `from` to `to`, as the player's drag does.
fn canal(env: &mut TestEnvironment, from: Cell, to: Cell) {
    let segment = road::build_segment(&env.tile_map, from, to, RoadKind::Canal);
    assert!(segment.is_valid, "a canal from {from} to {to}");
    let context = env.new_sim_context(0.0);
    TilePlacement::place_road_segment(&context, &segment, false).expect("the canal");
}

// Where the water at a canal or a field comes from, if any reaches it.
fn water_at(env: &mut TestEnvironment, cell: Cell) -> Option<farms::WaterSource> {
    let context = env.new_sim_context(0.0);
    farms::Irrigation::read(context.world(), context.tile_map()).source(cell)
}

fn terrain_name(env: &TestEnvironment, cell: Cell) -> &'static str {
    env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).expect("terrain").variation_name()
}

// Behind the dyke along x = 17: a farmstead at (12, 24) with a field at (15, 24) and one at (12, 29),
// a sluice gate at (16, 22) against the dyke (the river across it), and a canal from its outlet,
// (15, 22), to (15, 23), beside the first field: the gate waters that one only.
fn gated_farm(reach: u32) -> (TestEnvironment, BuildingKindAndId, BuildingKindAndId) {
    let mut env = dyked_map(reach);
    let farmstead = farmstead(&mut env, Cell::new(12, 24));
    for cell in [Cell::new(15, 24), Cell::new(12, 29)] {
        assert!(place(&mut env, cell, field_def()).is_ok(), "a field at {cell}");
    }
    assert!(place(&mut env, Cell::new(16, 22), gate_def()).is_ok(), "the sluice gate");
    let gate = env.world.find_building_for_cell(Cell::new(16, 22), &env.tile_map).expect("the gate").kind_and_id();
    canal(&mut env, Cell::new(15, 22), Cell::new(15, 23));
    (env, farmstead, gate)
}

// A house of `population` whose people stay put (their updates frozen), to staff buildings from.
fn house_with(env: &mut TestEnvironment, cell: Cell, population: u32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, cell, "house2");
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

fn test_a_harvest_wears_the_land() {
    // Behind the dyke: no silt, so each harvest leaves the land poorer.
    let mut env = dyked_map(3);
    let farmstead = farmstead(&mut env, Cell::new(12, 24));
    let field = Cell::new(14, 24);
    assert!(place(&mut env, field, field_def()).is_ok());
    let before = env.sim.floods().fertility_at(field);
    assert_eq!(before, 40, "beyond the usual reach");
    // Sown in month 5, reaped in month 9.
    months(&mut env, 5, 9);
    assert!(rice_held(&env, farmstead) > 0, "reaped");
    assert_eq!(env.sim.floods().fertility_at(field), before - rules().depletion_per_harvest as u8);
    // 4 rice x (0.5 + 40 / 100): the land's yield.
    assert_eq!(rice_held(&env, farmstead), 4);
}

fn test_a_sluice_gate_stands_beside_a_dyke() {
    let mut env = dyked_map(3);
    let refused = place(&mut env, Cell::new(14, 20), gate_def());
    assert!(!refused.is_ok(), "away from the dyke");

    // Beside it on either bank, facing it: the dyke is north (+x) of (16, 20), south (-x) of (18, 20).
    for (cell, side) in [(Cell::new(16, 20), "north"), (Cell::new(18, 20), "south")] {
        assert!(place(&mut env, cell, gate_def()).is_ok(), "beside the dyke at {cell}");
        let tile = env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).expect("the gate");
        assert_eq!(tile.variation_name(), side, "{cell} faces its dyke");
        assert!(env.world.find_building_for_cell(cell, &env.tile_map).is_some_and(|gate| gate.is(BuildingKind::SluiceGate)));
    }

    // Placed beside a dyke running along x on its east (-y) side, it faces east.
    let segment = road::build_segment(&env.tile_map, Cell::new(10, 14), Cell::new(13, 14), RoadKind::Dyke);
    let context = env.new_sim_context(0.0);
    TilePlacement::place_road_segment(&context, &segment, false).expect("the dyke");
    assert!(place(&mut env, Cell::new(11, 15), gate_def()).is_ok());
    let tile = env.tile_map.try_tile_from_layer(Cell::new(11, 15), TileMapLayerKind::Objects).unwrap();
    assert_eq!(tile.variation_name(), "east");
}

fn test_a_sluice_gate_waters_the_dry_season() {
    let (mut env, farmstead, _) = gated_farm(3);
    let (watered, dry) = (Cell::new(15, 24), Cell::new(12, 29));
    let water = |env: &mut TestEnvironment, cell: Cell| {
        let context = env.new_sim_context(0.0);
        farms::field_report(&context, cell).and_then(|field| field.water)
    };
    assert_eq!(water(&mut env, watered), Some(farms::WaterSource::Gate(Cell::new(16, 22))), "through its canal");
    assert_eq!(water(&mut env, dry), None, "not joined to its canal");

    // The dry season's first month (11): only the watered field is sown.
    ordinal_month_start(&mut env, 10);
    assert!(sown(&env, farmstead, watered), "sown in the dry season");
    assert!(!sown(&env, farmstead, dry), "the rains only: not yet");

    // Reaped four months later, in month 3; both sown again when the rains come in month 5.
    for ordinal in 11..=14 {
        ordinal_month_start(&mut env, ordinal);
    }
    assert!(rice_held(&env, farmstead) > 0, "the dry season's harvest");
    assert!(!sown(&env, farmstead, watered));
    for ordinal in 15..=16 {
        ordinal_month_start(&mut env, ordinal);
    }
    assert!(sown(&env, farmstead, watered) && sown(&env, farmstead, dry), "the wet season's sowing");
}

fn test_an_unstaffed_sluice_gate_waters_nothing() {
    cheats::get_mut().ignore_worker_requirements = false;
    let (mut env, farmstead, gate) = gated_farm(3);
    let watered = Cell::new(15, 24);
    // The farmstead's workers: the cheat staffs it no more.
    let house = house_with(&mut env, Cell::new(8, 30), 4);
    let added = test_utils::find_building_mut(&mut env, farmstead).add_workers(2, house);
    assert_eq!(added, 2);

    assert!(!test_utils::find_building(&env, gate).is_operational(), "no workers");
    ordinal_month_start(&mut env, 10);
    assert!(!sown(&env, farmstead, watered), "not watered while it lacks workers");

    let added = test_utils::find_building_mut(&mut env, gate).add_workers(1, house);
    assert_eq!(added, 1);
    assert!(test_utils::find_building(&env, gate).is_operational());
    ordinal_month_start(&mut env, 11);
    assert!(sown(&env, farmstead, watered), "watered once it has a worker");
    cheats::get_mut().ignore_worker_requirements = true;
}

fn test_a_sluice_gate_lets_the_silt_in() {
    // The flood reaches 3 cells: to x 18 on the west bank, against the dyke at x 17.
    let (mut env, farmstead, _) = gated_farm(3);
    let (watered, dry) = (Cell::new(15, 24), Cell::new(12, 29));
    let (before_watered, before_dry) = (env.sim.floods().fertility_at(watered), env.sim.floods().fertility_at(dry));
    months(&mut env, 5, 6);
    assert!(sown(&env, farmstead, watered));
    river::take_notices();

    month_start(&mut env, 7);
    let city_floods = env.sim.floods();
    assert!(!city_floods.is_flooded(watered) && !city_floods.is_flooded(Cell::new(16, 22)), "behind the dyke, dry");
    assert_eq!(city_floods.fertility_at(watered), before_watered + rules().silt as u8, "the silt through the gate");
    assert_eq!(city_floods.fertility_at(dry), before_dry, "not joined to its canal");
    assert!(sown(&env, farmstead, watered), "its crop kept: the silt, not the water");
    assert_eq!(city_floods.log()[0].fields_flooded, 0);
    let notices = river::take_notices();
    assert!(notices.iter().any(|notice| matches!(notice, FloodNotice::Sluiced { fields: 1 })), "{notices:?}");
}

fn test_no_silt_where_the_water_doesnt_reach_the_gate() {
    // A flood of 1 cell stays on the shore (x 19): the dyke at x 17 never has water against it.
    let (mut env, _, _) = gated_farm(1);
    let watered = Cell::new(15, 24);
    let before = env.sim.floods().fertility_at(watered);
    river::take_notices();
    month_start(&mut env, 7);
    assert_eq!(env.sim.floods().fertility_at(watered), before);
    let notices = river::take_notices();
    assert!(!notices.iter().any(|notice| matches!(notice, FloodNotice::Sluiced { .. })), "{notices:?}");
}

fn test_the_fertility_lens() {
    let (mut env, _, gate) = gated_farm(3);
    assert!(!MinimapLens::offered(false).contains(&MinimapLens::Fertility), "not while the river doesn't flood");
    assert!(MinimapLens::offered(true).contains(&MinimapLens::Fertility));

    // A field the gate doesn't water, sown and growing, and a canal joined to nothing.
    months(&mut env, 5, 6);
    canal(&mut env, Cell::new(10, 18), Cell::new(11, 18));
    env.sim.floods_mut().set_fertility(Cell::new(10, 20), 100);
    let style = MinimapStyle::default();
    let mut data = LensData::default();
    let context = env.new_sim_context(0.0);
    let map_size = context.tile_map().size_in_cells();
    let lens_context =
        LensContext { sim: &context, features: context.tile_map().minimap().features(), map_size, style: &style };
    lens::provider(MinimapLens::Fertility).expect("a provider").compute(&lens_context, &mut data);

    let value = |cell: Cell| data.values[(cell.y * map_size.width + cell.x) as usize];
    assert_eq!(value(Cell::new(20, 20)), 0, "water has none");
    assert!(value(Cell::new(10, 20)) > value(Cell::new(10, 24)), "rich land over poor");
    assert!(value(Cell::new(10, 24)) > 0, "poor land still shows");
    let colour = |cell: Cell| data.building_colors.get(&(cell.y, cell.x)).copied();
    let near = |a: Option<common::Color>, b: common::Color| {
        a.is_some_and(|a| (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 1e-4)
    };
    assert!(near(colour(Cell::new(15, 24)), style.lenses.fertility_irrigated.color()), "watered");
    assert!(near(colour(Cell::new(12, 29)), style.lenses.fertility_growing.color()), "growing");
    let gate_cell = test_utils::find_building(&env, gate).base_cell();
    assert!(near(colour(gate_cell), style.lenses.fertility_irrigated.color()), "the gate");
    // The canals: the gate's wet, one joined to nothing dry.
    let mark = |cell: Cell| data.cell_colors.iter().find(|(at, _)| *at == cell).map(|(_, color)| *color);
    assert!(near(mark(Cell::new(15, 23)), style.lenses.fertility_irrigated.color()), "a wet canal");
    assert!(near(mark(Cell::new(10, 18)), style.lenses.fertility_canal_dry.color()), "a dry canal");
    let legend: Vec<&str> = data.legend.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(legend, ["Fertile", "Poor", "Growing", "Ripe", "Fallow", "Irrigated", "Dry canal"]);
}

// ----------------------------------------------
// Canals and irrigation as connected water (Phase 6)
// ----------------------------------------------

fn test_a_canal_is_drawn_with_the_waters_shores() {
    let mut env = flooding_map(3);
    canal(&mut env, Cell::new(14, 18), Cell::new(14, 22));
    let tile = env.tile_map.try_tile_from_layer(Cell::new(14, 20), TileMapLayerKind::Terrain).unwrap();
    assert!(water::is_canal_def(tile.tile_def()) && tile.path_kind().is_water(), "a water tile");
    // A run along y: land north (+x) and south (-x) of it, a one-cell channel; its ends closed.
    assert_eq!(terrain_name(&env, Cell::new(14, 20)), "1010");
    assert_eq!(terrain_name(&env, Cell::new(14, 18)), "1110", "land to the east (-y)");
    assert_eq!(terrain_name(&env, Cell::new(14, 22)), "1011", "land to the west (+y)");

    // Only on open land: not across the river, a road or a field.
    assert!(!road::build_segment(&env.tile_map, Cell::new(19, 25), Cell::new(22, 25), RoadKind::Canal).is_valid);
    let segment = road::build_segment(&env.tile_map, Cell::new(10, 18), Cell::new(10, 22), RoadKind::Dirt);
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &segment, false).expect("a road");
    assert!(!road::build_segment(&env.tile_map, Cell::new(8, 20), Cell::new(12, 20), RoadKind::Canal).is_valid);

    // Joined to the river, the river's shore opens to it (the test map's river is laid without
    // shores: they're drawn as the terrain beside them changes).
    canal(&mut env, Cell::new(17, 26), Cell::new(19, 26));
    assert_eq!(terrain_name(&env, Cell::new(19, 26)), "0101", "a channel along x into the river");
    assert_eq!(terrain_name(&env, Cell::new(20, 26)), "se_sw_corner", "the river opens to the canal");

    // Cleared, it's grass again, and the river's bank closes.
    let context = env.new_sim_context(0.0);
    let tile = context.tile_map().try_tile_from_layer(Cell::new(19, 26), TileMapLayerKind::Terrain).unwrap();
    assert!(TilePlacement::can_clear(tile));
    assert!(matches!(TilePlacement::clear(&context, tile, false, false), PlaceOrClearResult::ClearedTile(_)));
    let tile = env.tile_map.try_tile_from_layer(Cell::new(19, 26), TileMapLayerKind::Terrain).unwrap();
    assert_eq!(tile.name(), "grass");
    assert_eq!(terrain_name(&env, Cell::new(20, 26)), "0010");
}

fn test_water_flows_through_fields_and_canals() {
    // Floods or not: river_map's river doesn't flood.
    let mut env = river_map();
    farmstead(&mut env, Cell::new(15, 20));
    for cell in [Cell::new(17, 20), Cell::new(18, 20), Cell::new(19, 20), Cell::new(17, 23)] {
        assert!(place(&mut env, cell, field_def()).is_ok(), "a field at {cell}");
    }
    // Beside the river, and joined to one beside it: watered from the river.
    for cell in [Cell::new(19, 20), Cell::new(18, 20), Cell::new(17, 20)] {
        assert_eq!(water_at(&mut env, cell), Some(farms::WaterSource::River), "{cell}");
    }
    assert_eq!(water_at(&mut env, Cell::new(17, 23)), None, "the rains only");

    // A canal from the river to it.
    canal(&mut env, Cell::new(18, 23), Cell::new(19, 23));
    assert_eq!(water_at(&mut env, Cell::new(19, 23)), Some(farms::WaterSource::River));
    assert_eq!(water_at(&mut env, Cell::new(17, 23)), Some(farms::WaterSource::River), "through the canal");

    // A canal joined to nothing is dry.
    canal(&mut env, Cell::new(12, 26), Cell::new(14, 26));
    assert_eq!(water_at(&mut env, Cell::new(13, 26)), None);

    // A watered field is sown in the dry season too.
    month_start(&mut env, 11);
    let report = {
        let context = env.new_sim_context(0.0);
        farms::field_report(&context, Cell::new(17, 23)).expect("the field")
    };
    assert!(report.irrigated && report.sown.is_some(), "sown in month 11");
}

fn test_a_sluice_gate_needs_water_across_its_dyke() {
    let mut env = flooding_map(3);
    // A dyke at x = 8, eleven cells from the river, a gate behind it at (7, 20) facing it, and a
    // canal from its outlet (6, 20).
    let segment = road::build_segment(&env.tile_map, Cell::new(8, 17), Cell::new(8, 23), RoadKind::Dyke);
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &segment, false).expect("the dyke");
    assert!(place(&mut env, Cell::new(7, 20), gate_def()).is_ok(), "the sluice gate");
    canal(&mut env, Cell::new(6, 20), Cell::new(6, 22));
    let supply = |env: &mut TestEnvironment| {
        let context = env.new_sim_context(0.0);
        let irrigation = farms::Irrigation::read(context.world(), context.tile_map());
        let supply = irrigation.gate_at(Cell::new(7, 20)).expect("the gate's supply").clone();
        (supply.intake, supply.supplies(), supply.outlets.clone())
    };
    assert_eq!(supply(&mut env), (false, false, vec![Cell::new(6, 20)]), "no water within 3 cells across its dyke");
    assert_eq!(water_at(&mut env, Cell::new(6, 21)), None);

    // A canal from the river to the dyke's foot brings it.
    canal(&mut env, Cell::new(9, 20), Cell::new(19, 20));
    assert_eq!(water_at(&mut env, Cell::new(9, 20)), Some(farms::WaterSource::River));
    assert!(supply(&mut env).1, "it has its intake");
    assert_eq!(water_at(&mut env, Cell::new(6, 21)), Some(farms::WaterSource::Gate(Cell::new(7, 20))), "through the gate");
}

fn test_dry_canals_are_flagged() {
    let mut env = river_map();
    let dry = |env: &TestEnvironment, cell: Cell| {
        env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).unwrap().has_flags(TileFlags::DryCanal)
    };
    canal(&mut env, Cell::new(14, 22), Cell::new(16, 22));
    farms::irrigation::refresh_canals_if_changed(&env.new_sim_context(0.0));
    assert!(dry(&env, Cell::new(15, 22)), "joined to nothing");

    // Joined to the river: refreshed at the next frame's start, as placing it marked the network.
    canal(&mut env, Cell::new(17, 22), Cell::new(19, 22));
    assert!(dry(&env, Cell::new(15, 22)), "not until the refresh");
    farms::irrigation::refresh_canals_if_changed(&env.new_sim_context(0.0));
    assert!(!dry(&env, Cell::new(15, 22)) && !dry(&env, Cell::new(19, 22)), "wet");
}

fn test_an_open_canal_carries_the_flood() {
    // The flood reaches 2 cells from the river's banks; a canal from the river carries it inland.
    let mut env = flooding_map(2);
    canal(&mut env, Cell::new(12, 28), Cell::new(19, 28));
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, None));
    let city_floods = env.sim.floods();
    assert!(city_floods.is_flooded(Cell::new(12, 29)) && city_floods.is_flooded(Cell::new(13, 27)), "beside the canal");
    assert!(!city_floods.is_flooded(Cell::new(12, 31)), "beyond its reach of the canal");
    assert!(!city_floods.is_flooded(Cell::new(12, 20)), "away from the canal and the river");
}

fn test_the_flood_passes_over_a_canal_it_doesnt_come_from() {
    // Reaching 6 cells, past a canal at x = 16 joined to nothing: (15, 20) is 5 cells from the
    // river's bank across it, 7 around its end.
    let mut env = flooding_map(6);
    canal(&mut env, Cell::new(16, 16), Cell::new(16, 24));
    let context = env.new_sim_context(0.0);
    assert!(river::flood_now(&context, None));
    let city_floods = env.sim.floods();
    assert!(city_floods.is_flooded(Cell::new(16, 20)), "the canal under the flood");
    assert!(city_floods.is_flooded(Cell::new(15, 20)), "the land behind it");
}

fn test_a_canal_never_makes_water_flood() {
    // Bodies of at least 5 cells: the river; not the pond (4), nor 6 cells of canal.
    let mut env = river_map();
    canal(&mut env, Cell::new(10, 18), Cell::new(10, 23));
    assert_eq!(floods::water_bodies(&env.tile_map, 5).len(), 1, "the river only");
    // Joined to the river, a canal is part of it, not a body of its own.
    canal(&mut env, Cell::new(17, 26), Cell::new(19, 26));
    assert_eq!(floods::water_bodies(&env.tile_map, 5).len(), 1);
}

// ----------------------------------------------
// Terrain height (docs/plans/terrain-height.md, §9)
// ----------------------------------------------

// Sets cells' heights straight on the map (before what stands there is placed).
fn raise(env: &mut TestEnvironment, from: (i32, i32), to: (i32, i32), level: u8) {
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            env.tile_map.set_elevation(Cell::new(x, y), level, Shape::Flat);
        }
    }
}

// The river's west bank a level up from x = 18 westward, with a ramp up from the shore along row
// 20: its foot at (19, 20), its head at (18, 20). Floods reaching 3 cells, up to `flood_level`.
fn terraced_bank(flood_level: u8) -> TestEnvironment {
    let mut env = river_map();
    raise(&mut env, (0, 0), (18, 39), 1);
    env.tile_map.set_elevation(Cell::new(19, 20), 0, Shape::Ramp(Dir::S, Half::Low));
    env.tile_map.set_elevation(Cell::new(18, 20), 0, Shape::Ramp(Dir::S, Half::High));
    assert!(env.tile_map.elevation().problems().is_empty(), "{:?}", env.tile_map.elevation().problems());
    let mission =
        MissionFloodsDef { enabled: Some(true), reach: Some([3, 3]), flood_level: Some(flood_level), ..Default::default() };
    let tile_map = &env.tile_map;
    env.sim.floods_mut().start(true, &mission, rules(), tile_map);
    river::take_notices();
    env
}

// Q11: the water rises over the land at or below the flood level only (0: level-0 land). A ramp's
// foot floods; its head, and the land above it, stay dry, starting with the fertility beyond the
// reach and taking no silt. At level 1 the bank floods too. A level above the highest is a problem in a mission.
fn test_floods_stay_below_their_level() {
    let mut env = terraced_bank(0);
    assert_eq!(env.sim.floods().profile().flood_level, 0);
    assert_eq!(env.sim.floods().fertility_at(Cell::new(19, 10)), 70, "the shore within the usual reach");
    assert_eq!(env.sim.floods().fertility_at(Cell::new(17, 10)), 40, "the bank beyond it");
    assert!(river::flood_now(&env.new_sim_context(0.0), None));
    let city_floods = env.sim.floods();
    for cell in [Cell::new(19, 10), Cell::new(19, 20), Cell::new(22, 20), Cell::new(24, 20)] {
        assert!(city_floods.is_flooded(cell), "{cell} is low ground in reach");
    }
    for cell in [Cell::new(18, 10), Cell::new(17, 10), Cell::new(18, 20), Cell::new(17, 20)] {
        assert!(!city_floods.is_flooded(cell), "{cell} is above the water");
    }
    assert_eq!(city_floods.fertility_at(Cell::new(19, 10)), 100, "silt on the shore");
    assert_eq!(city_floods.fertility_at(Cell::new(17, 10)), 40, "none on the bank");
    let reach = city_floods.ordinary_reach(&env.tile_map);
    assert!(reach.iter().all(|&(cell, _)| env.tile_map.elevation().level(cell) == 0), "the Flood lens's reach");

    let mut env = terraced_bank(1);
    assert!(river::flood_now(&env.new_sim_context(0.0), None));
    let city_floods = env.sim.floods();
    for cell in [Cell::new(18, 10), Cell::new(17, 10), Cell::new(18, 20)] {
        assert!(city_floods.is_flooded(cell), "{cell} is under a flood rising a level");
    }
    assert!(!city_floods.is_flooded(Cell::new(16, 10)), "3 cells from the shore");

    let wet = [5, 6, 7, 8, 9, 10];
    let too_high = MissionFloodsDef { flood_level: Some(8), ..Default::default() };
    assert_eq!(too_high.problems(&wet).len(), 1, "{:?}", too_high.problems(&wet));
}

// Water passes only between canals and fields whose ground joins: a field a level above the river,
// or above a watered field, stays dry (Q9).
fn test_water_doesnt_climb_to_a_plateau() {
    let mut env = river_map();
    raise(&mut env, (17, 22), (19, 31), 1);
    for cell in [Cell::new(19, 21), Cell::new(19, 22), Cell::new(18, 22)] {
        env.tile_map.try_place_tile(cell, field_def()).unwrap_or_else(|err| panic!("a field at {cell}: {}", err.message));
    }
    assert_eq!(water_at(&mut env, Cell::new(19, 21)), Some(farms::WaterSource::River), "beside the river, on its level");
    assert_eq!(water_at(&mut env, Cell::new(19, 22)), None, "beside the river and a wet field, a level up");
    assert_eq!(water_at(&mut env, Cell::new(18, 22)), None, "joined to a dry one");
}

// A gate a level up draws nothing from the river below its dyke, and lets nothing out over a cliff.
fn test_a_sluice_gate_on_high_ground_draws_no_water() {
    let mut env = river_map();
    raise(&mut env, (14, 22), (19, 31), 1);
    let segment = road::build_segment(&env.tile_map, Cell::new(19, 24), Cell::new(19, 28), RoadKind::Dyke);
    assert!(segment.is_valid, "a dyke along the plateau's edge");
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &segment, false).expect("the dyke");
    assert!(place(&mut env, Cell::new(18, 26), gate_def()).is_ok(), "the gate behind it, on its level");
    let supply = |env: &mut TestEnvironment| {
        let context = env.new_sim_context(0.0);
        let irrigation = farms::Irrigation::read(context.world(), context.tile_map());
        let supply = irrigation.gate_at(Cell::new(18, 26)).expect("the gate's supply").clone();
        (supply.intake, supply.outlets.clone())
    };
    assert_eq!(supply(&mut env), (false, vec![Cell::new(17, 26)]), "the river is a level below, across its dyke");

    // Ground a level higher still behind it: no outlet.
    let context = env.new_sim_context(0.0);
    TilePlacement::set_elevation(&context, &[(Cell::new(17, 26), game::tile::elevation::encode(2, Shape::Flat))], false);
    assert_eq!(supply(&mut env), (false, vec![]));
}

// A farmstead's reach covers the land its farmers walk to: on its level, not over a cliff (Q9).
fn test_a_farmsteads_reach_stops_at_a_cliff() {
    let mut env = river_map();
    // A ridge a level up across the map at x = 14, and a lone raised cell at (12, 23).
    raise(&mut env, (14, 0), (14, 39), 1);
    raise(&mut env, (12, 23), (12, 23), 1);
    let farmstead = farmstead(&mut env, Cell::new(10, 20));
    let footprint = test_utils::find_building(&env, farmstead).cell_range();

    assert!(place(&mut env, Cell::new(13, 20), field_def()).is_ok(), "in reach on its level");
    assert!(!place(&mut env, Cell::new(15, 20), field_def()).is_ok(), "in the square, but over the ridge");
    assert!(!place(&mut env, Cell::new(12, 23), field_def()).is_ok(), "a level up");
    assert!(footprint.end.x < 13, "the field lies past the footprint, at {}", footprint.end);
    assert!(farms::owner_of_field(&env.world, Cell::new(13, 20)) == Some(farmstead));

    // A cliff raised between them cuts the field off: its farmstead drops it.
    let edits: Vec<(Cell, u8)> =
        (0..40).map(|y| (Cell::new(12, y), game::tile::elevation::encode(1, Shape::Flat))).collect();
    let result = TilePlacement::set_elevation(&env.new_sim_context(0.0), &edits, false);
    assert!(result.changed.contains(&Cell::new(12, 20)));
    assert!(farms::owner_of_field(&env.world, Cell::new(13, 20)).is_none(), "dropped");
}
