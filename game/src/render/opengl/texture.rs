#![allow(clippy::too_many_arguments)]

use std::{any::Any, ffi::c_void};
use num_enum::TryFromPrimitive;
use strum::VariantArray;
use strum_macros::Display;
use image::GenericImageView;
use bitflags::bitflags;
use slab::Slab;

use super::{
    gl_error_to_string,
    panic_if_gl_error,
    shader::{ShaderVarTrait, ShaderVariable},
};
use crate::{
    log,
    utils::Size,
    imgui_ui::UiSystem,
    render::{self, NativeTextureHandle, TextureHandle},
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
                                 texture.tex_unit.0 as _);
        }
    }
}

// ----------------------------------------------
// Texture Sampling
// ----------------------------------------------

// Equivalent to the GL_TEXTURE_FILTER enums.
#[repr(u32)]
#[derive(Copy, Clone, Display)]
pub enum TextureFilter {
    Nearest = gl::NEAREST,
    Linear = gl::LINEAR,
    NearestMipmapNearest = gl::NEAREST_MIPMAP_NEAREST,
    LinearMipmapNearest = gl::LINEAR_MIPMAP_NEAREST,
    NearestMipmapLinear = gl::NEAREST_MIPMAP_LINEAR,
    LinearMipmapLinear = gl::LINEAR_MIPMAP_LINEAR,
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

        // Avoid conversion if the image is already in RGBA8 format.
        let image_buffer = {
            if image.color() != image::ColorType::Rgba8 {
                &image.to_rgba8()
            } else {
                image.as_rgba8().expect("Expected an RGBA8 image!")
            }
        };

        let (image_w, image_h) = image.dimensions();
        let image_pixels = image_buffer.as_raw();

