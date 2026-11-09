#![allow(clippy::too_many_arguments)]

use bitflags::bitflags;
use smallvec::SmallVec;

use common::{
    Color,
    Vec2,
    Rect,
    RectTexCoords,
    constants::*,
    mem::RawPtr,
    coords::{self, CellRange, IsoPointF32, WorldToScreenTransform},
    hash::StrHashPair,
};
use engine::{
    render::{RenderSystem, debug::DebugDraw},
    ui::{self, UiSystem},
};

use super::{
    Tile, TileDepthSortOverride, TileFlags, TileKind, TileMap, TileMapLayerKind, road,
    sets::{OBJECTS_OVERLAYS_CATEGORY, TileDef, TileSets, TileTexInfo},
};
use crate::{building, debug, pathfind::{Node, NodeKind as PathNodeKind}};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const HIGHLIGHT_TILE_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const INVALID_TILE_COLOR:   Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red
pub const SELECTION_RECT_COLOR: Color = Color::new(0.7,  0.2,  0.2,  1.0); // red-ish

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
        const DrawTerrain    = 1 << 0;
        const DrawBuildings  = 1 << 1;
        const DrawProps      = 1 << 2;
        const DrawUnits      = 1 << 3;
        const DrawVegetation = 1 << 4;

        const DrawAllObjects = Self::DrawBuildings.bits()
                             | Self::DrawProps.bits()
                             | Self::DrawUnits.bits()
                             | Self::DrawVegetation.bits();

        const DrawTerrainAndObjects = Self::DrawTerrain.bits()
                                    | Self::DrawAllObjects.bits();

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
        const DrawSearchGraphDebug    = 1 << 15;
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

impl Default for TileMapRenderer {
    fn default() -> Self {
        Self::new(DEFAULT_GRID_COLOR, 1.0)
    }
}

