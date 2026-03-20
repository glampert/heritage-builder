// WASM Game Runner
//
// On WASM, the browser owns the event loop. This module provides the
// ApplicationHandler implementation that drives the game on the web.
//
// Flow:
//  1. wasm_main() creates an EventLoop and calls spawn_app(WasmGameRunner).
//  2. resumed() creates the window, kicks off async wgpu init via spawn_local.
//  3. Once async init completes, the Engine and GameLoop are created.
//  4. about_to_wait() calls GameLoop::update() each frame.

use std::sync::Arc;
use wasm_bindgen::JsCast;
use winit::{
    application::ApplicationHandler,
    event::{MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    platform::web::EventLoopExtWebSys,
    window::WindowId,
};

use super::window::WinitWindowManager;
use super::WinitApplication;
use super::super::input::{
    winit_physical_key_to_input_key,
    winit_modifiers_to_input_modifiers,
    winit_mouse_button_to_mouse_button,
    winit_element_state_to_input_action,
};
use crate::{
    log,
    engine,
    utils::Vec2,
    game::{GameLoop, config::GameConfigs},
    app::{Application, ApplicationEvent},
    render::wgpu::WgpuInitResources,
};

// ----------------------------------------------
// Loading screen helpers
// ----------------------------------------------

// Update the browser loading screen progress bar and status text.
fn set_loading_progress(percent: u32, message: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(bar) = document.get_element_by_id("loading-bar") {
        let _ = bar.unchecked_ref::<web_sys::HtmlElement>()
            .style()
            .set_property("width", &format!("{percent}%"));
    }
    if let Some(status) = document.get_element_by_id("loading-status") {
        status.set_text_content(Some(message));
    }
}

// Hide the browser loading screen overlay.
fn hide_loading_screen() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(el) = document.get_element_by_id("loading-screen") {
        let _ = el.class_list().add_1("hidden");
    }
}

// ----------------------------------------------
// WasmGameRunner / WasmRunnerState
// ----------------------------------------------

enum WasmRunnerState {
    // Waiting for the event loop to call resumed().
    WaitingForResume,

    // Window created, async wgpu init in progress.
    InitializingGpu {
        window: Arc<winit::window::Window>,
        window_manager: WinitWindowManager,
    },

    // Game is fully initialized and running.
    Running {
        window: Arc<winit::window::Window>,
    },

    // Initialization failed.
    Failed,
}

pub struct WasmGameRunner {
    state: WasmRunnerState,
}

impl WasmGameRunner {
    fn new() -> Self {
        Self {
            state: WasmRunnerState::WaitingForResume,
        }
    }
}

impl ApplicationHandler for WasmGameRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Only initialize once.
        if !matches!(self.state, WasmRunnerState::WaitingForResume) {
            return;
        }

        log::info!(log::channel!("app"), "WASM: resumed() — creating window...");

        // Use defaults for window creation. The actual game configs will be loaded
        // after assets are fetched (they live in the asset cache on WASM).
        let defaults = engine::config::EngineConfigs::default();

        let window_manager = WinitWindowManager::create(
            event_loop,
            &defaults.window_title,
            defaults.window_size,
            defaults.window_mode,
            defaults.resizable_window,
            defaults.confine_cursor_to_window,
            defaults.content_scale,
        );

        let window = window_manager.window_arc();

        self.state = WasmRunnerState::InitializingGpu {
            window: window.clone(),
            window_manager,
        };

        // Kick off async wgpu initialization.
        wasm_bindgen_futures::spawn_local(async move {
            log::info!(log::channel!("app"), "WASM: starting async wgpu init...");

            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
                ..Default::default()
            });

            let surface = instance.create_surface(wgpu::SurfaceTarget::from(window.clone()))
                .expect("Failed to create wgpu surface!");

            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }).await.expect("Failed to find a suitable GPU adapter!");

            log::info!(log::channel!("app"), "WASM: adapter: {:?}", adapter.get_info());
            set_loading_progress(30, "GPU initialized...");

            // On WebGL, ADDRESS_MODE_CLAMP_TO_BORDER may not be available.
            let features = if adapter.features().contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER) {
                wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
            } else {
                log::warning!(log::channel!("app"), "WASM: ADDRESS_MODE_CLAMP_TO_BORDER not available, using ClampToEdge fallback.");
                wgpu::Features::empty()
            };

            let (device, queue) = adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("heritage_builder_device"),
                    required_features: features,
                    ..Default::default()
                },
            ).await.expect("Failed to create wgpu device!");

            let surface_caps = surface.get_capabilities(&adapter);
            let surface_format = surface_caps.formats.iter()
                .find(|f| !f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0]);

            let phys_size = window.inner_size();
            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: phys_size.width,
                height: phys_size.height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: surface_caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &surface_config);

            log::info!(log::channel!("app"), "WASM: wgpu initialized. Format: {surface_format:?}");
            set_loading_progress(40, "Loading assets...");

            // Load all game assets into the in-memory cache.
            log::info!(log::channel!("app"), "WASM: loading assets from manifest...");
            match crate::web::asset_cache::load_from_manifest("asset_manifest.json").await {
                Ok(count) => log::info!(log::channel!("app"), "WASM: loaded {count} assets."),
                Err(err) => log::error!(log::channel!("app"), "WASM: asset loading failed: {err}"),
            }

            set_loading_progress(70, "Loading configs...");

            // Now that assets are cached, load game configs from the asset cache.
            log::info!(log::channel!("app"), "WASM: loading game configs...");
            let configs = GameConfigs::load();
            log::info!(log::channel!("app"), "WASM: Game configs loaded!");

            set_loading_progress(90, "Starting game...");

            // Store the resources in a global so the event loop handler can pick them up.
            WGPU_INIT_RESULT.with(|cell| {
                cell.replace(Some(WasmInitResult {
                    resources: WgpuInitResources {
                        device,
                        queue,
                        surface,
                        surface_config,
                        surface_format,
                    },
                    configs,
                }));
            });
        });
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Only forward events when the game is running.
        if !matches!(self.state, WasmRunnerState::Running { .. }) {
            return;
        }

        let game_loop = GameLoop::get_mut();
        let app = game_loop.engine().app()
            .as_any()
            .downcast_ref::<WinitApplication>()
            .unwrap();

        match event {
            WindowEvent::CloseRequested => {
                app.push_event(ApplicationEvent::Quit);
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let key = winit_physical_key_to_input_key(key_event.physical_key);
                let action = winit_element_state_to_input_action(key_event.state, key_event.repeat);
                let modifiers = app.input_state_mut().modifiers;

                app.input_state_mut().set_key(key, key_event.state.is_pressed());
                app.push_event(ApplicationEvent::KeyInput(key, action, modifiers));

                if let Some(text) = key_event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        app.push_event(ApplicationEvent::CharInput(c));
                    }
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                app.input_state_mut().modifiers =
                    winit_modifiers_to_input_modifiers(new_modifiers.state());
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(x, y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        Vec2::new(pos.x as f32 / 20.0, pos.y as f32 / 20.0)
                    }
                };
                app.push_event(ApplicationEvent::Scroll(scroll));
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if let Some(mb) = winit_mouse_button_to_mouse_button(button) {
                    let action = winit_element_state_to_input_action(state, false);
                    let modifiers = app.input_state_mut().modifiers;
                    app.input_state_mut().set_mouse_button(mb, state.is_pressed());
                    app.push_event(ApplicationEvent::MouseButton(mb, action, modifiers));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = app.content_scale();
                app.input_state_mut().cursor_pos = Vec2::new(
                    position.x as f32 / scale.x,
                    position.y as f32 / scale.y,
                );
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        match &self.state {
            WasmRunnerState::InitializingGpu { .. } => {
                // Check if async wgpu init has completed.
                let init_result: Option<WasmInitResult> = WGPU_INIT_RESULT.with(|cell| cell.take());

                if let Some(init_result) = init_result {
                    // Extract the window_manager from our state.
                    let window_manager = match std::mem::replace(
                        &mut self.state, WasmRunnerState::Failed
                    ) {
                        WasmRunnerState::InitializingGpu { window_manager, .. } => {
                            window_manager
                        }
                        _ => unreachable!(),
                    };

                    self.finish_init(window_manager, init_result);
                }
            }
            WasmRunnerState::Running { window } => {
                let game_loop = GameLoop::get_mut();
                if game_loop.is_running() {
                    game_loop.update();
                }
                // Request the next frame so the browser keeps calling us
                // via requestAnimationFrame, even when there are no input events.
                window.request_redraw();
            }
            _ => {}
        }
    }
}

