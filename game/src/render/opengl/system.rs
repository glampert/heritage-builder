use std::any::Any;
use arrayvec::ArrayVec;

use super::{
    batch::{DrawBatch, DrawBatchEntry, UiDrawBatch},
    texture::TextureCache,
    target::RenderTarget,
    context::*,
    shader::*,
    vertex::*,
};
use crate::{
    log,
    ui::UiRenderFrameBundle,
    render::{self, RenderStats, TextureHandle},
    utils::{Color, Rect, RectTexCoords, Size, Vec2, time::PerfTimer},
};

// ----------------------------------------------
// RenderSystem
// ----------------------------------------------

pub struct RenderSystem {
    frame_started: bool,
    render_context: RenderContext,
    sprites_batch: DrawBatch<SpriteVertex2D, SpriteIndex2D>,
    sprites_shader: sprites::Shader,
    lines_batch: DrawBatch<LineVertex2D, LineIndex2D>,
    lines_shader: lines::Shader,
    points_batch: DrawBatch<PointVertex2D, PointIndex2D>,
    points_shader: points::Shader,
    ui_batch: UiDrawBatch,
    ui_shader: ui::Shader,
    stats: RenderStats,
    viewport: Rect,
    framebuffer_size: Size,
    tex_cache: TextureCache,
    offscreen_render_target: RenderTarget,
}

