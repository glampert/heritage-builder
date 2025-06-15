use std::time::{self};

use crate::{
    utils::{
        self, Size, Vec2, Rect,
        coords::{
            CellRange,
            WorldToScreenTransform
        }
    }
};

use super::{
    selection::{self},
    sets::BASE_TILE_SIZE
};

// ----------------------------------------------
// Constants / Enums
// ----------------------------------------------

pub const CONFINE_CURSOR_TO_WINDOW: bool = true;

pub const MIN_TILE_SPACING: f32 = 0.0;
pub const MAX_TILE_SPACING: f32 = 10.0;

// Zoom / scaling defaults:
pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 10.0;
const ZOOM_SPEED: f32 = 1.0; // pixels per second

// Cursor map scrolling defaults:
const SCROLL_MARGIN: f32 = 20.0;  // pixels from edge
const SCROLL_SPEED:  f32 = 500.0; // pixels per second

pub enum Offset {
    Center,
    Point(f32, f32),
}

#[repr(u32)]
pub enum Zoom {
    In,
    Out
}

// ----------------------------------------------
// Camera
// ----------------------------------------------

pub struct Camera {
    viewport_size: Size,
    map_size_in_cells: Size,
    transform: WorldToScreenTransform,
    current_zoom: f32,
    target_zoom: f32,
    is_zooming: bool,
}

impl Camera {
    pub fn new(viewport_size: Size,
               map_size_in_cells: Size,
               zoom: f32,
               offset: Offset,
               tile_spacing: f32) -> Self {

        let clamped_scaling = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let clamped_tile_spacing = tile_spacing.clamp(MIN_TILE_SPACING, MAX_TILE_SPACING);

        let clamped_offset = match offset {
            Offset::Center => {
                calc_map_center(map_size_in_cells, clamped_scaling, viewport_size)
            }
            Offset::Point(x, y) => {
                clamp_to_map_bounds(map_size_in_cells, clamped_scaling, viewport_size, Vec2::new(x, y))
            }
        };

        Self {
            viewport_size: viewport_size,
            map_size_in_cells: map_size_in_cells,
            transform: WorldToScreenTransform::new(
                clamped_scaling,
                clamped_offset,
                clamped_tile_spacing
            ),
            current_zoom: clamped_scaling,
            target_zoom: clamped_scaling,
            is_zooming: false,
        }
    }

    #[inline]
    pub fn visible_cells_range(&self) -> CellRange {
        calc_visible_cells_range(self.map_size_in_cells, self.viewport_size, &self.transform)
    }

    #[inline]
    pub fn transform(&self) -> &WorldToScreenTransform {
        &self.transform
    }

    #[inline]
    pub fn set_viewport_size(&mut self, new_size: Size) {
        self.viewport_size = new_size;
    }

    #[inline]
    pub fn viewport_size(&self) -> Size {
        self.viewport_size
    }

    #[inline]
    pub fn set_map_size_in_cells(&mut self, new_size: Size) {
        self.map_size_in_cells = new_size;
    }

    #[inline]
    pub fn map_size_in_cells(&self) -> Size {
        self.map_size_in_cells
    }

    // ----------------------
    // Tile spacing offsets:
    // ----------------------

    #[inline]
    pub fn tile_spacing_limits(&self) -> (f32, f32) {
        (MIN_TILE_SPACING, MAX_TILE_SPACING)
    }

    #[inline]
    pub fn current_tile_spacing(&self) -> f32 {
        self.transform.tile_spacing
    }

    #[inline]
    pub fn set_tile_spacing(&mut self, spacing: f32) {
        self.transform.tile_spacing = spacing.clamp(MIN_TILE_SPACING, MAX_TILE_SPACING);
    }

    // ----------------------
    // Zoom/scaling:
    // ----------------------

    #[inline]
    pub fn zoom_limits(&self) -> (f32, f32) {
        (MIN_ZOOM, MAX_ZOOM)
    }

    #[inline]
    pub fn current_zoom(&self) -> f32 {
        self.transform.scaling
    }

