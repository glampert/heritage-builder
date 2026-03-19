use std::any::Any;
use std::sync::Arc;

use arrayvec::ArrayVec;

use super::{
    batch::{DrawBatch, UiDrawBatch},
    pipeline,
    target::OffscreenTarget,
    texture::TextureCache,
    vertex::*,
    WgpuInitResources,
};
use crate::{
    log,
    engine::time::PerfTimer,
    render::{self, RenderStats, TextureHandle},
    ui::UiRenderFrameBundle,
    utils::{Color, Rect, RectTexCoords, Size, Vec2},
};

// ----------------------------------------------
// Uniform data
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniforms {
    viewport_size: [f32; 2],
    _padding: [f32; 2], // Align to 16 bytes for uniform buffer.
}

// ----------------------------------------------
// GpuBufferPair
// ----------------------------------------------

// Vertex + index buffers on the GPU, lazily grown.
struct GpuBufferPair {
    vertex_buffer: wgpu::Buffer,
    index_buffer:  wgpu::Buffer,
    vertex_capacity: usize, // In bytes.
    index_capacity:  usize, // In bytes.
}

impl GpuBufferPair {
    fn new(device: &wgpu::Device, label: &str, vb_bytes: usize, ib_bytes: usize) -> Self {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}_vb")),
            size: vb_bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}_ib")),
            size: ib_bytes as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { vertex_buffer, index_buffer, vertex_capacity: vb_bytes, index_capacity: ib_bytes }
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, label: &str, vb_bytes: usize, ib_bytes: usize) {
        if vb_bytes > self.vertex_capacity {
            let new_cap = vb_bytes.next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("{label}_vb")),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = new_cap;
        }
        if ib_bytes > self.index_capacity {
            let new_cap = ib_bytes.next_power_of_two();
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("{label}_ib")),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = new_cap;
        }
    }
}

// ----------------------------------------------
// RenderSystem
// ----------------------------------------------

pub struct RenderSystem {
    // Core wgpu state.
    device: wgpu::Device,
    queue:  wgpu::Queue,
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
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    blit_texture_layout:       wgpu::BindGroupLayout,

    // Per-frame uniform buffer + bind group.
    uniform_buffer:     wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    // CPU-side batches.
    sprites_batch: DrawBatch<SpriteVertex2D, SpriteIndex2D>,
    lines_batch:   DrawBatch<LineVertex2D, LineIndex2D>,
    points_batch:  DrawBatch<LineVertex2D, u16>, // Points expanded to quads.
    ui_batch:      UiDrawBatch,

    // GPU-side buffers.
    sprites_gpu: GpuBufferPair,
    lines_gpu:   GpuBufferPair,
    points_gpu:  GpuBufferPair,
    ui_gpu:      GpuBufferPair,

    // Offscreen render target.
    offscreen_rt: OffscreenTarget,

    // Texture cache.
    tex_cache: TextureCache,

    // UI draw commands recorded during the frame.
    ui_draw_commands: Vec<UiDrawCommand>,
    ui_base_vertex:   i32,  // Current draw list's base vertex (in vertices).
    ui_index_offset:  u32,  // Current draw list's index offset (in indices).

    // State.
    frame_started:    bool,
    viewport:         Rect,
    framebuffer_size: Size,
    clear_color:      Color,
    stats:            RenderStats,
}

impl RenderSystem {
    /// Create the RenderSystem from pre-initialized wgpu resources.
    /// On WASM, adapter/device creation is async and happens before this call.
    pub fn from_init_resources(
        resources: WgpuInitResources,
        viewport_size: Size,
        framebuffer_size: Size,
        clear_color: Color,
        texture_settings: render::TextureSettings,
    ) -> Self {
        let WgpuInitResources { device, queue, surface, surface_config, surface_format } = resources;
        Self::init(device, queue, surface, surface_config, surface_format,
                   viewport_size, framebuffer_size, clear_color, texture_settings)
    }

