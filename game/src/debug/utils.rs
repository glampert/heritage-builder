use crate::{
    log,
    imgui_ui::UiSystem,
    game::{
        world::World,
        cheats::{self, Cheats},
        sim::debug::DebugQueryBuilder
    },
    render::{RenderSystem, RenderStats},
    utils::{
        Color,
        Size,
        Vec2,
        Rect,
        coords::{self, Cell, WorldToScreenTransform}
    },
    tile::{
        Tile,
        TileKind,
        TileFlags,
        TileMap,
        TileMapLayerKind,
        BASE_TILE_SIZE,
        sets::{TileSets, TileDef},
        rendering::{TileMapRenderFlags, TileMapRenderStats}
    }
};

// ----------------------------------------------
// Debug Draw Helpers
// ----------------------------------------------

pub fn draw_tile_debug(render_sys: &mut impl RenderSystem,
                       ui_sys: &UiSystem,
                       tile_screen_rect: Rect,
                       transform: &WorldToScreenTransform,
                       tile: &Tile,
                       flags: TileMapRenderFlags) {

    let draw_debug_info = {
        tile.has_flags(TileFlags::DrawDebugInfo | TileFlags::DrawBlockerInfo) ||
        (tile.is(TileKind::Terrain)    && flags.contains(TileMapRenderFlags::DrawTerrainTileDebug))   ||
        (tile.is(TileKind::Blocker)    && flags.contains(TileMapRenderFlags::DrawBlockersTileDebug))  ||
        (tile.is(TileKind::Building)   && flags.contains(TileMapRenderFlags::DrawBuildingsTileDebug)) ||
        (tile.is(TileKind::Prop)       && flags.contains(TileMapRenderFlags::DrawPropsTileDebug))     ||
        (tile.is(TileKind::Unit)       && flags.contains(TileMapRenderFlags::DrawUnitsTileDebug))     ||
        (tile.is(TileKind::Vegetation) && flags.contains(TileMapRenderFlags::DrawVegetationTileDebug))
    };

    let draw_debug_bounds =
        tile.has_flags(TileFlags::DrawDebugBounds) ||
        flags.contains(TileMapRenderFlags::DrawDebugBounds);

    if draw_debug_info {
        draw_tile_info(render_sys, ui_sys, tile_screen_rect, tile);
    }

    if draw_debug_bounds {
        if tile.has_flags(TileFlags::BuildingRoadLink) {
            draw_road_link_bounds(render_sys, tile_screen_rect, transform, tile);
        } else {
            draw_tile_bounds(render_sys, tile_screen_rect, transform, tile, true, true);
        }
    }
}

// Show a small debug overlay under the cursor with its current position or provided text.
pub fn draw_cursor_overlay(ui_sys: &UiSystem, transform: &WorldToScreenTransform, opt_text: Option<&str>) {
    let ui = ui_sys.builder();
    let cursor_screen_pos = Vec2::new(ui.io().mouse_pos[0], ui.io().mouse_pos[1]);

    // Make the window background transparent and remove decorations.
    let window_flags =
        imgui::WindowFlags::NO_DECORATION |
        imgui::WindowFlags::NO_MOVE |
        imgui::WindowFlags::NO_SAVED_SETTINGS |
        imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
        imgui::WindowFlags::NO_NAV |
        imgui::WindowFlags::NO_MOUSE_INPUTS;

    // Draw a tiny window near the cursor
    ui.window("Cursor Debug")
        .position([cursor_screen_pos.x + 10.0, cursor_screen_pos.y + 10.0], imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.6) // Semi-transparent
        .build(|| {
            if let Some(text) = opt_text {
                ui.text(text);
            } else {
                let cursor_iso_pos = coords::screen_to_iso_point(
                    cursor_screen_pos,
                    transform,
                    BASE_TILE_SIZE);

                let cursor_approx_cell = coords::iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);

                ui.text(format!("C:{},{}", cursor_approx_cell.x, cursor_approx_cell.y));
                ui.text(format!("S:{:.1},{:.1}", cursor_screen_pos.x, cursor_screen_pos.y));
                ui.text(format!("I:{},{}", cursor_iso_pos.x, cursor_iso_pos.y));
            }
        });
}

