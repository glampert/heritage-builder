use rand::SeedableRng;

use common::{
    Color,
    Rect,
    Size,
    Vec2,
    coords::{self, Cell, CellRange, WorldToScreenTransform},
    time::UpdateTimer,
};
use engine::{
    log,
    render::{RenderStats, debug::DebugDraw},
    ui::{self, UiFontScale, UiSystem},
};

use crate::{
    GameLoopStats,
    config::GameConfigs,
    cheats::{self, Cheats},
    pathfind::{Graph, Search},
    unit::task::UnitTaskManager,
    world::{World, object::{Spawner, SpawnerResult}},
    sim::{RandomGenerator, SimContext, resources::GlobalTreasury},
    tile::{
        self,
        Tile,
        TileDepthSortOverride,
        TileFlags,
        TileKind,
        TileMap,
        TileMapLayerKind,
        rendering::{TileMapRenderFlags, TileMapRenderStats},
        sets::{TileDef, TileSets},
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

// ----------------------------------------------
// DebugSimContextBuilder
// ----------------------------------------------

// Dummy SimContext for unit tests/debug.
pub struct DebugSimContextBuilder<'game> {
    rng: RandomGenerator,
    graph: Graph,
    search: Search,
    task_manager: UnitTaskManager,
    treasury: GlobalTreasury,
    world: &'game mut World,
    tile_map: &'game mut TileMap,
}

impl<'game> DebugSimContextBuilder<'game> {
    pub fn new(world: &'game mut World, tile_map: &'game mut TileMap, map_size_in_cells: Size, configs: &GameConfigs) -> Self {
        Self {
            rng: RandomGenerator::seed_from_u64(configs.sim.random_seed),
            graph: Graph::with_empty_grid(map_size_in_cells),
            search: Search::with_grid_size(map_size_in_cells),
            task_manager: UnitTaskManager::default(),
            treasury: GlobalTreasury::new(configs.sim.starting_gold_units),
            world,
            tile_map,
        }
    }

    pub fn new_sim_context(&mut self) -> SimContext {
        SimContext::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            &mut self.task_manager,
            self.world,
            self.tile_map,
            &mut self.treasury,
            0.0,
            false,
            false,
        )
    }
}

// ----------------------------------------------
// Built-in preset test TileMaps
// ----------------------------------------------

#[rustfmt::skip]
mod preset_maps {
    use super::*;

    struct PresetTiles {
        preset_name: &'static str,
        map_size_in_cells: Size,
        terrain_tiles: &'static [i32],
        building_tiles: &'static [i32],
        enable_cheats_fn: Option<fn(&mut Cheats)>,
    }

    // TERRAIN:
    const G: i32 = 0; // grass
    const D: i32 = 1; // dirt
    const R: i32 = 2; // dirt road
    const TERRAIN_TILE_NAMES: [&str; 3] = [
        "grass",
        "dirt",
        road::tile_name(road::RoadKind::Dirt).string,
    ];

    // BUILDINGS:
    const X: i32 = -1; // empty (dummy value)
    const H: i32 = 0;  // house0
    const W: i32 = 1;  // small_well
    const B: i32 = 2;  // large_well
    const M: i32 = 3;  // market
    const F: i32 = 4;  // rice_farm
    const S: i32 = 5;  // granary
    const Y: i32 = 6;  // storage_yard
    const A: i32 = 7;  // distillery
    const BUILDING_TILE_NAMES: [&str; 8] = [
        "house0",
        "small_well",
        "large_well",
        "market",
        "rice_farm",
        "granary",
        "storage_yard",
        "distillery",
    ];

