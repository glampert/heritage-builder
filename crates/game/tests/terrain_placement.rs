use common::{coords::Cell, Size};
use game::tile::{TileKind, TileMapLayerKind};

mod test_utils;
use test_utils::{TestEnvironment, find_terrain_def, find_water_def, place_tile_as_player};

// ----------------------------------------------
// Integration tests for terrain placement
// ----------------------------------------------

// Water tiles draw the shores of the land around them, so placing land on water must pick the
// right variation for every water tile it touches: one shore per land side, a channel between
// two shores on opposite sides, and a corner for each diagonal that no side's shore covers.
// Placement goes through the player's path, `TilePlacement::place`, as in the game.
//
// Directions: north is +x, south -x, east -y, west +y (see game::tile::water).

fn main() {
    test_utils::run_tests("Terrain Placement", &[
        test_utils::test_fn!(test_single_island_shores),
        test_utils::test_fn!(test_channel_between_islands_along_x),
        test_utils::test_fn!(test_channel_between_islands_along_y),
        test_utils::test_fn!(test_bay_and_pond),
        test_utils::test_fn!(test_side_with_far_corners),
        test_utils::test_fn!(test_inner_corner_with_opposite_corner),
        test_utils::test_fn!(test_channel_entering_a_lake),
        test_utils::test_fn!(test_every_neighbor_pattern_has_a_variation),
        test_utils::test_fn!(test_placement_order_does_not_matter),
        test_utils::test_fn!(test_water_placed_back_restores_shores),
        test_utils::test_fn!(test_map_edge_counts_as_water),
    ]);
}

const MAP_SIZE: Size = Size::new(12, 12);

fn water_map() -> TestEnvironment {
    TestEnvironment::with_terrain(MAP_SIZE, find_water_def())
}

fn place_grass(env: &mut TestEnvironment, cells: &[Cell]) {
    let grass = find_terrain_def("grass");
    for cell in cells {
        place_tile_as_player(env, *cell, grass);
    }
}

fn place_water(env: &mut TestEnvironment, cells: &[Cell]) {
    let water = find_water_def();
    for cell in cells {
        place_tile_as_player(env, *cell, water);
    }
}

// The name of the water variation drawn at `cell`.
fn shore_at(env: &TestEnvironment, cell: Cell) -> &'static str {
    let tile = env.tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
        .unwrap_or_else(|| panic!("No terrain tile at {cell}"));
    assert!(tile.path_kind().is_water(), "Expected water at {cell}, found '{}'", tile.name());
    tile.variation_name()
}

fn assert_shores(env: &TestEnvironment, expected: &[(Cell, &str)]) {
    for (cell, name) in expected {
        let actual = shore_at(env, *cell);
        assert_eq!(actual, *name, "Water at {cell} draws '{actual}', expected '{name}'");
    }
}

// The variation name of every water tile on the map, by cell. Land tiles are left out: grass
// takes a random variation, which depends on the order tiles are placed in. The map's playable
// area is the rectangle that fits inside the map diamond, so the cells outside it hold no tiles.
fn all_shores(env: &TestEnvironment) -> Vec<(Cell, &'static str)> {
    let mut names = Vec::new();
    env.tile_map.for_each_tile(TileKind::Terrain, |_, tile| {
        if tile.path_kind().is_water() {
            names.push((tile.base_cell(), tile.variation_name()));
        }
    });
    names.sort_by_key(|(cell, _)| (cell.y, cell.x));
    names
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

fn test_single_island_shores() {
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(5, 5)]);

    assert_shores(&env, &[
        // Each side draws the shore of the land next to it.
        (Cell::new(6, 5), "0010"), // North of the island: land to the south.
        (Cell::new(4, 5), "1000"), // South: land to the north.
        (Cell::new(5, 4), "0001"), // East: land to the west.
        (Cell::new(5, 6), "0100"), // West: land to the east.
        // Each diagonal draws the island's corner.
        (Cell::new(6, 4), "sw_corner"),
        (Cell::new(6, 6), "se_corner"),
        (Cell::new(4, 4), "nw_corner"),
        (Cell::new(4, 6), "ne_corner"),
        // Further out, open water.
        (Cell::new(7, 5), "0000"),
        (Cell::new(3, 5), "0000"),
        (Cell::new(5, 7), "0000"),
        (Cell::new(5, 3), "0000"),
        (Cell::new(7, 7), "0000"),
    ]);
}

