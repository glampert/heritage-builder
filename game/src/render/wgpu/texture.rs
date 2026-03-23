use std::any::Any;
use strum::VariantArray;
use image::GenericImageView;
use slab::Slab;

use crate::{
    log,
    render::{self, NativeTextureHandle, TextureHandle},
    ui::UiSystem,
    utils::{Size, hash::{self, PreHashedKeyMap, StringHash}},
    file_sys::paths::PathRef,
};

// ----------------------------------------------
// WgpuTexture
// ----------------------------------------------

pub struct WgpuTexture {
    pub texture:    wgpu::Texture,
    pub view:       wgpu::TextureView,
    pub sampler:    wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub size:       Size,
    pub has_mipmaps: bool,
    pub name:       String,
    settings:       WgpuTextureSettings,
}

impl WgpuTexture {
    pub fn hash(&self) -> StringHash {
        hash::fnv1a_from_str(&self.name)
    }
}

// ----------------------------------------------
// WgpuTextureSettings
// ----------------------------------------------

#[derive(Copy, Clone)]
struct WgpuTextureSettings {
    filter:      render::TextureFilter,
    wrap_mode:   render::TextureWrapMode,
    gen_mipmaps: bool,
}

impl From<render::TextureSettings> for WgpuTextureSettings {
    fn from(s: render::TextureSettings) -> Self {
        Self {
            filter:      s.filter,
            wrap_mode:   s.wrap_mode,
            gen_mipmaps: s.gen_mipmaps,
        }
    }
}

fn to_wgpu_filter_mode(filter: render::TextureFilter) -> (wgpu::FilterMode, wgpu::FilterMode, wgpu::MipmapFilterMode) {
    // Returns (min_filter, mag_filter, mipmap_filter).
    match filter {
        render::TextureFilter::Nearest              => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
        render::TextureFilter::Linear               => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Nearest),
        render::TextureFilter::NearestMipmapNearest => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
        render::TextureFilter::LinearMipmapNearest  => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Nearest),
        render::TextureFilter::NearestMipmapLinear  => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Linear),
        render::TextureFilter::LinearMipmapLinear   => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Linear),
    }
}

fn to_wgpu_address_mode(wrap: render::TextureWrapMode, device: &wgpu::Device) -> wgpu::AddressMode {
    match wrap {
        render::TextureWrapMode::Repeat        => wgpu::AddressMode::Repeat,
        render::TextureWrapMode::ClampToEdge   => wgpu::AddressMode::ClampToEdge,
        render::TextureWrapMode::ClampToBorder => {
            if device.features().contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER) {
                wgpu::AddressMode::ClampToBorder
            } else {
                wgpu::AddressMode::ClampToEdge
            }
        }
    }
}

