use common::{Size, coords::Cell};
use game::{
    cheats,
    tile::{
        TileMap,
        TileMapLayerKind,
        minimap::{
            Minimap,
            MinimapLens,
            MinimapTool,
            model::{FeatureState, LandmarkKind, MinimapClass, MinimapRoad, tile_class},
            style::{MinimapRealm, MinimapStyle},
        },
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
        // Bake:
        test_utils::test_fn!(test_patched_bake_matches_full_bake),
        // Data:
        test_utils::test_fn!(test_every_building_resolves_to_a_class),
        test_utils::test_fn!(test_styles_load_with_their_sprites),
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

// The grid and features of two minimaps, as comparable lists.
fn snapshot(minimap: &Minimap) -> (Vec<(Terrain, Cover, MinimapRoad)>, Vec<(Cell, MinimapClass, Option<LandmarkKind>, FeatureState)>) {
    let size = minimap.size_in_cells();
    let mut cells = Vec::new();
    for y in 0..size.height {
        for x in 0..size.width {
            let cell = Cell::new(x, y);
            cells.push((minimap.grid().terrain(cell), minimap.grid().cover(cell), minimap.grid().road(cell)));
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
    test_utils::place_tile_as_player(&mut env, Cell::new(4, 4), test_utils::find_building_def("rice_farm"));
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
    test_utils::place_terrain_by_def(&mut env, Cell::new(20, 20), test_utils::find_water_def());
    test_utils::place_tile_as_player(&mut env, Cell::new(8, 8), test_utils::find_building_def("market"));
    test_utils::place_tile_as_player(&mut env, Cell::new(12, 12), test_utils::find_vegetation_def("bamboo"));

    let mut rebuilt = Minimap::new(env.tile_map.size_in_cells());
    rebuilt.rebuild_from(&env.tile_map);
    assert!(snapshot(&rebuilt) == snapshot(env.tile_map.minimap()), "a rebuilt minimap differs from the one kept up to date");
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
    let buildings = TileSets::get().find_category_by_hash(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.hash).unwrap();
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

// Every realm's style loads, and the sprites it names exist: frame, landmark icons, field
// stripes and every lens and tool button in all four states.
fn test_styles_load_with_their_sprites() {
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ui");
    let exists = |path: &str| assert!(ui.join(path).is_file(), "missing sprite assets/ui/{path}");

    for realm in MinimapRealm::iter() {
        let style = MinimapStyle::load(realm).unwrap_or_else(|err| panic!("{err}"));
        exists(&style.frame);
        exists(&style.field_texture);
        exists(&style.alert_icon);
        for landmark in LandmarkKind::iter() {
            let path = style.landmark_icons.get(&landmark).unwrap_or_else(|| panic!("{realm} has no icon for {landmark}"));
            exists(path);
        }
        let sprites = MinimapLens::iter().map(|lens| lens.sprite()).chain(MinimapTool::iter().map(|tool| tool.sprite()));
        for sprite in sprites {
            for state in ["idle", "hovered", "pressed", "disabled"] {
                exists(&format!("buttons/{}/{sprite}_{state}.png", style.buttons));
            }
        }
    }

    let _ = TileMap::new(Size::new(4, 4), None); // An empty map makes an empty minimap.
}
