#![allow(clippy::collapsible_else_if)]

use std::{iter::FusedIterator, ops::RangeInclusive};
use serde::{Deserialize, Serialize};

use super::{FieldAccessorXY, Rect, Size, Vec2, Color, constants::*};
use crate::field_accessor_xy;

// ----------------------------------------------
// IsoPoint
// ----------------------------------------------

// Isometric 2D point coords, in pure isometric space,
// before any WorldToScreenTransform are applied.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IsoPoint {
    pub x: i32,
    pub y: i32,
}

impl IsoPoint {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }
}

impl std::fmt::Display for IsoPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{},{}]", self.x, self.y)
    }
}

field_accessor_xy! { IsoPoint, i32, x, y }

// IsoPoint with fractional coords.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IsoPointF32(pub Vec2);

impl IsoPointF32 {
    #[inline]
    pub fn from_integer_iso(iso: IsoPoint) -> Self {
        Self(Vec2::new(iso.x as f32, iso.y as f32))
    }

    #[inline]
    pub fn to_integer_iso(self) -> IsoPoint {
        IsoPoint::new(self.0.x.floor() as i32, self.0.y.floor() as i32)
    }
}

// ----------------------------------------------
// Cell
// ----------------------------------------------

// X,Y position in the tile map grid of cells.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self { x: -1, y: -1 }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.x >= 0 && self.y >= 0
    }

    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }

    #[inline]
    pub fn manhattan_distance(self, other: Cell) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

impl Default for Cell {
    #[inline]
    fn default() -> Self {
        Cell::invalid()
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "[{},{}]", self.x, self.y)
        } else {
            write!(f, "[invalid]")
        }
    }
}

field_accessor_xy! { Cell, i32, x, y }

// Cell with fractional coords.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellF32(pub Vec2);

impl CellF32 {
    #[inline]
    pub fn from_integer_cell(cell: Cell) -> Self {
        Self(Vec2::new(cell.x as f32, cell.y as f32))
    }

    #[inline]
    pub fn to_integer_cell(self) -> Cell {
        Cell::new(self.0.x.floor() as i32, self.0.y.floor() as i32)
    }
}

// ----------------------------------------------
// CellRange
// ----------------------------------------------

#[derive(Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRange {
    // Inclusive rage, e.g.: [start..=end]
    pub start: Cell,
    pub end: Cell,
}

impl CellRange {
    #[inline]
    pub const fn new(start: Cell, end: Cell) -> Self {
        Self { start, end }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.start.is_valid()
        && self.end.is_valid()
        && self.start.x <= self.end.x
        && self.start.y <= self.end.y
    }

    #[inline]
    pub fn x_range(&self) -> RangeInclusive<i32> {
        self.start.x..=self.end.x
    }

    #[inline]
    pub fn y_range(&self) -> RangeInclusive<i32> {
        self.start.y..=self.end.y
    }

    #[inline]
    pub fn iter(&self) -> CellRangeIter<false> {
        CellRangeIter::new(*self)
    }

    #[inline]
    pub fn iter_rev(&self) -> CellRangeIter<true> {
        CellRangeIter::new(*self)
    }

    #[inline]
    pub fn contains(&self, cell: Cell) -> bool {
        if cell.x < self.start.x || cell.y < self.start.y {
            return false;
        }
        if cell.x > self.end.x || cell.y > self.end.y {
            return false;
        }
        true
    }

    #[inline]
    pub fn x(&self) -> i32 {
        self.start.x
    }

    #[inline]
    pub fn y(&self) -> i32 {
        self.start.y
    }

    #[inline]
    pub fn width(&self) -> i32 {
        self.end.x - self.start.x + 1
    }

    #[inline]
    pub fn height(&self) -> i32 {
        self.end.y - self.start.y + 1
    }

    #[inline]
    pub fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }
}

impl std::fmt::Display for CellRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{}; {}]", self.start, self.end)
    }
}

// ----------------------------------------------
// CellRangeIter
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct CellRangeIter<const REVERSED: bool> {
    range: CellRange,
    curr_y: i32,
    curr_x: i32,
    done: bool,
}