    #[inline]
    pub fn set_zoom(&mut self, zoom: f32) {
        let current_zoom = self.transform.scaling;
        let new_zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);

        let current_bounds = calc_map_bounds(self.map_size_in_cells, current_zoom, self.viewport_size);
        let new_bounds = calc_map_bounds(self.map_size_in_cells, new_zoom, self.viewport_size);

        // Remap the offset to the new scaled map bounds, so we stay at the same relative position as before.
        self.transform.offset.x = utils::map_value_to_range(
            self.transform.offset.x,
            current_bounds.min.x,
            current_bounds.max.x,
            new_bounds.min.x,
            new_bounds.max.x);

        self.transform.offset.y = utils::map_value_to_range(
            self.transform.offset.y,
            current_bounds.min.y,
            current_bounds.max.y,
            new_bounds.min.y,
            new_bounds.max.y);

        self.transform.scaling = new_zoom;
    }

    #[inline]
    pub fn request_zoom(&mut self, zoom: Zoom) {
        match zoom {
            Zoom::In => {
                // request zoom-in
                self.target_zoom = (self.target_zoom + 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
            },
            Zoom::Out => {
                // request zoom-out
                self.target_zoom = (self.target_zoom - 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
            }
        }
        self.is_zooming = true;
    }

    #[inline]
    pub fn update_zooming(&mut self, delta_time: time::Duration) {
        if self.is_zooming {
            if !utils::approx_equal(self.current_zoom, self.target_zoom, 0.001) {
                let delta_seconds = delta_time.as_secs_f32();
                self.current_zoom = utils::lerp(self.current_zoom, self.target_zoom, delta_seconds * ZOOM_SPEED);
            } else {
                self.current_zoom = self.target_zoom;
                self.is_zooming = false;
            }
            self.set_zoom(self.current_zoom);
        }
    }

    // ----------------------
    // Map X/Y scrolling:
    // ----------------------

    #[inline]
    pub fn scroll_limits(&self) -> (Vec2, Vec2) {
        let bounds = calc_map_bounds(self.map_size_in_cells, self.transform.scaling, self.viewport_size);
        (bounds.min, bounds.max)
    }

    #[inline]
    pub fn current_scroll(&self) -> Vec2 {
        self.transform.offset
    }

    #[inline]
    pub fn set_scroll(&mut self, scroll: Vec2) {
        self.transform.offset = clamp_to_map_bounds(
            self.map_size_in_cells,
            self.transform.scaling,
            self.viewport_size,
            scroll);
    }

    #[inline]
    pub fn update_scrolling(&mut self, cursor_screen_pos: Vec2, delta_time: time::Duration) {
        let delta_seconds = delta_time.as_secs_f32();

        let scroll_delta = calc_scroll_delta(cursor_screen_pos, self.viewport_size);
        let scroll_speed  = calc_scroll_speed(cursor_screen_pos, self.viewport_size);

        let offset_change = scroll_delta * scroll_speed * delta_seconds;
        let current = self.current_scroll();

        self.set_scroll(Vec2::new(current.x + offset_change.x, current.y + offset_change.y));
    }

    // Center camera to the map.
    pub fn center(&mut self) {
        let map_center = calc_map_center(self.map_size_in_cells, self.transform.scaling, self.viewport_size);
        self.set_scroll(map_center);
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

fn calc_visible_cells_range(map_size_in_cells: Size,
                            viewport_size: Size,
                            transform: &WorldToScreenTransform) -> CellRange {

    // Add one extra row of tiles on each end to avoid any visual popping while scrolling.
    let tile_width  = (BASE_TILE_SIZE.width  as f32) * transform.scaling;
    let tile_height = (BASE_TILE_SIZE.height as f32) * transform.scaling;

    let screen_rect = Rect::new(
        Vec2::new(-tile_width, -tile_height),
        Vec2::new((viewport_size.width as f32) + tile_width, (viewport_size.height as f32) + tile_height));

    selection::bounds(
        &screen_rect,
        BASE_TILE_SIZE,
        map_size_in_cells,
        &transform)
}

fn calc_scroll_delta(cursor_screen_pos: Vec2, viewport_size: Size) -> Vec2 {
    let mut scroll_delta = Vec2::zero();

    if cursor_screen_pos.x < SCROLL_MARGIN {
        scroll_delta.x += 1.0;
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - SCROLL_MARGIN {
        scroll_delta.x -= 1.0;
    }

    if cursor_screen_pos.y < SCROLL_MARGIN {
        scroll_delta.y += 1.0;
    } else if cursor_screen_pos.y > (viewport_size.height as f32) - SCROLL_MARGIN {
        scroll_delta.y -= 1.0;
    }

    scroll_delta
}

fn calc_scroll_speed(cursor_screen_pos: Vec2, viewport_size: Size) -> f32 {
    let edge_dist_x = if cursor_screen_pos.x < SCROLL_MARGIN {
        SCROLL_MARGIN - cursor_screen_pos.x
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - SCROLL_MARGIN {
        cursor_screen_pos.x - ((viewport_size.width as f32) - SCROLL_MARGIN)
    } else {
        0.0
    };

    let edge_dist_y = if cursor_screen_pos.y < SCROLL_MARGIN {
        SCROLL_MARGIN - cursor_screen_pos.y
    } else if cursor_screen_pos.y > (viewport_size.height as f32) - SCROLL_MARGIN {
        cursor_screen_pos.y - ((viewport_size.height as f32) - SCROLL_MARGIN)
    } else {
        0.0
    };

    let max_edge_dist = edge_dist_x.max(edge_dist_y);

    let scroll_strength = (max_edge_dist / SCROLL_MARGIN).clamp(0.0, 1.0);
    let scroll_speed_scaled = SCROLL_SPEED * scroll_strength;

    scroll_speed_scaled
}

fn calc_map_center(map_size_in_cells: Size, scaling: f32, viewport_size: Size) -> Vec2 {
    let bounds = calc_map_bounds(map_size_in_cells, scaling, viewport_size);

    let half_diff_x = (bounds.max.x - bounds.min.x).abs() / 2.0;
    let half_diff_y = (bounds.max.y - bounds.min.y).abs() / 2.0;

    let x = bounds.max.x - half_diff_x;
    let y = bounds.max.y - half_diff_y;

    Vec2::new(x, y)
}

fn calc_map_bounds(map_size_in_cells: Size, scaling: f32, viewport_size: Size) -> Rect {
    debug_assert!(map_size_in_cells.is_valid());
    debug_assert!(viewport_size.is_valid());

    let tile_width_pixels  = (BASE_TILE_SIZE.width  as f32) * scaling;
    let tile_height_pixels = (BASE_TILE_SIZE.height as f32) * scaling;

    let map_width_pixels  = (map_size_in_cells.width  as f32) * tile_width_pixels;
    let map_height_pixels = (map_size_in_cells.height as f32) * tile_height_pixels;

    let half_tile_width_pixels = tile_width_pixels / 2.0;
    let half_map_width_pixels  = map_width_pixels  / 2.0;

    let min_pt = Vec2::new(
        -(half_map_width_pixels + half_tile_width_pixels - (viewport_size.width as f32)),
        (viewport_size.height as f32) - tile_height_pixels);

    let max_pt = Vec2::new(
        half_map_width_pixels - half_tile_width_pixels,
        map_height_pixels - tile_height_pixels);

    Rect::from_extents(min_pt, max_pt)
}

fn clamp_to_map_bounds(map_size_in_cells: Size, scaling: f32, viewport_size: Size, offset: Vec2) -> Vec2 {
    let bounds = calc_map_bounds(map_size_in_cells, scaling, viewport_size);

    let off_x = offset.x.clamp(bounds.min.x, bounds.max.x);
    let off_y = offset.y.clamp(bounds.min.y, bounds.max.y);

    Vec2::new(off_x, off_y)
}
