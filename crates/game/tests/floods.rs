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
        minimap::{
            MinimapLens,
            lens::{self, LensContext, LensData},
            style::MinimapStyle,
        },
        road::{self, RoadKind},
        rubble,
        sets::{OBJECTS_FIELDS_CATEGORY, TileDef, TileSets},
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
// Integration tests for the river's floods, the land's fertility and the dykes
// (docs/plans/seasons-floods.md, Phases 0, 2 and 3).
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
    let reached = floods::reach_fill(&env.tile_map, &seeds, 2, |_| false);
    for cell in [Cell::new(18, 20), Cell::new(19, 20), Cell::new(22, 20), Cell::new(23, 20)] {
        assert!(reached.contains(&cell), "{cell}");
    }
    for cell in [Cell::new(17, 20), Cell::new(24, 20), Cell::new(20, 20)] {
        assert!(!reached.contains(&cell), "{cell}: past 2, or water");
    }
    assert!(reached[0].x == 19 || reached[0].x == 22, "nearest first: {}", reached[0]);
    assert!(floods::reach_fill(&env.tile_map, &seeds, 0, |_| false).is_empty());
    assert!(floods::reach_fill(&env.tile_map, &[Cell::new(5, 20)], 3, |_| false).is_empty(), "a land seed");
}

fn test_the_reach_fill_stops_where_it_is_blocked() {
    let env = river_map();
    // A wall along x = 18: the west bank floods one cell, the east bank three.
    let reached = floods::reach_fill(&env.tile_map, &[river(&env)], 3, |cell| cell.x == 18);
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
    let expected = floods::reach_fill(&env.tile_map, &city_floods.profile().sources, 3, |_| false);
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
