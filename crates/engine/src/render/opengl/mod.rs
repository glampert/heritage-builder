use std::ffi::{CStr, c_char};

use arrayvec::ArrayVec;
use common::{Color, Rect, RectTexCoords, Size, Vec2, time::PerfTimer};
use batch::*;
use context::*;
use shader::*;
use target::*;
use texture::*;
use vertex::*;

pub use texture::OpenGlTexture;

use super::{
    RenderApi,
    RenderStats,
    RenderSystemBackend,
    RenderSystemInitParams,
    capture::{self, CaptureQueue, CaptureResult, CapturedImage},
    ui_draw::UiDrawData,
};
use crate::log;

mod batch;
mod buffer;
mod context;
mod shader;
mod target;
mod texture;
mod vertex;

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
        self.render_context
            .set_viewport(Rect::from_pos_and_size(Vec2::zero(), self.offscreen_render_target.size().to_vec2()));

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
        self.sprites_batch.draw_entries(&mut self.render_context, &self.sprites_shader.program, set_shader_vars_fn);
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

    // Draws the UI over what's in the screen's back buffer: one upload, then a draw per command.
    fn draw_ui(&mut self, ui_draw_data: &UiDrawData, tex_cache: &mut super::texture::TextureCache) {
        debug_assert!(self.frame_started);

        if ui_draw_data.is_empty() {
            return;
        }

        self.ui_batch.upload(&mut self.render_context, ui_draw_data.vertices(), ui_draw_data.indices());

        self.render_context
            .set_alpha_blend(AlphaBlend::Premultiplied)
            .set_clip_test(ClipTest::Enabled)
            .set_primitive_topology(PrimitiveTopology::Triangles)
            .set_shader_program(&self.ui_shader.program);

        let framebuffer_height = self.framebuffer_size.height as u32;
        let mut last_texture = None;

        for command in ui_draw_data.commands() {
            // glScissor counts rows from the bottom, UiClipRect from the top.
            let clip = command.clip_rect;
            let clip_y = framebuffer_height.saturating_sub(clip.y + clip.height);
            self.render_context.set_clip_rect(clip.x, clip_y, clip.width, clip.height);

            if last_texture != Some(command.texture) {
                last_texture = Some(command.texture);
                let gl_texture = tex_cache.texture_for_handle(command.texture).as_opengl();
                self.ui_shader.set_sprite_texture(gl_texture);
                self.render_context.set_texture(gl_texture);
            }

            self.ui_batch.draw(&mut self.render_context, command);
            self.stats.triangles_drawn += command.index_count / 3;
        }

        self.render_context.unset_vertex_array();
        self.render_context.unset_shader_program();

        self.render_context.set_clip_test(ClipTest::Disabled).set_alpha_blend(AlphaBlend::Enabled);
    }

    // Reads back what has been drawn to the screen's back buffer so far.
    fn read_screen_pixels(&self) -> CaptureResult {
        let size = self.framebuffer_size;
        let row_bytes = size.width as usize * 4;
        let mut pixels = vec![0u8; row_bytes * size.height as usize];

        // Errors from earlier in the frame would otherwise be blamed on the readback.
        panic_if_gl_error();

        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, buffer::NULL_BUFFER_HANDLE);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(0, 0, size.width, size.height, gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_mut_ptr().cast());

            let error_code = gl::GetError();
            if error_code != gl::NO_ERROR {
                return Err(format!("glReadPixels failed: {} (0x{:X})", gl_error_to_string(error_code), error_code));
            }
        }

        capture::flip_rows(&mut pixels, row_bytes);
        capture::make_opaque(&mut pixels);

        Ok(CapturedImage { size, scale: capture::content_scale(size, self.viewport), pixels })
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

        let offscreen_render_target =
            RenderTarget::new(params.viewport_size.max(params.framebuffer_size), TextureFilter::Linear);

        let mut s = Box::new(OpenGlSystemState {
            frame_started: false,
            stats: RenderStats::default(),
            render_context: RenderContext::new(),

            viewport: Rect::default(),
            framebuffer_size: Size::default(),
            offscreen_render_target,

            sprites_batch: DrawBatch::new(512, 512, 512, PrimitiveTopology::Triangles),
            sprites_shader: sprites::Shader::load(),

            lines_batch: DrawBatch::new(64, 64, 0, PrimitiveTopology::Lines),
            lines_shader: lines::Shader::load(),

            points_batch: DrawBatch::new(64, 64, 0, PrimitiveTopology::Points),
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

    fn end_frame(
        &mut self,
        ui_draw_data: &UiDrawData,
        tex_cache: &mut super::texture::TextureCache,
        captures: &mut CaptureQueue,
    ) -> RenderStats {
        let s = self.state_mut();
        debug_assert!(s.framebuffer_size.is_valid());

        let render_submit_timer = PerfTimer::begin();
        let pending_captures = captures.take_pending();

        s.flush_sprites(tex_cache);
        s.flush_lines();
        s.flush_points();

        // Blit OffscreenRT to the screen framebuffer.
        s.offscreen_render_target.blit_to_screen(s.framebuffer_size);

        // Reset viewport to default screen framebuffer size.
        s.render_context.set_viewport(Rect::from_pos_and_size(Vec2::zero(), s.framebuffer_size.to_vec2()));

        // The back buffer now holds the scene alone, at screen resolution.
        if !pending_captures.scene.is_empty() {
            captures.finish(&pending_captures.scene, s.read_screen_pixels());
        }

        // Render UI last so it will draw over the tile map.
        s.draw_ui(ui_draw_data, tex_cache);

        if !pending_captures.with_ui.is_empty() {
            captures.finish(&pending_captures.with_ui, s.read_screen_pixels());
        }

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
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self, vertices: &[Vec2], indices: &[super::DrawIndex], color: Color) {
        let s = self.state_mut();
        debug_assert!(s.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!(indices.len().is_multiple_of(3)); // We expect triangles.

        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();

        // Expand to sprite vertices with defaulted (unused) texture coordinates.
        for vert in vertices {
            sprite_verts.push(SpriteVertex2D { position: *vert, tex_coords: Vec2::default() });
        }

        s.sprites_batch.add_entry(&sprite_verts, indices, super::texture::TextureHandle::white(), color);

        s.stats.triangles_drawn += (indices.len() / 3) as u32;
    }

    fn draw_textured_colored_rect(
        &mut self,
        rect: Rect,
        tex_coords: &RectTexCoords,
        texture: super::texture::TextureHandle,
        color: Color,
    ) {
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

    fn new_texture_from_pixels(
        &mut self,
        name: &str,
        size: Size,
        pixels: &[u8],
        settings: super::texture::TextureSettings,
        allow_settings_change: bool,
    ) -> super::texture::TextureBackendImpl {
        let gl_texture = OpenGlTexture::new(TextureCreationParams {
            name,
            size,
            pixels,
            settings: TextureSettings::from(settings),
            tex_unit: TextureUnit(0),
            allow_settings_change,
        });

        super::texture::TextureBackendImpl::OpenGl(gl_texture)
    }

    fn update_texture_pixels(
        &mut self,
        texture: &mut super::texture::TextureBackendImpl,
        offset_x: u32,
        offset_y: u32,
        size: Size,
        mip_level: u32,
        pixels: &[u8],
    ) {
        let gl_texture = texture.as_opengl_mut();
        gl_texture.update(offset_x, offset_y, size, mip_level, pixels);
    }

    fn update_texture_settings(
        &mut self,
        texture: &mut super::texture::TextureBackendImpl,
        settings: super::texture::TextureSettings,
    ) {
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
            log::info!(
                log::channel!("render"),
                "GL_VERSION: {}",
                CStr::from_ptr(gl_version as *const c_char).to_str().unwrap()
            );
        }

        let gl_vendor = gl::GetString(gl::VENDOR);
        if !gl_vendor.is_null() {
            log::info!(
                log::channel!("render"),
                "GL_VENDOR: {}",
                CStr::from_ptr(gl_vendor as *const c_char).to_str().unwrap()
            );
        }

        let glsl_version = gl::GetString(gl::SHADING_LANGUAGE_VERSION);
        if !glsl_version.is_null() {
            log::info!(
                log::channel!("render"),
                "GLSL_VERSION: {}",
                CStr::from_ptr(glsl_version as *const c_char).to_str().unwrap()
            );
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