// The reported bug: land every other cell along x left the water cell between two islands as
// open water, with no shore on either side, and the cells diagonal to both islands with only
// one corner.
fn test_channel_between_islands_along_x() {
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(4, 5), Cell::new(6, 5)]);

    assert_shores(&env, &[
        (Cell::new(5, 5), "1010"),         // Between the islands: a shore on both sides.
        (Cell::new(5, 6), "ne_se_corner"), // West of the channel: both islands' corners.
        (Cell::new(5, 4), "nw_sw_corner"), // East of the channel.
        // The far sides and corners of each island are unaffected.
        (Cell::new(3, 5), "1000"),
        (Cell::new(7, 5), "0010"),
        (Cell::new(4, 4), "0001"),
        (Cell::new(4, 6), "0100"),
        (Cell::new(6, 4), "0001"),
        (Cell::new(6, 6), "0100"),
        (Cell::new(3, 4), "nw_corner"),
        (Cell::new(3, 6), "ne_corner"),
        (Cell::new(7, 4), "sw_corner"),
        (Cell::new(7, 6), "se_corner"),
    ]);
}

fn test_channel_between_islands_along_y() {
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(5, 4), Cell::new(5, 6)]);

    assert_shores(&env, &[
        (Cell::new(5, 5), "0101"),         // Between the islands: land to the east and the west.
        (Cell::new(6, 5), "se_sw_corner"), // North of the channel.
        (Cell::new(4, 5), "ne_nw_corner"), // South of the channel.
        (Cell::new(5, 3), "0001"),
        (Cell::new(5, 7), "0100"),
    ]);
}

