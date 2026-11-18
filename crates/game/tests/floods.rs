use common::{Size, coords::Cell};
use game::{
    campaign::config::MissionFloodsDef,
    config::{FloodRules, GameConfigs},
    sim::floods::{self, CityFloods},
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the river's floods and the land's fertility
// (docs/plans/seasons-floods.md, Phase 0).
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
