use common::{Size, coords::Cell};
use game::{
    building::{BuildingKind, construction::ConstructionMode},
    campaign::config::MissionDefenceDef,
    config::{DefenceRules, GameConfigs},
    defence,
    menu::{PlaceOrClearResult, TilePlacement},
    pathfind::{Graph, Node, NodeKind, SearchResult},
    sim::{
        RandomGenerator,
        calendar::CalendarDate,
        defence::{CityDefence, DefenceNotice, MapSide, RaidFrom, RaidKind, WallTier, random_raid_size},
    },
    tile::{
        TileFlags,
        TileGameObjectHandle,
        TileKind,
        TileMapLayerKind,
        road::{self, RoadKind},
        rubble,
        wall,
    },
    undo_redo,
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
        // Phase 1: walls and gatehouses.
        test_utils::test_fn!(test_wall_pieces_join),
        test_utils::test_fn!(test_walls_go_on_open_land_only),
        test_utils::test_fn!(test_walls_block_searches),
        test_utils::test_fn!(test_walls_block_searches_after_a_load),
        test_utils::test_fn!(test_gatehouse_lets_the_city_through),
        test_utils::test_fn!(test_gatehouse_needs_a_wall_either_side),
        test_utils::test_fn!(test_gatehouse_on_a_wall_piece),
        test_utils::test_fn!(test_gatehouse_in_a_gap_of_open_land),
        test_utils::test_fn!(test_roads_dont_cross_walls),
        test_utils::test_fn!(test_road_drawn_through_a_gatehouse),
        test_utils::test_fn!(test_undoing_a_gatehouse_on_a_wall),
        test_utils::test_fn!(test_clearing_a_gatehouse),
        test_utils::test_fn!(test_stronger_tier_replaces_a_weaker),
        test_utils::test_fn!(test_clearing_a_piece),
        test_utils::test_fn!(test_undoing_a_wall),
        test_utils::test_fn!(test_undoing_a_stronger_tier),
        test_utils::test_fn!(test_damage_shows_cracks_and_breaches),
        test_utils::test_fn!(test_breached_cell_opens),
        test_utils::test_fn!(test_builders_repair_walls),
        test_utils::test_fn!(test_wall_records_follow_the_map),
        test_utils::test_fn!(test_walls_round_trip_a_save),
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

// ----------------------------------------------
// Phase 1: walls and gatehouses
// ----------------------------------------------

// A grass map whose cells at(0, 0) to at(30, 30) are all in its playable area (the triangular margins
// of a map's grid hold no tiles).
fn grass_map() -> TestEnvironment {
    TestEnvironment::with_terrain(Size::new(64, 64), test_utils::find_terrain_def("grass"))
}

fn at(x: i32, y: i32) -> Cell {
    Cell::new(x + 16, y + 16)
}

// Draws a wall (or a road) from `from` to `to` as a player's drag does, failing the test if it's refused.
fn draw(env: &mut TestEnvironment, from: Cell, to: Cell, kind: RoadKind) {
    let segment = road::build_segment(&env.tile_map, from, to, kind);
    assert!(segment.is_valid, "{kind:?} from {from} to {to} should be valid");
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &segment, true).expect("the segment");
}

fn mask_at(env: &TestEnvironment, cell: Cell) -> usize {
    env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).expect("a wall piece").variation_index()
}

// The road at `cell`: its tile's name and junction mask (its variation).
fn road_at(env: &TestEnvironment, cell: Cell) -> (&'static str, usize) {
    let tile = env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).expect("a terrain tile");
    assert!(tile.path_kind().is_road(), "a road at {cell}, not {}", tile.name());
    (tile.name(), tile.variation_index())
}

fn gate_variation(env: &TestEnvironment, cell: Cell) -> &'static str {
    let gate = env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).expect("the gatehouse");
    assert!(wall::is_gatehouse(&env.tile_map, cell));
    gate.tile_def().variations[gate.variation_index()].name.as_str()
}

fn place_gatehouse(env: &mut TestEnvironment, cell: Cell, undo_redo: bool) -> PlaceOrClearResult {
    let gatehouse = test_utils::find_building_def("gatehouse");
    TilePlacement::place(&env.new_sim_context(0.0), cell, gatehouse, false, undo_redo, ConstructionMode::Finished)
}

