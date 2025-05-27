use std::collections::HashMap;

use crate::{
    ui::UiSystem,
    render::RenderSystem,
    utils::{self, Color, Cell2D, Point2D, Rect2D, IsoPoint2D, Size2D, WorldToScreenTransform}
};

use super::{
    sets::TileSets,
    rendering::TileMapRenderFlags,
    def::{self, TileDef, TileKind, BASE_TILE_SIZE},
    map::{Tile, TileFlags, TileMap, TileMapLayerKind}
};

// ----------------------------------------------
// Debug Draw Helpers
// ----------------------------------------------

pub fn draw_tile_debug(render_sys: &mut RenderSystem,
                       ui_sys: &UiSystem,
                       tile_iso_coords: IsoPoint2D,
                       tile_rect: Rect2D,
                       transform: &WorldToScreenTransform,
                       tile: &Tile,
                       flags: TileMapRenderFlags) {

    let draw_debug_info =
        if tile.flags.contains(TileFlags::DrawDebugInfo) || tile.flags.contains(TileFlags::DrawBlockerInfo) {
            true
        } else {
            if tile.is_terrain() && flags.contains(TileMapRenderFlags::DrawTerrainTileDebugInfo) {
                true
            } else if tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildingsTileDebugInfo) {
                true
            } else if tile.is_blocker() && flags.contains(TileMapRenderFlags::DrawBlockerTilesDebug) {
                true
            } else if tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnitsTileDebugInfo) {
                true
            } else {
                false
            }
        };

    let draw_debug_bounds =
        tile.flags.contains(TileFlags::DrawDebugBounds) ||
        flags.contains(TileMapRenderFlags::DrawTileDebugBounds);

    if draw_debug_info {
        draw_tile_info(
            render_sys,
            ui_sys,
            tile_rect,
            tile_iso_coords,
            tile);
    }

    if draw_debug_bounds {
        draw_tile_bounds(render_sys, tile_rect, transform, tile);
    }
}

fn draw_tile_overlay_text(ui_sys: &UiSystem,
                          debug_overlay_pos: Point2D,
                          tile_screen_pos: Point2D,
                          tile_iso_pos: IsoPoint2D,
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
    let label = format!("{}_{}_{}", tile.name(), tile.cell.x, tile.cell.y);
    let position = [ debug_overlay_pos.x as f32, debug_overlay_pos.y as f32 ];

    let bg_color = match tile.kind() {
        TileKind::Blocker => Color::red().to_array(),
        TileKind::Building => Color::yellow().to_array(),
        TileKind::Unit => Color::cyan().to_array(),
        _ => Color::black().to_array()
    };

    let text_color = match tile.kind() {
        TileKind::Terrain => Color::white().to_array(),
        _ => Color::black().to_array()
    };

    let ui = ui_sys.builder();

    // Adjust window background color based on tile kind.
    // The returned tokens take care of popping back to the previous color/font.
    let _0 = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
    let _1 = ui.push_style_color(imgui::StyleColor::Text, text_color);
    let _2  = ui.push_font(ui_sys.fonts().small);

    ui.window(label)
        .position(position, imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.4) // Semi-transparent
        .build(|| {
            ui.text(format!("C:{},{}", tile.cell.x,       tile.cell.y));       // Cell position
            ui.text(format!("S:{},{}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
            ui.text(format!("I:{},{}", tile_iso_pos.x,    tile_iso_pos.y));    // 2D isometric position
        });
}

fn draw_tile_info(render_sys: &mut RenderSystem,
                  ui_sys: &UiSystem,
                  tile_screen_rect: Rect2D,
                  tile_iso_pos: IsoPoint2D,
                  tile: &Tile) {

    let tile_screen_pos = tile_screen_rect.position();
    let tile_center = tile_screen_rect.center();

    // Center the overlay text box roughly over the tile's center.
    let debug_overlay_pos = Point2D::new(
        tile_center.x - 30,
        tile_center.y - 30);

    draw_tile_overlay_text(
        ui_sys,
        debug_overlay_pos,
        tile_screen_pos,
        tile_iso_pos,
        tile);

    // Put a red dot at the tile's center.
    render_sys.draw_point_fast(tile_center, Color::red(), 10.0);
}

fn draw_tile_bounds(render_sys: &mut RenderSystem,
                    tile_rect: Rect2D,
                    transform: &WorldToScreenTransform,
                    tile: &Tile) {

    let color = match tile.kind() {
        TileKind::Building => Color::yellow(),
        TileKind::Unit => Color::cyan(),
        _ => Color::red()
    };

    // Tile isometric "diamond" bounding box:
    let diamond_points = def::cell_to_screen_diamond_points(
        tile.cell,
        tile.logical_size(),
        transform);

    render_sys.draw_line_fast(diamond_points[0], diamond_points[1], color, color);
    render_sys.draw_line_fast(diamond_points[1], diamond_points[2], color, color);
    render_sys.draw_line_fast(diamond_points[2], diamond_points[3], color, color);
    render_sys.draw_line_fast(diamond_points[3], diamond_points[0], color, color);

    for point in diamond_points {
        render_sys.draw_point_fast(point, Color::green(), 10.0);
    }

    // Tile axis-aligned bounding rectangle of the actual sprite image:
    render_sys.draw_wireframe_rect_fast(tile_rect, color);
}

// Show a small debug overlay under the cursor with its current position.
pub fn draw_cursor_overlay(ui_sys: &UiSystem, transform: &WorldToScreenTransform) {
    let ui = ui_sys.builder();
    let cursor_screen_pos = ui.io().mouse_pos;

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
        .position([cursor_screen_pos[0] + 10.0, cursor_screen_pos[1] + 10.0], imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.6) // Semi-transparent
        .build(|| {
            let cursor_iso_pos = utils::screen_to_iso_point(
                Point2D::new(cursor_screen_pos[0] as i32, cursor_screen_pos[1] as i32),
                transform, BASE_TILE_SIZE, false);

            let cursor_approx_cell = utils::iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);

            ui.text(format!("C:{},{}", cursor_approx_cell.x, cursor_approx_cell.y));
            ui.text(format!("S:{},{}", cursor_screen_pos[0], cursor_screen_pos[1]));
            ui.text(format!("I:{},{}", cursor_iso_pos.x, cursor_iso_pos.y));
        });
}

pub fn draw_screen_origin_marker(render_sys: &mut RenderSystem) {
    // 50x50px white square to mark the origin.
    render_sys.draw_point_fast(
        Point2D::new(0, 0), 
        Color::white(),
        50.0);

    // Red line for the X axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Point2D::new(0, 0),
        Point2D::new(100, 0),
        Color::red(),
        15.0);

    render_sys.draw_colored_rect(
        Rect2D::new(Point2D::new(100, 0), Size2D::new(10, 10)),
        Color::green());

    // Blue line for the Y axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Point2D::new(0, 0),
        Point2D::new(0, 100),
        Color::blue(),
        15.0);

    render_sys.draw_colored_rect(
        Rect2D::new(Point2D::new(0, 100), Size2D::new(10, 10)),
        Color::green());
}

