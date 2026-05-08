use common::{
    Color,
    Rect,
    Vec2,
    coords::{self, CellRange, WorldToScreenTransform},
    time::UpdateTimer,
};
use engine::{
    render::{RenderStats, debug::DebugDraw},
    ui::{self, UiFontScale, UiSystem},
};

use crate::{
    GameLoopStats,
    world::World,
    tile::{
        Tile,
        TileDepthSortOverride,
        TileFlags,
        TileKind,
        TileMap,
        rendering::{TileMapRenderFlags, TileMapRenderStats},
        road,
        water,
    },
};

// ----------------------------------------------
// Debug Draw Helpers
// ----------------------------------------------

// Format small strings (128 bytes max) without allocating.
macro_rules! format_fast {
    ($($arg:tt)*) => {
        common::format_fixed_string_trunc!(128, $($arg)*)
    };
}

pub fn draw_tile_debug(
    debug_draw: &mut DebugDraw,
    ui_sys: &UiSystem,
    tile_screen_rect: Rect,
    transform: WorldToScreenTransform,
    tile: &Tile,
    flags: TileMapRenderFlags,
) {
    let draw_debug_info = {
        tile.has_flags(TileFlags::DrawDebugInfo | TileFlags::DrawBlockerInfo) ||
        (tile.is(TileKind::Terrain)    && flags.contains(TileMapRenderFlags::DrawTerrainTileDebug))   ||
        (tile.is(TileKind::Blocker)    && flags.contains(TileMapRenderFlags::DrawBlockersTileDebug))  ||
        (tile.is(TileKind::Building)   && flags.contains(TileMapRenderFlags::DrawBuildingsTileDebug)) ||
        (tile.is(TileKind::Unit)       && flags.contains(TileMapRenderFlags::DrawUnitsTileDebug))     ||
        (tile.is(TileKind::Rocks)      && flags.contains(TileMapRenderFlags::DrawPropsTileDebug))     ||
        (tile.is(TileKind::Vegetation) && flags.contains(TileMapRenderFlags::DrawVegetationTileDebug))
    };

    let draw_debug_bounds =
        tile.has_flags(TileFlags::DrawDebugBounds) || flags.contains(TileMapRenderFlags::DrawDebugBounds);

    if draw_debug_info {
        draw_tile_info(debug_draw, ui_sys, tile_screen_rect, tile);
    }

    if draw_debug_bounds {
        if tile.has_flags(TileFlags::BuildingRoadLink) {
            draw_road_link_bounds(debug_draw, tile_screen_rect, transform, tile);
        } else {
            draw_tile_bounds(debug_draw, tile_screen_rect, transform, tile, true, true);
        }
    }
}

// Show a small debug overlay under the cursor with its current position or provided text.
pub fn draw_cursor_overlay(
    ui_sys: &UiSystem,
    transform: WorldToScreenTransform,
    cursor_screen_pos: Vec2,
    opt_text: Option<&str>,
) {
    let ui = ui_sys.ui();

    ui::overlay(ui, "Cursor Debug", cursor_screen_pos + Vec2::new(10.0, 10.0), 0.6, || {
        if let Some(text) = opt_text {
            ui.text(text);
        } else {
            let cursor_iso_pos = coords::screen_to_iso_point(cursor_screen_pos, transform);
            let cursor_approx_cell = coords::iso_to_cell(cursor_iso_pos);

            ui.text(format_fast!("C:{},{}", cursor_approx_cell.x, cursor_approx_cell.y));
            ui.text(format_fast!("S:{:.1},{:.1}", cursor_screen_pos.x, cursor_screen_pos.y));
            ui.text(format_fast!("I:{},{}", cursor_iso_pos.x, cursor_iso_pos.y));
        }
    });
}

