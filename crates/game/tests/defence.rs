use common::coords::Cell;
use game::{
    building::BuildingKind,
    campaign::config::MissionDefenceDef,
    config::{DefenceRules, GameConfigs},
    sim::{
        RandomGenerator,
        defence::{CityDefence, MapSide, RaidFrom, RaidKind, random_raid_size},
    },
    tile::TileGameObjectHandle,
    unit::config::{DEFAULT_MAX_HEALTH, UnitConfigKey},
};
use rand::{Rng, SeedableRng};
use serde_json::json;

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Defence and attacks (docs/plans/defence.md), Phase 0: the data and the state.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Defence", &[
        test_utils::test_fn!(test_mission_raids_parse),
        test_utils::test_fn!(test_mission_raid_problems),
        test_utils::test_fn!(test_start_schedules_the_missions_raids),
        test_utils::test_fn!(test_random_raids),
        test_utils::test_fn!(test_games_without_random_raids_draw_nothing),
        test_utils::test_fn!(test_moving_a_raid),
        test_utils::test_fn!(test_random_raid_size),
        test_utils::test_fn!(test_saved_shape_matches_the_migration),
        test_utils::test_fn!(test_units_spawn_with_full_health),
        test_utils::test_fn!(test_defence_building_kinds),
        test_utils::test_fn!(test_object_handles_hold_wide_kinds),
        test_utils::test_fn!(test_campaigns_still_validate),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn def(value: serde_json::Value) -> MissionDefenceDef {
    serde_json::from_value(value).expect("a valid defence block")
}

// Two raids out of order: one from the west edge with the rules' warning, a war party at a cell
// with three months' warning.
fn two_raids() -> MissionDefenceDef {
    def(json!({
        "raids": [
            { "id": "second", "month": 30, "from": [5, 60], "kind": "war_party", "size": 20, "warning_months": 3,
              "enemy": "enemy.kambara_raiders" },
            { "id": "first", "month": 18, "from": "west", "size": 8 },
        ]
    }))
}