impl RenderSystem {
    fn flush_sprites(&mut self) {
        debug_assert!(self.frame_started);

        let set_shader_vars_fn = |render_context: &mut RenderContext, entry: &DrawBatchEntry| {
            let texture2d = self.tex_cache.handle_to_texture(entry.texture);
            render_context.set_texture_2d(texture2d);

            self.sprites_shader.set_sprite_tint(entry.color);
            self.sprites_shader.set_sprite_texture(texture2d);
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

impl render::RenderSystemFactory for RenderSystem {
    fn new(viewport_size: Size,
           framebuffer_size: Size,
           clear_color: Color,
           texture_settings: render::TextureSettings,
           _app_context: Option<&dyn std::any::Any>) -> Self
    {
        debug_assert!(viewport_size.is_valid());
        debug_assert!(framebuffer_size.is_valid());

        log::info!(log::channel!("render"), "== Render Backend: OpenGL ==");

        let mut tex_cache = TextureCache::new(128, texture_settings);

        let with_depth_buffer = false; // Pure 2D rendering, no depth buffer.
        let offscreen_render_target = RenderTarget::new(
            &mut tex_cache,
            viewport_size.max(framebuffer_size),
            with_depth_buffer,
            render::TextureFilter::Linear,
            "OffscreenRT"
        );

        let mut render_sys = Self {
            frame_started: false,
            render_context: RenderContext::new(),
            sprites_batch: DrawBatch::new(512, 512, 512, PrimitiveTopology::Triangles),
            sprites_shader: sprites::Shader::load(),
            lines_batch: DrawBatch::new(8, 8, 0, PrimitiveTopology::Lines),
            lines_shader: lines::Shader::load(),
            points_batch: DrawBatch::new(8, 8, 0, PrimitiveTopology::Points),
            points_shader: points::Shader::load(),
            ui_batch: UiDrawBatch::new(),
            ui_shader: ui::Shader::load(),
            stats: RenderStats::default(),
            viewport: Rect::default(),
            framebuffer_size: Size::default(),
            tex_cache,
            offscreen_render_target,
        };

        use render::RenderSystem;
        render_sys.set_viewport_size(viewport_size);
        render_sys.set_framebuffer_size(framebuffer_size);

        render_sys.render_context
                  .set_clear_color(clear_color)
                  .set_alpha_blend(AlphaBlend::Enabled)
                  // Pure 2D rendering, no depth test or back-face culling.
                  .set_backface_culling(BackFaceCulling::Disabled)
                  .set_depth_test(DepthTest::Disabled)
                  .set_clip_test(ClipTest::Disabled);

        render_sys
    }
}

impl render::RenderSystem for RenderSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn begin_frame(&mut self, viewport_size: Size, framebuffer_size: Size) {
        debug_assert!(!self.frame_started);

        self.render_context.set_offscreen_render_target(&self.offscreen_render_target);
        self.set_viewport_size(viewport_size);
        self.set_framebuffer_size(framebuffer_size);

        self.render_context.begin_frame();
        self.frame_started = true;

        self.stats.triangles_drawn  = 0;
        self.stats.lines_drawn      = 0;
        self.stats.points_drawn     = 0;
        self.stats.texture_changes  = 0;
        self.stats.draw_calls       = 0;

        self.stats.render_submit_time_ms = 0.0;
    }

    fn end_frame(&mut self, ui_frame_bundle: &mut UiRenderFrameBundle) -> RenderStats {
        debug_assert!(self.frame_started);
        debug_assert!(self.viewport.is_valid());
        debug_assert!(self.framebuffer_size.is_valid());

        let render_submit_timer = PerfTimer::begin();

        self.flush_sprites();
        self.flush_lines();
        self.flush_points();

        // Blit OffscreenRT to the screen framebuffer.
        self.offscreen_render_target.blit_to_screen(self.framebuffer_size);

        // Reset viewport to default screen framebuffer size.
        self.render_context.set_viewport(Rect::from_pos_and_size(Vec2::zero(), self.framebuffer_size.to_vec2()));

        // Render UI last so it will draw over the tile map.
        ui_frame_bundle.render(self);

        self.render_context.end_frame();
        self.frame_started = false;

        self.stats.render_submit_time_ms = render_submit_timer.end();

        self.stats.texture_changes      = self.render_context.texture_changes();
        self.stats.draw_calls           = self.render_context.draw_calls();
        self.stats.peak_triangles_drawn = self.stats.triangles_drawn.max(self.stats.peak_triangles_drawn);
        self.stats.peak_lines_drawn     = self.stats.lines_drawn.max(self.stats.peak_lines_drawn);
        self.stats.peak_points_drawn    = self.stats.points_drawn.max(self.stats.peak_points_drawn);
        self.stats.peak_texture_changes = self.stats.texture_changes.max(self.stats.peak_texture_changes);
        self.stats.peak_draw_calls      = self.stats.draw_calls.max(self.stats.peak_draw_calls);

        self.stats
    }

    #[inline]
    fn texture_cache(&self) -> &dyn render::TextureCache {
        &self.tex_cache
    }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut dyn render::TextureCache {
        &mut self.tex_cache
    }

    #[inline]
    fn viewport(&self) -> Rect {
        self.viewport
    }

    #[inline]
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

    #[inline]
    fn set_framebuffer_size(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());
        self.framebuffer_size = new_size;
    }

    #[inline]
    fn begin_ui_render(&mut self) {
        self.render_context.set_clip_test(ClipTest::Enabled);
        self.ui_batch.begin(&mut self.render_context, &self.ui_shader.program);
    }

    #[inline]
    fn end_ui_render(&mut self) {
        self.ui_batch.end(&mut self.render_context);
        self.render_context.set_clip_test(ClipTest::Disabled);
    }

    #[inline]
    fn set_ui_draw_buffers(&mut self, vtx_buffer: &[imgui::DrawVert], idx_buffer: &[imgui::DrawIdx]) {
        debug_assert!(!vtx_buffer.is_empty() && !idx_buffer.is_empty());
        self.ui_batch.sync(&mut self.render_context, vtx_buffer, idx_buffer);
    }

    #[inline]
    fn draw_ui_elements(&mut self, first_index: u32, index_count: u32, texture: TextureHandle, clip_rect: Rect) {
        debug_assert!(index_count.is_multiple_of(3)); // We expect triangles.

        self.render_context.set_clip_rect(clip_rect);

        let texture2d = self.tex_cache.handle_to_texture(texture);
        self.ui_shader.set_sprite_texture(texture2d);
        self.render_context.set_texture_2d(texture2d);

        self.ui_batch.draw(&mut self.render_context, first_index, index_count);
        self.stats.triangles_drawn += index_count / 3;
    }

    fn draw_colored_indexed_triangles(&mut self, vertices: &[Vec2], indices: &[u16], color: Color) {
        debug_assert!(self.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!(indices.len().is_multiple_of(3)); // We expect triangles.

        // Expand to sprite vertices with defaulted (unused) texture coordinates.
        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();
        for vert in vertices {
            sprite_verts.push(SpriteVertex2D { position: *vert, tex_coords: Vec2::default() });
        }

        self.sprites_batch.add_entry(&sprite_verts, indices, TextureHandle::white(), color);
        self.stats.triangles_drawn += (indices.len() / 3) as u32;
    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: TextureHandle,
                                  color: Color) {
        debug_assert!(self.frame_started);

        if render::is_rect_fully_offscreen(&self.viewport, &rect) {
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

        self.sprites_batch.add_entry(&vertices, &INDICES, texture, color);
        self.stats.triangles_drawn += 2;
    }

    // NOTE: By default lines and points are batched separately and drawn
    // on top of all sprites, so they will not respect draw order in relation
    // to the textured sprites. These "fast" functions are mainly intended for
    // debugging. It is possible to produce a custom draw order by manually
    // calling one of the flush_* methods to force draws to be submitted early.

    fn draw_line_fast(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        debug_assert!(self.frame_started);

        if render::is_line_fully_offscreen(&self.viewport, &from_pos, &to_pos) {
            return; // Cull if fully offscreen.
        }

        let vertices = [
            LineVertex2D { position: from_pos, color: from_color },
            LineVertex2D { position: to_pos,   color: to_color   },
        ];

        const INDICES: [LineIndex2D; 2] = [0, 1];

        self.lines_batch.add_fast(&vertices, &INDICES);
        self.stats.lines_drawn += 1;
    }

    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32) {
        debug_assert!(self.frame_started);

        if render::is_point_fully_offscreen(&self.viewport, &pt) {
            return; // Cull if fully offscreen.
        }

        let vertices = [PointVertex2D { position: pt, color, size }];
        const INDICES: [PointIndex2D; 1] = [0];

        self.points_batch.add_fast(&vertices, &INDICES);
        self.stats.points_drawn += 1;
    }
}