        Ok(Self::with_data_raw(image_pixels.as_ptr() as _,
                               Size::new(image_w as _, image_h as _),
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

        let (handle, has_mipmaps) = unsafe {
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

            let has_mipmaps = set_current_gl_texture_params(filter,
                                                                  wrap_mode,
                                                                  gen_mipmaps,
                                                                  debug_name);

            unbind_gl_texture();

            (handle, has_mipmaps)
        };

        Self { handle,
               tex_unit,
               size,
               filter,
               wrap_mode,
               has_mipmaps,
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

    fn change_settings(&mut self, settings: OpenGlTextureSettings) {
        debug_assert!(self.is_valid());

        bind_gl_texture(self.handle, self.tex_unit);

        self.filter = settings.filter;
        self.wrap_mode = settings.wrap_mode;
        self.has_mipmaps = set_current_gl_texture_params(settings.filter,
                                                         settings.wrap_mode,
                                                         settings.gen_mipmaps,
                                                         &self.name);

        unbind_gl_texture();
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
// OpenGlTextureSettings
// ----------------------------------------------

#[derive(Copy, Clone)]
struct OpenGlTextureSettings {
    filter: TextureFilter,
    wrap_mode: TextureWrapMode,
    gen_mipmaps: bool,
}

impl From<render::TextureSettings> for OpenGlTextureSettings {
    fn from(settings: render::TextureSettings) -> Self {
        let filter = match settings.filter {
            render::TextureFilter::Nearest => TextureFilter::Nearest,
            render::TextureFilter::Linear => TextureFilter::Linear,
            render::TextureFilter::NearestMipmapNearest => TextureFilter::NearestMipmapNearest,
            render::TextureFilter::LinearMipmapNearest => TextureFilter::LinearMipmapNearest,
            render::TextureFilter::NearestMipmapLinear => TextureFilter::NearestMipmapLinear,
            render::TextureFilter::LinearMipmapLinear => TextureFilter::LinearMipmapLinear,
        };
        let wrap_mode = match settings.wrap_mode {
            render::TextureWrapMode::Repeat => TextureWrapMode::Repeat,
            render::TextureWrapMode::ClampToEdge => TextureWrapMode::ClampToEdge,
            render::TextureWrapMode::ClampToBorder => TextureWrapMode::ClampToBorder,
        };
        Self {
            filter,
            wrap_mode,
            gen_mipmaps: settings.gen_mipmaps,
        }
    }
}

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
// Returns true if gen_mipmaps & mipmap building succeeded.
fn set_current_gl_texture_params(filter: TextureFilter,
                                 wrap_mode: TextureWrapMode,
                                 gen_mipmaps: bool,
                                 debug_name: &str) -> bool {
    unsafe {
        let has_mipmaps = {
            if gen_mipmaps && gl::GenerateMipmap::is_loaded() {
                gl::GenerateMipmap(gl::TEXTURE_2D);
                let error_code = gl::GetError();
                if error_code != gl::NO_ERROR {
                    panic!("Failed to generate texture mipmaps for '{debug_name}'. OpenGL Error: {} (0x{:X})",
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

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

struct TexCacheEntry {
    texture: Texture2D,
    allow_settings_change: bool,
}

// FIXME: Have to make this a proper cache now.
// Keep a map with all loaded textures, key is file path + TextureSettings.
pub struct TextureCache {
    textures: Slab<TexCacheEntry>,
    settings: render::TextureSettings,

    // These are 8x8 pixels.
    dummy_texture_handle: TextureHandle, // TextureHandle::Invalid
    white_texture_handle: TextureHandle, // TextureHandle::White
}

impl TextureCache {
    pub fn new(initial_capacity: usize) -> Self {
        let mut tex_cache = Self {
            textures: Slab::with_capacity(initial_capacity),
            settings: render::TextureSettings::default(),
            dummy_texture_handle: TextureHandle::invalid(),
            white_texture_handle: TextureHandle::invalid(),
        };

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
                if let Some(entry) = self.textures.get(handle_index as usize) {
                    &entry.texture
                } else {
                    self.dummy_texture()
                }
            }
        }
    }

    #[inline]
    pub fn dummy_texture(&self) -> &Texture2D {
        match self.dummy_texture_handle {
            TextureHandle::Index(index) => {
                &self.textures.get(index as usize).unwrap().texture
            }
            _ => panic!("Unexpected value for dummy_texture_handle!"),
        }
    }

    #[inline]
    pub fn white_texture(&self) -> &Texture2D {
        match self.white_texture_handle {
            TextureHandle::Index(index) => {
                &self.textures.get(index as usize).unwrap().texture
            }
            _ => panic!("Unexpected value for white_texture_handle!"),
        }
    }

    #[inline]
    fn add_texture(&mut self, texture: Texture2D, allow_settings_change: bool) -> TextureHandle {
        let index = self.textures.insert(TexCacheEntry { texture, allow_settings_change });
        TextureHandle::Index(index as u32)
    }

    fn load_texture_with_settings_internal(&mut self,
                                           file_path: &str,
                                           flags: TextureLoaderFlags,
                                           filter: TextureFilter,
                                           wrap_mode: TextureWrapMode,
                                           tex_unit: TextureUnit,
                                           gen_mipmaps: bool,
                                           allow_settings_change: bool)
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

        self.add_texture(texture, allow_settings_change)
    }

    fn create_color_filled_8x8_texture(&mut self,
                                       debug_name: &str,
                                       color: [u8; 4])
                                       -> TextureHandle {
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

        let gl_settings = OpenGlTextureSettings::from(self.settings);
        let texture = Texture2D::with_data_raw(pixels.as_ptr() as _,
                                               SIZE,
                                               gl_settings.filter,
                                               gl_settings.wrap_mode,
                                               TextureUnit(0),
                                               gl_settings.gen_mipmaps,
                                               debug_name);

        self.add_texture(texture, true)
    }
}

impl render::TextureCache for TextureCache {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_native_handle(&self, handle: TextureHandle) -> NativeTextureHandle {
        self.handle_to_texture(handle).native_handle()
    }

    fn load_texture(&mut self, file_path: &str) -> TextureHandle {
        let allow_settings_change = true;
        let gl_settings = OpenGlTextureSettings::from(self.settings);

        self.load_texture_with_settings_internal(file_path,
                                                 TextureLoaderFlags::empty(),
                                                 gl_settings.filter,
                                                 gl_settings.wrap_mode,
                                                 TextureUnit(0),
                                                 gl_settings.gen_mipmaps,
                                                 allow_settings_change)
    }

    fn load_texture_with_settings(&mut self,
                                  file_path: &str,
                                  settings: Option<render::TextureSettings>)
                                  -> TextureHandle {
        let allow_settings_change = settings.is_none();
        let gl_settings = OpenGlTextureSettings::from(settings.unwrap_or(self.settings));

        self.load_texture_with_settings_internal(file_path,
                                                 TextureLoaderFlags::empty(),
                                                 gl_settings.filter,
                                                 gl_settings.wrap_mode,
                                                 TextureUnit(0),
                                                 gl_settings.gen_mipmaps,
                                                 allow_settings_change)
    }

    fn change_texture_settings(&mut self, settings: render::TextureSettings) {
        log::info!(log::channel!("render"),
                   "Changing texture settings: filter:{}, wrap:{}, mipmaps:{}",
                   settings.filter,
                   settings.wrap_mode,
                   settings.gen_mipmaps);

        self.settings = settings;
        let gl_settings = OpenGlTextureSettings::from(settings);

        for (_, entry) in &mut self.textures {
            if entry.allow_settings_change {
                entry.texture.change_settings(gl_settings);
            }
        }

        panic_if_gl_error();
    }

    fn current_texture_settings(&self) -> render::TextureSettings {
        self.settings
    }

    fn new_uninitialized_texture(&mut self,
                                 debug_name: &str,
                                 size: Size,
                                 settings: Option<render::TextureSettings>)
                                 -> TextureHandle {
        debug_assert!(size.is_valid());

        let allow_settings_change = settings.is_none();
        let gl_settings = OpenGlTextureSettings::from(settings.unwrap_or(self.settings));

        let texture = Texture2D::with_data_raw(core::ptr::null(),
                                               size,
                                               gl_settings.filter,
                                               gl_settings.wrap_mode,
                                               TextureUnit(0),
                                               gl_settings.gen_mipmaps,
                                               debug_name);

        self.add_texture(texture, allow_settings_change)
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

    fn release_texture(&mut self, handle: &mut TextureHandle) {
        if let TextureHandle::Index(handle_index) = handle {
            self.textures.try_remove(*handle_index as usize);
        }
        *handle = TextureHandle::invalid();
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if let Some(_tab_bar) = ui.tab_bar("Texture Cache Tab Bar") {
            if let Some(_tab) = ui.tab_item("Filtering") {
                let mut current_settings = self.current_texture_settings();
                let mut settings_changed = false;

                let mut current_filter_index = current_settings.filter as usize;
                if ui.combo("Filter",
                            &mut current_filter_index,
                            render::TextureFilter::VARIANTS,
                            |v| { v.to_string().into() })
                {
                    settings_changed = true;
                }

                let mut current_wrap_mode_index = current_settings.wrap_mode as usize;
                if ui.combo("Wrap Mode",
                            &mut current_wrap_mode_index,
                            render::TextureWrapMode::VARIANTS,
                            |v| { v.to_string().into() })
                {
                    settings_changed = true;
                }

                let mut gen_mipmaps = current_settings.gen_mipmaps;
                if ui.checkbox("Mipmaps", &mut gen_mipmaps) {
                    settings_changed = true;
                }

                if settings_changed {
                    current_settings.filter = render::TextureFilter::try_from_primitive(current_filter_index as u32).unwrap();
                    current_settings.wrap_mode = render::TextureWrapMode::try_from_primitive(current_wrap_mode_index as u32).unwrap();
                    current_settings.gen_mipmaps = gen_mipmaps;
                    self.change_texture_settings(current_settings);
                }
            }

            if let Some(_tab) = ui.tab_item("Loaded Textures") {
                let table_col = |label: &str| {
                    ui.text(label);
                    ui.next_column();
                };

                let bool_str = |val: bool| {
                    if val { "yes" } else { "no" }
                };

                ui.text(format!("Loaded Count: {}", self.textures.len()));
                ui.separator();

                // Set number of rows (emulated with columns):
                ui.columns(8, "texture_columns", true);

                // Header row:
                table_col("Index");
                table_col("Name");
                table_col("Size");
                table_col("Change Settings");
                table_col("Mipmaps");
                table_col("Filter");
                table_col("Wrap");
                table_col("Unit");

                ui.separator();

                for (index, entry) in &mut self.textures {
                    table_col(&format!("{}", index));
                    table_col(&entry.texture.name);
                    table_col(&format!("{}x{}", entry.texture.size.width, entry.texture.size.height));
                    table_col(bool_str(entry.allow_settings_change));
                    table_col(bool_str(entry.texture.has_mipmaps));
                    table_col(&entry.texture.filter.to_string());
                    table_col(&entry.texture.wrap_mode.to_string());
                    table_col(&format!("{}", entry.texture.tex_unit.0));
                }

                // Return to single column.
                ui.columns(1, "", false);
            }
        }
    }
}
