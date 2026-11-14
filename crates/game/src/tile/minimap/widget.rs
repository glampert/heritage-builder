// The minimap widget's layout and view maths.
//
// The map is drawn in the world's own isometric projection, scaled down: a point's position on
// the minimap is its position on screen at zoom 1 (its "world" position), scaled and moved. So
// the map's playable rectangle, which is 2:1 like the map area, fills the map area at zoom 1, the
// camera's view is an axis-aligned rectangle on the minimap, and picking is the same maths
// backwards.

use common::{
    Rect,
    Size,
    Vec2,
    constants::{HALF_BASE_TILE_HEIGHT_F32, HALF_BASE_TILE_WIDTH_F32},
    coords::{CellF32, IsoDiamond, IsoPointF32, WorldToScreenTransform},
    time::Seconds,
};

use crate::camera::Camera;

// ----------------------------------------------
// Layout
// ----------------------------------------------

// The widget, as in the HUD mockups (docs/design/mockups, `widget_layout` in minimap.py): lens
// buttons in a row across the top, the map under them, the tools in a column on the right. The
// map's size comes from the style; while a lens is open, its legend gets rows under the map and
// the widget grows to fit them.
pub const BUTTON_SIZE: f32 = 28.0;
pub const GAP: f32 = 8.0;
pub const PADDING: f32 = 12.0;
pub const TOOL_MIN_SPACING: f32 = 3.0;
pub const LEGEND_ROW_HEIGHT: f32 = 14.0;
pub const LEGEND_COLUMN_WIDTH: f32 = 75.0;

// The room round the minimap's contents (left, top, right, bottom points: its window's frame and
// padding) and between its window and the screen's edges: the developer UI's plain window, or the
// retail HUD's panel.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MinimapFrame {
    pub insets: [f32; 4],
    pub margin: f32,
}

impl Default for MinimapFrame {
    fn default() -> Self {
        Self { insets: [PADDING; 4], margin: 0.0 }
    }
}

#[derive(Copy, Clone, Default)]
pub struct MinimapLayout {
    pub window: Rect,   // The whole widget, in window points.
    pub map: Rect,      // The map area: also its clip rect.
    pub lens_row: Rect, // The top row: lens buttons and toggles, right-aligned against the tools.
    pub tools: Rect,    // Where the tool buttons go, top to bottom.
    pub legend: Rect,   // The lens legend's rows, under the map; empty without a lens.
    pub tool_spacing: f32,
    pub legend_columns: usize,
    pub legend_scale: f32, // Of the legend's rows, columns and swatches (1 at the default text size).
}

impl MinimapLayout {
    // Legend entries per row for a map `map_width` points wide.
    pub fn legend_columns(map_width: f32, legend_scale: f32) -> usize {
        ((map_width / (LEGEND_COLUMN_WIDTH * legend_scale)).floor() as usize).max(1)
    }

    // In the bottom left corner of a screen of `screen_size` points, with `tool_count` tools, a map
    // of `map_size` points and `legend_entries` legend entries (0 without a lens) at `legend_scale`.
    pub fn new(screen_size: Vec2, tool_count: usize, map_size: Vec2, legend_entries: usize, legend_scale: f32, frame: MinimapFrame) -> Self {
        let [left, top, right, bottom] = frame.insets;
        let legend_columns = Self::legend_columns(map_size.x, legend_scale);
        let legend_rows = legend_entries.div_ceil(legend_columns);
        let legend_height = if legend_rows > 0 { GAP + legend_rows as f32 * LEGEND_ROW_HEIGHT * legend_scale } else { 0.0 };

        // The tools take the lens row and the map's height, or more if they don't fit in it.
        let tools_height = (BUTTON_SIZE + GAP + map_size.y)
            .max(BUTTON_SIZE * tool_count as f32 + TOOL_MIN_SPACING * tool_count.saturating_sub(1) as f32);
        let inner_height = tools_height.max(BUTTON_SIZE + GAP + map_size.y + legend_height);
        let size = Vec2::new(left + right + map_size.x + GAP + BUTTON_SIZE, top + bottom + inner_height);

        let window = Rect::from_pos_and_size(Vec2::new(frame.margin, screen_size.y - size.y - frame.margin), size);
        let inner_min = window.min + Vec2::new(left, top);
        let lens_row = Rect::from_pos_and_size(inner_min, Vec2::new(map_size.x, BUTTON_SIZE));
        let map = Rect::from_pos_and_size(inner_min + Vec2::new(0.0, BUTTON_SIZE + GAP), map_size);
        let legend = Rect::from_pos_and_size(Vec2::new(map.min.x, map.max.y + GAP), Vec2::new(map_size.x, legend_height - GAP).max(Vec2::zero()));
        let tools = Rect::from_pos_and_size(Vec2::new(map.max.x + GAP, inner_min.y), Vec2::new(BUTTON_SIZE, tools_height));
        let tool_spacing = if tool_count > 1 {
            ((tools_height - BUTTON_SIZE * tool_count as f32) / (tool_count - 1) as f32).max(0.0)
        } else {
            0.0
        };
        Self { window, map, lens_row, tools, legend, tool_spacing, legend_columns, legend_scale }
    }
}

