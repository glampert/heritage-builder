use super::{RenderSystemBackend, texture::Texture};

pub mod batch;
pub mod pipeline;
pub mod system;
pub mod target;
pub mod texture;
pub mod vertex;

// ----------------------------------------------
// WgpuInitResources
// ----------------------------------------------

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

// ----------------------------------------------
// WgpuRenderSystemBackend
// ----------------------------------------------

pub struct WgpuRenderSystemBackend {

}

impl RenderSystemBackend for WgpuRenderSystemBackend {
    // ----------------------
    // Begin/End frame:
    // ----------------------

    fn begin_frame(&mut self, viewport_size: Size, framebuffer_size: Size) {

    }

    fn end_frame(&mut self, ui_frame_bundle: &mut UiRenderFrameBundle) -> RenderStats {

    }

    // ----------------------
    // Viewport/Framebuffer:
    // ----------------------

    fn viewport(&self) -> Rect {

    }

    fn set_viewport_size(&mut self, new_size: Size) {

    }

    fn set_framebuffer_size(&mut self, new_size: Size) {

    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    fn begin_ui_render(&mut self) {

    }

    fn end_ui_render(&mut self) {

    }

    fn set_ui_draw_buffers(&mut self, vtx_buffer: &[imgui::DrawVert], idx_buffer: &[imgui::DrawIdx]) {

    }

    fn draw_ui_elements(&mut self, first_index: u32, index_count: u32, texture: TextureHandle, clip_rect: Rect) {

    }

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[u16],
                                      color: Color)
    {

    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: TextureHandle,
                                  color: Color)
    {

    }

    // ----------------------
    // Debug drawing:
    // ----------------------

    fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {

    }

    fn draw_point(&mut self, pt: Vec2, color: Color, size: f32) {

    }

    // ----------------------
    // Texture Allocation:
    // ----------------------

    fn new_texture_from_pixels(&mut self,
                               name: &str,
                               size: Size,
                               pixels: &[u8],
                               settings: TextureSettings,
                               allow_settings_change: bool) -> TextureBackendImpl
    {
    }

    fn update_texture_pixels(&mut self,
                             texture: &mut TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8])
    {
    }

    fn update_texture_settings(&mut self,
                               texture: &mut TextureBackendImpl,
                               settings: TextureSettings)
    {
    }

    fn release_texture(&mut self, texture: &mut TextureBackendImpl) {
    }
}

// ----------------------------------------------
// WgpuTexture
// ----------------------------------------------

pub struct WgpuTexture {

}

impl Texture for WgpuTexture {
    fn is_valid(&self) -> bool {

    }

    fn name(&self) -> &str {

    }

    fn hash(&self) -> StringHash {

    }

    fn size(&self) -> Size {

    }

    fn has_mipmaps(&self) -> bool {

    }

    fn filter(&self) -> TextureFilter {

    }

    fn wrap_mode(&self) -> TextureWrapMode {

    }

    fn allow_settings_change(&self) -> bool {

    }
}
