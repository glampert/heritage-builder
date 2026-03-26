use std::ffi::c_void;
use strum::Display;

use super::{
    gl_error_to_string,
    shader::{ShaderVarTrait, ShaderVariable},
};
use crate::{
    render,
    utils::{Size, hash::{self, StringHash}},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const NULL_TEXTURE_HANDLE: gl::types::GLuint = 0;
pub const MAX_TEXTURE_UNITS: usize = 4;

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
                                 texture.tex_unit.0 as _);
        }
    }
}

// ----------------------------------------------
// Texture Sampling / Addressing
// ----------------------------------------------

// Equivalent to the GL_TEXTURE_FILTER enums.
#[repr(u32)]
#[derive(Copy, Clone, Display)]
pub enum TextureFilter {
    Nearest = gl::NEAREST,
    Linear  = gl::LINEAR,
    NearestMipmapNearest = gl::NEAREST_MIPMAP_NEAREST,
    LinearMipmapNearest  = gl::LINEAR_MIPMAP_NEAREST,
    NearestMipmapLinear  = gl::NEAREST_MIPMAP_LINEAR,
    LinearMipmapLinear   = gl::LINEAR_MIPMAP_LINEAR,
}

// Equivalent to the GL_TEXTURE_WRAP enums.
#[repr(u32)]
#[derive(Copy, Clone, Display)]
pub enum TextureWrapMode {
    Repeat = gl::REPEAT,
    ClampToEdge = gl::CLAMP_TO_EDGE,
    ClampToBorder = gl::CLAMP_TO_BORDER,
}

// ----------------------------------------------
// Texture2D
// ----------------------------------------------

pub struct Texture2D {
    name: String,
    size: Size,
    settings: TextureSettings,
    tex_unit: TextureUnit,
    allow_settings_change: bool,
    handle: gl::types::GLuint,
}

impl Texture2D {
    pub fn with_data_raw(name: &str,
                         size: Size,
                         data: *const c_void,
                         mut settings: TextureSettings,
                         tex_unit: TextureUnit,
                         allow_settings_change: bool) -> Self
    {
        debug_assert!(!name.is_empty());
        debug_assert!(size.is_valid());
        debug_assert!((tex_unit.0 as usize) < MAX_TEXTURE_UNITS);

        let handle = unsafe {
            let mut handle = NULL_TEXTURE_HANDLE;
            gl::GenTextures(1, &mut handle);
            if handle == NULL_TEXTURE_HANDLE {
                panic!("Failed to create texture handle!");
            }

            bind_gl_texture(handle, tex_unit);

            gl::TexImage2D(gl::TEXTURE_2D,
                           0,
                           gl::RGBA as _, // Only RGBA images supported for now.
                           size.width as _,
                           size.height as _,
                           0,
                           gl::RGBA,
                           gl::UNSIGNED_BYTE,
                           data);

            let has_mipmaps = set_current_gl_texture_params(settings.filter,
                                                            settings.wrap_mode,
                                                            settings.mipmaps,
                                                            name);

            unbind_gl_texture();

            settings.mipmaps = has_mipmaps;
            handle
        };

        Self {
            name: name.to_string(),
            size,
            settings,
            tex_unit,
            allow_settings_change,
            handle,
        }
    }

    pub fn update(&self,
                  offset_x: u32,
                  offset_y: u32,
                  size: Size,
                  mip_level: u32,
                  pixels: &[u8])
    {
        debug_assert!(self.is_valid());
        debug_assert!(offset_x as i32 + size.width  <= self.size.width);
        debug_assert!(offset_y as i32 + size.height <= self.size.height);
        debug_assert!(!pixels.is_empty());

        bind_gl_texture(self.handle, self.tex_unit);

        unsafe {
            gl::TexSubImage2D(gl::TEXTURE_2D,
                              mip_level as _,
                              offset_x as _,
                              offset_y as _,
                              size.width,
                              size.height,
                              gl::RGBA,
                              gl::UNSIGNED_BYTE,
                              pixels.as_ptr() as _);
        }

        unbind_gl_texture();
    }

    pub fn change_settings(&mut self, settings: TextureSettings) {
        debug_assert!(self.is_valid());
        debug_assert!(self.allow_settings_change());

        bind_gl_texture(self.handle, self.tex_unit);

        let has_mipmaps = set_current_gl_texture_params(settings.filter,
                                                        settings.wrap_mode,
                                                        settings.mipmaps,
                                                        &self.name);

        unbind_gl_texture();

        self.settings = settings;
        self.settings.mipmaps = has_mipmaps;
    }

    pub fn release(&mut self) {
        if self.handle != NULL_TEXTURE_HANDLE {
            unsafe {
                gl::DeleteTextures(1, &self.handle);
            }

            self.handle = NULL_TEXTURE_HANDLE;
            self.name   = String::new();
            self.size   = Size::zero();
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.handle != NULL_TEXTURE_HANDLE && self.size.is_valid()
    }

    #[inline]
    pub fn handle(&self) -> gl::types::GLuint {
        self.handle
    }

    #[inline]
    pub fn tex_unit(&self) -> TextureUnit {
        self.tex_unit
    }

    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }

    #[inline]
    pub fn filter(&self) -> TextureFilter {
        self.settings.filter
    }

    #[inline]
    pub fn wrap_mode(&self) -> TextureWrapMode {
        self.settings.wrap_mode
    }

