use batch::*;
use pipeline::*;
use texture::*;
use vertex::*;
use target::*;

pub use texture::WgpuTexture;

use super::{
    RenderApi,
    RenderStats,
    RenderSystemBackend,
    RenderSystemInitParams,
};
use crate::{
    log,
    ui::UiRenderFrameBundle,
    utils::{Vec2, Size, Color, Rect, RectTexCoords, time::PerfTimer},
};

pub mod batch;
pub mod pipeline;
pub mod target;
pub mod texture;
pub mod vertex;

// ----------------------------------------------
// WgpuRenderSystemBackend
// ----------------------------------------------

pub struct WgpuRenderSystemBackend {

}

impl WgpuRenderSystemBackend {
    pub fn new() -> Self {
        Self {} // TODO
    }
}

impl RenderSystemBackend for WgpuRenderSystemBackend {
    // ----------------------
    // Initialization:
    // ----------------------

    fn initialize(&mut self, params: &RenderSystemInitParams, tex_cache: &mut super::texture::TextureCache) {
        debug_assert!(params.render_api == RenderApi::Wgpu);

        // TODO
    }

    // ----------------------
    // Begin/End frame:
    // ----------------------

    fn begin_frame(&mut self,
                   viewport_size: Size,
                   framebuffer_size: Size)
    {
        // TODO
    }

    fn end_frame(&mut self,
                 ui_frame_bundle: &mut UiRenderFrameBundle,
                 tex_cache: &mut super::texture::TextureCache)
                 -> RenderStats
    {
        // TODO
        RenderStats::default()
    }

    // ----------------------
    // Viewport/Framebuffer:
    // ----------------------

    fn viewport(&self) -> Rect {
        // TODO
        Rect::default()
    }

    fn set_viewport_size(&mut self, new_size: Size) {
        // TODO
    }

    fn set_framebuffer_size(&mut self, new_size: Size) {
        // TODO
    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    fn begin_ui_render(&mut self) {
        // TODO
    }

    fn end_ui_render(&mut self) {
        // TODO
    }

    fn set_ui_draw_buffers(&mut self,
                           vtx_buffer: &[super::UiDrawVertex],
                           idx_buffer: &[super::UiDrawIndex])
    {
        // TODO
    }

    fn draw_ui_elements(&mut self,
                        first_index: u32,
                        index_count: u32,
                        texture: super::texture::TextureHandle,
                        tex_cache: &mut super::texture::TextureCache,
                        clip_rect: Rect)
    {
        // TODO
    }

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[super::DrawIndex],
                                      color: Color)
    {
        // TODO
    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: super::texture::TextureHandle,
                                  color: Color)
    {
        // TODO
    }

    // ----------------------
    // Debug drawing:
    // ----------------------

    fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        // TODO
    }

    fn draw_point(&mut self, pt: Vec2, color: Color, size: f32) {
        // TODO
    }

    // ----------------------
    // Texture Allocation:
    // ----------------------

    fn new_texture_from_pixels(&mut self,
                               name: &str,
                               size: Size,
                               pixels: &[u8],
                               settings: super::texture::TextureSettings,
                               allow_settings_change: bool) -> super::texture::TextureBackendImpl
    {
        // TODO
        todo!()
    }

    fn update_texture_pixels(&mut self,
                             texture: &mut super::texture::TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8])
    {
        // TODO
    }

    fn update_texture_settings(&mut self,
                               texture: &mut super::texture::TextureBackendImpl,
                               settings: super::texture::TextureSettings)
    {
        // TODO
    }

    fn release_texture(&mut self, texture: &mut super::texture::TextureBackendImpl) {
        // TODO
    }
}
