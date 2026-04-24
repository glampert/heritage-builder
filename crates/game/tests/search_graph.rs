use common::{coords::Cell, Size};
use game::{
    pathfind::Node,
    tile::{
        TileKind, TileMap, TileMapLayerKind,
        placement::{TileClearingErr, TilePlacementErr},
    },
};

mod test_utils;

// ----------------------------------------------
// Integration tests for pathfind::Graph
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Search Graph", &[
        test_utils::test_fn!(test_vacant_lot_lifecycle),
        test_utils::test_fn!(test_vacant_lot_obstructed_by_building),
        test_utils::test_fn!(test_non_vacant_terrain_restored_on_object_clear),
        test_utils::test_fn!(test_vacant_lot_counter_no_underflow),
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
    assert!(tile_map.graph().has_vacant_lot_nodes(), "VacantLot should be reclaimed once the obstructing building is cleared");

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

    assert!(tile_map.graph().node_kind(Node::new(lot_cell)).unwrap().is_vacant_lot(),
        "Terrain's VacantLot path_kind must be restored when the Object above is cleared");

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