impl<const REVERSED: bool> CellRangeIter<REVERSED> {
    #[inline]
    pub fn new(range: CellRange) -> Self {
        let curr_y = if REVERSED { range.end.y } else { range.start.y };
        let curr_x = if REVERSED { range.end.x } else { range.start.x };
        Self { range, curr_y, curr_x, done: false }
    }
}

impl<const REVERSED: bool> Iterator for CellRangeIter<REVERSED> {
    type Item = Cell;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let result = Cell { x: self.curr_x, y: self.curr_y };

        // Determine next x,y:
        if REVERSED {
            if self.curr_x > self.range.start.x {
                self.curr_x -= 1;
            } else if self.curr_y > self.range.start.y {
                self.curr_y -= 1;
                self.curr_x = self.range.end.x;
            } else {
                self.done = true;
            }
        } else {
            if self.curr_x < self.range.end.x {
                self.curr_x += 1;
            } else if self.curr_y < self.range.end.y {
                self.curr_y += 1;
                self.curr_x = self.range.start.x;
            } else {
                self.done = true;
            }
        }

        Some(result)
    }
}

// Returns exactly how many elements are left.
// The Rust standard library can use this trait for iterator performance
// optimizations.
impl<const REVERSED: bool> ExactSizeIterator for CellRangeIter<REVERSED> {
    #[inline]
    fn len(&self) -> usize {
        let dx = (self.range.end.x - self.range.start.x + 1) as usize;
        let dy = (self.range.end.y - self.range.start.y + 1) as usize;
        let total = dx * dy;

        // Subtract how many were already yielded:
        let cur_index = if self.done {
            total
        } else {
            let y_offset = if REVERSED {
                (self.range.end.y - self.curr_y) as usize
            } else {
                (self.curr_y - self.range.start.y) as usize
            };

            let x_offset = if REVERSED {
                (self.range.end.x - self.curr_x) as usize
            } else {
                (self.curr_x - self.range.start.x) as usize
            };

            y_offset * dx + x_offset
        };

        total - cur_index
    }
}

// Guarantees next() always stays None after exhaustion.
impl<const REVERSED: bool> FusedIterator for CellRangeIter<REVERSED> {}

// Support for-each style iteration.
impl IntoIterator for &CellRange {
    type Item = Cell;
    type IntoIter = CellRangeIter<false>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// ----------------------------------------------
// WorldToScreenTransform
// ----------------------------------------------

// Transformations applied to a tile before rendering to screen.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct WorldToScreenTransform {
    pub scaling: f32, // Draw scaling. 1 = no scaling.
    pub offset: Vec2, // Screen-space offset (e.g. camera scroll).
}

impl WorldToScreenTransform {
    pub fn new(scaling: f32, offset: Vec2) -> Self {
        let transform = Self { scaling, offset };
        debug_assert!(transform.is_valid());
        transform
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.scaling > 0.0
    }

    #[inline]
    pub fn apply_to_iso_point(&self, iso_point: IsoPoint) -> Vec2 {
        // Apply offset and scaling:
        let screen_x = ((iso_point.x as f32) * self.scaling) + self.offset.x;
        let screen_y = ((iso_point.y as f32) * self.scaling) + self.offset.y;
        Vec2::new(screen_x, screen_y)
    }

    #[inline]
    pub fn apply_to_screen_point(&self, screen_point: Vec2) -> IsoPoint {
        // Remove offset and scaling:
        let iso_x = (screen_point.x - self.offset.x) / self.scaling;
        let iso_y = (screen_point.y - self.offset.y) / self.scaling;
        IsoPoint::new(iso_x as i32, iso_y as i32)
    }

    #[inline]
    pub fn apply_to_rect(&self, iso_position: IsoPoint, size: Size) -> Rect {
        let screen_position = self.apply_to_iso_point(iso_position);
        // Apply scaling:
        let screen_width  = (size.width  as f32) * self.scaling;
        let screen_height = (size.height as f32) * self.scaling;
        Rect::from_pos_and_size(screen_position, Vec2::new(screen_width, screen_height))
    }

