use serde::{Deserialize, Serialize};

use super::{selection};
use crate::{
    singleton,
    save::*,
    game::config::GameConfigs,
    ui::{self, UiSystem, UiStaticVar},
    engine::{DebugDraw, time::Seconds},
    utils::{
        self,
        constants::*,
        Rect, Size, Vec2, Color,
        coords::{self, Cell, CellF32, CellRange, WorldToScreenTransform, IsoPointF32},
    },
};

// ----------------------------------------------
// Camera Coordinates and Conventions
// ----------------------------------------------

/*
Screen origin (0,0): Top-left corner.
 +X : points towards right-hand-side
 -X : points towards left-hand-side
 +Y : points towards bottom
 -Y : points towards top

Camera Scroll Directions:
 +X offset : scrolls left
 -X offset : scrolls right
 +Y offset : scrolls up
 -Y offset : scrolls down

Camera Scroll Offsets:
 Offset = (0,0) : Only tile (0,0) is half visible at the screen origin (top-left corner).
 Y < 0 : Map fully offscreen at the top of the screen.
 Y > map_height_pixels : Map fully offscreen at the bottom of the screen.
 X < 0 : Scrolling right, past the middle of the map.
 X > 0 : Scrolling left, past the middle of the map.

Camera Iso Position:
 Scroll up    : -iso Y
 Scroll down  : +iso Y
 Scroll right : +iso X
 Scroll left  : -iso X

Map Bounds Clamping:
 Axis-aligned bounding box of the rotated isometric world map.
 Always clamp camera offsets to these limits. Void corners can
 be seen when near the bounding box limits. All tiles are accessible.

Map Playable Area Clamping:
 Inner isometric diamond playable area. Clamp the camera so that
 it always stays inside the map diamond, preventing void corners
 from being visible. The top/bottom, left/right edge tiles of the
 map are not accessible/visible when this is enabled.
 See `CameraConstraint` for details.
*/

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

    // Constrain camera movement to inner map diamond playable area? (debug option).
    pub constrain_to_playable_map_area: bool,

    // Constrain camera movement to map AABB? This is a superset of the playable area. (debug option).
    pub clamp_to_map_bounds: bool,

    // Display map debug bounds and camera debug overlays.
    pub enable_debug_draw: bool,

    // Camera scroll/movement speed in pixels per second.
    pub scroll_speed: f32,

    // In pixels from screen edge.
    pub scroll_margin: f32,
}

singleton! { GLOBAL_SETTINGS_SINGLETON, CameraGlobalSettings }

impl CameraGlobalSettings {
    const fn new() -> Self {
        Self {
            fixed_step_zoom_amount: 0.5,
            disable_smooth_mouse_scroll_zoom: false,
            disable_mouse_scroll_zoom: false,
            disable_key_shortcut_zoom: false,
            constrain_to_playable_map_area: true,
            clamp_to_map_bounds: true,
            enable_debug_draw: false,
            scroll_speed: 500.0,
            scroll_margin: 20.0,
        }
    }