fn path_exists(env: &mut TestEnvironment, kinds: NodeKind, from: Cell, to: Cell) -> bool {
    let context = env.new_sim_context(0.0);
    matches!(context.find_path(kinds, from, to), SearchResult::PathFound(_))
}

// A ring of palisade round the cells from (9, 9) to (15, 15), its corners at (8, 8) and (16, 16). With
// `gate`, the ring is left open at (12, 8) for a road running along x = 12 from (12, 4) to (12, 12).
fn walled_ring(gate: bool) -> TestEnvironment {
    let mut env = grass_map();
    if gate {
        draw(&mut env, at(12, 4), at(12, 12), RoadKind::Dirt);
        draw(&mut env, at(8, 8), at(11, 8), RoadKind::Palisade);
        draw(&mut env, at(13, 8), at(16, 8), RoadKind::Palisade);
    } else {
        draw(&mut env, at(8, 8), at(16, 8), RoadKind::Palisade);
    }
    draw(&mut env, at(16, 9), at(16, 16), RoadKind::Palisade);
    draw(&mut env, at(15, 16), at(8, 16), RoadKind::Palisade);
    draw(&mut env, at(8, 15), at(8, 9), RoadKind::Palisade);
    env
}

const N: usize = 0b1000;
const E: usize = 0b0100;
const S: usize = 0b0010;
const W: usize = 0b0001;

// A wall drawn like a road: a piece per cell, each one's variation its junction mask (north +x, east
// -y, south -x, west +y), and a record per piece at its tier's full strength.
fn test_wall_pieces_join() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::Palisade);
    for y in 8..=12 {
        assert_eq!(wall::tier_at(&env.tile_map, at(10, y)), Some(WallTier::Palisade));
    }
    assert_eq!(mask_at(&env, at(10, 8)), W, "its first piece joins only the next");
    assert_eq!(mask_at(&env, at(10, 10)), E | W);
    assert_eq!(mask_at(&env, at(10, 12)), E);

    // A corner and a tee.
    draw(&mut env, at(11, 12), at(12, 12), RoadKind::Palisade);
    assert_eq!(mask_at(&env, at(10, 12)), E | N);
    draw(&mut env, at(9, 10), at(9, 10), RoadKind::Palisade);
    assert_eq!(mask_at(&env, at(10, 10)), E | W | S);

    let rules = &GameConfigs::get().sim.defence;
    let walls = env.sim.defence().walls();
    assert_eq!(walls.len(), 8);
    assert!(walls.iter().all(|wall| wall.tier == WallTier::Palisade && wall.condition == rules.palisade_strength));
    assert!(walls.windows(2).all(|pair| (pair[0].cell.y, pair[0].cell.x) < (pair[1].cell.y, pair[1].cell.x)), "row order");
}

// Walls go on open land, not over a road, water, a building or a ramp.
fn test_walls_go_on_open_land_only() {
    let mut env = grass_map();
    draw(&mut env, at(5, 10), at(15, 10), RoadKind::Dirt);
    assert!(!road::build_segment(&env.tile_map, at(10, 8), at(10, 12), RoadKind::Rampart).is_valid);
    test_utils::place_terrain_by_def(&mut env, at(20, 20), test_utils::find_water_def());
    assert!(!road::build_segment(&env.tile_map, at(20, 20), at(20, 20), RoadKind::Palisade).is_valid);
    test_utils::spawn_building(&mut env, at(25, 25), "small_well");
    assert!(!road::build_segment(&env.tile_map, at(25, 25), at(25, 25), RoadKind::CityWall).is_valid);
}

// A ring of wall shuts the cells inside it off from those outside, on foot or by road.
fn test_walls_block_searches() {
    let mut env = walled_ring(false);
    let (inside, outside) = (at(12, 12), at(4, 4));
    assert!(!path_exists(&mut env, NodeKind::EmptyLand | NodeKind::Road, inside, outside));
    assert!(path_exists(&mut env, NodeKind::EmptyLand, at(10, 10), at(14, 14)), "open inside");
    assert!(path_exists(&mut env, NodeKind::EmptyLand, at(4, 4), at(20, 20)), "open outside");
    let node = env.tile_map.graph().node_kind(Node::new(at(8, 12))).expect("a node");
    assert!(node.intersects(NodeKind::Wall) && !node.intersects(NodeKind::EmptyLand), "{node}");
}

