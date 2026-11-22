use common::{Size, coords::Cell};
use game::{
    menu::{ElevationRefusal, HeightBrush, TilePlacement, end_height_stroke},
    tile::{
        TileKind,
        TileMapLayerKind,
        elevation::{self, Dir, Half, HeightTool, Shape},
        height_pieces::{self, Course, FaceEdge, RampSurface},
    },
    undo_redo,
};

mod test_utils;
use test_utils::{
    TestEnvironment,
    find_building_def,
    find_terrain_def,
    find_vegetation_def,
    find_water_def,
    place_tile_as_player,
    spawn_building,
};

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
        // Terrain height (docs/plans/terrain-height.md, Stage 1):
        test_utils::test_fn!(test_heights_start_flat),
        test_utils::test_fn!(test_heights_set_undo_and_redo),
        test_utils::test_fn!(test_heights_refused_under_buildings_and_on_water),
        test_utils::test_fn!(test_height_brush_stroke_is_one_undo_step),
        test_utils::test_fn!(test_heights_reach_the_minimap),
        test_utils::test_fn!(test_heights_reset_with_the_map),
        test_utils::test_fn!(test_height_pieces_exist),
        test_utils::test_fn!(test_placement_follows_heights),
        test_utils::test_fn!(test_road_segments_follow_heights),
        test_utils::test_fn!(test_picking_follows_heights),
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
    let tile = env
        .tile_map
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
        (Cell::new(5, 6), "0010_nw_sw_corner"), /* West cell: land south, the channel between north-west and south-west land. */
        (Cell::new(5, 7), "1010"),              // The channel.
        (Cell::new(6, 6), "0001_ne_nw_corner"), /* North cell: land west, the channel between north-east and north-west land. */
        (Cell::new(7, 6), "0101"),
        (Cell::new(5, 5), "0110"), // The lake's other corners are plain inner corners.
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
        let cells: Vec<Cell> = NEIGHBORS
            .iter()
            .enumerate()
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

// ----------------------------------------------
// Terrain height
// ----------------------------------------------

fn grass_map() -> TestEnvironment {
    TestEnvironment::with_terrain(MAP_SIZE, find_terrain_def("grass"))
}

fn rect(from: (i32, i32), to: (i32, i32)) -> Vec<Cell> {
    let mut cells = Vec::new();
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            cells.push(Cell::new(x, y));
        }
    }
    cells
}

fn set_heights(
    env: &mut TestEnvironment,
    cells: &[Cell],
    level: u8,
    shape: Shape,
    undo: bool,
) -> game::menu::ElevationPlacement {
    let byte = elevation::encode(level, shape);
    let bytes: Vec<(Cell, u8)> = cells.iter().map(|cell| (*cell, byte)).collect();
    TilePlacement::set_elevation(&env.new_sim_context(0.0), &bytes, undo)
}

fn level_at(env: &TestEnvironment, x: i32, y: i32) -> u8 {
    env.tile_map.elevation().level(Cell::new(x, y))
}

fn test_heights_start_flat() {
    let env = grass_map();
    let map = env.tile_map.elevation();
    assert!(map.is_flat());
    assert_eq!(map.size(), MAP_SIZE);
    assert_eq!(map.max_height(), 0);
    assert!(map.problems().is_empty());
}

