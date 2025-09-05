#![allow(clippy::mut_from_ref)] // mut-casts in UnsafeWeakRef/UnsafeMutable are intentional and required.

use core::ptr::NonNull;
use arrayvec::ArrayString;

use std::{
    time::{self},
    cell::UnsafeCell,
    sync::OnceLock,
    thread::ThreadId,
    ops::{
        Add,
        AddAssign,
        Sub,
        SubAssign,
        Mul,
        MulAssign,
        Div,
        DivAssign,
        Deref,
        DerefMut
    }
};

use serde::{
    Deserialize,
    Deserializer
};

pub mod coords;
pub mod file_sys;
pub mod hash;
pub mod platform;

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

// Defines a bitflags struct with a Display implementation.
#[macro_export]
macro_rules! bitflags_with_display {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident: $ty:ty {
            $(
                const $flag:ident = $value:expr;
            )+
        }
    ) => {
        bitflags! {
            $(#[$meta])*
            $vis struct $name: $ty {
                $(
                    const $flag = $value;
                )+
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut first = true;
                $(
                    if self.contains($name::$flag) {
                        if !first {
                            write!(f, " | ")?;
                        }
                        write!(f, stringify!($flag))?;
                        first = false;
                    }
                )+
                if first {
                    write!(f, "(empty)")
                } else {
                    Ok(())
                }
            }
        }
    };
}

// Returns refs to the x,y fields of structs like Vec2, Cell, IsoPoint, etc.
// Used with imgui debug widgets.
pub trait FieldAccessorXY<T> {
    fn x_ref(&self) -> &T;
    fn y_ref(&self) -> &T;
    fn x_mut(&mut self) -> &mut T;
    fn y_mut(&mut self) -> &mut T;
}

#[macro_export]
macro_rules! field_accessor_xy {
    ($struct_name:ty, $field_type:ty, $x_field:ident, $y_field:ident) => {
        impl FieldAccessorXY<$field_type> for $struct_name {
            #[inline] fn x_ref(&self) -> &$field_type { &self.$x_field }
            #[inline] fn y_ref(&self) -> &$field_type { &self.$y_field }
            #[inline] fn x_mut(&mut self) -> &mut $field_type { &mut self.$x_field }
            #[inline] fn y_mut(&mut self) -> &mut $field_type { &mut self.$y_field }
        }
    };
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
        Self { x, y }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
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

impl std::fmt::Display for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{:.2},{:.2}]", self.x, self.y)
    }
}

field_accessor_xy! { Vec2, f32, x, y }

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
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
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

    #[inline]
    pub fn clamp(&self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }
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

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{},{},{},{}]", self.r, self.g, self.b, self.a)
    }
}

// ----------------------------------------------
// Size
// ----------------------------------------------

// Integer width & height pair.
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq, Deserialize)]
pub struct Size {
    pub width:  i32,
    pub height: i32,
}

impl Size {
    #[inline]
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn from_array(s: [i32; 2]) -> Self {
        Self { width: s[0], height: s[1] }
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
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }

    #[inline]
    pub fn to_array(self) -> [i32; 2] {
        [ self.width, self.height ]
    }
}

// Size + i32
impl Add<i32> for Size {
    type Output = Size;
    fn add(self, rhs: i32) -> Size {
        Size {
            width:  self.width  + rhs,
            height: self.height + rhs,
        }
    }
}

// Size - i32
impl Sub<i32> for Size {
    type Output = Size;
    fn sub(self, rhs: i32) -> Size {
        Size {
            width:  self.width  - rhs,
            height: self.height - rhs,
        }
    }
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{},{}]", self.width, self.height)
    }
}

field_accessor_xy! { Size, i32, width, height }

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
        Self { coords }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            coords: [Vec2::zero(); 4],
        }
    }

    // NOTE: This needs to be const for static declarations, so we don't just derive from Default.
    #[inline]
    pub const fn default_ref() -> &'static Self {
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

impl Default for RectTexCoords {
    fn default() -> Self { *RectTexCoords::default_ref() }
}

// ----------------------------------------------
// Conversion & math helpers
// ----------------------------------------------

// Maps a value in the numerical range [in_min, in_max] to the range [out_min, out_max].
#[inline]
pub fn map_value_to_range<T>(val: T, in_min: T, in_max: T, out_min: T, out_max: T) -> T
    where T: Sub<Output = T> + Div<Output = T> + Mul<Output = T> + Add<Output = T> + PartialEq + Copy
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
    where T: Sub<Output = T> + Div<Output = T> + Copy
{
    (val - minimum) / (maximum - minimum)
}

// Linear interpolation.
#[inline]
pub fn lerp<T>(a: T, b: T, t: f32) -> T
    where T: Mul<f32, Output = T> + Add<Output = T> + Copy,
          f32: Mul<T, Output = T> // for (1.0 - t) * a
{
    (1.0 - t) * a + t * b
}

#[inline]
pub fn approx_equal(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

pub fn snake_case_to_title<const N: usize>(s: &str) -> ArrayString<N> {
    let mut result = ArrayString::<N>::new();

    for (i, word) in s.split('_').enumerate() {
        if i > 0 && result.try_push(' ').is_err() {
            break;
        }

        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for c in first.to_uppercase() {
                if result.try_push(c).is_err() {
                    return result;
                }
            }

            for c in chars {
                if result.try_push(c).is_err() {
                    return result;
                }
            }
        }
    }

    result
}

