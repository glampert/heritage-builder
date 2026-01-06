use serde::{Deserialize, Serialize};

use super::{selection};
use crate::{
    singleton,
    engine::time::Seconds,
    game::config::GameConfigs,
    save::*,
    utils::{
        self,
        constants::*,
        Rect, Size, Vec2,
        coords::{self, Cell, CellF32, CellRange, WorldToScreenTransform, IsoPointF32},
    },
};

// ----------------------------------------------
// Camera Helpers
// ----------------------------------------------

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum CameraOffset {
    Center,
    Point(f32, f32),
}

impl std::fmt::Display for CameraOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Center => write!(f, "Center"),
            Self::Point(x, y) => write!(f, "Point({x:.2},{y:.2})"),
        }
    }
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum CameraZoom {
    In,
    Out,
}

impl CameraZoom {
    // Zoom / scaling defaults:
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 10.0;
    pub const DEFAULT: f32 = 1.0;
    pub const SPEED: f32 = 1.0; // pixels per second
}

pub struct CameraGlobalSettings {
    // For fixed step zoom with CTRL +/= key shortcuts.
    pub fixed_step_zoom_amount: f32,

    // Use fixed step zoom with mouse scroll zoom instead of smooth interpolation.
    pub disable_smooth_mouse_scroll_zoom: bool,

    // Disables mouse scroll zoom altogether.
    pub disable_mouse_scroll_zoom: bool,

    // Disables zooming with keyboard shortcuts.
    pub disable_key_shortcut_zoom: bool,
}

singleton! { GLOBAL_SETTINGS_SINGLETON, CameraGlobalSettings }

impl CameraGlobalSettings {
    const fn new() -> Self {
        Self {
            fixed_step_zoom_amount: 0.5,
            disable_smooth_mouse_scroll_zoom: false,
            disable_mouse_scroll_zoom: false,
            disable_key_shortcut_zoom: false,
        }
    }

    pub fn set_from_game_configs(&mut self, configs: &GameConfigs) {
        self.fixed_step_zoom_amount           = configs.camera.fixed_step_zoom_amount;
        self.disable_smooth_mouse_scroll_zoom = configs.camera.disable_smooth_mouse_scroll_zoom;
        self.disable_mouse_scroll_zoom        = configs.camera.disable_mouse_scroll_zoom;
        self.disable_key_shortcut_zoom        = configs.camera.disable_key_shortcut_zoom;
    }
}

// ----------------------------------------------
// Camera
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Camera {
    viewport_size: Size,
    map_size_in_cells: Size,
    transform: WorldToScreenTransform,
    current_zoom: f32,
    target_zoom: f32,
    is_zooming: bool,

    #[serde(skip)]
    is_scrolling: bool,
}

impl Camera {
    // Cursor map scrolling defaults:
    const SCROLL_MARGIN: f32 = 20.0; // pixels from edge
    const SCROLL_SPEED: f32 = 500.0; // pixels per second

    pub fn new(viewport_size: Size,
               map_size_in_cells: Size,
               zoom: f32,
               offset: CameraOffset)
               -> Self {
        let clamped_scaling = zoom.clamp(CameraZoom::MIN, CameraZoom::MAX);
        let clamped_offset = match offset {
            CameraOffset::Center => {
                calc_map_center(map_size_in_cells, clamped_scaling, viewport_size)
            }
            CameraOffset::Point(x, y) => clamp_to_map_bounds(map_size_in_cells,
                                                             clamped_scaling,
                                                             viewport_size,
                                                             Vec2::new(x, y)),
        };

        Self {
            viewport_size,
            map_size_in_cells,
            transform: WorldToScreenTransform::new(clamped_scaling, clamped_offset),
            current_zoom: clamped_scaling,
            target_zoom: clamped_scaling,
            is_zooming: false,
            is_scrolling: false
        }
    }

    #[inline]
    pub fn visible_cells_range(&self) -> CellRange {
        calc_visible_cells_range(self.map_size_in_cells, self.viewport_size, self.transform)
    }