// The search graph a load builds from the map (Graph::rebuild_from_tile_map) blocks them too.
fn test_walls_block_searches_after_a_load() {
    let env = walled_ring(false);
    let graph = Graph::from_tile_map(&env.tile_map);
    for cell in [at(8, 8), at(12, 8), at(16, 12)] {
        let node = graph.node_kind(Node::new(cell)).expect("a node");
        assert!(node.intersects(NodeKind::Wall) && !node.intersects(NodeKind::EmptyLand), "{cell}: {node}");
    }
    let beside = graph.node_kind(Node::new(at(9, 12))).expect("a node");
    assert!(!beside.intersects(NodeKind::BuildingAccess), "a wall gives no building access: {beside}");
}

// A gatehouse where the road crosses the wall: the city's walkers pass on the road, nothing else.
fn test_gatehouse_lets_the_city_through() {
    let mut env = walled_ring(true);
    let (inside, outside) = (at(12, 12), at(12, 4));
    assert!(path_exists(&mut env, NodeKind::Road, inside, outside), "the road runs through the gap");

    test_utils::place_tile_as_player(&mut env, at(12, 8), test_utils::find_building_def("gatehouse"));
    assert_eq!(gate_variation(&env, at(12, 8)), "palisade_ns", "its walls run north and south");
    assert_eq!(road_at(&env, at(12, 8)), ("dirt_road", E | W), "the road it stands on");
    assert_eq!(mask_at(&env, at(11, 8)) & N, N, "the wall south of it joins it");
    assert_eq!(mask_at(&env, at(13, 8)) & S, S, "the wall north of it joins it");

    let node = env.tile_map.graph().node_kind(Node::new(at(12, 8))).expect("a node");
    assert!(node.contains(NodeKind::Road | NodeKind::Gate), "{node}");
    assert!(path_exists(&mut env, NodeKind::Road, inside, outside), "the city's walkers pass the gatehouse");
    assert!(!path_exists(&mut env, NodeKind::EmptyLand, at(10, 12), at(4, 4)), "off the road, the wall holds");
}

// A gatehouse goes only in a wall: a wall piece on both sides of it along one axis. A piece at the
// end of a wall, or beside one, takes none.
fn test_gatehouse_needs_a_wall_either_side() {
    let mut env = grass_map();
    draw(&mut env, at(12, 4), at(12, 12), RoadKind::Dirt);
    draw(&mut env, at(8, 8), at(11, 8), RoadKind::Rampart);
    let refused = |env: &mut TestEnvironment, cell: Cell| matches!(place_gatehouse(env, cell, false), PlaceOrClearResult::Failed { message, .. } if message.contains("in a wall"));
    assert!(refused(&mut env, at(12, 8)), "a wall on one side only");
    assert!(refused(&mut env, at(12, 6)), "no wall at all");
    assert!(refused(&mut env, at(20, 20)), "open land");
    assert!(refused(&mut env, at(8, 8)), "the wall's end");
    assert!(refused(&mut env, at(10, 9)), "beside the wall");
    assert_eq!(wall::tier_at(&env.tile_map, at(8, 8)), Some(WallTier::Rampart), "the piece refused stays");
    assert_eq!(env.sim.defence().walls().len(), 4);

    draw(&mut env, at(13, 8), at(16, 8), RoadKind::CityWall);
    test_utils::place_tile_as_player(&mut env, at(12, 8), test_utils::find_building_def("gatehouse"));
    assert_eq!(gate_variation(&env, at(12, 8)), "rampart_ns", "the weaker tier's look");
}

