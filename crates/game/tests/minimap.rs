use common::{Size, coords::Cell};
use game::{
    building::construction::ConstructionMode,
    cheats,
    menu::{PlaceOrClearResult, TilePlacement},
    realm::Realm,
    tile::{
        TileMap,
        TileMapLayerKind,
        minimap::{
            Minimap,
            MinimapLens,
            MinimapTool,
            model::{FeatureState, LandmarkKind, MinimapClass, MinimapRoad, MinimapWall, tile_class},
            shapes::ShapeCache,
            style::MinimapStyle,
        },
        road::{self, RoadKind},
        sets::{OBJECTS_BUILDINGS_CATEGORY, TileSets},
    },
};
use minimap_bake::{Cover, Terrain};
use strum::IntoEnumIterator;

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the minimap
// (docs/plans/minimap.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Minimap", &[
        // Model:
        test_utils::test_fn!(test_grid_follows_terrain),
        test_utils::test_fn!(test_cover_follows_vegetation),
        test_utils::test_fn!(test_features_follow_buildings),
        test_utils::test_fn!(test_construction_sites_show_as_sites),
        test_utils::test_fn!(test_rebuild_matches_incremental_updates),
        test_utils::test_fn!(test_walls_draw_as_lines),
        // Bake:
        test_utils::test_fn!(test_patched_bake_matches_full_bake),
        // Data:
        test_utils::test_fn!(test_every_building_resolves_to_a_class),
        test_utils::test_fn!(test_styles_load_with_their_sprites),
        test_utils::test_fn!(test_realm_styles_inherit_dai_long),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn grass_map(size: i32) -> TestEnvironment {
    TestEnvironment::with_terrain(Size::new(size, size), test_utils::find_terrain_def("grass"))
}

// Through the tile map, which takes any of the building's cells.
fn clear_building(env: &mut TestEnvironment, cell: Cell) {
    assert!(env.tile_map.try_clear_tile_from_layer(cell, TileMapLayerKind::Objects).is_ok());
}

// Draws a wall (or a road) from `from` to `to` as a player's drag does.
fn draw(env: &mut TestEnvironment, from: Cell, to: Cell, kind: RoadKind) {
    let segment = road::build_segment(&env.tile_map, from, to, kind);
    assert!(segment.is_valid, "{kind:?} from {from} to {to} should be valid");
    TilePlacement::place_road_segment(&env.new_sim_context(0.0), &segment, true).expect("the segment");
}

fn place_gatehouse(env: &mut TestEnvironment, cell: Cell) -> PlaceOrClearResult {
    let gatehouse = test_utils::find_building_def("gatehouse");
    TilePlacement::place(&env.new_sim_context(0.0), cell, gatehouse, false, false, ConstructionMode::Finished)
}

type CellSnapshot = (Terrain, Cover, MinimapRoad, MinimapWall);
type FeatureSnapshot = (Cell, MinimapClass, Option<LandmarkKind>, FeatureState);

// The grid and features of two minimaps, as comparable lists.
fn snapshot(minimap: &Minimap) -> (Vec<CellSnapshot>, Vec<FeatureSnapshot>) {
    let size = minimap.size_in_cells();
    let mut cells = Vec::new();
    for y in 0..size.height {
        for x in 0..size.width {
            let cell = Cell::new(x, y);
            let grid = minimap.grid();
            cells.push((grid.terrain(cell), grid.cover(cell), grid.road(cell), grid.wall(cell)));
        }
    }
    let features = minimap.features().iter().map(|f| (f.base_cell, f.class, f.landmark, f.state)).collect();
    (cells, features)
}

// ----------------------------------------------
// Model
// ----------------------------------------------

