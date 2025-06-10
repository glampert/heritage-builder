use bitflags::bitflags;
use smallvec::SmallVec;

use crate::{
    imgui_ui::UiSystem,
    render::RenderSystem,
    debug::{self},
    utils::{self, Cell, Color, IsoPoint, Vec2, WorldToScreenTransform}
};

use super::{
    sets::BASE_TILE_SIZE,
    selection::CellRange,
    map::{Tile, TileFlags, TileMap, TileMapLayerKind}
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const HIGHLIGHT_TILE_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const INVALID_TILE_COLOR:   Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red

pub const SELECTION_RECT_COLOR: Color = Color::new(0.2, 0.7, 0.2, 1.0); // green-ish
pub const MAP_BACKGROUND_COLOR: Color = Color::gray();

pub const DEFAULT_GRID_COLOR:   Color = Color::white();
pub const HIGHLIGHT_GRID_COLOR: Color = Color::green();
pub const INVALID_GRID_COLOR:   Color = Color::red();

pub const MIN_GRID_LINE_THICKNESS: f32 = 0.5;
pub const MAX_GRID_LINE_THICKNESS: f32 = 20.0;

// ----------------------------------------------
// TileMapRenderFlags
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct TileMapRenderFlags: u32 {
        const DrawTerrain         = 1 << 1;
        const DrawBuildings       = 1 << 2;
        const DrawUnits           = 1 << 3;
        const DrawGrid            = 1 << 4; // Grid draws on top of terrain but under buildings/units.
        const DrawGridIgnoreDepth = 1 << 5; // Grid draws on top of everything ignoring z-sort order.

        // Debug flags:
        const DrawTerrainTileDebugInfo   = 1 << 6;
        const DrawBuildingsTileDebugInfo = 1 << 7;
        const DrawUnitsTileDebugInfo     = 1 << 8;
        const DrawTileDebugBounds        = 1 << 9;
        const DrawBlockerTilesDebug      = 1 << 10;
    }
}

// ----------------------------------------------
// TileMapRenderStats
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct TileMapRenderStats {
    // Current frame totals:
    pub tiles_drawn: u32,
    pub tiles_drawn_highlighted: u32,
    pub tiles_drawn_invalidated: u32,
    pub tile_sort_list_len: u32,
    // Peaks for the whole run:
    pub peak_tiles_drawn: u32,
    pub peak_tiles_drawn_highlighted: u32,
    pub peak_tiles_drawn_invalidated: u32,
    pub peak_tile_sort_list_len: u32,
}

// ----------------------------------------------
// TileMapRenderer
// ----------------------------------------------

pub struct TileMapRenderer {
    grid_color: Color,
    grid_line_thickness: f32,
    stats: TileMapRenderStats,
    temp_tile_sort_list: Vec<TileDrawListEntry>, // For z-sorting.
}

impl TileMapRenderer {
    pub fn new(grid_color: Color, grid_line_thickness: f32) -> Self {
        Self {
            grid_color: grid_color,
            grid_line_thickness: grid_line_thickness.clamp(MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS),
            stats: TileMapRenderStats::default(),
            temp_tile_sort_list: Vec::with_capacity(512),
        }
    }

    pub fn set_grid_color(&mut self, color: Color) {
        self.grid_color = color;
    }

    pub fn grid_color(&self) -> Color {
        self.grid_color
    }

