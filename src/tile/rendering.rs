use bitflags::bitflags;
use smallvec::SmallVec;

use crate::{
    ui::UiSystem,
    render::RenderSystem,
    utils::{self, Color, Cell2D, Point2D, IsoPoint2D, WorldToScreenTransform}
};

use super::{
    debug::{self},
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

// ----------------------------------------------
// TileMapRenderFlags / TileDrawListEntry
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct TileMapRenderFlags: u32 {
        const None                = 0;
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
        const DrawDebugBuildingBlockers  = 1 << 10;
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
// TileMapRenderer
// ----------------------------------------------

pub struct TileMapRenderer {
    world_to_screen: WorldToScreenTransform,
    grid_color: Color,
    grid_line_thickness: f32,
    temp_tile_sort_list: Vec<TileDrawListEntry>, // For z-sorting.
}

impl TileMapRenderer {
    pub fn new() -> Self {
        Self {
            world_to_screen: WorldToScreenTransform::default(),
            grid_color: Color::white(),
            grid_line_thickness: 1.0,
            temp_tile_sort_list: Vec::with_capacity(512),
        }
    }

    pub fn set_draw_scaling(&mut self, scaling: i32) -> &mut Self {
        debug_assert!(scaling > 0);
        self.world_to_screen.scaling = scaling;
        self
    }

    pub fn set_draw_offset(&mut self, offset: Point2D) -> &mut Self {
        self.world_to_screen.offset = offset;
        self
    }

    pub fn set_tile_spacing(&mut self, spacing: i32) -> &mut Self {
        debug_assert!(spacing >= 0);
        self.world_to_screen.tile_spacing = spacing;
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
        self.world_to_screen
    }

    pub fn draw_map(&mut self,
                    render_sys: &mut RenderSystem,
                    ui_sys: &UiSystem,
                    tile_map: &TileMap,
                    flags: TileMapRenderFlags) {

        debug_assert!(self.temp_tile_sort_list.is_empty());
        let map_cells = tile_map.size();

        // Terrain:
        if flags.contains(TileMapRenderFlags::DrawTerrain) {
            let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
            debug_assert!(terrain_layer.size() == map_cells);

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let tile = terrain_layer.tile(Cell2D::new(x, y));
                    if tile.is_empty() {
                        continue;
                    }

                    // Terrain tiles size is constrained.
                    debug_assert!(tile.is_terrain() && tile.logical_size() == BASE_TILE_SIZE);

                    let tile_iso_coords = tile.calc_adjusted_iso_coords();
                    self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile, flags);
                }
            }
        }

        if flags.contains(TileMapRenderFlags::DrawGrid) &&
          !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map);
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

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let cell = Cell2D::new(x, y);
                    let building_tile = buildings_layer.tile(cell);
                    let unit_tile = units_layer.tile(cell);

                    if building_tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildings) {
                        add_to_sort_list(building_tile);
                    } else if unit_tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnits) {
                        add_to_sort_list(unit_tile);
                    } else if building_tile.is_building_blocker() && // DEBUG:
                              flags.contains(TileMapRenderFlags::DrawDebugBuildingBlockers) {

                        let tile_iso_coords = building_tile.calc_adjusted_iso_coords();
                        let tile_rect = utils::iso_to_screen_rect(
                            tile_iso_coords,
                            building_tile.draw_size(),
                            &self.world_to_screen,
                            true);

                        debug::draw_tile_debug(
                            render_sys,
                            ui_sys,
                            tile_iso_coords,
                            tile_rect,
                            &self.world_to_screen,
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
                self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile, flags);
            }

            self.temp_tile_sort_list.clear();
        }

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map);
        }
    }

    fn draw_isometric_grid(&self,
                           render_sys: &mut RenderSystem,
                           tile_map: &TileMap) {
    
        let map_cells = tile_map.size();
        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * (self.world_to_screen.scaling as f32);

        let mut highlighted_cells = SmallVec::<[[Point2D; 4]; 128]>::new();
        let mut invalidated_cells = SmallVec::<[[Point2D; 4]; 128]>::new();

        for y in (0..map_cells.height).rev() {
            for x in (0..map_cells.width).rev() {
                let cell = Cell2D::new(x, y);
                let points = def::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE, &self.world_to_screen);

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

    fn draw_tile(&self,
                 render_sys: &mut RenderSystem,
                 ui_sys: &UiSystem,
                 tile_iso_coords: IsoPoint2D,
                 tile: &Tile,
                 flags: TileMapRenderFlags) {

        debug_assert!(tile.is_valid() && !tile.is_empty());

        // Only terrain and buildings might require spacing.
        let apply_spacing = if !tile.is_unit() { true } else { false };

        let tile_rect = utils::iso_to_screen_rect(
            tile_iso_coords,
            tile.draw_size(),
            &self.world_to_screen,
            apply_spacing);

        if !tile.flags.contains(TileFlags::Hidden) {
            let highlight_color =
                if tile.flags.contains(TileFlags::Highlighted) {
                    TILE_HIGHLIGHT_COLOR
                } else if tile.flags.contains(TileFlags::Invalidated) {
                    TILE_INVALID_COLOR
                } else {
                    Color::white()
                };

            render_sys.draw_textured_colored_rect(
                tile_rect,
                &tile.def.tex_info.coords,
                tile.def.tex_info.texture,
                tile.def.color * highlight_color);
        }

        debug::draw_tile_debug(
            render_sys,
            ui_sys,
            tile_iso_coords,
            tile_rect,
            &self.world_to_screen,
            tile,
            flags);
    }
}