    pub fn set_from_game_configs(&mut self, configs: &GameConfigs) {
        self.fixed_step_zoom_amount           = configs.camera.fixed_step_zoom_amount;
        self.disable_smooth_mouse_scroll_zoom = configs.camera.disable_smooth_mouse_scroll_zoom;
        self.disable_mouse_scroll_zoom        = configs.camera.disable_mouse_scroll_zoom;
        self.disable_key_shortcut_zoom        = configs.camera.disable_key_shortcut_zoom;
        self.constrain_to_playable_map_area   = configs.camera.constrain_to_playable_map_area;
        self.scroll_speed                     = configs.camera.scroll_speed;
        self.scroll_margin                    = configs.camera.scroll_margin;
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
            is_scrolling: false,
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
    pub fn viewport_center(&self) -> Vec2 {
        self.viewport_size.to_vec2() * 0.5
    }

    #[inline]
    pub fn set_map_size_in_cells(&mut self, new_size: Size) {
        self.map_size_in_cells = new_size;
    }

    #[inline]
    pub fn map_size_in_cells(&self) -> Size {
        self.map_size_in_cells
    }

    // Camera center position in screen space.
    #[inline]
    pub fn screen_space_position(&self) -> Vec2 {
        let iso_point = self.iso_world_position();
        coords::iso_to_screen_rect_f32(iso_point, BASE_TILE_SIZE_I32, self.transform).position()
    }

    // Camera center position in isometric world space.
    #[inline]
    pub fn iso_world_position(&self) -> IsoPointF32 {
        // Convert screen -> iso/world
        IsoPointF32((self.viewport_center() - self.current_scroll()) / self.current_zoom())
    }

    #[inline]
    pub fn iso_viewport_center(&self) -> IsoPointF32 {
        IsoPointF32(self.viewport_center() / self.current_zoom())
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

    pub fn map_diamond_corners(&self, camera_relative: bool) -> [Vec2; 4] {
        let map_origin_cell = Cell::zero();
        let map_size_in_pixels = Size::new(
            self.map_size_in_cells.width   * BASE_TILE_SIZE_I32.width,
            self.map_size_in_cells.height * BASE_TILE_SIZE_I32.height,
        );

        let transform = if camera_relative {
            self.transform
        } else {
            // Exclude camera offset/scroll from bounds. Keep scaling.
            WorldToScreenTransform::new(self.current_zoom(), Vec2::zero())
        };

        let corners = coords::cell_to_screen_diamond_points(
            map_origin_cell,
            map_size_in_pixels,
            transform);

        fn signed_area(poly: &[Vec2; 4]) -> f32 {
            let mut area = 0.0;
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                area += (a.x * b.y) - (b.x * a.y);
            }
            area * 0.5
        }

        // Expected CCW winding, area must be positive.
        debug_assert!(signed_area(&corners) > 0.0);
        corners
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
        let current_zoom = self.current_zoom();
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
    // Camera X/Y scrolling:
    // ----------------------

    #[inline]
    pub fn is_scrolling(&self) -> bool {
        self.is_scrolling
    }

    #[inline]
    pub fn scroll_limits(&self) -> (Vec2, Vec2) {
        let bounds = calc_map_bounds(self.map_size_in_cells, self.current_zoom(), self.viewport_size);
        (bounds.min, bounds.max)
    }

    #[inline]
    pub fn current_scroll(&self) -> Vec2 {
        self.transform.offset
    }

    #[inline]
    pub fn set_scroll(&mut self, scroll: Vec2) {
        self.transform.offset = if CameraGlobalSettings::get().clamp_to_map_bounds {
            clamp_to_map_bounds(self.map_size_in_cells,
                                self.current_zoom(),
                                self.viewport_size,
                                scroll)
        } else {
            scroll
        };
    }

    pub fn update_scrolling(&mut self, cursor_screen_pos: Vec2, delta_time_secs: Seconds) {
        let settings = CameraGlobalSettings::get();

        let scroll_dir = calc_scroll_delta(
            cursor_screen_pos,
            self.viewport_size,
            settings.scroll_margin);

        let scroll_speed = calc_scroll_speed(
            cursor_screen_pos,
            self.viewport_size,
            settings.scroll_margin,
            settings.scroll_speed);

        let desired_delta = scroll_dir * scroll_speed * delta_time_secs;
        if desired_delta == Vec2::zero() {
            self.is_scrolling = false;
            return;
        }

        // If unconstrained, move freely.
        if !settings.constrain_to_playable_map_area {
            self.set_scroll(self.current_scroll() + desired_delta);
            self.is_scrolling = true;
            return;
        }

        // NOTE: Compute constraint in *unscrolled* screen space (without camera offset).
        let camera_relative = false;
        let constraints = CameraConstraints::new(
            &self.map_diamond_corners(camera_relative),
            self.viewport_center());

        // Match unscrolled diamond points (camera_relative=false).
        let camera_center = self.viewport_center() - self.current_scroll();
        let final_delta = constraints.clamp_delta(camera_center, desired_delta);

        if final_delta != Vec2::zero() {
            self.set_scroll(self.current_scroll() + final_delta);
            self.is_scrolling = true;
        } else {
            self.is_scrolling = false;
        }
    }

    // ----------------------
    // Camera Teleporting:
    // ----------------------

    // Center camera to the map.
    pub fn center(&mut self) {
        let map_center = calc_map_center(self.map_size_in_cells, self.current_zoom(), self.viewport_size);
        self.set_scroll(map_center);
    }

    // Snaps the camera to `destination_cell`.
    pub fn teleport(&mut self, destination_cell: Cell) -> bool {
        if !destination_cell.is_valid() {
            return false;
        }

        let viewport_center = self.viewport_center();

        let iso_point = coords::cell_to_iso(destination_cell);

        let transform_no_offset =
            WorldToScreenTransform::new(self.current_zoom(), Vec2::zero());

        let screen_point = coords::iso_to_screen_point(iso_point, transform_no_offset);

        self.set_scroll(viewport_center - screen_point);
        true
    }

    // Snaps the camera to `destination_iso` isometric point.
    pub fn teleport_iso(&mut self, destination_iso: IsoPointF32) -> bool {
        let viewport_center = self.viewport_center();

        let transform_no_offset =
            WorldToScreenTransform::new(self.current_zoom(), Vec2::zero());

        let screen_point =
            coords::iso_to_screen_rect_f32(destination_iso, BASE_TILE_SIZE_I32, transform_no_offset);

        self.set_scroll(viewport_center - screen_point.position());
        true
    }

    // ----------------------
    // Camera Debug:
    // ----------------------

    pub fn draw_debug(&self, debug_draw: &mut dyn DebugDraw, ui_sys: &UiSystem) {
        if !CameraGlobalSettings::get().enable_debug_draw {
            return;
        }

        let camera_relative = true;

        // Map diamond bounds and inward-facing normals:
        draw_diamond(debug_draw,
                     &self.map_diamond_corners(camera_relative),
                     Color::red(),
                     Color::green());

        // Half map diamond bounds and normals, where we constrain
        // the camera center to, in camera-relative screen/render space:
        let constraints = CameraConstraints::new(
            &self.map_diamond_corners(camera_relative),
            self.viewport_center());
        draw_diamond(debug_draw,
                     &constraints.diamond,
                     Color::blue(),
                     Color::yellow());

        // Camera center point, in screen/render space:
        let camera_center = self.screen_space_position();
        debug_draw.point(camera_center, Color::magenta(), 15.0);

        let ui = ui_sys.ui();
        ui::overlay(ui, "Camera Center", camera_center, 0.6, || {
            ui.text(format!("C:{:.1},{:.1}", camera_center.x, camera_center.y));
            ui.text(format!("O:{:.1},{:.1}", self.current_scroll().x, self.current_scroll().y));
            ui.text(format!("I:{:.1},{:.1}", self.iso_world_position().0.x, self.iso_world_position().0.y));
        });
    }

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        let settings = CameraGlobalSettings::get_mut();

        let mut key_shortcut_zoom = !settings.disable_key_shortcut_zoom;
        if ui.checkbox("Keyboard Zoom", &mut key_shortcut_zoom) {
            settings.disable_key_shortcut_zoom = !key_shortcut_zoom;
        }

        let mut mouse_scroll_zoom = !settings.disable_mouse_scroll_zoom;
        if ui.checkbox("Mouse Scroll Zoom", &mut mouse_scroll_zoom) {
            settings.disable_mouse_scroll_zoom = !mouse_scroll_zoom;
        }

        let mut smooth_mouse_scroll_zoom = !settings.disable_smooth_mouse_scroll_zoom;
        if ui.checkbox("Smooth Mouse Scroll Zoom", &mut smooth_mouse_scroll_zoom) {
            settings.disable_smooth_mouse_scroll_zoom = !smooth_mouse_scroll_zoom;
        }

        ui.checkbox("Constrain To Playable Map Area", &mut settings.constrain_to_playable_map_area);
        ui.checkbox("Clamp To Map AABB Bounds", &mut settings.clamp_to_map_bounds);
        ui.checkbox("Enable Debug Draw", &mut settings.enable_debug_draw);

        ui.separator();

        let (zoom_min, zoom_max) = self.zoom_limits();
        let mut zoom = self.current_zoom();

        if ui.slider("Zoom", zoom_min, zoom_max, &mut zoom) {
            self.set_zoom(zoom);
        }

        let mut step_zoom = settings.fixed_step_zoom_amount;
        if ui.input_float("Step Zoom", &mut step_zoom)
            .display_format("%.1f")
            .step(0.5)
            .build()
        {
            settings.fixed_step_zoom_amount = step_zoom.clamp(zoom_min, zoom_max);
        }

        ui.separator();

        let scroll_limits = self.scroll_limits();
        let mut scroll = self.current_scroll();

        if ui.slider_config("Scroll X", scroll_limits.0.x, scroll_limits.1.x)
             .display_format("%.1f")
             .build(&mut scroll.x)
        {
            self.set_scroll(scroll);
        }

        if ui.slider_config("Scroll Y", scroll_limits.0.y, scroll_limits.1.y)
             .display_format("%.1f")
             .build(&mut scroll.y)
        {
            self.set_scroll(scroll);
        }

        ui.separator();

        static TELEPORT_CELL: UiStaticVar<Cell> = UiStaticVar::new(Cell::invalid());
        ui::input_i32_xy(ui, "Teleport To Cell:", TELEPORT_CELL.as_mut(), false, None, None);

        if ui.button("Teleport") {
            self.teleport(*TELEPORT_CELL);
        }

        ui.same_line();

        if ui.button("Re-center") {
            self.center();
        }
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
// Helper functions / structs
// ----------------------------------------------

#[inline]
fn inward_normal(edge: Vec2) -> Vec2 {
    // CCW winding -> inward normal.
    // For a CCW polygon, rotating the edge by +90° produces an inward-facing normal.
    Vec2::new(-edge.y, edge.x).normalize()
}

// Convex camera constraints derived from the isometric map diamond.
//
// The constraint polygon is constructed by shrinking the map diamond by half
// the viewport size in screen space. By constraining the camera *center* to
// this shrunken diamond, we guarantee that the viewport never exposes void
// space outside the map.
//
// Constraint enforcement is done using inward-facing half-space planes.
// Camera motion is clamped by projecting away outward velocity components,
// which naturally produces smooth sliding along edges and stable behavior at
// corners.
struct CameraConstraints {
    // CCW convex polygon with inward-facing normals.
    diamond: [Vec2; 4],

    // Precomputed edge normals for each diamond[i] and diamond[i+1] pair.
    normals: [Vec2; 4],
}

impl CameraConstraints {
    // Constructs camera constraints from the map diamond.
    // `diamond` must be a CCW-ordered convex polygon in screen space.
    // `viewport_half_extents` is half the viewport size in the same coordinate space.
    // The resulting polygon represents the valid region for the camera center.
    fn new(diamond: &[Vec2; 4], viewport_half_extents: Vec2) -> Self {
        debug_assert!(viewport_half_extents.x > 0.0 && viewport_half_extents.y > 0.0);

        #[derive(Copy, Clone, Default)]
        struct ConstraintPlane {
            p: Vec2, // Point on the plane (boundary).
            n: Vec2, // Inward-facing unit normal.
        }

        // Computes the intersection point between two half-space boundary planes.
        // Each plane is represented by a point on the plane and an inward-facing
        // unit normal. The intersection is found by advancing along A's tangent
        // direction until the point lies on B's plane.
        fn compute_constraint_vertex(plane_a: &ConstraintPlane, plane_b: &ConstraintPlane) -> Vec2 {
            // Tangent direction of plane A (perpendicular to its inward normal).
            let plane_a_tangent = Vec2::new(plane_a.n.y, -plane_a.n.x);

            const NORMAL_EPS: f32 = 1e-6;
            let tangent_dot_normal = plane_a_tangent.dot(plane_b.n);
            debug_assert!(tangent_dot_normal.abs() > NORMAL_EPS, "Degenerate constraint planes: edges nearly parallel!");

            let distance_along_tangent = (plane_b.p - plane_a.p).dot(plane_b.n) / tangent_dot_normal;
            plane_a.p + (plane_a_tangent * distance_along_tangent)
        }

        let mut planes  = [ConstraintPlane::default(); 4];
        let mut normals = [Vec2::zero(); 4];

        // Shrink diamond polygon in half:
        for i in 0..diamond.len() {
            let a = diamond[i];
            let b = diamond[(i + 1) % diamond.len()];

            // CCW diamond, inward normal.
            let edge = b - a;
            let normal = inward_normal(edge);

            let offset = (viewport_half_extents.x * normal.x.abs()) + 
                              (viewport_half_extents.y * normal.y.abs());

            planes[i] = ConstraintPlane {
                p: a + (normal * offset),
                n: normal,
            };

            normals[i] = normal;
        }

        // Reconstruct shrunken diamond vertices from constraint planes:
        Self {
            diamond: [
                compute_constraint_vertex(&planes[0], &planes[1]),
                compute_constraint_vertex(&planes[1], &planes[2]),
                compute_constraint_vertex(&planes[2], &planes[3]),
                compute_constraint_vertex(&planes[3], &planes[0]),
            ],
            normals,
        }
    }

    // Clamps a desired camera movement so the camera center remains within
    // the constraint polygon.
    //
    // The algorithm treats each edge as a half-space constraint:
    //  - Motion that moves further outside the constraint is removed.
    //  - Motion parallel to an edge is preserved (sliding).
    //  - Motion inward is always allowed.
    //
    // When two constraints are active simultaneously (corner case),
    // motion is fully blocked to avoid jitter.
    fn clamp_delta(&self, center: Vec2, mut delta: Vec2) -> Vec2 {
        let mut t_max: f32 = 1.0;
        let mut active_constraints: i32 = 0;

        for i in 0..self.diamond.len() {
            let point  = self.diamond[i];
            let normal = self.normals[i];

            // signed_distance > 0 -> strictly inside
            // signed_distance = 0 -> exactly on boundary
            // signed_distance < 0 -> already outside
            let signed_distance = (center - point).dot(normal);

            // normal_velocity > 0 -> motion is outward (toward violation)
            // normal_velocity < 0 -> motion is inward (safe)
            // normal_velocity = 0 -> motion parallel to edge
            let normal_velocity = delta.dot(normal);

            const ZERO_EPSILON:   f32 = 1e-6;
            const DIST_TOLERANCE: f32 = 1e-4;

            // Moving outward?
            if normal_velocity > ZERO_EPSILON {
                if signed_distance < DIST_TOLERANCE {
                    // Already outside -> project velocity (sliding).
                    delta -= normal * normal_velocity;
                    active_constraints += 1;
                } else {
                    // Inside -> clamp time of impact.
                    let t = signed_distance / normal_velocity;
                    if t >= 0.0 {
                        t_max = t_max.min(t);
                    }
                }
            }
        }

        // If we have more than one active constraint edge
        // it means we are at the intersection of two edges.
        // Clamp to zero now and prevent any further outwards
        // movement to avoid jittering.
        if active_constraints > 1 {
            t_max = 0.0;
        }

        if t_max <= 0.0 {
            Vec2::zero()
        } else {
            delta * t_max.clamp(0.0, 1.0)
        }
    }
}

fn draw_diamond(debug_draw: &mut dyn DebugDraw, diamond: &[Vec2; 4], edge_color: Color, normal_color: Color) {
    for i in 0..diamond.len() {
        let a = diamond[i];
        let b = diamond[(i + 1) % diamond.len()];

        let edge = b - a;
        let mid  = (a + b) * 0.5;
        let normal = inward_normal(edge);

        // Edge:
        debug_draw.line(a, b, edge_color, edge_color);

        // Normal (inward-facing):
        debug_draw.line(mid, mid + (normal * 40.0), normal_color, normal_color);

        // Point at each vertex:
        debug_draw.point(a, coords::DIAMOND_DEBUG_COLORS[i], 12.0);
    }
}

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

fn calc_scroll_delta(cursor_screen_pos: Vec2, viewport_size: Size, scroll_margin: f32) -> Vec2 {
    let mut scroll_delta = Vec2::zero();

    if cursor_screen_pos.x < scroll_margin {
        scroll_delta.x += 1.0;
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - scroll_margin {
        scroll_delta.x -= 1.0;
    }

    if cursor_screen_pos.y < scroll_margin {
        scroll_delta.y += 1.0;
    } else if cursor_screen_pos.y > (viewport_size.height as f32) - scroll_margin {
        scroll_delta.y -= 1.0;
    }

    scroll_delta
}

fn calc_scroll_speed(cursor_screen_pos: Vec2, viewport_size: Size, scroll_margin: f32, scroll_speed: f32) -> f32 {
    let edge_dist_x = if cursor_screen_pos.x < scroll_margin {
        scroll_margin - cursor_screen_pos.x
    } else if cursor_screen_pos.x > (viewport_size.width as f32) - scroll_margin {
        cursor_screen_pos.x - ((viewport_size.width as f32) - scroll_margin)
    } else {
        0.0
    };

    let edge_dist_y = if cursor_screen_pos.y < scroll_margin {
        scroll_margin - cursor_screen_pos.y
    } else if cursor_screen_pos.y > (viewport_size.height as f32) - scroll_margin {
        cursor_screen_pos.y - ((viewport_size.height as f32) - scroll_margin)
    } else {
        0.0
    };

    let max_edge_dist = edge_dist_x.max(edge_dist_y);
    let scroll_strength = (max_edge_dist / scroll_margin).clamp(0.0, 1.0);

    scroll_speed * scroll_strength
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
