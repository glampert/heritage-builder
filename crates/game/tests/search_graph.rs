use common::{
    Size,
    coords::{Cell, CellRange},
};
use game::{
    building::BuildingKind,
    pathfind::{self, AStarUniformCostHeuristic, Node, NodeKind, Path, Search, SearchResult},
    tile::{
        TileKind,
        TileMap,
        TileMapLayerKind,
        elevation::{Dir, Half, Shape},
        placement::{TileClearingErr, TilePlacementErr},
    },
    unit::config::UnitConfigKey,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for pathfind::Graph
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Search Graph", &[
        test_utils::test_fn!(test_vacant_lot_lifecycle),
        test_utils::test_fn!(test_vacant_lot_obstructed_by_building),
        test_utils::test_fn!(test_non_vacant_terrain_restored_on_object_clear),
        test_utils::test_fn!(test_vacant_lot_counter_no_underflow),
        // Terrain height (docs/plans/terrain-height.md, §6):
        test_utils::test_fn!(test_a_cliff_blocks_and_a_ramp_lets_walkers_by),
        test_utils::test_fn!(test_a_ramp_chain_climbs_levels),
        test_utils::test_fn!(test_ramps_cost_more_than_flat_ground),
        test_utils::test_fn!(test_road_links_stay_on_their_level),
        test_utils::test_fn!(test_buildings_are_reached_on_their_level),
        test_utils::test_fn!(test_a_walker_stops_where_a_cliff_rises),
    ]);
}

trait OkOrPanic<T> {
    fn ok_or_panic(self, msg: &str) -> T;
}

impl<T> OkOrPanic<T> for Result<T, TilePlacementErr> {
    fn ok_or_panic(self, msg: &str) -> T {
        self.unwrap_or_else(|e| panic!("{msg}: {}", e.message))
    }
}

impl<T> OkOrPanic<T> for Result<T, TileClearingErr> {
    fn ok_or_panic(self, msg: &str) -> T {
        self.unwrap_or_else(|e| panic!("{msg}: {}", e.message))
    }
}

const MAP_SIZE_IN_CELLS: Size = Size::new(32, 32);

// Place a VacantLot, confirm the graph reports it; clear the VacantLot,
// confirm the graph no longer reports it. Guards the basic diff-based
// counter path (terrain-only, no Object overlay).
fn test_vacant_lot_lifecycle() {
    let mut tile_map = TileMap::new(MAP_SIZE_IN_CELLS, None);
    let vacant_lot = test_utils::find_terrain_def("vacant_lot");
    let cell = Cell::new(5, 5);

    assert!(!tile_map.graph().has_vacant_lot_nodes());

    tile_map.try_place_tile(cell, vacant_lot).ok_or_panic("place vacant_lot");
    assert!(tile_map.graph().has_vacant_lot_nodes());
    assert!(tile_map.graph().node_kind(Node::new(cell)).unwrap().is_vacant_lot());

    tile_map.try_clear_tile_from_layer(cell, TileMapLayerKind::Terrain).ok_or_panic("clear vacant_lot");
    assert!(!tile_map.graph().has_vacant_lot_nodes(), "Counter did not decrement when VacantLot terrain was cleared");
    assert!(!tile_map.graph().node_kind(Node::new(cell)).unwrap().is_vacant_lot());
}