impl TileMapRenderer {
    pub fn new(grid_color: Color, grid_line_thickness: f32) -> Self {
        Self {
            grid_color,
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

    pub fn stats(&self) -> &TileMapRenderStats {
        &self.stats
    }

    pub fn draw_map(
        &mut self,
        render_sys: &mut RenderSystem,
        debug_draw: &mut DebugDraw,
        ui_sys: &UiSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        flags: TileMapRenderFlags,
    ) {
        self.reset_stats();

        self.draw_terrain_layer(render_sys, debug_draw, ui_sys, tile_map, transform, visible_range, flags);

        if flags.contains(TileMapRenderFlags::DrawGrid) && !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range);
        } else if flags.contains(TileMapRenderFlags::DrawSearchGraphDebug) {
            // Base terrain search graph debug pass draws with respect to scene depth.
            self.draw_search_graph_debug_grid(render_sys, ui_sys, tile_map, transform, visible_range, TileMapLayerKind::Terrain);
        }

        self.draw_objects_layer(render_sys, debug_draw, ui_sys, tile_map, transform, visible_range, flags);

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow grid lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range);
        } else if flags.contains(TileMapRenderFlags::DrawSearchGraphDebug) {
            // Search graph debug objects pass draws on top of everything else.
            self.draw_search_graph_debug_grid(render_sys, ui_sys, tile_map, transform, visible_range, TileMapLayerKind::Objects);
        }

        self.update_stats();
    }

    fn draw_terrain_layer(
        &mut self,
        render_sys: &mut RenderSystem,
        debug_draw: &mut DebugDraw,
        ui_sys: &UiSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        flags: TileMapRenderFlags,
    ) {
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
                debug_assert!(tile.is(TileKind::Terrain) && tile.logical_size() == BASE_TILE_SIZE_I32);

                // As an optimization, skip drawing terrain tile if fully occluded by any object.
                if cull_occluded_terrain && let Some(object) = objects.try_tile(cell) {
                    if object.has_flags(TileFlags::OccludesTerrain) {
                        continue;
                    }
                }

                Self::draw_tile(render_sys, debug_draw, &mut self.stats, ui_sys, transform, tile, tile_map, flags);
            }
        }
    }

    fn draw_objects_layer(
        &mut self,
        render_sys: &mut RenderSystem,
        debug_draw: &mut DebugDraw,
        ui_sys: &UiSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        flags: TileMapRenderFlags,
    ) {
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
            fn should_draw(tile: &Tile, flags: TileMapRenderFlags) -> bool {
                !tile.is(TileKind::Blocker)    &&
                (tile.is(TileKind::Building)   && flags.contains(TileMapRenderFlags::DrawBuildings)) ||
                (tile.is(TileKind::Unit)       && flags.contains(TileMapRenderFlags::DrawUnits))     ||
                (tile.is(TileKind::Rocks)      && flags.contains(TileMapRenderFlags::DrawProps))     ||
                (tile.is(TileKind::Vegetation) && flags.contains(TileMapRenderFlags::DrawVegetation))
            }

            if should_draw(tile, flags) {
                self.temp_tile_sort_list.push(TileDrawListEntry::new(tile, transform));

                // Push stacked chained tiles into the list so they will sort.
                tile_map.visit_next_tiles(tile, |next_tile| {
                    if should_draw(next_tile, flags) {
                        self.temp_tile_sort_list.push(TileDrawListEntry::new(next_tile, transform));
                    }
                });
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
                let tile_iso_pos = coords::cell_to_iso(cell);
                let tile_screen_rect = coords::iso_to_screen_rect(tile_iso_pos, BASE_TILE_SIZE_I32, transform);
                debug::utils::draw_tile_debug(debug_draw, ui_sys, tile_screen_rect, transform, tile, flags);
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

        self.temp_tile_sort_list.sort_by(TileDrawListEntry::compare);

        for entry in &self.temp_tile_sort_list {
            let tile = entry.tile();
            debug_assert!(tile.is(TileKind::Object));

            Self::draw_tile(render_sys, debug_draw, &mut self.stats, ui_sys, transform, tile, tile_map, flags);
        }

        self.stats.tile_sort_list_len += self.temp_tile_sort_list.len() as u32;
        self.temp_tile_sort_list.clear();
    }

    fn draw_isometric_grid(
        &self,
        render_sys: &mut RenderSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
    ) {
        let viewport = render_sys.viewport();

        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * transform.scaling;

        let mut highlighted_cells = SmallVec::<[[Vec2; 4]; 64]>::new();
        let mut invalidated_cells = SmallVec::<[[Vec2; 4]; 64]>::new();

        for cell in &visible_range {
            let points = coords::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE_I32, transform);
            if Self::is_fully_offscreen(&viewport, &points) {
                continue; // Cull if fully offscreen.
            }

            // Save highlighted grid cells for drawing at the end,
            // so they display in the correct order.
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
            render_sys.draw_polyline_with_thickness(points, HIGHLIGHT_GRID_COLOR, line_thickness, true);
        }

        for points in &invalidated_cells {
            render_sys.draw_polyline_with_thickness(points, INVALID_GRID_COLOR, line_thickness, true);
        }
    }

    fn draw_search_graph_debug_grid(
        &self,
        render_sys: &mut RenderSystem,
        ui_sys: &UiSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        layer_kind: TileMapLayerKind,
    ) {
        let graph = tile_map.graph();
        let viewport = render_sys.viewport();

        match layer_kind {
            TileMapLayerKind::Terrain => {
                let line_thickness = 2.0 * transform.scaling;
                const OPACITY: f32 = 0.7;

                for cell in &visible_range {
                    let path_kind = graph.node_kind(Node::new(cell))
                        .unwrap_or(PathNodeKind::empty());

                    if path_kind.intersects(PathNodeKind::Water) {
                        let points = coords::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE_I32, transform);
                        if Self::is_fully_offscreen(&viewport, &points) {
                            continue; // Cull if fully offscreen.
                        }

                        let mut color = path_kind.debug_color();
                        color.a = OPACITY;

                        render_sys.draw_polyline_with_thickness(&points, color, line_thickness, true);
                    }
                }
                ui::overlay(ui_sys, ui::UiOverlay::new("Search Graph", Vec2::new(10.0, 30.0)).bg_alpha(0.7), |ui_sys| {
                    ui::text(ui_sys, common::format_small!("Vacant Lots: {}", graph.vacant_lot_nodes_count()));
                    ui::text(ui_sys, common::format_small!("Settlers Spawn Point: {}", graph.settlers_spawn_point().unwrap_or(Node::invalid())));
                });
            }
            TileMapLayerKind::Objects => {
                let line_thickness = 5.0 * transform.scaling;
                const OPACITY: f32 = 0.7;

                let mut nodes = Vec::with_capacity((visible_range.width() * visible_range.height()) as usize);

                for cell in &visible_range {
                    let path_kind = graph.node_kind(Node::new(cell))
                        .unwrap_or(PathNodeKind::empty());

                    if !path_kind.intersects(PathNodeKind::EmptyLand | PathNodeKind::Water)
                        || path_kind.intersects(PathNodeKind::BuildingAccess | PathNodeKind::SettlersSpawnPoint)
                    {
                        nodes.push((cell, path_kind));
                    }
                }

                nodes.sort_by(|a, b| b.1.cmp(&a.1));

                for (cell, path_kind) in nodes {
                    let points = coords::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE_I32, transform);
                    if Self::is_fully_offscreen(&viewport, &points) {
                        continue; // Cull if fully offscreen.
                    }

                    let mut color = path_kind.debug_color();
                    color.a = OPACITY;

                    render_sys.draw_polyline_with_thickness(&points, color, line_thickness, true);
                }
            }
        }
    }

    // Returns true only if all points are offscreen.
    #[inline]
    fn is_fully_offscreen(viewport: &Rect, points: &[Vec2; 4]) -> bool {
        let mut offscreen_count = 0;
        for pt in points {
            if pt.x < viewport.min.x || pt.y < viewport.min.y || pt.x > viewport.max.x || pt.y > viewport.max.y {
                offscreen_count += 1;
            }
        }
        offscreen_count == points.len()
    }

    fn draw_tile(
        render_sys: &mut RenderSystem,
        debug_draw: &mut DebugDraw,
        stats: &mut TileMapRenderStats,
        ui_sys: &UiSystem,
        transform: WorldToScreenTransform,
        tile: &Tile,
        tile_map: &TileMap,
        flags: TileMapRenderFlags,
    ) {
        debug_assert!(tile.is_valid());
        debug_assert!(!tile.is(TileKind::Blocker));

        let tile_screen_rect = tile.screen_rect(transform, true);

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

                let tex_coords = &tile_sprite.coords;
                let texture = tile_sprite.texture;
                let color = tile.tint_color() * highlight_color;

                if let Some(stage) = building::construction_stage_from_tile_flags(tile.flags()) {
                    if let Some((rect, scaffold)) = construction_overlay(tile, &SCAFFOLD_TILES, usize::from(stage), transform) {
                        // Construction site: the scaffold for its footprint and stage, instead of the building.
                        render_sys.draw_textured_colored_rect(rect, &scaffold.coords, scaffold.texture, highlight_color);
                    } else {
                        // No scaffold art for this size: a faint outline of the whole building, and
                        // the part built so far rising from the ground through it.
                        render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * CONSTRUCTION_OUTLINE_COLOR);
                        let (rect, coords) = construction_site_rect(tile_screen_rect, tex_coords, stage);
                        render_sys.draw_textured_colored_rect(rect, &coords, texture, color * CONSTRUCTION_SITE_COLOR);
                    }
                } else if tile.has_flags(TileFlags::Burning) {
                    // On fire: darkened, with flames over each of its cells.
                    render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * BURNING_COLOR);
                    draw_fire_overlay(render_sys, tile, transform, tile_map.overlay_anim_secs(), highlight_color);
                } else if tile.has_flags(TileFlags::Renovating) {
                    // A house being renovated before an upgrade: lightly tinted, with scaffolding over it.
                    render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * RENOVATION_COLOR);
                    if let Some((rect, overlay)) = construction_overlay(tile, &RENOVATION_TILES, 0, transform) {
                        render_sys.draw_textured_colored_rect(rect, &overlay.coords, overlay.texture, highlight_color);
                    }
                } else {
                    // Standard render:
                    render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color);
                }
                stats.tiles_drawn += 1;

                // Road placement overlay:
                if tile.has_flags(TileFlags::DirtRoadPlacement | TileFlags::PavedRoadPlacement) {
                    Self::draw_road_placement_overlay(render_sys, transform, tile, tile_map);
                }
            }
        }

        debug::utils::draw_tile_debug(debug_draw, ui_sys, tile_screen_rect, transform, tile, flags);
    }

    fn draw_road_placement_overlay(
        render_sys: &mut RenderSystem,
        transform: WorldToScreenTransform,
        tile: &Tile,
        tile_map: &TileMap,
    ) {
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
                road::junction_mask(tile_map, cell)
            } else {
                0
            }
        };

        if let Some(anim_set) = tile_def.anim_set_by_index(variation_index, 0) {
            let tile_sprite = &anim_set.frames[0].tex_info;
            let tex_coords = &tile_sprite.coords;
            let texture = tile_sprite.texture;

            let iso_position = IsoPointF32::from_integer_iso(coords::cell_to_iso(cell));
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
    fn update_stats(&mut self) {
        self.stats.peak_tiles_drawn             = self.stats.tiles_drawn.max(self.stats.peak_tiles_drawn);
        self.stats.peak_tiles_drawn_highlighted = self.stats.tiles_drawn_highlighted.max(self.stats.peak_tiles_drawn_highlighted);
        self.stats.peak_tiles_drawn_invalidated = self.stats.tiles_drawn_invalidated.max(self.stats.peak_tiles_drawn_invalidated);
        self.stats.peak_tile_sort_list_len      = self.stats.tile_sort_list_len.max(self.stats.peak_tile_sort_list_len);
    }
}

