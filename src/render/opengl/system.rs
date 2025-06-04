use crate::{
    render::{self, RenderStats, TextureCache, TextureHandle},
    utils::{Color, Rect, RectTexCoords, Size, Vec2}
};

use super::{
    shader::*,
    vertex::*,
    context::*,
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
        debug_assert!(self.frame_started == true);

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
        debug_assert!(self.frame_started == true);

        self.lines_batch.sync();
        self.lines_batch.draw_fast(&mut self.render_context, &self.lines_shader.program);
        self.lines_batch.clear();
    }

    fn flush_points(&mut self) {
        debug_assert!(self.frame_started == true);

        self.points_batch.sync();
        self.points_batch.draw_fast(&mut self.render_context, &self.points_shader.program);
        self.points_batch.clear();
    }

    #[inline]
    fn is_rect_fully_offscreen(&self, rect: &Rect) -> bool {
        if rect.max.x < self.viewport.min.x || rect.max.y < self.viewport.min.y {
            return true;
        }
        if rect.min.x > self.viewport.max.x || rect.min.y > self.viewport.max.y {
            return true;
        }
        false
    }

    #[inline]
    fn is_line_fully_offscreen(&self, from: &Vec2, to: &Vec2) -> bool {
        if (from.x < self.viewport.min.x && to.x < self.viewport.min.x) ||
           (from.y < self.viewport.min.y && to.y < self.viewport.min.y) {
            return true;
        }
        if (from.x > self.viewport.max.x && to.x > self.viewport.max.x) ||
           (from.y > self.viewport.max.y && to.y > self.viewport.max.y) {
            return true;
        }
        false
    }

    #[inline]
    fn is_point_fully_offscreen(&self, pt: &Vec2) -> bool {
        if pt.x < self.viewport.min.x || pt.y < self.viewport.min.y {
            return true;
        }
        if pt.x > self.viewport.max.x || pt.y > self.viewport.max.y {
            return true;
        }
        false
    }
}

impl render::RenderSystem for RenderSystem {
    fn begin_frame(&mut self) {
        debug_assert!(self.frame_started == false);

        self.render_context.begin_frame();
        self.frame_started = true;

        self.stats.triangles_drawn = 0;
        self.stats.lines_drawn     = 0;
        self.stats.points_drawn    = 0;
        self.stats.texture_changes = 0;
        self.stats.draw_calls      = 0;
    }

