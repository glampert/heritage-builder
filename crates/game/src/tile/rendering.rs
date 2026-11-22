#![allow(clippy::too_many_arguments)]

use arrayvec::ArrayVec;
use bitflags::bitflags;
use common::{
    Color,
    Rect,
    RectTexCoords,
    Vec2,
    constants::*,
    coords::{self, CellRange, IsoPointF32, WorldToScreenTransform},
    hash::StrHashPair,
    mem::RawPtr,
};
use engine::{
    render::{RenderSystem, debug::DebugDraw},
    ui::{self, UiSystem},
};
use smallvec::SmallVec;

use super::{
    Tile,
    TileDepthSortOverride,
    TileFlags,
    TileKind,
    TileMap,
    TileMapLayerKind,
    elevation::{self, Dir, ElevationMap, ElevationProblem, LEVEL_STEP_PX, Shape},
    height_pieces::{self, Course, FaceEdge, RampSurface},
    road,
    sets::{OBJECTS_OVERLAYS_CATEGORY, TileDef, TileSets, TileTexInfo},
    water,
};
use crate::{
    building,
    debug,
    pathfind::{Node, NodeKind as PathNodeKind},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const HIGHLIGHT_TILE_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const INVALID_TILE_COLOR: Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red
pub const SELECTION_RECT_COLOR: Color = Color::new(0.7, 0.2, 0.2, 1.0); // red-ish

pub const DEFAULT_GRID_COLOR: Color = Color::white();
pub const HIGHLIGHT_GRID_COLOR: Color = Color::green();
pub const INVALID_GRID_COLOR: Color = Color::red();

// The heights overlay: a translucent tint per level, from 1 up (7 levels).
const ELEVATION_LEVEL_COLORS: [Color; elevation::MAX_LEVEL as usize] = [
    Color::new(0.55, 0.85, 0.35, 0.35),
    Color::new(0.95, 0.90, 0.30, 0.40),
    Color::new(0.98, 0.65, 0.25, 0.45),
    Color::new(0.95, 0.40, 0.25, 0.50),
    Color::new(0.85, 0.25, 0.45, 0.50),
    Color::new(0.60, 0.30, 0.80, 0.50),
    Color::new(0.35, 0.40, 0.90, 0.55),
];
const RAMP_ARROW_COLOR: Color = Color::new(1.0, 1.0, 1.0, 0.9);
const TALL_CLIFF_COLOR: Color = Color::new(1.0, 0.2, 0.9, 1.0);

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

        // The terrain's heights as an overlay (docs/plans/terrain-height.md, §8): each raised
        // cell tinted by its level, an arrow up each ramp, problems outlined.
        const DrawElevation           = 1 << 16;
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
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range, false);
        } else if flags.contains(TileMapRenderFlags::DrawSearchGraphDebug) {
            // Base terrain search graph debug pass draws with respect to scene depth.
            self.draw_search_graph_debug_grid(
                render_sys,
                ui_sys,
                tile_map,
                transform,
                visible_range,
                TileMapLayerKind::Terrain,
            );
        }

        self.draw_objects_layer(render_sys, debug_draw, ui_sys, tile_map, transform, visible_range, flags);

        // Over everything: raised ground is drawn in the objects pass.
        if flags.contains(TileMapRenderFlags::DrawElevation) {
            Self::draw_elevation_overlay(render_sys, tile_map, transform, visible_range);
        }

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow grid lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map, transform, visible_range, true);
        } else if flags.contains(TileMapRenderFlags::DrawSearchGraphDebug) {
            // Search graph debug objects pass draws on top of everything else.
            self.draw_search_graph_debug_grid(
                render_sys,
                ui_sys,
                tile_map,
                transform,
                visible_range,
                TileMapLayerKind::Objects,
            );
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
        let raised = raised_terrain(tile_map);

        for cell in visible_range.iter_rev() {
            // Raised ground is drawn in the sorted objects pass; ground a raised top in front of it
            // covers whole isn't drawn at all (docs/plans/terrain-height.md, §3).
            if let Some(map) = raised
                && (map.is_raised(cell) || is_ground_hidden(map, cell))
            {
                continue;
            }
            if let Some(tile) = terrain.try_tile(cell) {
                // Terrain tiles size is constrained. Sanity check it:
                debug_assert!(tile.is(TileKind::Terrain) && tile.logical_size() == BASE_TILE_SIZE_I32);

                // As an optimization, skip drawing terrain tile if fully occluded by any object.
                if cull_occluded_terrain
                    && let Some(object) = objects.try_tile(cell)
                    && object.has_flags(TileFlags::OccludesTerrain)
                {
                    continue;
                }

                Self::draw_tile(render_sys, debug_draw, &mut self.stats, ui_sys, transform, tile, tile_map, flags, 0.0);
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
        let raised = raised_terrain(tile_map);
        let draw_raised_terrain = raised.is_some() && flags.contains(TileMapRenderFlags::DrawTerrain);
        if !flags.intersects(TileMapRenderFlags::DrawAllObjects) && !draw_raised_terrain {
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
                !tile.is(TileKind::Blocker)
                    && (tile.is(TileKind::Building | TileKind::Field) && flags.contains(TileMapRenderFlags::DrawBuildings))
                    || (tile.is(TileKind::Unit) && flags.contains(TileMapRenderFlags::DrawUnits))
                    || (tile.is(TileKind::Rocks) && flags.contains(TileMapRenderFlags::DrawProps))
                    || (tile.is(TileKind::Vegetation) && flags.contains(TileMapRenderFlags::DrawVegetation))
            }

            if should_draw(tile, flags) {
                self.temp_tile_sort_list.push(TileDrawListEntry::new(tile, transform, raised.is_some()));

                // Push stacked chained tiles into the list so they will sort.
                tile_map.visit_next_tiles(tile, |next_tile| {
                    if should_draw(next_tile, flags) {
                        self.temp_tile_sort_list.push(TileDrawListEntry::new(next_tile, transform, raised.is_some()));
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
                let lift = raised.map_or(0.0, |map| map.centre_height(cell) * LEVEL_STEP_PX * transform.scaling);
                let tile_screen_rect = coords::iso_to_screen_rect(tile_iso_pos, BASE_TILE_SIZE_I32, transform)
                    .translated(Vec2::new(0.0, -lift));
                debug::utils::draw_tile_debug(debug_draw, ui_sys, tile_screen_rect, transform, tile, flags);
            }
            should_draw
        };

        // Drawing in reverse order (bottom to top) is required to ensure
        // buildings with the same Z-sort value don't overlap in weird ways.
        for cell in visible_range.iter_rev() {
            if let Some(tile) = objects.try_tile(cell)
                && !debug_draw_blocker_tile(cell, tile)
            {
                try_add_to_sort_list(tile);
            }
        }

        // Raised ground sorts with the objects, by its own cell's diamond (§3). The ground under a
        // building takes the building's key, so it's drawn just before it (ties put ground first):
        // a footprint's front cells' keys are in front of the building's own.
        if let Some(map) = raised
            && draw_raised_terrain
        {
            for cell in visible_range.iter_rev() {
                if !map.is_raised(cell) {
                    continue;
                }
                let Some(tile) = terrain.try_tile(cell) else {
                    continue;
                };
                let key = match objects.try_tile(cell) {
                    Some(object) if object.is(TileKind::Building | TileKind::Field | TileKind::Blocker) => {
                        coords::cell_to_screen_diamond_center_y(object.base_cell(), object.logical_size(), transform)
                    }
                    _ => coords::cell_to_screen_diamond_center_y(cell, BASE_TILE_SIZE_I32, transform),
                };
                let tile_ptr = RawPtr::from_ref(tile);
                if has_courses(map, cell) {
                    self.temp_tile_sort_list.push(TileDrawListEntry {
                        tile: tile_ptr,
                        depth_sort_key: key,
                        part: DrawPart::Courses,
                    });
                }
                self.temp_tile_sort_list.push(TileDrawListEntry {
                    tile: tile_ptr,
                    depth_sort_key: key,
                    part: DrawPart::Top,
                });
                if has_rims(map, cell) {
                    self.temp_tile_sort_list.push(TileDrawListEntry {
                        tile: tile_ptr,
                        depth_sort_key: key,
                        part: DrawPart::Rims,
                    });
                }
            }
        }

        self.temp_tile_sort_list.sort_by(TileDrawListEntry::compare);

        let occluded = |cell| {
            flags.intersects(TileMapRenderFlags::CullOccludedTerrainTiles)
                && objects.try_tile(cell).is_some_and(|object| object.has_flags(TileFlags::OccludesTerrain))
        };

        for entry in &self.temp_tile_sort_list {
            let tile = entry.tile();

            if entry.part != DrawPart::Object {
                let map = raised.expect("raised ground in the sort list");
                Self::draw_raised_part(
                    render_sys,
                    debug_draw,
                    &mut self.stats,
                    ui_sys,
                    transform,
                    tile,
                    tile_map,
                    map,
                    flags,
                    occluded(tile.base_cell()),
                    (self.grid_color, self.grid_line_thickness),
                    entry.part,
                );
                continue;
            }

            debug_assert!(tile.is(TileKind::Object));
            let lift = raised.map_or(0.0, |map| object_lift_px(map, tile) * transform.scaling);
            Self::draw_tile(render_sys, debug_draw, &mut self.stats, ui_sys, transform, tile, tile_map, flags, lift);
        }

        self.stats.tile_sort_list_len += self.temp_tile_sort_list.len() as u32;
        self.temp_tile_sort_list.clear();
    }

    // A part of a raised cell (docs/plans/terrain-height.md, §3 and §5): its cliff courses on its two
    // front edges, from the ground in front up to its level (the top one with its terrain's lip); its
    // terrain lifted to its level, or its ramp half; or a rim along each side whose ground is lower.
    fn draw_raised_part(
        render_sys: &mut RenderSystem,
        debug_draw: &mut DebugDraw,
        stats: &mut TileMapRenderStats,
        ui_sys: &UiSystem,
        transform: WorldToScreenTransform,
        tile: &Tile,
        tile_map: &TileMap,
        map: &ElevationMap,
        flags: TileMapRenderFlags,
        occluded: bool,
        (grid_color, grid_thickness): (Color, f32),
        part: DrawPart,
    ) {
        let cell = tile.base_cell();
        let (level, shape) = map.get(cell);
        let tint = highlight_tint(tile);

        match part {
            DrawPart::Courses => {
                // A level each, from the lowest ground along the edge up to the cell's.
                for edge in FaceEdge::ALL {
                    let ground = lowest_ground_along(map, cell, edge.side());
                    for course_level in ground..level {
                        let course = if shape == Shape::Flat && course_level + 1 == level {
                            Course::Top { dirt_lip: height_pieces::has_dirt_lip(tile.tile_def()) }
                        } else {
                            Course::Mid
                        };
                        if let Some((def, variation)) = height_pieces::cliff_course(edge, course, cell, course_level) {
                            draw_piece(render_sys, transform, def, variation, cell, f32::from(course_level), tint);
                        }
                    }
                }
            }
            DrawPart::Top => match shape {
                Shape::Flat => {
                    if !occluded {
                        let lift = f32::from(level) * LEVEL_STEP_PX * transform.scaling;
                        Self::draw_tile(render_sys, debug_draw, stats, ui_sys, transform, tile, tile_map, flags, lift);
                    }
                    // The depth-sorted grid's line for this cell, over its top.
                    if flags.contains(TileMapRenderFlags::DrawGrid)
                        && !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth)
                    {
                        let points = ground_diamond(Some(map), cell, transform);
                        render_sys.draw_polyline_with_thickness(
                            &points,
                            grid_color,
                            grid_thickness * transform.scaling,
                            true,
                        );
                    }
                }
                Shape::Ramp(dir, half) => {
                    let surface = RampSurface::of_terrain(tile.tile_def());
                    if let Some((def, variation)) = height_pieces::ramp(surface, dir, half) {
                        draw_piece(render_sys, transform, def, variation, cell, f32::from(level), tint);
                        stats.tiles_drawn += 1;
                    }
                }
            },
            DrawPart::Rims => {
                for side in Dir::ALL {
                    if highest_ground_along(map, cell, side) < f32::from(level)
                        && let Some((def, variation)) = height_pieces::rim(side)
                    {
                        draw_piece(render_sys, transform, def, variation, cell, f32::from(level), tint);
                    }
                }
            }
            DrawPart::Object => unreachable!("objects are drawn by draw_tile"),
        }
    }

    fn draw_isometric_grid(
        &self,
        render_sys: &mut RenderSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        over_everything: bool,
    ) {
        let viewport = render_sys.viewport();

        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * transform.scaling;

        let mut highlighted_cells = SmallVec::<[[Vec2; 4]; 64]>::new();
        let mut invalidated_cells = SmallVec::<[[Vec2; 4]; 64]>::new();
        let raised = raised_terrain(tile_map);

        for cell in &visible_range {
            // Raised cells draw their own line after their top (draw_raised_cell), so raised ground
            // drawn in the objects pass doesn't cover it; the grid drawn over everything draws all.
            if !over_everything && raised.is_some_and(|map| map.is_raised(cell)) {
                continue;
            }
            let points = ground_diamond(raised, cell, transform);
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
                    let path_kind = graph.node_kind(Node::new(cell)).unwrap_or(PathNodeKind::empty());

                    if path_kind.intersects(PathNodeKind::Water) {
                        let points = ground_diamond(raised_terrain(tile_map), cell, transform);
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
                    ui::text(
                        ui_sys,
                        common::format_small!(
                            "Settlers Spawn Point: {}",
                            graph.settlers_spawn_point().unwrap_or(Node::invalid())
                        ),
                    );
                });
            }
            TileMapLayerKind::Objects => {
                let line_thickness = 5.0 * transform.scaling;
                const OPACITY: f32 = 0.7;

                let mut nodes = Vec::with_capacity((visible_range.width() * visible_range.height()) as usize);

                for cell in &visible_range {
                    let path_kind = graph.node_kind(Node::new(cell)).unwrap_or(PathNodeKind::empty());

                    if !path_kind.intersects(PathNodeKind::EmptyLand | PathNodeKind::Water)
                        || path_kind.intersects(PathNodeKind::BuildingAccess | PathNodeKind::SettlersSpawnPoint)
                    {
                        nodes.push((cell, path_kind));
                    }
                }

                nodes.sort_by_key(|node| std::cmp::Reverse(node.1));

                let raised = raised_terrain(tile_map);
                for (cell, path_kind) in nodes {
                    let points = ground_diamond(raised, cell, transform);
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

    // The heights overlay: a tint per level on each raised cell, an arrow up each ramp, and an
    // outline on each cell with a problem (red: a broken ramp or a bad byte; magenta: a cliff taller
    // than elevation::MAX_CLIFF_LEVELS). Drawn on the flat diamonds until raised terrain is drawn.
    fn draw_elevation_overlay(
        render_sys: &mut RenderSystem,
        tile_map: &TileMap,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
    ) {
        let viewport = render_sys.viewport();
        let map = tile_map.elevation();
        let line_thickness = 2.0 * transform.scaling;

        for cell in &visible_range {
            let byte = map.byte(cell);
            let problem = if byte == 0 { None } else { map.problem_at(cell) };
            if byte == 0 && problem.is_none() {
                continue;
            }
            let points = ground_diamond(Some(map), cell, transform);
            if Self::is_fully_offscreen(&viewport, &points) {
                continue;
            }

            let (level, shape) = map.get(cell);
            if level > 0 {
                let color = ELEVATION_LEVEL_COLORS[usize::from(level - 1).min(ELEVATION_LEVEL_COLORS.len() - 1)];
                render_sys.draw_colored_indexed_triangles(&points, &[0, 1, 2, 2, 3, 0], color);
            }

            if let Shape::Ramp(dir, _) = shape {
                // From the middle of the downhill edge to the middle of the uphill one.
                let (dx, dy) = dir.offset();
                let (x, y) = (cell.x as f32, cell.y as f32);
                let at = |fx: f32, fy: f32| {
                    let height = map.surface_height(coords::CellF32(Vec2::new(x + fx, y + fy)));
                    elevation::ground_point(x + fx, y + fy, height, transform)
                };
                let centre = at(0.0, 0.0);
                let towards = at(dx as f32 * 0.4, dy as f32 * 0.4);
                let tail = centre - (towards - centre);
                render_sys.draw_line_with_thickness(tail, towards, RAMP_ARROW_COLOR, line_thickness);
                let back = (centre - towards) * 0.45;
                let side = Vec2::new(-back.y, back.x) * 0.6;
                render_sys.draw_line_with_thickness(towards, towards + back + side, RAMP_ARROW_COLOR, line_thickness);
                render_sys.draw_line_with_thickness(towards, towards + back - side, RAMP_ARROW_COLOR, line_thickness);
            }

            if let Some(problem) = problem {
                let color = match problem {
                    ElevationProblem::TallCliff { .. } => TALL_CLIFF_COLOR,
                    _ => INVALID_GRID_COLOR,
                };
                render_sys.draw_polyline_with_thickness(&points, color, line_thickness * 1.5, true);
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
        lift: f32, // How far up it's drawn, in screen px: the height of the ground it stands on.
    ) {
        debug_assert!(tile.is_valid());
        debug_assert!(!tile.is(TileKind::Blocker));

        let up = Vec2::new(0.0, -lift);
        let tile_screen_rect = tile.screen_rect(transform, true).translated(up);

        if !tile.has_flags(TileFlags::Hidden)
            && let Some(tile_sprite) = tile.anim_frame_tex_info()
        {
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
                    render_sys.draw_textured_colored_rect(
                        rect.translated(up),
                        &scaffold.coords,
                        scaffold.texture,
                        highlight_color,
                    );
                } else {
                    // No scaffold art for this size: a faint outline of the whole building, and
                    // the part built so far rising from the ground through it.
                    render_sys.draw_textured_colored_rect(
                        tile_screen_rect,
                        tex_coords,
                        texture,
                        color * CONSTRUCTION_OUTLINE_COLOR,
                    );
                    let (rect, coords) = construction_site_rect(tile_screen_rect, tex_coords, stage);
                    render_sys.draw_textured_colored_rect(rect, &coords, texture, color * CONSTRUCTION_SITE_COLOR);
                }
            } else if tile.has_flags(TileFlags::Burning) {
                // On fire: darkened, with flames over each of its cells.
                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * BURNING_COLOR);
                draw_cell_overlay(
                    render_sys,
                    tile,
                    transform,
                    tile_map.overlay_anim_secs(),
                    highlight_color,
                    &FIRE_TILE,
                    FIRE_FRAME_SECS,
                    up,
                );
            } else if tile.has_flags(TileFlags::Diseased) {
                // A sick house: a sickly tint, with a yellow cloth and incense smoke over each of its cells.
                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * DISEASED_COLOR);
                draw_cell_overlay(
                    render_sys,
                    tile,
                    transform,
                    tile_map.overlay_anim_secs(),
                    highlight_color,
                    &SICKNESS_TILE,
                    SICKNESS_FRAME_SECS,
                    up,
                );
            } else if tile.has_flags(TileFlags::Renovating) {
                // A house being renovated before an upgrade: lightly tinted, with scaffolding over it.
                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * RENOVATION_COLOR);
                if let Some((rect, overlay)) = construction_overlay(tile, &RENOVATION_TILES, 0, transform) {
                    render_sys.draw_textured_colored_rect(
                        rect.translated(up),
                        &overlay.coords,
                        overlay.texture,
                        highlight_color,
                    );
                }
            } else if tile.has_flags(TileFlags::Flooded) && tile.is(TileKind::Terrain) {
                // A cell under the river's flood (crate::floods): silty water in place of its land or
                // its shore, drawn as terrain is, so it joins its neighbours' without seams and the
                // cells in front cover its sides.
                for (rect, sprite, overlay_color) in flood_overlays(tile, transform) {
                    render_sys.draw_textured_colored_rect(
                        rect.translated(up),
                        &sprite.coords,
                        sprite.texture,
                        overlay_color * highlight_color,
                    );
                }
            } else if tile.has_flags(TileFlags::DryCanal) && tile.is(TileKind::Terrain) {
                // A canal no water reaches (crate::farms::irrigation): dulled, the bare dirt washed
                // over it.
                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color * DRY_CANAL_COLOR);
                if let Some((rect, dirt)) = terrain_sprite(tile, LAND_CATEGORY, SILT_TILE, transform) {
                    render_sys.draw_textured_colored_rect(
                        rect.translated(up),
                        &dirt.coords,
                        dirt.texture,
                        DRY_CANAL_WASH * highlight_color,
                    );
                }
            } else if tile.has_flags(TileFlags::Festive)
                && let Some((rect, festive)) = festive_overlay(tile, transform)
            {
                // A festival square during a festival: dressed for it, in its place.
                render_sys.draw_textured_colored_rect(rect.translated(up), &festive.coords, festive.texture, color);
            } else {
                // Standard render:
                render_sys.draw_textured_colored_rect(tile_screen_rect, tex_coords, texture, color);
            }
            stats.tiles_drawn += 1;

            // Road placement overlay:
            if tile.has_flags(road::ROAD_PLACEMENT_FLAGS) {
                Self::draw_road_placement_overlay(render_sys, transform, tile, tile_map, up);
            }
        }

        debug::utils::draw_tile_debug(debug_draw, ui_sys, tile_screen_rect, transform, tile, flags);
    }

    fn draw_road_placement_overlay(
        render_sys: &mut RenderSystem,
        transform: WorldToScreenTransform,
        tile: &Tile,
        tile_map: &TileMap,
        up: Vec2,
    ) {
        let cell = tile.base_cell();

        let Some(kind) = road::placement_kind(tile.flags()) else {
            panic!("Expected one of the road placement flags");
        };
        let tile_def = road::tile_def(kind);

        // A canal's piece is the water's shore it will draw; a road's its junction.
        let variation_index = if kind == road::RoadKind::Canal {
            water::canal_preview_variation(tile_map, tile_def, cell)
        } else if tile_def.has_variations() {
            road::junction_mask_for(tile_map, cell, kind)
        } else {
            0
        };

        if let Some(anim_set) = tile_def.anim_set_by_index(variation_index, 0) {
            let tile_sprite = &anim_set.frames[0].tex_info;
            let tex_coords = &tile_sprite.coords;
            let texture = tile_sprite.texture;

            let iso_position = IsoPointF32::from_integer_iso(coords::cell_to_iso(cell));
            let tile_screen_rect =
                coords::iso_to_screen_rect_f32(iso_position, tile_def.draw_size, transform).translated(up);

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
        self.stats.peak_tiles_drawn = self.stats.tiles_drawn.max(self.stats.peak_tiles_drawn);
        self.stats.peak_tiles_drawn_highlighted =
            self.stats.tiles_drawn_highlighted.max(self.stats.peak_tiles_drawn_highlighted);
        self.stats.peak_tiles_drawn_invalidated =
            self.stats.tiles_drawn_invalidated.max(self.stats.peak_tiles_drawn_invalidated);
        self.stats.peak_tile_sort_list_len = self.stats.tile_sort_list_len.max(self.stats.peak_tile_sort_list_len);
    }
}

// ----------------------------------------------
// TileDrawListEntry
// ----------------------------------------------

const TILE_DEPTH_SORT_KEY_TOPMOST: f32 = f32::MAX; // Tile draws on top of everything else.
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

    // Which part of a raised cell a terrain entry draws, or Object for anything else: ties on the
    // key draw a row's courses, then its tops, then its rims, then what stands there. Cells in a row
    // don't overlap, so each part's sprites stay together, a texture switch a part a row rather than
    // two a raised cell (the courses and rims are in the objects atlas, the tops in the terrain's).
    part: DrawPart,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum DrawPart {
    Courses,
    Top,
    Rims,
    Object,
}

impl TileDrawListEntry {
    #[inline]
    fn new(tile: &Tile, transform: WorldToScreenTransform, raised_terrain: bool) -> Self {
        let depth_sort_key = {
            // User defined override?
            match tile.depth_sort_override() {
                TileDepthSortOverride::None => {
                    // On raised ground a walker sorts by the cell it's nearest, not the one it
                    // started its step from: otherwise the raised ground it steps onto, drawn after
                    // it, would cover its feet (docs/plans/terrain-height.md, §3).
                    let cell = if raised_terrain && tile.is(TileKind::Unit) {
                        let at = unit_position(tile);
                        coords::Cell::new(at.0.x.round() as i32, at.0.y.round() as i32)
                    } else {
                        tile.base_cell()
                    };
                    // Compute from tile screen space diamond.
                    coords::cell_to_screen_diamond_center_y(cell, tile.logical_size(), transform)
                }
                TileDepthSortOverride::Topmost => TILE_DEPTH_SORT_KEY_TOPMOST,
                TileDepthSortOverride::Bottommost => TILE_DEPTH_SORT_KEY_BOTTOMMOST,
            }
        };

        Self { tile: RawPtr::from_ref(tile), depth_sort_key, part: DrawPart::Object }
    }

    #[inline]
    fn compare(a: &TileDrawListEntry, b: &TileDrawListEntry) -> std::cmp::Ordering {
        let a_tile = a.tile();
        let b_tile = b.tile();

        a.depth_sort_key
            .partial_cmp(&b.depth_sort_key)
            .unwrap()
            // Raised ground's parts, in order, before what stands on it.
            .then_with(|| a.part.cmp(&b.part))
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
// Terrain height
// ----------------------------------------------

// The map's heights when it has raised ground, else None: a flat map draws exactly as before.
fn raised_terrain(tile_map: &TileMap) -> Option<&ElevationMap> {
    let map = tile_map.elevation();
    (!map.is_flat()).then_some(map)
}

// A cell's ground diamond on screen at its height (tile::elevation::ground_diamond).
#[inline]
fn ground_diamond(raised: Option<&ElevationMap>, cell: coords::Cell, transform: WorldToScreenTransform) -> [Vec2; 4] {
    elevation::ground_diamond(raised, cell, transform)
}

// The neighbour's ground along `cell`'s `side`, at the two corners they share: past the map's
// edge, the ground at 0.
fn ground_along(map: &ElevationMap, cell: coords::Cell, side: Dir) -> [f32; 2] {
    let neighbour = side.neighbour(cell);
    if !map.is_cell_within_bounds(neighbour) {
        return [0.0, 0.0];
    }
    map.side_heights(neighbour, side.opposite())
}

// The lowest ground beside `side`, in whole levels: where a cliff on that side starts.
fn lowest_ground_along(map: &ElevationMap, cell: coords::Cell, side: Dir) -> u8 {
    let [a, b] = ground_along(map, cell, side);
    a.min(b).floor().max(0.0) as u8
}

// Whether a raised cell has a cliff course on either front edge.
fn has_courses(map: &ElevationMap, cell: coords::Cell) -> bool {
    let level = map.level(cell);
    FaceEdge::ALL.into_iter().any(|edge| lowest_ground_along(map, cell, edge.side()) < level)
}

// Whether a flat raised cell has a rim on any side.
fn has_rims(map: &ElevationMap, cell: coords::Cell) -> bool {
    let (level, shape) = map.get(cell);
    shape == Shape::Flat && Dir::ALL.into_iter().any(|side| highest_ground_along(map, cell, side) < f32::from(level))
}

// The highest ground beside `side`: a raised top above it has a rim on that side.
fn highest_ground_along(map: &ElevationMap, cell: coords::Cell, side: Dir) -> f32 {
    let [a, b] = ground_along(map, cell, side);
    a.max(b)
}

// Whether a level-0 cell's ground is covered whole by the top of a raised flat cell in front of it:
// a cell at level `k` draws its top over the cell `k` steps behind it along the screen's vertical.
fn is_ground_hidden(map: &ElevationMap, cell: coords::Cell) -> bool {
    (1..=map.max_height()).any(|k| {
        let front = coords::Cell::new(cell.x - i32::from(k), cell.y - i32::from(k));
        map.get(front) == (k, Shape::Flat)
    })
}

// Where a walker stands in continuous cell space (its cell's centre at whole numbers), from the
// position its navigation set (tile::calc_unit_iso_coords, blended between cells).
fn unit_position(tile: &Tile) -> coords::CellF32 {
    let draw_size = tile.draw_size();
    let mut iso = tile.iso_coords_f32();
    iso.0.x -= (HALF_BASE_TILE_WIDTH_I32 - (draw_size.width / 2)) as f32;
    iso.0.y += (draw_size.height - HALF_BASE_TILE_HEIGHT_I32) as f32;
    coords::iso_to_cell_f32(iso)
}

// How far up an object is drawn, in logical px: a walker at the ground's height where it stands (so
// it climbs a ramp smoothly), anything else at its base cell's ground.
fn object_lift_px(map: &ElevationMap, tile: &Tile) -> f32 {
    if tile.is(TileKind::Unit) {
        map.surface_height(unit_position(tile)) * LEVEL_STEP_PX
    } else {
        map.centre_height(tile.base_cell()) * LEVEL_STEP_PX
    }
}

// The tint a tile is drawn with while the cursor's selection marks it.
fn highlight_tint(tile: &Tile) -> Color {
    if tile.has_flags(TileFlags::Highlighted) {
        HIGHLIGHT_TILE_COLOR
    } else if tile.has_flags(TileFlags::Invalidated) {
        INVALID_TILE_COLOR
    } else {
        Color::white()
    }
}

// A piece of raised terrain (tile::height_pieces) on `cell`, anchored as a 1 × 1 building and
// lifted `levels` up.
fn draw_piece(
    render_sys: &mut RenderSystem,
    transform: WorldToScreenTransform,
    def: &'static TileDef,
    variation: usize,
    cell: coords::Cell,
    levels: f32,
    tint: Color,
) {
    let Some(sprite) = def.anim_set_by_index(variation, 0).and_then(|set| set.frames.first()) else {
        return;
    };
    let iso = super::calc_object_iso_coords(TileKind::Building, cell, BASE_TILE_SIZE_I32, def.draw_size);
    let rect = coords::iso_to_screen_rect_f32(iso, def.draw_size, transform)
        .translated(Vec2::new(0.0, -levels * LEVEL_STEP_PX * transform.scaling));
    render_sys.draw_textured_colored_rect(rect, &sprite.tex_info.coords, sprite.tex_info.texture, def.color * tint);
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

// A festival square dressed for a festival (faith::festivals, TileFlags::Festive), from the tile
// generator, drawn in the square's place and anchored as it is.
const FESTIVE_TILE: StrHashPair = StrHashPair::from_str("festival_square_festive");

// The festive square's sprite and the rect to draw it in, for a tile of its footprint. None without
// the art, or for another footprint.
fn festive_overlay(tile: &Tile, transform: WorldToScreenTransform) -> Option<(Rect, &'static TileTexInfo)> {
    let def = TileSets::get().try_find_tile_def_by_hash(
        TileMapLayerKind::Objects,
        OBJECTS_OVERLAYS_CATEGORY.hash,
        FESTIVE_TILE.hash,
    )?;
    if def.logical_size != tile.logical_size() {
        return None;
    }
    let sprite = def.anim_set_by_index(0, 0)?.frames.first()?;
    let iso = super::calc_object_iso_coords(TileKind::Building, tile.base_cell(), tile.logical_size(), def.draw_size);
    Some((coords::iso_to_screen_rect_f32(iso, def.draw_size, transform), &sprite.tex_info))
}

// ----------------------------------------------
// Floods
// ----------------------------------------------

// The open water tile (the water tile def's first variation, `0000`), drawn in a flooded cell's
// place, and the bare dirt washed over it: the silt the flood carries.
const WATER_CATEGORY: StrHashPair = StrHashPair::from_str("water");
const WATER_TILE: StrHashPair = StrHashPair::from_str("water");
const LAND_CATEGORY: StrHashPair = StrHashPair::from_str("land");
const SILT_TILE: StrHashPair = StrHashPair::from_str("dirt");

// A dry canal, until it has art of its own: the water's shores dulled, under a wash of bare dirt.
const DRY_CANAL_COLOR: Color = Color::new(0.88, 0.84, 0.76, 1.0);
const DRY_CANAL_WASH: Color = Color::new(1.0, 0.9, 0.74, 0.62);

// A terrain tile def's first sprite, in a tile's place.
fn terrain_sprite(
    tile: &Tile,
    category: StrHashPair,
    def: StrHashPair,
    transform: WorldToScreenTransform,
) -> Option<(Rect, &'static TileTexInfo)> {
    let def = TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Terrain, category.hash, def.hash)?;
    let sprite = def.anim_set_by_index(0, 0)?.frames.first()?;
    Some((coords::iso_to_screen_rect_f32(tile.iso_coords_f32(), def.draw_size, transform), &sprite.tex_info))
}

const FLOOD_WATER_COLOR: Color = Color::new(1.0, 1.0, 1.0, 1.0);
const FLOOD_SILT_COLOR: Color = Color::new(1.0, 0.82, 0.55, 0.42);

// The water's sprite and the silt's for a terrain tile, each with its rect, anchored as terrain is
// (its top corner on the cell's), and its colour. Empty without the art.
fn flood_overlays(tile: &Tile, transform: WorldToScreenTransform) -> ArrayVec<(Rect, &'static TileTexInfo, Color), 2> {
    let mut overlays = ArrayVec::new();
    for (category, def, color) in
        [(WATER_CATEGORY, WATER_TILE, FLOOD_WATER_COLOR), (LAND_CATEGORY, SILT_TILE, FLOOD_SILT_COLOR)]
    {
        let Some(def) = TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Terrain, category.hash, def.hash) else {
            continue;
        };
        let Some(sprite) = def.anim_set_by_index(0, 0).and_then(|set| set.frames.first()) else {
            continue;
        };
        overlays.push((
            coords::iso_to_screen_rect_f32(tile.iso_coords_f32(), def.draw_size, transform),
            &sprite.tex_info,
            color,
        ));
    }
    overlays
}

// ----------------------------------------------
// Fires and sickness
// ----------------------------------------------

// The flames drawn over each cell of a burning building (building::risks), from the tile
// generator, looping. Anchored like a 1x1 building on the cell.
const FIRE_TILE: StrHashPair = StrHashPair::from_str("fire_1x1");

// How long each frame of the flames shows, unless the tile def's anim set gives a duration.
const FIRE_FRAME_SECS: f32 = 0.15;

// A burning building, darkened under its flames.
const BURNING_COLOR: Color = Color::new(0.5, 0.42, 0.38, 1.0);

// The cloth and smoke drawn over each cell of a sick house, looping more slowly than flames.
const SICKNESS_TILE: StrHashPair = StrHashPair::from_str("sickness_1x1");
const SICKNESS_FRAME_SECS: f32 = 0.3;

// A sick house, faintly yellow-green.
const DISEASED_COLOR: Color = Color::new(0.86, 0.93, 0.7, 1.0);

// An animated 1x1 overlay (`overlay`, from the objects tile set's overlays category) over each
// cell of `tile`, each cell at its own frame.
fn draw_cell_overlay(
    render_sys: &mut RenderSystem,
    tile: &Tile,
    transform: WorldToScreenTransform,
    anim_secs: f32,
    color: Color,
    overlay: &StrHashPair,
    default_frame_secs: f32,
    up: Vec2,
) {
    let Some(def) =
        TileSets::get().try_find_tile_def_by_hash(TileMapLayerKind::Objects, OBJECTS_OVERLAYS_CATEGORY.hash, overlay.hash)
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

    let frame_secs = if anim_set.frame_duration_secs() > 0.0 { anim_set.frame_duration_secs() } else { default_frame_secs };
    let tick = (anim_secs / frame_secs) as usize;

    let mut flames = SmallVec::<[(Rect, &'static TileTexInfo); 25]>::new();
    for cell in &tile.cell_range() {
        // Each cell starts at its own frame, so a big fire doesn't flicker in step.
        let phase = (cell.x * 3 + cell.y * 5).rem_euclid(frames.len() as i32) as usize;
        let sprite = &frames[(tick + phase) % frames.len()];
        let iso = super::calc_object_iso_coords(TileKind::Building, cell, def.logical_size, def.draw_size);
        flames.push((coords::iso_to_screen_rect_f32(iso, def.draw_size, transform).translated(up), &sprite.tex_info));
    }

    // Back to front: the pieces lower on screen stand in front.
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

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use common::Size;

    use super::*;
    use crate::tile::elevation::{Half, encode};

    fn plateau() -> ElevationMap {
        // Level 1 over x 2..=4, y 2..=4 on an 8 × 8 map; a ramp up its south side, climbing north.
        let mut map = ElevationMap::new(Size::new(8, 8));
        for y in 2..=4 {
            for x in 2..=4 {
                map.set_byte(coords::Cell::new(x, y), encode(1, Shape::Flat));
            }
        }
        map.set_byte(coords::Cell::new(0, 3), encode(0, Shape::Ramp(Dir::N, Half::Low)));
        map.set_byte(coords::Cell::new(1, 3), encode(0, Shape::Ramp(Dir::N, Half::High)));
        map
    }

    #[test]
    fn test_cliffs_and_rims_follow_the_ground_around() {
        let map = plateau();
        let c = coords::Cell::new;

        // The plateau's front corner: courses on both front edges, rims on its south and east sides.
        assert_eq!(lowest_ground_along(&map, c(2, 2), Dir::S), 0);
        assert_eq!(lowest_ground_along(&map, c(2, 2), Dir::E), 0);
        assert!(has_courses(&map, c(2, 2)) && has_rims(&map, c(2, 2)));
        // Its middle: neither.
        assert!(!has_courses(&map, c(3, 3)) && !has_rims(&map, c(3, 3)));
        // Its back corner: no courses (its back edges face away), but rims on its north and west sides.
        assert!(!has_courses(&map, c(4, 4)));
        assert!(has_rims(&map, c(4, 4)));
        assert!(highest_ground_along(&map, c(4, 4), Dir::N) < 1.0);
        assert!(highest_ground_along(&map, c(4, 4), Dir::W) < 1.0);

        // Where the ramp's head meets the plateau: the ground along that side is the ramp's top, a
        // level up, so no course and no rim there.
        assert_eq!(highest_ground_along(&map, c(2, 3), Dir::S), 1.0);
        assert_eq!(lowest_ground_along(&map, c(2, 3), Dir::S), 1);
        // Beside the ramp's flank (the plateau's south edge at y 2): the ground at 0.
        assert_eq!(lowest_ground_along(&map, c(2, 2), Dir::S), 0);

        // Past the map's edge the ground is at 0.
        let mut edge = ElevationMap::new(Size::new(2, 2));
        edge.set_byte(c(0, 0), encode(2, Shape::Flat));
        assert_eq!(lowest_ground_along(&edge, c(0, 0), Dir::S), 0);
    }

    #[test]
    fn test_ground_hidden_behind_a_raised_top() {
        let map = plateau();
        let c = coords::Cell::new;
        // (2, 2) at level 1 draws its top over (3, 3)'s flat diamond: that's raised too. The cell
        // behind the plateau's back corner, (5, 5), is covered by (4, 4)'s top.
        assert!(is_ground_hidden(&map, c(5, 5)));
        // Ground beside the plateau isn't.
        assert!(!is_ground_hidden(&map, c(6, 1)));
        // A ramp's top doesn't cover the cell behind it whole.
        assert!(!is_ground_hidden(&map, c(1, 4)));
    }
}
