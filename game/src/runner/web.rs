// ----------------------------------------------
// Web/WASM Game Runner (Winit + Wgpu)
// ----------------------------------------------
//
// On WASM, the browser owns the event loop. This module provides the
// ApplicationHandler implementation that drives the game on the web.
//
// Initialization flow (three async phases):
//  1. resumed()       — kick off async asset fetch + config load.
//  2. about_to_wait() — once configs are ready, create window with real configs, then kick off async Wgpu init.
//  3. about_to_wait() — once GPU resources are ready, build Application, RenderSystem, Engine, and start the GameLoop.
//  4. about_to_wait() — each subsequent frame calls GameLoop::update().

use std::sync::Arc;

use winit::{
    event::WindowEvent,
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::web::EventLoopExtWebSys,
    window::{Window, WindowId},
};

use super::{Runner, RunLoop};
use crate::{
    log,
    platform,
    engine::Engine,
    file_sys::{self, paths},
    utils::{Size, Vec2, mem::singleton},
    game::config::GameConfigs,
    debug::log_viewer::LogViewer,
    render::{
        RenderApi,
        RenderSystem,
        RenderSystemInitParams,
        wgpu::WgpuInitResources,
    },
    app::{
        self,
        Application,
        ApplicationEvent,
        ApplicationInitParams,
        ApplicationApi,
        winit::input, // Re-use winit input conversion helpers.
    },
};

// ----------------------------------------------
// Web page loading screen helpers
// ----------------------------------------------

fn set_loading_progress(percent: u32, message: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(bar) = document.get_element_by_id("loading-bar") {
        use wasm_bindgen::JsCast;
        let _ = bar.unchecked_ref::<web_sys::HtmlElement>()
            .style()
            .set_property("width", &format!("{percent}%"));
    }
    if let Some(status) = document.get_element_by_id("loading-status") {
        status.set_text_content(Some(message));
    }
}

fn hide_loading_screen() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(el) = document.get_element_by_id("loading-screen") {
        let _ = el.class_list().add_1("hidden");
    }
}

// ----------------------------------------------
// WebRunner
// ----------------------------------------------

pub struct WebRunner;

impl Runner for WebRunner {
    fn new() -> Self { Self }

    fn run<GameLoop: RunLoop + 'static>(&self) {
        log::info!(log::channel!("game"), "--- Web Runner entry point started ---");

        // Early initialization:
        LogViewer::initialize();
        platform::initialize();
        paths::set_working_directory(paths::base_path());

        let event_loop = EventLoop::new()
            .expect("Failed to create Winit event loop!");

        let handler = WebEventHandler::<GameLoop>::new();
        event_loop.spawn_app(handler);
    }
}

// ----------------------------------------------
// WebEventHandler state machine
// ----------------------------------------------

enum WebRunnerState {
    // Waiting for the event loop to call resumed().
    WaitingForResume,

    // Async asset fetch + config load in progress (no window yet).
    LoadingAssets,

    // Window created, async Wgpu GPU init in progress.
    InitializingGpu {
        window: Arc<Window>,
        configs: &'static GameConfigs,
    },

    // Finished initialization. About to go into Running state.
    ReadyToStartRunLoop,

    // Game is fully initialized and running.
    Running { window: Arc<Window> },
}

struct WebEventHandler<GameLoop: RunLoop + 'static> {
    state: WebRunnerState,
    _phantom: std::marker::PhantomData<GameLoop>,
}