pub fn draw_screen_origin_marker(render_sys: &mut impl RenderSystem) {
    // 50x50px white square to mark the origin.
    render_sys.draw_point_fast(
        Vec2::zero(), 
        Color::white(),
        50.0);

    // Red line for the X axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Vec2::zero(),
        Vec2::new(100.0, 0.0),
        Color::red(),
        15.0);

    render_sys.draw_colored_rect(
        Rect::new(Vec2::new(100.0, 0.0), Vec2::new(10.0, 10.0)),
        Color::green());

    // Blue line for the Y axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Vec2::zero(),
        Vec2::new(0.0, 100.0),
        Color::blue(),
        15.0);

    render_sys.draw_colored_rect(
        Rect::new(Vec2::new(0.0, 100.0), Vec2::new(10.0, 10.0)),
        Color::green());
}

pub fn draw_render_stats(ui_sys: &UiSystem,
                         render_sys_stats: &RenderStats,
                         tile_render_stats: &TileMapRenderStats) {

    let ui = ui_sys.builder();

    let window_flags =
        imgui::WindowFlags::NO_DECORATION |
        imgui::WindowFlags::NO_MOVE |
        imgui::WindowFlags::NO_SAVED_SETTINGS |
        imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
        imgui::WindowFlags::NO_NAV |
        imgui::WindowFlags::NO_MOUSE_INPUTS;

    // Place the window at the bottom-left corner of the screen.
    let window_position = [
        5.0,
        ui.io().display_size[1] - 185.0,
    ];

    ui.window("Render Stats")
        .position(window_position, imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.6) // Semi-transparent
        .build(|| {
            ui.text_colored(Color::yellow().to_array(),
                format!("Tiles drawn: {} | Peak: {}",
                              tile_render_stats.tiles_drawn,
                              tile_render_stats.peak_tiles_drawn));

            ui.text_colored(Color::yellow().to_array(),
                format!("Triangles drawn: {} | Peak: {}",
                              render_sys_stats.triangles_drawn,
                              render_sys_stats.peak_triangles_drawn));

            ui.text_colored(Color::yellow().to_array(),
                format!("Texture changes: {} | Peak: {}",
                              render_sys_stats.texture_changes,
                              render_sys_stats.peak_texture_changes));

            ui.text_colored(Color::yellow().to_array(),
                format!("Draw calls: {} | Peak: {}",
                              render_sys_stats.draw_calls,
                              render_sys_stats.peak_draw_calls));

            ui.text(format!("Tile sort list: {} | Peak: {}",
                tile_render_stats.tile_sort_list_len,
                tile_render_stats.peak_tile_sort_list_len));

            ui.text(format!("Tiles highlighted: {} | Peak: {}",
                tile_render_stats.tiles_drawn_highlighted,
                tile_render_stats.peak_tiles_drawn_highlighted));

            ui.text(format!("Tiles invalidated: {} | Peak: {}",
                tile_render_stats.tiles_drawn_invalidated,
                tile_render_stats.peak_tiles_drawn_invalidated));

            ui.text(format!("Lines drawn: {} | Peak: {}",
                render_sys_stats.lines_drawn,
                render_sys_stats.peak_lines_drawn));

            ui.text(format!("Points drawn: {} | Peak: {}",
                render_sys_stats.points_drawn,
                render_sys_stats.peak_points_drawn));
        });
}

// ----------------------------------------------
// Internal Helpers
// ----------------------------------------------

