use std::ops::{Add, AddAssign, Sub, SubAssign, Mul, MulAssign, Div, DivAssign};
use std::time::{self};
use serde::Deserialize;

pub mod hash;
pub mod file_sys;

// ----------------------------------------------
// Macros
// ----------------------------------------------

#[macro_export]
macro_rules! name_of {
    ($t:ty, $field:ident) => {{
        // Access the field to force a compile-time check for the field existence.
        let _ = |x: &$t| { let _ = &x.$field; };
        stringify!($field)
    }};
}

// ----------------------------------------------
// Vec2
// ----------------------------------------------

// 2D screen space vector or point (f32).
// For interfacing with shaders and the rendering system.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self {
            x: x,
            y: y,
        }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
        }
    }

    #[inline]
    #[must_use]
    pub fn dot(&self, other: Self) -> f32 {
        (self.x * other.x) + (self.y * other.y)
    }

    #[inline]
    #[must_use]
    pub fn length_squared(&self) -> f32 {
        self.dot(*self)
    }

    #[inline]
    #[must_use]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    #[must_use]
    pub fn normalize(&self) -> Self {
        let inv_len = 1.0 / self.length();
        Self {
            x: self.x * inv_len,
            y: self.y * inv_len,
        }
    }
}

// Vec2 + Vec2
impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

// Vec2 += Vec2
impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Vec2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

// Vec2 - Vec2
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

// Vec2 -= Vec2
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Vec2) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

// Vec2 * f32
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f32) -> Vec2 {
        Vec2 {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

// f32 * Vec2
impl Mul<Vec2> for f32 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self * rhs.x,
            y: self * rhs.y,
        }
    }
}

// Vec2 *= f32
impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

// Vec2 * Vec2
impl Mul for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

// Vec2 *= Vec2
impl MulAssign for Vec2 {
    fn mul_assign(&mut self, rhs: Vec2) {
        self.x *= rhs.x;
        self.y *= rhs.y;
    }
}

// Vec2 / f32
impl Div<f32> for Vec2 {
    type Output = Vec2;
    fn div(self, rhs: f32) -> Vec2 {
        Vec2 {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

// f32 / Vec2
impl Div<Vec2> for f32 {
    type Output = Vec2;
    fn div(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self / rhs.x,
            y: self / rhs.y,
        }
    }
}

// Vec2 /= f32
impl DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

// Vec2 / Vec2
impl Div for Vec2 {
    type Output = Vec2;
    fn div(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

// Vec2 /= Vec2
impl DivAssign for Vec2 {
    fn div_assign(&mut self, rhs: Vec2) {
        self.x /= rhs.x;
        self.y /= rhs.y;
    }
}

// ----------------------------------------------
// Color
// ----------------------------------------------

// Normalized RGBA color (f32, [0,1] range).
// For interfacing with shaders and the rendering system.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r: r, g: g, b: b, a: a }
    }

    #[inline]
    pub const fn to_array(&self) -> [f32; 4] {
        [ self.r, self.g, self.b, self.a ]
    }

    #[inline] pub const fn white()   -> Self { Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
    #[inline] pub const fn black()   -> Self { Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    #[inline] pub const fn red()     -> Self { Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 } }
    #[inline] pub const fn green()   -> Self { Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 } }
    #[inline] pub const fn blue()    -> Self { Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 } }
    #[inline] pub const fn yellow()  -> Self { Self { r: 1.0, g: 1.0, b: 0.0, a: 1.0 } }
    #[inline] pub const fn cyan()    -> Self { Self { r: 0.0, g: 1.0, b: 1.0, a: 1.0 } }
    #[inline] pub const fn magenta() -> Self { Self { r: 1.0, g: 0.0, b: 1.0, a: 1.0 } }
    #[inline] pub const fn gray()    -> Self { Self { r: 0.7, g: 0.7, b: 0.7, a: 1.0 } }
}

impl Default for Color {
    fn default() -> Self { Color::white() }
}

// Color * Color
impl Mul for Color {
    type Output = Color;
    fn mul(self, rhs: Color) -> Color {
        Color {
            r: (self.r * rhs.r).min(1.0),
            g: (self.g * rhs.g).min(1.0),
            b: (self.b * rhs.b).min(1.0),
            a: (self.a * rhs.a).min(1.0),
        }
    }
}

// Color *= Color
impl MulAssign for Color {
    fn mul_assign(&mut self, rhs: Color) {
        self.r = (self.r * rhs.r).min(1.0);
        self.g = (self.g * rhs.g).min(1.0);
        self.b = (self.b * rhs.b).min(1.0);
        self.a = (self.a * rhs.a).min(1.0);
    }
}

// Color * f32
impl Mul<f32> for Color {
    type Output = Color;
    fn mul(self, rhs: f32) -> Color {
        Color {
            r: (self.r * rhs).min(1.0),
            g: (self.g * rhs).min(1.0),
            b: (self.b * rhs).min(1.0),
            a: (self.a * rhs).min(1.0),
        }
    }
}

// f32 * Color
impl Mul<Color> for f32 {
    type Output = Color;
    fn mul(self, rhs: Color) -> Color {
        Color {
            r: (self * rhs.r).min(1.0),
            g: (self * rhs.g).min(1.0),
            b: (self * rhs.b).min(1.0),
            a: (self * rhs.a).min(1.0),
        }
    }
}

// Color *= f32
impl MulAssign<f32> for Color {
    fn mul_assign(&mut self, rhs: f32) {
        self.r = (self.r * rhs).min(1.0);
        self.g = (self.g * rhs).min(1.0);
        self.b = (self.b * rhs).min(1.0);
        self.a = (self.a * rhs).min(1.0);
    }
}

// ----------------------------------------------
// Size
// ----------------------------------------------

// Integer width & height pair.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Size {
    pub width:  i32,
    pub height: i32,
}

impl Size {
    #[inline]
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width: width, height: height }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { width: 0, height: 0 }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    #[inline]
    pub fn to_vec2(&self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }
}