impl<GameLoop: RunLoop + 'static> WebEventHandler<GameLoop> {
    fn new() -> Self {
        Self {
            state: WebRunnerState::WaitingForResume,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Async handoff variables.
// These must be static to outlive the wasm_bindgen_futures::spawn_local() closures.
struct AsyncInitResults {
    loaded_configs: Option<&'static GameConfigs>,
    wgpu_resources: Option<WgpuInitResources>,
}

impl AsyncInitResults {
    const fn new() -> Self {
        Self { loaded_configs: None, wgpu_resources: None }
    }
}

singleton! { ASYNC_INIT_RESULTS_SINGLETON, AsyncInitResults }

// ----------------------------------------------
// ApplicationHandler for WebEventHandler
// ----------------------------------------------

impl<GameLoop: RunLoop + 'static> ApplicationHandler for WebEventHandler<GameLoop> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Only initialize once.
        if !matches!(self.state, WebRunnerState::WaitingForResume) {
            return;
        }

        self.start_asset_loading();
    }

    fn window_event(&mut self,
                    _event_loop: &ActiveEventLoop,
                    _window_id: WindowId,
                    event: WindowEvent)
    {
        // Only forward events when the game is running.
        if !matches!(self.state, WebRunnerState::Running { .. }) {
            return;
        }

        let app = Engine::get_mut().app_mut();
        self.handle_window_event(app, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match &self.state {
            WebRunnerState::LoadingAssets => {
                // Phase 2: Once configs are ready, create Winit window and start Wgpu init.
                let Some(configs) = AsyncInitResults::get_mut().loaded_configs.take() else { return };

                self.create_window_and_wgpu_device(configs, event_loop);
            }
            WebRunnerState::InitializingGpu { .. } => {
                // Phase 3: Once GPU resources are ready, finish initialization.
                let Some(wgpu_resources) = AsyncInitResults::get_mut().wgpu_resources.take() else { return };

                // Start GameLoop and set state to Running.
                self.finish_init(wgpu_resources);
            }
            WebRunnerState::Running { window } => {
                // Run one iteration of the GameLoop.
                self.run_loop();

                // Request the next frame so the browser keeps calling us
                // via requestAnimationFrame, even when there are no input events.
                window.request_redraw();
            }
            _ => {}
        }
    }
}

// ----------------------------------------------
// WebEventHandler internals
// ----------------------------------------------

impl<GameLoop: RunLoop + 'static> WebEventHandler<GameLoop> {
    fn start_asset_loading(&mut self) {
        log::info!(log::channel!("runner"), "Web Runner: resumed() — Starting asset loading...");
        self.state = WebRunnerState::LoadingAssets;

        // Phase 1: Async asset fetch + config load (no window/GPU needed).
        wasm_bindgen_futures::spawn_local(async {
            set_loading_progress(10, "Loading assets...");

            log::info!(log::channel!("runner"), "Web Runner: Loading assets from manifest...");

            match file_sys::preload_asset_cache("asset_manifest.json").await {
                Ok(count) => log::info!(log::channel!("runner"), "Web Runner: Loaded {count} assets."),
                Err(err) => log::error!(log::channel!("runner"), "Web Runner: Asset loading failed: {err}"),
            }

            set_loading_progress(40, "Loading configs...");

            log::info!(log::channel!("runner"), "Web Runner: Loading game configs...");
            let configs = GameConfigs::load();
            log::info!(log::channel!("runner"), "Web Runner: Game configs loaded!");

            log::set_level(configs.engine.log_level);

            set_loading_progress(50, "Initializing Engine...");

            AsyncInitResults::get_mut().loaded_configs = Some(configs); // WebRunnerState::LoadingAssets finished.
        });
    }

    fn create_window_and_wgpu_device(&mut self,
                                     configs: &'static GameConfigs,
                                     event_loop: &ActiveEventLoop)
    {
        log::info!(log::channel!("runner"), "Web Runner: Configs ready — creating window...");

        let window_params = ApplicationInitParams {
            app_api:          ApplicationApi::Winit,
            render_api:       RenderApi::Wgpu,
            window_title:     &configs.engine.window_title,
            window_size:      configs.engine.window_size,
            window_mode:      configs.engine.window_mode,
            content_scale:    configs.engine.content_scale,
            resizable_window: configs.engine.resizable_window,
            confine_cursor:   configs.engine.confine_cursor_to_window,
            ..Default::default()
        };

        let window = Arc::new(app::winit::wgpu::create_window(event_loop, &window_params));

        self.state = WebRunnerState::InitializingGpu {
            window: window.clone(),
            configs,
        };

        // Kick off async wgpu initialization.
        wasm_bindgen_futures::spawn_local(async move {
            log::info!(log::channel!("runner"), "Web Runner: Starting async Wgpu init...");

            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
                ..Default::default()
            });

            let surface = instance.create_surface(wgpu::SurfaceTarget::from(window.clone()))
                .expect("Failed to create Wgpu surface!");

            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }).await.expect("Failed to find a suitable GPU adapter!");

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
            set_loading_progress(70, "GPU initialized...");

            // On WebGL, ADDRESS_MODE_CLAMP_TO_BORDER may not be available.
            let features = if adapter.features().contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER) {
                wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
            } else {
                log::warning!(log::channel!("runner"), "Web Runner: ADDRESS_MODE_CLAMP_TO_BORDER not available, using ClampToEdge fallback.");
                wgpu::Features::empty()
            };

            let (device, queue) = adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("heritage_builder_device"),
                    required_features: features,
                    ..Default::default()
                },
            ).await.expect("Failed to create Wgpu device!");

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

            log::info!(log::channel!("runner"), "Web Runner: Wgpu initialized. Format: {surface_format:?}");
            set_loading_progress(90, "Starting game...");

            AsyncInitResults::get_mut().wgpu_resources = Some(WgpuInitResources {
                device,
                queue,
                surface,
                surface_config,
                surface_format,
            });
        });
    }

    fn finish_init(&mut self, wgpu_resources: WgpuInitResources) {
        // Extract window + configs from the current state.
        let (window, configs) = match std::mem::replace(&mut self.state, WebRunnerState::ReadyToStartRunLoop) {
            WebRunnerState::InitializingGpu { window, configs } => (window, configs),
            _ => panic!("Invalid WebRunnerState!"),
        };

        // Initialize Application:
        let app = Application::new(
            ApplicationInitParams {
                app_api:          ApplicationApi::Winit,
                render_api:       RenderApi::Wgpu,
                window_title:     &configs.engine.window_title,
                window_size:      configs.engine.window_size,
                window_mode:      configs.engine.window_mode,
                content_scale:    configs.engine.content_scale,
                resizable_window: configs.engine.resizable_window,
                confine_cursor:   configs.engine.confine_cursor_to_window,
                opt_window:       Some(&window), // With pre-created Window instance.
            }
        );
        log::info!(log::channel!("runner"), "Application initialized.");

        // Create the RenderSystem from pre-initialized resources:
        let render_system = RenderSystem::new(
            RenderSystemInitParams {
                render_api:       RenderApi::Wgpu,
                clear_color:      configs.engine.window_background_color,
                texture_settings: configs.engine.texture_settings,
                viewport_size:    app.window_size(),
                framebuffer_size: app.framebuffer_size(),
                wgpu_resources:   Some(wgpu_resources),
                ..Default::default()
            }
        );
        log::info!(log::channel!("runner"), "RenderSystem initialized.");

        // Create Engine and start the GameLoop.
        let engine = Engine::start(&configs.engine, app, render_system);
        GameLoop::start(engine, configs);

        // Hide the browser loading screen — game is fully initialized.
        set_loading_progress(100, "Ready!");
        hide_loading_screen();

        self.state = WebRunnerState::Running { window };

        log::info!(log::channel!("runner"), "Web Runner: Game initialized and running!");
    }

    fn run_loop(&self) {
        let game_loop = GameLoop::get_mut();

        if game_loop.is_running() {
            game_loop.update();
        }
    }

    fn handle_window_event(&mut self, app: &mut Application, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                app.push_event(ApplicationEvent::Quit);
            }
            WindowEvent::Resized(new_phys_size) => {
                let new_size = Size::new(new_phys_size.width as i32, new_phys_size.height as i32);
                app.push_event(ApplicationEvent::WindowResize {
                    window_size: app.window_size(),
                    framebuffer_size: new_size,
                });
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let key = input::winit_physical_key_to_input_key(key_event.physical_key);
                let action = input::winit_element_state_to_input_action(key_event.state, key_event.repeat);

                let input_state = get_input_state(app);
                let modifiers = input_state.modifiers();

                input_state.set_key(key, key_event.state.is_pressed());
                app.push_event(ApplicationEvent::KeyInput(key, action, modifiers));

                if let Some(text) = key_event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        app.push_event(ApplicationEvent::CharInput(c));
                    }
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                get_input_state(app).set_modifiers(
                    input::winit_modifiers_to_input_modifiers(new_modifiers.state())
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = input::winit_mouse_scroll_delta_to_vec2(delta);
                app.push_event(ApplicationEvent::Scroll(scroll));
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if let Some(mb) = input::winit_mouse_button_to_mouse_button(button) {
                    let action = input::winit_element_state_to_input_action(state, false);

                    let input_state = get_input_state(app);
                    let modifiers = input_state.modifiers();
    
                    input_state.set_mouse_button(mb, state.is_pressed());
                    app.push_event(ApplicationEvent::MouseButton(mb, action, modifiers));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = app.content_scale();
                get_input_state(app).set_cursor_pos(Vec2::new(
                    position.x as f32 / scale.x,
                    position.y as f32 / scale.y,
                ));
            }
            _ => {}
        }
    }
}

fn get_input_state(app: &mut Application) -> &mut input::WinitInputState {
    // Get mutable access to the underlying WinitInputState for direct input mutation.
    match app.input_system_mut().backend_mut() {
        app::input::InputSystemBackendImpl::Winit(backend) => backend.input_state_mut(),
        #[allow(unreachable_patterns)]
        _ => panic!("WebRunner requires a Winit input backend!"),
    }
}