fn test_grid_follows_terrain() {
    let mut env = grass_map(16);
    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.grid().terrain(Cell::new(8, 8)), Terrain::Land);
    assert_eq!(minimap.grid().terrain(Cell::new(1, 1)), Terrain::None, "outside the playable area: no tiles");

    test_utils::place_terrain_by_def(&mut env, Cell::new(8, 8), test_utils::find_water_def());
    test_utils::place_road(&mut env, &[Cell::new(5, 5), Cell::new(6, 5)]);
    let grid = env.tile_map.minimap().grid();
    assert_eq!(grid.terrain(Cell::new(8, 8)), Terrain::Water);
    assert_eq!(grid.terrain(Cell::new(5, 5)), Terrain::Land);
    assert_eq!(grid.road(Cell::new(5, 5)), MinimapRoad::Dirt);
    assert_eq!(grid.road(Cell::new(4, 5)), MinimapRoad::None);

    test_utils::clear_terrain(&mut env, &[Cell::new(6, 5)]);
    let grid = env.tile_map.minimap().grid();
    assert_eq!(grid.terrain(Cell::new(6, 5)), Terrain::None);
    assert_eq!(grid.road(Cell::new(6, 5)), MinimapRoad::None);
}

fn test_cover_follows_vegetation() {
    let mut env = grass_map(16);
    let tree = test_utils::find_vegetation_def("tree");
    test_utils::place_tile_as_player(&mut env, Cell::new(7, 7), tree);
    assert_eq!(env.tile_map.minimap().grid().cover(Cell::new(7, 7)), Cover::Vegetation);

    assert!(env.tile_map.try_clear_tile_from_layer(Cell::new(7, 7), TileMapLayerKind::Objects).is_ok());
    assert_eq!(env.tile_map.minimap().grid().cover(Cell::new(7, 7)), Cover::None);
}

fn test_features_follow_buildings() {
    let mut env = grass_map(32);
    test_utils::place_tile_as_player(&mut env, Cell::new(4, 4), test_utils::find_building_def("livestock_farm"));
    test_utils::place_tile_as_player(&mut env, Cell::new(10, 4), test_utils::find_building_def("temple"));
    test_utils::place_tile_as_player(&mut env, Cell::new(16, 4), test_utils::find_building_def("governor_palace"));

    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.features().len(), 3);
    let farm = minimap.features().get(Cell::new(4, 4)).expect("the farm");
    assert_eq!(farm.class, MinimapClass::Field);
    assert_eq!(farm.size, Size::new(3, 3));
    assert_eq!(minimap.features().get(Cell::new(10, 4)).unwrap().landmark, Some(LandmarkKind::Temple));
    assert_eq!(minimap.seat_cell(), Some(Cell::new(18, 6)), "the palace's centre, rounded");
    for x in 4..7 {
        for y in 4..7 {
            assert_eq!(minimap.grid().cover(Cell::new(x, y)), Cover::Building);
        }
    }
    assert!(minimap.features().at(Cell::new(6, 6)).is_some_and(|f| f.base_cell == Cell::new(4, 4)));

    // Cleared from a cell that isn't its base: the feature still goes.
    clear_building(&mut env, Cell::new(5, 6));
    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.features().len(), 2);
    assert!(minimap.features().get(Cell::new(4, 4)).is_none());
    assert_eq!(minimap.grid().cover(Cell::new(5, 5)), Cover::None);
}

fn test_construction_sites_show_as_sites() {
    cheats::get_mut().instant_construction = false;
    let mut env = grass_map(16);
    test_utils::place_tile_as_player(&mut env, Cell::new(4, 4), test_utils::find_building_def("temple"));
    let state = env.tile_map.minimap().features().get(Cell::new(4, 4)).map(|f| f.state);
    cheats::get_mut().instant_construction = true;
    assert_eq!(state, Some(FeatureState::UnderConstruction));
}

fn test_rebuild_matches_incremental_updates() {
    let mut env = grass_map(32);
    test_utils::place_road(&mut env, &[Cell::new(3, 10), Cell::new(4, 10), Cell::new(5, 10)]);
    draw(&mut env, Cell::new(14, 16), Cell::new(18, 16), RoadKind::Rampart);
    assert!(matches!(place_gatehouse(&mut env, Cell::new(16, 16)), PlaceOrClearResult::PlacedTile(_)));
    test_utils::place_terrain_by_def(&mut env, Cell::new(20, 20), test_utils::find_water_def());
    test_utils::place_tile_as_player(&mut env, Cell::new(8, 8), test_utils::find_building_def("market"));
    test_utils::place_tile_as_player(&mut env, Cell::new(12, 12), test_utils::find_vegetation_def("bamboo"));

    let mut rebuilt = Minimap::new(env.tile_map.size_in_cells());
    rebuilt.rebuild_from(&env.tile_map);
    assert!(
        snapshot(&rebuilt) == snapshot(env.tile_map.minimap()),
        "a rebuilt minimap differs from the one kept up to date"
    );
}