fn create_sampler(device: &wgpu::Device, settings: WgpuTextureSettings) -> wgpu::Sampler {
    let (min, mag, mip) = to_wgpu_filter_mode(settings.filter);
    let address = to_wgpu_address_mode(settings.wrap_mode, device);
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: mag,
        min_filter: min,
        mipmap_filter: mip,
        ..Default::default()
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: Option<&str>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label,
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

struct TexCacheEntry {
    texture: WgpuTexture,
    allow_settings_change: bool,
}

pub struct TextureCache {
    lookup:   PreHashedKeyMap<StringHash, usize>,
    textures: Slab<TexCacheEntry>,
    settings: render::TextureSettings,

    device: wgpu::Device,
    queue:  wgpu::Queue,

    // Shared bind group layout for texture+sampler pairs.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,

    dummy_texture_handle: TextureHandle,
    white_texture_handle: TextureHandle,
}

impl TextureCache {
    pub fn new(
        initial_capacity: usize,
        settings: render::TextureSettings,
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_bind_group_layout: wgpu::BindGroupLayout,
    ) -> Self {
        log::info!(log::channel!("render"),
            "Texture settings: filter:{}, wrap:{}, mipmaps:{}",
            settings.filter, settings.wrap_mode, settings.gen_mipmaps);

        let mut cache = Self {
            lookup: PreHashedKeyMap::default(),
            textures: Slab::with_capacity(initial_capacity),
            settings,
            device,
            queue,
            texture_bind_group_layout,
            dummy_texture_handle: TextureHandle::invalid(),
            white_texture_handle: TextureHandle::invalid(),
        };

        cache.dummy_texture_handle =
            cache.create_color_filled_8x8_texture("dummy_texture", [255, 0, 255, 255]);
        cache.white_texture_handle =
            cache.create_color_filled_8x8_texture("white_texture", [255, 255, 255, 255]);

        cache
    }

    #[inline]
    pub fn handle_to_wgpu_texture(&self, handle: TextureHandle) -> &WgpuTexture {
        match handle {
            TextureHandle::Invalid => self.dummy_texture(),
            TextureHandle::White   => self.white_texture(),
            TextureHandle::Index(idx) => {
                self.textures.get(idx as usize)
                    .map(|e| &e.texture)
                    .unwrap_or_else(|| self.dummy_texture())
            }
        }
    }

    #[inline]
    pub fn dummy_texture(&self) -> &WgpuTexture {
        match self.dummy_texture_handle {
            TextureHandle::Index(idx) => &self.textures[idx as usize].texture,
            _ => panic!("Unexpected dummy_texture_handle value!"),
        }
    }

    #[inline]
    pub fn white_texture(&self) -> &WgpuTexture {
        match self.white_texture_handle {
            TextureHandle::Index(idx) => &self.textures[idx as usize].texture,
            _ => panic!("Unexpected white_texture_handle value!"),
        }
    }

    // ----------------------
    // Internals:
    // ----------------------

    fn add_texture_internal(&mut self, texture: WgpuTexture, allow_settings_change: bool) -> TextureHandle {
        debug_assert!(self.textures.len() == self.lookup.len());

        let hash = texture.hash();
        let index = self.textures.insert(TexCacheEntry { texture, allow_settings_change });

        if let Some(existing) = self.lookup.insert(hash, index) {
            let name = &self.textures[existing].texture.name;
            panic!("Texture '{name}' ({hash:X}) already in TextureCache at [{existing}].");
        }

        TextureHandle::Index(index as u32)
    }

    fn remove_texture_internal(&mut self, index: usize) {
        debug_assert!(self.textures.len() == self.lookup.len());

        if let Some(entry) = self.textures.try_remove(index) {
            if self.lookup.remove(&entry.texture.hash()).is_none() {
                panic!("Failed to remove TextureCache entry for '{}'!", entry.texture.name);
            }
        }
    }

    fn find_texture_internal(&self, name: &str) -> TextureHandle {
        debug_assert!(self.textures.len() == self.lookup.len());

        if let Some(&index) = self.lookup.get(&hash::fnv1a_from_str(name)) {
            debug_assert!(self.textures.get(index).is_some());
            TextureHandle::Index(index as u32)
        } else {
            TextureHandle::Invalid
        }
    }

    fn create_wgpu_texture(
        &self,
        name: &str,
        size: Size,
        pixels: Option<&[u8]>,
        settings: WgpuTextureSettings,
    ) -> WgpuTexture {
        let wgpu_size = wgpu::Extent3d {
            width:  size.width as u32,
            height: size.height as u32,
            depth_or_array_layers: 1,
        };

        let mip_level_count = if settings.gen_mipmaps {
            (size.width.max(size.height) as f32).log2().floor() as u32 + 1
        } else {
            1
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name),
            size: wgpu_size,
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                 | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        if let Some(pixels) = pixels {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * size.width as u32),
                    rows_per_image: Some(size.height as u32),
                },
                wgpu_size,
            );
        }

        // TODO: generate mipmaps if settings.gen_mipmaps (requires compute or blit passes).

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = create_sampler(&self.device, settings);
        let bind_group = create_bind_group(
            &self.device,
            &self.texture_bind_group_layout,
            &view,
            &sampler,
            Some(name),
        );

        WgpuTexture {
            texture,
            view,
            sampler,
            bind_group,
            size,
            has_mipmaps: settings.gen_mipmaps && mip_level_count > 1,
            name: name.to_string(),
            settings,
        }
    }

    fn new_texture_with_data_internal(
        &mut self,
        name: &str,
        size: Size,
        pixels: Option<&[u8]>,
        settings: Option<render::TextureSettings>,
    ) -> TextureHandle {
        debug_assert!(!name.is_empty());
        debug_assert!(size.is_valid());

        if self.find_texture_internal(name).is_valid() {
            panic!("TextureCache: A texture with name '{name}' already exists!");
        }

        let allow_settings_change = settings.is_none();
        let wgpu_settings = WgpuTextureSettings::from(settings.unwrap_or(self.settings));

        let texture = self.create_wgpu_texture(name, size, pixels, wgpu_settings);
        self.add_texture_internal(texture, allow_settings_change)
    }

    fn load_texture_internal(
        &mut self,
        file_path: PathRef,
        settings: Option<render::TextureSettings>,
    ) -> TextureHandle {
        let loaded = self.find_texture_internal(file_path.as_str());
        if loaded.is_valid() {
            return loaded;
        }

        let image = match crate::file_sys::load_bytes(file_path) {
            Ok(bytes) => match image::load_from_memory(&bytes) {
                Ok(img) => img,
                Err(err) => {
                    log::error!(log::channel!("render"), "TextureCache Decode Error for '{}': {err}", file_path.as_str());
                    return self.dummy_texture_handle;
                }
            },
            Err(err) => {
                log::error!(log::channel!("render"), "TextureCache Load Error: {err}");
                return self.dummy_texture_handle;
            }
        };

        let rgba = image.to_rgba8();
        let (w, h) = image.dimensions();
        let size = Size::new(w as i32, h as i32);
        let pixels = rgba.as_raw();

        let allow_settings_change = settings.is_none();
        let wgpu_settings = WgpuTextureSettings::from(settings.unwrap_or(self.settings));

        let texture = self.create_wgpu_texture(file_path.as_str(), size, Some(pixels), wgpu_settings);
        self.add_texture_internal(texture, allow_settings_change)
    }

    fn create_color_filled_8x8_texture(&mut self, name: &str, color: [u8; 4]) -> TextureHandle {
        const SIZE: Size = Size::new(8, 8);
        const PIXEL_COUNT: usize = (SIZE.width * SIZE.height) as usize;

        let pixels: Vec<u8> = color.iter().copied().cycle().take(PIXEL_COUNT * 4).collect();
        self.new_texture_with_data_internal(name, SIZE, Some(&pixels), None)
    }

    fn rebuild_texture_resources(&mut self, entry: &mut TexCacheEntry) {
        let settings = WgpuTextureSettings::from(render::TextureSettings {
            filter:      self.settings.filter,
            wrap_mode:   self.settings.wrap_mode,
            gen_mipmaps: self.settings.gen_mipmaps,
        });

        entry.texture.sampler = create_sampler(&self.device, settings);
        entry.texture.bind_group = create_bind_group(
            &self.device,
            &self.texture_bind_group_layout,
            &entry.texture.view,
            &entry.texture.sampler,
            Some(&entry.texture.name),
        );
        entry.texture.settings = settings;
    }
}