    fn end_frame(&mut self) -> RenderStats {
        debug_assert!(self.frame_started == true);

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
    fn texture_cache(&self) -> &TextureCache {
        &self.tex_cache
    }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut TextureCache {
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

    #[inline]
    fn draw_colored_rect(&mut self, rect: Rect, color: Color) {
        // Just call this with the default white texture.
        self.draw_textured_colored_rect(
            rect,
            RectTexCoords::default(),
            TextureHandle::white(),
            color);
    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: TextureHandle,
                                  color: Color) {

        debug_assert!(self.frame_started);

        if self.is_rect_fully_offscreen(&rect) {
            return; // Cull if fully offscreen.
        }

        let vertices: [SpriteVertex2D; 4] = [
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
        self.stats.triangles_drawn += (INDICES.len() / 3) as u32;
    }

    fn draw_wireframe_rect_with_thickness(&mut self,
                                          rect: Rect,
                                          color: Color,
                                          thickness: f32) {

        debug_assert!(self.frame_started);

        if self.is_rect_fully_offscreen(&rect) {
            return; // Cull if fully offscreen.
        }

        let points: [Vec2; 4] = [
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

        debug_assert!(self.frame_started);

        if self.is_line_fully_offscreen(&from_pos, &to_pos) {
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
        let p0 = Vec2::new((from_pos.x + offset_x).round(), (from_pos.y + offset_y).round());
        let p1 = Vec2::new((to_pos.x   + offset_x).round(), (to_pos.y   + offset_y).round());
        let p2 = Vec2::new((to_pos.x   - offset_x).round(), (to_pos.y   - offset_y).round());
        let p3 = Vec2::new((from_pos.x - offset_x).round(), (from_pos.y - offset_y).round());

        // Draw two triangles to form a quad
        let vertices: [SpriteVertex2D; 4] = [
            SpriteVertex2D { position: p0, tex_coords: Vec2::default() },
            SpriteVertex2D { position: p1, tex_coords: Vec2::default() },
            SpriteVertex2D { position: p2, tex_coords: Vec2::default() },
            SpriteVertex2D { position: p3, tex_coords: Vec2::default() },
        ];

        const INDICES: [SpriteIndex2D; 6] = [
            0, 1, 2, // first triangle
            2, 3, 0, // second triangle
        ];

        self.sprites_batch.add_entry(&vertices, &INDICES, TextureHandle::white(), color);
        self.stats.triangles_drawn += (INDICES.len() / 3) as u32;
    }

    // Handles connecting lines or closed polygons with seamless mitered joints.
    // Slower but with correct visual results and no seams.
    fn draw_polyline_with_thickness(&mut self,
                                    points: &[Vec2],
                                    color: Color,
                                    thickness: f32,
                                    is_closed: bool) {

        const MAX_POINTS:  usize = 32;
        const MAX_VERTS:   usize = 2 * MAX_POINTS;
        const MAX_INDICES: usize = 6 * MAX_POINTS;

        let num_points = points.len();

        debug_assert!(self.frame_started);
        debug_assert!(num_points >= 2 && num_points <= MAX_POINTS);

        let mut vertices: [SpriteVertex2D; MAX_VERTS] = [SpriteVertex2D::default(); MAX_VERTS];
        let mut indices: [SpriteIndex2D; MAX_INDICES] = [0; MAX_INDICES];

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
            vertices[v_count] = SpriteVertex2D {
                position: curr + offset,
                tex_coords: Vec2::default(),
            };
            vertices[v_count + 1] = SpriteVertex2D {
                position: curr - offset,
                tex_coords: Vec2::default(),
            };
    
            // Build indices
            if i < num_points - 1 || is_closed {
                let i0 = v_count as SpriteIndex2D;
                let i1 = i0 + 1;
                let i2 = ((i + 1) % num_points * 2) as SpriteIndex2D;
                let i3 = i2 + 1;
    
                indices[i_count..i_count + 6].copy_from_slice(&[i0, i2, i1, i1, i2, i3]);
                i_count += 6;
            }

            v_count += 2;
        }

        self.sprites_batch.add_entry(
            &vertices[..v_count],
            &indices[..i_count],
            TextureHandle::white(),
            color);

        self.stats.triangles_drawn += (i_count / 3) as u32;
    }

    // NOTE: By default lines and points are batched separately and drawn
    // on top of all sprites, so they will not respect draw order in relation
    // to the textured sprites. These "fast" functions are mainly intended for
    // debugging. It is possible to produce a custom draw order by manually
    // calling one of the flush_* methods to force draws to be submitted early.

    fn draw_wireframe_rect_fast(&mut self, rect: Rect, color: Color) {
        debug_assert!(self.frame_started);

        if self.is_rect_fully_offscreen(&rect) {
            return; // Cull if fully offscreen.
        }

        let vertices: [LineVertex2D; 4] = [
            LineVertex2D { position: rect.bottom_left(),  color: color },
            LineVertex2D { position: rect.bottom_right(), color: color },
            LineVertex2D { position: rect.top_right(),    color: color },
            LineVertex2D { position: rect.top_left(),     color: color },
        ];

        const INDICES: [LineIndex2D; 8] = [ 
            0, 1, // line 0
            1, 2, // line 1
            2, 3, // line 2
            3, 0, // line 3
        ];

        self.lines_batch.add_fast(&vertices, &INDICES);
        self.stats.lines_drawn += (INDICES.len() / 2) as u32;
    }

    fn draw_line_fast(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        debug_assert!(self.frame_started);

        if self.is_line_fully_offscreen(&from_pos, &to_pos) {
            return; // Cull if fully offscreen.
        }

        let vertices: [LineVertex2D; 2] = [
            LineVertex2D { position: from_pos, color: from_color },
            LineVertex2D { position: to_pos,   color: to_color   },
        ];

        const INDICES: [LineIndex2D; 2] = [ 0, 1 ];

        self.lines_batch.add_fast(&vertices, &INDICES);
        self.stats.lines_drawn += (INDICES.len() / 2) as u32;
    }

    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32) {
        debug_assert!(self.frame_started);

        if self.is_point_fully_offscreen(&pt) {
            return; // Cull if fully offscreen.
        }

        let vertices: [PointVertex2D; 1] = [
            PointVertex2D { position: pt, color: color, size: size }
        ];

        const INDICES: [PointIndex2D; 1] = [ 0 ];

        self.points_batch.add_fast(&vertices, &INDICES);
        self.stats.points_drawn += 1;
    }
}
