use std::ffi::{CStr, c_char, c_void};
use arrayvec::ArrayVec;

use batch::*;
use context::*;
use shader::*;
use texture::*;
use vertex::*;
use target::*;

pub use texture::OpenGlTexture;

use super::{
    RenderApi,
    RenderStats,
    RenderSystemBackend,
    RenderSystemInitParams,
};
use crate::{
    log,
    ui::UiRenderFrameBundle,
    utils::{Vec2, Size, Color, Rect, RectTexCoords, time::PerfTimer},
};

mod batch;
mod buffer;
mod context;
mod shader;
mod texture;
mod vertex;
mod target;

// ----------------------------------------------
// OpenGlSystemState
// ----------------------------------------------

struct OpenGlSystemState {
    frame_started: bool,
    stats: RenderStats,
    render_context: RenderContext,

    viewport: Rect,
    framebuffer_size: Size,
    offscreen_render_target: RenderTarget,

    sprites_batch: DrawBatch<SpriteVertex2D, SpriteIndex2D>,
    sprites_shader: sprites::Shader,

    lines_batch: DrawBatch<LineVertex2D, LineIndex2D>,
    lines_shader: lines::Shader,

    points_batch: DrawBatch<PointVertex2D, PointIndex2D>,
    points_shader: points::Shader,

    ui_batch: UiDrawBatch,
    ui_shader: ui::Shader,
}

impl OpenGlSystemState {
    fn set_viewport_size(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());
        self.viewport = Rect::from_pos_and_size(Vec2::zero(), new_size.to_vec2());

        // NOTE: Set render viewport to render target size; everything else is set
        // to the virtual viewport size, so we decouple rendering resolution from
        // logical viewport. 
        self.render_context.set_viewport(
            Rect::from_pos_and_size(Vec2::zero(), self.offscreen_render_target.size().to_vec2())
        );

        self.sprites_shader.set_viewport_size(self.viewport.size());
        self.lines_shader.set_viewport_size(self.viewport.size());
        self.points_shader.set_viewport_size(self.viewport.size());
        self.ui_shader.set_viewport_size(self.viewport.size());
    }

    fn set_framebuffer_size(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());
        self.framebuffer_size = new_size;
    }

    fn flush_sprites(&mut self, tex_cache: &mut super::texture::TextureCache) {
        debug_assert!(self.frame_started);

        let set_shader_vars_fn = |render_ctx: &mut RenderContext, entry: &DrawBatchEntry| {
            let gl_texture = tex_cache.texture_for_handle(entry.texture).as_opengl();
            render_ctx.set_texture(gl_texture);

            self.sprites_shader.set_sprite_tint(entry.color);
            self.sprites_shader.set_sprite_texture(gl_texture);
        };

        self.sprites_batch.sync();
        self.sprites_batch.draw_entries(&mut self.render_context,
                                        &self.sprites_shader.program,
                                        set_shader_vars_fn);
        self.sprites_batch.clear();
    }

    fn flush_lines(&mut self) {
        debug_assert!(self.frame_started);

        self.lines_batch.sync();
        self.lines_batch.draw_fast(&mut self.render_context, &self.lines_shader.program);
        self.lines_batch.clear();
    }

    fn flush_points(&mut self) {
        debug_assert!(self.frame_started);

        self.points_batch.sync();
        self.points_batch.draw_fast(&mut self.render_context, &self.points_shader.program);
        self.points_batch.clear();
    }
}

// ----------------------------------------------
// OpenGlRenderSystemBackend
// ----------------------------------------------

pub struct OpenGlRenderSystemBackend {
    state: Box<OpenGlSystemState>,
}

impl OpenGlRenderSystemBackend {
    // ----------------------
    // Initialization:
    // ----------------------