fn test_bay_and_pond() {
    let mut env = water_map();

    // Land to the north, east and south: a bay open to the west.
    place_grass(&mut env, &[Cell::new(6, 5), Cell::new(5, 4), Cell::new(4, 5)]);
    assert_shores(&env, &[
        (Cell::new(5, 5), "1110"),
        (Cell::new(5, 6), "ne_se_corner"), // West of the bay: the corners of the north and south land.
    ]);

    // Land on every side: a pond.
    place_grass(&mut env, &[Cell::new(5, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1111")]);

    // The other three bays.
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(5, 4), Cell::new(4, 5), Cell::new(5, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "0111")]);

    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(6, 5), Cell::new(4, 5), Cell::new(5, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1011")]);

    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(6, 5), Cell::new(5, 4), Cell::new(5, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1101")]);
}

fn test_side_with_far_corners() {
    let mut env = water_map();

    // Land to the north, plus its north-east diagonal: the north shore covers that diagonal.
    place_grass(&mut env, &[Cell::new(6, 5), Cell::new(6, 4)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1000")]);

    // Land at the south-west diagonal, which no shore covers.
    place_grass(&mut env, &[Cell::new(4, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1000_sw_corner")]);

    // And at the south-east one.
    place_grass(&mut env, &[Cell::new(4, 4)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1000_se_sw_corner")]);
}

fn test_inner_corner_with_opposite_corner() {
    let mut env = water_map();

    place_grass(&mut env, &[Cell::new(6, 5), Cell::new(5, 4)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1100")]);

    place_grass(&mut env, &[Cell::new(4, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1100_sw_corner")]);
}

// A one-cell channel entering a lake beside the lake's shore: the lake's corner cell lists the
// diagonal next to its land side too, and its shore bends from the channel's bank into the lake.
fn test_channel_entering_a_lake() {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE, find_terrain_def("grass"));

    // A two by two lake, a channel into its west cell from the west, and one out of its north
    // cell to the north.
    let lake = [Cell::new(5, 5), Cell::new(6, 5), Cell::new(5, 6), Cell::new(6, 6)];
    place_water(&mut env, &lake);
    place_water(&mut env, &[Cell::new(5, 7), Cell::new(5, 8)]);
    place_water(&mut env, &[Cell::new(7, 6), Cell::new(8, 6)]);

    assert_shores(&env, &[
        (Cell::new(5, 6), "0010_nw_sw_corner"), // West cell: land south, the channel between north-west and south-west land.
        (Cell::new(5, 7), "1010"),              // The channel.
        (Cell::new(6, 6), "0001_ne_nw_corner"), // North cell: land west, the channel between north-east and north-west land.
        (Cell::new(7, 6), "0101"),
        (Cell::new(5, 5), "0110"),              // The lake's other corners are plain inner corners.
        (Cell::new(6, 5), "1100"),
    ]);

    // Land at the far diagonal alone is another island, not a channel.
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(4, 5), Cell::new(6, 6)]);
    assert_shores(&env, &[(Cell::new(5, 5), "0010_nw_corner")]);
}

// Every one of the 256 patterns of land around a water cell maps to a variation the tile set
// has, named by the rule in game::tile::water: the four sides as digits (north, east, south,
// west), then the diagonals not covered by a side's shore, in the order ne, nw, se, sw, plus,
// with a single land side, the diagonal next to it where a channel enters beside the land.
fn test_every_neighbor_pattern_has_a_variation() {
    const CENTER: Cell = Cell::new(5, 5);
    // Neighbor offsets, in bit order: n, e, s, w, ne, nw, se, sw.
    const NEIGHBORS: [(i32, i32); 8] = [(1, 0), (0, -1), (-1, 0), (0, 1), (1, -1), (1, 1), (-1, -1), (-1, 1)];

    for pattern in 0..256u32 {
        let land = |bit: usize| (pattern & (1 << bit)) != 0;
        let (n, e, s, w) = (land(0), land(1), land(2), land(3));
        let (ne, nw, se, sw) = (land(4), land(5), land(6), land(7));
        let mut diagonals = [("ne", ne && !n && !e), ("nw", nw && !n && !w), ("se", se && !s && !e), ("sw", sw && !s && !w)];
        match (n, e, s, w) {
            (true, false, false, false) => {
                diagonals[0].1 |= se && ne && !sw;
                diagonals[1].1 |= sw && nw && !se;
            }
            (false, false, true, false) => {
                diagonals[2].1 |= ne && se && !nw;
                diagonals[3].1 |= nw && sw && !ne;
            }
            (false, true, false, false) => {
                diagonals[0].1 |= nw && ne && !sw;
                diagonals[2].1 |= sw && se && !nw;
            }
            (false, false, false, true) => {
                diagonals[1].1 |= ne && nw && !se;
                diagonals[3].1 |= se && sw && !ne;
            }
            _ => {}
        }
        let any_diagonal = diagonals.iter().any(|(_, shown)| *shown);

        let mut expected = String::new();
        if n || e || s || w || !any_diagonal {
            expected = format!("{}{}{}{}", n as u8, e as u8, s as u8, w as u8);
        }
        for (name, shown) in diagonals {
            if shown {
                if !expected.is_empty() {
                    expected.push('_');
                }
                expected.push_str(name);
            }
        }
        if any_diagonal {
            expected.push_str("_corner");
        }

        let mut env = water_map();
        let cells: Vec<Cell> = NEIGHBORS.iter().enumerate()
            .filter(|(bit, _)| land(*bit))
            .map(|(_, (dx, dy))| Cell::new(CENTER.x + dx, CENTER.y + dy))
            .collect();
        place_grass(&mut env, &cells);

        let actual = shore_at(&env, CENTER);
        assert_eq!(actual, expected, "Land pattern {pattern:08b} (bits n,e,s,w,ne,nw,se,sw) draws '{actual}'");
    }
}

fn test_placement_order_does_not_matter() {
    let line = [Cell::new(3, 5), Cell::new(5, 5), Cell::new(7, 5), Cell::new(5, 7)];

    let mut env = water_map();
    place_grass(&mut env, &line);
    let forwards = all_shores(&env);
    assert!(forwards.iter().any(|(_, name)| *name == "1010"), "Expected a channel between the islands");

    let mut env = water_map();
    let mut reversed = line;
    reversed.reverse();
    place_grass(&mut env, &reversed);
    assert_eq!(forwards, all_shores(&env), "Placing the same cells in reverse order gave different shores");

    let mut env = water_map();
    place_grass(&mut env, &[line[1], line[3], line[0], line[2]]);
    assert_eq!(forwards, all_shores(&env), "Placing the same cells in another order gave different shores");
}

fn test_water_placed_back_restores_shores() {
    let mut env = water_map();
    place_grass(&mut env, &[Cell::new(4, 5), Cell::new(6, 5)]);
    assert_shores(&env, &[(Cell::new(5, 5), "1010")]);

    // Water over the north island: a single island again.
    place_water(&mut env, &[Cell::new(6, 5)]);
    assert_shores(&env, &[
        (Cell::new(6, 5), "0000"),
        (Cell::new(5, 5), "0010"),
        (Cell::new(5, 6), "se_corner"),
        (Cell::new(5, 4), "sw_corner"),
        (Cell::new(7, 5), "0000"),
    ]);

    // And over the other: open water everywhere.
    place_water(&mut env, &[Cell::new(4, 5)]);
    for (cell, name) in all_shores(&env) {
        assert_eq!(name, "0000", "The water at {cell} still draws a shore");
    }
}

// Cells past the edge of the map count as water, so a shore stops at the edge instead of running
// along it, and land next to the edge doesn't make a channel out of the water between the two.
fn test_map_edge_counts_as_water() {
    let mut env = water_map();
    let within = |env: &TestEnvironment, cell: Cell| env.tile_map.is_cell_within_playable_area(cell);

    // The water cell on the map's north edge, out from the center, and the one on the east edge.
    let mut north_edge = Cell::new(MAP_SIZE.width / 2, MAP_SIZE.height / 2);
    while within(&env, Cell::new(north_edge.x + 1, north_edge.y)) {
        north_edge.x += 1;
    }
    let mut east_edge = Cell::new(MAP_SIZE.width / 2, MAP_SIZE.height / 2);
    while within(&env, Cell::new(east_edge.x, east_edge.y - 1)) {
        east_edge.y -= 1;
    }

    // Land on their other side.
    place_grass(&mut env, &[Cell::new(north_edge.x - 1, north_edge.y), Cell::new(east_edge.x, east_edge.y + 1)]);

    assert_shores(&env, &[
        (north_edge, "0010"), // Land to the south only, no shore towards the map edge.
        (east_edge, "0001"),  // Land to the west only.
    ]);
}