// ----------------------------------------------
// World positions
// ----------------------------------------------

// A cell's position on screen at zoom 1 with no scroll: its world position. Cell centres are at
// whole cell coordinates.
#[inline]
pub fn cell_to_world(cell: Vec2) -> Vec2 {
    Vec2::new((cell.x - cell.y) * HALF_BASE_TILE_WIDTH_F32 + HALF_BASE_TILE_WIDTH_F32, -(cell.x + cell.y) * HALF_BASE_TILE_HEIGHT_F32 + HALF_BASE_TILE_HEIGHT_F32)
}

#[inline]
pub fn world_to_cell(world: Vec2) -> Vec2 {
    let a = (world.x - HALF_BASE_TILE_WIDTH_F32) / HALF_BASE_TILE_WIDTH_F32; // x - y
    let b = -(world.y - HALF_BASE_TILE_HEIGHT_F32) / HALF_BASE_TILE_HEIGHT_F32; // x + y
    Vec2::new((a + b) * 0.5, (b - a) * 0.5)
}

// The iso point the camera centres on for a world position.
#[inline]
pub fn world_to_iso(world: Vec2) -> IsoPointF32 {
    IsoPointF32(world - Vec2::new(HALF_BASE_TILE_WIDTH_F32, HALF_BASE_TILE_HEIGHT_F32))
}

// ----------------------------------------------
// CellToMinimap
// ----------------------------------------------

// The affine map from cell coordinates to minimap points for one frame: p = m · cell + t.
#[derive(Copy, Clone, Default, Debug)]
pub struct CellToMinimap {
    pub m: [f32; 4], // Row-major 2 × 2.
    pub t: Vec2,
}

impl CellToMinimap {
    #[inline]
    pub fn apply(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(self.m[0] * x + self.m[1] * y + self.t.x, self.m[2] * x + self.m[3] * y + self.t.y)
    }

    #[inline]
    pub fn apply_vec(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(self.m[0] * x + self.m[1] * y, self.m[2] * x + self.m[3] * y)
    }

    pub fn inverse(&self, p: Vec2) -> Vec2 {
        let det = self.m[0] * self.m[3] - self.m[1] * self.m[2];
        let q = p - self.t;
        Vec2::new((self.m[3] * q.x - self.m[1] * q.y) / det, (-self.m[2] * q.x + self.m[0] * q.y) / det)
    }

    // Points per cell along a cell's side.
    pub fn cell_side(&self) -> f32 {
        (self.m[0] * self.m[0] + self.m[2] * self.m[2]).sqrt()
    }
}

// ----------------------------------------------
// MinimapView
// ----------------------------------------------

pub const ZOOM_STEP: f32 = 1.5;
const AUTO_SCROLL_MARGIN: f32 = 0.12; // Of the view, kept between the camera's view and its edges.

pub struct MinimapView {
    pub zoom: f32,                  // 1: the playable area fills the map. More zooms in.
    pub center: Vec2,               // Centre of the view, in world points.
    pub auto_zoom: bool,            // Zoom so about `desired_visible_cells` show across.
    pub auto_scroll: bool,          // Scroll to keep the camera's view on the map.
    pub scroll_speed: f32,          // How fast auto scroll catches up, per second.
    pub desired_visible_cells: f32,

    // Recomputed every update:
    pub map_rect: Rect,       // The map area on screen.
    pub playable: Rect,       // The playable area in world points.
    pub camera_rect: Rect,    // The camera's view on the minimap, clamped to the map area.
    pub cursor_pos: Vec2,
    pub map_size: Size,
}

