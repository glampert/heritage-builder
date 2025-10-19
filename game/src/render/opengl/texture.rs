use std::{any::Any, ffi::c_void};

use bitflags::bitflags;
use image::GenericImageView;

use super::{
    gl_error_to_string,
    panic_if_gl_error,
    shader::{ShaderVarTrait, ShaderVariable},
};
use crate::{
    log,
    render::{self, NativeTextureHandle, TextureHandle},
    utils::Size,
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const NULL_TEXTURE_HANDLE: gl::types::GLuint = 0;
pub const MAX_TEXTURE_UNITS: usize = 4;

bitflags! {
    pub struct TextureLoaderFlags: u32 {
        const FlipV = 1 << 0;
        const FlipH = 1 << 1;
    }
}

// ----------------------------------------------
// TextureUnit
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct TextureUnit(pub u32);

impl ShaderVarTrait for &Texture2D {
    fn set_uniform(variable: &ShaderVariable, texture: &Texture2D) {
        unsafe {
            gl::ProgramUniform1i(variable.program_handle,
                                 variable.location,
                                 texture.tex_unit.0 as gl::types::GLint);
        }
    }
}

// ----------------------------------------------
// Texture Sampling
// ----------------------------------------------

// Equivalent to the GL_TEXTURE_FILTER enums.
#[repr(u32)]
#[derive(Copy, Clone)]
pub enum TextureFilter {
    Nearest = gl::NEAREST,
    Linear = gl::LINEAR,
    NearestMipmapNearest = gl::NEAREST_MIPMAP_NEAREST,
    LinearMipmapLinear = gl::LINEAR_MIPMAP_LINEAR,
}

// Equivalent to the GL_TEXTURE_WRAP enums.
#[repr(u32)]
#[derive(Copy, Clone)]
pub enum TextureWrapMode {
    Repeat = gl::REPEAT,
    ClampToEdge = gl::CLAMP_TO_EDGE,
    ClampToBorder = gl::CLAMP_TO_BORDER,
}

// ----------------------------------------------
// Texture2D
// ----------------------------------------------

pub struct Texture2D {
    handle: gl::types::GLuint,
    tex_unit: TextureUnit,
    size: Size,
    filter: TextureFilter,
    wrap_mode: TextureWrapMode,
    has_mipmaps: bool,
    name: String,
}

impl Texture2D {
    pub fn from_file(file_path: &str,
                     flags: TextureLoaderFlags,
                     filter: TextureFilter,
                     wrap_mode: TextureWrapMode,
                     tex_unit: TextureUnit,
                     gen_mipmaps: bool)
                     -> Result<Self, String> {
        debug_assert!(!file_path.is_empty());

        let mut image = match image::open(file_path) {
            Ok(image) => image,
            Err(err) => {
                return Err(format!("Failed to load image file '{file_path}': {err:?}"));
            }
        };

        if flags.contains(TextureLoaderFlags::FlipV) {
            image.apply_orientation(image::metadata::Orientation::FlipVertical);
        }

        if flags.contains(TextureLoaderFlags::FlipH) {
            image.apply_orientation(image::metadata::Orientation::FlipHorizontal);
        }

        let (image_w, image_h) = image.dimensions();
        let image_buffer = image.as_rgba8().expect("Expected an RGBA8 image!");
        let image_pixels = image_buffer.as_raw();

        Ok(Self::with_data_raw(image_pixels.as_ptr() as *const c_void,
                               Size::new(image_w as i32, image_h as i32),
                               filter,
                               wrap_mode,
                               tex_unit,
                               gen_mipmaps,
                               file_path))
    }

    pub fn with_data_raw(data: *const c_void,
                         size: Size,
                         filter: TextureFilter,
                         wrap_mode: TextureWrapMode,
                         tex_unit: TextureUnit,
                         gen_mipmaps: bool,
                         debug_name: &str)
                         -> Self {
        debug_assert!((tex_unit.0 as usize) < MAX_TEXTURE_UNITS);
        debug_assert!(size.is_valid());

        let handle = unsafe {
            let mut handle = NULL_TEXTURE_HANDLE;
            gl::GenTextures(1, &mut handle);
            if handle == NULL_TEXTURE_HANDLE {
                panic!("Failed to create texture handle!");
            }

            gl::ActiveTexture(gl::TEXTURE0 + tex_unit.0);
            gl::BindTexture(gl::TEXTURE_2D, handle);

            gl::TexImage2D(gl::TEXTURE_2D,
                           0,
                           gl::RGBA as gl::types::GLint, // Only RGBA images supported for now.
                           size.width as gl::types::GLsizei,
                           size.height as gl::types::GLsizei,
                           0,
                           gl::RGBA,
                           gl::UNSIGNED_BYTE,
                           data);

            if gen_mipmaps {
                gl::GenerateMipmap(gl::TEXTURE_2D);

                let error_code = gl::GetError();
                if error_code != gl::NO_ERROR {
                    panic!("Failed to generate texture mipmaps. OpenGL Error: {} (0x{:X})",
                           gl_error_to_string(error_code),
                           error_code);
                }
            }

            let (gl_min_filter, gl_mag_filter) = match filter {
                TextureFilter::Nearest => (gl::NEAREST, gl::NEAREST),
                TextureFilter::Linear => (gl::LINEAR, gl::LINEAR),
                TextureFilter::NearestMipmapNearest => (gl::NEAREST_MIPMAP_NEAREST, gl::NEAREST),
                TextureFilter::LinearMipmapLinear => (gl::LINEAR_MIPMAP_LINEAR, gl::LINEAR),
            };

            gl::TexParameteri(gl::TEXTURE_2D,
                              gl::TEXTURE_MIN_FILTER,
                              gl_min_filter as gl::types::GLint);
            gl::TexParameteri(gl::TEXTURE_2D,
                              gl::TEXTURE_MAG_FILTER,
                              gl_mag_filter as gl::types::GLint);

            let gl_wrap_mode = wrap_mode as gl::types::GLint;
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl_wrap_mode);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl_wrap_mode);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_R, gl_wrap_mode);

            // Unbind.
            gl::BindTexture(gl::TEXTURE_2D, NULL_TEXTURE_HANDLE);

            handle
        };

        Self { handle,
               tex_unit,
               size,
               filter,
               wrap_mode,
               has_mipmaps: gen_mipmaps,
               name: debug_name.to_string() }
    }

    pub fn update(&self,
                  offset_x: u32,
                  offset_y: u32,
                  size: Size,
                  mip_level: u32,
                  pixels: &[u8]) {
        debug_assert!(self.is_valid());
        debug_assert!(offset_x as i32 + size.width  <= self.size.width);
        debug_assert!(offset_y as i32 + size.height <= self.size.height);

        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + self.tex_unit.0);
            gl::BindTexture(gl::TEXTURE_2D, self.handle);

            gl::TexSubImage2D(gl::TEXTURE_2D,
                              mip_level as gl::types::GLint,
                              offset_x as gl::types::GLint,
                              offset_y as gl::types::GLint,
                              size.width,
                              size.height,
                              gl::RGBA,
                              gl::UNSIGNED_BYTE,
                              pixels.as_ptr() as *const c_void);

            // Unbind.
            gl::BindTexture(gl::TEXTURE_2D, NULL_TEXTURE_HANDLE);
        }
    }

    pub fn is_valid(&self) -> bool {
        self.handle != NULL_TEXTURE_HANDLE && self.size.is_valid()
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.handle
    }

    pub fn tex_unit(&self) -> TextureUnit {
        self.tex_unit
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn filter(&self) -> TextureFilter {
        self.filter
    }

    pub fn wrap_mode(&self) -> TextureWrapMode {
        self.wrap_mode
    }

    pub fn has_mipmaps(&self) -> bool {
        self.has_mipmaps
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    #[inline]
    pub fn native_handle(&self) -> NativeTextureHandle {
        NativeTextureHandle { bits: self.handle as usize }
    }
}

impl Drop for Texture2D {
    fn drop(&mut self) {
        if self.handle != NULL_TEXTURE_HANDLE {
            unsafe {
                gl::DeleteTextures(1, &self.handle);
            }
            self.handle = NULL_TEXTURE_HANDLE;
        }
    }
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
        let mut tex_cache = Self { textures: Vec::with_capacity(initial_capacity),
                                   dummy_texture_handle: TextureHandle::invalid(),
                                   white_texture_handle: TextureHandle::invalid() };

        tex_cache.dummy_texture_handle =
            tex_cache.create_color_filled_8x8_texture("dummy_texture", [255, 0, 255, 255]); // magenta

        tex_cache.white_texture_handle =
            tex_cache.create_color_filled_8x8_texture("white_texture", [255, 255, 255, 255]); // white

        tex_cache
    }

    #[inline]
    pub fn handle_to_texture(&self, handle: TextureHandle) -> &Texture2D {
        match handle {
            TextureHandle::Invalid => self.dummy_texture(),
            TextureHandle::White => self.white_texture(),
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
            _ => panic!("Unexpected value for dummy_texture_handle!"),
        }
    }

    #[inline]
    pub fn white_texture(&self) -> &Texture2D {
        match self.white_texture_handle {
            TextureHandle::Index(index) => &self.textures[index as usize],
            _ => panic!("Unexpected value for white_texture_handle!"),
        }
    }

    #[inline]
    fn add_texture(&mut self, texture: Texture2D) -> TextureHandle {
        let index = self.textures.len();
        self.textures.push(texture);
        TextureHandle::Index(index as u32)
    }

    fn load_texture_with_settings(&mut self,
                                  file_path: &str,
                                  flags: TextureLoaderFlags,
                                  filter: TextureFilter,
                                  wrap_mode: TextureWrapMode,
                                  tex_unit: TextureUnit,
                                  gen_mipmaps: bool)
                                  -> TextureHandle {
        let texture = match Texture2D::from_file(file_path,
                                                 flags,
                                                 filter,
                                                 wrap_mode,
                                                 tex_unit,
                                                 gen_mipmaps)
        {
            Ok(texture) => texture,
            Err(err) => {
                log::error!(log::channel!("render"), "TextureCache Load Error: {err}");
                return self.dummy_texture_handle;
            }
        };

        self.add_texture(texture)
    }

    fn create_color_filled_8x8_texture(&mut self,
                                       debug_name: &str,
                                       color: [u8; 4])
                                       -> TextureHandle {
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
        let pixels = [RGBA8 { r: color[0], g: color[1], b: color[2], a: color[3] }; PIXEL_COUNT];

        let texture = Texture2D::with_data_raw(pixels.as_ptr() as *const c_void,
                                               SIZE,
                                               TextureFilter::Nearest,
                                               TextureWrapMode::ClampToEdge,
                                               TextureUnit(0),
                                               false,
                                               debug_name);

        self.add_texture(texture)
    }
}