// ----------------------------------------------
// TileDrawListEntry
// ----------------------------------------------

const TILE_DEPTH_SORT_KEY_TOPMOST:    f32 = f32::MAX; // Tile draws on top of everything else.
const TILE_DEPTH_SORT_KEY_BOTTOMMOST: f32 = f32::MIN; // Tile draws below everything else.

struct TileDrawListEntry {
    // NOTE: Raw pointer, no lifetime.
    // This is only a temporary reference that lives
    // for the scope of TileMapRenderer::draw_map().
    // Since we store this in a temp vector that is a member
    // of TileMapRenderer we need to bypass the borrow checker.
    // Not ideal but avoids having to allocate a new temporary
    // local Vec each time draw_map() is called.
    tile: RawPtr<Tile>,

    // Y value of the left or right corner of the tile iso diamond for sorting.
    // Simulates a pseudo depth value so we can render units and buildings correctly.
    // May be overridden by TileDepthSortOverride.
    depth_sort_key: f32,
}

impl TileDrawListEntry {
    #[inline]
    fn new(tile: &Tile, transform: WorldToScreenTransform) -> Self {
        let depth_sort_key = {
            // User defined override?
            match tile.depth_sort_override() {
                TileDepthSortOverride::None => {
                    // Compute from tile screen space diamond.
                    coords::cell_to_screen_diamond_center_y(tile.base_cell(), tile.logical_size(), transform)
                }
                TileDepthSortOverride::Topmost => TILE_DEPTH_SORT_KEY_TOPMOST,
                TileDepthSortOverride::Bottommost => TILE_DEPTH_SORT_KEY_BOTTOMMOST,
            }
        };

        Self { tile: RawPtr::from_ref(tile), depth_sort_key }
    }

