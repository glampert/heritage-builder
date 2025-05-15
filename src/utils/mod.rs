use std::ops::{Add, Sub, Mul};

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
    pub const fn new(x: f32, y: f32) -> Self {
        Self {
            x: x,
            y: y,
        }
    }

    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
        }
    }

    #[must_use]
    pub fn dot(&self, other: Self) -> f32 {
        (self.x * other.x) + (self.y * other.y)
    }

    #[must_use]
    pub fn length_squared(&self) -> f32 {
        self.dot(*self)
    }

    #[must_use]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[must_use]
    pub fn normalize(&self) -> Self {
        let inv_len = 1.0 / self.length();
        Self {
            x: self.x * inv_len,
            y: self.y * inv_len,
        }
    }

    pub fn to_point2d(&self) -> Point2D {
        Point2D::new(self.x as i32, self.y as i32)
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

// Vec2 * f32
impl Mul<f32> for Vec2 {
    type Output = Vec2;

    fn mul(self, scalar: f32) -> Vec2 {
        Vec2 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

// f32 * Vec2
impl Mul<Vec2> for f32 {
    type Output = Vec2;

    fn mul(self, vec: Vec2) -> Vec2 {
        Vec2 {
            x: vec.x * self,
            y: vec.y * self,
        }
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
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r: r, g: g, b: b, a: a }
    }

    pub const fn white()  -> Self { Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
    pub const fn black()  -> Self { Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub const fn red()    -> Self { Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub const fn green()  -> Self { Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 } }
    pub const fn blue()   -> Self { Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 } }
    pub const fn yellow() -> Self { Self { r: 1.0, g: 1.0, b: 0.0, a: 1.0 } }
    pub const fn cyan()   -> Self { Self { r: 0.0, g: 1.0, b: 1.0, a: 1.0 } }
    pub const fn purple() -> Self { Self { r: 1.0, g: 0.0, b: 1.0, a: 1.0 } }
    pub const fn gray()   -> Self { Self { r: 0.7, g: 0.7, b: 0.7, a: 1.0 } }

    pub const fn to_array(&self) -> [f32; 4] {
        [ self.r, self.g, self.b, self.a ]
    }
}

impl Default for Color {
    fn default() -> Self { Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
}

// ----------------------------------------------
// Size2D
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Size2D {
    pub width:  i32,
    pub height: i32,
}

impl Size2D {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width: width, height: height }
    }

    pub const fn zero() -> Self {
        Self { width: 0, height: 0 }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

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
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x: x, y: y }
    }

    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

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
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x: x, y: y }
    }

    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

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
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x: x, y: y }
    }

    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    pub const fn invalid() -> Self {
        Self { x: -1, y: -1 }
    }

    pub fn is_valid(&self) -> bool {
        self.x > 0 && self.y > 0
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
    pub const fn new(scaling: i32, offset: Point2D, tile_spacing: i32) -> Self {
        Self {
            scaling: scaling,
            offset: offset,
            tile_spacing: tile_spacing,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.scaling > 0 && self.tile_spacing >= 0
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
pub const fn iso_to_cell(iso: IsoPoint2D, tile_size: Size2D) -> Cell2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    // Invert Y axis to match top-left origin
    let cell_x = (( iso.x / half_tile_width)  + (-iso.y / half_tile_height)) / 2;
    let cell_y = ((-iso.y / half_tile_height) - ( iso.x / half_tile_width))  / 2;

    Cell2D::new(cell_x, cell_y)
}

#[inline]
pub const fn cell_to_iso(cell: Cell2D, tile_size: Size2D) -> IsoPoint2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;

    let iso_x = (cell.x - cell.y) *  half_tile_width;
    let iso_y = (cell.x + cell.y) * -half_tile_height; // flip Y (top-left origin)

    IsoPoint2D::new(iso_x, iso_y)
}

#[inline]
pub const fn iso_to_screen_point(iso: IsoPoint2D, transform: &WorldToScreenTransform) -> Point2D {
    let half_spacing = transform.tile_spacing / 2;

    // Apply spacing, scaling, and offset to position:
    let screen_x = ((iso.x + half_spacing) * transform.scaling) + transform.offset.x;
    let screen_y = ((iso.y + half_spacing) * transform.scaling) + transform.offset.y;

    Point2D::new(screen_x, screen_y)
}

#[inline]
pub const fn iso_to_screen_rect(iso: IsoPoint2D, size: Size2D, transform: &WorldToScreenTransform) -> Rect2D {
    let screen_point = iso_to_screen_point(iso, transform);

    // Shrink size by spacing and apply scaling:
    let screen_width  = (size.width  - transform.tile_spacing) * transform.scaling;
    let screen_height = (size.height - transform.tile_spacing) * transform.scaling;

    Rect2D::new(screen_point, Size2D::new(screen_width, screen_height))
}

#[inline]
pub const fn screen_to_iso_point(screen_pos: Point2D, transform: &WorldToScreenTransform) -> IsoPoint2D {
    let spacing = transform.tile_spacing;
    let half_spacing = spacing / 2;

    // Remove offset and scaling:
    let iso_x = (screen_pos.x - transform.offset.x) / transform.scaling;
    let iso_y = (screen_pos.y - transform.offset.y) / transform.scaling;

    // Remove the half-spacing added during rendering:
    IsoPoint2D::new(iso_x - half_spacing, iso_y - half_spacing)
}

#[inline]
pub fn cell_to_screen_diamond_points(cell: Cell2D,
                                     tile_size: Size2D,
                                     transform: &WorldToScreenTransform,
                                     apply_spacing: bool) -> [Point2D; 4] {

    let mut iso_center = cell_to_iso(cell, tile_size);

    // We want to apply spacing when precisely picking tiles but not when rendering the tile map grid.
    let spacing = if apply_spacing { transform.tile_spacing } else { 0 };
    let half_spacing = if apply_spacing { transform.tile_spacing / 2 } else { 0 };

    let width  = tile_size.width  - spacing;
    let height = tile_size.height - spacing;

    // Apply scale, offset and optional tile spacing:
    iso_center.x = ((iso_center.x + half_spacing) * transform.scaling) + transform.offset.x;
    iso_center.y = ((iso_center.y + half_spacing) * transform.scaling) + transform.offset.y;

    // We're off by one cell, so adjust:
    iso_center.x += width;
    iso_center.y += height;

    // 4 corners of the tile:
    let top    = Point2D::new(iso_center.x, iso_center.y - height);
    let right  = Point2D::new(iso_center.x + width, iso_center.y);
    let bottom = Point2D::new(iso_center.x, iso_center.y + height);
    let left   = Point2D::new(iso_center.x - width, iso_center.y);

    [ top, right, bottom, left ]
}

#[inline]
pub fn screen_point_inside_triangle(p: Point2D, a: Point2D, b: Point2D, c: Point2D) -> bool {
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

#[inline]
pub fn screen_point_inside_diamond(p: Point2D, points: &[Point2D; 4]) -> bool {
    // Triangle 1: top, right, bottom
    if screen_point_inside_triangle(p, points[0], points[1], points[2]) {
        return true;
    }
    // Triangle 2: bottom, left, top
    if screen_point_inside_triangle(p, points[2], points[3], points[0]) {
        return true;
    }
    false
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
    pub const fn new(pos: Point2D, size: Size2D) -> Self {
        Self {
            mins: pos,
            maxs: Point2D::new(pos.x + size.width, pos.y + size.height),
        }
    }

    pub const fn zero() -> Self {
        Self {
            mins: Point2D::zero(),
            maxs: Point2D::zero(),
        }
    }

    pub fn with_points(a: Point2D, b: Point2D) -> Self {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        Self {
            mins: Point2D::new(min_x, min_y),
            maxs: Point2D::new(max_x, max_y),
        }
    }

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

    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }

    pub fn center(&self) -> Point2D {
        Point2D::new(
            self.x() + self.width()  / 2,
            self.y() + self.height() / 2)
    }

    pub fn canonicalize(&mut self) {
        if self.mins.x > self.maxs.x {
            std::mem::swap(&mut self.mins.x, &mut self.maxs.x);
        }
        if self.mins.y > self.maxs.y {
            std::mem::swap(&mut self.mins.y, &mut self.maxs.y);
        }
    }

    // Flips min/max bounds if needed.
    pub fn update_min_extent(&mut self, new_mins: Point2D) {
        self.mins = new_mins;
        self.canonicalize();
    }

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

    pub fn top_left(&self) -> Vec2 {
        Vec2::new(
            self.x() as f32,
            (self.y() + self.height()) as f32)
    }

    pub fn bottom_left(&self) -> Vec2 {
        Vec2::new(
            self.x() as f32,
            self.y() as f32)
    }

    pub fn top_right(&self) -> Vec2 {
        Vec2::new(
            (self.x() + self.width())  as f32,
            (self.y() + self.height()) as f32)
    }

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
    pub const fn new(coords: [Vec2; 4]) -> Self {
        Self { coords: coords }
    }

    pub const fn zero() -> Self {
        Self {
            coords: [ Vec2::zero(); 4 ],
        }
    }

    pub const fn default() -> Self {
        Self {
            coords: [
                Vec2::new(0.0, 0.0), // top_left
                Vec2::new(0.0, 1.0), // bottom_left
                Vec2::new(1.0, 0.0), // top_right
                Vec2::new(1.0, 1.0), // bottom_right
            ],
        }
    }

    //
    // NOTE: Top-left is the origin.
    //

    pub fn top_left(&self) -> Vec2 {
        self.coords[0]
    }

    pub fn bottom_left(&self) -> Vec2 {
        self.coords[1]
    }

    pub fn top_right(&self) -> Vec2 {
        self.coords[2]
    }

    pub fn bottom_right(&self) -> Vec2 {
        self.coords[3]
    }
}

// ----------------------------------------------
// FrameClock
// ----------------------------------------------

use std::time::{self};

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
