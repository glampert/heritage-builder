use crate::{
    imgui_ui::UiSystem,
    render::{RenderSystem, RenderStats},
    utils::{
        Color, Vec2, Rect,
        coords::{
            self,
            IsoPoint,
            WorldToScreenTransform
        }
    },
    tile::{
        map::{Tile, TileFlags},
        sets::{TileKind, BASE_TILE_SIZE},
        rendering::{TileMapRenderFlags, TileMapRenderStats}
    }
};

// ----------------------------------------------
// Debug Draw Helpers
// ----------------------------------------------

#[inline]
pub fn draw_tile_debug(render_sys: &mut impl RenderSystem,
                       ui_sys: &UiSystem,
                       tile_iso_pos: IsoPoint,
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
        draw_tile_info(
            render_sys,
            ui_sys,
            tile_screen_rect,
            tile_iso_pos,
            tile);
    }

    if draw_debug_bounds {
        draw_tile_bounds(render_sys, tile_screen_rect, transform, tile);
    }
}

// Show a small debug overlay under the cursor with its current position.
pub fn draw_cursor_overlay(ui_sys: &UiSystem, transform: &WorldToScreenTransform) {
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
            let cursor_iso_pos = coords::screen_to_iso_point(
                cursor_screen_pos,
                transform,
                BASE_TILE_SIZE);

            let cursor_approx_cell = coords::iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);

            ui.text(format!("C:{},{}", cursor_approx_cell.x, cursor_approx_cell.y));
            ui.text(format!("S:{:.1},{:.1}", cursor_screen_pos.x, cursor_screen_pos.y));
            ui.text(format!("I:{},{}", cursor_iso_pos.x, cursor_iso_pos.y));
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
                          tile_iso_pos: IsoPoint,
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
    let position = [ debug_overlay_pos.x, debug_overlay_pos.y ];

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
    let _0 = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
    let _1 = ui.push_style_color(imgui::StyleColor::Text, text_color);
    let _2  = ui.push_font(ui_sys.fonts().small);

    ui.window(label)
        .position(position, imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.4) // Semi-transparent
        .build(|| {
            ui.text(format!("C:{},{}", cell.x, cell.y)); // Cell position
            ui.text(format!("S:{:.1},{:.1}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
            ui.text(format!("I:{},{}", tile_iso_pos.x, tile_iso_pos.y)); // 2D isometric position
            ui.text(format!("Z:{}", tile.calc_z_sort())); // Z-sort
        });
}

fn draw_tile_info(render_sys: &mut impl RenderSystem,
                  ui_sys: &UiSystem,
                  tile_screen_rect: Rect,
                  tile_iso_pos: IsoPoint,
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
        tile_iso_pos,
        tile);

    // Put a dot at the tile's center.
    let center_pt_color = if tile.is(TileKind::Blocker) { Color::white() } else { Color::red() };
    render_sys.draw_point_fast(tile_center, center_pt_color, 10.0);
}

fn draw_tile_bounds(render_sys: &mut impl RenderSystem,
                    tile_screen_rect: Rect,
                    transform: &WorldToScreenTransform,
                    tile: &Tile) {

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

    // Tile axis-aligned bounding rectangle of the actual sprite image:
    render_sys.draw_wireframe_rect_fast(tile_screen_rect, color);
}
