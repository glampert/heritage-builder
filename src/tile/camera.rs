use std::time::{self};

use crate::{
    utils::{Vec2, Point2D, Size2D, Rect2D, WorldToScreenTransform}
};

use super::{
    def::BASE_TILE_SIZE,
    selection::{self, CellRange}
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const MIN_ZOOM: i32 = 1;
const MAX_ZOOM: i32 = 10;

const MIN_TILE_SPACING: i32 = 0;
const MAX_TILE_SPACING: i32 = 10;

const SCROLL_MARGIN: i32 = 20;    // pixels from edge
const SCROLL_SPEED:  f32 = 500.0; // pixels per second

// ----------------------------------------------
// Camera
// ----------------------------------------------

pub struct Camera {
    viewport_size: Size2D,
    map_size_in_cells: Size2D,
    transform: WorldToScreenTransform,
    offset_unscaled: Point2D,
}

impl Camera {
    pub fn new(viewport_size: Size2D,
               map_size_in_cells: Size2D,
               scaling: i32,
               offset: Point2D,
               tile_spacing: i32) -> Self {

        let clamped_scaling = scaling.clamp(MIN_ZOOM, MAX_ZOOM);
        let clamped_tile_spacing = tile_spacing.clamp(MIN_TILE_SPACING, MAX_TILE_SPACING);
        let clamped_offset = clamp_to_map_bounds(map_size_in_cells, clamped_scaling, clamped_tile_spacing, offset);

        Self {
            viewport_size: viewport_size,
            map_size_in_cells: map_size_in_cells,
            transform: WorldToScreenTransform::new(
                clamped_scaling,
                clamped_offset,
                clamped_tile_spacing
            ),
            offset_unscaled: clamped_offset,
        }
    }

    #[inline]
    pub fn visible_cells_range(&self) -> CellRange {
        calc_visible_cells_range(self.viewport_size, self.map_size_in_cells, &self.transform)
    }

    #[inline]
    pub fn transform(&self) -> &WorldToScreenTransform {
        &self.transform
    }

    #[inline]
    pub fn set_viewport_size(&mut self, new_size: Size2D) {
        self.viewport_size = new_size;
    }

    #[inline]
    pub fn viewport_size(&self) -> Size2D {
        self.viewport_size
    }

    #[inline]
    pub fn set_map_size_in_cells(&mut self, new_size: Size2D) {
        self.map_size_in_cells = new_size;
    }

    #[inline]
    pub fn map_size_in_cells(&self) -> Size2D {
        self.map_size_in_cells
    }

    // ----------------------
    // Tile spacing offsets:
    // ----------------------

    #[inline]
    pub fn tile_spacing_limits(&self) -> (i32, i32) {
        (MIN_TILE_SPACING, MAX_TILE_SPACING)
    }

    #[inline]
    pub fn current_tile_spacing(&self) -> i32 {
        self.transform.tile_spacing
    }

    #[inline]
    pub fn set_tile_spacing(&mut self, spacing: i32) {
        self.transform.tile_spacing = spacing.clamp(MIN_TILE_SPACING, MAX_TILE_SPACING);
    }

    // ----------------------
    // Zoom/scaling:
    // ----------------------

    #[inline]
    pub fn zoom_limits(&self) -> (i32, i32) {
        (MIN_ZOOM, MAX_ZOOM)
    }

    #[inline]
    pub fn current_zoom(&self) -> i32 {
        self.transform.scaling
    }

    #[inline]
    pub fn set_zoom(&mut self, zoom: i32) {
        self.transform.scaling = zoom.clamp(MIN_ZOOM, MAX_ZOOM);

        self.transform.offset.x = self.offset_unscaled.x * self.transform.scaling;
        self.transform.offset.y = self.offset_unscaled.y * self.transform.scaling;
    }

    #[inline]
    pub fn update_zooming(&mut self, amount: i32, _delta_time: time::Duration) {
        // TODO: Smooth zooming with mouse wheel (interpolation).
        // Need to first implement floating point zoom/scaling and rendering.
        self.set_zoom(self.transform.scaling + amount);
    }

    // ----------------------
    // Map X/Y scrolling:
    // ----------------------

    #[inline]
    pub fn scroll_limits(&self) -> (Point2D, Point2D) {
        calc_map_bounds(
            self.map_size_in_cells,
            self.transform.scaling,
            self.transform.tile_spacing)
    }

    #[inline]
    pub fn current_scroll(&self) -> Point2D {
        self.transform.offset
    }

    #[inline]
    pub fn set_scroll(&mut self, scroll: Point2D) {
        let clamped_scroll = clamp_to_map_bounds(
            self.map_size_in_cells,
            self.transform.scaling,
            self.transform.tile_spacing,
            scroll);

        self.transform.offset = clamped_scroll;

        self.offset_unscaled.x = self.transform.offset.x / self.transform.scaling;
        self.offset_unscaled.y = self.transform.offset.y / self.transform.scaling;
    }

    #[inline]
    pub fn update_scrolling(&mut self, cursor_screen_pos: Point2D, delta_time: time::Duration) {
        let delta_seconds = delta_time.as_secs_f32();

        let scroll_delta = calc_scroll_delta(cursor_screen_pos, self.viewport_size);
        let scroll_speed  = calc_scroll_speed(cursor_screen_pos, self.viewport_size);

        let offset_change = scroll_delta * scroll_speed * delta_seconds;

        let change  = offset_change.to_point2d();
        let current = self.current_scroll();

        self.set_scroll(Point2D::new(current.x + change.x, current.y + change.y));
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

fn calc_visible_cells_range(viewport_size: Size2D,
                            map_size_in_cells: Size2D,
                            transform: &WorldToScreenTransform) -> CellRange {

    // Add one extra row of tiles on each end to avoid any visual popping while scrolling.
    let tile_width  = BASE_TILE_SIZE.width  * transform.scaling;
    let tile_height = BASE_TILE_SIZE.height * transform.scaling;

    let screen_rect = Rect2D::new(
        Point2D::new(-tile_width, -tile_height),
        Size2D::new(viewport_size.width + tile_width, viewport_size.height + tile_height));

    selection::bounds(
        &screen_rect,
        BASE_TILE_SIZE,
        map_size_in_cells,
        &transform)
}

fn calc_scroll_delta(cursor_screen_pos: Point2D, viewport_size: Size2D) -> Vec2 {
    let mut scroll_delta = Vec2::zero();

    if cursor_screen_pos.x < SCROLL_MARGIN {
        scroll_delta.x += 1.0;
    } else if cursor_screen_pos.x > viewport_size.width - SCROLL_MARGIN {
        scroll_delta.x -= 1.0;
    }

    if cursor_screen_pos.y < SCROLL_MARGIN {
        scroll_delta.y += 1.0;
    } else if cursor_screen_pos.y > viewport_size.height - SCROLL_MARGIN {
        scroll_delta.y -= 1.0;
    }

    scroll_delta
}

fn calc_scroll_speed(cursor_screen_pos: Point2D, viewport_size: Size2D) -> f32 {
    let edge_dist_x = if cursor_screen_pos.x < SCROLL_MARGIN {
        SCROLL_MARGIN - cursor_screen_pos.x
    } else if cursor_screen_pos.x > viewport_size.width - SCROLL_MARGIN {
        cursor_screen_pos.x - (viewport_size.width - SCROLL_MARGIN)
    } else {
        0
    };

    let edge_dist_y = if cursor_screen_pos.y < SCROLL_MARGIN {
        SCROLL_MARGIN - cursor_screen_pos.y
    } else if cursor_screen_pos.y > viewport_size.height - SCROLL_MARGIN {
        cursor_screen_pos.y - (viewport_size.height - SCROLL_MARGIN)
    } else {
        0
    };

    let max_edge_dist = edge_dist_x.max(edge_dist_y);

    let scroll_strength = ((max_edge_dist as f32) / (SCROLL_MARGIN as f32)).clamp(0.0, 1.0);
    let scroll_speed_scaled = SCROLL_SPEED * scroll_strength;

    scroll_speed_scaled
}

fn calc_map_bounds(_map_size_in_cells: Size2D, _scaling: i32, _tile_spacing: i32) -> (Point2D, Point2D) {
    //let width  = (map_size_in_cells.width  * (BASE_TILE_SIZE.width  + tile_spacing) * scaling);
    //let height = (map_size_in_cells.height * (BASE_TILE_SIZE.height + tile_spacing) * scaling);

    //let half_w = width  / 2;
    //let half_h = height / 2;
    
    //(Point2D::new(-half_w, half_h), Point2D::new(half_w, height * 2))

    // TODO: WIP
    (Point2D::new(-16384, -16384), Point2D::new(16384, 16384))
}

fn clamp_to_map_bounds(map_size_in_cells: Size2D, scaling: i32, tile_spacing: i32, offset: Point2D) -> Point2D {
    let (mins, maxs) = calc_map_bounds(map_size_in_cells, scaling, tile_spacing);

    let off_x = offset.x.clamp(mins.x, maxs.x);
    let off_y = offset.y.clamp(mins.y, maxs.y);

    Point2D::new(off_x, off_y)
}