    #[inline]
    fn compare(a: &TileDrawListEntry, b: &TileDrawListEntry) -> std::cmp::Ordering {
        let a_tile = a.tile();
        let b_tile = b.tile();

        a.depth_sort_key
            .partial_cmp(&b.depth_sort_key)
            .unwrap()
            .then_with(|| {
                // In case of tie, draw units/props in front of buildings.
                let a_prio = a_tile.is(TileKind::Unit | TileKind::Vegetation | TileKind::Rocks);
                let b_prio = b_tile.is(TileKind::Unit | TileKind::Vegetation | TileKind::Rocks);
                a_prio.cmp(&b_prio)
            })
            .then_with(|| a_tile.index().cmp(&b_tile.index())) // Fallback to tile pool index if all else equal.
    }

    #[inline(always)]
    fn tile(&self) -> &Tile {
        // SAFETY: This reference only lives for the scope of draw_map().
        // The only reason we store it in a member Vec is to avoid the
        // memory allocation cost of a temp local Vec. `temp_tile_sort_list`
        // is always cleared at the end of the drawing pass.
        &self.tile
    }
}

// ----------------------------------------------
// Construction sites
// ----------------------------------------------

// Construction art from the tile generator (overlays category), by footprint size in cells:
// scaffold_NxN has a frame per construction stage and is drawn instead of a site's building;
// renovation_NxN is drawn over a renovating house. Both are anchored like a building of that
// footprint, at their own draw size.
const SCAFFOLD_TILES: [StrHashPair; 5] = [
    StrHashPair::from_str("scaffold_1x1"),
    StrHashPair::from_str("scaffold_2x2"),
    StrHashPair::from_str("scaffold_3x3"),
    StrHashPair::from_str("scaffold_4x4"),
    StrHashPair::from_str("scaffold_5x5"),
];
const RENOVATION_TILES: [StrHashPair; 4] = [
    StrHashPair::from_str("renovation_1x1"),
    StrHashPair::from_str("renovation_2x2"),
    StrHashPair::from_str("renovation_3x3"),
    StrHashPair::from_str("renovation_4x4"),
];