impl WasmGameRunner {
    fn finish_init(
        &mut self,
        window_manager: WinitWindowManager,
        init_result: WasmInitResult,
    ) {
        let WasmInitResult { mut resources, configs: game_configs } = init_result;
        let engine_configs = &game_configs.engine;
        let resizable = engine_configs.resizable_window;

        // Resize the window/canvas to the size from configs (it was created
        // with defaults since configs weren't loaded yet).
        let config_size = engine_configs.window_size;
        let _ = window_manager.window.request_inner_size(
            winit::dpi::LogicalSize::new(config_size.width as f64, config_size.height as f64)
        );

        // Re-read sizes after resize and reconfigure the wgpu surface to match.
        let viewport_size = window_manager.window_size();
        let framebuffer_size = window_manager.framebuffer_size();
        resources.surface_config.width = framebuffer_size.width as u32;
        resources.surface_config.height = framebuffer_size.height as u32;
        resources.surface.configure(&resources.device, &resources.surface_config);

        // Keep a reference to the window for requesting redraws.
        let window = window_manager.window_arc();

        // Create the WinitApplication (without an event loop — WASM mode).
        let app = WinitApplication::from_window_manager(window_manager, resizable);

        // Create the wgpu RenderSystem from pre-initialized resources.
        let render_system = crate::render::wgpu::system::RenderSystem::from_init_resources(
            resources,
            viewport_size,
            framebuffer_size,
            engine_configs.window_background_color,
            engine_configs.texture_settings,
        );

        // Create the engine from pre-built parts.
        let engine = Box::new(
            engine::backend::WinitWgpuEngine::from_parts(app, render_system, engine_configs)
        );

        // Initialize the game loop.
        GameLoop::start_with_engine(engine, game_configs);

        // Hide the browser loading screen — game is fully initialized.
        set_loading_progress(100, "Ready!");
        hide_loading_screen();

        self.state = WasmRunnerState::Running { window };

        log::info!(log::channel!("app"), "WASM: Game initialized and running!");
    }
}

// Bundles async init results (wgpu resources + loaded configs) for handoff to the event loop.
struct WasmInitResult {
    resources: WgpuInitResources,
    configs: &'static GameConfigs,
}

// Thread-local storage for async init result.
thread_local! {
    static WGPU_INIT_RESULT: std::cell::Cell<Option<WasmInitResult>> = const { std::cell::Cell::new(None) };
}

// ----------------------------------------------
// Public entry point
// ----------------------------------------------

// Launch the WASM game. Called from main.rs.
pub fn run_wasm_event_loop() {
    let event_loop = EventLoop::new()
        .expect("Failed to create winit event loop!");

    let runner = WasmGameRunner::new();
    event_loop.spawn_app(runner);
}