// A house placed over a VacantLot makes the lot inaccessible to settlers.
// The graph must report zero vacant lots while the building is there, and
// again report the lot as available once the building is removed.
fn test_vacant_lot_obstructed_by_building() {
    let mut tile_map = TileMap::new(MAP_SIZE_IN_CELLS, None);
    let vacant_lot = test_utils::find_terrain_def("vacant_lot");
    let house = test_utils::find_building_def("house0");
    let cell = Cell::new(7, 7);

    tile_map.try_place_tile(cell, vacant_lot).ok_or_panic("place vacant_lot");
    assert!(tile_map.graph().has_vacant_lot_nodes());

    tile_map.try_place_tile(cell, house).ok_or_panic("place house on vacant_lot");
    assert!(!tile_map.graph().has_vacant_lot_nodes(), "VacantLot should not be counted while obstructed by a building");

    let obstructed_kind = tile_map.graph().node_kind(Node::new(cell)).unwrap();
    assert!(obstructed_kind.is_building());
    assert!(!obstructed_kind.is_vacant_lot());

    tile_map.try_clear_tile_from_layer(cell, TileMapLayerKind::Objects).ok_or_panic("clear house");
    assert!(
        tile_map.graph().has_vacant_lot_nodes(),
        "VacantLot should be reclaimed once the obstructing building is cleared"
    );

    let restored_kind = tile_map.graph().node_kind(Node::new(cell)).unwrap();
    assert!(restored_kind.is_vacant_lot());
    assert!(!restored_kind.is_building());
}

// Regression guard for the Objects-clear path: after removing an Object,
// the graph node should reflect the underlying Terrain's path_kind, not a
// hard-coded EmptyLand. Uses a road under a blocker-style tile via
// small_well placement (buildings can sit over roads only via the road
// link logic; here we just assert path_kind restoration on clear).
fn test_non_vacant_terrain_restored_on_object_clear() {
    let mut tile_map = TileMap::new(MAP_SIZE_IN_CELLS, None);
    let dirt_road = test_utils::find_terrain_def("dirt_road");
    let cell = Cell::new(3, 3);

    tile_map.try_place_tile(cell, dirt_road).ok_or_panic("place dirt_road");
    let road_kind = tile_map.graph().node_kind(Node::new(cell)).unwrap();
    assert!(road_kind.is_road());

    // Sanity check: clearing a Terrain tile zeroes the node.
    tile_map.try_clear_tile_from_layer(cell, TileMapLayerKind::Terrain).ok_or_panic("clear road");
    assert!(tile_map.graph().node_kind(Node::new(cell)).unwrap().is_empty());

    // Now exercise Object-layer clear path under a VacantLot and confirm
    // the Terrain's VacantLot flag is restored (not silently replaced
    // with EmptyLand like the old behavior).
    let vacant_lot = test_utils::find_terrain_def("vacant_lot");
    let house = test_utils::find_building_def("house0");
    let lot_cell = Cell::new(9, 9);

    tile_map.try_place_tile(lot_cell, vacant_lot).ok_or_panic("place vacant_lot");
    tile_map.try_place_tile(lot_cell, house).ok_or_panic("place house");
    tile_map.try_clear_tile_from_layer(lot_cell, TileMapLayerKind::Objects).ok_or_panic("clear house");

    assert!(
        tile_map.graph().node_kind(Node::new(lot_cell)).unwrap().is_vacant_lot(),
        "Terrain's VacantLot path_kind must be restored when the Object above is cleared"
    );

    // TileMap.find_tile should still see the Terrain tile underneath.
    assert!(tile_map.find_tile(lot_cell, TileKind::Terrain).is_some());
}

// Repeatedly place/clear VacantLot terrain to exercise the diff-based
// counter. Would trip the debug_assert in set_node_kind_internal if the
// counter ever went negative.
fn test_vacant_lot_counter_no_underflow() {
    let mut tile_map = TileMap::new(MAP_SIZE_IN_CELLS, None);
    let vacant_lot = test_utils::find_terrain_def("vacant_lot");
    let cell = Cell::new(1, 1);

    for _ in 0..5 {
        tile_map.try_place_tile(cell, vacant_lot).ok_or_panic("place vacant_lot");
        assert!(tile_map.graph().has_vacant_lot_nodes());

        tile_map.try_clear_tile_from_layer(cell, TileMapLayerKind::Terrain).ok_or_panic("clear vacant_lot");
        assert!(!tile_map.graph().has_vacant_lot_nodes());
    }
}

// ----------------------------------------------
// Terrain height
// ----------------------------------------------

fn grass_map() -> TileMap {
    TileMap::new(MAP_SIZE_IN_CELLS, Some(test_utils::find_terrain_def("grass")))
}