// A gatehouse placed on a piece of a wall takes its place: the piece goes (its record with it), a road
// is laid under the gatehouse running through its passage both ways, and the roads drawn up to it on
// either side join it, so the city's walkers pass on the road and nothing else does.
fn test_gatehouse_on_a_wall_piece() {
    let mut env = walled_ring(false);
    let pieces = env.sim.defence().walls().len();
    let (inside, outside) = (at(12, 12), at(12, 4));
    assert!(matches!(place_gatehouse(&mut env, at(12, 8), false), PlaceOrClearResult::PlacedTile(_)));
    assert_eq!(gate_variation(&env, at(12, 8)), "palisade_ns");
    assert!(!wall::is_wall(&env.tile_map, at(12, 8)));
    assert_eq!(env.sim.defence().walls().len(), pieces - 1, "the piece's record goes");
    assert_eq!(road_at(&env, at(12, 8)), ("dirt_road", E | W), "a road through its passage, both ways");
    assert_eq!(mask_at(&env, at(11, 8)), N | S, "the pieces beside it join it");
    assert_eq!(mask_at(&env, at(13, 8)), N | S);

    draw(&mut env, at(12, 4), at(12, 7), RoadKind::Dirt);
    draw(&mut env, at(12, 9), at(12, 12), RoadKind::Dirt);
    assert_eq!(road_at(&env, at(12, 7)).1 & W, W, "the road outside joins it");
    assert_eq!(road_at(&env, at(12, 9)).1 & E, E, "and the road inside");
    let node = env.tile_map.graph().node_kind(Node::new(at(12, 8))).expect("a node");
    assert!(node.contains(NodeKind::Road | NodeKind::Gate), "{node}");
    assert!(path_exists(&mut env, NodeKind::Road, inside, outside), "the city's walkers pass the gatehouse");
    assert!(!path_exists(&mut env, NodeKind::EmptyLand, at(10, 12), at(4, 4)), "off the road, the wall holds");

    // A paved road meeting the wall gets a paved road under its gatehouse.
    let mut env = walled_ring(false);
    draw(&mut env, at(10, 4), at(10, 7), RoadKind::Paved);
    assert!(matches!(place_gatehouse(&mut env, at(10, 8), false), PlaceOrClearResult::PlacedTile(_)));
    assert_eq!(road_at(&env, at(10, 8)), ("paved_road", E | W));
}

// A gatehouse in a gap of open land between two pieces: a road is laid under it too.
fn test_gatehouse_in_a_gap_of_open_land() {
    let mut env = grass_map();
    draw(&mut env, at(10, 6), at(10, 7), RoadKind::Rampart);
    draw(&mut env, at(10, 9), at(10, 10), RoadKind::Rampart);
    assert!(matches!(place_gatehouse(&mut env, at(10, 8), false), PlaceOrClearResult::PlacedTile(_)));
    assert_eq!(gate_variation(&env, at(10, 8)), "rampart_ew", "its walls run east and west");
    assert_eq!(road_at(&env, at(10, 8)), ("dirt_road", N | S), "its passage runs north and south");
    assert_eq!(mask_at(&env, at(10, 7)), E | W);
}

// Roads, paved roads and dykes are never drawn over a wall: only a gatehouse opens one.
fn test_roads_dont_cross_walls() {
    let env = walled_ring(false);
    for kind in [RoadKind::Dirt, RoadKind::Paved, RoadKind::Dyke] {
        assert!(!road::build_segment(&env.tile_map, at(12, 4), at(12, 12), kind).is_valid, "{kind:?} across the wall");
        assert!(!road::build_segment(&env.tile_map, at(12, 8), at(12, 8), kind).is_valid, "{kind:?} on a piece");
    }
    assert!(!road::build_segment(&env.tile_map, at(12, 8), at(12, 8), RoadKind::Canal).is_valid, "a canal on a piece");
}

// A road drawn across the wall through its gatehouse: the road either side of it is laid, and the
// road under the gatehouse stays as it was.
fn test_road_drawn_through_a_gatehouse() {
    let mut env = walled_ring(false);
    assert!(matches!(place_gatehouse(&mut env, at(12, 8), false), PlaceOrClearResult::PlacedTile(_)));
    draw(&mut env, at(12, 4), at(12, 12), RoadKind::Paved);
    assert!(wall::is_gatehouse(&env.tile_map, at(12, 8)), "the gatehouse stays");
    assert_eq!(road_at(&env, at(12, 8)), ("dirt_road", E | W), "and the road under it");
    assert_eq!(road_at(&env, at(12, 7)), ("paved_road", E | W));
    assert_eq!(road_at(&env, at(12, 9)), ("paved_road", E | W));
    assert!(path_exists(&mut env, NodeKind::Road, at(12, 12), at(12, 4)));

    // Not along the wall, nor through a gatehouse from its side.
    assert!(!road::build_segment(&env.tile_map, at(10, 8), at(14, 8), RoadKind::Dirt).is_valid);
}