pub fn draw_screen_origin_marker(debug_draw: &mut DebugDraw) {
    let y_offset = 20.0; // Draw it below the main menu bar.

    // 50x50px white square to mark the origin.
    debug_draw.point(Vec2::new(0.0, y_offset), Color::white(), 50.0);

    // Red line for the X axis, green square at the end.
    debug_draw.line_with_thickness(Vec2::new(0.0, y_offset), Vec2::new(100.0, y_offset), Color::red(), 15.0);

    debug_draw
        .colored_rect(Rect::from_pos_and_size(Vec2::new(100.0, y_offset - 2.0), Vec2::new(10.0, 10.0)), Color::green());

    // Blue line for the Y axis, green square at the end.
    debug_draw.line_with_thickness(Vec2::new(2.0, y_offset), Vec2::new(2.0, 100.0 + y_offset), Color::blue(), 15.0);

    debug_draw
        .colored_rect(Rect::from_pos_and_size(Vec2::new(0.0, 100.0 + y_offset), Vec2::new(10.0, 10.0)), Color::green());
}

pub fn draw_render_perf_stats(ui_sys: &UiSystem, render_sys_stats: &RenderStats, tile_render_stats: &TileMapRenderStats) {
    let ui = ui_sys.ui();
    let position = Vec2::new(5.0, ui.io().display_size[1] - 250.0);

    ui::overlay(ui, "Render Stats", position, 0.8, || {
        ui.text_colored(Color::yellow().to_array(),
            format_fast!("Render submit     : {:.2}ms",
                               render_sys_stats.render_submit_time_ms));

        ui.text_colored(Color::yellow().to_array(),
            format_fast!("Tiles drawn       : {} | Peak: {}",
                               tile_render_stats.tiles_drawn,
                               tile_render_stats.peak_tiles_drawn));

        ui.text_colored(Color::yellow().to_array(),
            format_fast!("Triangles drawn   : {} | Peak: {}",
                               render_sys_stats.triangles_drawn,
                               render_sys_stats.peak_triangles_drawn));

        ui.text_colored(Color::yellow().to_array(),
            format_fast!("Texture changes   : {} | Peak: {}",
                               render_sys_stats.texture_changes,
                               render_sys_stats.peak_texture_changes));

        ui.text_colored(Color::yellow().to_array(),
            format_fast!("Draw calls        : {} | Peak: {}",
                               render_sys_stats.draw_calls,
                               render_sys_stats.peak_draw_calls));

        ui.text(format_fast!("Tile sort list    : {} | Peak: {}",
                             tile_render_stats.tile_sort_list_len,
                             tile_render_stats.peak_tile_sort_list_len));

        ui.text(format_fast!("Tiles highlighted : {} | Peak: {}",
                             tile_render_stats.tiles_drawn_highlighted,
                             tile_render_stats.peak_tiles_drawn_highlighted));

        ui.text(format_fast!("Tiles invalidated : {} | Peak: {}",
                             tile_render_stats.tiles_drawn_invalidated,
                             tile_render_stats.peak_tiles_drawn_invalidated));

        ui.text(format_fast!("Lines drawn       : {} | Peak: {}",
                             render_sys_stats.lines_drawn,
                             render_sys_stats.peak_lines_drawn));

        ui.text(format_fast!("Points drawn      : {} | Peak: {}",
                             render_sys_stats.points_drawn,
                             render_sys_stats.peak_points_drawn));
    });
}