    /// Create the RenderSystem from an Arc<Window> (desktop path).
    /// Uses pollster::block_on to synchronously request adapter and device.
    #[cfg(feature = "desktop")]
    pub fn from_window(
        window: Arc<winit::window::Window>,
        viewport_size: Size,
        framebuffer_size: Size,
        clear_color: Color,
        texture_settings: render::TextureSettings,
    ) -> Self {
        debug_assert!(viewport_size.is_valid());
        debug_assert!(framebuffer_size.is_valid());

        // Create wgpu instance & surface.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(wgpu::SurfaceTarget::from(window))
            .expect("Failed to create wgpu surface!");

        // Request adapter & device.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).expect("Failed to find a suitable GPU adapter!");

        log::info!(log::channel!("render"), "wgpu adapter: {:?}", adapter.get_info());

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("heritage_builder_device"),
                required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER,
                ..Default::default()
            },
        )).expect("Failed to create wgpu device!");

        // Configure surface.
        let surface_caps = surface.get_capabilities(&adapter);
        // Use a non-sRGB surface format to match the OpenGL renderer's behavior.
        // The game does all color work in sRGB space with no linear-space lighting,
        // so we want raw byte pass-through — no automatic gamma conversion.
        let surface_format = surface_caps.formats.iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width:  framebuffer_size.width as u32,
            height: framebuffer_size.height as u32,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Self::init(device, queue, surface, surface_config, surface_format,
                   viewport_size, framebuffer_size, clear_color, texture_settings)
    }

    fn init(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        surface_config: wgpu::SurfaceConfiguration,
        surface_format: wgpu::TextureFormat,
        viewport_size: Size,
        framebuffer_size: Size,
        clear_color: Color,
        texture_settings: render::TextureSettings,
    ) -> Self {
        debug_assert!(viewport_size.is_valid());
        debug_assert!(framebuffer_size.is_valid());

        // Create shared bind group layouts.
        let uniform_bind_group_layout = pipeline::create_uniform_bind_group_layout(&device);
        let texture_bind_group_layout = pipeline::create_texture_bind_group_layout(&device);
        let blit_texture_layout       = pipeline::create_texture_bind_group_layout(&device);

        // Use the same format for the offscreen RT as the surface.
        let offscreen_format = surface_format;

        // Create pipelines.
        let sprites_pipeline = pipeline::create_sprites_pipeline(
            &device, offscreen_format, &uniform_bind_group_layout, &texture_bind_group_layout);
        let lines_pipeline = pipeline::create_lines_pipeline(
            &device, offscreen_format, &uniform_bind_group_layout);
        let points_pipeline = pipeline::create_points_pipeline(
            &device, offscreen_format, &uniform_bind_group_layout);
        let ui_pipeline = pipeline::create_ui_pipeline(
            &device, surface_format, &uniform_bind_group_layout, &texture_bind_group_layout);
        let blit_pipeline = pipeline::create_blit_pipeline(
            &device, surface_format, &blit_texture_layout);

        // Uniform buffer.
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport_uniforms"),
            size: std::mem::size_of::<ViewportUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport_uniform_bind_group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Offscreen RT.
        let rt_size = viewport_size.max(framebuffer_size);
        let offscreen_rt = OffscreenTarget::new(&device, rt_size, offscreen_format, &blit_texture_layout);

        // GPU buffers.
        let sprites_gpu = GpuBufferPair::new(&device, "sprites",
            512 * std::mem::size_of::<SpriteVertex2D>(),
            512 * std::mem::size_of::<SpriteIndex2D>());
        let lines_gpu = GpuBufferPair::new(&device, "lines",
            64 * std::mem::size_of::<LineVertex2D>(),
            64 * std::mem::size_of::<LineIndex2D>());
        let points_gpu = GpuBufferPair::new(&device, "points",
            64 * std::mem::size_of::<LineVertex2D>(),
            64 * std::mem::size_of::<u16>());
        let ui_gpu = GpuBufferPair::new(&device, "ui",
            1024 * std::mem::size_of::<UiVertex>(),
            1024 * std::mem::size_of::<u16>());

        // Texture cache.
        let tex_cache = TextureCache::new(128, texture_settings,
            device.clone(), queue.clone(), texture_bind_group_layout);

        log::info!(log::channel!("render"), "WebGPU RenderSystem initialized. Format: {surface_format:?}");
        log::info!(log::channel!("render"), "  viewport_size: {viewport_size}, framebuffer_size: {framebuffer_size}");
        log::info!(log::channel!("render"), "  offscreen RT: {}x{}", rt_size.width, rt_size.height);
        log::info!(log::channel!("render"), "  surface: {}x{}", surface_config.width, surface_config.height);

        Self {
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

            uniform_bind_group_layout,
            blit_texture_layout,

            uniform_buffer,
            uniform_bind_group,

            sprites_batch: DrawBatch::new(512, 512, 512),
            lines_batch:   DrawBatch::new(64, 64, 0),
            points_batch:  DrawBatch::new(64, 96, 0),
            ui_batch:      UiDrawBatch::new(),

            sprites_gpu,
            lines_gpu,
            points_gpu,
            ui_gpu,

            offscreen_rt,
            tex_cache,

            ui_draw_commands: Vec::with_capacity(64),
            ui_base_vertex: 0,
            ui_index_offset: 0,

            frame_started: false,
            viewport: Rect::from_pos_and_size(Vec2::zero(), viewport_size.to_vec2()),
            framebuffer_size,
            clear_color,
            stats: RenderStats::default(),
        }
    }

    fn reconfigure_surface(&mut self) {
        self.surface_config.width  = self.framebuffer_size.width as u32;
        self.surface_config.height = self.framebuffer_size.height as u32;
        self.surface.configure(&self.device, &self.surface_config);
    }
}