fn set_cells(tile_map: &mut TileMap, from: (i32, i32), to: (i32, i32), level: u8, shape: Shape) {
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            tile_map.set_elevation(Cell::new(x, y), level, shape);
        }
    }
}

// The map's playable area is a diamond in cell space: rows 15 and 16 run its whole width, so the
// layouts below lie along them.
//
// A plateau at level 1 over x 16 to 31 (the map's whole height), with nothing to climb it by.
fn plateau_map() -> TileMap {
    let mut tile_map = grass_map();
    set_cells(&mut tile_map, (16, 0), (31, 31), 1, Shape::Flat);
    tile_map
}

// The plateau's ramp: up to the north (+x) along row 15, its foot at (14, 15) and its head at
// (15, 15).
fn add_ramp(tile_map: &mut TileMap) {
    tile_map.set_elevation(Cell::new(14, 15), 0, Shape::Ramp(Dir::N, Half::Low));
    tile_map.set_elevation(Cell::new(15, 15), 0, Shape::Ramp(Dir::N, Half::High));
}

fn find_land_path(tile_map: &TileMap, from: (i32, i32), to: (i32, i32)) -> Option<Path> {
    let mut search = Search::with_graph(tile_map.graph());
    match search.find_path(
        tile_map.graph(),
        &AStarUniformCostHeuristic::new(),
        NodeKind::EmptyLand | NodeKind::Road,
        Node::new(Cell::new(from.0, from.1)),
        Node::new(Cell::new(to.0, to.1)),
    ) {
        SearchResult::PathFound(path) => Some(path.clone()),
        SearchResult::PathNotFound => None,
    }
}

fn passes(path: &Path, x: i32, y: i32) -> bool {
    path.iter().any(|node| node.cell == Cell::new(x, y))
}

// Every step of a path is between cells whose ground joins.
fn assert_every_step_joins(tile_map: &TileMap, path: &Path) {
    for step in path.windows(2) {
        assert!(
            tile_map.elevation().joins(step[0].cell, step[1].cell),
            "the step from {} to {} crosses a cliff",
            step[0].cell,
            step[1].cell
        );
    }
}

fn assert_no_path(tile_map: &TileMap, from: (i32, i32), to: (i32, i32), why: &str) {
    assert!(find_land_path(tile_map, from, to).is_none(), "{why}");
}

// No walker crosses a cliff; a ramp takes them up and down, by its way only.
fn test_a_cliff_blocks_and_a_ramp_lets_walkers_by() {
    let mut tile_map = plateau_map();
    assert_eq!(tile_map.graph().elevation().byte(Cell::new(16, 3)), tile_map.elevation().byte(Cell::new(16, 3)));
    assert_no_path(&tile_map, (5, 15), (25, 15), "the cliff blocks the way up");
    assert_no_path(&tile_map, (25, 15), (5, 15), "and the way down");

    add_ramp(&mut tile_map);
    let path = find_land_path(&tile_map, (5, 15), (25, 15)).expect("the ramp leads up");
    assert_eq!(path.len(), 21, "straight up it: {path:?}");
    assert_every_step_joins(&tile_map, &path);

    let path = find_land_path(&tile_map, (22, 12), (6, 13)).expect("and down, from along the plateau");
    assert!(passes(&path, 15, 15) && passes(&path, 14, 15), "by the ramp: {path:?}");
    assert_every_step_joins(&tile_map, &path);

    // Not over its sides: the ground beside its foot doesn't join it.
    let graph = tile_map.graph();
    assert!(!graph.neighbors(Node::new(Cell::new(14, 14)), NodeKind::EmptyLand).contains(&Node::new(Cell::new(14, 15))));
    assert!(graph.neighbors(Node::new(Cell::new(13, 15)), NodeKind::EmptyLand).contains(&Node::new(Cell::new(14, 15))));

    // The graph is built afresh with the heights too.
    tile_map.reset_search_graph();
    assert!(find_land_path(&tile_map, (5, 15), (25, 15)).is_some());
    // And a height edit reaches it: the ramp levelled leaves a cliff.
    tile_map.set_elevation(Cell::new(15, 15), 0, Shape::Flat);
    tile_map.set_elevation(Cell::new(14, 15), 0, Shape::Flat);
    assert_no_path(&tile_map, (5, 15), (25, 15), "the ramp is gone");
}