pub fn draw_world_perf_stats(
    ui_sys: &UiSystem,
    world: &World,
    tile_map: &TileMap,
    visible_range: CellRange,
    game_stats: &GameLoopStats,
) {
    let ui = ui_sys.ui();
    let position = Vec2::new(5.0, 30.0);

    ui::overlay(ui, "Game Stats", position, 0.8, || {
        let (buildings_spawned, peak_buildings_spawned) = world.buildings_stats();
        let (units_spawned, peak_units_spawned) = world.units_stats();
        let (props_spawned, peak_props_spawned) = world.prop_stats();

        let map_stats = tile_map.stats();
        let map_mem_usage_bytes = tile_map.memory_usage_estimate();

        ui.text("Game Objects:");
        ui.text(format_fast!("- Buildings  : {buildings_spawned} | Peak: {peak_buildings_spawned}"));
        ui.text(format_fast!("- Units      : {units_spawned} | Peak: {peak_units_spawned}"));
        ui.text(format_fast!("- Props      : {props_spawned} | Peak: {peak_props_spawned}"));
        ui.text("Tile Map:");
        ui.text(format_fast!("- Terrain    : {}", map_stats.terrain_tiles));
        ui.text(format_fast!("- Buildings  : {}", map_stats.building_tiles));
        ui.text(format_fast!("- Blockers   : {}", map_stats.blocker_tiles));
        ui.text(format_fast!("- Units      : {}", map_stats.unit_tiles));
        ui.text(format_fast!("- Vegetation : {}", map_stats.vegetation_tiles));
        ui.text(format_fast!("- Rocks      : {}", map_stats.rock_tiles));
        ui.text(format_fast!("- Memory     : {}kb", map_mem_usage_bytes / 1024));
        ui.text("Vis Cells:");
        ui.text(format_fast!("- Start      : [{},{}]", visible_range.x(), visible_range.y()));
        ui.text(format_fast!("- Count      : {}x{}", visible_range.width(), visible_range.height()));
        ui.text("Frame Times:");
        ui.text(format_fast!("- FPS        : {:.1}", game_stats.fps));
        ui.text(format_fast!("- Frame      : {:.2}ms", game_stats.total_frame_time_ms));
        ui.text(format_fast!("- Sim        : {:.2}ms", game_stats.sim_frame_time_ms));
        ui.text(format_fast!("- Anim       : {:.2}ms", game_stats.anim_frame_time_ms));
        ui.text(format_fast!("- Sound      : {:.2}ms", game_stats.sound_frame_time_ms));
        ui.text(format_fast!("- Draw World : {:.2}ms", game_stats.draw_world_frame_time_ms));
        ui.text(format_fast!("- Ui B/E     : {:.2}ms/{:.2}ms", game_stats.ui_begin_frame_time_ms, game_stats.ui_end_frame_time_ms));
        ui.text(format_fast!("- Engine B/E : {:.2}ms/{:.2}ms", game_stats.engine_begin_frame_time_ms, game_stats.engine_end_frame_time_ms));
        ui.text(format_fast!("- Present    : {:.2}ms", game_stats.present_frame_time_ms));
    });
}

// ----------------------------------------------
// UpdateTimerDebugUi
// ----------------------------------------------

// Extension trait adding draw_debug_ui() to UpdateTimer.
pub trait UpdateTimerDebugUi {
    fn draw_debug_ui(&mut self, label: &str, imgui_id: u32, ui_sys: &UiSystem);
}

impl UpdateTimerDebugUi for UpdateTimer {
    fn draw_debug_ui(&mut self, label: &str, imgui_id: u32, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        ui.text(format_fast!("{}:", label));

        ui.input_float(format_fast!("Frequency (secs)##_timer_frequency_{}", imgui_id), &mut self.update_frequency_secs)
            .display_format("%.2f")
            .step(0.5)
            .build();

        ui.input_float(format_fast!("Time since last##_last_update_{}", imgui_id), &mut self.time_since_last_secs())
            .display_format("%.2f")
            .read_only(true)
            .build();
    }
}

// ----------------------------------------------
// Internal Helpers
// ----------------------------------------------

