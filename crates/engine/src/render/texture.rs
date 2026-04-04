use slab::Slab;
use arrayvec::ArrayVec;
use image::RgbaImage;
use strum::VariantArray;
use num_enum::TryFromPrimitive;
use proc_macros::DrawDebugUi;
use serde::{Serialize, Deserialize};

use common::{
    format_fixed_string,
    mem::WeakMut,
    hash::{self, StringHash, PreHashedKeyMap},
};
use super::*;
use crate::{
    log,
    ui::UiSystem,
    file_sys::{self, paths::PathRef},
};

// ----------------------------------------------
// TextureHandle
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TextureHandle {
    Invalid,    // Returns built-in dummy_texture.
    White,      // Returns built-in white_texture.
    Index(u32), // Index into TextureCache array of textures.
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
        !matches!(self, TextureHandle::Invalid)
    }

    // Pack/unpack into usize for ImGui TextureId.
    // Tag in the top 2 bits, index in the lower 30 bits.
    // Supports up to 2^30 (~1 billion) texture indices.
    const TAG_SHIFT: u32 = usize::BITS - 2;
    const INDEX_MASK: usize = (1 << Self::TAG_SHIFT) - 1;

    #[inline]
    pub fn pack(&self) -> usize {
        match self {
            Self::Invalid => 0,
            Self::White   => 1 << Self::TAG_SHIFT,
            Self::Index(idx) => (2 << Self::TAG_SHIFT) | (*idx as usize & Self::INDEX_MASK),
        }
    }

    #[inline]
    pub fn unpack(value: usize) -> Self {
        let tag = value >> Self::TAG_SHIFT;
        let idx = (value & Self::INDEX_MASK) as u32;
        match tag {
            0 => { debug_assert_eq!(idx, 0); Self::Invalid }
            1 => { debug_assert_eq!(idx, 0); Self::White   }
            2 => Self::Index(idx),
            _ => panic!("Invalid packed TextureHandle!"),
        }
    }
}

impl Default for TextureHandle {
    #[inline]
    fn default() -> Self {
        TextureHandle::invalid()
    }
}

// ----------------------------------------------
// TextureSettings (Filter, Wrap Mode)
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Display, VariantArray, TryFromPrimitive, Serialize, Deserialize)]
pub enum TextureFilter {
    Nearest,
    Linear,
    NearestMipmapNearest,
    LinearMipmapNearest,
    NearestMipmapLinear,
    LinearMipmapLinear,
}

#[repr(u32)]
#[derive(Copy, Clone, Display, VariantArray, TryFromPrimitive, Serialize, Deserialize)]
pub enum TextureWrapMode {
    Repeat,
    ClampToEdge,
    ClampToBorder,
}

#[derive(Copy, Clone, DrawDebugUi, Serialize, Deserialize)]
pub struct TextureSettings {
    pub filter: TextureFilter,
    pub wrap_mode: TextureWrapMode,
    pub mipmaps: bool,
}

impl Default for TextureSettings {
    #[inline]
    fn default() -> Self {
        Self {
            filter: TextureFilter::Nearest,
            wrap_mode: TextureWrapMode::ClampToEdge,
            mipmaps: false,
        }
    }
}

// ----------------------------------------------
// Texture
// ----------------------------------------------

#[enum_dispatch(TextureBackendImpl)]
pub(super) trait Texture: Sized {
    fn is_valid(&self) -> bool;
    fn name(&self) -> &str;
    fn hash(&self) -> StringHash;
    fn size(&self) -> Size;
    fn has_mipmaps(&self) -> bool;
    fn filter(&self) -> TextureFilter;
    fn wrap_mode(&self) -> TextureWrapMode;
    fn allow_settings_change(&self) -> bool;
}

// ----------------------------------------------
// Backend Texture implementations
// ----------------------------------------------

#[enum_dispatch]
pub(super) enum TextureBackendImpl {
    Wgpu(wgpu::WgpuTexture),

    #[cfg(feature = "desktop")]
    OpenGl(opengl::OpenGlTexture),
}

macro_rules! texture_backend_type_casts {
    ($variant:ident, $func:ident, $func_mut:ident, $type:ty) => {
        #[inline]
        #[must_use]
        #[allow(unreachable_patterns)]
        pub(super) fn $func(&self) -> &$type {
            match self {
                Self::$variant(tex) => tex,
                _ => panic!("Unexpected TextureBackendImpl variant!"),
            }
        }
        #[inline]
        #[must_use]
        #[allow(unreachable_patterns)]
        pub(super) fn $func_mut(&mut self) -> &mut $type {
            match self {
                Self::$variant(tex) => tex,
                _ => panic!("Unexpected TextureBackendImpl variant!"),
            }
        }
    };
}

