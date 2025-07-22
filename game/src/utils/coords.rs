use std::ops::RangeInclusive;
use std::iter::FusedIterator;

use super::{
    Vec2,
    Size,
    Rect
};

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
        Self { x: x, y: y }
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

// ----------------------------------------------
// Cell
// ----------------------------------------------

// X,Y position in the tile map grid of cells.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x: x, y: y }
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
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{},{}]", self.x, self.y)
    }
}

// ----------------------------------------------
// CellRange
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct CellRange {
    // Inclusive rage, e.g.: [start..=end]
    pub start: Cell,
    pub end: Cell,
}

impl CellRange {
    #[inline]
    pub const fn new(start: Cell, end: Cell) -> Self {
        Self {
            start: start,
            end: end,
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.start.is_valid() && self.end.is_valid() &&
        self.start.x <= self.end.x && self.start.y <= self.end.y
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
}

impl std::fmt::Display for CellRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{},{}; {},{}]",
               self.start.x,
               self.start.y,
               self.end.x,
               self.end.y)
    }
}

// ----------------------------------------------
// CellRangeIter
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct CellRangeIter<const REVERSED: bool> {
    range:  CellRange,
    curr_y: i32,
    curr_x: i32,
    done:   bool,
}

impl<const REVERSED: bool> CellRangeIter<REVERSED> {
    #[inline]
    pub fn new(range: CellRange) -> Self {
        let curr_y = if REVERSED { range.end.y } else { range.start.y };
        let curr_x = if REVERSED { range.end.x } else { range.start.x };
        Self {
            range,
            curr_y,
            curr_x,
            done: false,
        }
    }
}

impl<const REVERSED: bool> Iterator for CellRangeIter<REVERSED> {
    type Item = Cell;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let result = Cell {
            x: self.curr_x,
            y: self.curr_y,
        };

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
// The Rust standard library can use this trait for iterator performance optimizations.
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
#[derive(Copy, Clone, Debug)]
pub struct WorldToScreenTransform {
    pub scaling: f32, // Draw scaling. 1 = no scaling.
    pub offset: Vec2, // Screen-space offset (e.g. camera scroll).
}

impl WorldToScreenTransform {
    pub fn new(scaling: f32, offset: Vec2) -> Self {
        let transform = Self {
            scaling: scaling,
            offset: offset,
        };
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
        Rect::new(screen_position, Vec2::new(screen_width, screen_height))
    }

    #[inline]
    pub fn scale_and_offset_rect(&self, rect: Rect) -> Rect {
        let x = rect.x() + (self.offset.x * self.scaling);
        let y = rect.y() + (self.offset.y * self.scaling);

        let width  = rect.width()  * self.scaling;
        let height = rect.height() * self.scaling;

        Rect::new(Vec2::new(x, y), Vec2::new(width, height))
    }
}

impl Default for WorldToScreenTransform {
    fn default() -> Self {
        Self {
            scaling: 1.0,
            offset: Vec2::zero()
        }
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
pub fn iso_to_cell(iso_point: IsoPoint, tile_size: Size) -> Cell {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    // Invert Y axis to match top-left origin
    let cell_x = (( iso_point.x / half_tile_width)  + (-iso_point.y / half_tile_height)) / 2;
    let cell_y = ((-iso_point.y / half_tile_height) - ( iso_point.x / half_tile_width))  / 2;

    Cell::new(cell_x, cell_y)
}

#[inline]
pub fn cell_to_iso(cell: Cell, tile_size: Size) -> IsoPoint {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    let iso_x = (cell.x - cell.y) *  half_tile_width;
    let iso_y = (cell.x + cell.y) * -half_tile_height; // flip Y (top-left origin)

    IsoPoint::new(iso_x, iso_y)
}

#[inline]
pub fn iso_to_screen_point(iso_point: IsoPoint,
                           transform: &WorldToScreenTransform,
                           tile_size: Size) -> Vec2 {
    // Undo offsetting.
    let mut iso = iso_point;
    iso.x += tile_size.width  / 2;
    iso.y += tile_size.height / 2;

    transform.apply_to_iso_point(iso)
}

#[inline]
pub fn screen_to_iso_point(screen_point: Vec2,
                           transform: &WorldToScreenTransform,
                           tile_size: Size) -> IsoPoint {

    let mut iso_pos = transform.apply_to_screen_point(screen_point);

    // Offset the iso point downward by half a tile (visually centers the hit test to the tile center).
    iso_pos.x -= tile_size.width  / 2;
    iso_pos.y -= tile_size.height / 2;
    iso_pos
}

#[inline]
pub fn iso_to_screen_rect(iso_position: IsoPoint,
                          size: Size,
                          transform: &WorldToScreenTransform) -> Rect {

    transform.apply_to_rect(iso_position, size)
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
                                   base_tile_size: Size,
                                   transform: &WorldToScreenTransform) -> bool {

    debug_assert!(transform.is_valid());

    let screen_points = cell_to_screen_diamond_points(
        cell,
        tile_size,
        base_tile_size,
        transform);

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
    //  - If all weights are between 0 and 1 and their sum is <= 1, the point lies inside the triangle.
    u >= 0.0 && v >= 0.0 && (u + v) <= 1.0
}

// Creates an isometric-aligned diamond rectangle for the given tile size and cell location.
pub fn cell_to_screen_diamond_points(cell: Cell,
                                     tile_size: Size,
                                     base_tile_size: Size,
                                     transform: &WorldToScreenTransform) -> [Vec2; 4] {

    debug_assert!(transform.is_valid());

    let iso_center = cell_to_iso(cell, base_tile_size);
    let screen_center = iso_to_screen_point(iso_center, transform, base_tile_size);

    let tile_width  = (tile_size.width as f32) * transform.scaling;
    let tile_height = (tile_size.height as f32) * transform.scaling;
    let base_height = (base_tile_size.height as f32) * transform.scaling;

    let half_tile_w = tile_width  / 2.0;
    let half_tile_h = tile_height / 2.0;
    let half_base_h = base_height / 2.0;

    // Build 4 corners of the tile:
    let top    = Vec2::new(screen_center.x, screen_center.y - tile_height + half_base_h);
    let bottom = Vec2::new(screen_center.x, screen_center.y + half_base_h);
    let right  = Vec2::new(screen_center.x + half_tile_w, screen_center.y - half_tile_h + half_base_h);
    let left   = Vec2::new(screen_center.x - half_tile_w, screen_center.y - half_tile_h + half_base_h);

    [ top, right, bottom, left ]
}