fn draw_tile_overlay_text(ui_sys: &UiSystem, debug_overlay_pos: Vec2, tile_screen_pos: Vec2, tile: &Tile) {
    // NOTE: Label has to be unique for each tile because it will be used as the
    // ImGui ID for this widget.
    let cell = tile.actual_base_cell();
    let label = format_fast!("{}_{}_{}", tile.name(), cell.x, cell.y);

    let bg_color = {
        if tile.is(TileKind::Blocker) {
            Color::red().to_array()
        } else if tile.is(TileKind::Building) {
            Color::yellow().to_array()
        } else if tile.is(TileKind::Rocks) {
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

    let ui = ui_sys.ui();

    // Adjust window background color based on tile kind.
    // The returned tokens take care of popping back to the previous colors.
    let _bg_col = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
    let _text_col = ui.push_style_color(imgui::StyleColor::Text, text_color);

    ui::overlay(ui, &label, debug_overlay_pos, 0.4, || {
        ui_sys.set_window_font_scale(UiFontScale(0.8));

        let tile_iso_pos = tile.iso_coords_f32();
        ui.text(format_fast!("C:{},{}", cell.x, cell.y)); // Cell position
        ui.text(format_fast!("S:{:.1},{:.1}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
        ui.text(format_fast!("I:{:.1},{:.1}", tile_iso_pos.0.x, tile_iso_pos.0.y)); // 2D isometric position

        // Z/Depth sorting:
        match tile.depth_sort_override() {
            TileDepthSortOverride::None => {}
            TileDepthSortOverride::Topmost => ui.text("Z: Top"),
            TileDepthSortOverride::Bottommost => ui.text("Z: Bottom"),
        }

        ui_sys.set_window_font_scale(UiFontScale::default());
    });
}

fn draw_tile_info(debug_draw: &mut DebugDraw, ui_sys: &UiSystem, tile_screen_rect: Rect, tile: &Tile) {
    let tile_screen_pos = tile_screen_rect.position();
    let tile_center = tile_screen_rect.center();

    // Center the overlay text box roughly over the tile's center.
    let debug_overlay_pos = Vec2::new(tile_center.x - 40.0, tile_center.y - 40.0);

    draw_tile_overlay_text(ui_sys, debug_overlay_pos, tile_screen_pos, tile);

    // Put a dot at the tile's center.
    let center_pt_color = if tile.is(TileKind::Blocker) { Color::white() } else { Color::red() };
    debug_draw.point(tile_center - Vec2::new(2.5, 2.5), center_pt_color, 10.0);
}

// Alternate debug info used for displaying building road link tiles.
fn draw_road_link_bounds(
    debug_draw: &mut DebugDraw,
    tile_screen_rect: Rect,
    transform: WorldToScreenTransform,
    tile: &Tile,
) {
    draw_tile_bounds(debug_draw, tile_screen_rect, transform, tile, true, false);

    let tile_center = tile_screen_rect.center();
    debug_draw.point(tile_center - Vec2::new(2.5, 2.5), Color::blue(), 10.0);
}

fn draw_tile_bounds(
    debug_draw: &mut DebugDraw,
    tile_screen_rect: Rect,
    transform: WorldToScreenTransform,
    tile: &Tile,
    diamond_iso: bool,
    sprite_aabb: bool,
) {
    let color = {
        if tile.is(TileKind::Blocker) {
            Color::red()
        } else if tile.is(TileKind::Building) {
            Color::yellow()
        } else if tile.is(TileKind::Rocks) {
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
        let diamond_points = coords::cell_to_screen_diamond_points(tile.base_cell(), tile.logical_size(), transform);

        debug_draw.line(diamond_points[0], diamond_points[1], color, color);
        debug_draw.line(diamond_points[1], diamond_points[2], color, color);
        debug_draw.line(diamond_points[2], diamond_points[3], color, color);
        debug_draw.line(diamond_points[3], diamond_points[0], color, color);

        for point in diamond_points {
            debug_draw.point(point, Color::green(), 10.0);
        }
    }

    // Tile axis-aligned bounding rectangle of the actual sprite image:
    if sprite_aabb {
        debug_draw.wireframe_rect(tile_screen_rect, color);
    }
}

// Refresh state cached from TileDef during placement and road junction variations.
pub fn refresh_cached_tile_visuals(tile_map: &mut TileMap) {
    let mut road_cells  = Vec::new();
    let mut water_cells = Vec::new();
    let mut port_cells  = Vec::new();

    tile_map.for_each_tile_mut(TileKind::Terrain, |tile_map, tile| {
        tile_map.on_tile_def_edited(tile);
        if tile.path_kind().is_road() {
            road_cells.push(tile.base_cell());
        }
        if tile.path_kind().is_water() {
            water_cells.push(tile.base_cell());
        }
    });

    tile_map.for_each_tile_mut(
        TileKind::Building | TileKind::Unit | TileKind::Rocks | TileKind::Vegetation,
        |tile_map, tile| {
            tile_map.on_tile_def_edited(tile);
            if water::is_port_or_wharf(tile.tile_def()) {
                port_cells.push(tile.base_cell());
            }
        },
    );

    for cell in road_cells {
        road::update_junctions(tile_map, cell);
    }

    for cell in water_cells {
        water::update_transitions(tile_map, cell);
    }

    for cell in port_cells {
        water::update_port_wharf_orientation(tile_map, cell);
    }
}