    #[inline]
    pub fn transform(&self) -> WorldToScreenTransform {
        self.transform
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

    #[inline]
    pub fn iso_world_position(&self) -> IsoPointF32 {
        let viewport_center = self.viewport_size.to_vec2() * 0.5;
        // Convert screen -> iso/world
        IsoPointF32((viewport_center - self.transform.offset) / self.transform.scaling)
    }

    #[inline]
    pub fn iso_viewport_center(&self) -> IsoPointF32 {
        let viewport_center = self.viewport_size.to_vec2() * 0.5;
        IsoPointF32(viewport_center / self.transform.scaling)
    }

    #[inline]
    pub fn iso_bounds(&self) -> (IsoPointF32, IsoPointF32) {
        let center_iso = self.iso_world_position();
        let half_iso   = self.iso_viewport_center();

        let top_left     = IsoPointF32(center_iso.0 - half_iso.0);
        let bottom_right = IsoPointF32(center_iso.0 + half_iso.0);

        (top_left, bottom_right)
    }

    #[inline]
    pub fn iso_corners(&self) -> [IsoPointF32; 4] {
        let center_iso = self.iso_world_position();
        let half_iso   = self.iso_viewport_center();

        let top_left     = IsoPointF32(center_iso.0 - half_iso.0);
        let bottom_right = IsoPointF32(center_iso.0 + half_iso.0);
        let top_right    = IsoPointF32(Vec2::new(center_iso.0.x + half_iso.0.x, center_iso.0.y - half_iso.0.y));
        let bottom_left  = IsoPointF32(Vec2::new(center_iso.0.x - half_iso.0.x, center_iso.0.y + half_iso.0.y));

        [top_left, top_right, bottom_right, bottom_left]
    }

    #[inline]
    pub fn cell_bounds(&self) -> (CellF32, CellF32) {
        let (iso_top_left, iso_bottom_right) = self.iso_bounds();

        // Convert iso rect corners to fractional cell coordinates (continuous).
        let cell_min = coords::iso_to_cell_f32(iso_top_left);
        let cell_max = coords::iso_to_cell_f32(iso_bottom_right);

        // Ensure correct ordering (min <= max).
        let cell_x_min = cell_min.0.x.min(cell_max.0.x);
        let cell_x_max = cell_min.0.x.max(cell_max.0.x);
        let cell_y_min = cell_min.0.y.min(cell_max.0.y);
        let cell_y_max = cell_min.0.y.max(cell_max.0.y);

        // Build cell rect corners in fractional cell coords (CellF32).
        let cell_top_left     = CellF32(Vec2::new(cell_x_min, cell_y_min));
        let cell_bottom_right = CellF32(Vec2::new(cell_x_max, cell_y_max));

        (cell_top_left, cell_bottom_right)
    }

    #[inline]
    pub fn cell_corners(&self) -> [CellF32; 4] {
        let iso_corners = self.iso_corners();
        [
            coords::iso_to_cell_f32(iso_corners[0]),
            coords::iso_to_cell_f32(iso_corners[1]),
            coords::iso_to_cell_f32(iso_corners[2]),
            coords::iso_to_cell_f32(iso_corners[3]),
        ]
    }

    // ----------------------
    // Zoom/scaling:
    // ----------------------

    #[inline]
    pub fn zoom_limits(&self) -> (f32, f32) {
        (CameraZoom::MIN, CameraZoom::MAX)
    }

    #[inline]
    pub fn current_zoom(&self) -> f32 {
        self.transform.scaling
    }

    #[inline]
    pub fn set_zoom(&mut self, zoom: f32) {
        let current_zoom = self.transform.scaling;
        let new_zoom = zoom.clamp(CameraZoom::MIN, CameraZoom::MAX);

        let current_bounds = calc_map_bounds(self.map_size_in_cells, current_zoom, self.viewport_size);
        let new_bounds = calc_map_bounds(self.map_size_in_cells, new_zoom, self.viewport_size);

        // Remap the offset to the new scaled map bounds, so we stay at the same
        // relative position as before.
        self.transform.offset.x = utils::map_value_to_range(self.transform.offset.x,
                                                            current_bounds.min.x,
                                                            current_bounds.max.x,
                                                            new_bounds.min.x,
                                                            new_bounds.max.x);

        self.transform.offset.y = utils::map_value_to_range(self.transform.offset.y,
                                                            current_bounds.min.y,
                                                            current_bounds.max.y,
                                                            new_bounds.min.y,
                                                            new_bounds.max.y);

        self.transform.scaling = new_zoom;
    }

    #[inline]
    pub fn request_zoom(&mut self, zoom: CameraZoom) {
        match zoom {
            CameraZoom::In => {
                // request zoom-in
                self.target_zoom = (self.target_zoom + 1.0).clamp(CameraZoom::MIN, CameraZoom::MAX);
            }
            CameraZoom::Out => {
                // request zoom-out
                self.target_zoom = (self.target_zoom - 1.0).clamp(CameraZoom::MIN, CameraZoom::MAX);
            }
        }
        self.is_zooming = true;
    }

    #[inline]
    pub fn update_zooming(&mut self, delta_time_secs: Seconds) {
        if self.is_zooming {
            if !utils::approx_equal(self.current_zoom, self.target_zoom, 0.001) {
                self.current_zoom = utils::lerp(self.current_zoom,
                                                self.target_zoom,
                                                delta_time_secs * CameraZoom::SPEED);
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
    pub fn is_scrolling(&self) -> bool {
        self.is_scrolling
    }

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
        self.transform.offset = clamp_to_map_bounds(self.map_size_in_cells,
                                                    self.transform.scaling,
                                                    self.viewport_size,
                                                    scroll);
    }

    #[inline]
    pub fn update_scrolling(&mut self,
                            ui_hovered: bool,
                            cursor_screen_pos: Vec2,
                            delta_time_secs: Seconds) {
        let scroll_delta = calc_scroll_delta(ui_hovered, cursor_screen_pos, self.viewport_size);
        let scroll_speed  = calc_scroll_speed(ui_hovered, cursor_screen_pos, self.viewport_size);

        let offset_change = scroll_delta * scroll_speed * delta_time_secs;
        let previous_scroll = self.current_scroll();

        self.set_scroll(previous_scroll + offset_change);
        self.is_scrolling = (offset_change.x > 0.0 || offset_change.y > 0.0) && previous_scroll != self.current_scroll();
    }

    // ----------------------
    // Camera Teleporting:
    // ----------------------

    // Center camera to the map.
    pub fn center(&mut self) {
        let map_center = calc_map_center(self.map_size_in_cells, self.transform.scaling, self.viewport_size);
        self.set_scroll(map_center);
    }

    // Snaps the camera to `destination_cell`.
    pub fn teleport(&mut self, destination_cell: Cell) -> bool {
        if !destination_cell.is_valid() {
            return false;
        }

        let viewport_center = self.viewport_size.to_vec2() * 0.5;

        let iso_point = coords::cell_to_iso(destination_cell);

        let transform_no_offset =
            WorldToScreenTransform::new(self.transform.scaling, Vec2::zero());

        let screen_point = coords::iso_to_screen_point(iso_point, transform_no_offset);

        self.set_scroll(viewport_center - screen_point);
        true
    }

    // Snaps the camera to `destination_iso` isometric point.
    pub fn teleport_iso(&mut self, destination_iso: IsoPointF32) -> bool {
        let viewport_center = self.viewport_size.to_vec2() * 0.5;

        let transform_no_offset =
            WorldToScreenTransform::new(self.transform.scaling, Vec2::zero());

        let screen_point =
            coords::iso_to_screen_rect_f32(destination_iso, BASE_TILE_SIZE_I32, transform_no_offset);

        self.set_scroll(viewport_center - screen_point.position());
        true
    }
}

// ----------------------------------------------
// Save/Load for Camera
// ----------------------------------------------

impl Save for Camera {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load for Camera {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        // Stop zooming and snap to target zoom.
        self.current_zoom = self.target_zoom;
        self.is_zooming = false;
        self.set_zoom(self.current_zoom);
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

fn calc_visible_cells_range(map_size_in_cells: Size,
                            viewport_size: Size,
                            transform: WorldToScreenTransform)
                            -> CellRange {
    if !map_size_in_cells.is_valid() {
        return CellRange::new(Cell::zero(), Cell::zero());
    }

    // Add one extra row of tiles on each end to avoid any visual popping while scrolling.
    let tile_width  = BASE_TILE_WIDTH_F32  * transform.scaling;
    let tile_height = BASE_TILE_HEIGHT_F32 * transform.scaling;

    let pos  = Vec2::new(-tile_width, -tile_height);
    let size = Vec2::new((viewport_size.width as f32) + tile_width, (viewport_size.height as f32) + tile_height);
    let screen_rect = Rect::new(pos, size);

    selection::bounds(&screen_rect, map_size_in_cells, transform)
}

fn calc_scroll_delta(ui_hovered: bool, cursor_screen_pos: Vec2, viewport_size: Size) -> Vec2 {
    let mut scroll_delta = Vec2::zero();

    if cursor_screen_pos.x < Camera::SCROLL_MARGIN {
        scroll_delta.x += 1.0;
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - Camera::SCROLL_MARGIN {
        scroll_delta.x -= 1.0;
    }

    // Only block scrolling if hovering an ImGui item (like menu buttons).
    if !ui_hovered {
        if cursor_screen_pos.y < Camera::SCROLL_MARGIN {
            scroll_delta.y += 1.0;
        } else if cursor_screen_pos.y > (viewport_size.height as f32) - Camera::SCROLL_MARGIN {
            scroll_delta.y -= 1.0;
        }
    }

    scroll_delta
}

fn calc_scroll_speed(ui_hovered: bool, cursor_screen_pos: Vec2, viewport_size: Size) -> f32 {
    if ui_hovered {
        return 0.0; // Stop scrolling entirely while over menu items.
    }

    let edge_dist_x = if cursor_screen_pos.x < Camera::SCROLL_MARGIN {
        Camera::SCROLL_MARGIN - cursor_screen_pos.x
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - Camera::SCROLL_MARGIN {
        cursor_screen_pos.x - ((viewport_size.width as f32) - Camera::SCROLL_MARGIN)
    } else {
        0.0
    };

    let edge_dist_y = if cursor_screen_pos.y < Camera::SCROLL_MARGIN {
        Camera::SCROLL_MARGIN - cursor_screen_pos.y
    } else if cursor_screen_pos.y > (viewport_size.height as f32) - Camera::SCROLL_MARGIN {
        cursor_screen_pos.y - ((viewport_size.height as f32) - Camera::SCROLL_MARGIN)
    } else {
        0.0
    };

    let max_edge_dist = edge_dist_x.max(edge_dist_y);
    let scroll_strength = (max_edge_dist / Camera::SCROLL_MARGIN).clamp(0.0, 1.0);

    Camera::SCROLL_SPEED * scroll_strength
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
    debug_assert!(viewport_size.is_valid());

    if !map_size_in_cells.is_valid() {
        return Rect::from_pos_and_size(Vec2::zero(), viewport_size);
    }

    let tile_width_pixels  = BASE_TILE_WIDTH_F32  * scaling;
    let tile_height_pixels = BASE_TILE_HEIGHT_F32 * scaling;

    let map_width_pixels  = (map_size_in_cells.width  as f32) * tile_width_pixels;
    let map_height_pixels = (map_size_in_cells.height as f32) * tile_height_pixels;

    let half_tile_width_pixels = tile_width_pixels * 0.5;
    let half_map_width_pixels  = map_width_pixels  * 0.5;

    let min_pt = Vec2::new(
        -(half_map_width_pixels + half_tile_width_pixels - (viewport_size.width as f32)),
        (viewport_size.height as f32) - tile_height_pixels);

    let max_pt = Vec2::new(
        half_map_width_pixels - half_tile_width_pixels,
        map_height_pixels - tile_height_pixels);

    Rect::from_extents(min_pt, max_pt)
}

fn clamp_to_map_bounds(map_size_in_cells: Size,
                       scaling: f32,
                       viewport_size: Size,
                       offset: Vec2)
                       -> Vec2 {
    let bounds = calc_map_bounds(map_size_in_cells, scaling, viewport_size);

    let off_x = offset.x.clamp(bounds.min.x, bounds.max.x);
    let off_y = offset.y.clamp(bounds.min.y, bounds.max.y);

    Vec2::new(off_x, off_y)
}