    pub fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.grid_line_thickness = thickness.clamp(MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS);
    }

    pub fn grid_line_thickness(&self) -> f32 {
        self.grid_line_thickness
    }

    pub fn draw_map(&mut self,
                    render_sys: &mut impl RenderSystem,
                    ui_sys: &UiSystem,
                    tile_map: &TileMap,
                    transform: &WorldToScreenTransform,
                    visible_range: CellRange,
                    flags: TileMapRenderFlags) -> TileMapRenderStats {

        self.reset_stats();

        self.draw_terrain_layer(render_sys, ui_sys, tile_map, transform, visible_range, flags);

        if flags.contains(TileMapRenderFlags::DrawGrid) && !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range);
        }

        self.draw_buildings_and_units_layer(render_sys, ui_sys, tile_map, transform, visible_range, flags);

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow grid lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range);
        }

        self.update_stats()
    }

    fn draw_terrain_layer(&mut self,
                          render_sys: &mut impl RenderSystem,
                          ui_sys: &UiSystem,
                          tile_map: &TileMap,
                          transform: &WorldToScreenTransform,
                          visible_range: CellRange,
                          flags: TileMapRenderFlags) {

        if !flags.contains(TileMapRenderFlags::DrawTerrain) {
            return;
        }

        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let buildings_layer = tile_map.layer(TileMapLayerKind::Buildings);

        debug_assert!(terrain_layer.size() == tile_map.size());
        debug_assert!(buildings_layer.size() == tile_map.size());

        for y in (visible_range.min.y..=visible_range.max.y).rev() {
            for x in (visible_range.min.x..=visible_range.max.x).rev() {
                let cell = Cell::new(x, y);

                let tile = terrain_layer.tile(cell);
                if tile.is_empty() {
                    continue;
                }

                // Terrain tiles size is constrained.
                debug_assert!(tile.is_terrain() && tile.logical_size() == BASE_TILE_SIZE);

                let building_tile = buildings_layer.tile(cell);
                if !building_tile.is_empty() && building_tile.occludes_terrain() {
                    // Skip drawing terrain if fully occluded.
                    continue;
                }

                Self::draw_tile(render_sys,
                                &mut self.stats,
                                ui_sys,
                                tile.calc_adjusted_iso_coords(),
                                transform,
                                tile,
                                flags);
            }
        }
    }

    fn draw_buildings_and_units_layer(&mut self,
                                      render_sys: &mut impl RenderSystem,
                                      ui_sys: &UiSystem,
                                      tile_map: &TileMap,
                                      transform: &WorldToScreenTransform,
                                      visible_range: CellRange,
                                      flags: TileMapRenderFlags) {

        if !flags.intersects(TileMapRenderFlags::DrawBuildings | TileMapRenderFlags::DrawUnits) {
            return;
        }

        let buildings_layer = tile_map.layer(TileMapLayerKind::Buildings);
        let units_layer = tile_map.layer(TileMapLayerKind::Units);

        debug_assert!(buildings_layer.size() == tile_map.size());
        debug_assert!(units_layer.size() == tile_map.size());
        debug_assert!(self.temp_tile_sort_list.is_empty());

        let mut add_to_sort_list = |tile: &Tile| {
            self.temp_tile_sort_list.push(TileDrawListEntry {
                tile_ptr: tile as *const Tile<'_> as *const Tile<'static>,
                z_sort: tile.calc_z_sort(),
            });
        };

        // Drawing in reverse order (bottom to top) is required to ensure
        // buildings with the same Z-sort value don't overlap in weird ways.
        for y in (visible_range.min.y..=visible_range.max.y).rev() {
            for x in (visible_range.min.x..=visible_range.max.x).rev() {

                let cell = Cell::new(x, y);
                let building_tile = buildings_layer.tile(cell);
                let unit_tile = units_layer.tile(cell);

                if building_tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildings) {
                    add_to_sort_list(building_tile);
                } else if unit_tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnits) {
                    add_to_sort_list(unit_tile);
                } else if building_tile.is_blocker() && // DEBUG:
                            (flags.contains(TileMapRenderFlags::DrawBlockerTilesDebug) ||
                            building_tile.has_flags(TileFlags::DrawBlockerInfo)) {

                    Self::draw_tile(render_sys,
                                    &mut self.stats,
                                    ui_sys,
                                    building_tile.calc_adjusted_iso_coords(),
                                    transform,
                                    building_tile,
                                    flags);
                }
            }
        }

        self.temp_tile_sort_list.sort_by(|a, b| {
            a.z_sort.cmp(&b.z_sort)
        });

        for entry in &self.temp_tile_sort_list {
            // SAFETY: This reference only lives for the scope of this function.
            // The only reason we store it in a member Vec is to avoid the memory
            // allocation cost of a temp local Vec. temp_tile_draw_list is always
            // cleared at the end of this function.
            debug_assert!(entry.tile_ptr.is_null() == false);
            let tile = unsafe { &*entry.tile_ptr };

            debug_assert!(tile.is_building() || tile.is_unit());

            Self::draw_tile(render_sys,
                            &mut self.stats,
                            ui_sys,
                            tile.calc_adjusted_iso_coords(),
                            transform,
                            tile,
                            flags);
        }

        self.stats.tile_sort_list_len += self.temp_tile_sort_list.len() as u32;
        self.temp_tile_sort_list.clear();
    }

    fn draw_isometric_grid(&self,
                           render_sys: &mut impl RenderSystem,
                           tile_map: &TileMap,
                           transform: &WorldToScreenTransform,
                           visible_range: CellRange) {

        // Returns true only if all points are offscreen.
        let viewport = render_sys.viewport();
        let is_fully_offscreen = |points: &[Vec2; 4]| {

            let mut offscreen_count = 0;
            for pt in points {
                if pt.x < viewport.min.x || pt.y < viewport.min.y {
                    offscreen_count += 1;
                } else if pt.x > viewport.max.x || pt.y > viewport.max.y {
                    offscreen_count += 1;
                }
            }

            if offscreen_count == points.len() { true } else { false }
        };

        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * transform.scaling;

        let mut highlighted_cells = SmallVec::<[[Vec2; 4]; 128]>::new();
        let mut invalidated_cells = SmallVec::<[[Vec2; 4]; 128]>::new();

        for y in visible_range.min.y..=visible_range.max.y {
            for x in visible_range.min.x..=visible_range.max.x {
                let cell = Cell::new(x, y);
                let points = utils::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE, BASE_TILE_SIZE, transform);

                if is_fully_offscreen(&points) {
                    continue; // Cull if fully offscreen.
                }

                // Save highlighted grid cells for drawing at the end, so they display in the right order.
                let tile = terrain_layer.tile(cell);
    
                if tile.has_flags(TileFlags::Highlighted) {
                    highlighted_cells.push(points);
                    continue;
                }

                if tile.has_flags(TileFlags::Invalidated) {
                    invalidated_cells.push(points);
                    continue;
                }

                // Draw diamond:
                render_sys.draw_polyline_with_thickness(&points, self.grid_color, line_thickness, true);
            }

            // Highlighted on top:
            for points in &highlighted_cells {
                render_sys.draw_polyline_with_thickness(points, HIGHLIGHT_GRID_COLOR, line_thickness, true);
            }

            for points in &invalidated_cells {
                render_sys.draw_polyline_with_thickness(points, INVALID_GRID_COLOR, line_thickness, true);
            }
        }
    }

    fn draw_tile(render_sys: &mut impl RenderSystem,
                 stats: &mut TileMapRenderStats,
                 ui_sys: &UiSystem,
                 tile_iso_pos: IsoPoint,
                 transform: &WorldToScreenTransform,
                 tile: &Tile,
                 flags: TileMapRenderFlags) {

        debug_assert!(tile.is_valid() && !tile.is_empty());

        let tile_screen_rect = tile.calc_screen_rect(transform);

        if !tile.has_flags(TileFlags::Hidden) {
            if let Some(tile_sprite) = tile.anim_frame_tex_info() {
                let highlight_color =
                    if tile.has_flags(TileFlags::Highlighted) {
                        stats.tiles_drawn_highlighted += 1;
                        HIGHLIGHT_TILE_COLOR
                    } else if tile.has_flags(TileFlags::Invalidated) {
                        stats.tiles_drawn_invalidated += 1;
                        INVALID_TILE_COLOR
                    } else {
                        Color::white()
                    };

                let tex_coords = &tile_sprite.coords;
                let texture = tile_sprite.texture;
                let color = tile.tint_color() * highlight_color;

                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color);
                stats.tiles_drawn += 1;
            }
        }

        debug::utils::draw_tile_debug(
            render_sys,
            ui_sys,
            tile_iso_pos,
            tile_screen_rect,
            transform,
            tile,
            flags);
    }

    #[inline]
    fn reset_stats(&mut self) {
        self.stats.tiles_drawn = 0;
        self.stats.tiles_drawn_highlighted = 0;
        self.stats.tiles_drawn_invalidated = 0;
        self.stats.tile_sort_list_len = 0;
    }

    #[inline]
    fn update_stats(&mut self) -> TileMapRenderStats {
        self.stats.peak_tiles_drawn             = self.stats.tiles_drawn.max(self.stats.peak_tiles_drawn);
        self.stats.peak_tiles_drawn_highlighted = self.stats.tiles_drawn_highlighted.max(self.stats.peak_tiles_drawn_highlighted);
        self.stats.peak_tiles_drawn_invalidated = self.stats.tiles_drawn_invalidated.max(self.stats.peak_tiles_drawn_invalidated);
        self.stats.peak_tile_sort_list_len      = self.stats.tile_sort_list_len.max(self.stats.peak_tile_sort_list_len);
        self.stats.clone()
    }
}

// ----------------------------------------------
// TileDrawListEntry
// ----------------------------------------------

struct TileDrawListEntry {
    // NOTE: Raw pointer, no lifetime.
    // This is only a temporary reference that lives
    // for the scope of TileMapRenderer::draw_map().
    // Since we store this in a temp vector that is a member
    // of TileMapRenderer we need to bypass the borrow checker.
    // Not ideal but avoids having to allocate a new temporary
    // local Vec each time draw_map() is called.
    tile_ptr: *const Tile<'static>,

    // Y value of the bottom left corner of the tile sprite for sorting.
    // Simulates a pseudo depth value so we can render units and buildings
    // correctly.
    z_sort: i32,
}
