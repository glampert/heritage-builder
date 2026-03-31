use std::sync::Arc;
use arrayvec::ArrayVec;

use batch::*;
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
    render,
    ui::UiRenderFrameBundle,
    utils::{Vec2, Size, Color, Rect, RectTexCoords, time::PerfTimer},
};

mod batch;
mod pipeline;
mod target;
mod texture;
mod vertex;

// ----------------------------------------------
// ShaderUniforms (struct Uniforms in shaders)
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ShaderUniforms {
    viewport_size: [f32; 2],
    _padding: [f32; 2], // Align to 16 bytes for uniform buffer.
}

// ----------------------------------------------
// WgpuSystemState
// ----------------------------------------------

// All Wgpu resources, created during initialize().
struct WgpuSystemState {
    // Core wgpu state.
    device:  wgpu::Device,
    queue:   wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    surface_format: wgpu::TextureFormat,

    // Render pipelines.
    sprites_pipeline: wgpu::RenderPipeline,
    lines_pipeline:   wgpu::RenderPipeline,
    points_pipeline:  wgpu::RenderPipeline,
    ui_pipeline:      wgpu::RenderPipeline,
    blit_pipeline:    wgpu::RenderPipeline,

    // Shared bind group layouts.
    texture_bind_group_layout: wgpu::BindGroupLayout,
    blit_texture_layout:       wgpu::BindGroupLayout,

    // Per-frame uniform buffer + bind group.
    uniform_buffer:     wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    // CPU-side batches.
    sprites_batch: DrawBatch<SpriteVertex2D, SpriteIndex2D>,
    lines_batch:   DrawBatch<LineVertex2D, LineIndex2D>,
    points_batch:  DrawBatch<LineVertex2D, render::DrawIndex>,
    ui_batch:      UiDrawBatch,

    // GPU-side buffers.
    sprites_gpu: GpuVertexIndexBuffers,
    lines_gpu:   GpuVertexIndexBuffers,
    points_gpu:  GpuVertexIndexBuffers,
    ui_gpu:      GpuVertexIndexBuffers,

    // Offscreen render target.
    offscreen_render_target: RenderTarget,

    // UI draw commands recorded during the frame.
    ui_draw_commands: Vec<UiDrawCommand>,
    ui_base_vertex:   i32,
    ui_index_offset:  u32,

    // Frame state.
    frame_started:    bool,
    viewport:         Rect,
    framebuffer_size: Size,
    clear_color:      Color,
    stats:            RenderStats,
}

impl WgpuSystemState {
    fn reconfigure_surface(&mut self) {
        self.surface_config.width  = self.framebuffer_size.width  as u32;
        self.surface_config.height = self.framebuffer_size.height as u32;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn set_viewport_size(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());
        self.viewport = Rect::from_pos_and_size(Vec2::zero(), new_size.to_vec2());
    }

    fn set_framebuffer_size(&mut self, new_size: Size) {
        debug_assert!(new_size.is_valid());

        if self.framebuffer_size != new_size {
            self.framebuffer_size = new_size;
            self.reconfigure_surface();

            // Resize offscreen RT if the new framebuffer exceeds it.
            let vp = Size::new(self.viewport.width() as i32, self.viewport.height() as i32);
            let rt_size = vp.max(new_size);
            if self.offscreen_render_target.needs_resize(rt_size) {
                self.offscreen_render_target = RenderTarget::new(
                    &self.device, rt_size, self.surface_format, &self.blit_texture_layout);
            }
        }
    }
}

// ----------------------------------------------
// WgpuRenderSystemBackend
// ----------------------------------------------

pub struct WgpuRenderSystemBackend {
    state: Box<WgpuSystemState>,
}

impl WgpuRenderSystemBackend {
    // ----------------------
    // Initialization:
    // ----------------------

