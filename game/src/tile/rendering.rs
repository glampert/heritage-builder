use bitflags::bitflags;
use smallvec::SmallVec;

use super::{road, Tile, TileFlags, TileKind, TileMap, TileMapLayerKind, BASE_TILE_SIZE};
use crate::{
    debug::{self},
    imgui_ui::UiSystem,
    render::RenderSystem,
    utils::{
        coords::{self, CellRange, WorldToScreenTransform},
        mem, Color, Vec2,
    },
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const HIGHLIGHT_TILE_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const INVALID_TILE_COLOR: Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red

pub const SELECTION_RECT_COLOR: Color = Color::new(0.2, 0.7, 0.2, 1.0); // green-ish
pub const MAP_BACKGROUND_COLOR: Color = Color::gray();

pub const DEFAULT_GRID_COLOR: Color = Color::white();
pub const HIGHLIGHT_GRID_COLOR: Color = Color::green();
pub const INVALID_GRID_COLOR: Color = Color::red();

pub const MIN_GRID_LINE_THICKNESS: f32 = 0.5;
pub const MAX_GRID_LINE_THICKNESS: f32 = 20.0;

// ----------------------------------------------
// TileMapRenderFlags
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct TileMapRenderFlags: u32 {
        const DrawTerrain    = 1 << 0;
        const DrawBuildings  = 1 << 1;
        const DrawProps      = 1 << 2;
        const DrawUnits      = 1 << 3;
        const DrawVegetation = 1 << 4;
        const DrawAllObjects = Self::DrawBuildings.bits()
                             | Self::DrawProps.bits()
                             | Self::DrawUnits.bits()
                             | Self::DrawVegetation.bits();

        // Grid rendering:
        const DrawGrid            = 1 << 5; // Grid draws on top of terrain but under objects (buildings/units).
        const DrawGridIgnoreDepth = 1 << 6; // Grid draws on top of everything ignoring z-sort order.

        // If this flag is set, terrain tiles under objects
        // with TileFlags::OccludesTerrain are not rendered.
        const CullOccludedTerrainTiles = 1 << 7;

        // Debug flags:
        const DrawDebugBounds         = 1 << 8;
        const DrawTerrainTileDebug    = 1 << 9;
        const DrawBuildingsTileDebug  = 1 << 10;
        const DrawPropsTileDebug      = 1 << 11;
        const DrawUnitsTileDebug      = 1 << 12;
        const DrawVegetationTileDebug = 1 << 13;
        const DrawBlockersTileDebug   = 1 << 14;
    }
}

