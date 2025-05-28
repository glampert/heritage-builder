use bitflags::bitflags;
use smallvec::SmallVec;

use crate::{
    ui::UiSystem,
    render::RenderSystem,
    utils::{self, Color, Cell2D, IsoPoint2D, Point2D, Size2D, Rect2D, WorldToScreenTransform}
};

use super::{
    debug::{self},
    selection::{self},
    def::{self, BASE_TILE_SIZE},
    map::{Tile, TileFlags, TileMapLayerKind, TileMap}
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const TILE_HIGHLIGHT_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const TILE_INVALID_COLOR:   Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red

pub const GRID_HIGHLIGHT_COLOR: Color = Color::green();
pub const GRID_INVALID_COLOR:   Color = Color::red();

pub const SELECTION_RECT_COLOR: Color = Color::new(0.2, 0.7, 0.2, 1.0); // green-ish
pub const MAP_BACKGROUND_COLOR: Color = Color::gray();

// ----------------------------------------------
// TileMapRenderFlags / TileDrawListEntry
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
    transform: WorldToScreenTransform,
    grid_color: Color,
    grid_line_thickness: f32,
    stats: TileMapRenderStats,
    temp_tile_sort_list: Vec<TileDrawListEntry>, // For z-sorting.
}

impl TileMapRenderer {
    pub fn new() -> Self {
        Self {
            transform: WorldToScreenTransform::default(),
            grid_color: Color::white(),
            grid_line_thickness: 1.0,
            stats: TileMapRenderStats::default(),
            temp_tile_sort_list: Vec::with_capacity(512),
        }
    }

    pub fn set_draw_scaling(&mut self, scaling: i32) -> &mut Self {
        debug_assert!(scaling > 0);
        self.transform.scaling = scaling;
        self
    }

    pub fn set_draw_offset(&mut self, offset: Point2D) -> &mut Self {
        self.transform.offset = offset;
        self
    }

    pub fn set_tile_spacing(&mut self, spacing: i32) -> &mut Self {
        debug_assert!(spacing >= 0);
        self.transform.tile_spacing = spacing;
        self
    }

    pub fn set_grid_color(&mut self, color: Color) -> &mut Self {
        self.grid_color = color;
        self
    }

    pub fn set_grid_line_thickness(&mut self, thickness: f32) -> &mut Self {
        debug_assert!(thickness > 0.0);
        self.grid_line_thickness = thickness;
        self
    }

    pub fn world_to_screen_transform(&self) -> WorldToScreenTransform {
        self.transform
    }

