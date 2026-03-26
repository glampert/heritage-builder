use serde::{Serialize, Deserialize};
use enum_dispatch::enum_dispatch;
use strum::Display;

use crate::{
    ui::UiRenderFrameBundle,
    utils::{Vec2, Size, Color, Rect, RectTexCoords, time::Milliseconds, mem::RcMut},
};

pub mod debug;
pub mod texture;

// ----------------------------------------------
// Internal backend implementations
// ----------------------------------------------

//mod wgpu;
mod opengl;

#[enum_dispatch]
enum RenderSystemBackendImpl {
//    Wgpu(wgpu::WgpuRenderSystemBackend),
    OpenGl(opengl::OpenGlRenderSystemBackend),
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum RenderApi {
    #[default]
//    Wgpu,
    OpenGl,
}

// ----------------------------------------------
// RenderSystemBackend
// ----------------------------------------------

#[enum_dispatch(RenderSystemBackendImpl)]
trait RenderSystemBackend: Sized {
    // Initialization:
    fn initialize(&mut self,
                  params: &RenderSystemInitParams,
                  tex_cache: &mut texture::TextureCache);

    // Begin/End frame:
    fn begin_frame(&mut self,
                   viewport_size: Size,
                   framebuffer_size: Size);

    fn end_frame(&mut self,
                 ui_frame_bundle: &mut UiRenderFrameBundle,
                 tex_cache: &mut texture::TextureCache)
                 -> RenderStats;

    // Viewport/Framebuffer:
    fn viewport(&self) -> Rect;
    fn set_viewport_size(&mut self, new_size: Size);
    fn set_framebuffer_size(&mut self, new_size: Size);

    // UI (ImGui) Drawing:
    fn begin_ui_render(&mut self);
    fn end_ui_render(&mut self);

    fn set_ui_draw_buffers(&mut self,
                           vtx_buffer: &[imgui::DrawVert],
                           idx_buffer: &[imgui::DrawIdx]);

    fn draw_ui_elements(&mut self,
                        first_index: u32,
                        index_count: u32,
                        texture: texture::TextureHandle,
                        tex_cache: &mut texture::TextureCache,
                        clip_rect: Rect);

    // Draw commands:
    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[u16],
                                      color: Color);

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: texture::TextureHandle,
                                  color: Color);

    // Line/point debug drawing:
    fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color);
    fn draw_point(&mut self, pt: Vec2, color: Color, size: f32);

    // Texture Allocation:
    fn new_texture_from_pixels(&mut self,
                               name: &str,
                               size: Size,
                               pixels: &[u8],
                               settings: texture::TextureSettings,
                               allow_settings_change: bool)
                               -> texture::TextureBackendImpl;

    fn update_texture_pixels(&mut self,
                             texture: &mut texture::TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8]);

    fn update_texture_settings(&mut self,
                               texture: &mut texture::TextureBackendImpl,
                               settings: texture::TextureSettings);

    fn release_texture(&mut self, texture: &mut texture::TextureBackendImpl);
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
    pub render_submit_time_ms: Milliseconds,

    // Peaks for the whole run:
    pub peak_triangles_drawn: u32,
    pub peak_lines_drawn: u32,
    pub peak_points_drawn: u32,
    pub peak_texture_changes: u32,
    pub peak_draw_calls: u32,
}

// ----------------------------------------------
// RenderSystemInitParams
// ----------------------------------------------

pub struct RenderSystemInitParams<'a> {
    pub render_api: RenderApi,
    pub viewport_size: Size,
    pub framebuffer_size: Size,
    pub clear_color: Color,
    pub texture_settings: texture::TextureSettings,
    pub tex_cache_initial_capacity: usize,
    pub app_context: Option<&'a dyn std::any::Any>,
}

impl Default for RenderSystemInitParams<'_> {
    fn default() -> Self {
        Self {
            render_api: RenderApi::default(),
            viewport_size: Size::new(1024, 768),
            framebuffer_size: Size::new(1024, 768),
            clear_color: Color::black(),
            texture_settings: texture::TextureSettings::default(),
            tex_cache_initial_capacity: 128, // Hint only, can grow.
            app_context: None,
        }
    }
}