// ----------------------------------------------
// Test map setup helpers
// ----------------------------------------------

pub fn create_test_tile_map(tile_sets: &TileSets) -> TileMap {
    println!("Creating test tile map...");

    const MAP_WIDTH:  i32 = 8;
    const MAP_HEIGHT: i32 = 8;

    const G:  i32 = 0; // ground:grass (empty)
    const R:  i32 = 1; // ground:road/dirt (empty)
    const U:  i32 = 2; // unit:ped
    const HH: i32 = 3; // building:house (2x2)
    const TT: i32 = 4; // building:tower (3x3)
    const B0: i32 = 5; // special blocker for the 3x3 building.
    const B1: i32 = 6; // special blocker for the 2x2 building.
    const B2: i32 = 7; // special blocker for the 2x2 building.
    const B3: i32 = 8; // special blocker for the 2x2 building.
    const B4: i32 = 9; // special blocker for the 2x2 building.

    const TILE_NAMES: [&str; 5] = [ "grass", "dirt", "ped", "house", "tower" ];
    const TILE_CATEGORIES: [&str; 5] = [ "ground", "ground", "on_foot", "residential", "residential" ];

    let find_tile = |layer_kind: TileMapLayerKind, tile_id: i32| {
        let tile_name = TILE_NAMES[tile_id as usize];
        let category_name = TILE_CATEGORIES[tile_id as usize];
        tile_sets.find_tile_by_name(layer_kind, category_name, tile_name).unwrap_or(TileDef::empty())
    };

    const TERRAIN_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,
    ];

    const BUILDINGS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        G, G, G, G, G, G, G, G, // <-- start, tile zero is the leftmost (top-left)
        G, TT,B0,B0,G, HH,B1,G,
        G, B0,B0,B0,G, B1,B1,G,
        G, B0,B0,B0,G, HH,B2,G,
        G, G, G, G, G, B2,B2,G,
        G, HH,B4,G, G, HH,B3,G,
        G, B4,B4,G, G, B3,B3,G,
        G, G, G, G, G, G, G, G,
    ];

    const UNITS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        U,U,U,U,U,U,U,U, // <-- start, tile zero is the leftmost (top-left)
        U,G,G,G,U,G,G,U,
        U,G,G,G,U,G,G,U,
        U,G,G,G,U,G,G,U,
        U,U,U,U,U,G,G,U,
        U,G,G,U,U,G,G,U,
        U,G,G,U,U,G,G,U,
        U,U,U,U,U,U,U,U,
    ];

    let blockers_mapping = HashMap::from([
        (B0, Cell2D::new(1, 1)),
        (B1, Cell2D::new(5, 1)),
        (B2, Cell2D::new(5, 3)),
        (B3, Cell2D::new(5, 5)),
        (B4, Cell2D::new(1, 5)),
    ]);

    let mut tile_map = TileMap::new(Size2D::new(MAP_WIDTH, MAP_HEIGHT));

    // Terrain:
    {
        let terrain_layer = tile_map.layer_mut(TileMapLayerKind::Terrain);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = TERRAIN_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let tile_def = find_tile(TileMapLayerKind::Terrain, tile_id);
                terrain_layer.add_tile(Cell2D::new(x, y), tile_def);
            }
        }
    }

    // Buildings:
    {
        let buildings_layer = tile_map.layer_mut(TileMapLayerKind::Buildings);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = BUILDINGS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let cell = Cell2D::new(x, y);

                if tile_id == G { // ground/empty
                    buildings_layer.add_empty_tile(cell);
                } else if tile_id >= B0 { // building blocker
                    let owner_cell = blockers_mapping.get(&tile_id).unwrap();
                    buildings_layer.add_blocker_tile(cell, *owner_cell);
                } else { // building tile
                    let tile_def = find_tile(TileMapLayerKind::Buildings, tile_id);
                    buildings_layer.add_tile(cell, tile_def);
                }
            }
        }
    }

    // Units:
    {
        let units_layer = tile_map.layer_mut(TileMapLayerKind::Units);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = UNITS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let cell = Cell2D::new(x, y);

                if tile_id == G { // ground/empty
                    units_layer.add_empty_tile(cell);
                } else { // unit tile
                    let tile_def = find_tile(TileMapLayerKind::Units, tile_id);
                    units_layer.add_tile(cell, tile_def);
                }
            }
        }
    }

    tile_map
}