    pub fn new(params: &RenderSystemInitParams) -> Self {
        debug_assert!(params.render_api == RenderApi::OpenGl);
        debug_assert!(params.viewport_size.is_valid());
        debug_assert!(params.framebuffer_size.is_valid());

        log::info!(log::channel!("render"), "--- Render Backend: OpenGL ---");

        // Pure 2D rendering, without depth buffer.
        const WITH_DEPTH_BUFFER: bool = false;
        let offscreen_render_target = RenderTarget::new(
            params.viewport_size.max(params.framebuffer_size),
            WITH_DEPTH_BUFFER,
            TextureFilter::Linear,
            "offscreen_render_target"
        );

        let mut s = Box::new(OpenGlSystemState {
            frame_started: false,
            stats: RenderStats::default(),
            render_context: RenderContext::new(),

            viewport: Rect::default(),
            framebuffer_size: Size::default(),
            offscreen_render_target,

            sprites_batch: DrawBatch::new(512, 512, 512, PrimitiveTopology::Triangles),
            sprites_shader: sprites::Shader::load(),

            lines_batch: DrawBatch::new(8, 8, 0, PrimitiveTopology::Lines),
            lines_shader: lines::Shader::load(),

            points_batch: DrawBatch::new(8, 8, 0, PrimitiveTopology::Points),
            points_shader: points::Shader::load(),

            ui_batch: UiDrawBatch::new(),
            ui_shader: ui::Shader::load(),
        });

        s.set_viewport_size(params.viewport_size);
        s.set_framebuffer_size(params.framebuffer_size);

        // Pure 2D rendering, no depth test or back-face culling.
        s.render_context
            .set_clear_color(params.clear_color)
            .set_alpha_blend(AlphaBlend::Enabled)
            .set_backface_culling(BackFaceCulling::Disabled)
            .set_depth_test(DepthTest::Disabled)
            .set_clip_test(ClipTest::Disabled);

        Self { state: s }
    }

    #[inline]
    fn state(&self) -> &OpenGlSystemState {
        &self.state
    }

    #[inline]
    fn state_mut(&mut self) -> &mut OpenGlSystemState {
        &mut self.state
    }
}

impl RenderSystemBackend for OpenGlRenderSystemBackend {
    // ----------------------
    // Begin/End frame:
    // ----------------------

    fn begin_frame(&mut self, viewport_size: Size, framebuffer_size: Size) {
        let s = self.state_mut();
        debug_assert!(!s.frame_started);

        s.render_context.set_offscreen_render_target(&s.offscreen_render_target);
        s.set_viewport_size(viewport_size);
        s.set_framebuffer_size(framebuffer_size);

        s.render_context.begin_frame();
        s.frame_started = true;

        s.stats.triangles_drawn  = 0;
        s.stats.lines_drawn      = 0;
        s.stats.points_drawn     = 0;
        s.stats.texture_changes  = 0;
        s.stats.draw_calls       = 0;

        s.stats.render_submit_time_ms = 0.0;
    }

    fn end_frame(&mut self,
                 ui_frame_bundle: &mut UiRenderFrameBundle,
                 tex_cache: &mut super::texture::TextureCache)
                 -> RenderStats
    {
        let s = self.state_mut();
        debug_assert!(s.framebuffer_size.is_valid());

        let render_submit_timer = PerfTimer::begin();

        s.flush_sprites(tex_cache);
        s.flush_lines();
        s.flush_points();

        // Blit OffscreenRT to the screen framebuffer.
        s.offscreen_render_target.blit_to_screen(s.framebuffer_size);

        // Reset viewport to default screen framebuffer size.
        s.render_context.set_viewport(Rect::from_pos_and_size(Vec2::zero(), s.framebuffer_size.to_vec2()));

        // Render UI last so it will draw over the tile map.
        ui_frame_bundle.render();

        s.render_context.end_frame();
        s.frame_started = false;

        s.stats.render_submit_time_ms = render_submit_timer.end();

        s.stats.texture_changes      = s.render_context.texture_changes();
        s.stats.draw_calls           = s.render_context.draw_calls();
        s.stats.peak_triangles_drawn = s.stats.triangles_drawn.max(s.stats.peak_triangles_drawn);
        s.stats.peak_lines_drawn     = s.stats.lines_drawn.max(s.stats.peak_lines_drawn);
        s.stats.peak_points_drawn    = s.stats.points_drawn.max(s.stats.peak_points_drawn);
        s.stats.peak_texture_changes = s.stats.texture_changes.max(s.stats.peak_texture_changes);
        s.stats.peak_draw_calls      = s.stats.draw_calls.max(s.stats.peak_draw_calls);

        s.stats
    }

