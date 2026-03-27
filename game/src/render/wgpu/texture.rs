use crate::{
    render,
    utils::{Size, hash::{self, StringHash}},
};

// ----------------------------------------------
// WgpuTexture
// ----------------------------------------------

// Texture type referenced by the frontend TextureCache.
pub struct WgpuTexture {
    pub name: String,
    pub size: Size,
    pub settings: render::texture::TextureSettings,
    pub allow_settings_change: bool,

    // Wgpu state:
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
}

// Methods exposed to the renderer frontend.
impl render::texture::Texture for WgpuTexture {
    #[inline]
    fn is_valid(&self) -> bool {
        self.size.is_valid()
    }

    #[inline]
    fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    fn hash(&self) -> StringHash {
        hash::fnv1a_from_str(&self.name)
    }

    #[inline]
    fn size(&self) -> Size {
        self.size
    }

    #[inline]
    fn has_mipmaps(&self) -> bool {
        self.settings.mipmaps
    }

    #[inline]
    fn filter(&self) -> render::texture::TextureFilter {
        self.settings.filter
    }

    #[inline]
    fn wrap_mode(&self) -> render::texture::TextureWrapMode {
        self.settings.wrap_mode
    }

    #[inline]
    fn allow_settings_change(&self) -> bool {
        self.allow_settings_change
    }
}

// ----------------------------------------------
// Texture creation
// ----------------------------------------------

pub struct TextureCreationParams<'a> {
    pub name: &'a str,
    pub size: Size,
    pub pixels: &'a [u8], // Can be empty.
    pub settings: render::texture::TextureSettings,
    pub allow_settings_change: bool,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub texture_bind_group_layout: &'a wgpu::BindGroupLayout,
}

impl WgpuTexture {
    // Creates a new WgpuTexture with optional initial pixel data.
    pub fn new(params: &TextureCreationParams) -> Self {
        let wgpu_size = wgpu::Extent3d {
            width:  params.size.width  as u32,
            height: params.size.height as u32,
            depth_or_array_layers: 1,
        };

        let mip_level_count = if params.settings.mipmaps {
            (params.size.width.max(params.size.height) as f32).log2().floor() as u32 + 1
        } else {
            1
        };

        let texture = params.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(params.name),
            size: wgpu_size,
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        if !params.pixels.is_empty() {
            params.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                params.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * params.size.width as u32), // Rgba8
                    rows_per_image: Some(params.size.height as u32),
                },
                wgpu_size,
            );
        }

        // TODO: Generate mipmaps if settings.mipmaps (requires compute or blit passes).

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = create_sampler(params.device, params.settings);
        let bind_group = create_bind_group(
            params.device,
            params.texture_bind_group_layout,
            &view,
            &sampler,
            Some(params.name)
        );

        Self {
            name: params.name.to_string(),
            size: params.size,
            settings: params.settings,
            allow_settings_change: params.allow_settings_change,
            texture,
            view,
            sampler,
            bind_group,
        }
    }

    // Rebuilds the sampler and bind group after a texture settings change.
    pub fn rebuild_sampler_and_bind_group(&mut self,
                                          device: &wgpu::Device,
                                          texture_bind_group_layout: &wgpu::BindGroupLayout,
                                          settings: render::texture::TextureSettings)
    {
        self.settings   = settings;
        self.sampler    = create_sampler(device, settings);
        self.bind_group = create_bind_group(
            device,
            texture_bind_group_layout,
            &self.view,
            &self.sampler,
            Some(&self.name)
        );
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn to_wgpu_filter_mode(filter: render::texture::TextureFilter) -> (wgpu::FilterMode, wgpu::FilterMode, wgpu::MipmapFilterMode) {
    // Returns (min_filter, mag_filter, mipmap_filter).
    match filter {
        render::texture::TextureFilter::Nearest              => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
        render::texture::TextureFilter::Linear               => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Nearest),
        render::texture::TextureFilter::NearestMipmapNearest => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
        render::texture::TextureFilter::LinearMipmapNearest  => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Nearest),
        render::texture::TextureFilter::NearestMipmapLinear  => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Linear),
        render::texture::TextureFilter::LinearMipmapLinear   => (wgpu::FilterMode::Linear,  wgpu::FilterMode::Linear,  wgpu::MipmapFilterMode::Linear),
    }
}

fn to_wgpu_address_mode(wrap: render::texture::TextureWrapMode, device: &wgpu::Device) -> wgpu::AddressMode {
    match wrap {
        render::texture::TextureWrapMode::Repeat        => wgpu::AddressMode::Repeat,
        render::texture::TextureWrapMode::ClampToEdge   => wgpu::AddressMode::ClampToEdge,
        render::texture::TextureWrapMode::ClampToBorder => {
            if device.features().contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER) {
                wgpu::AddressMode::ClampToBorder
            } else {
                wgpu::AddressMode::ClampToEdge
            }
        }
    }
}

fn create_sampler(device: &wgpu::Device, settings: render::texture::TextureSettings) -> wgpu::Sampler {
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

fn create_bind_group(device: &wgpu::Device,
                     layout: &wgpu::BindGroupLayout,
                     view: &wgpu::TextureView,
                     sampler: &wgpu::Sampler,
                     label: Option<&str>) -> wgpu::BindGroup
{
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label,
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler)  },
        ],
    })
}