fn draw_tile_overlay_text(ui_sys: &UiSystem,
                          debug_overlay_pos: Vec2,
                          tile_screen_pos: Vec2,
                          tile: &Tile) {

    // Make the window background transparent and remove decorations:
    let window_flags =
        imgui::WindowFlags::NO_DECORATION |
        imgui::WindowFlags::NO_MOVE |
        imgui::WindowFlags::NO_SAVED_SETTINGS |
        imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
        imgui::WindowFlags::NO_NAV |
        imgui::WindowFlags::NO_MOUSE_INPUTS;

    // NOTE: Label has to be unique for each tile because it will be used as the ImGui ID for this widget.
    let cell = tile.actual_base_cell();
    let label = format!("{}_{}_{}", tile.name(), cell.x, cell.y);
    let position = [debug_overlay_pos.x, debug_overlay_pos.y];

    let bg_color = {
        if tile.is(TileKind::Blocker) {
            Color::red().to_array()
        } else if tile.is(TileKind::Building) {
            Color::yellow().to_array()
        } else if tile.is(TileKind::Prop) {
            Color::magenta().to_array()
        } else if tile.is(TileKind::Unit) {
            Color::cyan().to_array()
        } else if tile.is(TileKind::Vegetation) {
            Color::green().to_array()
        } else {
            Color::black().to_array()
        }
    };

    let text_color = {
        if tile.is(TileKind::Terrain) {
            Color::white().to_array()
        } else {
            Color::black().to_array()
        }
    };

    let ui = ui_sys.builder();

    // Adjust window background color based on tile kind.
    // The returned tokens take care of popping back to the previous color/font.
    let _bg_col   = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
    let _text_col = ui.push_style_color(imgui::StyleColor::Text, text_color);
    let _font     = ui.push_font(ui_sys.fonts().small);

    ui.window(label)
        .position(position, imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.4) // Semi-transparent
        .build(|| {
            let tile_iso_pos = tile.iso_coords_f32();
            ui.text(format!("C:{},{}", cell.x, cell.y)); // Cell position
            ui.text(format!("S:{:.1},{:.1}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
            ui.text(format!("I:{:.1},{:.1}", tile_iso_pos.x, tile_iso_pos.y)); // 2D isometric position
            ui.text(format!("Z:{}", tile.z_sort_key())); // Z-sort
        });
}

fn draw_tile_info(render_sys: &mut impl RenderSystem,
                  ui_sys: &UiSystem,
                  tile_screen_rect: Rect,
                  tile: &Tile) {

    let tile_screen_pos = tile_screen_rect.position();
    let tile_center = tile_screen_rect.center();

    // Center the overlay text box roughly over the tile's center.
    let debug_overlay_pos = Vec2::new(
        tile_center.x - 40.0,
        tile_center.y - 40.0);

    draw_tile_overlay_text(
        ui_sys,
        debug_overlay_pos,
        tile_screen_pos,
        tile);

    // Put a dot at the tile's center.
    let center_pt_color = if tile.is(TileKind::Blocker) { Color::white() } else { Color::red() };
    render_sys.draw_point_fast(tile_center - Vec2::new(2.5, 2.5), center_pt_color, 10.0);
}

// Alternate debug info used for displaying building road link tiles.
fn draw_road_link_bounds(render_sys: &mut impl RenderSystem,
                         tile_screen_rect: Rect,
                         transform: &WorldToScreenTransform,
                         tile: &Tile) {

    draw_tile_bounds(render_sys, tile_screen_rect, transform, tile, true, false);

    let tile_center = tile_screen_rect.center();
    render_sys.draw_point_fast(tile_center - Vec2::new(2.5, 2.5), Color::blue(), 10.0);
}

fn draw_tile_bounds(render_sys: &mut impl RenderSystem,
                    tile_screen_rect: Rect,
                    transform: &WorldToScreenTransform,
                    tile: &Tile,
                    diamond_iso: bool,
                    sprite_aabb: bool) {

    let color = {
        if tile.is(TileKind::Blocker) {
            Color::red()
        } else if tile.is(TileKind::Building) {
            Color::yellow()
        } else if tile.is(TileKind::Prop) {
            Color::magenta()
        } else if tile.is(TileKind::Unit) {
            Color::cyan()
        } else if tile.is(TileKind::Vegetation) {
            Color::green()
        } else {
            Color::red()
        }
    };

    // Tile isometric "diamond" bounding box:
    if diamond_iso {
        let diamond_points = coords::cell_to_screen_diamond_points(
            tile.base_cell(),
            tile.logical_size(),
            BASE_TILE_SIZE,
            transform);

        render_sys.draw_line_fast(diamond_points[0], diamond_points[1], color, color);
        render_sys.draw_line_fast(diamond_points[1], diamond_points[2], color, color);
        render_sys.draw_line_fast(diamond_points[2], diamond_points[3], color, color);
        render_sys.draw_line_fast(diamond_points[3], diamond_points[0], color, color);

        for point in diamond_points {
            render_sys.draw_point_fast(point, Color::green(), 10.0);
        }
    }

    // Tile axis-aligned bounding rectangle of the actual sprite image:
    if sprite_aabb {
        render_sys.draw_wireframe_rect_fast(tile_screen_rect, color);
    }
}

// ----------------------------------------------
// Built-in test TileMaps
// ----------------------------------------------

mod test_maps {
    use super::*;

    struct PresetTiles {
        preset_name: &'static str,
        map_size_in_cells: Size,
        terrain_tiles:  &'static [i32],
        building_tiles: &'static [i32],
        enable_cheats_fn: Option<fn(&mut Cheats)>,
    }

    // TERRAIN:
    const G: i32 = 0; // grass
    const D: i32 = 1; // dirt
    const R: i32 = 2; // stone_path (road)
    const TERRAIN_TILE_NAMES: [&str; 3] = [
        "grass",
        "dirt",
        "stone_path",
    ];

    // BUILDINGS:
    const X: i32 = -1; // empty (dummy value)
    const H: i32 = 0;  // house0
    const W: i32 = 1;  // well_small
    const B: i32 = 2;  // well_big
    const M: i32 = 3;  // market
    const F: i32 = 4;  // rice_farm
    const S: i32 = 5;  // granary
    const Y: i32 = 6;  // storage_yard
    const A: i32 = 7;  // distillery
    const BUILDING_TILE_NAMES: [&str; 8] = [
        "house0",
        "well_small",
        "well_big",
        "market",
        "rice_farm",
        "granary",
        "storage_yard",
        "distillery",
    ];

    // 1 house, 2 wells, 1 market, 1 farm, 1 storage (granary)
    const PRESET_TILES_0: PresetTiles = PresetTiles {
        preset_name: "[0] - 1 house, 2 wells, 1 market, 1 farm, 1 granary",
        map_size_in_cells: Size::new(9, 9),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,R,G,G,G,G,G,G,R,
            R,G,G,G,G,R,R,R,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,R,R,R,R,R,R,R,R,
        ],
        building_tiles: &[
            X,X,X,X,X,X,X,X,X, // <-- start, tile zero is the leftmost (top-left)
            X,H,X,B,X,M,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,W,X,X,X,X,X,X,
            X,F,X,X,X,X,X,X,X,
            X,X,X,X,X,S,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    // 1 farm, 1 storage (granary)
    const PRESET_TILES_1: PresetTiles = PresetTiles {
        preset_name: "[1] - 1 farm, 1 granary",
        map_size_in_cells: Size::new(9, 9),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,R,R,R,R,R,R,R,R,
        ],
        building_tiles: &[
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,S,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,F,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    // 1 farm, 2 storages (granary, storage yard), 1 factory (distillery)
    const PRESET_TILES_2: PresetTiles = PresetTiles {
        preset_name: "[2] - 1 farm, 2 storages (G|Y), 1 distillery",
        map_size_in_cells: Size::new(12, 12),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R,R,R,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,R,R,R,R,R,R,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,R,R,R,R,R,R,R,R,R,R,R,
        ],
        building_tiles: &[
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,A,X,X,X,X,X,X,S,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,F,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,Y,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    const PRESET_TILES: [&PresetTiles; 3] = [
        &PRESET_TILES_0,
        &PRESET_TILES_1,
        &PRESET_TILES_2,
    ];

    fn find_tile(tile_sets: &TileSets, layer_kind: TileMapLayerKind, tile_id: i32) -> Option<&TileDef> {
        if tile_id < 0 {
            return None;
        }

        let category_name = match layer_kind {
            TileMapLayerKind::Terrain => "ground",
            TileMapLayerKind::Objects => "buildings",
        };

        let tile_name = match layer_kind {
            TileMapLayerKind::Terrain => TERRAIN_TILE_NAMES[tile_id as usize],
            TileMapLayerKind::Objects => BUILDING_TILE_NAMES[tile_id as usize],
        };

        tile_sets.find_tile_def_by_name(layer_kind, category_name, tile_name)
    }

    fn build_tile_map<'tile_sets>(preset: &'static PresetTiles,
                                  world: &mut World,
                                  tile_sets: &'tile_sets TileSets) -> TileMap<'tile_sets> {

        let map_size_in_cells = preset.map_size_in_cells;

        let tile_count = (map_size_in_cells.width * map_size_in_cells.height) as usize;
        debug_assert!(preset.terrain_tiles.len()  == tile_count);
        debug_assert!(preset.building_tiles.len() == tile_count);

        let mut tile_map = TileMap::new(map_size_in_cells, None);

        let mut query_builder = DebugQueryBuilder::new(map_size_in_cells);
        let query = query_builder.new_query(world, &mut tile_map, tile_sets);

        // Terrain:
        for y in 0..map_size_in_cells.height {
            for x in 0..map_size_in_cells.width {
                let tile_id = preset.terrain_tiles[(x + (y * map_size_in_cells.width)) as usize];
                if let Some(tile_def) = find_tile(tile_sets, TileMapLayerKind::Terrain, tile_id) {
                    tile_map.try_place_tile_in_layer(Cell::new(x, y), TileMapLayerKind::Terrain, tile_def)
                        .expect("Failed to place Terrain tile!");
                }
            }
        }

        // Buildings (Objects):
        for y in 0..map_size_in_cells.height {
            for x in 0..map_size_in_cells.width {
                let tile_id = preset.building_tiles[(x + (y * map_size_in_cells.width)) as usize];
                if let Some(tile_def) = find_tile(tile_sets, TileMapLayerKind::Objects, tile_id) {
                    world.try_spawn_building_with_tile_def(&query, Cell::new(x, y), tile_def)
                        .expect("Failed to place Building tile!");
                }
            }
        }

        tile_map
    }

    pub fn preset_tile_maps_list_internal() -> Vec<&'static str> {
        PRESET_TILES.iter().map(|preset| preset.preset_name).collect()
    }

    pub fn create_preset_tile_map_internal<'tile_sets>(world: &mut World,
                                                       tile_sets: &'tile_sets TileSets,
                                                       preset_number: usize) -> TileMap<'tile_sets> {
        log::info!("Creating debug tile map: PRESET {} ...", preset_number);
        let preset = PRESET_TILES[preset_number];
        if let Some(enable_cheats_fn) = preset.enable_cheats_fn {
            enable_cheats_fn(cheats::get_mut());
        }
        build_tile_map(preset, world, tile_sets)
    }
}

pub fn preset_tile_maps_list() -> Vec<&'static str> {
    test_maps::preset_tile_maps_list_internal()
}

pub fn create_preset_tile_map<'tile_sets>(world: &mut World,
                                          tile_sets: &'tile_sets TileSets,
                                          preset_number: usize) -> TileMap<'tile_sets> {
    test_maps::create_preset_tile_map_internal(world, tile_sets, preset_number)
}
