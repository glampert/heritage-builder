use super::*;

// ----------------------------------------------
// DebugDraw
// ----------------------------------------------

pub struct DebugDraw {
    render_system: RcMut<RenderSystem>,
}

impl DebugDraw {
    pub fn new(render_system: RcMut<RenderSystem>) -> Self {
        Self { render_system }
    }

    #[inline]
    pub fn texture_cache(&self) -> &texture::TextureCache {
        self.render_system.texture_cache()
    }

    #[inline]
    pub fn texture_cache_mut(&mut self) -> &mut texture::TextureCache {
        self.render_system.texture_cache_mut()
    }

    #[inline]
    pub fn point(&mut self, pt: Vec2, color: Color, size: f32) {
        self.render_system.draw_point(pt, color, size);
    }

    #[inline]
    pub fn line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        self.render_system.draw_line(from_pos, to_pos, from_color, to_color);
    }

    #[inline]
    pub fn line_with_thickness(&mut self, from_pos: Vec2, to_pos: Vec2, color: Color, thickness: f32) {
        self.render_system.draw_line_with_thickness(from_pos, to_pos, color, thickness);
    }

    #[inline]
    pub fn wireframe_rect(&mut self, rect: Rect, color: Color) {
        self.render_system.draw_wireframe_rect(rect, color);
    }

    #[inline]
    pub fn wireframe_rect_with_thickness(&mut self, rect: Rect, color: Color, thickness: f32) {
        self.render_system.draw_wireframe_rect_with_thickness(rect, color, thickness);
    }

    #[inline]
    pub fn colored_rect(&mut self, rect: Rect, color: Color) {
        self.render_system.draw_colored_rect(rect, color);
    }

    #[inline]
    pub fn textured_colored_rect(&mut self,
                                 rect: Rect,
                                 tex_coords: &RectTexCoords,
                                 texture: texture::TextureHandle,
                                 color: Color)
    {
        self.render_system.draw_textured_colored_rect(rect, tex_coords, texture, color);
    }
}