fn test_heights_set_undo_and_redo() {
    undo_redo::clear();
    let mut env = grass_map();
    let plateau = rect((3, 3), (5, 5));
    let placed = set_heights(&mut env, &plateau, 2, Shape::Flat, true);
    assert_eq!(placed.changed.len(), 9);
    assert!(placed.refused.is_empty());
    assert_eq!(env.tile_map.elevation().raised_count(), 9);
    assert_eq!(env.tile_map.elevation().max_height(), 2);

    // A ramp up its south-west side, its foot at (1, 4) and its head at (2, 4), one undo step: it
    // climbs one level, but the plateau is two up, so its head is a problem.
    let ramp = [
        (Cell::new(1, 4), elevation::encode(0, Shape::Ramp(Dir::N, Half::Low))),
        (Cell::new(2, 4), elevation::encode(0, Shape::Ramp(Dir::N, Half::High))),
    ];
    TilePlacement::set_elevation(&env.new_sim_context(0.0), &ramp, true);
    assert_eq!(env.tile_map.elevation().problem_at(Cell::new(1, 4)), None);
    assert_eq!(
        env.tile_map.elevation().problem_at(Cell::new(2, 4)),
        Some(elevation::ElevationProblem::RampUphill { dir: Dir::N })
    );

    // Undo takes the ramp back, then the plateau; redo puts the plateau back.
    undo_redo::undo(&env.new_sim_context(0.0));
    assert_eq!(env.tile_map.elevation().shape(Cell::new(1, 4)), Shape::Flat);
    assert_eq!(env.tile_map.elevation().shape(Cell::new(2, 4)), Shape::Flat);
    assert_eq!(level_at(&env, 4, 4), 2);
    undo_redo::undo(&env.new_sim_context(0.0));
    assert!(env.tile_map.elevation().is_flat());
    undo_redo::redo(&env.new_sim_context(0.0));
    assert_eq!(level_at(&env, 4, 4), 2);
    assert_eq!(env.tile_map.elevation().raised_count(), 9);
}

fn test_heights_refused_under_buildings_and_on_water() {
    let mut env = grass_map();
    spawn_building(&mut env, Cell::new(6, 6), "small_well");
    place_tile_as_player(&mut env, Cell::new(9, 9), find_water_def());

    let placed = set_heights(&mut env, &[Cell::new(6, 6), Cell::new(9, 9), Cell::new(1, 1)], 1, Shape::Flat, false);
    assert_eq!(placed.changed, vec![Cell::new(1, 1)]);
    assert_eq!(placed.refused, vec![
        (Cell::new(6, 6), ElevationRefusal::Building),
        (Cell::new(9, 9), ElevationRefusal::Water)
    ]);

    // Water may be set flat at 0 (it is already: nothing changes).
    let placed = set_heights(&mut env, &[Cell::new(9, 9)], 0, Shape::Flat, false);
    assert!(placed.refused.is_empty());

    // Outside the map.
    let placed = set_heights(&mut env, &[Cell::new(40, 40)], 1, Shape::Flat, false);
    assert_eq!(placed.refused, vec![(Cell::new(40, 40), ElevationRefusal::OutsideMap)]);
}

fn test_height_brush_stroke_is_one_undo_step() {
    undo_redo::clear();
    let mut env = grass_map();
    let brush = HeightBrush { tool: HeightTool::Raise, size: 3 };

    // A stroke dragged over two places: one level up from where it started, everywhere it went.
    assert!(TilePlacement::paint_height(&env.new_sim_context(0.0), brush, Cell::new(4, 4)));
    assert!(TilePlacement::paint_height(&env.new_sim_context(0.0), brush, Cell::new(6, 4)));
    // Painting the same cells again changes nothing.
    assert!(!TilePlacement::paint_height(&env.new_sim_context(0.0), brush, Cell::new(6, 4)));
    end_height_stroke();
    assert_eq!(env.tile_map.elevation().raised_count(), 15);
    assert_eq!(env.tile_map.elevation().max_height(), 1);

    // One undo takes the whole stroke back.
    undo_redo::undo(&env.new_sim_context(0.0));
    assert!(env.tile_map.elevation().is_flat());
    assert!(!undo_redo::can_undo());

    // A second stroke starting on raised ground goes one level higher.
    undo_redo::redo(&env.new_sim_context(0.0));
    assert!(TilePlacement::paint_height(
        &env.new_sim_context(0.0),
        HeightBrush { tool: HeightTool::Raise, size: 1 },
        Cell::new(4, 4)
    ));
    end_height_stroke();
    assert_eq!(level_at(&env, 4, 4), 2);
}