    #[inline]
    pub fn has_mipmaps(&self) -> bool {
        self.settings.mipmaps
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn hash(&self) -> StringHash {
        hash::fnv1a_from_str(&self.name)
    }

    #[inline]
    pub fn allow_settings_change(&self) -> bool {
        self.allow_settings_change
    }
}

impl Drop for Texture2D {
    fn drop(&mut self) {
        self.release();
    }
}

// ----------------------------------------------
// TextureSettings
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct TextureSettings {
    pub filter: TextureFilter,
    pub wrap_mode: TextureWrapMode,
    pub mipmaps: bool,
}

// Convert to/from frontend render::texture settings/filter/wrap mode:

impl From<render::texture::TextureSettings> for TextureSettings {
    fn from(settings: render::texture::TextureSettings) -> Self {
        Self {
            filter: settings.filter.into(),
            wrap_mode: settings.wrap_mode.into(),
            mipmaps: settings.mipmaps,
        }
    }
}

impl From<TextureFilter> for render::texture::TextureFilter {
    fn from(filter: TextureFilter) -> render::texture::TextureFilter {
        match filter {
            TextureFilter::Nearest              => render::texture::TextureFilter::Nearest,
            TextureFilter::Linear               => render::texture::TextureFilter::Linear,
            TextureFilter::NearestMipmapNearest => render::texture::TextureFilter::NearestMipmapNearest,
            TextureFilter::LinearMipmapNearest  => render::texture::TextureFilter::LinearMipmapNearest,
            TextureFilter::NearestMipmapLinear  => render::texture::TextureFilter::NearestMipmapLinear,
            TextureFilter::LinearMipmapLinear   => render::texture::TextureFilter::LinearMipmapLinear,
        }
    }
}

impl From<render::texture::TextureFilter> for TextureFilter {
    fn from(filter: render::texture::TextureFilter) -> TextureFilter {
        match filter {
            render::texture::TextureFilter::Nearest              => TextureFilter::Nearest,
            render::texture::TextureFilter::Linear               => TextureFilter::Linear,
            render::texture::TextureFilter::NearestMipmapNearest => TextureFilter::NearestMipmapNearest,
            render::texture::TextureFilter::LinearMipmapNearest  => TextureFilter::LinearMipmapNearest,
            render::texture::TextureFilter::NearestMipmapLinear  => TextureFilter::NearestMipmapLinear,
            render::texture::TextureFilter::LinearMipmapLinear   => TextureFilter::LinearMipmapLinear,
        }
    }
}

impl From<TextureWrapMode> for render::texture::TextureWrapMode {
    fn from(mode: TextureWrapMode) -> render::texture::TextureWrapMode {
        match mode {
            TextureWrapMode::Repeat        => render::texture::TextureWrapMode::Repeat,
            TextureWrapMode::ClampToEdge   => render::texture::TextureWrapMode::ClampToEdge,
            TextureWrapMode::ClampToBorder => render::texture::TextureWrapMode::ClampToBorder,
        }
    }
}

impl From<render::texture::TextureWrapMode> for TextureWrapMode {
    fn from(wrap_mode: render::texture::TextureWrapMode) -> TextureWrapMode {
        match wrap_mode {
            render::texture::TextureWrapMode::Repeat        => TextureWrapMode::Repeat,
            render::texture::TextureWrapMode::ClampToEdge   => TextureWrapMode::ClampToEdge,
            render::texture::TextureWrapMode::ClampToBorder => TextureWrapMode::ClampToBorder,
        }
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

#[inline]
fn bind_gl_texture(handle: gl::types::GLuint, tex_unit: TextureUnit) {
    unsafe {
        gl::ActiveTexture(gl::TEXTURE0 + tex_unit.0);
        gl::BindTexture(gl::TEXTURE_2D, handle);
    }
}

#[inline]
fn unbind_gl_texture() {
    unsafe {
        gl::BindTexture(gl::TEXTURE_2D, NULL_TEXTURE_HANDLE);
    }
}

// Set params for currently bound texture.
// Returns true if `mipmaps=true` & mipmap building succeeded.
fn set_current_gl_texture_params(filter: TextureFilter,
                                 wrap_mode: TextureWrapMode,
                                 mipmaps: bool,
                                 name: &str) -> bool
{
    unsafe {
        let has_mipmaps = {
            if mipmaps && gl::GenerateMipmap::is_loaded() {
                gl::GenerateMipmap(gl::TEXTURE_2D);
                let error_code = gl::GetError();
                if error_code != gl::NO_ERROR {
                    panic!("Failed to generate texture mipmaps for '{name}'. OpenGL Error: {} (0x{:X})",
                           gl_error_to_string(error_code),
                           error_code);
                }
                true
            } else {
                false
            }
        };

        let (gl_min_filter, gl_mag_filter) = match filter {
            TextureFilter::Nearest => (gl::NEAREST, gl::NEAREST),
            TextureFilter::Linear  => (gl::LINEAR,  gl::LINEAR),
            TextureFilter::NearestMipmapNearest => (gl::NEAREST_MIPMAP_NEAREST, gl::NEAREST),
            TextureFilter::LinearMipmapNearest  => (gl::LINEAR_MIPMAP_NEAREST,  gl::LINEAR),
            TextureFilter::NearestMipmapLinear  => (gl::NEAREST_MIPMAP_LINEAR,  gl::NEAREST),
            TextureFilter::LinearMipmapLinear   => (gl::LINEAR_MIPMAP_LINEAR,   gl::LINEAR),
        };

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl_min_filter as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl_mag_filter as _);

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, wrap_mode as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, wrap_mode as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_R, wrap_mode as _);

        has_mipmaps
    }
}