fn overlay_tile_def(tiles: &[StrHashPair], footprint_cells: i32) -> Option<&'static TileDef> {
    let name = tiles.get(usize::try_from(footprint_cells).ok()?.checked_sub(1)?)?;
    TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Objects, OBJECTS_OVERLAYS_CATEGORY.hash, name.hash)
}

// The scaffold drawn instead of a construction site's building, for a square footprint this
// many cells wide, with a frame per construction stage. None when there's no art for that size.
pub fn scaffold_tile_def(footprint_cells: i32) -> Option<&'static TileDef> {
    overlay_tile_def(&SCAFFOLD_TILES, footprint_cells)
}

// The scaffolding drawn over a renovating house this many cells wide. None without art.
pub fn renovation_tile_def(footprint_cells: i32) -> Option<&'static TileDef> {
    overlay_tile_def(&RENOVATION_TILES, footprint_cells)
}

// The overlay from `tiles` for this tile's footprint, frame `frame` (clamped to the last), and
// the screen rect to draw it in. None when there's no art for the footprint.
fn construction_overlay(
    tile: &Tile,
    tiles: &[StrHashPair],
    frame: usize,
    transform: WorldToScreenTransform,
) -> Option<(Rect, &'static TileTexInfo)> {
    let def = overlay_tile_def(tiles, tile.logical_size().width / BASE_TILE_WIDTH_I32)?;
    let frames = &def.anim_set_by_index(0, 0)?.frames;
    let sprite = frames.get(frame.min(frames.len().checked_sub(1)?))?;
    let iso = super::calc_object_iso_coords(TileKind::Building, tile.base_cell(), tile.logical_size(), def.draw_size);
    Some((coords::iso_to_screen_rect_f32(iso, def.draw_size, transform), &sprite.tex_info))
}