impl TextureBackendImpl {
    texture_backend_type_casts! { Wgpu, as_wgpu, as_wgpu_mut, wgpu::WgpuTexture }

    #[cfg(feature = "desktop")]
    texture_backend_type_casts! { OpenGl, as_opengl, as_opengl_mut, opengl::OpenGlTexture }
}

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

pub struct TextureCache {
    lookup:   PreHashedKeyMap<StringHash, u32>,
    textures: Slab<TextureBackendImpl>,
    settings: TextureSettings, // Global default settings.

    // These are 8x8 pixels.
    dummy_texture_handle: TextureHandle, // TextureHandle::Invalid
    white_texture_handle: TextureHandle, // TextureHandle::White

    render_system: WeakMut<RenderSystem>,
}

impl TextureCache {
    // ----------------------
    // Texture Cache API:
    // ----------------------

    // Tries to find an already loaded texture with the given name or file path.
    pub fn find_loaded_texture(&self, name_or_file_path: &str) -> Option<TextureHandle> {
        debug_assert!(!name_or_file_path.is_empty());
        debug_assert!(self.is_valid());

        if let Some(&idx) = self.lookup.get(&hash::fnv1a_from_str(name_or_file_path)) {
            debug_assert!(self.texture_at_index(idx).is_valid());
            return Some(TextureHandle::Index(idx));
        }

        None
    }

    // Load texture with default settings, which can be overridden by change_texture_settings().
    pub fn load_texture(&mut self, file_path: PathRef) -> TextureHandle {
        self.load_texture_with_settings(file_path, None)
    }

    // If settings are provided they will be used and will not be affected by change_texture_settings().
    pub fn load_texture_with_settings(&mut self,
                                      file_path: PathRef,
                                      settings: Option<TextureSettings>)
                                      -> TextureHandle
    {
        if let Some(loaded_texture) = self.find_loaded_texture(file_path.as_str()) {
            return loaded_texture; // Already loaded.
        }

        let image = match Self::load_image_file(file_path) {
            Ok(image) => image,
            Err(_) => {
                log::error!(log::channel!("render"), "Failed to load texture '{file_path}'.");
                return TextureHandle::invalid();
            }
        };

        let size = Size::new(image.width() as i32, image.height() as i32);
        let pixels = image.as_raw();

        self.new_initialized_texture(file_path.as_str(), size, pixels, settings)
    }

    // If settings are provided they will be used and will not be affected by change_texture_settings().
    pub fn new_uninitialized_texture(&mut self,
                                     name: &str,
                                     size: Size,
                                     settings: Option<TextureSettings>)
                                     -> TextureHandle
    {
        let pixels = []; // Empty pixels slice = uninitialized.
        self.new_initialized_texture(name, size, &pixels, settings)
    }

    // New texture with initial pixel data.
    pub fn new_initialized_texture(&mut self,
                                   name: &str,
                                   size: Size,
                                   pixels: &[u8],
                                   settings: Option<TextureSettings>)
                                   -> TextureHandle
    {
        debug_assert!(!name.is_empty());
        debug_assert!(size.is_valid());
        // `pixels` slice may be empty.

        if self.find_loaded_texture(name).is_some() {
            log::error!(log::channel!("render"), "A texture named '{name}' already exists! Choose a different name.");
            return TextureHandle::invalid();
        }

        let mut render_system = self.render_sys_rc();

        let allow_settings_change = settings.is_none();
        let texture = render_system.new_texture_from_pixels(
            name,
            size,
            pixels,
            settings.unwrap_or(self.settings),
            allow_settings_change
        );

        self.register_new_texture(texture)
    }

    // Update texture mip-level sub-rect or whole texture.
    pub fn update_texture(&mut self,
                          handle: TextureHandle,
                          offset_x: u32,
                          offset_y: u32,
                          size: Size,
                          mip_level: u32,
                          pixels: &[u8])
    {
        debug_assert!(size.is_valid());
        debug_assert!(pixels.len() >= (size.width * size.height * 4) as usize); // RGBA images only.
        debug_assert!(!matches!(handle, TextureHandle::Invalid | TextureHandle::White));
        debug_assert!(self.is_valid());

        let mut render_system = self.render_sys_rc();
        let texture = self.texture_for_handle(handle);

        render_system.update_texture_pixels(texture, offset_x, offset_y, size, mip_level, pixels);
    }

