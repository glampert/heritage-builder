use arrayvec::ArrayVec;

use crate::{
    render::{self, RenderStats, TextureHandle},
    utils::{Color, Rect, RectTexCoords, Size, Vec2}
};

use super::{
    shader::*,
    vertex::*,
    context::*,
    texture::TextureCache,
    batch::{DrawBatch, DrawBatchEntry}
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
    stats: RenderStats,
    viewport: Rect,
    tex_cache: TextureCache,
}

impl RenderSystem {
    pub fn new(viewport_size: Size, clear_color: Color) -> Self {
        debug_assert!(viewport_size.is_valid());

        let mut render_sys = Self {
            frame_started: false,
            render_context: RenderContext::new(),
            sprites_batch: DrawBatch::new(
                512,
                512,
                512,
                PrimitiveTopology::Triangles,
            ),
            sprites_shader: sprites::Shader::load(),
            lines_batch: DrawBatch::new(
                8,
                8,
                0,
                PrimitiveTopology::Lines,
            ),
            lines_shader: lines::Shader::load(),
            points_batch: DrawBatch::new(
                8,
                8,
                0,
                PrimitiveTopology::Points,
            ),
            points_shader: points::Shader::load(),
            stats: RenderStats::default(),
            viewport: Rect::from_pos_and_size(Vec2::zero(), viewport_size),
            tex_cache: TextureCache::new(128),
        };

        render_sys.render_context
            .set_clear_color(clear_color)
            .set_alpha_blend(AlphaBlend::Enabled)
            // Pure 2D rendering, no depth test or back-face culling.
            .set_backface_culling(BackFaceCulling::Disabled)
            .set_depth_test(DepthTest::Disabled);

        render_sys.update_viewport(viewport_size);

        render_sys
    }

    fn update_viewport(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());
        self.viewport = Rect::from_pos_and_size(Vec2::zero(), new_size);

        self.render_context.set_viewport(self.viewport);
        self.sprites_shader.set_viewport_size(self.viewport.size_as_vec2());
        self.lines_shader.set_viewport_size(self.viewport.size_as_vec2());
        self.points_shader.set_viewport_size(self.viewport.size_as_vec2());
    }

    fn flush_sprites(&mut self) {
        debug_assert!(self.frame_started);

        let set_shader_vars_fn = 
            |render_context: &mut RenderContext, entry: &DrawBatchEntry| {

            let texture2d = self.tex_cache.handle_to_texture(entry.texture);
            render_context.set_texture_2d(texture2d);

            self.sprites_shader.set_sprite_tint(entry.color);
            self.sprites_shader.set_sprite_texture(texture2d);
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
}

impl render::RenderSystem for RenderSystem {
    fn begin_frame(&mut self) {
        debug_assert!(!self.frame_started);

        self.render_context.begin_frame();
        self.frame_started = true;

        self.stats.triangles_drawn = 0;
        self.stats.lines_drawn     = 0;
        self.stats.points_drawn    = 0;
        self.stats.texture_changes = 0;
        self.stats.draw_calls      = 0;
    }

    fn end_frame(&mut self) -> RenderStats {
        debug_assert!(self.frame_started);

        self.flush_sprites();
        self.flush_lines();
        self.flush_points();

        self.render_context.end_frame();
        self.frame_started = false;

        self.stats.texture_changes      = self.render_context.texture_changes();
        self.stats.draw_calls           = self.render_context.draw_calls();
        self.stats.peak_triangles_drawn = self.stats.triangles_drawn.max(self.stats.peak_triangles_drawn);
        self.stats.peak_lines_drawn     = self.stats.lines_drawn.max(self.stats.peak_lines_drawn);
        self.stats.peak_points_drawn    = self.stats.points_drawn.max(self.stats.peak_points_drawn);
        self.stats.peak_texture_changes = self.stats.texture_changes.max(self.stats.peak_texture_changes);
        self.stats.peak_draw_calls      = self.stats.draw_calls.max(self.stats.peak_draw_calls);

        self.stats.clone()
    }

    #[inline]
    fn texture_cache(&self) -> &impl render::TextureCache {
        &self.tex_cache
    }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut impl render::TextureCache {
        &mut self.tex_cache
    }

    #[inline]
    fn viewport(&self) -> Rect {
        self.viewport
    }

    #[inline]
    fn set_viewport_size(&mut self, new_size: Size) {
        self.update_viewport(new_size);
    }

    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[u16],
                                      color: Color) {

        debug_assert!(self.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!((indices.len() % 3) == 0); // We expect triangles.

        // Expand to sprite vertices with defaulted (unused) texture coordinates.
        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();
        for vert in vertices {
            sprite_verts.push(SpriteVertex2D {
                position: *vert,
                tex_coords: Vec2::default(),
            });
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

        const INDICES: [LineIndex2D; 2] = [ 0, 1 ];

        self.lines_batch.add_fast(&vertices, &INDICES);
        self.stats.lines_drawn += 1;
    }

    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32) {
        debug_assert!(self.frame_started);

        if render::is_point_fully_offscreen(&self.viewport, &pt) {
            return; // Cull if fully offscreen.
        }

        let vertices = [ PointVertex2D { position: pt, color, size } ];
        const INDICES: [PointIndex2D; 1] = [ 0 ];

        self.points_batch.add_fast(&vertices, &INDICES);
        self.stats.points_drawn += 1;
    }
}