    pub fn draw_map(&mut self,
                    render_sys: &mut RenderSystem,
                    ui_sys: &UiSystem,
                    tile_map: &TileMap,
                    flags: TileMapRenderFlags) -> TileMapRenderStats {

        debug_assert!(self.temp_tile_sort_list.is_empty());

        self.reset_stats();

        let map_cells = tile_map.size();

        let (cell_min, cell_max) =
            self.calc_visible_cells_range(render_sys.window_size(), map_cells);

        // Terrain:
        if flags.contains(TileMapRenderFlags::DrawTerrain) {
            let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
            let buildings_layer = tile_map.layer(TileMapLayerKind::Buildings);

            debug_assert!(terrain_layer.size() == map_cells);
            debug_assert!(buildings_layer.size() == map_cells);

            for y in (cell_min.y..=cell_max.y).rev() {
                for x in (cell_min.x..=cell_max.x).rev() {
                    let cell = Cell2D::new(x, y);

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

                    let tile_iso_coords = tile.calc_adjusted_iso_coords();
                    Self::draw_tile(render_sys,
                                    &mut self.stats,
                                    ui_sys,
                                    tile_iso_coords,
                                    &self.transform,
                                    tile,
                                    flags);
                }
            }
        }

        if flags.contains(TileMapRenderFlags::DrawGrid) &&
          !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map, cell_min, cell_max);
        }

        // Buildings & Units:
        if flags.intersects(TileMapRenderFlags::DrawBuildings | TileMapRenderFlags::DrawUnits) {
            let buildings_layer = tile_map.layer(TileMapLayerKind::Buildings);
            let units_layer = tile_map.layer(TileMapLayerKind::Units);

            debug_assert!(buildings_layer.size() == map_cells);
            debug_assert!(units_layer.size() == map_cells);

            let mut add_to_sort_list = |tile: &Tile| {
                self.temp_tile_sort_list.push(TileDrawListEntry {
                    tile_ptr: tile as *const Tile<'_> as *const Tile<'static>,
                    z_sort: tile.calc_z_sort(),
                });
            };

            // Drawing in reverse order (bottom to top) is required to ensure
            // buildings with the same Z-sort value don't overlap in weird ways.
            for y in (cell_min.y..=cell_max.y).rev() {
                for x in (cell_min.x..=cell_max.x).rev() {

                    let cell = Cell2D::new(x, y);
                    let building_tile = buildings_layer.tile(cell);
                    let unit_tile = units_layer.tile(cell);

                    if building_tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildings) {
                        add_to_sort_list(building_tile);
                    } else if unit_tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnits) {
                        add_to_sort_list(unit_tile);
                    } else if building_tile.is_blocker() && // DEBUG:
                              (flags.contains(TileMapRenderFlags::DrawBlockerTilesDebug) ||
                               building_tile.flags.contains(TileFlags::DrawBlockerInfo)) {

                        let tile_iso_coords = building_tile.calc_adjusted_iso_coords();
                        Self::draw_tile(render_sys,
                                        &mut self.stats,
                                        ui_sys,
                                        tile_iso_coords,
                                        &self.transform,
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

                let tile_iso_coords = tile.calc_adjusted_iso_coords();
                Self::draw_tile(render_sys,
                                &mut self.stats,
                                ui_sys,
                                tile_iso_coords,
                                &self.transform,
                                tile,
                                flags);
            }

            self.stats.tile_sort_list_len += self.temp_tile_sort_list.len() as u32;
            self.temp_tile_sort_list.clear();
        }

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map, cell_min, cell_max);
        }

        self.update_stats();
        self.stats.clone()
    }

    fn draw_isometric_grid(&self,
                           render_sys: &mut RenderSystem,
                           tile_map: &TileMap,
                           cell_min: Cell2D,
                           cell_max: Cell2D) {
    
        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * (self.transform.scaling as f32);

        let mut highlighted_cells = SmallVec::<[[Point2D; 4]; 128]>::new();
        let mut invalidated_cells = SmallVec::<[[Point2D; 4]; 128]>::new();

        for y in cell_min.y..=cell_max.y {
            for x in cell_min.x..=cell_max.x {
                let cell = Cell2D::new(x, y);
                let points = def::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE, &self.transform);

                // Save highlighted grid cells for drawing at the end, so they display in the right order.
                let tile = terrain_layer.tile(cell);
    
                if tile.flags.contains(TileFlags::Highlighted) {
                    highlighted_cells.push(points);
                    continue;
                }

                if tile.flags.contains(TileFlags::Invalidated) {
                    invalidated_cells.push(points);
                    continue;
                }

                // Draw diamond:
                render_sys.draw_polyline_with_thickness(&points, self.grid_color, line_thickness, true);
            }

            // Highlighted on top:
            for points in &highlighted_cells {
                render_sys.draw_polyline_with_thickness(&points, GRID_HIGHLIGHT_COLOR, line_thickness, true);
            }

            for points in &invalidated_cells {
                render_sys.draw_polyline_with_thickness(&points, GRID_INVALID_COLOR, line_thickness, true);
            }
        }
    }

    fn draw_tile(render_sys: &mut RenderSystem,
                 stats: &mut TileMapRenderStats,
                 ui_sys: &UiSystem,
                 tile_iso_coords: IsoPoint2D,
                 transform: &WorldToScreenTransform,
                 tile: &Tile,
                 flags: TileMapRenderFlags) {

        debug_assert!(tile.is_valid() && !tile.is_empty());

        // Only terrain and buildings might require spacing.
        let apply_spacing = if !tile.is_unit() { true } else { false };

        let tile_rect = utils::iso_to_screen_rect(
            tile_iso_coords,
            tile.draw_size(),
            transform,
            apply_spacing);

        if !tile.flags.contains(TileFlags::Hidden) {
            let highlight_color =
                if tile.flags.contains(TileFlags::Highlighted) {
                    stats.tiles_drawn_highlighted += 1;
                    TILE_HIGHLIGHT_COLOR
                } else if tile.flags.contains(TileFlags::Invalidated) {
                    stats.tiles_drawn_invalidated += 1;
                    TILE_INVALID_COLOR
                } else {
                    Color::white()
                };

            if let Some(anim_frame) = tile.def.anim_frame_by_index(0, 0, 0) {
                render_sys.draw_textured_colored_rect(
                    tile_rect,
                    &anim_frame.tex_info.coords,
                    anim_frame.tex_info.texture,
                    tile.def.color * highlight_color);

                stats.tiles_drawn += 1;
            }
        }

        debug::draw_tile_debug(
            render_sys,
            ui_sys,
            tile_iso_coords,
            tile_rect,
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
    fn update_stats(&mut self) {
        self.stats.peak_tiles_drawn             = self.stats.tiles_drawn.max(self.stats.peak_tiles_drawn);
        self.stats.peak_tiles_drawn_highlighted = self.stats.tiles_drawn_highlighted.max(self.stats.peak_tiles_drawn_highlighted);
        self.stats.peak_tiles_drawn_invalidated = self.stats.tiles_drawn_invalidated.max(self.stats.peak_tiles_drawn_invalidated);
        self.stats.peak_tile_sort_list_len      = self.stats.tile_sort_list_len.max(self.stats.peak_tile_sort_list_len);
    }

    #[inline]
    fn calc_visible_cells_range(&self, screen_size: Size2D, map_size: Size2D) -> (Cell2D, Cell2D) {
        let screen_rect = Rect2D::new(
            Point2D::zero(),
            screen_size);

        selection::tile_selection_bounds(
            &screen_rect,
            BASE_TILE_SIZE,
            map_size,
            &self.transform)
    }
}