// Ramps facing the same way, end to end, climb a level each: two take walkers up two levels.
fn test_a_ramp_chain_climbs_levels() {
    let mut tile_map = grass_map();
    set_cells(&mut tile_map, (12, 0), (31, 31), 2, Shape::Flat);
    tile_map.set_elevation(Cell::new(12, 15), 0, Shape::Ramp(Dir::N, Half::Low));
    tile_map.set_elevation(Cell::new(13, 15), 0, Shape::Ramp(Dir::N, Half::High));
    tile_map.set_elevation(Cell::new(14, 15), 1, Shape::Ramp(Dir::N, Half::Low));
    tile_map.set_elevation(Cell::new(15, 15), 1, Shape::Ramp(Dir::N, Half::High));
    assert!(tile_map.elevation().problems().is_empty(), "{:?}", tile_map.elevation().problems());

    let path = find_land_path(&tile_map, (3, 15), (25, 15)).expect("up the chain");
    assert_eq!(path.len(), 23, "{path:?}");
    assert_every_step_joins(&tile_map, &path);
    assert_eq!(tile_map.elevation().level(path.last().unwrap().cell), 2);

    // A landing at level 1 in place of the upper ramp leaves a cliff up to level 2.
    tile_map.set_elevation(Cell::new(14, 15), 1, Shape::Flat);
    tile_map.set_elevation(Cell::new(15, 15), 1, Shape::Flat);
    assert!(find_land_path(&tile_map, (3, 15), (15, 15)).is_some(), "the first ramp still reaches the landing");
    assert_no_path(&tile_map, (3, 15), (25, 15), "but nothing climbs from it");
}

// Q7: a step onto, along or off a ramp costs half as much again as a flat one, and takes as much
// longer, so a path goes round a hump about as short; a flat map's steps cost 1.
fn test_ramps_cost_more_than_flat_ground() {
    let mut tile_map = grass_map();
    let (a, b) = (Node::new(Cell::new(8, 15)), Node::new(Cell::new(9, 15)));
    assert_eq!((tile_map.graph().step_cost(a, b), tile_map.graph().cost_scale()), (1, 1));
    assert_eq!(tile_map.graph().step_time_scale(a, b), 1.0);

    // A hump across row 15: up a ramp to (11, 15) at level 1 and down another.
    tile_map.set_elevation(Cell::new(9, 15), 0, Shape::Ramp(Dir::N, Half::Low));
    tile_map.set_elevation(Cell::new(10, 15), 0, Shape::Ramp(Dir::N, Half::High));
    tile_map.set_elevation(Cell::new(11, 15), 1, Shape::Flat);
    tile_map.set_elevation(Cell::new(12, 15), 0, Shape::Ramp(Dir::S, Half::High));
    tile_map.set_elevation(Cell::new(13, 15), 0, Shape::Ramp(Dir::S, Half::Low));
    assert!(tile_map.elevation().problems().is_empty(), "{:?}", tile_map.elevation().problems());

    let graph = tile_map.graph();
    assert_eq!((graph.step_cost(a, b), graph.cost_scale()), (3, 2));
    assert_eq!(graph.step_cost(Node::new(Cell::new(8, 14)), Node::new(Cell::new(9, 14))), 2);
    assert_eq!(graph.step_time_scale(a, b), 1.5);

    let over = find_land_path(&tile_map, (8, 15), (11, 15)).expect("onto the hump");
    assert_every_step_joins(&tile_map, &over);
    // Straight over it is 6 ramp steps (18); round it 8 flat ones (16).
    let path = find_land_path(&tile_map, (8, 15), (14, 15)).expect("past the hump");
    assert_eq!(path.len(), 9, "round it: {path:?}");
    assert!(!passes(&path, 11, 15), "not over the top: {path:?}");
}