impl render::TextureCache for TextureCache {
    fn as_any(&self) -> &dyn Any { self }

    fn to_native_handle(&self, handle: TextureHandle) -> NativeTextureHandle {
        // For wgpu we don't have a single integer handle.
        // Return a sentinel; this is only used by the OpenGL RenderTarget path.
        let _ = handle;
        NativeTextureHandle { bits: 0 }
    }

    fn find_loaded_texture(&self, name_or_file_path: &str) -> Option<TextureHandle> {
        let handle = self.find_texture_internal(name_or_file_path);
        if handle.is_valid() { Some(handle) } else { None }
    }

    fn load_texture(&mut self, file_path: PathRef) -> TextureHandle {
        self.load_texture_internal(file_path, None)
    }

    fn load_texture_with_settings(
        &mut self,
        file_path: PathRef,
        settings: Option<render::TextureSettings>,
    ) -> TextureHandle {
        self.load_texture_internal(file_path, settings)
    }

    fn change_texture_settings(&mut self, settings: render::TextureSettings) {
        log::info!(log::channel!("render"),
            "Changing texture settings: filter:{}, wrap:{}, mipmaps:{}",
            settings.filter, settings.wrap_mode, settings.gen_mipmaps);

        self.settings = settings;

        // Collect indices first to avoid borrow conflict.
        let indices: Vec<usize> = self.textures.iter()
            .filter(|(_, e)| e.allow_settings_change)
            .map(|(i, _)| i)
            .collect();

        for index in indices {
            let entry = &mut self.textures[index];
            // Inline the rebuild to avoid borrowing self while entry is borrowed.
            let wgpu_settings = WgpuTextureSettings::from(settings);
            entry.texture.sampler = create_sampler(&self.device, wgpu_settings);
            entry.texture.bind_group = create_bind_group(
                &self.device,
                &self.texture_bind_group_layout,
                &entry.texture.view,
                &entry.texture.sampler,
                Some(&entry.texture.name),
            );
            entry.texture.settings = wgpu_settings;
        }
    }

