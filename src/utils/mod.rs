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

// 2D screen space vector or point.
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
    pub fn to_point2d(&self) -> Point2D {
        Point2D::new(self.x as i32, self.y as i32)
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
// Vec3
// ----------------------------------------------

// For interfacing with shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            x: x,
            y: y,
            z: z,
        }
    }

    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

// ----------------------------------------------
// Vec4
// ----------------------------------------------

// For interfacing with shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            x: x,
            y: y,
            z: z,
            w: w,
        }
    }

    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        }
    }
}

// ----------------------------------------------
// Color
// ----------------------------------------------

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
// Size2D
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Size2D {
    pub width:  i32,
    pub height: i32,
}

impl Size2D {
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
// Point2D
// ----------------------------------------------

// Cartesian 2D point coords in screen space.
// Usually with WorldToScreenTransform applied.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point2D {
    pub x: i32,
    pub y: i32,
}

impl Point2D {
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
// IsoPoint2D
// ----------------------------------------------

// Isometric 2D point coords, in pure isometric space,
// before any WorldToScreenTransform are applied.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct IsoPoint2D {
    pub x: i32,
    pub y: i32,
}

impl IsoPoint2D {
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
// Cell2D
// ----------------------------------------------

// Position in the tile map grid of cells.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Cell2D {
    pub x: i32,
    pub y: i32,
}

impl Cell2D {
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
// Rect2D
// ----------------------------------------------

// Screen space rectangle defined by min and max extents.
// `mins`` is the top-left corner and `maxs` is the bottom-right.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect2D {
    pub mins: Point2D,
    pub maxs: Point2D,
}

