use std::any::Any;

use crate::{
    utils::{Vec2, Color, Size, Rect, RectTexCoords}
};

// Internal implementation.
mod opengl;
pub mod backend {
    use super::*;
    pub type RenderSystemOpenGl = opengl::system::RenderSystem;
    pub type TextureCacheOpenGl = opengl::texture::TextureCache;
}

// ----------------------------------------------
// RenderStats
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub struct RenderStats {
    // Current frame totals:
    pub triangles_drawn: u32,
    pub lines_drawn: u32,
    pub points_drawn: u32,
    pub texture_changes: u32,
    pub draw_calls: u32,
    // Peaks for the whole run:
    pub peak_triangles_drawn: u32,
    pub peak_lines_drawn: u32,
    pub peak_points_drawn: u32,
    pub peak_texture_changes: u32,
    pub peak_draw_calls: u32,
}

// ----------------------------------------------
// RenderSystem
// ----------------------------------------------

pub trait RenderSystem: Any {
    fn as_any(&self) -> &dyn Any;

    // ----------------------
    // Render frame markers:
    // ----------------------

    fn begin_frame(&mut self);
    fn end_frame(&mut self) -> RenderStats;

    // ----------------------
    // TextureCache access:
    // ----------------------

    fn texture_cache(&self) -> &dyn TextureCache;
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache;

    // ----------------------
    // Viewport:
    // ----------------------

    fn viewport(&self) -> Rect;
    fn set_viewport_size(&mut self, new_size: Size);

    // ----------------------
    // Draw commands:
    // ----------------------

    // This is used for emulated line drawing with custom thickness.
    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[u16],
                                      color: Color);

    // This is used for drawing sprite rectangles. There is a special case with
    // `texture=TextureHandle::white()` for drawing rectangles with color only.
    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: TextureHandle,
                                  color: Color);

    fn draw_colored_rect(&mut self,
                         rect: Rect,
                         color: Color) {

        // Just call this with the default white texture.
        self.draw_textured_colored_rect(
            rect,
            &RectTexCoords::DEFAULT,
            TextureHandle::white(),
            color);
    }

    fn draw_wireframe_rect_with_thickness(&mut self,
                                          rect: Rect,
                                          color: Color,
                                          thickness: f32) {

        if is_rect_fully_offscreen(&self.viewport(), &rect) {
            return; // Cull if fully offscreen.
        }

        let points = [
            Vec2::new(rect.x(), rect.y()),
            Vec2::new(rect.x() + rect.width(), rect.y()),
            Vec2::new(rect.x() + rect.width(), rect.y() + rect.height()),
            Vec2::new(rect.x(), rect.y() + rect.height()),
        ];

        self.draw_polyline_with_thickness(&points, color, thickness, true);
    }

    // This can handle straight lines efficiently but might produce discontinuities at connecting edges of
    // rectangles and other polygons. To draw connecting lines/polygons use draw_polyline_with_thickness().
    fn draw_line_with_thickness(&mut self,
                                from_pos: Vec2,
                                to_pos: Vec2,
                                color: Color,
                                thickness: f32) {

        if is_line_fully_offscreen(&self.viewport(), &from_pos, &to_pos) {
            return; // Cull if fully offscreen.
        }

        let d = to_pos - from_pos;
        let length = d.length();

        // Normalize and rotate 90° to get perpendicular vector
        let nx = -d.y / length;
        let ny =  d.x / length;
    
        let offset_x = nx * (thickness / 2.0);
        let offset_y = ny * (thickness / 2.0);

        // Four corner points of the quad (screen space)
        let vertices = [
            Vec2::new((from_pos.x + offset_x).round(), (from_pos.y + offset_y).round()),
            Vec2::new((to_pos.x   + offset_x).round(), (to_pos.y   + offset_y).round()),
            Vec2::new((to_pos.x   - offset_x).round(), (to_pos.y   - offset_y).round()),
            Vec2::new((from_pos.x - offset_x).round(), (from_pos.y - offset_y).round()),
        ];

        // Draw two triangles to form a quad
        const INDICES: [u16; 6] = [
            0, 1, 2, // first triangle
            2, 3, 0, // second triangle
        ];

        self.draw_colored_indexed_triangles(&vertices, &INDICES, color);
    }

    // Handles connecting lines or closed polygons with seamless mitered joints.
    // Slower but with correct visual results and no seams.
    fn draw_polyline_with_thickness(&mut self,
                                    points: &[Vec2],
                                    color: Color,
                                    thickness: f32,
                                    is_closed: bool) {

        const MAX_POINTS:   usize = 32;
        const MAX_VERTICES: usize = 2 * MAX_POINTS;
        const MAX_INDICES:  usize = 6 * MAX_POINTS;

        let num_points = points.len();
        debug_assert!((2..=MAX_POINTS).contains(&num_points));

        let mut vertices = [Vec2::default(); MAX_VERTICES];
        let mut indices  = [0u16; MAX_INDICES];

        let mut v_count = 0;
        let mut i_count = 0;
    
        for i in 0..num_points {
            let prev = if i == 0 {
                if is_closed { points[num_points - 1] } else { points[0] }
            } else {
                points[i - 1]
            };
    
            let curr = points[i];
    
            let next = if i == num_points - 1 {
                if is_closed { points[0] } else { points[num_points - 1] }
            } else {
                points[i + 1]
            };

            // Compute averaged normal (miter join)
            let dir1 = (curr - prev).normalize();
            let dir2 = (next - curr).normalize();

            let normal1 = Vec2::new(-dir1.y, dir1.x);
            let normal2 = Vec2::new(-dir2.y, dir2.x);
            let avg_normal = (normal1 + normal2).normalize();
    
            // Limit how far the join stretches by clamping the offset length to a maximum miter limit.
            // - Scale the miter by 1 / dot(normal1, miter) to preserve line thickness.
            // - Clamp it to avoid distortions at sharp angles.
            // - This results in smooth joins that don’t overextend, making diamond shapes look uniform.
            let miter_length = (thickness * 0.5) / avg_normal.dot(normal1).abs().max(1e-4);
            let max_miter = thickness * 2.0;
            let clamped_length = miter_length.min(max_miter);
            let offset = avg_normal * clamped_length;

            // Two vertices per point: one offset +, one offset -
            vertices[v_count] = curr + offset;
            vertices[v_count + 1] = curr - offset;
    
            // Build indices
            if i < num_points - 1 || is_closed {
                let i0 = v_count.try_into().expect("Value cannot fit into a u16!");
                let i1 = i0 + 1;
                let i2 = ((i + 1) % num_points * 2).try_into().expect("Value cannot fit into a u16!");
                let i3 = i2 + 1;
    
                indices[i_count..i_count + 6].copy_from_slice(&[i0, i2, i1, i1, i2, i3]);
                i_count += 6;
            }

            v_count += 2;
        }

        self.draw_colored_indexed_triangles(&vertices[..v_count], &indices[..i_count], color);
    }

    // ----------------------
    // Debug drawing:
    // ----------------------

    // "Fast" line and point drawing, mainly used for debugging.
    // These lines and points are batched separately and drawn
    // on top of all sprites so they will not respect draw order
    // in relation to textured sprites and colored polygons.
    fn draw_line_fast(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color);
    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32);

    fn draw_wireframe_rect_fast(&mut self, rect: Rect, color: Color) {
        if is_rect_fully_offscreen(&self.viewport(), &rect) {
            return; // Cull if fully offscreen.
        }

        let vertices = [
            rect.bottom_left(),
            rect.bottom_right(),
            rect.top_right(),
            rect.top_left(),
            rect.bottom_left(), // close the loop
        ];

        for pair in vertices.windows(2) {
            self.draw_line_fast(pair[0], pair[1], color, color);
        }
    }
}