fn test_heights_reach_the_minimap() {
    let mut env = grass_map();
    set_heights(&mut env, &[Cell::new(5, 5)], 3, Shape::Flat, false);
    set_heights(&mut env, &[Cell::new(6, 5)], 1, Shape::Ramp(Dir::E, Half::Low), false);
    let grid = env.tile_map.minimap().grid();
    assert_eq!(grid.height(Cell::new(5, 5)), 3.0);
    assert_eq!(grid.height(Cell::new(6, 5)), 1.25);
    assert_eq!(grid.height(Cell::new(1, 1)), 0.0);
    assert!(!grid.bake_grid().heights.is_empty());

    // Back to flat: the bake reads the noise alone again.
    set_heights(&mut env, &[Cell::new(5, 5), Cell::new(6, 5)], 0, Shape::Flat, false);
    assert!(env.tile_map.minimap().grid().bake_grid().heights.is_empty());
}

fn test_heights_reset_with_the_map() {
    let mut env = grass_map();
    set_heights(&mut env, &rect((2, 2), (4, 4)), 1, Shape::Flat, false);
    env.tile_map.reset(Some(find_terrain_def("grass")), Some(Size::new(20, 16)));
    let map = env.tile_map.elevation();
    assert!(map.is_flat());
    assert_eq!(map.size(), Size::new(20, 16));
}

// The renderer's pieces for raised terrain (tile::height_pieces): every one it can ask for is in the
// objects tile set, a 1 × 1 sprite, and a cell always draws the same course.
fn test_height_pieces_exist() {
    for (tile, variation) in height_pieces::all_piece_names() {
        assert!(height_pieces::has_piece(tile, &variation), "No height piece {tile}/{variation} in the tile sets");
    }

    let mut looks = std::collections::HashSet::new();
    for edge in FaceEdge::ALL {
        for course in [Course::Mid, Course::Top { dirt_lip: false }, Course::Top { dirt_lip: true }] {
            for x in 0..6 {
                let cell = Cell::new(x, 3);
                let (def, variation) = height_pieces::cliff_course(edge, course, cell, 1).expect("a cliff course");
                assert_eq!(def.logical_size, common::Size::new(64, 32), "{} isn't a 1 × 1 piece", def.name);
                let again = height_pieces::cliff_course(edge, course, cell, 1)
                    .map(|(def, variation)| (def.name.as_str(), variation));
                assert_eq!(again, Some((def.name.as_str(), variation)));
                looks.insert((def.name.as_str(), variation));
            }
        }
    }
    // Two looks each for the grass-lipped top and the mid course, one dirt-lipped top, both edges.
    assert_eq!(looks.len(), 10, "{looks:?}");

    for side in Dir::ALL {
        assert!(height_pieces::rim(side).is_some());
    }
    for surface in RampSurface::ALL {
        for dir in Dir::ALL {
            for half in [Half::Low, Half::High] {
                assert!(height_pieces::ramp(surface, dir, half).is_some(), "{surface:?} {dir:?} {half:?}");
            }
        }
    }

    assert_eq!(RampSurface::of_terrain(find_terrain_def("dirt_road")), RampSurface::DirtRoad);
    assert_eq!(RampSurface::of_terrain(find_terrain_def("grass")), RampSurface::Grass);
    assert!(height_pieces::has_dirt_lip(find_terrain_def("dirt")));
    assert!(!height_pieces::has_dirt_lip(find_terrain_def("grass")));
}

