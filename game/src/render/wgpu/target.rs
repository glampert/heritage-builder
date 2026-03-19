use crate::utils::Size;

// ----------------------------------------------
// OffscreenTarget
// ----------------------------------------------

// A wgpu texture used as an offscreen render target.
// The rendered contents are blitted to the screen surface via a blit pass.
pub struct OffscreenTarget {
    pub texture:    wgpu::Texture,
    pub view:       wgpu::TextureView,
    pub sampler:    wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub size:       Size,
}

impl OffscreenTarget {
    pub fn new(
        device: &wgpu::Device,
        size: Size,
        format: wgpu::TextureFormat,
        blit_texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        debug_assert!(size.is_valid());

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_rt"),
            size: wgpu::Extent3d {
                width:  size.width as u32,
                height: size.height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                 | wgpu::TextureUsages::TEXTURE_BINDING,
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

        Self { texture, view, sampler, bind_group, size }
    }
}