// ----------------------------------------------
// RenderSystem
// ----------------------------------------------

pub struct RenderSystem {
    render_api: RenderApi,
    backend: RenderSystemBackendImpl,
    tex_cache: texture::TextureCache,
}

impl RenderSystem {
    pub fn render_api(&self) -> RenderApi {
        self.render_api
    }

    // ----------------------
    // Initialization:
    // ----------------------

    pub fn new(params: &RenderSystemInitParams) -> RcMut<Self> {
        debug_assert!(params.viewport_size.is_valid());
        debug_assert!(params.framebuffer_size.is_valid());

        let mut render_system = RcMut::new_cyclic(|render_system| {
            let backend = match params.render_api {
//                RenderApi::Wgpu   => RenderSystemBackendImpl::from(wgpu::WgpuRenderSystemBackend::new()),
                RenderApi::OpenGl => RenderSystemBackendImpl::from(opengl::OpenGlRenderSystemBackend::new()),
            };

            let tex_cache = texture::TextureCache::new(
                render_system,
                params.tex_cache_initial_capacity,
                params.texture_settings
            );

            Self { render_api: params.render_api, backend, tex_cache }
        });

        render_system.initialize(params);
        render_system
    }

    // Post-construction initialization.
    fn initialize(&mut self, params: &RenderSystemInitParams) {
        self.tex_cache.initialize(params);
        self.backend.initialize(params, &mut self.tex_cache);
    }

    // ----------------------
    // Render frame markers:
    // ----------------------

    #[inline]
    pub fn begin_frame(&mut self, viewport_size: Size, framebuffer_size: Size) {
        self.backend.begin_frame(viewport_size, framebuffer_size);
    }

    #[inline]
    pub fn end_frame(&mut self, ui_frame_bundle: &mut UiRenderFrameBundle) -> RenderStats {
        self.backend.end_frame(ui_frame_bundle, &mut self.tex_cache)
    }

    // ----------------------
    // TextureCache access:
    // ----------------------

    #[inline]
    pub fn texture_cache(&self) -> &texture::TextureCache {
        &self.tex_cache
    }

    #[inline]
    pub fn texture_cache_mut(&mut self) -> &mut texture::TextureCache {
        &mut self.tex_cache
    }

    // ----------------------
    // Viewport:
    // ----------------------

    #[inline]
    pub fn viewport(&self) -> Rect {
        self.backend.viewport()
    }

    #[inline]
    pub fn set_viewport_size(&mut self, new_size: Size) {
        self.backend.set_viewport_size(new_size)
    }