// ----------------------------------------------
// IsoPoint
// ----------------------------------------------

// Isometric 2D point coords, in pure isometric space,
// before any WorldToScreenTransform are applied.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
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
    pub fn to_vec2(&self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }
}

// ----------------------------------------------
// Cell
// ----------------------------------------------

// X,Y position in the tile map grid of cells.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
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

// ----------------------------------------------
// Rect
// ----------------------------------------------

// Screen space rectangle defined by min and max extents (f32).
// `min` is the top-left corner and `max` is the bottom-right.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    #[inline]
    pub const fn new(pos: Vec2, size: Vec2) -> Self {
        Self {
            min: pos,
            max: Vec2::new(pos.x + size.x, pos.y + size.y),
        }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            min: Vec2::zero(),
            max: Vec2::zero(),
        }
    }

    #[inline]
    pub fn from_pos_and_size(pos: Vec2, size: Size) -> Self {
        Self {
            min: pos,
            max: Vec2::new(pos.x + (size.width as f32), pos.y + (size.height as f32)),
        }
    }

    #[inline]
    pub fn from_extents(a: Vec2, b: Vec2) -> Self {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        Self {
            min: Vec2::new(min_x, min_y),
            max: Vec2::new(max_x, max_y),
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.width() > 0.0 && self.height() > 0.0
    }

    #[inline]
    pub fn x(&self) -> f32 {
        self.min.x
    }

    #[inline]
    pub fn y(&self) -> f32 {
        self.min.y
    }

    #[inline]
    pub fn position(&self) -> Vec2 {
        Vec2::new(self.x(), self.y())
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    #[inline]
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    #[inline]
    pub fn center(&self) -> Vec2 {
        Vec2::new(
            self.x() + self.width()  / 2.0,
            self.y() + self.height() / 2.0)
    }

    #[inline]
    pub fn size(&self) -> Size {
        Size::new(self.width() as i32, self.height() as i32)
    }

    #[inline]
    pub fn size_as_vec2(&self) -> Vec2 {
        Vec2::new(self.width(), self.height())
    }

    #[inline]
    pub fn canonicalize(&mut self) {
        if self.min.x > self.max.x {
            std::mem::swap(&mut self.min.x, &mut self.max.x);
        }
        if self.min.y > self.max.y {
            std::mem::swap(&mut self.min.y, &mut self.max.y);
        }
    }

    // Flips min/max bounds if needed.
    #[inline]
    pub fn update_min_extent(&mut self, new_min: Vec2) {
        self.min = new_min;
        self.canonicalize();
    }

    #[inline]
    pub fn update_max_extent(&mut self, new_max: Vec2) {
        self.max = new_max;
        self.canonicalize();
    }

    // Returns `true` if this rect intersects with another.
    #[inline]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x < other.max.x &&
        self.max.x > other.min.x &&
        self.min.y < other.max.y &&
        self.max.y > other.min.y
    }

    // Returns `true` if the point is inside this rect (inclusive of mins, exclusive of maxs).
    #[inline]
    pub fn contains_point(&self, point: Vec2) -> bool {
        point.x >= self.min.x &&
        point.x <  self.max.x &&
        point.y >= self.min.y &&
        point.y <  self.max.y
    }

    // Returns `true` if this rect fully contains the other rect.
    #[inline]
    pub fn contains_rect(&self, other: &Rect) -> bool {
        self.min.x <= other.min.x &&
        self.max.x >= other.max.x &&
        self.min.y <= other.min.y &&
        self.max.y >= other.max.y
    }

    //
    // NOTE: Top-left is the origin.
    //

    #[inline]
    pub fn top_left(&self) -> Vec2 {
        Vec2::new(self.x(), self.y() + self.height())
    }

    #[inline]
    pub fn bottom_left(&self) -> Vec2 {
        Vec2::new(self.x(), self.y())
    }

    #[inline]
    pub fn top_right(&self) -> Vec2 {
        Vec2::new(self.x() + self.width(), self.y() + self.height())
    }

    #[inline]
    pub fn bottom_right(&self) -> Vec2 {
        Vec2::new(self.x() + self.width(), self.y())
    }
}