impl Default for MinimapView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center: Vec2::zero(),
            auto_zoom: true,
            auto_scroll: true,
            scroll_speed: 6.0,
            desired_visible_cells: 95.0,
            map_rect: Rect::default(),
            playable: Rect::default(),
            camera_rect: Rect::default(),
            cursor_pos: Vec2::zero(),
            map_size: Size::zero(),
        }
    }
}

impl MinimapView {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_valid(&self) -> bool {
        self.map_rect.is_valid() && self.playable.is_valid() && self.map_size.is_valid()
    }

    // Zoom limits: the whole playable area, down to about 16 cells across.
    pub fn max_zoom(&self) -> f32 {
        (self.map_size.width.max(self.map_size.height) as f32 / 16.0).max(1.0)
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(1.0, self.max_zoom());
        self.clamp_center();
    }

    // Zooms in (steps > 0) or out by ZOOM_STEP per step, and stops the auto zoom.
    pub fn zoom_by(&mut self, steps: i32) {
        self.auto_zoom = false;
        self.set_zoom(self.zoom * ZOOM_STEP.powi(steps));
    }

    // About how many cells show across the map at the current zoom.
    pub fn visible_cells(&self) -> f32 {
        self.map_size.width.max(self.map_size.height) as f32 / self.zoom
    }

    // Points per world point.
    pub fn scale(&self) -> f32 {
        let window = self.window_size();
        (self.map_rect.width() / window.x).min(self.map_rect.height() / window.y)
    }

    // The part of the world the map shows, in world points: the largest rectangle of the map
    // area's shape inside the playable area, at zoom 1.
    pub fn window_size(&self) -> Vec2 {
        let full = self.playable.size();
        let map = self.map_rect.size();
        if map.x <= 0.0 || map.y <= 0.0 || full.y <= 0.0 {
            return full / self.zoom;
        }
        let aspect = map.x / map.y;
        let fitted = if full.x / full.y > aspect { Vec2::new(full.y * aspect, full.y) } else { Vec2::new(full.x, full.x / aspect) };
        fitted / self.zoom
    }

    pub fn cell_to_minimap(&self) -> CellToMinimap {
        let k = self.scale();
        let center = self.map_rect.center();
        let w = HALF_BASE_TILE_WIDTH_F32 * k;
        let h = HALF_BASE_TILE_HEIGHT_F32 * k;
        let origin = cell_to_world(Vec2::zero());
        CellToMinimap { m: [w, -w, -h, -h], t: center + (origin - self.center) * k }
    }

    #[inline]
    pub fn world_to_minimap(&self, world: Vec2) -> Vec2 {
        self.map_rect.center() + (world - self.center) * self.scale()
    }

    #[inline]
    pub fn minimap_to_world(&self, point: Vec2) -> Vec2 {
        self.center + (point - self.map_rect.center()) / self.scale()
    }

    // The cell under a point of the minimap, or None off the map area or the playable area.
    pub fn pick_cell(&self, point: Vec2) -> Option<CellF32> {
        if !self.is_valid() || !self.map_rect.contains_point(point) {
            return None;
        }
        let world = self.minimap_to_world(point);
        if !self.playable.contains_point(world) {
            return None;
        }
        Some(CellF32(world_to_cell(world)))
    }

    // Centres the view on a cell, as far as the playable area allows.
    pub fn center_on_cell(&mut self, cell: Vec2) {
        self.center = cell_to_world(cell);
        self.clamp_center();
    }

    pub fn update(&mut self, camera: &mut Camera, map_rect: Rect, map_size: Size, cursor_pos: Vec2, delta_time_secs: Seconds) {
        let first_update = !self.is_valid() || self.map_size != map_size;

        self.map_rect = map_rect;
        self.map_size = map_size;
        self.cursor_pos = cursor_pos;
        self.playable = IsoDiamond::from_tile_map(map_size, WorldToScreenTransform::default()).inner_rect();

        if first_update {
            self.center = self.playable.center();
        }

        if self.auto_zoom {
            let full_cells = map_size.width.max(map_size.height) as f32;
            self.zoom = (full_cells / self.desired_visible_cells.max(1.0)).clamp(1.0, self.max_zoom());
        }
        self.zoom = self.zoom.clamp(1.0, self.max_zoom());

        let camera_world = camera_world_rect(camera);

        if self.auto_scroll && self.zoom > 1.0 {
            if first_update {
                self.center = camera_world.center();
            } else {
                self.follow(camera_world, delta_time_secs);
            }
        }
        self.clamp_center();

        let mut camera_rect = Rect::from_extents(self.world_to_minimap(camera_world.min), self.world_to_minimap(camera_world.max));
        self.camera_rect = *camera_rect.clamp(&map_rect.shrunk(Vec2::one()));
    }