// Wall pieces are lines on the grid's wall layer, as roads are, not features: a run per tier, its
// ends meeting the next tier's and reaching into a gatehouse, which is a feature and a gate cell.
fn test_walls_draw_as_lines() {
    let mut env = grass_map(64);
    draw(&mut env, Cell::new(20, 30), Cell::new(25, 30), RoadKind::Palisade);
    draw(&mut env, Cell::new(26, 30), Cell::new(30, 30), RoadKind::Rampart);
    draw(&mut env, Cell::new(36, 36), Cell::new(36, 36), RoadKind::CityWall);

    let walls = |env: &TestEnvironment| {
        let minimap = env.tile_map.minimap();
        let style = MinimapStyle::load(Realm::DaiLong).unwrap();
        let mut shapes = ShapeCache::default();
        shapes.refresh(minimap.features(), minimap.grid(), &style, 0);
        let mut runs: Vec<_> = shapes.walls.iter().map(|run| (run.kind, run.from, run.to)).collect();
        runs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        runs
    };

    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.grid().wall(Cell::new(20, 30)), MinimapWall::Palisade);
    assert_eq!(minimap.grid().wall(Cell::new(26, 30)), MinimapWall::Rampart);
    assert_eq!(minimap.grid().wall(Cell::new(19, 30)), MinimapWall::None);
    assert_eq!(minimap.grid().cover(Cell::new(20, 30)), Cover::Building);
    assert!(minimap.features().is_empty(), "wall pieces aren't features");
    assert_eq!(minimap.describe_cell(Cell::new(26, 30)).as_deref(), Some("Rampart"));
    assert_eq!(walls(&env), [
        (MinimapWall::Palisade, (20.0, 30.0), (25.5, 30.0)),
        (MinimapWall::Rampart, (25.5, 30.0), (30.0, 30.0)),
        (MinimapWall::CityWall, (36.0, 36.0), (36.0, 36.0)), // A piece alone.
    ]);

    // A gatehouse on a piece: a gate cell and a feature, the runs either side reaching its centre.
    assert!(matches!(place_gatehouse(&mut env, Cell::new(22, 30)), PlaceOrClearResult::PlacedTile(_)));
    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.grid().wall(Cell::new(22, 30)), MinimapWall::Gate);
    assert_eq!(minimap.grid().road(Cell::new(22, 30)), MinimapRoad::Dirt, "the road under it");
    assert_eq!(minimap.features().len(), 1);
    assert_eq!(walls(&env), [
        (MinimapWall::Palisade, (20.0, 30.0), (22.0, 30.0)),
        (MinimapWall::Palisade, (22.0, 30.0), (25.5, 30.0)),
        (MinimapWall::Rampart, (25.5, 30.0), (30.0, 30.0)),
        (MinimapWall::CityWall, (36.0, 36.0), (36.0, 36.0)),
    ]);

    // Cleared, the gap's road stays and the wall layer forgets the gate.
    clear_building(&mut env, Cell::new(22, 30));
    let minimap = env.tile_map.minimap();
    assert_eq!(minimap.grid().wall(Cell::new(22, 30)), MinimapWall::None);
    assert!(minimap.features().is_empty());
    assert_eq!(walls(&env)[0], (MinimapWall::Palisade, (20.0, 30.0), (21.0, 30.0)));
}

// ----------------------------------------------
// Bake
// ----------------------------------------------