// ----------------------------------------------
// RectTexCoords
// ----------------------------------------------

#[derive(Copy, Clone, Debug)]
pub struct RectTexCoords {
    pub coords: [Vec2; 4],
}

impl RectTexCoords {
    #[inline]
    pub const fn new(coords: [Vec2; 4]) -> Self {
        Self { coords: coords }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            coords: [Vec2::zero(); 4],
        }
    }

    // NOTE: This needs to be const for static declarations, so we don't derive from Default.
    #[inline]
    pub const fn default() -> &'static Self {
        static DEFAULT: RectTexCoords = RectTexCoords::new(
            [
                Vec2::new(0.0, 0.0), // top_left
                Vec2::new(0.0, 1.0), // bottom_left
                Vec2::new(1.0, 0.0), // top_right
                Vec2::new(1.0, 1.0), // bottom_right
            ]);
        &DEFAULT
    }

    //
    // NOTE: Top-left is the origin.
    //

    #[inline]
    pub fn top_left(&self) -> Vec2 {
        self.coords[0]
    }

    #[inline]
    pub fn bottom_left(&self) -> Vec2 {
        self.coords[1]
    }

    #[inline]
    pub fn top_right(&self) -> Vec2 {
        self.coords[2]
    }

    #[inline]
    pub fn bottom_right(&self) -> Vec2 {
        self.coords[3]
    }
}

// ----------------------------------------------
// WorldToScreenTransform
// ----------------------------------------------

// Transformations applied to a tile before rendering to screen.
#[derive(Copy, Clone, Debug)]
pub struct WorldToScreenTransform {
    pub scaling: f32,      // Draw scaling. 1 = no scaling.
    pub offset: Vec2,      // Screen-space offset (e.g. camera scroll).
    pub tile_spacing: f32, // Pixels between tiles (0 = tight fit).
}

impl WorldToScreenTransform {
    pub fn new(scaling: f32, offset: Vec2, tile_spacing: f32) -> Self {
        let transform = Self {
            scaling: scaling,
            offset: offset,
            tile_spacing: tile_spacing,
        };
        debug_assert!(transform.is_valid());
        transform
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.scaling > 0.0 && self.tile_spacing >= 0.0
    }

    #[inline]
    pub fn apply_to_iso_point(&self, iso_point: IsoPoint, apply_spacing: bool) -> Vec2 {
        let half_spacing = if apply_spacing { self.tile_spacing / 2.0 } else { 0.0 };

        // Apply spacing, offset and scaling:
        let screen_x = (((iso_point.x as f32) + half_spacing) * self.scaling) + self.offset.x;
        let screen_y = (((iso_point.y as f32) + half_spacing) * self.scaling) + self.offset.y;

        Vec2::new(screen_x, screen_y)
    }

    #[inline]
    pub fn apply_to_screen_point(&self, screen_point: Vec2, apply_spacing: bool) -> IsoPoint {
        let half_spacing = if apply_spacing { self.tile_spacing / 2.0 } else { 0.0 };

        // Remove spacing, offset and scaling:
        let iso_x = ((screen_point.x - self.offset.x) / self.scaling) - half_spacing;
        let iso_y = ((screen_point.y - self.offset.y) / self.scaling) - half_spacing;

        IsoPoint::new(iso_x as i32, iso_y as i32)
    }