// A building's road link is a road beside it on its level (or a ramp up to it), never across a cliff.
fn test_road_links_stay_on_their_level() {
    let mut tile_map = plateau_map();
    add_ramp(&mut tile_map);
    let road = test_utils::find_terrain_def("dirt_road");
    let footprint = CellRange::new(Cell::new(16, 13), Cell::new(16, 13)); // On the plateau's edge.

    tile_map.try_place_tile(Cell::new(15, 13), road).ok_or_panic("road below the cliff");
    assert_eq!(pathfind::find_nearest_road_link(tile_map.graph(), footprint), None, "not up the cliff");

    tile_map.try_place_tile(Cell::new(16, 12), road).ok_or_panic("road on the plateau");
    assert_eq!(pathfind::find_nearest_road_link(tile_map.graph(), footprint), Some(Cell::new(16, 12)));

    // A ramp's head joins the plateau.
    tile_map.try_place_tile(Cell::new(15, 15), road).ok_or_panic("road on the ramp's head");
    let beside_ramp = CellRange::new(Cell::new(16, 15), Cell::new(16, 15));
    assert_eq!(pathfind::find_nearest_road_link(tile_map.graph(), beside_ramp), Some(Cell::new(15, 15)));
}

// A walker finds a building only from its level: the cells below a cliff beside it don't count.
fn test_buildings_are_reached_on_their_level() {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE_IN_CELLS, test_utils::find_terrain_def("grass"));
    set_cells(&mut env.tile_map, (16, 0), (31, 31), 1, Shape::Flat);
    test_utils::spawn_building(&mut env, Cell::new(16, 13), "small_well");

    let find = |env: &mut TestEnvironment| {
        let context = env.new_sim_context(0.0);
        context
            .find_nearest_buildings(Cell::new(14, 13), BuildingKind::SmallWell, NodeKind::EmptyLand, Some(30), |_, _| false)
            .map(|(_, path)| path.clone())
    };
    assert!(find(&mut env).is_none(), "not from below the cliff, though (15, 13) is beside it");

    add_ramp(&mut env.tile_map);
    let path = find(&mut env).expect("up the ramp");
    assert!(passes(&path, 15, 15), "{path:?}");
    let end = path.last().unwrap().cell;
    assert_eq!(env.tile_map.elevation().level(end), 1, "it ends on the plateau, at {end}");
}

// A walker whose way is cut by a cliff raised after it set off stops before it (and its task paths
// again).
fn test_a_walker_stops_where_a_cliff_rises() {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE_IN_CELLS, test_utils::find_terrain_def("grass"));
    let unit_id = test_utils::spawn_unit(&mut env, Cell::new(4, 15), UnitConfigKey::Peasant);
    let kinds = NodeKind::EmptyLand | NodeKind::Road;
    let path = find_land_path(&env.tile_map, (4, 15), (27, 15)).expect("a flat way");
    {
        let unit = env.world.find_unit_mut(unit_id).unwrap();
        unit.set_traversable_node_kinds(kinds);
        unit.follow_path(Some(&path));
    }
    test_utils::tick_until(&mut env, 400, TestEnvironment::TICK_DELTA_SECS, |env| {
        test_utils::find_unit(env, unit_id).cell().x >= 8
    });

    // A wall a level high across the way.
    set_cells(&mut env.tile_map, (19, 0), (19, 31), 1, Shape::Flat);
    test_utils::tick_until(&mut env, 400, TestEnvironment::TICK_DELTA_SECS, |env| {
        test_utils::find_unit(env, unit_id).path_is_blocked()
    });
    let unit = test_utils::find_unit(&env, unit_id);
    assert!(!unit.is_following_path());
    assert_eq!(unit.cell(), Cell::new(18, 15), "it stopped at the foot of the cliff");
}