    // Empty 9x9 map. Ring road around the whole map.
    const PRESET_TILES_0: PresetTiles = PresetTiles {
        preset_name: "[0] - empty | 9x9",
        map_size_in_cells: Size::new(9, 9),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
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
            X,X,X,X,X,X,X,X,X, // <-- start, tile zero is the leftmost (top-left)
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
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
        preset_name: "[1] - 1 farm, 1 granary | 9x9",
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
            X,X,X,X,X,X,X,X,X,
            X,F,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    // 1 house, 2 wells, 1 market, 1 farm, 1 storage (granary)
    const PRESET_TILES_2: PresetTiles = PresetTiles {
        preset_name: "[2] - 1 house, 2 wells, 1 market, 1 farm, 1 granary | 9x9",
        map_size_in_cells: Size::new(9, 9),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,G,R,
            R,R,G,G,R,R,R,R,R,
            R,G,G,G,R,G,G,G,R,
            R,G,G,G,R,G,G,G,R,
            R,G,G,G,R,G,G,G,R,
            R,R,R,R,R,R,R,R,R,
        ],
        building_tiles: &[
            X,X,X,X,X,X,X,X,X,
            X,H,X,X,B,X,M,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,W,X,X,X,X,X,X,
            X,F,X,X,X,S,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    // 1 farm, 2 storages (granary, storage yard), 1 factory (distillery)
    const PRESET_TILES_3: PresetTiles = PresetTiles {
        preset_name: "[3] - 1 farm, 2 storages (G|Y), 1 distillery | 12x12",
        map_size_in_cells: Size::new(12, 12),
        terrain_tiles: &[
            R,R,R,R,R,R,R,R,R,R,R,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,G,G,G,G,G,G,R,G,G,G,R,
            R,R,R,R,R,R,R,R,G,G,G,R,
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
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,F,X,X,X,X,X,X,Y,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
            X,X,X,X,X,X,X,X,X,X,X,X,
        ],
        enable_cheats_fn: Some(|cheats| {
            cheats.ignore_worker_requirements = true
        })
    };

    const PRESET_TILES: [&PresetTiles; 4] = [
        &PRESET_TILES_0,
        &PRESET_TILES_1,
        &PRESET_TILES_2,
        &PRESET_TILES_3,
    ];

    fn find_tile(layer_kind: TileMapLayerKind, tile_id: i32) -> Option<&'static TileDef> {
        if tile_id < 0 {
            return None;
        }

        let category_name = match layer_kind {
            TileMapLayerKind::Terrain => tile::sets::TERRAIN_LAND_CATEGORY.string,
            TileMapLayerKind::Objects => tile::sets::OBJECTS_BUILDINGS_CATEGORY.string,
        };

        let tile_name = match layer_kind {
            TileMapLayerKind::Terrain => TERRAIN_TILE_NAMES[tile_id as usize],
            TileMapLayerKind::Objects => BUILDING_TILE_NAMES[tile_id as usize],
        };

        TileSets::get().find_tile_def_by_name(layer_kind, category_name, tile_name)
    }

    fn build_tile_map(preset: &'static PresetTiles, world: &mut World) -> TileMap {
        let map_size_in_cells = preset.map_size_in_cells;

        let tile_count = (map_size_in_cells.width * map_size_in_cells.height) as usize;
        debug_assert!(preset.terrain_tiles.len() == tile_count);
        debug_assert!(preset.building_tiles.len() == tile_count);

        let configs = GameConfigs::get();

        let mut tile_map = TileMap::new(map_size_in_cells, None);
        let mut builder = DebugSimContextBuilder::new(world, &mut tile_map, map_size_in_cells, configs);

        let context = builder.new_sim_context();
        let mut spawner = Spawner::new(&context);
        spawner.set_subtract_tile_cost(false);

        // Terrain:
        for y in 0..map_size_in_cells.height {
            for x in 0..map_size_in_cells.width {
                let tile_id = preset.terrain_tiles[(x + (y * map_size_in_cells.width)) as usize];
                if let Some(tile_def) = find_tile(TileMapLayerKind::Terrain, tile_id) {
                    match spawner.try_spawn_tile_with_def(Cell::new(x, y), tile_def) {
                        SpawnerResult::Tile(tile) => {
                            // Set a random terrain tile variation:
                            if tile.has_flags(TileFlags::RandomizePlacement) {
                                tile.set_random_variation_index(context.rng_mut());
                            }
                        },
                        SpawnerResult::Err(err) => {
                            log::error!(log::channel!("debug"), "Preset: Failed to place Terrain tile: {} - {}", err.reason, err.message);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }

        // Buildings (Objects):
        for y in 0..map_size_in_cells.height {
            for x in 0..map_size_in_cells.width {
                let tile_id = preset.building_tiles[(x + (y * map_size_in_cells.width)) as usize];
                if let Some(tile_def) = find_tile(TileMapLayerKind::Objects, tile_id) {
                    if let Err(err) = spawner.try_spawn_building_with_tile_def(Cell::new(x, y), tile_def) {
                        log::error!(log::channel!("debug"), "Preset: Failed to place Building tile: {} - {}", err.reason, err.message);
                    }
                }
            }
        }

        refresh_cached_tile_visuals(&mut tile_map);
        tile_map
    }

    pub fn preset_tile_maps_list_internal() -> Vec<&'static str> {
        PRESET_TILES.iter().map(|preset| preset.preset_name).collect()
    }

    pub fn create_preset_tile_map_internal(world: &mut World, mut preset_number: usize) -> TileMap {
        preset_number = preset_number.min(PRESET_TILES.len());

        log::info!(log::channel!("debug"),
                   "Creating debug tile map - PRESET: {} ...",
                   preset_number);
        let preset = PRESET_TILES[preset_number];

        if let Some(enable_cheats_fn) = preset.enable_cheats_fn {
            enable_cheats_fn(cheats::get_mut());
        }

        build_tile_map(preset, world)
    }
}

pub fn preset_tile_maps_list() -> Vec<&'static str> {
    preset_maps::preset_tile_maps_list_internal()
}

pub fn create_preset_tile_map(world: &mut World, preset_number: usize) -> TileMap {
    preset_maps::create_preset_tile_map_internal(world, preset_number)
}
