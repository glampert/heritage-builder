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

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x: x, y: y }
    }

    #[must_use]
    pub fn dot(&self, v: Self) -> f32 {
        (self.x * v.x) + (self.y * v.y)
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

    #[must_use]
    pub fn add(&self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
        }
    }

    #[must_use]
    pub fn sub(&self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y,
        }
    }

    #[must_use]
    pub fn scale(&self, val: f32) -> Self {
        Self {
            x: self.x * val,
            y: self.y * val,
        }
    }

    pub fn to_point2d(&self) -> Point2D {
        Point2D {
            x: self.x as i32,
            y: self.y as i32,
        }
    }
}

// ----------------------------------------------
// Vec3
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// ----------------------------------------------
// Vec4
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

// ----------------------------------------------
// Color
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn white()  -> Self { Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
    pub const fn black()  -> Self { Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub const fn red()    -> Self { Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub const fn green()  -> Self { Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 } }
    pub const fn blue()   -> Self { Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 } }
    pub const fn yellow() -> Self { Self { r: 1.0, g: 1.0, b: 0.0, a: 1.0 } }
    pub const fn cyan()   -> Self { Self { r: 0.0, g: 1.0, b: 1.0, a: 1.0 } }
    pub const fn purple() -> Self { Self { r: 1.0, g: 0.0, b: 1.0, a: 1.0 } }
    pub const fn gray()   -> Self { Self { r: 0.7, g: 0.7, b: 0.7, a: 1.0 } }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r: r, g: g, b: b, a: a }
    }
}

impl Default for Color {
    fn default() -> Self { Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
}

// ----------------------------------------------
// Size2D
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default)]
pub struct Size2D {
    pub width: i32,
    pub height: i32,
}

impl Size2D {
    pub const fn zero() -> Self {
        Self {
            width: 0,
            height: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.width as f32,
            y: self.height as f32,
        }
    }
}

// ----------------------------------------------
// Point2D
// ----------------------------------------------

// Cartesian 2D point coords.
#[derive(Copy, Clone, Debug, Default)]
pub struct Point2D {
    pub x: i32,
    pub y: i32,
}

impl Point2D {
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    pub fn with_coords(cx: i32, cy: i32) -> Self {
        Self { x: cx, y: cy }
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x as f32,
            y: self.y as f32,
        }
    }
}

// ----------------------------------------------
// IsoPoint2D
// ----------------------------------------------

// Isometric 2D point coords.
#[derive(Copy, Clone, Debug, Default)]
pub struct IsoPoint2D {
    pub x: i32,
    pub y: i32,
}

impl IsoPoint2D {
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    pub fn with_coords(ix: i32, iy: i32) -> Self {
        Self { x: ix, y: iy }
    }

    pub fn to_screen_point(&self) -> Point2D {
        Point2D {
            x: self.x,
            y: self.y,
        }
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x as f32,
            y: self.y as f32,
        }
    }
}

// ----------------------------------------------
// Cell2D
// ----------------------------------------------

// Position in the tile map grid of cells.
#[derive(Copy, Clone, Debug, Default)]
pub struct Cell2D {
    pub x: i32,
    pub y: i32,
}

impl Cell2D {
    pub fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    pub fn with_coords(cx: i32, cy: i32) -> Self {
        Self { x: cx, y: cy }
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x as f32,
            y: self.y as f32,
        }
    }
}

#[inline]
pub const fn isometric_to_cell(iso: IsoPoint2D, tile_size: Size2D) -> Cell2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;
    let cx = ((iso.x / half_tile_width)  + (iso.y / half_tile_height)) / 2;
    let cy = ((iso.y / half_tile_height) - (iso.x / half_tile_width))  / 2;
    Cell2D { x: cx, y: cy }
}

#[inline]
pub const fn cell_to_isometric(cell: Cell2D, tile_size: Size2D) -> IsoPoint2D {
    let half_tile_width  = tile_size.width  / 2;
    let half_tile_height = tile_size.height / 2;
    let iso_x = (cell.x - cell.y) * half_tile_width;
    let iso_y = (cell.x + cell.y) * half_tile_height;
    IsoPoint2D { x: iso_x, y: iso_y }
}

// ----------------------------------------------
// Rect2D
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default)]
pub struct Rect2D {
    pub mins: Point2D,
    pub maxs: Point2D,
}

impl Rect2D {
    pub const fn zero() -> Self {
        Self {
            mins: Point2D::zero(),
            maxs: Point2D::zero(),
        }
    }

    pub fn with_bounds(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> Self {
        Self {
            mins: Point2D::with_coords(x_min, y_min),
            maxs: Point2D::with_coords(x_max, y_max),
        }
    }

    pub fn with_xy_and_size(x: i32, y: i32, size: Size2D) -> Self {
        Self {
            mins: Point2D { x: x, y: y },
            maxs: Point2D { x: x + size.width, y: y + size.height },
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width() > 0 && self.height() > 0
    }

    pub fn x(&self) -> i32 {
        self.mins.x
    }

    pub fn y(&self) -> i32 {
        self.mins.y
    }

    pub fn width(&self) -> i32 {
        self.maxs.x - self.mins.x
    }

    pub fn height(&self) -> i32 {
        self.maxs.y - self.mins.y
    }

    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }

    pub fn center(&self) -> Point2D {
        Point2D {
            x: self.x() + self.width()  / 2,
            y: self.y() + self.height() / 2,
        }
    }

    //
    // NOTE: Bottom-left is the origin.
    //

    pub fn top_left(&self) -> Vec2 {
        Vec2 {
            x: self.x() as f32,
            y: (self.y() + self.height()) as f32,
        }
    }

    pub fn bottom_left(&self) -> Vec2 {
        Vec2 {
            x: self.x() as f32,
            y: self.y() as f32,
        }
    }

    pub fn top_right(&self) -> Vec2 {
        Vec2 {
            x: (self.x() + self.width()) as f32,
            y: (self.y() + self.height()) as f32,
        }
    }

    pub fn bottom_right(&self) -> Vec2 {
        Vec2 {
            x: (self.x() + self.width()) as f32,
            y: self.y() as f32,
        }
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
    pub const fn zero() -> Self {
        Self {
            coords: [ Vec2 { x: 0.0, y: 0.0 }; 4 ],
        }
    }

    pub const fn default() -> Self {
        Self {
            coords: [
                Vec2 { x: 0.0, y: 0.0 }, // bottom_left
                Vec2 { x: 0.0, y: 1.0 }, // top_left
                Vec2 { x: 1.0, y: 1.0 }, // top_right
                Vec2 { x: 1.0, y: 0.0 }, // bottom_right
            ],
        }
    }

    //
    // NOTE: Bottom-left is the origin.
    //

    pub fn top_left(&self) -> Vec2 {
        self.coords[1]
    }

    pub fn bottom_left(&self) -> Vec2 {
        self.coords[0]
    }

    pub fn top_right(&self) -> Vec2 {
        self.coords[2]
    }

    pub fn bottom_right(&self) -> Vec2 {
        self.coords[3]
    }
}