    #[inline]
    pub fn set_framebuffer_size(&mut self, new_size: Size) {
        self.backend.set_framebuffer_size(new_size)
    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    #[inline]
    pub fn begin_ui_render(&mut self) {
        self.backend.begin_ui_render();
    }

    #[inline]
    pub fn end_ui_render(&mut self) {
        self.backend.end_ui_render();
    }

    #[inline]
    pub fn set_ui_draw_buffers(&mut self,
                               vtx_buffer: &[imgui::DrawVert],
                               idx_buffer: &[imgui::DrawIdx])
    {
        self.backend.set_ui_draw_buffers(vtx_buffer, idx_buffer);
    }

    #[inline]
    pub fn draw_ui_elements(&mut self,
                            first_index: u32,
                            index_count: u32,
                            texture: texture::TextureHandle,
                            clip_rect: Rect)
    {
        self.backend.draw_ui_elements(first_index, index_count, texture, &mut self.tex_cache, clip_rect);
    }

    // ----------------------
    // Draw commands:
    // ----------------------

    // This is used for emulated line drawing with custom thickness.
    #[inline]
    pub fn draw_colored_indexed_triangles(&mut self,
                                          vertices: &[Vec2],
                                          indices: &[u16],
                                          color: Color)
    {
        self.backend.draw_colored_indexed_triangles(vertices, indices, color);
    }

    // This is used for drawing sprite rectangles. There is a special case with
    // `texture=TextureHandle::white()` for drawing rectangles with color only.
    #[inline]
    pub fn draw_textured_colored_rect(&mut self,
                                      rect: Rect,
                                      tex_coords: &RectTexCoords,
                                      texture: texture::TextureHandle,
                                      color: Color)
    {
        self.backend.draw_textured_colored_rect(rect, tex_coords, texture, color);
    }

    #[inline]
    pub fn draw_colored_rect(&mut self, rect: Rect, color: Color) {
        // Just call this with the default white texture.
        self.draw_textured_colored_rect(rect,
                                        &RectTexCoords::DEFAULT,
                                        texture::TextureHandle::white(),
                                        color);
    }

    pub fn draw_wireframe_rect_with_thickness(&mut self,
                                              rect: Rect,
                                              color: Color,
                                              thickness: f32)
    {
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

    // This can handle straight lines efficiently but might produce discontinuities
    // at connecting edges of rectangles and other polygons. To draw connecting
    // lines/polygons use draw_polyline_with_thickness().
    pub fn draw_line_with_thickness(&mut self,
                                    from_pos: Vec2,
                                    to_pos: Vec2,
                                    color: Color,
                                    thickness: f32)
    {
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
    pub fn draw_polyline_with_thickness(&mut self,
                                        points: &[Vec2],
                                        color: Color,
                                        thickness: f32,
                                        is_closed: bool)
    {
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
                if is_closed {
                    points[num_points - 1]
                } else {
                    points[0]
                }
            } else {
                points[i - 1]
            };

            let curr = points[i];

            let next = if i == num_points - 1 {
                if is_closed {
                    points[0]
                } else {
                    points[num_points - 1]
                }
            } else {
                points[i + 1]
            };

            // Compute averaged normal (miter join)
            let dir1 = (curr - prev).normalize();
            let dir2 = (next - curr).normalize();

            let normal1 = Vec2::new(-dir1.y, dir1.x);
            let normal2 = Vec2::new(-dir2.y, dir2.x);
            let avg_normal = (normal1 + normal2).normalize();

            // Limit how far the join stretches by clamping the offset length to a maximum
            // miter limit.
            // - Scale the miter by 1 / dot(normal1, miter) to preserve line thickness.
            // - Clamp it to avoid distortions at sharp angles.
            // - This results in smooth joins that don’t overextend, making diamond shapes
            //   look uniform.
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

    // Simple line and point drawing, mainly used for debugging.
    // These lines and points are batched separately and drawn
    // on top of all sprites so they will not respect draw order
    // in relation to textured sprites and colored polygons.
    #[inline]
    pub fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        self.backend.draw_line(from_pos, to_pos, from_color, to_color);
    }

    #[inline]
    pub fn draw_point(&mut self, pt: Vec2, color: Color, size: f32) {
        self.backend.draw_point(pt, color, size);
    }

    pub fn draw_wireframe_rect(&mut self, rect: Rect, color: Color) {
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
            self.draw_line(pair[0], pair[1], color, color);
        }
    }

    // ----------------------
    // Texture Allocation:
    // ----------------------

    #[inline]
    fn new_texture_from_pixels(&mut self,
                               name: &str,
                               size: Size,
                               pixels: &[u8],
                               settings: texture::TextureSettings,
                               allow_settings_change: bool)
                               -> texture::TextureBackendImpl
    {
        self.backend.new_texture_from_pixels(name, size, pixels, settings, allow_settings_change)
    }

    #[inline]
    fn update_texture_pixels(&mut self,
                             texture: &mut texture::TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8])
    {
        self.backend.update_texture_pixels(texture, offset_x, offset_y, size, mip_level, pixels);
    }

    #[inline]
    fn update_texture_settings(&mut self,
                               texture: &mut texture::TextureBackendImpl,
                               settings: texture::TextureSettings)
    {
        self.backend.update_texture_settings(texture, settings);
    }

    #[inline]
    fn release_texture(&mut self, texture: &mut texture::TextureBackendImpl) {
        self.backend.release_texture(texture);
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
    if (from.x < viewport.min.x && to.x < viewport.min.x)
       || (from.y < viewport.min.y && to.y < viewport.min.y)
    {
        return true;
    }
    if (from.x > viewport.max.x && to.x > viewport.max.x)
       || (from.y > viewport.max.y && to.y > viewport.max.y)
    {
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