// Undoing a gatehouse placed on a wall puts the piece back and takes the road laid under it away;
// redo places them again.
fn test_undoing_a_gatehouse_on_a_wall() {
    let mut env = walled_ring(false);
    let pieces = env.sim.defence().walls().len();
    assert!(matches!(place_gatehouse(&mut env, at(12, 8), true), PlaceOrClearResult::PlacedTile(_)));
    undo_redo::undo(&env.new_sim_context(0.0));
    assert!(!wall::is_gatehouse(&env.tile_map, at(12, 8)));
    assert_eq!(wall::tier_at(&env.tile_map, at(12, 8)), Some(WallTier::Palisade), "the piece comes back");
    assert_eq!(mask_at(&env, at(12, 8)), N | S);
    assert_eq!(env.sim.defence().walls().len(), pieces);
    let ground = env.tile_map.try_tile_from_layer(at(12, 8), TileMapLayerKind::Terrain).expect("the ground");
    assert!(!ground.path_kind().is_road(), "the road under it goes: {}", ground.name());
    let node = env.tile_map.graph().node_kind(Node::new(at(12, 8))).expect("a node");
    assert!(node.intersects(NodeKind::Wall) && !node.intersects(NodeKind::Road), "{node}");

    undo_redo::redo(&env.new_sim_context(0.0));
    assert_eq!(gate_variation(&env, at(12, 8)), "palisade_ns");
    assert!(!wall::is_wall(&env.tile_map, at(12, 8)));
    assert_eq!(road_at(&env, at(12, 8)), ("dirt_road", E | W));
    assert_eq!(env.sim.defence().walls().len(), pieces - 1);
}

// Clearing a gatehouse leaves a gap in the wall with the road through it; the pieces beside it close
// up.
fn test_clearing_a_gatehouse() {
    let mut env = walled_ring(false);
    assert!(matches!(place_gatehouse(&mut env, at(12, 8), false), PlaceOrClearResult::PlacedTile(_)));
    draw(&mut env, at(12, 4), at(12, 7), RoadKind::Dirt);
    let tile = env.tile_map.try_tile_from_layer(at(12, 8), TileMapLayerKind::Objects).expect("the gatehouse");
    let tile: *const game::tile::Tile = tile;
    // SAFETY: the tile lives in the map until clear despawns it; nothing else touches the map meanwhile.
    let result = TilePlacement::clear(&env.new_sim_context(0.0), unsafe { &*tile }, false, false);
    assert!(matches!(result, PlaceOrClearResult::ClearedTile(_)));
    assert!(!wall::is_gatehouse(&env.tile_map, at(12, 8)));
    assert_eq!(road_at(&env, at(12, 8)), ("dirt_road", E), "the road through the gap joins the road outside");
    assert_eq!(mask_at(&env, at(11, 8)), S);
    assert_eq!(mask_at(&env, at(13, 8)), N);
}

// A stronger tier drawn over a weaker one replaces it at full strength; a weaker one over a
// stronger one leaves it.
fn test_stronger_tier_replaces_a_weaker() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 14), RoadKind::Palisade);
    defence::damage_wall(&env.new_sim_context(0.0), at(10, 11), 30.0);
    draw(&mut env, at(10, 10), at(10, 12), RoadKind::Rampart);
    let rules = &GameConfigs::get().sim.defence;
    for y in 8..=14 {
        let cell = at(10, y);
        let tier = if (10..=12).contains(&y) { WallTier::Rampart } else { WallTier::Palisade };
        assert_eq!(wall::tier_at(&env.tile_map, cell), Some(tier), "{cell}");
        let record = env.sim.defence().wall_at(cell).expect("a record");
        assert_eq!(record.tier, tier);
        assert_eq!(record.condition, tier.strength(rules), "{cell} at full strength");
    }
    assert_eq!(mask_at(&env, at(10, 10)), E | W, "tiers join each other");

    draw(&mut env, at(10, 9), at(10, 13), RoadKind::Palisade);
    assert_eq!(wall::tier_at(&env.tile_map, at(10, 11)), Some(WallTier::Rampart), "kept");
    assert_eq!(env.sim.defence().walls().len(), 7);
}

