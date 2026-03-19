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
use wasm_bindgen::prelude::*;

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
    WinitInputState, WinitInputSystem,
    winit_physical_key_to_input_key,
    winit_modifiers_to_input_modifiers,
    winit_mouse_button_to_mouse_button,
    winit_element_state_to_input_action,
};
use crate::{
    log,
    game::{GameLoop, config::GameConfigs},
    engine::{self, config::EngineConfigs},
    render::wgpu::WgpuInitResources,
    app::ApplicationEvent,
    utils::Vec2,
};

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
        resizable: bool,
    },

    // Game is fully initialized and running.
    Running,

    // Initialization failed.
    Failed,
}

pub struct WasmGameRunner {
    state: WasmRunnerState,
    configs: &'static GameConfigs,
}

impl WasmGameRunner {
    fn new(configs: &'static GameConfigs) -> Self {
        Self {
            state: WasmRunnerState::WaitingForResume,
            configs,
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

        let engine_configs = &self.configs.engine;

        let window_manager = WinitWindowManager::create(
            event_loop,
            &engine_configs.window_title,
            engine_configs.window_size,
            engine_configs.window_mode,
            engine_configs.resizable_window,
            engine_configs.confine_cursor_to_window,
            engine_configs.content_scale,
        );

        let window = window_manager.window_arc();

        self.state = WasmRunnerState::InitializingGpu {
            window: window.clone(),
            window_manager,
            resizable: engine_configs.resizable_window,
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

            // Store the resources in a global so the event loop handler can pick them up.
            WGPU_INIT_RESULT.with(|cell| {
                cell.replace(Some(WgpuInitResources {
                    device,
                    queue,
                    surface,
                    surface_config,
                    surface_format,
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
        if !matches!(self.state, WasmRunnerState::Running) {
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
                let app_ref = game_loop.engine().app()
                    .as_any()
                    .downcast_ref::<WinitApplication>()
                    .unwrap();
                let scale = app_ref.content_scale();
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
                let resources = WGPU_INIT_RESULT.with(|cell| cell.take());

                if let Some(resources) = resources {
                    // Extract the window_manager from our state.
                    let (window_manager, resizable) = match std::mem::replace(
                        &mut self.state, WasmRunnerState::Failed
                    ) {
                        WasmRunnerState::InitializingGpu { window_manager, resizable, .. } => {
                            (window_manager, resizable)
                        }
                        _ => unreachable!(),
                    };

                    self.finish_init(window_manager, resizable, resources);
                }
            }
            WasmRunnerState::Running => {
                let game_loop = GameLoop::get_mut();
                if game_loop.is_running() {
                    game_loop.update();
                }
            }
            _ => {}
        }
    }
}

impl WasmGameRunner {
    fn finish_init(
        &mut self,
        window_manager: WinitWindowManager,
        resizable: bool,
        resources: WgpuInitResources,
    ) {
        let game_configs = self.configs;
        let engine_configs = &game_configs.engine;

        let viewport_size = window_manager.window_size();
        let framebuffer_size = window_manager.framebuffer_size();

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

        self.state = WasmRunnerState::Running;

        log::info!(log::channel!("app"), "WASM: Game initialized and running!");
    }
}

// Thread-local storage for async wgpu init result.
thread_local! {
    static WGPU_INIT_RESULT: std::cell::Cell<Option<WgpuInitResources>> = const { std::cell::Cell::new(None) };
}

// ----------------------------------------------
// Public entry point
// ----------------------------------------------

// Launch the WASM game. Called from main.rs.
pub fn run_wasm_event_loop(configs: &'static GameConfigs) {
    let event_loop = EventLoop::new()
        .expect("Failed to create winit event loop!");

    let runner = WasmGameRunner::new(configs);
    event_loop.spawn_app(runner);
}
