use crate::utils::{Color, Vec2, Size2D, Point2D, Rect2D, RectTexCoords};

use super::shader::*;
use super::vertex::*;
use super::context::*;
use super::batch::{DrawBatch, DrawBatchEntry};
use super::texture::{TextureCache, TextureHandle};

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
}

impl RenderSystem {
    pub fn new(window_size: Size2D) -> Self {
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
        };

        render_sys.render_context
            .set_clear_color(Color::gray())
            .set_alpha_blend(AlphaBlend::Enabled)
            // Pure 2D rendering, no depth test or back-face culling.
            .set_backface_culling(BackFaceCulling::Disabled)
            .set_depth_test(DepthTest::Disabled);

        render_sys.set_window_size(window_size);

        render_sys
    }

    pub fn begin_frame(&mut self) {
        debug_assert!(self.frame_started == false);
        self.render_context.begin_frame();
        self.frame_started = true;
    }

    pub fn end_frame(&mut self, tex_cache: &TextureCache) {
        debug_assert!(self.frame_started == true);

        self.flush_sprites(tex_cache);
        self.flush_lines();
        self.flush_points();

        self.render_context.end_frame();
        self.frame_started = false;
    }

    pub fn flush_sprites(&mut self, tex_cache: &TextureCache) {
        debug_assert!(self.frame_started == true);

        let set_sprite_shader_vars = 
            |render_context: &mut RenderContext, entry: &DrawBatchEntry| {

            let texture2d = tex_cache.handle_to_texture(entry.texture);
            render_context.set_texture_2d(texture2d);

            self.sprites_shader.set_sprite_tint(entry.color);
            self.sprites_shader.set_sprite_texture(texture2d);
        };

        self.sprites_batch.sync();
        self.sprites_batch.draw_entries(&mut self.render_context, &self.sprites_shader.program, set_sprite_shader_vars);
        self.sprites_batch.clear();
    }

    pub fn flush_lines(&mut self) {
        debug_assert!(self.frame_started == true);

        self.lines_batch.sync();
        self.lines_batch.draw_fast(&mut self.render_context, &self.lines_shader.program);
        self.lines_batch.clear();
    }

    pub fn flush_points(&mut self) {
        debug_assert!(self.frame_started == true);

        self.points_batch.sync();
        self.points_batch.draw_fast(&mut self.render_context, &self.points_shader.program);
        self.points_batch.clear();
    }

    pub fn set_window_size(&mut self, new_size: Size2D) {
        self.render_context.set_viewport(Rect2D::new(Point2D::zero(), new_size));

        let viewport_size = new_size.to_vec2();
        self.sprites_shader.set_viewport_size(viewport_size);
        self.lines_shader.set_viewport_size(viewport_size);
        self.points_shader.set_viewport_size(viewport_size);
    }

    pub fn draw_colored_rect(&mut self,
                             rect: Rect2D,
                             color: Color) {
        // Just call this with the default white texture.
        self.draw_textured_colored_rect(
            rect,
            &RectTexCoords::default(),
            TextureHandle::white(),
            color);
    }

    pub fn draw_textured_colored_rect(&mut self,
                                      rect: Rect2D,
                                      tex_coords: &RectTexCoords,
                                      texture: TextureHandle,
                                      color: Color) {
        debug_assert!(self.frame_started);

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
    }

    pub fn draw_wireframe_rect_with_thickness(&mut self,
                                              rect: Rect2D,
                                              color: Color,
                                              thickness: f32) {
        debug_assert!(self.frame_started);

        let points: [Point2D; 4] = [
            Point2D::new(rect.x(), rect.y()),
            Point2D::new(rect.x() + rect.width(), rect.y()),
            Point2D::new(rect.x() + rect.width(), rect.y() + rect.height()),
            Point2D::new(rect.x(), rect.y() + rect.height()),
        ];

        self.draw_polyline_with_thickness(&points, color, thickness, true);
    }

    // This can handle straight lines efficiently but will produce discontinuities at connecting edges of
    // rectangles and other polygons. To draw connecting lines/polygons use draw_polyline_with_thickness().
    pub fn draw_line_with_thickness(&mut self,
                                    from_pos: Point2D,
                                    to_pos: Point2D,
                                    color: Color,
                                    thickness: f32) {
        debug_assert!(self.frame_started);

        // Convert to float vectors
        let v0 = from_pos.to_vec2();
        let v1 = to_pos.to_vec2();

        let d = v1 - v0;
        let length = d.length();

        // Normalize and rotate 90° to get perpendicular vector
        let nx = -d.y / length;
        let ny =  d.x / length;
    
        let offset_x = nx * (thickness / 2.0);
        let offset_y = ny * (thickness / 2.0);

        // Four corner points of the quad (screen space)
        let p0 = Vec2::new((v0.x + offset_x).round(), (v0.y + offset_y).round());
        let p1 = Vec2::new((v1.x + offset_x).round(), (v1.y + offset_y).round());
        let p2 = Vec2::new((v1.x - offset_x).round(), (v1.y - offset_y).round());
        let p3 = Vec2::new((v0.x - offset_x).round(), (v0.y - offset_y).round());

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
    }

    // Handles connecting lines or closed polygons with seamless mitered joints.
    // Slower but with correct visual results.
    pub fn draw_polyline_with_thickness<const N: usize>(&mut self,
                                                        points: &[Point2D; N],
                                                        color: Color,
                                                        thickness: f32,
                                                        is_closed: bool) {
        const MAX_POINTS:  usize = 32;
        const MAX_VERTS:   usize = 2 * MAX_POINTS;
        const MAX_INDICES: usize = 6 * MAX_POINTS;

        debug_assert!(self.frame_started);
        debug_assert!(N >= 2 && N <= MAX_POINTS);

        let mut vertices: [SpriteVertex2D; MAX_VERTS] = [SpriteVertex2D::default(); MAX_VERTS];
        let mut indices: [SpriteIndex2D; MAX_INDICES] = [0; MAX_INDICES];

        let mut v_count = 0;
        let mut i_count = 0;
    
        for i in 0..N {
            let prev = if i == 0 {
                if is_closed { points[N - 1].to_vec2() } else { points[0].to_vec2() }
            } else {
                points[i - 1].to_vec2()
            };
    
            let curr = points[i].to_vec2();
    
            let next = if i == N - 1 {
                if is_closed { points[0].to_vec2() } else { points[N - 1].to_vec2() }
            } else {
                points[i + 1].to_vec2()
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
            if i < N - 1 || is_closed {
                let i0 = v_count as SpriteIndex2D;
                let i1 = i0 + 1;
                let i2 = ((i + 1) % N * 2) as SpriteIndex2D;
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
    }

    // NOTE: By default lines and points are batched separately and drawn
    // on top of all sprites, so they will not respect draw order in relation
    // to the textured sprites. These "fast" functions are mainly intended for
    // debugging. It is possible to produce a custom draw order by manually
    // calling one of the flush_* methods to force draws to be submitted early.

    pub fn draw_wireframe_rect_fast(&mut self, rect: Rect2D, color: Color) {
        debug_assert!(self.frame_started);

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
    }

    pub fn draw_line_fast(&mut self, from_pos: Point2D, to_pos: Point2D, from_color: Color, to_color: Color) {
        debug_assert!(self.frame_started);

        let vertices: [LineVertex2D; 2] = [
            LineVertex2D { position: from_pos.to_vec2(), color: from_color },
            LineVertex2D { position: to_pos.to_vec2(),   color: to_color   },
        ];

        const INDICES: [LineIndex2D; 2] = [ 0, 1 ];

        self.lines_batch.add_fast(&vertices, &INDICES);
    }

    pub fn draw_point_fast(&mut self, pos: Point2D, color: Color, size: f32) {
        debug_assert!(self.frame_started);

        let vertices: [PointVertex2D; 1] = [
            PointVertex2D { position: pos.to_vec2(), color: color, size: size }
        ];

        const INDICES: [PointIndex2D; 1] = [ 0 ];

        self.points_batch.add_fast(&vertices, &INDICES);
    }
}