// Clearing a piece: the pieces beside it close up, and its record goes.
fn test_clearing_a_piece() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::CityWall);
    let tile = env.tile_map.try_tile_from_layer(at(10, 10), TileMapLayerKind::Objects).expect("a piece");
    let tile: *const game::tile::Tile = tile;
    // SAFETY: the tile lives in the map until clear despawns it; nothing else touches the map meanwhile.
    let result = TilePlacement::clear(&env.new_sim_context(0.0), unsafe { &*tile }, false, false);
    assert!(matches!(result, PlaceOrClearResult::ClearedTile(_)));
    assert!(!wall::is_wall(&env.tile_map, at(10, 10)));
    assert_eq!(mask_at(&env, at(10, 9)), E);
    assert_eq!(mask_at(&env, at(10, 11)), W);
    assert_eq!(env.sim.defence().walls().len(), 4);
    assert!(env.sim.defence().wall_at(at(10, 10)).is_none());
    let node = env.tile_map.graph().node_kind(Node::new(at(10, 10))).expect("a node");
    assert!(node.intersects(NodeKind::EmptyLand) && !node.intersects(NodeKind::Wall), "{node}");
}

// Undo takes a drawn wall away, and its records with it; redo draws it again.
fn test_undoing_a_wall() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::Rampart);
    assert_eq!(env.sim.defence().walls().len(), 5);
    undo_redo::undo(&env.new_sim_context(0.0));
    assert!((8..=12).all(|y| !wall::is_wall(&env.tile_map, at(10, y))));
    assert!(env.sim.defence().walls().is_empty());
    undo_redo::redo(&env.new_sim_context(0.0));
    assert!((8..=12).all(|y| wall::tier_at(&env.tile_map, at(10, y)) == Some(WallTier::Rampart)));
    assert_eq!(env.sim.defence().walls().len(), 5);
    assert_eq!(mask_at(&env, at(10, 10)), E | W);
}

// Undoing a stronger tier drawn over a weaker one puts the weaker pieces back (whole); redo replaces
// them again.
fn test_undoing_a_stronger_tier() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::Palisade);
    draw(&mut env, at(10, 10), at(10, 14), RoadKind::CityWall);
    let tiers = |env: &TestEnvironment| (8..=14).map(|y| wall::tier_at(&env.tile_map, at(10, y))).collect::<Vec<_>>();
    let (p, c) = (Some(WallTier::Palisade), Some(WallTier::CityWall));
    assert_eq!(tiers(&env), [p, p, c, c, c, c, c]);

    undo_redo::undo(&env.new_sim_context(0.0));
    assert_eq!(tiers(&env), [p, p, p, p, p, None, None], "the palisade comes back under it");
    assert_eq!(mask_at(&env, at(10, 12)), E);
    assert_eq!(env.sim.defence().walls().len(), 5);
    assert!(env.sim.defence().walls().iter().all(|wall| wall.tier == WallTier::Palisade));

    undo_redo::redo(&env.new_sim_context(0.0));
    assert_eq!(tiers(&env), [p, p, c, c, c, c, c]);
    assert_eq!(env.sim.defence().walls().len(), 7);
}

// Below half its strength a piece shows its cracks; at 0 it breaches: rubble in its place, the
// pieces beside it closed up, its record gone and a notice for the log.
fn test_damage_shows_cracks_and_breaches() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::Palisade);
    let cell = at(10, 10);
    let cracked = |env: &TestEnvironment| {
        env.tile_map
            .try_tile_from_layer(cell, TileMapLayerKind::Objects)
            .is_some_and(|tile| tile.has_flags(TileFlags::WallDamaged))
    };

    assert_eq!(defence::damage_wall(&env.new_sim_context(0.0), cell, 40.0), Some(false));
    assert!(!cracked(&env), "60 of 100 is whole enough");
    assert_eq!(defence::damage_wall(&env.new_sim_context(0.0), cell, 20.0), Some(false));
    assert!(cracked(&env), "40 of 100 shows its cracks");
    assert_eq!(env.sim.defence().wall_at(cell).unwrap().condition, 40.0);
    assert_eq!(defence::damage_wall(&env.new_sim_context(0.0), at(20, 20), 10.0), None, "no wall there");

    assert_eq!(defence::damage_wall(&env.new_sim_context(0.0), cell, 50.0), Some(true));
    assert!(!wall::is_wall(&env.tile_map, cell));
    assert!(rubble::is_rubble(&env.tile_map, cell), "rubble in the gap");
    assert!(env.sim.defence().wall_at(cell).is_none());
    assert_eq!(env.sim.defence().walls().len(), 4);
    assert_eq!(mask_at(&env, at(10, 9)), E);
    assert_eq!(mask_at(&env, at(10, 11)), W);
    let notices = env.sim.defence_mut().take_notices();
    assert_eq!(notices, vec![DefenceNotice::Breach { cell, tier: WallTier::Palisade }]);
    let (message, _) = defence::notice_message(&notices[0]);
    assert!(message.text.contains("palisade"), "{}", message.text);
}