// A grass map with a plateau at level 1 over x 4..=7, y 4..=7, a ramp up its south side climbing
// north (its foot at (2, 5), its head at (3, 5)) and a block at level 3 at (10, 10).
fn terraced_map() -> TestEnvironment {
    let mut env = TestEnvironment::with_terrain(MAP_SIZE, find_terrain_def("grass"));
    set_heights(&mut env, &rect((4, 4), (7, 7)), 1, Shape::Flat, false);
    set_heights(&mut env, &[Cell::new(2, 5)], 0, Shape::Ramp(Dir::N, Half::Low), false);
    set_heights(&mut env, &[Cell::new(3, 5)], 0, Shape::Ramp(Dir::N, Half::High), false);
    set_heights(&mut env, &[Cell::new(10, 10)], 3, Shape::Flat, false);
    assert!(env.tile_map.elevation().problems().is_empty());
    env
}

fn placement_error(env: &mut TestEnvironment, cell: Cell, def: &'static game::tile::sets::TileDef) -> Option<String> {
    env.tile_map.try_place_tile(cell, def).err().map(|err| format!("{}", err.reason))
}

// docs/plans/terrain-height.md, §7: footprints flat at one level, nothing beside ground more than a
// level above, water at level 0, lots and dykes on flat ground.
fn test_placement_follows_heights() {
    let mut env = terraced_map();
    let well = find_building_def("small_well");
    let theater = find_building_def("theater"); // 2 × 2.

    // On the plateau, or on the ground: fine.
    assert_eq!(placement_error(&mut env, Cell::new(5, 5), well), None);
    assert_eq!(placement_error(&mut env, Cell::new(1, 1), well), None);
    // Across the plateau's edge, or on a ramp: uneven ground.
    assert_eq!(placement_error(&mut env, Cell::new(3, 3), theater).as_deref(), Some("UnevenGround"));
    assert_eq!(placement_error(&mut env, Cell::new(2, 5), well).as_deref(), Some("UnevenGround"));
    // Beside the level-3 block at its side (its right corner's neighbour on its row is (10, 10)).
    assert_eq!(placement_error(&mut env, Cell::new(9, 11), well).as_deref(), Some("BesideHigherGround"));
    // A tree on the plateau is fine; on a ramp it isn't.
    assert_eq!(placement_error(&mut env, Cell::new(6, 6), find_vegetation_def("tree")), None);
    assert_eq!(placement_error(&mut env, Cell::new(3, 5), find_vegetation_def("tree")).as_deref(), Some("UnevenGround"));

    // Water on raised ground, a lot or a dyke on a ramp: refused; on the ground: fine.
    assert_eq!(placement_error(&mut env, Cell::new(4, 7), find_water_def()).as_deref(), Some("RaisedGround"));
    assert_eq!(placement_error(&mut env, Cell::new(2, 5), find_terrain_def("vacant_lot")).as_deref(), Some("UnevenGround"));
    assert_eq!(placement_error(&mut env, Cell::new(3, 5), find_terrain_def("dyke")).as_deref(), Some("UnevenGround"));
    assert_eq!(placement_error(&mut env, Cell::new(11, 1), find_water_def()), None);
    // A road may lie on a ramp.
    assert_eq!(placement_error(&mut env, Cell::new(2, 5), find_terrain_def("dirt_road")), None);

    // A sluice gate needs its dyke on its own level: a dyke at the plateau's foot, (3, 4), serves a
    // gate beside it on the ground, not one on the plateau above it.
    place_tile_as_player(&mut env, Cell::new(3, 4), find_terrain_def("dyke"));
    let gate = find_building_def("sluice_gate");
    assert_eq!(placement_error(&mut env, Cell::new(4, 4), gate).as_deref(), Some("RequiresDyke"));
    assert_eq!(placement_error(&mut env, Cell::new(3, 3), gate), None);
}