    #[inline]
    pub fn apply_to_rect(&self, iso_position: IsoPoint, size: Size, apply_spacing: bool) -> Rect {
        let tile_spacing = if apply_spacing { self.tile_spacing } else { 0.0 };
        let screen_position = self.apply_to_iso_point(iso_position, true);

        // Shrink size by spacing and apply scaling:
        let screen_width  = ((size.width  as f32) - tile_spacing) * self.scaling;
        let screen_height = ((size.height as f32) - tile_spacing) * self.scaling;

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
            offset: Vec2::zero(),
            tile_spacing: 0.0
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
                           tile_size: Size,
                           apply_spacing: bool) -> Vec2 {
    // Undo offsetting.
    let mut iso = iso_point;
    iso.x += tile_size.width  / 2;
    iso.y += tile_size.height / 2;

    transform.apply_to_iso_point(iso, apply_spacing)
}

#[inline]
pub fn screen_to_iso_point(screen_point: Vec2,
                           transform: &WorldToScreenTransform,
                           tile_size: Size,
                           apply_spacing: bool) -> IsoPoint {

    let mut iso_pos = transform.apply_to_screen_point(screen_point, apply_spacing);

    // Offset the iso point downward by half a tile (visually centers the hit test to the tile center).
    iso_pos.x -= tile_size.width  / 2;
    iso_pos.y -= tile_size.height / 2;
    iso_pos
}

#[inline]
pub fn iso_to_screen_rect(iso_position: IsoPoint,
                          size: Size,
                          transform: &WorldToScreenTransform,
                          apply_spacing: bool) -> Rect {

    transform.apply_to_rect(iso_position, size, apply_spacing)
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
    let screen_center = iso_to_screen_point(iso_center, transform, base_tile_size, false);

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

// Maps a value in the numerical range [in_min, in_max] to the range [out_min, out_max].
#[inline]
pub fn map_value_to_range<T>(val: T, in_min: T, in_max: T, out_min: T, out_max: T) -> T
    where
        T: Sub<Output = T> + Div<Output = T> + Mul<Output = T> + Add<Output = T> + PartialEq + Copy
{
    if in_min == in_max {
        val // Would cause a division by zero in in_max - in_min, so just return back the input.
    } else {
        (val - in_min) * (out_max - out_min) / (in_max - in_min) + out_min
    }
}

// Maps a scalar value within a range to the [0,1] normalized range.
#[inline]
pub fn normalize_value<T>(val: T, minimum: T, maximum: T) -> T 
    where
        T: Sub<Output = T> + Div<Output = T> + Copy
{
    (val - minimum) / (maximum - minimum)
}

// Linear interpolation.
#[inline]
pub fn lerp<T>(a: T, b: T, t: f32) -> T
    where
        T: Mul<f32, Output = T> + Add<Output = T> + Copy,
        f32: Mul<T, Output = T> // for (1.0 - t) * a
{
    (1.0 - t) * a + t * b
}

#[inline]
pub fn approx_equal(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

// ----------------------------------------------
// FrameClock
// ----------------------------------------------

pub struct FrameClock {
    last_frame_time: time::Instant,
    delta_time: time::Duration,
}

impl FrameClock {
    pub fn new() -> Self {
        Self {
            last_frame_time: time::Instant::now(),
            delta_time: time::Duration::new(0, 0),
        }
    }

    #[inline]
    pub fn begin_frame(&self) {}

    #[inline]
    pub fn end_frame(&mut self) {
        let time_now = time::Instant::now();
        self.delta_time = time_now - self.last_frame_time;
        self.last_frame_time = time_now;
    }

    #[inline]
    #[must_use]
    pub fn delta_time(&self) -> time::Duration {
        self.delta_time
    }
}

// ----------------------------------------------
// macos_redirect_stderr()
// ----------------------------------------------

// Using this to deal with some TTY spam from the OpenGL loader on MacOS.
#[cfg(target_os = "macos")]
pub fn macos_redirect_stderr<F, R>(f: F, filename: &str) -> R where F: FnOnce() -> R {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    use libc::{dup, dup2, close, STDERR_FILENO};

    unsafe {
        let saved_fd = dup(STDERR_FILENO);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(filename)
            .expect("Failed to open stderr log file!");
        dup2(file.as_raw_fd(), STDERR_FILENO);
        let result = f();
        dup2(saved_fd, STDERR_FILENO);
        close(saved_fd);
        result
    }
}

#[cfg(not(target_os = "macos"))]
pub fn macos_redirect_stderr<F, R>(f: F, _filename: &str) -> R where F: FnOnce() -> R {
    f() // No-op on non-MacOS
}