impl render::RenderSystemFactory for RenderSystem {
    fn new(
        viewport_size: Size,
        framebuffer_size: Size,
        clear_color: Color,
        texture_settings: render::TextureSettings,
        app_context: Option<&dyn Any>,
    ) -> Self {
        // On WASM, the RenderSystem is created via from_init_resources() with
        // pre-initialized async wgpu resources. This factory path is desktop-only.
        #[cfg(feature = "web")]
        {
            let _ = (viewport_size, framebuffer_size, clear_color, texture_settings, app_context);
            panic!("On WASM, use from_init_resources() instead of RenderSystemFactory::new()");
        }

        #[cfg(feature = "desktop")]
        {
            let window: Arc<winit::window::Window> = app_context
                .expect("wgpu RenderSystem requires an app_context!")
                .downcast_ref::<Arc<winit::window::Window>>()
                .expect("app_context must be Arc<winit::window::Window>!")
                .clone();

            Self::from_window(window, viewport_size, framebuffer_size, clear_color, texture_settings)
        }
    }
}

impl render::RenderSystem for RenderSystem {
    fn as_any(&self) -> &dyn Any { self }

    fn begin_frame(&mut self, viewport_size: Size, framebuffer_size: Size) {
        debug_assert!(!self.frame_started);

        self.set_viewport_size(viewport_size);
        self.set_framebuffer_size(framebuffer_size);

        self.frame_started = true;

        self.stats.triangles_drawn     = 0;
        self.stats.lines_drawn         = 0;
        self.stats.points_drawn        = 0;
        self.stats.texture_changes     = 0;
        self.stats.draw_calls          = 0;
        self.stats.render_submit_time_ms = 0.0;
    }