    pub fn new(params: &RenderSystemInitParams) -> Self {
        debug_assert!(params.render_api == RenderApi::Wgpu);
        debug_assert!(params.viewport_size.is_valid());
        debug_assert!(params.framebuffer_size.is_valid());

        log::info!(log::channel!("render"), "--- Render Backend: Wgpu ---");

        // Desktop: get window from app_context, create wgpu resources synchronously.
        let window: Arc<winit::window::Window> = params.app_context
            .expect("Wgpu backend requires an app_context!")
            .downcast_ref::<Arc<winit::window::Window>>()
            .expect("app_context must be Arc<winit::window::Window>!")
            .clone();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(wgpu::SurfaceTarget::from(window))
            .expect("Failed to create Wgpu surface!");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).expect("Failed to find a suitable GPU adapter!");

        log::info!(log::channel!("render"), "Wgpu Adapter Info:");
        {
            let info = adapter.get_info();
            log::info!(log::channel!("render"), " - Name: {}", info.name);
            log::info!(log::channel!("render"), " - Backend: {:?}", info.backend);
            log::info!(log::channel!("render"), " - Device Type: {:?}", info.device_type);

            if !info.driver.is_empty() || !info.driver_info.is_empty() {
                log::info!(log::channel!("render"), " - Driver: {} {}", info.driver, info.driver_info);
            }
        }

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("heritage_builder_device"),
                required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER,
                ..Default::default()
            },
        )).expect("Failed to create Wgpu device!");

        // Configure surface — use a non-sRGB format to match OpenGL behaviour.
        // The game does all color work in sRGB space with no linear-space lighting,
        // so we want raw byte pass-through (no automatic gamma conversion).
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage:  wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width:  params.framebuffer_size.width  as u32,
            height: params.framebuffer_size.height as u32,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Bind group layouts.
        let uniform_bind_group_layout = pipeline::create_uniform_bind_group_layout(&device);
        let texture_bind_group_layout = pipeline::create_texture_bind_group_layout(&device);
        let blit_texture_layout       = pipeline::create_texture_bind_group_layout(&device);

        // Pipelines.
        let sprites_pipeline = pipeline::create_sprites_pipeline(
            &device, surface_format, &uniform_bind_group_layout, &texture_bind_group_layout);
        let lines_pipeline = pipeline::create_lines_pipeline(
            &device, surface_format, &uniform_bind_group_layout);
        let points_pipeline = pipeline::create_points_pipeline(
            &device, surface_format, &uniform_bind_group_layout);
        let ui_pipeline = pipeline::create_ui_pipeline(
            &device, surface_format, &uniform_bind_group_layout, &texture_bind_group_layout);
        let blit_pipeline = pipeline::create_blit_pipeline(
            &device, surface_format, &blit_texture_layout);

        // Uniform buffer.
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shader_uniform_vars"),
            size: std::mem::size_of::<ShaderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader_uniform_vars_bind_group"),
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }
            ],
        });

        // Offscreen render target.
        let rt_size = params.viewport_size.max(params.framebuffer_size);
        let offscreen_render_target = RenderTarget::new(&device, rt_size, surface_format, &blit_texture_layout);

        // GPU buffers.
        let sprites_gpu = GpuVertexIndexBuffers::new(&device, "sprites",
            512 * std::mem::size_of::<SpriteVertex2D>(),
            512 * std::mem::size_of::<SpriteIndex2D>());
        let lines_gpu = GpuVertexIndexBuffers::new(&device, "lines",
            64 * std::mem::size_of::<LineVertex2D>(),
            64 * std::mem::size_of::<LineIndex2D>());
        let points_gpu = GpuVertexIndexBuffers::new(&device, "points",
            64 * std::mem::size_of::<LineVertex2D>(),
            64 * std::mem::size_of::<render::DrawIndex>());
        let ui_gpu = GpuVertexIndexBuffers::new(&device, "ui",
            1024 * std::mem::size_of::<UiVertex2D>(),
            1024 * std::mem::size_of::<render::UiDrawIndex>());

        log::info!(log::channel!("render"), "Wgpu initialized.");
        log::info!(log::channel!("render"), " - Surface format: {:?}", surface_format);
        log::info!(log::channel!("render"), " - Offscreen RT: {}", rt_size);
        log::info!(log::channel!("render"), " - Viewport: {}, Framebuffer: {}", params.viewport_size, params.framebuffer_size);

        let s = Box::new(WgpuSystemState {
            device,
            queue,
            surface,
            surface_config,
            surface_format,

            sprites_pipeline,
            lines_pipeline,
            points_pipeline,
            ui_pipeline,
            blit_pipeline,

            texture_bind_group_layout,
            blit_texture_layout,

            uniform_buffer,
            uniform_bind_group,

            sprites_batch: DrawBatch::new(512, 512, 512),
            lines_batch:   DrawBatch::new(64, 64, 0),
            points_batch:  DrawBatch::new(64, 64, 0),
            ui_batch:      UiDrawBatch::new(),

            sprites_gpu,
            lines_gpu,
            points_gpu,
            ui_gpu,

            offscreen_render_target,

            ui_draw_commands: Vec::with_capacity(64),
            ui_base_vertex:  0,
            ui_index_offset: 0,

            frame_started: false,
            viewport: Rect::from_pos_and_size(Vec2::zero(), params.viewport_size.to_vec2()),
            framebuffer_size: params.framebuffer_size,
            clear_color: params.clear_color,
            stats: RenderStats::default(),
        });

        Self { state: s }
    }

    #[inline]
    fn state(&self) -> &WgpuSystemState {
        &self.state
    }

    #[inline]
    fn state_mut(&mut self) -> &mut WgpuSystemState {
        &mut self.state
    }
}