    #[inline]
    pub fn scale_and_offset_rect(&self, rect: Rect) -> Rect {
        let x = rect.x() + (self.offset.x * self.scaling);
        let y = rect.y() + (self.offset.y * self.scaling);

        let width  = rect.width()  * self.scaling;
        let height = rect.height() * self.scaling;

        Rect::from_pos_and_size(Vec2::new(x, y), Vec2::new(width, height))
    }
}

impl Default for WorldToScreenTransform {
    fn default() -> Self {
        Self { scaling: 1.0, offset: Vec2::zero() }
    }
}

// +-----------------------------------------------+
// |     COORDINATE SPACE TRANSFORMS REFERENCE     |
// +-----------------------------------------------+
// | Operation           | Function                |
// | ------------------- | ----------------------- |
// | Cell -> Iso         | cell_to_iso()           |
// | Iso -> Cell         | iso_to_cell()           |
// | Iso -> Screen Rect  | iso_to_screen_rect()    |
// | Iso -> Screen Point | iso_to_screen_point()   |
// | Screen -> Iso Point | screen_to_iso_point()   |
// +-----------------------------------------------+

#[inline]
pub fn iso_to_cell(iso_point: IsoPoint) -> Cell {
    // Invert Y axis to match top-left origin.
    let cell_x = (( iso_point.x / HALF_BASE_TILE_WIDTH_I32)  + (-iso_point.y / HALF_BASE_TILE_HEIGHT_I32)) / 2;
    let cell_y = ((-iso_point.y / HALF_BASE_TILE_HEIGHT_I32) - ( iso_point.x / HALF_BASE_TILE_WIDTH_I32))  / 2;
    Cell::new(cell_x, cell_y)
}

#[inline]
pub fn cell_to_iso(cell: Cell) -> IsoPoint {
    let iso_x = (cell.x - cell.y) *  HALF_BASE_TILE_WIDTH_I32;
    let iso_y = (cell.x + cell.y) * -HALF_BASE_TILE_HEIGHT_I32; // flip Y (top-left origin)
    IsoPoint::new(iso_x, iso_y)
}

// Returns fractional (float) cell coords for a given iso point.
// NOTE: This is the same as coords::iso_to_cell() but using f32.
#[inline]
pub fn iso_to_cell_f32(iso_point: IsoPointF32) -> CellF32 {
    // Invert Y axis to match top-left origin.
    let cell_x = (( iso_point.0.x / HALF_BASE_TILE_WIDTH_F32)  + (-iso_point.0.y / HALF_BASE_TILE_HEIGHT_F32)) * 0.5;
    let cell_y = ((-iso_point.0.y / HALF_BASE_TILE_HEIGHT_F32) - ( iso_point.0.x / HALF_BASE_TILE_WIDTH_F32))  * 0.5;
    CellF32(Vec2::new(cell_x, cell_y))
}

// Returns fractional (float) iso coords for a given tile map cell.
// NOTE: This is the same as coords::cell_to_iso() but using f32.
#[inline]
pub fn cell_to_iso_f32(cell: CellF32) -> IsoPointF32 {
    let iso_x = (cell.0.x - cell.0.y) *  HALF_BASE_TILE_WIDTH_F32;
    let iso_y = (cell.0.x + cell.0.y) * -HALF_BASE_TILE_HEIGHT_F32; // flip Y (top-left origin)
    IsoPointF32(Vec2::new(iso_x, iso_y))
}

#[inline]
pub fn iso_to_screen_point(iso_point: IsoPoint, transform: WorldToScreenTransform) -> Vec2 {
    // Undo offsetting.
    let mut iso = iso_point;
    iso.x += HALF_BASE_TILE_WIDTH_I32;
    iso.y += HALF_BASE_TILE_HEIGHT_I32;

    transform.apply_to_iso_point(iso)
}