// A breached piece opens the ring: its rubble is no wall, and what crosses rubble (Phase 2's raiders)
// gets through; the player clears it and draws the wall again.
fn test_breached_cell_opens() {
    let mut env = walled_ring(false);
    let gap = at(12, 8);
    assert!(!path_exists(&mut env, NodeKind::EmptyLand | NodeKind::Rocks, at(12, 12), at(4, 4)));
    assert!(defence::set_wall_condition(&env.new_sim_context(0.0), gap, 0.0));
    let node = env.tile_map.graph().node_kind(Node::new(gap)).expect("a node");
    assert!(!node.intersects(NodeKind::Wall) && node.intersects(NodeKind::Rocks), "{node}");
    assert!(path_exists(&mut env, NodeKind::EmptyLand | NodeKind::Rocks, at(12, 12), at(4, 4)));
}

// A builders' guild walker beside a damaged piece mends it by the rules' share of its strength, once
// a month; a whole piece needs nothing.
fn test_builders_repair_walls() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::Rampart);
    let rules = &GameConfigs::get().sim.defence;
    let cell = at(10, 10);
    let beside = at(11, 10);
    assert!(!defence::has_damaged_wall_beside(&env.new_sim_context(0.0), beside), "whole");

    defence::damage_wall(&env.new_sim_context(0.0), cell, 200.0); // 50 of 250.
    assert!(defence::has_damaged_wall_beside(&env.new_sim_context(0.0), beside));
    assert!(!defence::has_damaged_wall_beside(&env.new_sim_context(0.0), at(12, 10)), "two cells off");
    defence::builder_passed(&env.new_sim_context(0.0), beside);
    let mended = *env.sim.defence().wall_at(cell).unwrap();
    assert_eq!(mended.condition, 50.0 + rules.rampart_strength * rules.repair_share);
    assert!(mended.repaired.is_some());

    defence::builder_passed(&env.new_sim_context(0.0), beside);
    assert_eq!(env.sim.defence().wall_at(cell).unwrap().condition, 100.0, "once a month");

    // Next month, again, until the cracks go.
    let next = mended.repaired.unwrap() + 1;
    {
        let context = env.new_sim_context(0.0);
        assert!(context.defence_mut().repair_wall(cell, next, rules));
        assert!(context.defence_mut().repair_wall(cell, next + 1, rules));
        defence::refresh_wall_flags(&context);
    }
    let tile = env.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).unwrap();
    assert!(!tile.has_flags(TileFlags::WallDamaged), "200 of 250");
}

// A piece placed some other way (the editor, a preset) is matched at the month's start.
fn test_wall_records_follow_the_map() {
    let mut env = grass_map();
    let palisade = wall::tile_def(WallTier::Palisade).expect("the palisade's tile def");
    test_utils::place_terrain_by_def(&mut env, at(10, 10), palisade); // Through SimCmds, as a preset's.
    assert!(wall::is_wall(&env.tile_map, at(10, 10)));
    assert!(env.sim.defence().walls().is_empty());
    defence::on_new_month(&env.new_sim_context(0.0), CalendarDate { year: 1, month: 2, day: 1 });
    assert_eq!(env.sim.defence().walls().len(), 1);
}

fn test_walls_round_trip_a_save() {
    let mut env = grass_map();
    draw(&mut env, at(10, 8), at(10, 12), RoadKind::CityWall);
    defence::damage_wall(&env.new_sim_context(0.0), at(10, 9), 123.0);
    let json = serde_json::to_string(env.sim.defence()).unwrap();
    let back: CityDefence = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    let wall = back.wall_at(at(10, 9)).expect("the piece");
    assert_eq!((wall.tier, wall.condition), (WallTier::CityWall, 377.0));
    let _ = (BuildingKind::Gatehouse, TileKind::Wall);
}