// ----------------------------------------------
// TileMapRenderStats
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
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
        Self { grid_color,
               grid_line_thickness: grid_line_thickness.clamp(MIN_GRID_LINE_THICKNESS,
                                                              MAX_GRID_LINE_THICKNESS),
               stats: TileMapRenderStats::default(),
               temp_tile_sort_list: Vec::with_capacity(512) }
    }

    pub fn set_grid_color(&mut self, color: Color) {
        self.grid_color = color;
    }

    pub fn grid_color(&self) -> Color {
        self.grid_color
    }

    pub fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.grid_line_thickness =
            thickness.clamp(MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS);
    }

    pub fn grid_line_thickness(&self) -> f32 {
        self.grid_line_thickness
    }

    pub fn draw_map(&mut self,
                    render_sys: &mut impl RenderSystem,
                    ui_sys: &UiSystem,
                    tile_map: &TileMap,
                    transform: WorldToScreenTransform,
                    visible_range: CellRange,
                    flags: TileMapRenderFlags)
                    -> TileMapRenderStats {
        self.reset_stats();

        self.draw_terrain_layer(render_sys, ui_sys, tile_map, transform, visible_range, flags);

        if flags.contains(TileMapRenderFlags::DrawGrid)
           && !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth)
        {
            // Draw the grid now so that lines will be on top of the terrain but not on top
            // of buildings.
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range);
        }

        self.draw_objects_layer(render_sys, ui_sys, tile_map, transform, visible_range, flags);

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
                          transform: WorldToScreenTransform,
                          visible_range: CellRange,
                          flags: TileMapRenderFlags) {
        if !flags.contains(TileMapRenderFlags::DrawTerrain) {
            return;
        }

        let layers = tile_map.layers();
        let terrain = layers.get(TileMapLayerKind::Terrain);
        let objects = layers.get(TileMapLayerKind::Objects);

        debug_assert!(terrain.size_in_cells() == tile_map.size_in_cells());
        debug_assert!(objects.size_in_cells() == tile_map.size_in_cells());

        let cull_occluded_terrain = flags.intersects(TileMapRenderFlags::CullOccludedTerrainTiles);

        for cell in visible_range.iter_rev() {
            if let Some(tile) = terrain.try_tile(cell) {
                // Terrain tiles size is constrained. Sanity check it:
                debug_assert!(tile.is(TileKind::Terrain) && tile.logical_size() == BASE_TILE_SIZE);

                // As an optimization, skip drawing terrain tile
                // if fully occluded by any object.
                if cull_occluded_terrain {
                    if let Some(object) = objects.try_tile(cell) {
                        if object.has_flags(TileFlags::OccludesTerrain) {
                            continue;
                        }
                    }
                }

                Self::draw_tile(render_sys,
                                &mut self.stats,
                                ui_sys,
                                transform,
                                tile,
                                tile_map,
                                flags);
            }
        }
    }

    fn draw_objects_layer(&mut self,
                          render_sys: &mut impl RenderSystem,
                          ui_sys: &UiSystem,
                          tile_map: &TileMap,
                          transform: WorldToScreenTransform,
                          visible_range: CellRange,
                          flags: TileMapRenderFlags) {
        if !flags.intersects(TileMapRenderFlags::DrawAllObjects) {
            return;
        }

        let layers = tile_map.layers();
        let terrain = layers.get(TileMapLayerKind::Terrain);
        let objects = layers.get(TileMapLayerKind::Objects);

        debug_assert!(terrain.size_in_cells() == tile_map.size_in_cells());
        debug_assert!(objects.size_in_cells() == tile_map.size_in_cells());
        debug_assert!(self.temp_tile_sort_list.is_empty());

        let mut try_add_to_sort_list = |tile: &Tile| {
            let should_draw = {
                !tile.is(TileKind::Blocker)    &&
                (tile.is(TileKind::Building)   && flags.contains(TileMapRenderFlags::DrawBuildings)) ||
                (tile.is(TileKind::Prop)       && flags.contains(TileMapRenderFlags::DrawProps))     ||
                (tile.is(TileKind::Unit)       && flags.contains(TileMapRenderFlags::DrawUnits))     ||
                (tile.is(TileKind::Vegetation) && flags.contains(TileMapRenderFlags::DrawVegetation))
            };

            if should_draw {
                self.temp_tile_sort_list.push(TileDrawListEntry::new(tile));
            }
        };

        let mut debug_draw_blocker_tile = |cell, tile: &Tile| -> bool {
            let should_draw = {
                tile.is(TileKind::Blocker)
                && (tile.has_flags(TileFlags::DrawBlockerInfo)
                    || flags.contains(TileMapRenderFlags::DrawBlockersTileDebug))
            };

            if should_draw {
                // Debug display for blocker tiles:
                let tile_iso_pos = coords::cell_to_iso(cell, BASE_TILE_SIZE);
                let tile_screen_rect = coords::iso_to_screen_rect(tile_iso_pos, BASE_TILE_SIZE, transform);
                debug::utils::draw_tile_debug(render_sys,
                                              ui_sys,
                                              tile_screen_rect,
                                              transform,
                                              tile,
                                              flags);
            }
            should_draw
        };

        // Drawing in reverse order (bottom to top) is required to ensure
        // buildings with the same Z-sort value don't overlap in weird ways.
        for cell in visible_range.iter_rev() {
            if let Some(tile) = objects.try_tile(cell) {
                if !debug_draw_blocker_tile(cell, tile) {
                    try_add_to_sort_list(tile);
                }
            }
        }

        self.temp_tile_sort_list.sort_by(|a, b| a.z_sort.cmp(&b.z_sort));

        for entry in &self.temp_tile_sort_list {
            let tile = entry.tile_ref();
            debug_assert!(tile.is(TileKind::Object));

            Self::draw_tile(render_sys,
                            &mut self.stats,
                            ui_sys,
                            transform,
                            tile,
                            tile_map,
                            flags);

            // Draw stacked chained tiles.
            tile_map.visit_next_tiles(tile, |next_tile| {
                Self::draw_tile(render_sys,
                                &mut self.stats,
                                ui_sys,
                                transform,
                                next_tile,
                                tile_map,
                                flags);
            });
        }

        self.stats.tile_sort_list_len += self.temp_tile_sort_list.len() as u32;
        self.temp_tile_sort_list.clear();
    }

    fn draw_isometric_grid(&self,
                           render_sys: &mut impl RenderSystem,
                           tile_map: &TileMap,
                           transform: WorldToScreenTransform,
                           visible_range: CellRange) {
        // Returns true only if all points are offscreen.
        let viewport = render_sys.viewport();
        let is_fully_offscreen = |points: &[Vec2; 4]| {
            let mut offscreen_count = 0;
            for pt in points {
                if pt.x < viewport.min.x
                   || pt.y < viewport.min.y
                   || pt.x > viewport.max.x
                   || pt.y > viewport.max.y
                {
                    offscreen_count += 1;
                }
            }
            offscreen_count == points.len()
        };

        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * transform.scaling;

        let mut highlighted_cells = SmallVec::<[[Vec2; 4]; 128]>::new();
        let mut invalidated_cells = SmallVec::<[[Vec2; 4]; 128]>::new();

        for cell in &visible_range {
            let points = coords::cell_to_screen_diamond_points(cell,
                                                               BASE_TILE_SIZE,
                                                               BASE_TILE_SIZE,
                                                               transform);
            if is_fully_offscreen(&points) {
                continue; // Cull if fully offscreen.
            }

            // Save highlighted grid cells for drawing at the end, so they display in the
            // correct order.
            if let Some(tile) = terrain_layer.try_tile(cell) {
                if tile.has_flags(TileFlags::Highlighted) {
                    highlighted_cells.push(points);
                    continue;
                }

                if tile.has_flags(TileFlags::Invalidated) {
                    invalidated_cells.push(points);
                    continue;
                }
            }

            // Draw diamond:
            render_sys.draw_polyline_with_thickness(&points, self.grid_color, line_thickness, true);
        }

        // Highlighted on top:
        for points in &highlighted_cells {
            render_sys.draw_polyline_with_thickness(points,
                                                    HIGHLIGHT_GRID_COLOR,
                                                    line_thickness,
                                                    true);
        }

        for points in &invalidated_cells {
            render_sys.draw_polyline_with_thickness(points,
                                                    INVALID_GRID_COLOR,
                                                    line_thickness,
                                                    true);
        }
    }

    fn draw_tile(render_sys: &mut impl RenderSystem,
                 stats: &mut TileMapRenderStats,
                 ui_sys: &UiSystem,
                 transform: WorldToScreenTransform,
                 tile: &Tile,
                 tile_map: &TileMap,
                 flags: TileMapRenderFlags) {
        debug_assert!(tile.is_valid());
        debug_assert!(!tile.is(TileKind::Blocker));

        let tile_screen_rect = tile.screen_rect(transform);

        if !tile.has_flags(TileFlags::Hidden) {
            if let Some(tile_sprite) = tile.anim_frame_tex_info() {
                let highlight_color = {
                    if tile.has_flags(TileFlags::Highlighted) {
                        stats.tiles_drawn_highlighted += 1;
                        HIGHLIGHT_TILE_COLOR
                    } else if tile.has_flags(TileFlags::Invalidated) {
                        stats.tiles_drawn_invalidated += 1;
                        INVALID_TILE_COLOR
                    } else {
                        Color::white()
                    }
                };

                // Standard render:
                let tex_coords = &tile_sprite.coords;
                let texture = tile_sprite.texture;
                let color = tile.tint_color() * highlight_color;

                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color);
                stats.tiles_drawn += 1;

                // Road placement overlay:
                if tile.has_flags(TileFlags::DirtRoadPlacement | TileFlags::PavedRoadPlacement) {
                    Self::draw_road_placement_overlay(render_sys, transform, tile, tile_map);
                }
            }
        }

        debug::utils::draw_tile_debug(render_sys, ui_sys, tile_screen_rect, transform, tile, flags);
    }

    fn draw_road_placement_overlay(render_sys: &mut impl RenderSystem,
                                   transform: WorldToScreenTransform,
                                   tile: &Tile,
                                   tile_map: &TileMap) {
        let cell = tile.base_cell();

        let tile_def = {
            if tile.has_flags(TileFlags::DirtRoadPlacement) {
                road::tile_def(road::RoadKind::Dirt)
            } else if tile.has_flags(TileFlags::PavedRoadPlacement) {
                road::tile_def(road::RoadKind::Paved)
            } else {
                panic!("Expected one of TileFlags::RoadPlacement flags");
            }
        };

        let variation_index = {
            if tile_def.has_variations() {
                road::junction_mask(cell, tile_map)
            } else {
                0
            }
        };

        if let Some(anim_set) = tile_def.anim_set_by_index(variation_index, 0) {
            let tile_sprite = &anim_set.frames[0].tex_info;
            let tex_coords = &tile_sprite.coords;
            let texture = tile_sprite.texture;

            let iso_position = coords::cell_to_iso(cell, tile_def.logical_size).to_vec2();
            let tile_screen_rect = coords::iso_to_screen_rect_f32(iso_position, tile_def.draw_size, transform);

            let mut color = tile_def.color;
            if tile.has_flags(TileFlags::Invalidated) {
                color *= INVALID_TILE_COLOR;
            }
            color.a *= 0.7;

            render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color);
        }
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
        self.stats
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
    tile: mem::RawPtr<Tile>,

    // Y value of the bottom left corner of the tile sprite for sorting.
    // Simulates a pseudo depth value so we can render units and buildings
    // correctly.
    z_sort: i32,
}

impl TileDrawListEntry {
    #[inline]
    fn new(tile: &Tile) -> Self {
        Self { tile: mem::RawPtr::from_ref(tile), z_sort: tile.z_sort_key() }
    }

    #[inline]
    fn tile_ref(&self) -> &Tile {
        // SAFETY: This reference only lives for the scope of draw_map().
        // The only reason we store it in a member Vec is to avoid the
        // memory allocation cost of a temp local Vec. `temp_tile_sort_list`
        // is always cleared at the end of the drawing pass.
        &self.tile
    }
}
