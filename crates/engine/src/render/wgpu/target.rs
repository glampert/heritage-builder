use common::Size;

// ----------------------------------------------
// RenderTarget
// ----------------------------------------------

// A Wgpu texture used as an offscreen render target.
// The rendered contents are blitted to the screen surface via a blit pass.
pub struct RenderTarget {
    size: Size,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
}

impl RenderTarget {
    pub fn new(
        device: &wgpu::Device,
        size: Size,
        format: wgpu::TextureFormat,
        blit_texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        debug_assert!(size.is_valid());

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_render_target"),
            size: wgpu::Extent3d { width: size.width as u32, height: size.height as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("offscreen_rt_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("offscreen_rt_bind_group"),
            layout: blit_texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Self { size, texture, view, sampler, bind_group }
    }

    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }

    #[inline]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn needs_resize(&self, required_size: Size) -> bool {
        required_size.width > self.size.width || required_size.height > self.size.height
    }
}