impl RenderSystemBackend for WgpuRenderSystemBackend {
    // ----------------------
    // Begin/End frame:
    // ----------------------

    fn begin_frame(&mut self,
                   viewport_size: Size,
                   framebuffer_size: Size)
    {
        let s = self.state_mut();
        debug_assert!(!s.frame_started);

        s.set_viewport_size(viewport_size);
        s.set_framebuffer_size(framebuffer_size);

        s.frame_started = true;

        s.stats.triangles_drawn       = 0;
        s.stats.lines_drawn           = 0;
        s.stats.points_drawn          = 0;
        s.stats.texture_changes       = 0;
        s.stats.draw_calls            = 0;
        s.stats.render_submit_time_ms = 0.0;
    }

    fn end_frame(&mut self,
                 ui_frame_bundle: &mut UiRenderFrameBundle,
                 tex_cache: &mut super::texture::TextureCache)
                 -> RenderStats
    {
        let render_submit_timer = PerfTimer::begin();

        // Record UI draw data. This re-enters through RenderSystem to call
        // begin_ui_render, set_ui_draw_buffers, draw_ui_elements, end_ui_render.
        ui_frame_bundle.render();

        // All batches are now filled — do GPU work.
        let s = self.state_mut();
        debug_assert!(s.frame_started);
        debug_assert!(s.viewport.is_valid());
        debug_assert!(s.framebuffer_size.is_valid());

        // Upload viewport uniform.
        let uniforms = ShaderUniforms {
            viewport_size: [s.viewport.width(), s.viewport.height()],
            _padding: [0.0; 2],
        };
        s.queue.write_buffer(&s.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Upload batch data to GPU.
        s.sprites_gpu.upload(&s.device, &s.queue, s.sprites_batch.vertices(), s.sprites_batch.indices());
        s.lines_gpu.upload(&s.device, &s.queue, s.lines_batch.vertices(), s.lines_batch.indices());
        s.points_gpu.upload(&s.device, &s.queue, s.points_batch.vertices(), s.points_batch.indices());
        s.ui_gpu.upload_bytes(&s.device, &s.queue, s.ui_batch.vertex_bytes(), s.ui_batch.index_bytes());

        // Acquire surface texture.
        let output = match s.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                s.reconfigure_surface();
                s.surface.get_current_texture()
                    .expect("Failed to acquire surface texture after reconfigure!")
            }
            Err(e) => panic!("Failed to acquire surface texture: {e:?}"),
        };
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = s.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame_encoder"),
        });

        let cc = &s.clear_color;
        let clear_color = wgpu::Color {
            r: cc.r as f64, g: cc.g as f64, b: cc.b as f64, a: cc.a as f64,
        };

        // ---- Pass 1: Render world to offscreen RT ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: s.offscreen_render_target.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // Draw sprites.
            if !s.sprites_batch.is_empty() {
                pass.set_pipeline(&s.sprites_pipeline);
                pass.set_bind_group(0, Some(&s.uniform_bind_group), &[]);
                s.sprites_gpu.bind_to_render_pass(&mut pass, wgpu::IndexFormat::Uint16);

                let mut last_texture = super::texture::TextureHandle::invalid();
                for entry in s.sprites_batch.entries() {
                    if entry.texture != last_texture {
                        last_texture = entry.texture;
                        let bg = tex_cache.texture_for_handle(entry.texture).as_wgpu().bind_group();
                        pass.set_bind_group(1, Some(bg), &[]);
                        s.stats.texture_changes += 1;
                    }
                    pass.draw_indexed(
                        entry.first_index..entry.first_index + entry.index_count, 0, 0..1);
                    s.stats.draw_calls += 1;
                }
            }

            // Draw lines.
            if !s.lines_batch.is_empty() {
                pass.set_pipeline(&s.lines_pipeline);
                pass.set_bind_group(0, Some(&s.uniform_bind_group), &[]);
                s.lines_gpu.bind_to_render_pass(&mut pass, wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..s.lines_batch.indices().len() as u32, 0, 0..1);
                s.stats.draw_calls += 1;
            }

            // Draw points (expanded to quads on CPU).
            if !s.points_batch.is_empty() {
                pass.set_pipeline(&s.points_pipeline);
                pass.set_bind_group(0, Some(&s.uniform_bind_group), &[]);
                s.points_gpu.bind_to_render_pass(&mut pass, wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..s.points_batch.indices().len() as u32, 0, 0..1);
                s.stats.draw_calls += 1;
            }
        }

        // ---- Pass 2: Blit offscreen RT to surface ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            pass.set_pipeline(&s.blit_pipeline);
            pass.set_bind_group(0, Some(s.offscreen_render_target.bind_group()), &[]);
            pass.draw(0..3, 0..1); // Fullscreen triangle, no vertex buffer.
            s.stats.draw_calls += 1;
        }

        // ---- Pass 3: UI on top of surface ----
        if !s.ui_draw_commands.is_empty() && !s.ui_batch.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve blit result.
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            pass.set_pipeline(&s.ui_pipeline);
            pass.set_bind_group(0, Some(&s.uniform_bind_group), &[]);

            let idx_format = match std::mem::size_of::<super::UiDrawIndex>() {
                2 => wgpu::IndexFormat::Uint16,
                4 => wgpu::IndexFormat::Uint32,
                _ => panic!("Unsupported UiDrawIndex size!"),
            };
            s.ui_gpu.bind_to_render_pass(&mut pass, idx_format);

            let fb_w = s.framebuffer_size.width  as u32;
            let fb_h = s.framebuffer_size.height as u32;

            for cmd in &s.ui_draw_commands {
                // Un-flip Y coordinate. The shared UI renderer flips Y for OpenGL's
                // bottom-left scissor origin. Wgpu uses top-left, so undo the flip.
                let x = (cmd.clip_rect.x() as u32).min(fb_w);
                let wgpu_y = (fb_h as f32 - cmd.clip_rect.y() - cmd.clip_rect.height()).max(0.0);
                let y = (wgpu_y as u32).min(fb_h);
                let w = (cmd.clip_rect.width() as u32).min(fb_w.saturating_sub(x));
                let h = (cmd.clip_rect.height() as u32).min(fb_h.saturating_sub(y));

                if w == 0 || h == 0 {
                    continue;
                }

                pass.set_scissor_rect(x, y, w, h);

                let bg = tex_cache.texture_for_handle(cmd.texture).as_wgpu().bind_group();
                pass.set_bind_group(1, Some(bg), &[]);

                pass.draw_indexed(
                    cmd.first_index..cmd.first_index + cmd.index_count,
                    cmd.base_vertex, 0..1);
                s.stats.draw_calls += 1;
            }
        }

        // Submit and present.
        s.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        // Clear batches for next frame.
        s.sprites_batch.clear();
        s.lines_batch.clear();
        s.points_batch.clear();
        s.ui_batch.clear();
        s.ui_draw_commands.clear();
        s.ui_base_vertex  = 0;
        s.ui_index_offset = 0;

        s.frame_started = false;

        s.stats.render_submit_time_ms = render_submit_timer.end();
        s.stats.peak_triangles_drawn  = s.stats.triangles_drawn.max(s.stats.peak_triangles_drawn);
        s.stats.peak_lines_drawn      = s.stats.lines_drawn.max(s.stats.peak_lines_drawn);
        s.stats.peak_points_drawn     = s.stats.points_drawn.max(s.stats.peak_points_drawn);
        s.stats.peak_texture_changes  = s.stats.texture_changes.max(s.stats.peak_texture_changes);
        s.stats.peak_draw_calls       = s.stats.draw_calls.max(s.stats.peak_draw_calls);

        s.stats
    }

    // ----------------------
    // Viewport/Framebuffer:
    // ----------------------

    #[inline]
    fn viewport(&self) -> Rect {
        self.state().viewport
    }

    #[inline]
    fn set_viewport_size(&mut self, new_size: Size) {
        self.state_mut().set_viewport_size(new_size);
    }

    #[inline]
    fn set_framebuffer_size(&mut self, new_size: Size) {
        self.state_mut().set_framebuffer_size(new_size);
    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    fn begin_ui_render(&mut self) {
        // Nothing to do — UI pass is built in end_frame.
    }

    fn end_ui_render(&mut self) {
        // Nothing to do — UI pass is built in end_frame.
    }

    fn set_ui_draw_buffers(&mut self,
                           vtx_buffer: &[super::UiDrawVertex],
                           idx_buffer: &[super::UiDrawIndex])
    {
        debug_assert!(!vtx_buffer.is_empty() && !idx_buffer.is_empty());

        let s = self.state_mut();

        // Append this draw list's data (not replace). Each ImGui draw list has its own
        // vertex/index buffers; we merge them and track offsets.
        let (base_vertex, index_offset) = s.ui_batch.append_data(vtx_buffer, idx_buffer);
        s.ui_base_vertex  = base_vertex;
        s.ui_index_offset = index_offset;
    }

    fn draw_ui_elements(&mut self,
                        first_index: u32,
                        index_count: u32,
                        texture: super::texture::TextureHandle,
                        _tex_cache: &mut super::texture::TextureCache,
                        clip_rect: Rect)
    {
        debug_assert!(index_count.is_multiple_of(3));

        let s = self.state_mut();

        // Store draw command for replay during the UI render pass in end_frame.
        // first_index is relative to the current draw list, so offset it.
        s.ui_draw_commands.push(UiDrawCommand {
            first_index: first_index + s.ui_index_offset,
            index_count,
            base_vertex: s.ui_base_vertex,
            texture,
            clip_rect,
        });

        s.stats.triangles_drawn += index_count / 3;
    }

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self,
                                      vertices: &[Vec2],
                                      indices: &[super::DrawIndex],
                                      color: Color)
    {
        let s = self.state_mut();
        debug_assert!(s.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!(indices.len().is_multiple_of(3));

        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();
        for v in vertices {
            sprite_verts.push(SpriteVertex2D::new(*v, Vec2::default(), color));
        }

        s.sprites_batch.add_entry(&sprite_verts, indices, super::texture::TextureHandle::white());
        s.stats.triangles_drawn += (indices.len() / 3) as u32;
    }

    fn draw_textured_colored_rect(&mut self,
                                  rect: Rect,
                                  tex_coords: &RectTexCoords,
                                  texture: super::texture::TextureHandle,
                                  color: Color)
    {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_rect_fully_offscreen(&s.viewport, &rect) {
            return;
        }

        let vertices = [
            SpriteVertex2D::new(rect.bottom_left(),  tex_coords.bottom_left(),  color),
            SpriteVertex2D::new(rect.top_left(),     tex_coords.top_left(),     color),
            SpriteVertex2D::new(rect.top_right(),    tex_coords.top_right(),    color),
            SpriteVertex2D::new(rect.bottom_right(), tex_coords.bottom_right(), color),
        ];

        const INDICES: [SpriteIndex2D; 6] = [0, 1, 2, 2, 3, 0];

        s.sprites_batch.add_entry(&vertices, &INDICES, texture);
        s.stats.triangles_drawn += 2;
    }

    // ----------------------
    // Debug drawing:
    // ----------------------

    fn draw_line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_line_fully_offscreen(&s.viewport, &from_pos, &to_pos) {
            return;
        }

        let vertices = [
            LineVertex2D::new(from_pos, from_color),
            LineVertex2D::new(to_pos,   to_color),
        ];
        const INDICES: [LineIndex2D; 2] = [0, 1];

        s.lines_batch.add_fast(&vertices, &INDICES);
        s.stats.lines_drawn += 1;
    }

    fn draw_point(&mut self, pt: Vec2, color: Color, size: f32) {
        let s = self.state_mut();
        debug_assert!(s.frame_started);

        if super::is_point_fully_offscreen(&s.viewport, &pt) {
            return;
        }

        // wgpu doesn't support variable-size points, so expand to a quad.
        let half = size / 2.0;
        let vertices = [
            LineVertex2D::new(Vec2::new(pt.x - half, pt.y - half), color),
            LineVertex2D::new(Vec2::new(pt.x + half, pt.y - half), color),
            LineVertex2D::new(Vec2::new(pt.x + half, pt.y + half), color),
            LineVertex2D::new(Vec2::new(pt.x - half, pt.y + half), color),
        ];
        const INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

        s.points_batch.add_fast(&vertices, &INDICES);
        s.stats.points_drawn += 1;
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
        let s = self.state();
        let texture = WgpuTexture::new(&TextureCreationParams {
            name,
            size,
            pixels,
            settings,
            allow_settings_change,
            device: &s.device,
            queue: &s.queue,
            texture_bind_group_layout: &s.texture_bind_group_layout,
        });
        super::texture::TextureBackendImpl::Wgpu(texture)
    }

    fn update_texture_pixels(&mut self,
                             texture: &mut super::texture::TextureBackendImpl,
                             offset_x: u32,
                             offset_y: u32,
                             size: Size,
                             mip_level: u32,
                             pixels: &[u8])
    {
        let s = self.state();
        texture.as_wgpu().write_pixels(&s.queue, offset_x, offset_y, size, mip_level, pixels);
    }

    fn update_texture_settings(&mut self,
                               texture: &mut super::texture::TextureBackendImpl,
                               settings: super::texture::TextureSettings)
    {
        let s = self.state();
        texture.as_wgpu_mut().rebuild_sampler_and_bind_group(
            &s.device, &s.texture_bind_group_layout, settings);
    }

    fn release_texture(&mut self, texture: &mut super::texture::TextureBackendImpl) {
        texture.as_wgpu_mut().release();
    }
}