impl Rect2D {
    #[inline]
    pub const fn new(pos: Point2D, size: Size2D) -> Self {
        Self {
            mins: pos,
            maxs: Point2D::new(pos.x + size.width, pos.y + size.height),
        }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            mins: Point2D::zero(),
            maxs: Point2D::zero(),
        }
    }

    #[inline]
    pub fn from_extents(a: Point2D, b: Point2D) -> Self {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        Self {
            mins: Point2D::new(min_x, min_y),
            maxs: Point2D::new(max_x, max_y),
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.width() > 0 && self.height() > 0
    }

    #[inline]
    pub fn x(&self) -> i32 {
        self.mins.x
    }

    #[inline]
    pub fn y(&self) -> i32 {
        self.mins.y
    }

    #[inline]
    pub fn position(&self) -> Point2D {
        Point2D::new(self.x(), self.y())
    }

    #[inline]
    pub fn width(&self) -> i32 {
        self.maxs.x - self.mins.x
    }

    #[inline]
    pub fn height(&self) -> i32 {
        self.maxs.y - self.mins.y
    }

    #[inline]
    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }

    #[inline]
    pub fn center(&self) -> Point2D {
        Point2D::new(
            self.x() + self.width()  / 2,
            self.y() + self.height() / 2)
    }

    #[inline]
    pub fn canonicalize(&mut self) {
        if self.mins.x > self.maxs.x {
            std::mem::swap(&mut self.mins.x, &mut self.maxs.x);
        }
        if self.mins.y > self.maxs.y {
            std::mem::swap(&mut self.mins.y, &mut self.maxs.y);
        }
    }

    // Flips min/max bounds if needed.
    #[inline]
    pub fn update_min_extent(&mut self, new_mins: Point2D) {
        self.mins = new_mins;
        self.canonicalize();
    }

    #[inline]
    pub fn update_max_extent(&mut self, new_maxs: Point2D) {
        self.maxs = new_maxs;
        self.canonicalize();
    }

    // Returns `true` if this rect intersects with another.
    #[inline]
    pub fn intersects(&self, other: &Rect2D) -> bool {
        self.mins.x < other.maxs.x &&
        self.maxs.x > other.mins.x &&
        self.mins.y < other.maxs.y &&
        self.maxs.y > other.mins.y
    }

    // Returns `true` if the point is inside this rect (inclusive of mins, exclusive of maxs).
    #[inline]
    pub fn contains_point(&self, point: Point2D) -> bool {
        point.x >= self.mins.x &&
        point.x <  self.maxs.x &&
        point.y >= self.mins.y &&
        point.y <  self.maxs.y
    }

    // Returns `true` if this rect fully contains the other rect.
    #[inline]
    pub fn contains_rect(&self, other: &Rect2D) -> bool {
        self.mins.x <= other.mins.x &&
        self.maxs.x >= other.maxs.x &&
        self.mins.y <= other.mins.y &&
        self.maxs.y >= other.maxs.y
    }

    //
    // NOTE: Top-left is the origin.
    //

    #[inline]
    pub fn top_left(&self) -> Vec2 {
        Vec2::new(
            self.x() as f32,
            (self.y() + self.height()) as f32)
    }

    #[inline]
    pub fn bottom_left(&self) -> Vec2 {
        Vec2::new(
            self.x() as f32,
            self.y() as f32)
    }

    #[inline]
    pub fn top_right(&self) -> Vec2 {
        Vec2::new(
            (self.x() + self.width())  as f32,
            (self.y() + self.height()) as f32)
    }

    #[inline]
    pub fn bottom_right(&self) -> Vec2 {
        Vec2::new(
            (self.x() + self.width()) as f32,
            self.y() as f32)
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
    pub const fn default() -> Self {
        static DEFAULT: RectTexCoords = RectTexCoords::new(
            [
                Vec2::new(0.0, 0.0), // top_left
                Vec2::new(0.0, 1.0), // bottom_left
                Vec2::new(1.0, 0.0), // top_right
                Vec2::new(1.0, 1.0), // bottom_right
            ]);
        DEFAULT
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
    pub scaling: i32,      // draw scaling. 1 = no scaling.
    pub offset: Point2D,   // screen-space offset (e.g. camera pan).
    pub tile_spacing: i32, // pixels between tiles (0 = tight fit).
}

impl WorldToScreenTransform {
    pub fn new(scaling: i32, offset: Point2D, tile_spacing: i32) -> Self {
        let transform = Self {
            scaling: scaling,
            offset: offset,
            tile_spacing: tile_spacing,
        };
        debug_assert!(transform.is_valid());
        transform
    }

    pub fn is_valid(&self) -> bool {
        self.scaling > 0 && self.tile_spacing >= 0
    }

    #[inline]
    pub fn apply_to_iso_point(&self, iso_point: IsoPoint2D, apply_spacing: bool) -> Point2D {
        let half_spacing = if apply_spacing { self.tile_spacing / 2 } else { 0 };

        // Apply spacing, offset and scaling:
        let screen_x = ((iso_point.x + half_spacing) * self.scaling) + self.offset.x;
        let screen_y = ((iso_point.y + half_spacing) * self.scaling) + self.offset.y;

        Point2D::new(screen_x, screen_y)
    }

    #[inline]
    pub fn apply_to_screen_point(&self, screen_point: Point2D, apply_spacing: bool) -> IsoPoint2D {
        let half_spacing = if apply_spacing { self.tile_spacing / 2 } else { 0 };

        // Remove spacing, offset and scaling:
        let iso_x = ((screen_point.x - self.offset.x) / self.scaling) - half_spacing;
        let iso_y = ((screen_point.y - self.offset.y) / self.scaling) - half_spacing;

        IsoPoint2D::new(iso_x, iso_y)
    }

    #[inline]
    pub fn apply_to_rect(&self, iso_position: IsoPoint2D, size: Size2D, apply_spacing: bool) -> Rect2D {
        let tile_spacing = if apply_spacing { self.tile_spacing } else { 0 };
        let screen_position = self.apply_to_iso_point(iso_position, true);

        // Shrink size by spacing and apply scaling:
        let screen_width  = (size.width  - tile_spacing) * self.scaling;
        let screen_height = (size.height - tile_spacing) * self.scaling;

        Rect2D::new(screen_position, Size2D::new(screen_width, screen_height))
    }

    #[inline]
    pub fn scale_and_offset_rect(&self, rect: Rect2D) -> Rect2D {
        let x = rect.x() + (self.offset.x * self.scaling);
        let y = rect.y() + (self.offset.y * self.scaling);

        let width  = rect.width()  * self.scaling;
        let height = rect.height() * self.scaling;

        Rect2D::new(Point2D::new(x, y), Size2D::new(width, height))
    }
}

impl Default for WorldToScreenTransform {
    fn default() -> Self {
        Self {
            scaling: 1,
            offset: Point2D::zero(),
            tile_spacing: 0
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
pub fn iso_to_cell(iso_point: IsoPoint2D, tile_size: Size2D) -> Cell2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    // Invert Y axis to match top-left origin
    let cell_x = (( iso_point.x / half_tile_width)  + (-iso_point.y / half_tile_height)) / 2;
    let cell_y = ((-iso_point.y / half_tile_height) - ( iso_point.x / half_tile_width))  / 2;

    Cell2D::new(cell_x, cell_y)
}

#[inline]
pub fn cell_to_iso(cell: Cell2D, tile_size: Size2D) -> IsoPoint2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    let iso_x = (cell.x - cell.y) *  half_tile_width;
    let iso_y = (cell.x + cell.y) * -half_tile_height; // flip Y (top-left origin)

    IsoPoint2D::new(iso_x, iso_y)
}

#[inline]
pub fn iso_to_screen_point(iso_point: IsoPoint2D,
                           transform: &WorldToScreenTransform,
                           tile_size: Size2D,
                           apply_spacing: bool) -> Point2D {
    // Undo offsetting.
    let mut iso = iso_point;
    iso.x += tile_size.width  / 2;
    iso.y += tile_size.height / 2;

    transform.apply_to_iso_point(iso, apply_spacing)
}

#[inline]
pub fn screen_to_iso_point(screen_point: Point2D,
                           transform: &WorldToScreenTransform,
                           tile_size: Size2D,
                           apply_spacing: bool) -> IsoPoint2D {

    let mut iso_pos = transform.apply_to_screen_point(screen_point, apply_spacing);

    // Offset the iso point downward by half a tile (visually centers the hit test to the tile center).
    iso_pos.x -= tile_size.width  / 2;
    iso_pos.y -= tile_size.height / 2;
    iso_pos
}

#[inline]
pub fn iso_to_screen_rect(iso_position: IsoPoint2D,
                          size: Size2D,
                          transform: &WorldToScreenTransform,
                          apply_spacing: bool) -> Rect2D {

    transform.apply_to_rect(iso_position, size, apply_spacing)
}

#[inline]
pub fn is_screen_point_inside_diamond(p: Point2D, points: &[Point2D; 4]) -> bool {
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
pub fn is_screen_point_inside_cell(screen_point: Point2D,
                                   cell: Cell2D,
                                   tile_size: Size2D,
                                   base_tile_size: Size2D,
                                   transform: &WorldToScreenTransform) -> bool {

    debug_assert!(transform.is_valid());

    let screen_points = cell_to_screen_diamond_points(
        cell,
        tile_size,
        base_tile_size,
        transform);

    is_screen_point_inside_diamond(screen_point, &screen_points)
}

pub fn is_screen_point_inside_triangle(p: Point2D, a: Point2D, b: Point2D, c: Point2D) -> bool {
    // Compute edge vectors of the triangle relative to vertex `a`:
    let va = a.to_vec2();
    let v0 = c.to_vec2() - va;
    let v1 = b.to_vec2() - va;
    let v2 = p.to_vec2() - va; // vector from `a` to point `p`

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
pub fn cell_to_screen_diamond_points(cell: Cell2D,
                                     tile_size: Size2D,
                                     base_tile_size: Size2D,
                                     transform: &WorldToScreenTransform) -> [Point2D; 4] {

    debug_assert!(transform.is_valid());

    let iso_center = cell_to_iso(cell, base_tile_size);
    let screen_center = iso_to_screen_point(iso_center, transform, base_tile_size, false);

    let tile_width  = tile_size.width  * transform.scaling;
    let tile_height = tile_size.height * transform.scaling;
    let base_height = base_tile_size.height * transform.scaling;

    let half_tile_w = tile_width  / 2;
    let half_tile_h = tile_height / 2;
    let half_base_h = base_height / 2;

    // Build 4 corners of the tile:
    let top    = Point2D::new(screen_center.x, screen_center.y - tile_height + half_base_h);
    let bottom = Point2D::new(screen_center.x, screen_center.y + half_base_h);
    let right  = Point2D::new(screen_center.x + half_tile_w, screen_center.y - half_tile_h + half_base_h);
    let left   = Point2D::new(screen_center.x - half_tile_w, screen_center.y - half_tile_h + half_base_h);

    [ top, right, bottom, left ]
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
