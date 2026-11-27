use common::{
    Size,
    coords::{Cell, CellRange},
};
use game::{
    building::BuildingKind,
    pathfind::{self, AStarUniformCostHeuristic, Connectivity, Node, NodeKind, Path, Search, SearchResult},
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
        // Eight-direction movement (docs/plans/unit-movement.md, Phase 2):
        test_utils::test_fn!(test_open_land_paths_go_diagonally),
        test_utils::test_fn!(test_no_corner_cutting_between_buildings),
        test_utils::test_fn!(test_diagonals_keep_to_one_level),
        test_utils::test_fn!(test_road_paths_keep_four_neighbours),
        test_utils::test_fn!(test_diagonal_paths_are_deterministic),
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

fn find_road_path(tile_map: &TileMap, from: (i32, i32), to: (i32, i32)) -> Option<Path> {
    let mut search = Search::with_graph(tile_map.graph());
    match search.find_path(
        tile_map.graph(),
        &AStarUniformCostHeuristic::new(),
        NodeKind::Road,
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

// Every step of a path is between cells whose ground joins (a diagonal one: all four of its cells
// flat at one level).
fn assert_every_step_joins(tile_map: &TileMap, path: &Path) {
    for step in path.windows(2) {
        let joins = if step[0].is_diagonal_to(step[1]) {
            tile_map.graph().steps_diagonally(step[0], step[1])
        } else {
            tile_map.elevation().joins(step[0].cell, step[1].cell)
        };
        assert!(joins, "the step from {} to {} crosses a cliff", step[0].cell, step[1].cell);
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
// longer, so a path goes round a hump about as short; a flat map's road steps cost 1. An eight-way
// search (over open land) costs 10 a straight step, 14 a diagonal and 15 a ramp step on every map.
fn test_ramps_cost_more_than_flat_ground() {
    const FOUR: Connectivity = Connectivity::Four;
    const EIGHT: Connectivity = Connectivity::Eight;
    let mut tile_map = grass_map();
    let (a, b) = (Node::new(Cell::new(8, 15)), Node::new(Cell::new(9, 15)));
    let diagonal = Node::new(Cell::new(9, 14));
    assert_eq!((tile_map.graph().step_cost(FOUR, a, b), tile_map.graph().cost_scale(FOUR)), (1, 1));
    assert_eq!((tile_map.graph().step_cost(EIGHT, a, b), tile_map.graph().cost_scale(EIGHT)), (10, 10));
    assert_eq!(tile_map.graph().step_cost(EIGHT, a, diagonal), 14);
    assert_eq!(tile_map.graph().step_time_scale(a, b), 1.0);

    // A hump across row 15: up a ramp to (11, 15) at level 1 and down another.
    tile_map.set_elevation(Cell::new(9, 15), 0, Shape::Ramp(Dir::N, Half::Low));
    tile_map.set_elevation(Cell::new(10, 15), 0, Shape::Ramp(Dir::N, Half::High));
    tile_map.set_elevation(Cell::new(11, 15), 1, Shape::Flat);
    tile_map.set_elevation(Cell::new(12, 15), 0, Shape::Ramp(Dir::S, Half::High));
    tile_map.set_elevation(Cell::new(13, 15), 0, Shape::Ramp(Dir::S, Half::Low));
    assert!(tile_map.elevation().problems().is_empty(), "{:?}", tile_map.elevation().problems());

    let graph = tile_map.graph();
    assert_eq!((graph.step_cost(FOUR, a, b), graph.cost_scale(FOUR)), (3, 2));
    assert_eq!(graph.step_cost(FOUR, Node::new(Cell::new(8, 14)), Node::new(Cell::new(9, 14))), 2);
    assert_eq!((graph.step_cost(EIGHT, a, b), graph.cost_scale(EIGHT)), (15, 10));
    assert_eq!(graph.step_cost(EIGHT, Node::new(Cell::new(8, 14)), Node::new(Cell::new(9, 14))), 10);
    assert_eq!(graph.step_time_scale(a, b), 1.5);

    let over = find_land_path(&tile_map, (8, 15), (11, 15)).expect("onto the hump");
    assert_every_step_joins(&tile_map, &over);
    // Straight over it is 6 ramp steps (90); round it 8 flat ones (80, and 2 for turning: no diagonal
    // reaches the ramps' sides).
    let path = find_land_path(&tile_map, (8, 15), (14, 15)).expect("past the hump");
    assert_eq!(path.len(), 9, "round it: {path:?}");
    assert!(!passes(&path, 11, 15), "not over the top: {path:?}");
    assert_every_step_joins(&tile_map, &path);

    // Road only (four neighbours, 2 a flat step and 3 a ramp one): over it 18, round it 16.
    let road = test_utils::find_terrain_def("dirt_road");
    for y in 13..=17 {
        for x in 8..=14 {
            tile_map.try_place_tile(Cell::new(x, y), road).ok_or_panic("road over the hump");
        }
    }
    let path = find_road_path(&tile_map, (8, 15), (14, 15)).expect("past the hump by road");
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

// ----------------------------------------------
// Eight-direction movement
// ----------------------------------------------

// A path's steps as (dx, dy).
fn steps_of(path: &Path) -> Vec<(i32, i32)> {
    path.windows(2).map(|step| (step[1].cell.x - step[0].cell.x, step[1].cell.y - step[0].cell.y)).collect()
}

fn diagonal_steps(path: &Path) -> usize {
    steps_of(path).iter().filter(|(dx, dy)| *dx != 0 && *dy != 0).count()
}

fn turns(path: &Path) -> usize {
    steps_of(path).windows(2).filter(|pair| pair[0] != pair[1]).count()
}

// Over open land a path goes diagonally: as many cells as the larger difference, its diagonals in one
// run (the cost for turning), where four neighbours took as many as both differences.
fn test_open_land_paths_go_diagonally() {
    let tile_map = grass_map();

    let path = find_land_path(&tile_map, (8, 10), (18, 20)).expect("across the grass");
    assert_eq!(path.len(), 11, "straight along the diagonal: {path:?}");
    assert_eq!((diagonal_steps(&path), turns(&path)), (10, 0), "{path:?}");

    let path = find_land_path(&tile_map, (8, 13), (20, 17)).expect("and at a slant");
    assert_eq!(path.len(), 13, "{path:?}");
    assert_eq!((diagonal_steps(&path), turns(&path)), (4, 1), "4 diagonal steps, 8 straight, one turn: {path:?}");

    // The four-way path would be 12 + 4 cells: a road-only search still takes them.
    let road = test_utils::find_terrain_def("dirt_road");
    let mut tile_map = TileMap::new(MAP_SIZE_IN_CELLS, Some(road));
    let path = find_road_path(&tile_map, (8, 13), (20, 17)).expect("by road");
    assert_eq!((path.len(), diagonal_steps(&path)), (17, 0), "{path:?}");
    // A walker over open land (and roads) moves diagonally on roads too (Q3).
    tile_map.try_place_tile(Cell::new(30, 30), test_utils::find_terrain_def("grass")).ok_or_panic("a grass cell");
    let path = find_land_path(&tile_map, (8, 13), (20, 17)).expect("over the road");
    assert_eq!((path.len(), diagonal_steps(&path)), (13, 4), "{path:?}");
}

// A diagonal step needs both cells beside it open (Q2): no slipping between two buildings that
// touch at a corner, nor past one's corner.
fn test_no_corner_cutting_between_buildings() {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE_IN_CELLS, test_utils::find_terrain_def("grass"));
    test_utils::spawn_building(&mut env, Cell::new(10, 14), "small_well");
    test_utils::spawn_building(&mut env, Cell::new(11, 15), "small_well");
    let graph = env.tile_map.graph();
    let land = NodeKind::EmptyLand | NodeKind::Road;
    let (a, b) = (Node::new(Cell::new(10, 15)), Node::new(Cell::new(11, 14)));
    assert!(!graph.steps_past(a, b, land), "not between the two wells");
    assert!(!graph.neighbors_with(a, land, Connectivity::Eight).contains(&b));
    assert!(graph.neighbors_with(a, land, Connectivity::Eight).contains(&Node::new(Cell::new(9, 16))), "open land is");

    let path = find_land_path(&env.tile_map, (10, 15), (11, 14)).expect("round a well");
    assert!(path.len() > 3, "the long way: {path:?}");
    for step in path.windows(2) {
        assert!(
            !step[0].is_diagonal_to(step[1]) || graph.steps_past(step[0], step[1], land),
            "the step from {} to {} cuts a corner",
            step[0].cell,
            step[1].cell
        );
        for node in step {
            assert!(graph.node_kind(*node).unwrap().intersects(land), "onto a well at {}", node.cell);
        }
    }
    // Past one well's corner: from beside it to beyond its other side, it goes round, not across.
    let path = find_land_path(&env.tile_map, (9, 14), (10, 13)).expect("round the corner");
    assert_eq!(path.len(), 3, "by (9, 13): {path:?}");
    assert!(passes(&path, 9, 13), "{path:?}");
}

// A diagonal step keeps to flat ground at one level, its sides included: never onto a ramp (ramps are
// taken straight), across a cliff's corner or between levels.
fn test_diagonals_keep_to_one_level() {
    let land = NodeKind::EmptyLand;
    let node = |x: i32, y: i32| Node::new(Cell::new(x, y));

    let mut tile_map = plateau_map();
    add_ramp(&mut tile_map);
    let graph = tile_map.graph();
    let next_to = |from: Node| graph.neighbors_with(from, land, Connectivity::Eight);

    // Onto the ramp's foot (14, 15) diagonally from below it: no; straight, by its way: yes.
    assert!(!next_to(node(13, 14)).contains(&node(14, 15)));
    assert!(!next_to(node(13, 16)).contains(&node(14, 15)));
    assert!(next_to(node(13, 15)).contains(&node(14, 15)));
    // Nor past it, though both ends are flat at 0: the ramp is beside the step.
    assert!(!graph.steps_diagonally(node(13, 15), node(14, 14)));
    assert!(!next_to(node(13, 15)).contains(&node(14, 14)));
    // Between levels at the cliff: (15, 14) at 0 and (16, 13) at 1, either way.
    assert!(!next_to(node(15, 14)).contains(&node(16, 13)));
    assert!(!next_to(node(16, 13)).contains(&node(15, 14)));
    // Along the cliff's foot at 0, and on the plateau at 1, all four cells at one level: yes.
    assert!(next_to(node(15, 12)).contains(&node(14, 11)));
    assert!(next_to(node(17, 12)).contains(&node(18, 11)));

    // Up the ramp, a path's every step keeps the rules, its ramp steps straight.
    let path = find_land_path(&tile_map, (10, 10), (24, 20)).expect("up the ramp");
    assert!(passes(&path, 14, 15) && passes(&path, 15, 15), "by the ramp: {path:?}");
    assert!(diagonal_steps(&path) > 0, "diagonally across the open ground: {path:?}");
    assert_every_step_joins(&tile_map, &path);
    for step in path.windows(2).filter(|step| step[0].is_diagonal_to(step[1])) {
        assert!(!tile_map.elevation().shape(step[1].cell).is_ramp(), "diagonally onto the ramp: {path:?}");
    }

    // Across a lone raised cell's corner: (9, 10) and (10, 11) are both at 0, but (10, 10) beside the
    // step stands a level up.
    let mut tile_map = grass_map();
    tile_map.set_elevation(Cell::new(10, 10), 1, Shape::Flat);
    let graph = tile_map.graph();
    assert!(!graph.steps_diagonally(node(9, 10), node(10, 11)));
    assert!(!graph.neighbors_with(node(9, 10), land, Connectivity::Eight).contains(&node(10, 11)));
    assert!(graph.neighbors_with(node(9, 10), land, Connectivity::Eight).contains(&node(8, 11)));
    let path = find_land_path(&tile_map, (9, 10), (10, 11)).expect("round its corner");
    assert_eq!(path.len(), 3, "by (9, 11): {path:?}");
    assert!(passes(&path, 9, 11), "{path:?}");
}

// Goal 2: a road-only search keeps its four neighbours on open land beside it, and its costs, so
// a walker on roads goes the way it went before (pathfind's unit tests hash every kind of road-only
// search against the paths recorded before eight-direction movement).
fn test_road_paths_keep_four_neighbours() {
    let mut tile_map = grass_map();
    let road = test_utils::find_terrain_def("dirt_road");
    for x in 4..=20 {
        tile_map.try_place_tile(Cell::new(x, 12), road).ok_or_panic("a road along row 12");
    }
    for y in 13..=20 {
        tile_map.try_place_tile(Cell::new(20, y), road).ok_or_panic("and up column 20");
    }
    let path = find_road_path(&tile_map, (4, 12), (20, 20)).expect("along the road");
    assert_eq!((path.len(), diagonal_steps(&path)), (25, 0), "{path:?}");
    let graph = tile_map.graph();
    assert_eq!(
        graph.neighbors_with(Node::new(Cell::new(20, 12)), NodeKind::Road, Connectivity::Eight).len(),
        2,
        "no diagonal off the road's corner without open land"
    );
    assert_eq!(Connectivity::for_kinds(NodeKind::Road | NodeKind::BuildingRoadLink), Connectivity::Four);
    assert_eq!(Connectivity::for_kinds(NodeKind::EmptyLand | NodeKind::Road), Connectivity::Eight);
    // Over the grass too, it cuts the corner.
    let path = find_land_path(&tile_map, (4, 12), (20, 20)).expect("across the grass");
    assert_eq!(path.len(), 17, "{path:?}");
}

// The same search finds the same path every time, from a fresh search or a reused one.
fn test_diagonal_paths_are_deterministic() {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE_IN_CELLS, test_utils::find_terrain_def("grass"));
    for (x, y) in [(12, 12), (13, 14), (15, 11), (16, 16), (14, 18)] {
        test_utils::spawn_building(&mut env, Cell::new(x, y), "small_well");
    }
    let first = find_land_path(&env.tile_map, (8, 9), (21, 22)).expect("through the wells");
    assert!(diagonal_steps(&first) > 0);
    let graph = env.tile_map.graph();
    let mut search = Search::with_graph(graph);
    for _ in 0..5 {
        let again = match search.find_path(
            graph,
            &AStarUniformCostHeuristic::new(),
            NodeKind::EmptyLand | NodeKind::Road,
            Node::new(Cell::new(8, 9)),
            Node::new(Cell::new(21, 22)),
        ) {
            SearchResult::PathFound(path) => path.clone(),
            SearchResult::PathNotFound => panic!("no path"),
        };
        assert_eq!(again, first);
        // Another search in between, to reuse the search's state.
        let _ = search.find_path(
            graph,
            &AStarUniformCostHeuristic::new(),
            NodeKind::EmptyLand,
            Node::new(Cell::new(21, 9)),
            Node::new(Cell::new(9, 21)),
        );
    }
}