    fn end_frame(&mut self, ui_frame_bundle: &mut UiRenderFrameBundle) -> RenderStats {
        debug_assert!(self.frame_started);
        debug_assert!(self.viewport.is_valid());
        debug_assert!(self.framebuffer_size.is_valid());

        let render_submit_timer = PerfTimer::begin();

        // Render UI draw data into our batches.
        ui_frame_bundle.render(self);

        // Update uniform buffer with current viewport size.
        let uniforms = ViewportUniforms {
            viewport_size: [self.viewport.width(), self.viewport.height()],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Ensure GPU buffers are large enough and upload data.
        upload_batch(&self.queue, &self.device, &mut self.sprites_gpu, "sprites",
            &self.sprites_batch.vertices, &self.sprites_batch.indices);
        upload_batch(&self.queue, &self.device, &mut self.lines_gpu, "lines",
            &self.lines_batch.vertices, &self.lines_batch.indices);
        upload_batch(&self.queue, &self.device, &mut self.points_gpu, "points",
            &self.points_batch.vertices, &self.points_batch.indices);

        // UI batch uses raw bytes (not Pod types).
        // wgpu requires write_buffer data to respect COPY_BUFFER_ALIGNMENT (4 bytes).
        // imgui::DrawIdx is u16, so index data may not be 4-byte aligned.
        {
            let vb = align_to_4(self.ui_batch.vertices.len());
            let ib = align_to_4(self.ui_batch.indices.len());
            self.ui_gpu.ensure_capacity(&self.device, "ui", vb, ib);
            if !self.ui_batch.vertices.is_empty() {
                let data = pad_to_alignment::<4>(&self.ui_batch.vertices);
                self.queue.write_buffer(&self.ui_gpu.vertex_buffer, 0, &data);
            }
            if !self.ui_batch.indices.is_empty() {
                let data = pad_to_alignment::<4>(&self.ui_batch.indices);
                self.queue.write_buffer(&self.ui_gpu.index_buffer, 0, &data);
            }
        }

        // Acquire surface texture.
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.reconfigure_surface();
                self.surface.get_current_texture()
                    .expect("Failed to acquire surface texture after reconfigure!")
            }
            Err(e) => panic!("Failed to acquire surface texture: {e:?}"),
        };
        let surface_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame_encoder"),
        });

        let cc = &self.clear_color;
        let clear_color_wgpu = wgpu::Color {
            r: cc.r as f64, g: cc.g as f64, b: cc.b as f64, a: cc.a as f64,
        };

        // ---- Pass 1: Render world to offscreen RT ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.offscreen_rt.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color_wgpu),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // Draw sprites.
            if !self.sprites_batch.is_empty() {
                pass.set_pipeline(&self.sprites_pipeline);
                pass.set_bind_group(0, Some(&self.uniform_bind_group), &[]);
                pass.set_vertex_buffer(0, self.sprites_gpu.vertex_buffer.slice(..));
                pass.set_index_buffer(self.sprites_gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                let mut last_texture = TextureHandle::invalid();

                for entry in &self.sprites_batch.entries {
                    // Switch texture bind group when needed.
                    if !texture_handle_eq(entry.texture, last_texture) {
                        let wgpu_tex = self.tex_cache.handle_to_wgpu_texture(entry.texture);
                        pass.set_bind_group(1, Some(&wgpu_tex.bind_group), &[]);
                        last_texture = entry.texture;
                        self.stats.texture_changes += 1;
                    }

                    pass.draw_indexed(
                        entry.first_index..entry.first_index + entry.index_count,
                        0, 0..1);
                    self.stats.draw_calls += 1;
                }
            }

            // Draw lines.
            if !self.lines_batch.is_empty() {
                pass.set_pipeline(&self.lines_pipeline);
                pass.set_bind_group(0, Some(&self.uniform_bind_group), &[]);
                pass.set_vertex_buffer(0, self.lines_gpu.vertex_buffer.slice(..));
                pass.set_index_buffer(self.lines_gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.lines_batch.indices.len() as u32, 0, 0..1);
                self.stats.draw_calls += 1;
            }

            // Draw points (expanded to quads).
            if !self.points_batch.is_empty() {
                pass.set_pipeline(&self.points_pipeline);
                pass.set_bind_group(0, Some(&self.uniform_bind_group), &[]);
                pass.set_vertex_buffer(0, self.points_gpu.vertex_buffer.slice(..));
                pass.set_index_buffer(self.points_gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.points_batch.indices.len() as u32, 0, 0..1);
                self.stats.draw_calls += 1;
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

            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, Some(&self.offscreen_rt.bind_group), &[]);
            pass.draw(0..3, 0..1); // Fullscreen triangle, no vertex buffer.
            self.stats.draw_calls += 1;
        }

        // ---- Pass 3: UI on top of surface ----
        if !self.ui_draw_commands.is_empty() && !self.ui_batch.vertices.is_empty() {
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

            pass.set_pipeline(&self.ui_pipeline);
            pass.set_bind_group(0, Some(&self.uniform_bind_group), &[]);
            pass.set_vertex_buffer(0, self.ui_gpu.vertex_buffer.slice(..));

            let idx_format = match std::mem::size_of::<imgui::DrawIdx>() {
                2 => wgpu::IndexFormat::Uint16,
                4 => wgpu::IndexFormat::Uint32,
                _ => panic!("Unsupported imgui::DrawIdx size!"),
            };
            pass.set_index_buffer(self.ui_gpu.index_buffer.slice(..), idx_format);

            let fb_w = self.framebuffer_size.width as u32;
            let fb_h = self.framebuffer_size.height as u32;

            for cmd in &self.ui_draw_commands {
                // The shared UI renderer (renderer.rs) flips clip_rect Y for OpenGL's
                // bottom-left scissor origin. wgpu uses top-left origin, so un-flip:
                //   clip_rect.y() = fb_height - clip_max_y  (OpenGL convention)
                //   wgpu_y = fb_height - clip_rect.y() - clip_rect.height() = clip_min_y
                let x = (cmd.clip_rect.x() as u32).min(fb_w);
                let wgpu_y = (fb_h as f32 - cmd.clip_rect.y() - cmd.clip_rect.height()).max(0.0);
                let y = (wgpu_y as u32).min(fb_h);
                let w = (cmd.clip_rect.width() as u32).min(fb_w.saturating_sub(x));
                let h = (cmd.clip_rect.height() as u32).min(fb_h.saturating_sub(y));

                if w == 0 || h == 0 {
                    continue;
                }

                pass.set_scissor_rect(x, y, w, h);

                let wgpu_tex = self.tex_cache.handle_to_wgpu_texture(cmd.texture);
                pass.set_bind_group(1, Some(&wgpu_tex.bind_group), &[]);

                pass.draw_indexed(
                    cmd.first_index..cmd.first_index + cmd.index_count,
                    cmd.base_vertex, 0..1);
                self.stats.draw_calls += 1;
            }
        }

        // Submit and present.
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        // Clear batches for next frame.
        self.sprites_batch.clear();
        self.lines_batch.clear();
        self.points_batch.clear();
        self.ui_batch.clear();
        self.ui_draw_commands.clear();
        self.ui_base_vertex  = 0;
        self.ui_index_offset = 0;

        self.frame_started = false;

        self.stats.render_submit_time_ms = render_submit_timer.end();
        self.stats.peak_triangles_drawn  = self.stats.triangles_drawn.max(self.stats.peak_triangles_drawn);
        self.stats.peak_lines_drawn      = self.stats.lines_drawn.max(self.stats.peak_lines_drawn);
        self.stats.peak_points_drawn     = self.stats.points_drawn.max(self.stats.peak_points_drawn);
        self.stats.peak_texture_changes  = self.stats.texture_changes.max(self.stats.peak_texture_changes);
        self.stats.peak_draw_calls       = self.stats.draw_calls.max(self.stats.peak_draw_calls);

        self.stats
    }

    #[inline]
    fn texture_cache(&self) -> &dyn render::TextureCache { &self.tex_cache }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut dyn render::TextureCache { &mut self.tex_cache }

    #[inline]
    fn viewport(&self) -> Rect { self.viewport }

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
            if rt_size.width > self.offscreen_rt.size.width
                || rt_size.height > self.offscreen_rt.size.height
            {
                self.offscreen_rt = OffscreenTarget::new(
                    &self.device, rt_size, self.surface_format, &self.blit_texture_layout);
            }
        }
    }

    // ----------------------
    // UI (ImGui) Drawing:
    // ----------------------

    fn begin_ui_render(&mut self) {
        // Nothing to do for wgpu; UI pass is built in end_frame.
    }

    fn end_ui_render(&mut self) {
        // Nothing to do for wgpu; UI pass is built in end_frame.
    }

    fn set_ui_draw_buffers(&mut self, vtx_buffer: &[imgui::DrawVert], idx_buffer: &[imgui::DrawIdx]) {
        debug_assert!(!vtx_buffer.is_empty() && !idx_buffer.is_empty());

        // Append this draw list's data (NOT replace). Each ImGui draw list has its own
        // vertex/index buffers; we merge them into one combined buffer and track offsets.
        let (base_vertex, index_offset) = self.ui_batch.append_data(vtx_buffer, idx_buffer);
        self.ui_base_vertex  = base_vertex;
        self.ui_index_offset = index_offset;
    }

    fn draw_ui_elements(&mut self, first_index: u32, index_count: u32, texture: TextureHandle, clip_rect: Rect) {
        debug_assert!(index_count.is_multiple_of(3));

        // Store draw command for replay during the UI render pass in end_frame.
        // first_index is relative to the current draw list's index buffer, so offset it
        // to account for previously appended draw lists.
        self.ui_draw_commands.push(UiDrawCommand {
            first_index: first_index + self.ui_index_offset,
            index_count,
            base_vertex: self.ui_base_vertex,
            texture,
            clip_rect,
        });

        self.stats.triangles_drawn += index_count / 3;
    }

    // ----------------------
    // Draw commands:
    // ----------------------

    fn draw_colored_indexed_triangles(&mut self, vertices: &[Vec2], indices: &[u16], color: Color) {
        debug_assert!(self.frame_started);
        debug_assert!(!vertices.is_empty() && !indices.is_empty());
        debug_assert!(indices.len().is_multiple_of(3));

        let mut sprite_verts: ArrayVec<SpriteVertex2D, 64> = ArrayVec::new();
        for v in vertices {
            sprite_verts.push(SpriteVertex2D::new(*v, Vec2::default(), color));
        }

        self.sprites_batch.add_entry(&sprite_verts, indices, TextureHandle::white(), color);
        self.stats.triangles_drawn += (indices.len() / 3) as u32;
    }

    fn draw_textured_colored_rect(
        &mut self,
        rect: Rect,
        tex_coords: &RectTexCoords,
        texture: TextureHandle,
        color: Color,
    ) {
        debug_assert!(self.frame_started);

        if render::is_rect_fully_offscreen(&self.viewport, &rect) {
            return;
        }

        let vertices = [
            SpriteVertex2D::new(rect.bottom_left(),  tex_coords.bottom_left(),  color),
            SpriteVertex2D::new(rect.top_left(),     tex_coords.top_left(),     color),
            SpriteVertex2D::new(rect.top_right(),    tex_coords.top_right(),    color),
            SpriteVertex2D::new(rect.bottom_right(), tex_coords.bottom_right(), color),
        ];

        const INDICES: [SpriteIndex2D; 6] = [0, 1, 2, 2, 3, 0];

        self.sprites_batch.add_entry(&vertices, &INDICES, texture, color);
        self.stats.triangles_drawn += 2;
    }

    fn draw_line_fast(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        debug_assert!(self.frame_started);

        if render::is_line_fully_offscreen(&self.viewport, &from_pos, &to_pos) {
            return;
        }

        let vertices = [
            LineVertex2D::new(from_pos, from_color),
            LineVertex2D::new(to_pos,   to_color),
        ];

        const INDICES: [LineIndex2D; 2] = [0, 1];

        self.lines_batch.add_fast(&vertices, &INDICES);
        self.stats.lines_drawn += 1;
    }

    fn draw_point_fast(&mut self, pt: Vec2, color: Color, size: f32) {
        debug_assert!(self.frame_started);

        if render::is_point_fully_offscreen(&self.viewport, &pt) {
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

        self.points_batch.add_fast(&vertices, &INDICES);
        self.stats.points_drawn += 1;
    }
}