#[inline]
pub fn screen_to_iso_point(screen_point: Vec2, transform: WorldToScreenTransform) -> IsoPoint {
    let mut iso_pos = transform.apply_to_screen_point(screen_point);

    // Offset the iso point downward by half a tile
    // (visually centers the hit test to the tile center).
    iso_pos.x -= HALF_BASE_TILE_WIDTH_I32;
    iso_pos.y -= HALF_BASE_TILE_HEIGHT_I32;
    iso_pos
}

#[inline]
pub fn iso_to_screen_rect(iso_position: IsoPoint, size: Size, transform: WorldToScreenTransform) -> Rect {
    transform.apply_to_rect(iso_position, size)
}

// Same as iso_to_screen_rect() but the position is already in floating point.
#[inline]
pub fn iso_to_screen_rect_f32(iso_position: IsoPointF32, size: Size, transform: WorldToScreenTransform) -> Rect {
    // Apply offset and scaling:
    let screen_x = (iso_position.0.x * transform.scaling) + transform.offset.x;
    let screen_y = (iso_position.0.y * transform.scaling) + transform.offset.y;

    // Apply scaling:
    let screen_width  = (size.width  as f32) * transform.scaling;
    let screen_height = (size.height as f32) * transform.scaling;
    Rect::from_pos_and_size(Vec2::new(screen_x, screen_y), Vec2::new(screen_width, screen_height))
}

#[inline]
pub fn is_screen_point_inside_diamond(p: Vec2, points: &[Vec2; 4]) -> bool {
    // Triangle 1: top, right, bottom
    if is_screen_point_inside_triangle(p, points[0], points[1], points[2]) {
        return true;
    }
    // Triangle 2: bottom, left, top
    if is_screen_point_inside_triangle(p, points[2], points[3], points[0]) {
        return true;
    }
    false
}

// Test precisely if the screen point is inside the isometric cell diamond.
#[inline]
pub fn is_screen_point_inside_cell(screen_point: Vec2,
                                   cell: Cell,
                                   tile_size: Size,
                                   transform: WorldToScreenTransform)
                                   -> bool {
    let screen_points = cell_to_screen_diamond_points(cell, tile_size, transform);
    is_screen_point_inside_diamond(screen_point, &screen_points)
}

pub fn is_screen_point_inside_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    // Compute edge vectors of the triangle relative to vertex `a`:
    let v0 = c - a;
    let v1 = b - a;
    let v2 = point - a; // vector from `a` to point

    // Compute dot products between the edge vectors and vector to point:
    let dot00 = v0.dot(v0);
    let dot01 = v0.dot(v1);
    let dot02 = v0.dot(v2);
    let dot11 = v1.dot(v1);
    let dot12 = v1.dot(v2);

    // Compute the denominator of the barycentric coordinates formula:
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom == 0.0 {
        return false; // Degenerate triangle (zero area).
    }

    // Compute barycentric coordinates `u` and `v`:
    let inv_denom = 1.0 / denom;
    let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
    let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;

    // Check if point is inside the triangle:
    //  - If all weights are between 0 and 1 and their sum is <= 1, the point lies
    //    inside the triangle.
    u >= 0.0 && v >= 0.0 && (u + v) <= 1.0
}

// Creates an isometric-aligned diamond rectangle for the given tile size and cell location.
// Winding order of edges is Counter-Clockwise (CCW) in *screen space* (+Y points down, positive signed area).
pub fn cell_to_screen_diamond_points(cell: Cell,
                                     tile_size: Size,
                                     transform: WorldToScreenTransform)
                                     -> [Vec2; 4] {
    debug_assert!(transform.is_valid());

    let iso_center = cell_to_iso(cell);
    let screen_center = iso_to_screen_point(iso_center, transform);

    let tile_width  = (tile_size.width  as f32) * transform.scaling;
    let tile_height = (tile_size.height as f32) * transform.scaling;
    let base_height = BASE_TILE_HEIGHT_F32 * transform.scaling;

    let half_tile_w = tile_width  * 0.5;
    let half_tile_h = tile_height * 0.5;
    let half_base_h = base_height * 0.5;

    // Build 4 corners of the tile:
    let top    = Vec2::new(screen_center.x, screen_center.y - tile_height + half_base_h);
    let bottom = Vec2::new(screen_center.x, screen_center.y + half_base_h);
    let right  = Vec2::new(screen_center.x + half_tile_w, screen_center.y - half_tile_h + half_base_h);
    let left   = Vec2::new(screen_center.x - half_tile_w, screen_center.y - half_tile_h + half_base_h);

    [top, right, bottom, left]
}