    // ----------------------
    // Viewport/Framebuffer:
    // ----------------------

    #[inline]
    fn viewport(&self) -> Rect {
        self.state().viewport
    }

    #[inline]
    fn set_viewport_size(&mut self, new_size: Size) {
        self.state_mut().set_viewport_size(new_size);
    }

    #[inline]
    fn set_framebuffer_size(&mut self, new_size: Size) {
        self.state_mut().set_framebuffer_size(new_size);
    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    fn begin_ui_render(&mut self) {
        let s = self.state_mut();
        s.render_context.set_clip_test(ClipTest::Enabled);
        s.ui_batch.begin(&mut s.render_context, &s.ui_shader.program);
    }

    fn end_ui_render(&mut self) {
        let s = self.state_mut();
        s.ui_batch.end(&mut s.render_context);
        s.render_context.set_clip_test(ClipTest::Disabled);
    }

    fn set_ui_draw_buffers(&mut self,
                           vtx_buffer: &[super::UiDrawVertex],
                           idx_buffer: &[super::UiDrawIndex])
    {
        debug_assert!(!vtx_buffer.is_empty() && !idx_buffer.is_empty());
        let s = self.state_mut();
        s.ui_batch.sync(&mut s.render_context, vtx_buffer, idx_buffer);
    }

    fn draw_ui_elements(&mut self,
                        first_index: u32,
                        index_count: u32,
                        texture: super::texture::TextureHandle,
                        tex_cache: &mut super::texture::TextureCache,
                        clip_rect: Rect)
    {
        debug_assert!(index_count.is_multiple_of(3)); // We expect triangles.

        let s = self.state_mut();
        s.render_context.set_clip_rect(clip_rect);

        let gl_texture = tex_cache.texture_for_handle(texture).as_opengl();
        s.ui_shader.set_sprite_texture(gl_texture);
        s.render_context.set_texture(gl_texture);

        s.ui_batch.draw(&mut s.render_context, first_index, index_count);
        s.stats.triangles_drawn += index_count / 3;
    }

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[super::DrawIndex],
                                      color: Color)
    {
        let s = self.state_mut();
        debug_assert!(s.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!(indices.len().is_multiple_of(3)); // We expect triangles.

        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();

        // Expand to sprite vertices with defaulted (unused) texture coordinates.
        for vert in vertices {
            sprite_verts.push(SpriteVertex2D {
                position: *vert,
                tex_coords: Vec2::default(),
            });
        }

        s.sprites_batch.add_entry(
            &sprite_verts,
            indices,
            super::texture::TextureHandle::white(),
            color
        );

        s.stats.triangles_drawn += (indices.len() / 3) as u32;
    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: super::texture::TextureHandle,
                                  color: Color)
    {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_rect_fully_offscreen(&s.viewport, &rect) {
            return; // Cull if fully offscreen.
        }

        let vertices = [
            SpriteVertex2D { position: rect.bottom_left(),  tex_coords: tex_coords.bottom_left()  },
            SpriteVertex2D { position: rect.top_left(),     tex_coords: tex_coords.top_left()     },
            SpriteVertex2D { position: rect.top_right(),    tex_coords: tex_coords.top_right()    },
            SpriteVertex2D { position: rect.bottom_right(), tex_coords: tex_coords.bottom_right() },
        ];

        const INDICES: [SpriteIndex2D; 6] = [
            0, 1, 2, // first triangle
            2, 3, 0, // second triangle
        ];

        s.sprites_batch.add_entry(&vertices, &INDICES, texture, color);
        s.stats.triangles_drawn += 2;
    }

    // ----------------------
    // Debug drawing:
    // ----------------------

    fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_line_fully_offscreen(&s.viewport, &from_pos, &to_pos) {
            return; // Cull if fully offscreen.
        }

        let vertices = [
            LineVertex2D { position: from_pos, color: from_color },
            LineVertex2D { position: to_pos,   color: to_color   },
        ];

        const INDICES: [LineIndex2D; 2] = [0, 1];

        s.lines_batch.add_fast(&vertices, &INDICES);
        s.stats.lines_drawn += 1;
    }