// ----------------------------------------------
// Free helper — avoids borrow-splitting issues
// with &self.queue vs &mut self.*_gpu.
// ----------------------------------------------

fn upload_batch<V: bytemuck::Pod, I: bytemuck::Pod>(
    queue: &wgpu::Queue,
    device: &wgpu::Device,
    gpu: &mut GpuBufferPair,
    label: &str,
    vertices: &[V],
    indices: &[I],
) {
    if vertices.is_empty() {
        return;
    }

    let vb_bytes = std::mem::size_of_val(vertices);
    let ib_bytes = std::mem::size_of_val(indices);
    gpu.ensure_capacity(device, label, vb_bytes, ib_bytes);

    queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(vertices));
    if !indices.is_empty() {
        queue.write_buffer(&gpu.index_buffer, 0, bytemuck::cast_slice(indices));
    }
}

// ----------------------------------------------
// UiDrawCommand
// ----------------------------------------------

struct UiDrawCommand {
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
    texture:     TextureHandle,
    clip_rect:   Rect,
}

// TextureHandle doesn't implement Eq, so we compare by pack value.
#[inline]
fn texture_handle_eq(a: TextureHandle, b: TextureHandle) -> bool {
    a.pack() == b.pack()
}

// Round up to the next multiple of N.
#[inline]
fn align_to_4(n: usize) -> usize {
    (n + 3) & !3
}

// Pad a byte slice to ALIGN-byte boundary by appending zero bytes.
// Returns the original data if already aligned, or a padded copy.
fn pad_to_alignment<const ALIGN: usize>(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let remainder = data.len() % ALIGN;
    if remainder == 0 {
        std::borrow::Cow::Borrowed(data)
    } else {
        let mut padded = data.to_vec();
        padded.resize(data.len() + ALIGN - remainder, 0);
        std::borrow::Cow::Owned(padded)
    }
}