impl render::TextureCache for TextureCache {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn load_texture(&mut self, file_path: &str) -> TextureHandle {
        self.load_texture_with_settings(file_path,
                                        TextureLoaderFlags::empty(),
                                        TextureFilter::Nearest,
                                        TextureWrapMode::ClampToEdge,
                                        TextureUnit(0),
                                        false)
    }

    #[inline]
    fn to_native_handle(&self, handle: TextureHandle) -> NativeTextureHandle {
        self.handle_to_texture(handle).native_handle()
    }

    fn new_uninitialized_texture(&mut self,
                                 debug_name: &str,
                                 size: Size) -> TextureHandle {
        debug_assert!(size.is_valid());
        let texture = Texture2D::with_data_raw(core::ptr::null(),
                                               size,
                                               TextureFilter::Nearest,
                                               TextureWrapMode::ClampToEdge,
                                               TextureUnit(0),
                                               false,
                                               debug_name);
        self.add_texture(texture)
    }

    fn update_texture(&mut self,
                      handle: TextureHandle,
                      offset_x: u32,
                      offset_y: u32,
                      size: Size,
                      mip_level: u32,
                      pixels: &[u8]) {
        debug_assert!(handle.is_valid());
        debug_assert!(size.is_valid());
        debug_assert!(pixels.len() >= (size.width * size.height * 4) as usize); // RGBA images only.

        let texture = self.handle_to_texture(handle);
        texture.update(offset_x, offset_y, size, mip_level, pixels);

        panic_if_gl_error();
    }
}