    // Moves the view towards the smallest shift that puts the camera's view, plus a margin, on
    // the map.
    fn follow(&mut self, camera_world: Rect, delta_time_secs: Seconds) {
        let window = self.window_size();
        let margin = window * AUTO_SCROLL_MARGIN;
        let half = window * 0.5;

        let shift_axis = |center: f32, half: f32, margin: f32, min: f32, max: f32| -> f32 {
            let (low, high) = (center - half + margin, center + half - margin);
            if max - min > high - low {
                // Wider than the view: centre on it.
                (min + max) * 0.5 - center
            } else if min < low {
                min - low
            } else if max > high {
                max - high
            } else {
                0.0
            }
        };

        let shift = Vec2::new(
            shift_axis(self.center.x, half.x, margin.x, camera_world.min.x, camera_world.max.x),
            shift_axis(self.center.y, half.y, margin.y, camera_world.min.y, camera_world.max.y),
        );
        let t = (self.scroll_speed * delta_time_secs).clamp(0.0, 1.0);
        self.center += shift * t;
    }

    fn clamp_center(&mut self) {
        if !self.playable.is_valid() {
            return;
        }
        let half = self.window_size() * 0.5;
        let min = self.playable.min + half;
        let max = self.playable.max - half;
        self.center.x = if min.x <= max.x { self.center.x.clamp(min.x, max.x) } else { self.playable.center().x };
        self.center.y = if min.y <= max.y { self.center.y.clamp(min.y, max.y) } else { self.playable.center().y };
    }
}

// The camera's view in world points.
pub fn camera_world_rect(camera: &Camera) -> Rect {
    let transform = camera.transform();
    let screen = camera.camera_screen_rect();
    let to_world = |p: Vec2| transform.apply_to_screen_point(p).0;
    Rect::from_extents(to_world(screen.min), to_world(screen.max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_cell_round_trip() {
        for (x, y) in [(0.0, 0.0), (3.5, -2.0), (40.0, 17.25)] {
            let cell = Vec2::new(x, y);
            let back = world_to_cell(cell_to_world(cell));
            assert!((back.x - x).abs() < 1e-4 && (back.y - y).abs() < 1e-4);
        }
    }

    #[test]
    fn cell_to_minimap_matches_world_to_minimap() {
        let mut view = MinimapView { map_rect: Rect::from_pos_and_size(Vec2::new(10.0, 20.0), Vec2::new(300.0, 150.0)), map_size: Size::new(64, 64), ..Default::default() };
        view.playable = IsoDiamond::from_tile_map(view.map_size, WorldToScreenTransform::default()).inner_rect();
        view.center = view.playable.center();
        view.zoom = 1.3;
        let transform = view.cell_to_minimap();
        for (x, y) in [(0.0, 0.0), (31.5, 31.5), (10.0, 50.0)] {
            let a = transform.apply(x, y);
            let b = view.world_to_minimap(cell_to_world(Vec2::new(x, y)));
            assert!((a - b).length() < 1e-3, "{a:?} != {b:?}");
            let back = transform.inverse(a);
            assert!((back.x - x).abs() < 1e-3 && (back.y - y).abs() < 1e-3);
        }
    }

    #[test]
    fn playable_area_fills_the_map_at_zoom_one() {
        let mut view = MinimapView { map_rect: Rect::from_pos_and_size(Vec2::zero(), Vec2::new(300.0, 150.0)), map_size: Size::new(80, 80), ..Default::default() };
        view.playable = IsoDiamond::from_tile_map(view.map_size, WorldToScreenTransform::default()).inner_rect();
        view.center = view.playable.center();
        let min = view.world_to_minimap(view.playable.min);
        let max = view.world_to_minimap(view.playable.max);
        assert!(min.x.abs() < 1e-3 && min.y.abs() < 1e-3, "{min:?}");
        assert!((max.x - 300.0).abs() < 1e-3 && (max.y - 150.0).abs() < 1e-3, "{max:?}");
    }
}