// ----------------------------------------------
// FrameClock
// ----------------------------------------------

pub type Seconds = f32;

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
    pub fn delta_time(&self) -> Seconds {
        self.delta_time.as_secs_f32()
    }
}

// ----------------------------------------------
// UnsafeWeakRef
// ----------------------------------------------

// Store a raw pointer.
// This allows bypassing the language lifetime guarantees, so should be used with care.
pub struct UnsafeWeakRef<T> {
    ptr: NonNull<T>,
}

impl<T> UnsafeWeakRef<T> {
    #[inline]
    pub fn new(reference: &T) -> Self {
        let ptr = reference as *const T as *mut T;
        Self {
            ptr: NonNull::new(ptr).unwrap(),
        }
    }

    #[inline]
    pub fn from_ptr(ptr_in: *const T) -> Self {
        let ptr = ptr_in as *mut T;
        Self {
            ptr: NonNull::new(ptr).unwrap(),
        }
    }

    // Convert raw pointer to reference.
    // Pointer is never null but there are not guarantees about its lifetime.
    #[inline(always)]
    pub fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }

    // Convert raw pointer to mutable reference.
    // SAFETY: Caller must ensure there are no aliasing issues 
    // (e.g. no other refs) and valid pointer lifetime.
    #[inline(always)]
    pub fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }

    #[inline(always)]
    pub fn mut_ref_cast(&self) -> &mut T {
        unsafe { &mut *self.ptr.as_ptr() }
    }
}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for UnsafeWeakRef<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for UnsafeWeakRef<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Caller must ensure exclusive access (no aliasing).
        self.as_mut()
    }
}

impl<T> Copy  for UnsafeWeakRef<T> {}
impl<T> Clone for UnsafeWeakRef<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self // Just a cheap pointer copy.
    }
}

// ----------------------------------------------
// UnsafeMutable
// ----------------------------------------------

// Hold an UnsafeCell<T> which allows unchecked interior mutability (const casting).
pub struct UnsafeMutable<T> {
    cell: UnsafeCell<T>,
}

impl<T> UnsafeMutable<T> {
    #[inline]
    pub fn new(instance: T) -> Self {
        Self {
            cell: UnsafeCell::new(instance),
        }
    }

    // Safe to share immutable ref, no interior mut.
    #[inline(always)]
    pub fn as_ref(&self) -> &T {
        unsafe { &*self.cell.get() }
    }

    // SAFETY: Caller must ensure there are no aliasing issues (e.g. no other refs).
    #[inline(always)]
    pub fn as_mut(&self) -> &mut T {
        unsafe { &mut *self.cell.get() }
    }

    #[inline(always)]
    pub fn mut_ref_cast(reference: &T) -> &mut T {
        let ptr = reference as *const T as *mut T;
        unsafe { ptr.as_mut().unwrap() }
    }
}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for UnsafeMutable<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for UnsafeMutable<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Caller must ensure exclusive access (no aliasing).
        self.as_mut()
    }
}

// Serde deserialization support.
impl<'de, T> Deserialize<'de> for UnsafeMutable<T>
    where T: Deserialize<'de>
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        T::deserialize(deserializer).map(UnsafeMutable::new)
    }
}

impl<T: Default> Default for UnsafeMutable<T> {
    #[inline]
    fn default() -> Self {
        Self {
            cell: UnsafeCell::new(T::default()),
        }
    }
}

impl<T: Clone> Clone for UnsafeMutable<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            cell: UnsafeCell::new(self.as_ref().clone()),
        }
    }
}

// ----------------------------------------------
// SingleThreadStatic
// ----------------------------------------------

// A single-threaded mutable global static variable.
// Safe as long as only one thread ever touches it.
// If another thread tries, it will panic (not UB).
// First thread to access the instance claims ownership.
pub struct SingleThreadStatic<T> {
    value: UnsafeCell<T>,
    owner: OnceLock<ThreadId>,
}

impl<T> SingleThreadStatic<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            owner: OnceLock::new(),
        }
    }

    #[inline]
    pub fn set(&self, value: T) {
        *self.as_mut() = value;
    }

    #[inline]
    pub fn as_ref(&self) -> &T {
        self.assert_owner();
        unsafe { &*self.value.get() }
    }

    #[inline]
    pub fn as_mut(&self) -> &mut T {
        self.assert_owner();
        unsafe { &mut *self.value.get() }
    }

    fn assert_owner(&self) {
        let this_thread = std::thread::current().id();
        match self.owner.get() {
            Some(owner) if *owner == this_thread => {}, // Same thread, no action.
            Some(_) => panic!("SingleThreadStatic accessed from non-owner thread!"),
            None => {
                // First access claims ownership:
                self.owner.set(this_thread)
                    .unwrap_or_else(|_| panic!("Failed to set owner thread id!"));
            }
        }
    }
}

// SAFETY: Safe to share references because we enforce single-threaded access with assert_owner().
unsafe impl<T> Sync for SingleThreadStatic<T> {}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for SingleThreadStatic<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for SingleThreadStatic<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}