    // Explicitly unloads a texture. `handle` is set to invalid after this call.
    pub fn release_texture(&mut self, handle: &mut TextureHandle) {
        debug_assert!(self.is_valid());

        if let TextureHandle::Index(idx) = *handle {
            let mut removed_successfully = false;

            let mut texture = {
                if let Some(texture) = self.textures.try_remove(idx as usize) {
                    removed_successfully = self.lookup.remove(&texture.hash()).is_some();
                    Some(texture)
                } else {
                    None
                }
            };

            if let Some(texture) = &mut texture {
                let mut render_system = self.render_sys_rc();
                render_system.release_texture(texture); // Release GPU resources immediately.
            }   

            if !removed_successfully {
                log::error!(log::channel!("render"), "Failed to remove TextureCache entry [{idx}].");
            }
        }

        *handle = TextureHandle::invalid();
    }

    // ----------------------
    // Texture Settings:
    // ----------------------

    // Global texture settings override.
    pub fn change_texture_settings(&mut self, settings: TextureSettings) {
        debug_assert!(self.is_valid());

        log::info!(log::channel!("render"),
                   "Changing texture settings: Filter:{}, WrapMode:{}, Mipmaps:{}",
                   settings.filter, settings.wrap_mode, settings.mipmaps);

        let mut render_system = self.render_sys_rc();

        for (_, texture) in &mut self.textures {
            if texture.allow_settings_change() {
                render_system.update_texture_settings(texture, settings);
            }
        }

        self.settings = settings;
    }

    #[inline]
    pub fn current_texture_settings(&self) -> TextureSettings {
        self.settings
    }

    // ----------------------
    // Internal:
    // ----------------------

    pub(super) fn new(render_system: WeakMut<RenderSystem>,
                      initial_capacity: usize,
                      settings: TextureSettings) -> Self
    {
        log::info!(log::channel!("render"),
                   "Texture settings: Filter:{}, WrapMode:{}, Mipmaps:{}",
                   settings.filter, settings.wrap_mode, settings.mipmaps);

        Self {
            lookup: PreHashedKeyMap::default(),
            textures: Slab::with_capacity(initial_capacity),
            settings,
            dummy_texture_handle: TextureHandle::invalid(),
            white_texture_handle: TextureHandle::invalid(),
            render_system,
        }
    }

    pub(super) fn create_default_textures(&mut self) {
        debug_assert!(!self.dummy_texture_handle.is_valid());
        debug_assert!(!self.white_texture_handle.is_valid());

        self.dummy_texture_handle = self.create_filled_8x8_texture("dummy_texture", [255, 0,   255, 255]);
        self.white_texture_handle = self.create_filled_8x8_texture("white_texture", [255, 255, 255, 255]);
    }

    #[inline]
    pub(super) fn texture_for_handle(&mut self, handle: TextureHandle) -> &mut TextureBackendImpl {
        match handle {
            TextureHandle::Invalid => self.dummy_texture(),
            TextureHandle::White   => self.white_texture(),
            TextureHandle::Index(idx) => {
                // If we have an index, it should point to a valid slot.
                self.textures.get_mut(idx as usize)
                    .expect("Unexpected invalid TextureHandle::Index!")
            }
        }
    }

    #[inline]
    fn texture_at_index(&self, idx: u32) -> &TextureBackendImpl {
        &self.textures[idx as usize]
    }

    #[inline]
    fn texture_at_index_mut(&mut self, idx: u32) -> &mut TextureBackendImpl {
        &mut self.textures[idx as usize]
    }

    #[inline]
    fn dummy_texture(&mut self) -> &mut TextureBackendImpl {
        if let TextureHandle::Index(idx) = self.dummy_texture_handle {
            self.texture_at_index_mut(idx)
        } else {
            panic!("Unexpected dummy_texture_handle value!")
        }
    }

    #[inline]
    fn white_texture(&mut self) -> &mut TextureBackendImpl {
        if let TextureHandle::Index(idx) = self.white_texture_handle {
            self.texture_at_index_mut(idx)
        } else {
            panic!("Unexpected white_texture_handle value!")
        }
    }

    #[inline]
    fn register_new_texture(&mut self, texture: TextureBackendImpl) -> TextureHandle {
        let idx: u32 = self.textures.vacant_key().try_into().unwrap();

        if self.lookup.insert(texture.hash(), idx).is_some() {
            panic!("TextureCache key collision for: '{}' (0x{:X})", texture.name(), texture.hash());
        }

        self.textures.insert(texture);
        TextureHandle::Index(idx)
    }