// ----------------------------------------------
// Fires
// ----------------------------------------------

// The flames drawn over each cell of a burning building (building::risks), from the tile
// generator, looping. Anchored like a 1x1 building on the cell.
const FIRE_TILE: StrHashPair = StrHashPair::from_str("fire_1x1");

// How long each frame of the flames shows, unless the tile def's anim set gives a duration.
const FIRE_FRAME_SECS: f32 = 0.15;

// A burning building, darkened under its flames.
const BURNING_COLOR: Color = Color::new(0.5, 0.42, 0.38, 1.0);

fn draw_fire_overlay(
    render_sys: &mut RenderSystem,
    tile: &Tile,
    transform: WorldToScreenTransform,
    anim_secs: f32,
    color: Color,
) {
    let Some(def) = TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Objects, OBJECTS_OVERLAYS_CATEGORY.hash, FIRE_TILE.hash)
    else {
        return;
    };
    let Some(anim_set) = def.anim_set_by_index(0, 0) else {
        return;
    };
    let frames = &anim_set.frames;
    if frames.is_empty() {
        return;
    }

    let frame_secs = if anim_set.frame_duration_secs() > 0.0 { anim_set.frame_duration_secs() } else { FIRE_FRAME_SECS };
    let tick = (anim_secs / frame_secs) as usize;

    let mut flames = SmallVec::<[(Rect, &'static TileTexInfo); 25]>::new();
    for cell in &tile.cell_range() {
        // Each cell starts at its own frame, so a big fire doesn't flicker in step.
        let phase = (cell.x * 3 + cell.y * 5).rem_euclid(frames.len() as i32) as usize;
        let sprite = &frames[(tick + phase) % frames.len()];
        let iso = super::calc_object_iso_coords(TileKind::Building, cell, def.logical_size, def.draw_size);
        flames.push((coords::iso_to_screen_rect_f32(iso, def.draw_size, transform), &sprite.tex_info));
    }

    // Back to front: the flames lower on screen stand in front.
    flames.sort_by(|a, b| a.0.max.y.total_cmp(&b.0.max.y));
    for (rect, tex_info) in flames {
        render_sys.draw_textured_colored_rect(rect, &tex_info.coords, tex_info.texture, color);
    }
}

// Without scaffold art for its size, a construction site draws its building twice: a faint
// outline of all of it, and the part built so far in a sandy tint.
const CONSTRUCTION_OUTLINE_COLOR: Color = Color::new(1.0, 0.9, 0.75, 0.3);
const CONSTRUCTION_SITE_COLOR: Color = Color::new(1.0, 0.82, 0.6, 1.0);

// A house being renovated: a light sandy tint under its scaffolding overlay.
const RENOVATION_COLOR: Color = Color::new(1.0, 0.93, 0.82, 1.0);

// The part of a building's sprite a construction site at `stage` has built: its bottom
// (stage + 1) quarters, so it rises from the ground as the site progresses. Returns the screen
// rect and the texture coordinates for that part.
fn construction_site_rect(screen_rect: Rect, tex_coords: &RectTexCoords, stage: u8) -> (Rect, RectTexCoords) {
    let shown = (f32::from(stage) + 1.0) / f32::from(building::construction::CONSTRUCTION_STAGES);

    // A sprite's foot is at the rect's max.y, which the renderer pairs with tex coords 0 and 2
    // (Rect's and RectTexCoords' "top"); min.y goes with coords 1 and 3. Keep the part next to
    // the foot: move the min.y edge and coords 1 and 3 towards it.
    let mut rect = screen_rect;
    rect.min.y = screen_rect.max.y - (screen_rect.height() * shown);

    let mut coords = *tex_coords;
    for (moved, foot) in [(1, 0), (3, 2)] {
        let foot_v = tex_coords.coords[foot].y;
        let far_v = tex_coords.coords[moved].y;
        coords.coords[moved].y = foot_v + ((far_v - foot_v) * shown);
    }

    (rect, coords)
}

