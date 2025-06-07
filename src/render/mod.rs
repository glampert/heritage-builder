// Internal implementation.
mod opengl;

use opengl::texture::{
    TextureLoaderFlags,
    TextureUnit,
    TextureFilter,
    TextureWrapMode,
    Texture2D
};

use crate::{
    utils::{Vec2, Color, Size, Rect, RectTexCoords}
};

// ----------------------------------------------
// RenderStats
// ----------------------------------------------

#[derive(Clone, Default)]
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

pub trait RenderSystem {

    // ----------------------
    // Render frame markers:
    // ----------------------

    fn begin_frame(&mut self);
    fn end_frame(&mut self) -> RenderStats;

    // ----------------------
    // TextureCache access:
    // ----------------------

    fn texture_cache(&self) -> &TextureCache;
    fn texture_cache_mut(&mut self) -> &mut TextureCache;

    // ----------------------
    // Viewport:
    // ----------------------

    fn viewport(&self) -> Rect;
    fn set_viewport_size(&mut self, new_size: Size);

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_rect(&mut self,
                         rect: Rect,
                         color: Color);

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: TextureHandle,
                                  color: Color);

    fn draw_wireframe_rect_with_thickness(&mut self,
                                          rect: Rect,
                                          color: Color,
                                          thickness: f32);

    // This can handle straight lines efficiently but might produce discontinuities at connecting edges of
    // rectangles and other polygons. To draw connecting lines/polygons use draw_polyline_with_thickness().
    fn draw_line_with_thickness(&mut self,
                                from_pos: Vec2,
                                to_pos: Vec2,
                                color: Color,
                                thickness: f32);

    // Handles connecting lines or closed polygons with seamless mitered joints.
    // Slower but with correct visual results and no seams.
    fn draw_polyline_with_thickness(&mut self,
                                    points: &[Vec2],
                                    color: Color,
                                    thickness: f32,
                                    is_closed: bool);

    // ----------------------
    // Debug drawing:
    // ----------------------

    // "Fast" line and point drawing, mainly used for debugging.
    // These lines and points are batched separately and drawn
    // on top of all sprites so they will not respect draw order
    // in relation to textured sprites and colored polygons.
    fn draw_wireframe_rect_fast(&mut self, rect: Rect, color: Color);
    fn draw_line_fast(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color);
    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32);
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
        RenderSystemBuilder {
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

    pub fn build<'a>(&self) -> impl RenderSystem + use<'a> {
        opengl::system::RenderSystem::new(
            self.viewport_size,
            self.clear_color)
    }
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
        match self {
            TextureHandle::Invalid => false,
            _ => true
        }
    }
}

impl Default for TextureHandle {
    fn default() -> Self { TextureHandle::invalid() }
}

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

pub struct TextureCache {
    textures: Vec<Texture2D>,

    // These are 8x8 pixels.
    dummy_texture_handle: TextureHandle, // TextureHandle::Invalid
    white_texture_handle: TextureHandle, // TextureHandle::White
}

impl TextureCache {
    pub fn new(initial_capacity: usize) -> Self {
        let mut tex_cache = Self {
            textures: Vec::with_capacity(initial_capacity),
            dummy_texture_handle: TextureHandle::invalid(),
            white_texture_handle: TextureHandle::invalid(),
        };

        tex_cache.dummy_texture_handle = tex_cache.create_color_filled_8x8_texture(
            "dummy_texture", [ 255, 0, 255, 255 ]); // magenta

        tex_cache.white_texture_handle = tex_cache.create_color_filled_8x8_texture(
            "white_texture", [ 255, 255, 255, 255 ]); // white

        tex_cache
    }

    #[inline]
    pub fn handle_to_texture(&self, handle: TextureHandle) -> &Texture2D {
        match handle {
            TextureHandle::Invalid => self.dummy_texture(),
            TextureHandle::White   => self.white_texture(),
            TextureHandle::Index(handle_index) => {
                let index = handle_index as usize;
                if index < self.textures.len() {
                    &self.textures[index]
                } else {
                    self.dummy_texture()
                }
            }
        }
    }

    #[inline]
    pub fn dummy_texture(&self) -> &Texture2D {
        match self.dummy_texture_handle {
            TextureHandle::Index(index) => &self.textures[index as usize],
            _ => panic!("Unexpected value for dummy_texture_handle!")
        }
    }

    #[inline]
    pub fn white_texture(&self) -> &Texture2D {
        match self.white_texture_handle {
            TextureHandle::Index(index) => &self.textures[index as usize],
            _ => panic!("Unexpected value for white_texture_handle!")
        }
    }

    #[inline]
    pub fn to_native_handle(&self, handle: TextureHandle) -> usize {
        self.handle_to_texture(handle).native_handle()
    }

    #[inline]
    pub fn load_texture(&mut self, file_path: &str) -> TextureHandle {
        Self::load_texture_with_settings(self,
                                         file_path,
                                         TextureLoaderFlags::empty(),
                                         TextureFilter::Nearest,
                                         TextureWrapMode::ClampToEdge,
                                         TextureUnit(0),
                                         false)
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn load_texture_with_settings(&mut self,
                                  file_path: &str,
                                  flags: TextureLoaderFlags,
                                  filter: TextureFilter,
                                  wrap_mode: TextureWrapMode,
                                  tex_unit: TextureUnit,
                                  gen_mipmaps: bool) -> TextureHandle {

        let texture = match Texture2D::from_file(file_path,
                                                            flags,
                                                            filter,
                                                            wrap_mode,
                                                            tex_unit,
                                                            gen_mipmaps) {
            Ok(texture) => texture,
            Err(err) => {
                eprintln!("TextureCache Load Error: {}", err);
                return self.dummy_texture_handle;
            },
        };

        self.add_texture(texture)
    }

    fn create_color_filled_8x8_texture(&mut self,
                                       debug_name: &str,
                                       color: [u8; 4]) -> TextureHandle {
        use std::ffi::c_void;

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct RGBA8 {
            r: u8,
            g: u8,
            b: u8,
            a: u8,
        }
        debug_assert!(std::mem::size_of::<RGBA8>() == 4); // Ensure no padding.

        const SIZE: Size = Size::new(8, 8);
        const PIXEL_COUNT: usize = (SIZE.width * SIZE.height) as usize;
        let pixels = [RGBA8{ r: color[0], g: color[1], b: color[2], a: color[3] }; PIXEL_COUNT];

        let texture = Texture2D::with_data_raw(pixels.as_ptr() as *const c_void,
                                                          SIZE,
                                                          TextureFilter::Nearest,
                                                          TextureWrapMode::ClampToEdge,
                                                          TextureUnit(0),
                                                          false,
                                                          debug_name);

        self.add_texture(texture)
    }

    #[inline]
    fn add_texture(&mut self, texture: Texture2D) -> TextureHandle {
        let index = self.textures.len();
        self.textures.push(texture);
        TextureHandle::Index(index as u32)
    }
}