// Edits baked a patch at a time, within the per-frame budget, give the same pixels as baking
// everything again.
fn test_patched_bake_matches_full_bake() {
    let mut env = grass_map(48);
    env.tile_map.minimap_mut().bake(None);

    for (i, x) in (6..40).step_by(5).enumerate() {
        let cell = Cell::new(x, 10 + (i as i32 * 3) % 20);
        if i % 2 == 0 {
            test_utils::place_terrain_by_def(&mut env, cell, test_utils::find_water_def());
        } else {
            test_utils::place_tile_as_player(&mut env, cell, test_utils::find_vegetation_def("tree"));
        }
        // A small budget, so edits take several bakes.
        for _ in 0..64 {
            env.tile_map.minimap_mut().bake(Some(200));
        }
    }
    let (patched, size) = env.tile_map.minimap_mut().baked_pixels();
    let patched = patched.to_vec();

    let mut fresh = Minimap::new(env.tile_map.size_in_cells());
    fresh.rebuild_from(&env.tile_map);
    let (full, full_size) = fresh.baked_pixels();
    assert_eq!(size, full_size);
    assert!(patched == full, "patched pixels differ from a full bake");
}

// ----------------------------------------------
// Data
// ----------------------------------------------

fn test_every_building_resolves_to_a_class() {
    let buildings =
        TileSets::get().find_category_by_hash(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.hash).unwrap();
    let mut checked = 0;
    let mut landmarks = Vec::new();
    buildings.for_each_tile_def(|tile_def| {
        assert_ne!(tile_class(tile_def), MinimapClass::None, "'{}' has no minimap class", tile_def.name);
        if let Some(landmark) = game::tile::minimap::model::tile_landmark(tile_def) {
            landmarks.push(landmark);
        }
        checked += 1;
        true
    });
    assert!(checked > 20, "expected the building tile defs, found {checked}");
    assert!(landmarks.contains(&LandmarkKind::Seat), "no building is the seat, for the Home tool");
}

// Every realm's style loads, and the sprites it names exist: tooltip frame, landmark icons, field
// stripes and every lens and tool button in all four states.
fn test_styles_load_with_their_sprites() {
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ui");
    let exists = |path: &str| assert!(ui.join(path).is_file(), "missing sprite assets/ui/{path}");

    for realm in Realm::iter() {
        let style = MinimapStyle::load(realm).unwrap_or_else(|err| panic!("{err}"));
        if let Some(tooltip_frame) = style.tooltip_frame() {
            exists(tooltip_frame);
        }
        exists(&style.field_texture);
        exists(&style.alert_icon);
        for landmark in LandmarkKind::iter() {
            let path = style.landmark_icons.get(&landmark).unwrap_or_else(|| panic!("{realm} has no icon for {landmark}"));
            exists(path);
        }
        let sprites = MinimapLens::iter()
            .map(|lens| lens.sprite())
            .chain(MinimapTool::iter().map(|tool| tool.sprite()))
            .chain(["open", "debug", "landmarks_on", "landmarks_off"]); // The open button and the top row's toggles.
        for sprite in sprites {
            for state in ["idle", "hovered", "pressed", "disabled"] {
                exists(&format!("buttons/{}/{sprite}_{state}.png", style.buttons));
            }
        }
    }

    let _ = TileMap::new(Size::new(4, 4), None); // An empty map makes an empty minimap.
}

// Kambara's and Tampadesa's files hold only what they change: the rest comes from Đại Long's.
fn test_realm_styles_inherit_dai_long() {
    let base = MinimapStyle::load(Realm::DaiLong).unwrap();
    for realm in [Realm::Kambara, Realm::Tampadesa] {
        let style = MinimapStyle::load(realm).unwrap();
        assert_eq!(style.map_size, base.map_size, "{realm} map_size");
        assert_eq!(style.lenses.water_access, base.lenses.water_access, "{realm} lenses.water_access");
        assert_eq!(style.palette.road, base.palette.road, "{realm} palette.road (a field its palette doesn't set)");
        assert_ne!(style.palette.land_low, base.palette.land_low, "{realm} sets its own land colour");
        assert_ne!(style.buttons, base.buttons, "{realm} has its own buttons");
    }
}
