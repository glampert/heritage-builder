pub mod batch;
pub mod pipeline;
pub mod system;
pub mod target;
pub mod texture;
pub mod vertex;

// Pre-initialized wgpu resources for WASM.
// On WASM, adapter/device creation is async and must happen before
// the RenderSystem is constructed. These resources are passed through
// the Application's app_context().
pub struct WgpuInitResources {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
}