// Simplified version of cell_to_screen_diamond_points() that only
// computes the left/right corner Y coord, used as tile sorting key.
pub fn cell_to_screen_diamond_center_y(cell: Cell,
                                       tile_size: Size,
                                       transform: WorldToScreenTransform)
                                       -> f32 {
    debug_assert!(transform.is_valid());

    let iso_center = cell_to_iso(cell);
    let screen_center = iso_to_screen_point(iso_center, transform);

    let tile_height = (tile_size.height as f32) * transform.scaling;
    let base_height = BASE_TILE_HEIGHT_F32 * transform.scaling;

    let half_tile_h = tile_height * 0.5;
    let half_base_h = base_height * 0.5;

    // Left or right diamond corner Y (same as cell_to_screen_diamond_points()[1 or 3].y).
    screen_center.y - half_tile_h + half_base_h
}

pub fn inner_rect_from_diamond_points(points: &[Vec2; 4]) -> Rect {
    let rect = Rect::from_points(points);

    let half_width  = rect.width()  * 0.5;
    let half_height = rect.height() * 0.5;

    let inner_rect = rect.shrunk(Vec2::new(half_width * 0.5, half_height * 0.5));

    debug_assert!(inner_rect.min.x < inner_rect.max.x &&
                  inner_rect.min.y < inner_rect.max.y,
                  "Invalid inner diamond rect!");

    inner_rect
}

// ----------------------------------------------
// IsoDiamond
// ----------------------------------------------

pub struct IsoDiamond {
    // Screen-space diamond points/vertices, CCW winding: [top, right, bottom, left]
    points: [Vec2; 4],
}

impl IsoDiamond {
    pub const TOP:    usize = 0;
    pub const RIGHT:  usize = 1;
    pub const BOTTOM: usize = 2;
    pub const LEFT:   usize = 3;

    pub const DEBUG_COLORS: [Color; 4] = [
        Color::white(),  // TOP
        Color::cyan(),   // RIGHT
        Color::yellow(), // BOTTOM
        Color::green(),  // LEFT
    ];

    pub fn from_screen_points(points: [Vec2; 4]) -> Self {
        Self { points }
    }

    pub fn from_cell(cell: Cell,
                     tile_size: Size,
                     transform: WorldToScreenTransform) -> Self {
        let points = cell_to_screen_diamond_points(cell, tile_size, transform);
        Self { points }
    }

    pub fn from_tile_map(map_size_in_cells: Size,
                         transform: WorldToScreenTransform) -> Self {
        let map_origin_cell = Cell::zero();
        let map_size_in_pixels = Size::new(
            map_size_in_cells.width  * BASE_TILE_SIZE_I32.width,
            map_size_in_cells.height * BASE_TILE_SIZE_I32.height,
        );

        let points = cell_to_screen_diamond_points(
            map_origin_cell,
            map_size_in_pixels,
            transform);

        Self { points }
    }

    #[inline]
    pub fn screen_points(&self) -> &[Vec2; 4] {
        &self.points
    }

    #[inline]
    pub fn screen_point(&self, index: usize) -> Vec2 {
        self.points[index]
    }

    #[inline]
    pub fn inner_rect(&self) -> Rect {
        inner_rect_from_diamond_points(&self.points)
    }

    pub fn area(&self) -> f32 {
        let mut area = 0.0;
        for i in 0..self.points.len() {
            let a = self.points[i];
            let b = self.points[(i + 1) % self.points.len()];
            area += (a.x * b.y) - (b.x * a.y);
        }
        area * 0.5
    }
}
