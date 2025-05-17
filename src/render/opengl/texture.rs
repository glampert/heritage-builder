use std::ffi::c_void;
use bitflags::bitflags;
use image::GenericImageView;

use crate::utils::Size2D;
use super::gl_error_to_string;
use super::shader::{ShaderVarTrait, ShaderVariable};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const NULL_TEXTURE_HANDLE: gl::types::GLuint = 0;

bitflags! {
    pub struct ImageLoaderFlags: u32 {
        const None  = 0;
        const FlipV = 1 << 1;
        const FlipH = 1 << 2;
    }
}

// ----------------------------------------------
// TextureUnit
// ----------------------------------------------

pub const MAX_TEXTURE_UNITS: usize = 4;

#[derive(Copy, Clone)]
pub struct TextureUnit(pub u32);

impl ShaderVarTrait for &Texture2D {
    fn set_uniform(variable: &ShaderVariable, texture: &Texture2D) {
        unsafe {
            gl::ProgramUniform1i(
                variable.program_handle,
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
    Linear  = gl::LINEAR,
    NearestMipmapNearest = gl::NEAREST_MIPMAP_NEAREST,
    LinearMipmapLinear   = gl::LINEAR_MIPMAP_LINEAR,
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
    size: Size2D,
    filter: TextureFilter,
    wrap_mode: TextureWrapMode,
    has_mipmaps: bool,
    name: String,
}

impl Texture2D {
    pub fn from_file(file_path: &str,
                     flags: ImageLoaderFlags,
                     filter: TextureFilter,
                     wrap_mode: TextureWrapMode,
                     tex_unit: TextureUnit,
                     gen_mipmaps: bool) -> Result<Self, String> {

        debug_assert!(file_path.is_empty() == false);

        let mut image = match image::open(file_path) {
            Ok(image) => image,
            Err(error_info) => {
                return Err(format!("Failed to load image file '{}': {:?}", file_path, error_info));
            }
        };

        if flags.contains(ImageLoaderFlags::FlipV) {
            image.apply_orientation(image::metadata::Orientation::FlipVertical);
        }

        if flags.contains(ImageLoaderFlags::FlipH) {
            image.apply_orientation(image::metadata::Orientation::FlipHorizontal);
        }

        let image_dims = image.dimensions();
        let image_buffer = image.to_rgba8();
        let image_raw_bytes = image_buffer.into_raw();

        Ok(Self::with_data_raw(
            image_raw_bytes.as_ptr() as *const c_void,
            Size2D::new(
                image_dims.0  as i32,
                image_dims.1 as i32
            ),
            filter,
            wrap_mode,
            tex_unit,
            gen_mipmaps,
            file_path
        ))
    }

    pub fn with_data_raw(data: *const c_void,
                         size: Size2D,
                         filter: TextureFilter,
                         wrap_mode: TextureWrapMode,
                         tex_unit: TextureUnit,
                         gen_mipmaps: bool,
                         debug_name: &str) -> Self {

        debug_assert!((tex_unit.0 as usize) < MAX_TEXTURE_UNITS);
        debug_assert!(data.is_null() == false);
        debug_assert!(size.is_valid());

        let handle = unsafe {
            let mut handle = NULL_TEXTURE_HANDLE;
            gl::GenTextures(1, &mut handle);
            if handle == NULL_TEXTURE_HANDLE {
                panic!("Failed to create texture handle!");
            }

            gl::ActiveTexture(gl::TEXTURE0 + tex_unit.0);
            gl::BindTexture(gl::TEXTURE_2D, handle);

            gl::TexImage2D(
                gl::TEXTURE_2D,
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

            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_MIN_FILTER,
                gl_min_filter as gl::types::GLint);
            gl::TexParameteri(
                gl::TEXTURE_2D,
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

        Self {
            handle: handle,
            tex_unit: tex_unit,
            size: size,
            filter: filter,
            wrap_mode: wrap_mode,
            has_mipmaps: gen_mipmaps,
            name: debug_name.to_string(),
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

    pub fn size(&self) -> Size2D {
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

const DUMMY_TEXTURE_HANDLE_INDEX: i32 = -1;
const WHITE_TEXTURE_HANDLE_INDEX: i32 = -2;

#[derive(Copy, Clone)]
pub struct TextureHandle {
    index: i32,
}

impl TextureHandle {
    pub const fn invalid() -> Self {
        Self { index : DUMMY_TEXTURE_HANDLE_INDEX } // Returns dummy_texture
    }

    pub const fn white() -> Self {
        Self { index : WHITE_TEXTURE_HANDLE_INDEX } // Returns white_texture
    }

    pub fn is_valid(&self) -> bool {
        self.index >= 0
    }
}

pub struct TextureCache {
    textures: Vec<Texture2D>,

    // These are 8x8 pixels.
    dummy_texture: TextureHandle, // DUMMY_TEXTURE_HANDLE_INDEX
    white_texture: TextureHandle, // WHITE_TEXTURE_HANDLE_INDEX
}

impl TextureCache {
    pub fn new(initial_capacity: usize) -> Self {
        let mut tex_cache = Self {
            textures: Vec::with_capacity(initial_capacity),
            dummy_texture: TextureHandle::invalid(),
            white_texture: TextureHandle::invalid(),
        };

        tex_cache.dummy_texture = tex_cache.create_color_filled_8x8_texture(
            "dummy_texture", [ 255, 0,   255, 255 ]);

        tex_cache.white_texture = tex_cache.create_color_filled_8x8_texture(
            "white_texture", [ 255, 255, 255, 255 ]);

        tex_cache
    }

    pub fn handle_to_texture(&self, handle: TextureHandle) -> &Texture2D {
        if handle.is_valid() && (handle.index as usize) < self.textures.len() {
            &self.textures[handle.index as usize]
        } else {
            if handle.index == WHITE_TEXTURE_HANDLE_INDEX {
                &self.textures[self.white_texture.index as usize]
            } else {
                &self.textures[self.dummy_texture.index as usize]
            }
        }
    }

    pub fn load_texture_with_settings(&mut self,
                                      file_path: &str,
                                      flags: ImageLoaderFlags,
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
            Err(info_log) => {
                eprintln!("TextureCache Error: {}", info_log);
                return self.dummy_texture;
            },
        };

        let index = self.textures.len();
        self.textures.push(texture);

        TextureHandle { index: index as i32 }
    }

    pub fn load_texture(&mut self, file_path: &str) -> TextureHandle {
        Self::load_texture_with_settings(self,
                                         file_path,
                                         ImageLoaderFlags::None,
                                         TextureFilter::Nearest,
                                         TextureWrapMode::ClampToEdge,
                                         TextureUnit(0),
                                         false)
    }

    pub fn dummy_texture_handle(&self) -> TextureHandle {
        self.dummy_texture
    }

    pub fn white_texture_handle(&self) -> TextureHandle {
        self.white_texture
    }

    fn create_color_filled_8x8_texture(&mut self, debug_name: &str, color: [u8; 4]) -> TextureHandle {
        #[derive(Copy, Clone)]
        struct RGBA8 {
            r: u8,
            g: u8,
            b: u8,
            a: u8,
        }

        const SIZE: Size2D = Size2D::new(8, 8);
        const PIXEL_COUNT: usize = (SIZE.width * SIZE.height) as usize;
        let pixels = [RGBA8{ r: color[0], g: color[1], b: color[2], a: color[3] }; PIXEL_COUNT];

        let texture = Texture2D::with_data_raw(pixels.as_ptr() as *const c_void,
                                                          SIZE,
                                                          TextureFilter::Nearest,
                                                          TextureWrapMode::ClampToEdge,
                                                          TextureUnit(0),
                                                          false,
                                                          debug_name);

        let index = self.textures.len();
        self.textures.push(texture);

        TextureHandle { index: index as i32 }
    }
}