    fn current_texture_settings(&self) -> render::TextureSettings { self.settings }

    fn new_uninitialized_texture(
        &mut self,
        name: &str,
        size: Size,
        settings: Option<render::TextureSettings>,
    ) -> TextureHandle {
        self.new_texture_with_data_internal(name, size, None, settings)
    }

    fn new_initialized_texture(
        &mut self,
        name: &str,
        size: Size,
        pixels: &[u8],
        settings: Option<render::TextureSettings>,
    ) -> TextureHandle {
        self.new_texture_with_data_internal(name, size, Some(pixels), settings)
    }

    fn update_texture(
        &mut self,
        handle: TextureHandle,
        offset_x: u32,
        offset_y: u32,
        size: Size,
        mip_level: u32,
        pixels: &[u8],
    ) {
        debug_assert!(size.is_valid());
        debug_assert!(pixels.len() >= (size.width * size.height * 4) as usize);
        debug_assert!(!matches!(handle, TextureHandle::Invalid | TextureHandle::White));

        let tex = &self.handle_to_wgpu_texture(handle).texture;

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level,
                origin: wgpu::Origin3d { x: offset_x, y: offset_y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width as u32),
                rows_per_image: Some(size.height as u32),
            },
            wgpu::Extent3d {
                width:  size.width as u32,
                height: size.height as u32,
                depth_or_array_layers: 1,
            },
        );
    }

    fn release_texture(&mut self, handle: &mut TextureHandle) {
        if let TextureHandle::Index(idx) = handle {
            self.remove_texture_internal(*idx as usize);
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
                    |v| v.to_string().into())
                {
                    settings_changed = true;
                }

                let mut current_wrap_mode_index = current_settings.wrap_mode as usize;
                if ui.combo("Wrap Mode",
                    &mut current_wrap_mode_index,
                    render::TextureWrapMode::VARIANTS,
                    |v| v.to_string().into())
                {
                    settings_changed = true;
                }

                let mut gen_mipmaps = current_settings.gen_mipmaps;
                if ui.checkbox("Mipmaps", &mut gen_mipmaps) {
                    settings_changed = true;
                }

                if settings_changed {
                    use num_enum::TryFromPrimitive;
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

                let bool_str = |val: bool| if val { "yes" } else { "no" };

                ui.text(format!("Loaded Count: {}", self.textures.len()));
                ui.separator();

                ui.columns(6, "texture_columns", true);

                table_col("Index");
                table_col("Name");
                table_col("Size");
                table_col("Change Settings");
                table_col("Mipmaps");
                table_col("Filter");

                ui.separator();

                for (index, entry) in &self.textures {
                    table_col(&format!("{index}"));
                    table_col(&entry.texture.name);
                    table_col(&format!("{}x{}", entry.texture.size.width, entry.texture.size.height));
                    table_col(bool_str(entry.allow_settings_change));
                    table_col(bool_str(entry.texture.has_mipmaps));
                    table_col(&format!("{}", entry.texture.settings.filter));
                }

                ui.columns(1, "", false);
            }
        }
    }
}
