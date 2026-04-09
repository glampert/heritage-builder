use std::sync::Arc;

use common::{Size, Vec2};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};
#[cfg(feature = "desktop")]
use winit::{event_loop::EventLoop, platform::pump_events::EventLoopExtPumpEvents};

use crate::{
    app::{ApplicationApi, ApplicationInitParams},
    log,
    render::RenderApi,
};

// ----------------------------------------------
// WinitWindowManager (Wgpu)
// ----------------------------------------------

// Window manager for the Winit + Wgpu backend.
// Unlike the OpenGL variant, this does NOT create a GL context.
// The Wgpu RenderSystem creates its own surface/device from the
// window (via app_context).
pub struct WinitWindowManager {
    window: Arc<Window>,

    // Desktop only: owns the event loop for synchronous pump_app_events().
    // On web the browser owns the event loop; WebRunner drives events externally.
    #[cfg(feature = "desktop")]
    event_loop: EventLoop<()>,
}

impl WinitWindowManager {
    pub fn new(params: &ApplicationInitParams) -> Self {
        assert!(params.app_api == ApplicationApi::Winit);
        assert!(params.render_api == RenderApi::Wgpu);

        #[cfg(feature = "desktop")]
        {
            let mut event_loop = EventLoop::new().expect("Failed to create Winit event loop!");

            // Create the window during the first pump (triggers `resumed()`).
            let mut init_handler = WinitInitHandler::new(params);

            // Pump events once to trigger `resumed()`, which creates the window.
            let _ = event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut init_handler);

            let window = init_handler.result.expect("Winit: Window initialization failed — resumed() was not triggered!");

            // On MacOS, Winit's default app menu wires CMD+Q to `terminate:`,
            // which kills the process without giving us a clean shutdown.
            // Rewire it to `performClose:` so it surfaces as CloseRequested.
            #[cfg(target_os = "macos")]
            crate::app::platform::rewire_quit_menu_item_to_close();

            log::info!(log::channel!("app"), "WinitWindowManager (Wgpu) created.");

            Self { window: Arc::new(window), event_loop }
        }

        #[cfg(feature = "web")]
        {
            let window: Arc<winit::window::Window> = params
                .opt_window
                .expect("Web WinitWindowManager requires an opt_window!")
                .downcast_ref::<Arc<winit::window::Window>>()
                .expect("opt_window must be Arc<winit::window::Window>!")
                .clone();

            // Web: wrap a pre-created window (created by WebRunner inside resumed()).
            Self { window }
        }
    }
}

impl super::WinitWindowManager for WinitWindowManager {
    fn window(&self) -> &Window {
        &self.window
    }

    fn app_context(&self) -> Option<&dyn std::any::Any> {
        // Expose the Arc<Window> for the Wgpu RenderSystem to create a surface.
        Some(&self.window)
    }

    fn resize_framebuffer(&mut self, _new_size: Size) {
        // No surface resize needed; the Wgpu RenderSystem
        // reconfigures its surface in set_framebuffer_size().
    }

    fn present(&mut self) {
        // No-op for Wgpu: surface presentation is handled by the RenderSystem.
    }

    fn poll_events<F>(&mut self, handler: F)
    where
        F: FnMut(&ActiveEventLoop, WindowEvent),
    {
        // Desktop: synchronous event pump.
        #[cfg(feature = "desktop")]
        {
            let mut evt_handler = WinitWindowEventHandler { window_id: self.window.id(), handler };
            let _ = self.event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut evt_handler);
        }

        // Web: no-op — WebRunner drives events via ApplicationHandler.
        #[cfg(feature = "web")]
        { let _ = handler; }
    }

    fn set_cursor_position(&mut self, pos: Vec2) {
        #[cfg(feature = "desktop")]
        super::input::cursor::set_position_native(&self.window, pos.x as f64, pos.y as f64);

        #[cfg(feature = "web")]
        { let _ = pos; } // Unavailable.
    }
}

// ----------------------------------------------
// Window creation helpers
// ----------------------------------------------

// Create a winit window from an ActiveEventLoop.
// Used by both the desktop WinitInitHandler and the WebRunner.
pub fn create_window(event_loop: &ActiveEventLoop, params: &ApplicationInitParams) -> Window {
    // Fullscreen mode requires a resizable window attribute on some platforms.
    let needs_resizable = params.resizable_window || params.window_mode.is_fullscreen();
    let fullscreen = super::select_fullscreen(event_loop, params.window_mode);

    #[allow(unused_mut)]
    let mut window_attributes = WindowAttributes::default()
        .with_title(params.window_title)
        .with_inner_size(winit::dpi::LogicalSize::new(params.window_size.width as f64, params.window_size.height as f64))
        .with_resizable(needs_resizable)
        .with_fullscreen(fullscreen);

    // On WASM, attach to the HTML canvas element.
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowAttributesExtWebSys;

        let canvas = web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.get_element_by_id("game-canvas"))
            .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("Failed to find <canvas id='game-canvas'> element!");

        window_attributes = window_attributes.with_canvas(Some(canvas));
    }

    let window = event_loop.create_window(window_attributes).expect("Failed to create Winit window!");

    log::info!(log::channel!("app"), "Winit Window for Wgpu created.");
    log::info!(log::channel!("app"), "Window Inner Size: {:?}, Outer Size: {:?}", window.inner_size(), window.outer_size());

    window
}

// ----------------------------------------------
// WinitWindowEventHandler
// ----------------------------------------------

// Handles window events for the window with specified id only.
struct WinitWindowEventHandler<F> {
    window_id: WindowId,
    handler: F,
}

impl<F> ApplicationHandler for WinitWindowEventHandler<F>
where
    F: FnMut(&ActiveEventLoop, WindowEvent),
{
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Window is already created; nothing to do during normal polling.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if window_id != self.window_id {
            return;
        }

        (self.handler)(event_loop, event);
    }
}

// ----------------------------------------------
// WinitInitHandler
// ----------------------------------------------

// Creates window in resumed(). Used on desktop where we pump
// the event loop synchronously during initialization.
struct WinitInitHandler<'a> {
    params: &'a ApplicationInitParams<'a>,
    result: Option<Window>,
}

impl<'a> WinitInitHandler<'a> {
    fn new(params: &'a ApplicationInitParams<'a>) -> Self {
        Self { params, result: None }
    }
}

impl ApplicationHandler for WinitInitHandler<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return; // Already initialized (e.g. app resumed after suspend on mobile).
        }

        self.result = Some(create_window(event_loop, self.params));
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window_id: WindowId, _event: WindowEvent) {}
}