    fn draw_point(&mut self, pt: Vec2, color: Color, size: f32) {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_point_fully_offscreen(&s.viewport, &pt) {
            return; // Cull if fully offscreen.
        }

        let vertices = [PointVertex2D { position: pt, color, size }];
        const INDICES: [PointIndex2D; 1] = [0];

        s.points_batch.add_fast(&vertices, &INDICES);
        s.stats.points_drawn += 1;
    }

    // ----------------------
    // Texture Allocation:
    // ----------------------

    fn new_texture_from_pixels(&mut self,
                               name: &str,
                               size: Size,
                               pixels: &[u8],
                               settings: super::texture::TextureSettings,
                               allow_settings_change: bool)
                               -> super::texture::TextureBackendImpl
    {
        let data = if pixels.is_empty() {
            std::ptr::null()
        } else {
            pixels.as_ptr() as *const c_void
        };

        let gl_texture = OpenGlTexture::with_data_raw(
            name,
            size,
            data,
            TextureSettings::from(settings),
            TextureUnit(0),
            allow_settings_change,
        );

        super::texture::TextureBackendImpl::OpenGl(gl_texture)
    }

    fn update_texture_pixels(&mut self,
                             texture: &mut super::texture::TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8])
    {
        let gl_texture = texture.as_opengl_mut();
        gl_texture.update(offset_x, offset_y, size, mip_level, pixels);
    }

    fn update_texture_settings(&mut self,
                               texture: &mut super::texture::TextureBackendImpl,
                               settings: super::texture::TextureSettings)
    {
        let gl_texture = texture.as_opengl_mut();
        gl_texture.change_settings(TextureSettings::from(settings));
    }

    fn release_texture(&mut self, texture: &mut super::texture::TextureBackendImpl) {
        let gl_texture = texture.as_opengl_mut();
        gl_texture.release();
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

fn log_gl_info() {
    unsafe {
        let gl_version = gl::GetString(gl::VERSION);
        if !gl_version.is_null() {
            log::info!(log::channel!("render"),
                       "GL_VERSION: {}",
                       CStr::from_ptr(gl_version as *const c_char).to_str().unwrap());
        }

        let gl_vendor = gl::GetString(gl::VENDOR);
        if !gl_vendor.is_null() {
            log::info!(log::channel!("render"),
                       "GL_VENDOR: {}",
                       CStr::from_ptr(gl_vendor as *const c_char).to_str().unwrap());
        }

        let glsl_version = gl::GetString(gl::SHADING_LANGUAGE_VERSION);
        if !glsl_version.is_null() {
            log::info!(log::channel!("render"),
                       "GLSL_VERSION: {}",
                       CStr::from_ptr(glsl_version as *const c_char).to_str().unwrap());
        }
    }
}

fn gl_error_to_string(error: gl::types::GLenum) -> &'static str {
    match error {
        gl::NO_ERROR => "No error",
        gl::INVALID_ENUM => "Invalid enum",
        gl::INVALID_VALUE => "Invalid value",
        gl::INVALID_OPERATION => "Invalid operation",
        gl::STACK_OVERFLOW => "Stack overflow",
        gl::STACK_UNDERFLOW => "Stack underflow",
        gl::OUT_OF_MEMORY => "Out of memory",
        gl::INVALID_FRAMEBUFFER_OPERATION => "Invalid framebuffer operation",
        _ => "Unknown error",
    }
}

fn panic_if_gl_error() {
    let error_code = unsafe { gl::GetError() };
    if error_code != gl::NO_ERROR {
        panic!("OpenGL Error: {} (0x{:X})", gl_error_to_string(error_code), error_code);
    }
}