// ----------------------------------------------
// RenderSystemFactory
// ----------------------------------------------

pub trait RenderSystemFactory: Sized {
    fn new(viewport_size: Size, clear_color: Color) -> Self;
}

// ----------------------------------------------
// RenderSystemBuilder
// ----------------------------------------------

pub struct RenderSystemBuilder {
    viewport_size: Size,
    clear_color: Color,
}

impl RenderSystemBuilder {
    pub fn new() -> Self {
        Self {
            viewport_size: Size::new(1024, 768),
            clear_color: Color::black(),
        }
    }

    pub fn viewport_size(&mut self, size: Size) -> &mut Self {
        self.viewport_size = size;
        self
    }

    pub fn clear_color(&mut self, color: Color) -> &mut Self {
        self.clear_color = color;
        self
    }

    pub fn build<RenderSystemBackendImpl>(&self) -> Box<RenderSystemBackendImpl>
        where RenderSystemBackendImpl: RenderSystem + RenderSystemFactory + 'static
    {
        Box::new(RenderSystemBackendImpl::new(
            self.viewport_size,
            self.clear_color))
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

#[inline]
pub fn is_rect_fully_offscreen(viewport: &Rect, rect: &Rect) -> bool {
    if rect.max.x < viewport.min.x || rect.max.y < viewport.min.y {
        return true;
    }
    if rect.min.x > viewport.max.x || rect.min.y > viewport.max.y {
        return true;
    }
    false
}

#[inline]
pub fn is_line_fully_offscreen(viewport: &Rect, from: &Vec2, to: &Vec2) -> bool {
    if (from.x < viewport.min.x && to.x < viewport.min.x) ||
       (from.y < viewport.min.y && to.y < viewport.min.y) {
        return true;
    }
    if (from.x > viewport.max.x && to.x > viewport.max.x) ||
       (from.y > viewport.max.y && to.y > viewport.max.y) {
        return true;
    }
    false
}

#[inline]
pub fn is_point_fully_offscreen(viewport: &Rect, pt: &Vec2) -> bool {
    if pt.x < viewport.min.x || pt.y < viewport.min.y {
        return true;
    }
    if pt.x > viewport.max.x || pt.y > viewport.max.y {
        return true;
    }
    false
}

// ----------------------------------------------
// TextureHandle
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum TextureHandle {
    Invalid,   // Returns built-in dummy_texture.
    White,     // Returns built-in white_texture.
    Index(u32) // Index into TextureCache array of textures.
}

impl TextureHandle {
    #[inline]
    pub const fn invalid() -> Self {
        TextureHandle::Invalid
    }

    #[inline]
    pub const fn white() -> Self {
        TextureHandle::White
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        !matches!(self, TextureHandle::Invalid)
    }
}

impl Default for TextureHandle {
    fn default() -> Self { TextureHandle::invalid() }
}

pub struct NativeTextureHandle {
    pub bits: usize,
}

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

pub trait TextureCache: Any {
    fn as_any(&self) -> &dyn Any;
    fn load_texture(&mut self, file_path: &str) -> TextureHandle;
    fn to_native_handle(&self, handle: TextureHandle) -> NativeTextureHandle;
}