fn rng() -> RandomGenerator {
    RandomGenerator::seed_from_u64(7)
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

// A mission's `defence` block: a raid from a side or a cell, its kind (raiders by default), size,
// warning and enemy; unknown fields refused.
fn test_mission_raids_parse() {
    let defence = two_raids();
    assert_eq!(defence.raids.len(), 2);
    let second = &defence.raids[0];
    assert!(second.from == RaidFrom::Cell([5, 60]));
    assert!(second.kind == RaidKind::WarParty);
    assert_eq!(second.warning_months, Some(3));
    assert_eq!(second.enemy.as_deref(), Some("enemy.kambara_raiders"));
    let first = &defence.raids[1];
    assert!(first.from == RaidFrom::Side(MapSide::West));
    assert!(first.kind == RaidKind::Raiders, "raiders by default");
    assert_eq!(first.warning_months, None);
    assert!(defence.problems().is_empty(), "{:?}", defence.problems());

    let unknown = serde_json::from_value::<MissionDefenceDef>(json!({
        "raids": [{ "id": "a", "month": 1, "from": "north", "size": 1, "colour": "red" }]
    }));
    assert!(unknown.is_err(), "an unknown field is refused");
    let bad_side = serde_json::from_value::<MissionDefenceDef>(json!({
        "raids": [{ "id": "a", "month": 1, "from": "up", "size": 1 }]
    }));
    assert!(bad_side.is_err(), "a side that isn't north, east, south or west is refused");
}

fn test_mission_raid_problems() {
    let defence = def(json!({
        "raids": [
            { "id": "", "month": 3, "from": "east", "size": 2 },
            { "id": "Bad-Id", "month": 3, "from": "east", "size": 2 },
            { "id": "twice", "month": 3, "from": "east", "size": 2 },
            { "id": "twice", "month": 4, "from": "east", "size": 2 },
            { "id": "at_start", "month": 0, "from": "east", "size": 2 },
            { "id": "nobody", "month": 3, "from": "east", "size": 0 },
            { "id": "early_warning", "month": 2, "from": "east", "size": 2, "warning_months": 3 },
            { "id": "off_map", "month": 3, "from": [-1, 4], "size": 2 },
        ]
    }));
    let problems = defence.problems();
    let has = |text: &str| problems.iter().any(|problem| problem.contains(text));
    assert!(has("id '' must be"), "{problems:?}");
    assert!(has("id 'Bad-Id' must be"), "{problems:?}");
    assert!(has("two raids with the id 'twice'"), "{problems:?}");
    assert!(has("'at_start' arrives in month 0"), "{problems:?}");
    assert!(has("'nobody' sends no raiders"), "{problems:?}");
    assert!(has("'early_warning' is warned of 3 months ahead"), "{problems:?}");
    assert!(has("'off_map' comes from [-1, 4]"), "{problems:?}");
    assert_eq!(problems.len(), 7, "{problems:?}");
}

// A mission's raids are scheduled from its start, by arrival, each warned of its warning months
// ahead (the rules' when it doesn't say).
fn test_start_schedules_the_missions_raids() {
    let rules = DefenceRules::default();
    let mut defence = CityDefence::new();
    let now = 12 * 20; // Year 20, month 1.
    defence.start(Some(&two_raids()), false, now, &rules, &mut rng());

    let scheduled = defence.scheduled();
    assert_eq!(scheduled.len(), 2);
    assert_eq!(scheduled[0].id, "first", "by arrival");
    assert_eq!(scheduled[0].arrives, now + 18);
    assert_eq!(scheduled[0].warn_from, now + 18 - rules.warning_months);
    assert_eq!(scheduled[0].months_left(now), 18);
    assert_eq!(scheduled[1].id, "second");
    assert_eq!(scheduled[1].arrives, now + 30);
    assert_eq!(scheduled[1].warn_from, now + 27);
    assert!(scheduled[1].kind == RaidKind::WarParty && scheduled[1].size == 20);
    assert!(!defence.random_raids() && defence.next_random_raid().is_none());
    assert!(defence.has_raids());
    assert!(defence.under_way().is_empty() && defence.log().is_empty() && defence.walls().is_empty());

    // A game without a block has none, and starting again forgets the last game's.
    defence.start(None, false, now, &rules, &mut rng());
    assert!(defence.scheduled().is_empty() && !defence.has_raids());
}

// A custom game with its Raids switch on: the next raid between the rules' least and most years
// away, drawn from the simulation's RNG; off, none.
fn test_random_raids() {
    let rules = DefenceRules::default();
    let now = 100;
    for seed in 0..32 {
        let mut defence = CityDefence::new();
        defence.start(None, true, now, &rules, &mut RandomGenerator::seed_from_u64(seed));
        assert!(defence.random_raids() && defence.has_raids());
        let next = defence.next_random_raid().expect("a next random raid");
        let months = next - now;
        assert!(
            (rules.random_raid_min_years * 12..=rules.random_raid_max_years * 12).contains(&months),
            "{months} months away"
        );
    }
    let mut defence = CityDefence::new();
    defence.start(None, true, now, &rules, &mut rng());
    defence.set_random_raids(false, now, &rules, &mut rng());
    assert!(!defence.random_raids() && defence.next_random_raid().is_none());
    // The same seed gives the same raid: the simulation stays deterministic.
    let mut a = CityDefence::new();
    let mut b = CityDefence::new();
    a.start(None, true, now, &rules, &mut rng());
    b.start(None, true, now, &rules, &mut rng());
    assert_eq!(a.next_random_raid(), b.next_random_raid());
}

// A mission's raids and a game without random raids draw nothing from the RNG, so starting them
// changes nothing that follows.
fn test_games_without_random_raids_draw_nothing() {
    let rules = DefenceRules::default();
    let mut untouched = rng();
    let mut used = rng();
    CityDefence::new().start(Some(&two_raids()), false, 0, &rules, &mut used);
    CityDefence::new().start(None, false, 0, &rules, &mut used);
    assert_eq!(untouched.random::<u64>(), used.random::<u64>());
}

fn test_moving_a_raid() {
    let rules = DefenceRules::default();
    let mut defence = CityDefence::new();
    defence.start(Some(&two_raids()), false, 0, &rules, &mut rng());
    assert!(defence.set_raid_arrival("second", 5));
    let second = &defence.scheduled()[0];
    assert_eq!(second.id, "second", "the schedule stays by arrival");
    assert_eq!(second.arrives, 5);
    assert_eq!(second.warn_from, 2, "warned as far ahead as before");
    assert!(!defence.set_raid_arrival("none", 5));
}

fn test_random_raid_size() {
    let rules = DefenceRules::default();
    assert_eq!(random_raid_size(&rules, 0), rules.random_raid_min_size);
    assert_eq!(random_raid_size(&rules, rules.residents_per_raider * 10), 10);
    assert_eq!(random_raid_size(&rules, 1_000_000), rules.random_raid_max_size);
}

// The state as a save writes it, before any raid: what the v19 to v20 migration script adds to a
// save's `sim` (crates/tools/save-migration-scripts/v19_to_v20.py, EMPTY_DEFENCE).
fn test_saved_shape_matches_the_migration() {
    let saved = serde_json::to_value(CityDefence::new()).unwrap();
    assert_eq!(
        saved,
        json!({
            "random_raids": false,
            "next_random_raid": null,
            "scheduled": [],
            "under_way": [],
            "log": [],
            "walls": [],
        })
    );
    let back: CityDefence = serde_json::from_value(saved).unwrap();
    assert!(!back.has_raids());
}

// Every unit spawns with its config's max_health (100 for all so far, as the migration gives
// saved units); set_health keeps within it.
fn test_units_spawn_with_full_health() {
    let mut env = TestEnvironment::new();
    test_utils::fill_terrain(&mut env, "grass");
    for (k, config) in [UnitConfigKey::Settler, UnitConfigKey::Runner, UnitConfigKey::Buffalo].into_iter().enumerate() {
        let id = test_utils::spawn_unit(&mut env, Cell::new(8 + k as i32, 8), config);
        let unit = test_utils::find_unit(&env, id);
        assert_eq!(unit.max_health(), DEFAULT_MAX_HEALTH);
        assert_eq!(unit.health(), unit.max_health(), "{config:?} spawns at full health");
        let unit = env.world.find_unit_mut(id).unwrap();
        unit.set_health(40);
        assert_eq!(unit.health(), 40);
        unit.set_health(u16::MAX);
        assert_eq!(unit.health(), DEFAULT_MAX_HEALTH, "no more than its max");
    }
    assert_eq!(GameConfigs::get().sim.defence.warning_months, DefenceRules::default().warning_months);
}

// The defence's buildings are services, past the 27 kinds before them (BuildingKind is a u64 now).
fn test_defence_building_kinds() {
    let defences = BuildingKind::defences();
    assert_eq!(defences.bits().count_ones(), 5);
    for kind in defences.iter() {
        assert!(BuildingKind::services().contains(kind), "{kind} is a service");
    }
    assert_eq!(BuildingKind::Fortress.bits(), 1 << 31);
    assert!(!BuildingKind::all().intersects(BuildingKind::from_bits_retain(1 << 32)), "room left for more");
}

// A tile's handle holds a building's kind in a u64, any bit of it; a handle saved before (a u32's
// numbers, the invalid one u32::MAX) reads as it did.
fn test_object_handles_hold_wide_kinds() {
    let handle = TileGameObjectHandle::new_building(3, BuildingKind::Fortress.bits());
    assert!(handle.is_valid());
    assert!(BuildingKind::from_game_object_handle(handle) == BuildingKind::Fortress);
    let wide = TileGameObjectHandle::new_building(3, 1 << 40);
    assert!(wide.is_valid() && wide.kind() == 1 << 40, "a kind past 32 bits fits");

    let saved_house: TileGameObjectHandle =
        serde_json::from_value(json!({ "index": 5, "kind_or_generation": BuildingKind::House.bits() })).unwrap();
    assert!(saved_house.is_valid());
    assert!(BuildingKind::from_game_object_handle(saved_house) == BuildingKind::House);
    let saved_invalid: TileGameObjectHandle =
        serde_json::from_value(json!({ "index": 4294967295u64, "kind_or_generation": 4294967295u64 })).unwrap();
    assert!(!saved_invalid.is_valid(), "an old save's invalid handle stays invalid");
    assert!(!TileGameObjectHandle::invalid().is_valid());
    let unit = TileGameObjectHandle::new_unit(2, 9);
    assert!(unit.is_valid() && unit.generation() == 9);
}

// The shipped campaigns load and pass their checks, `defence` blocks included.
fn test_campaigns_still_validate() {
    use game::campaign::config::CampaignConfigs;
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::load();
    }
    let errors = CampaignConfigs::get().validate();
    assert!(errors.is_empty(), "{errors:?}");
}