    #[inline]
    fn render_sys_rc(&self) -> RcMut<RenderSystem> {
        self.render_system.upgrade().unwrap()
    }

    #[inline]
    fn is_valid(&self) -> bool {
        // Hash table length should match slab len.
        self.lookup.len() == self.textures.len()
    }

    fn load_image_file(file_path: PathRef) -> Result<RgbaImage, ()> {
        debug_assert!(!file_path.is_empty());

        let image = match file_sys::load_bytes(file_path) {
            Ok(bytes) => match image::load_from_memory(&bytes) {
                // Moves data, no pixel conversion if already RGBA8.
                Ok(image) => image.into_rgba8(),
                Err(err) => {
                    log::error!(log::channel!("render"), "Image decode error: {err}");
                    return Err(());
                }
            }
            Err(err) => {
                log::error!(log::channel!("render"), "Failed to load image file: {err}");
                return Err(());
            }
        };

        Ok(image)
    }

    fn create_filled_8x8_texture(&mut self, name: &str, rgba: [u8; 4]) -> TextureHandle {
        const SIZE: Size = Size::new(8, 8);
        const PIXEL_COUNT: usize = (SIZE.width * SIZE.height) as usize;

        let pixels: ArrayVec<u8, { PIXEL_COUNT * 4 }> =
            rgba.iter().copied().cycle().take(PIXEL_COUNT * 4).collect();

        self.new_initialized_texture(name, SIZE, &pixels, None)
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    // Lists all loaded textures.
    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        debug_assert!(self.is_valid());

        let ui = ui_sys.ui();

        if let Some(_tab_bar) = ui.tab_bar("Texture Cache Tab Bar") {
            if let Some(_tab) = ui.tab_item("Filtering") {
                self.draw_debug_ui_filtering(ui);
            }

            if let Some(_tab) = ui.tab_item("Loaded Textures") {
                self.draw_debug_ui_loaded_textures(ui);
            }
        }
    }

    fn draw_debug_ui_filtering(&mut self, ui: &imgui::Ui) {
        let current_settings = self.current_texture_settings();
        let mut settings_changed = false;

        let mut current_filter_index = current_settings.filter as usize;
        settings_changed |= ui.combo(
            "Filter",
            &mut current_filter_index,
            TextureFilter::VARIANTS,
            |filter| { filter.to_string().into() }
        );

        let mut current_wrap_mode_index = current_settings.wrap_mode as usize;
        settings_changed |= ui.combo(
            "Wrap Mode",
            &mut current_wrap_mode_index,
            TextureWrapMode::VARIANTS,
            |mode| { mode.to_string().into() }
        );

        let mut current_gen_mipmaps = current_settings.mipmaps;
        settings_changed |= ui.checkbox("Mipmaps", &mut current_gen_mipmaps);

        if settings_changed {
            let new_settings = TextureSettings {
                filter: TextureFilter::try_from_primitive(current_filter_index as u32).unwrap(),
                wrap_mode: TextureWrapMode::try_from_primitive(current_wrap_mode_index as u32).unwrap(),
                mipmaps: current_gen_mipmaps,
            };
            self.change_texture_settings(new_settings);
        }
    }

    fn draw_debug_ui_loaded_textures(&self, ui: &imgui::Ui) {
        let table_col = |label: &str| {
            ui.text(label);
            ui.next_column();
        };

        let bool_str = |val: bool| {
            if val { "yes" } else { "no" }
        };

        ui.text(format_fixed_string!(64, "Loaded Count: {}", self.textures.len()));
        ui.separator();

        // Set number of rows (emulated with columns):
        ui.columns(7, "texture_columns", true);

        // Header row:
        table_col("Index");
        table_col("Name");
        table_col("Size");
        table_col("Change Settings");
        table_col("Mipmaps");
        table_col("Filter");
        table_col("Wrap Mode");

        ui.separator();

        for (index, texture) in &self.textures {
            table_col(&format_fixed_string!(64, "{}", index));
            table_col(texture.name());
            table_col(&format_fixed_string!(64, "{}x{}", texture.size().width, texture.size().height));
            table_col(bool_str(texture.allow_settings_change()));
            table_col(bool_str(texture.has_mipmaps()));
            table_col(&format_fixed_string!(64, "{}", texture.filter()));
            table_col(&format_fixed_string!(64, "{}", texture.wrap_mode()));
        }

        // Return to single column.
        ui.columns(1, "", false);
    }
}