// A segment's steps must join by height: up a ramp along its way, never down a cliff. Junctions
// join only where the ground meets.
fn test_road_segments_follow_heights() {
    use game::tile::road::{self, RoadKind};
    let mut env = terraced_map();

    // Along y = 5 from the ground, up the ramp and onto the plateau.
    let up = road::build_segment(&env.tile_map, Cell::new(0, 5), Cell::new(6, 5), RoadKind::Dirt);
    assert!(up.is_valid, "{:?}", up.path);
    // Straight off the plateau's east edge (x = 5 from y = 6 down to 1): every way crosses the cliff.
    let down = road::build_segment(&env.tile_map, Cell::new(5, 6), Cell::new(5, 1), RoadKind::Dirt);
    assert!(!down.is_valid, "{:?}", down.path);
    // Onto the ramp from its side: the ramp's flank.
    let side = road::build_segment(&env.tile_map, Cell::new(3, 2), Cell::new(3, 5), RoadKind::Dirt);
    assert!(!side.is_valid, "{:?}", side.path);
    // A canal stays off raised ground.
    let canal = road::build_segment(&env.tile_map, Cell::new(4, 9), Cell::new(4, 6), RoadKind::Canal);
    assert!(!canal.is_valid);

    // Junction masks' bits (tile::road): west 1, south 2, east 4, north 8.
    const WEST: usize = 1;
    const SOUTH: usize = 2;
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &up, false).expect("the road up the ramp");
    // Up the ramp every step joins: the plateau's first cell joins the ramp's head south of it.
    assert_ne!(road::junction_mask(&env.tile_map, Cell::new(4, 5)) & SOUTH, 0);
    assert_ne!(road::junction_mask(&env.tile_map, Cell::new(3, 5)) & SOUTH, 0);
    // A road at the cliff's foot east of the plateau, (5, 3), beside a road on the plateau's edge
    // above it, (5, 4): they don't join across the cliff, but (5, 3) joins its neighbour on the ground.
    place_tile_as_player(&mut env, Cell::new(5, 4), find_terrain_def("dirt_road"));
    place_tile_as_player(&mut env, Cell::new(5, 3), find_terrain_def("dirt_road"));
    place_tile_as_player(&mut env, Cell::new(6, 3), find_terrain_def("dirt_road"));
    let foot = road::junction_mask(&env.tile_map, Cell::new(5, 3));
    assert_eq!(foot & WEST, 0, "the road at the cliff's foot joins the road above it: {foot:04b}");
    assert_ne!(foot, 0, "the road at the cliff's foot joins nothing: {foot:04b}");
    let edge = road::junction_mask(&env.tile_map, Cell::new(5, 4));
    assert_eq!(edge & 4, 0, "the road on the plateau's edge joins the road below it: {edge:04b}");
}

// The cell under a point is the one drawn there: a raised top, a cliff face's top cell, or the
// ground beside it (docs/plans/terrain-height.md, §4).
fn test_picking_follows_heights() {
    use common::{Vec2, coords::WorldToScreenTransform};
    let mut env = terraced_map();
    place_tile_as_player(&mut env, Cell::new(5, 5), find_building_def("small_well"));
    let transform = WorldToScreenTransform::new(2.0, Vec2::new(800.0, 900.0));
    let at = |x: i32, y: i32, height: f32| elevation::ground_point(x as f32, y as f32, height, transform);

    let pick =
        |env: &TestEnvironment, point| env.tile_map.find_exact_cell_for_point(TileMapLayerKind::Terrain, point, transform);
    // The plateau's front corner, drawn a level up.
    assert_eq!(pick(&env, at(4, 4, 1.0)), Cell::new(4, 4));
    // The same cell's flat position is its cliff face: still the corner.
    assert_eq!(pick(&env, at(4, 4, 0.0)), Cell::new(4, 4));
    // The ground beside the plateau.
    assert_eq!(pick(&env, at(9, 1, 0.0)), Cell::new(9, 1));
    // The ramp's head, half and three quarters of a level up.
    assert_eq!(pick(&env, at(3, 5, 0.75)), Cell::new(3, 5));
    // The topmost tile at the well's raised cell is the well.
    let tile = env.tile_map.topmost_tile_at_cursor(at(5, 5, 1.0), transform).expect("a tile");
    assert_eq!(tile.name(), "small_well");
}
