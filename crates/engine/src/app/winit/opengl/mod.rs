use std::num::NonZeroU32;

use common::{Size, Vec2};
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version},
    display::{GetGlDisplay, GlDisplay},
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::pump_events::EventLoopExtPumpEvents,
    raw_window_handle::HasWindowHandle,
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    app::{ApplicationApi, ApplicationInitParams},
    log,
    render::RenderApi,
};

// ----------------------------------------------
// WinitWindowManager (OpenGL)
// ----------------------------------------------

pub struct WinitWindowManager {
    window: Window,
    event_loop: EventLoop<()>,
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
}

impl WinitWindowManager {
    pub fn new(params: &ApplicationInitParams) -> Self {
        assert!(params.app_api == ApplicationApi::Winit);
        assert!(params.render_api == RenderApi::OpenGl);

        let mut event_loop = EventLoop::new().expect("Failed to create Winit event loop!");

        // Create the window and GL context during the first pump (triggers `resumed()`).
        let mut init_handler = WinitInitHandler::new(params);

        // Pump events once to trigger `resumed()`, which creates the window + GL context.
        let _ = event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut init_handler);

        let (window, gl_context, gl_surface) =
            init_handler.result.expect("Winit: Window initialization failed — resumed() was not triggered!");

        log::info!(log::channel!("app"), "WinitWindowManager (OpenGL) created.");

        Self { window, event_loop, gl_context, gl_surface }
    }
}

impl super::WinitWindowManager for WinitWindowManager {
    fn window(&self) -> &Window {
        &self.window
    }

    fn app_context(&self) -> Option<&dyn std::any::Any> {
        None
    }

    fn resize_framebuffer(&mut self, new_size: Size) {
        if new_size.is_valid() {
            self.gl_surface.resize(
                &self.gl_context,
                NonZeroU32::new(new_size.width as u32).unwrap(),
                NonZeroU32::new(new_size.height as u32).unwrap(),
            );
        }
    }

    fn present(&mut self) {
        self.gl_surface.swap_buffers(&self.gl_context).expect("Failed to swap GL buffers!");
    }

    fn poll_events<F>(&mut self, handler: F)
    where
        F: FnMut(&ActiveEventLoop, WindowEvent),
    {
        let mut evt_handler = WinitWindowEventHandler { window_id: self.window.id(), handler };

        let _ = self.event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut evt_handler);
    }

    fn set_cursor_position(&mut self, pos: Vec2) {
        super::input::cursor::set_position_native(&self.window, pos.x as f64, pos.y as f64);
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Called from inside `WinitInitHandler::resumed()` during init.
fn create_window_and_gl_context(
    event_loop: &ActiveEventLoop,
    params: &ApplicationInitParams,
) -> Option<(Window, PossiblyCurrentContext, Surface<WindowSurface>)> {
    // Fullscreen mode requires a resizable window attribute on some platforms.
    let needs_resizable = params.resizable_window || params.window_mode.is_fullscreen();
    let fullscreen = super::select_fullscreen(event_loop, params.window_mode);

    let window_attributes = WindowAttributes::default()
        .with_title(params.window_title)
        .with_inner_size(winit::dpi::LogicalSize::new(params.window_size.width as f64, params.window_size.height as f64))
        .with_resizable(needs_resizable)
        .with_fullscreen(fullscreen);

    let config_template = ConfigTemplateBuilder::new().with_alpha_size(8).with_api(glutin::config::Api::OPENGL);

    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window_opt, gl_config) = display_builder
        .build(event_loop, config_template, |configs| {
            configs
                .reduce(|best, config| if config.num_samples() > best.num_samples() { config } else { best })
                .expect("No suitable GL config found!")
        })
        .expect("Failed to build Winit window with GL display!");

    let window = window_opt.expect("Winit Window was not created during display build!");

    let raw_window_handle = window.window_handle().expect("Failed to get window handle!").as_raw();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .with_profile(GlProfile::Core)
        .build(Some(raw_window_handle));

    let not_current_context = unsafe {
        gl_config.display().create_context(&gl_config, &context_attributes).expect("Failed to create OpenGL context!")
    };

    // Build surface attributes using the glutin_winit helper.
    let surface_attributes =
        window.build_surface_attributes(SurfaceAttributesBuilder::new()).expect("Failed to build surface attributes!");

    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &surface_attributes)
            .expect("Failed to create GL window surface!")
    };

    let gl_context = not_current_context.make_current(&gl_surface).expect("Failed to make GL context current!");

    // On MacOS `gl::load_with` generates a lot of TTY spam about missing
    // OpenGL functions that we don't need or care about. This is a workaround
    // to stop the TTY spam but still keep a record of the errors if ever
    // required for inspection.
    let load_gl = || {
        gl::load_with(|symbol| {
            // Avoid a heap allocation per symbol: copy the name + null terminator
            // onto the stack. No GL function name exceeds 64 bytes.
            let bytes = symbol.as_bytes();
            debug_assert!(bytes.len() < 128, "GL symbol too long: {symbol}");

            let mut buf = [0u8; 128];
            buf[..bytes.len()].copy_from_slice(bytes);

            // SAFETY: buf ends with a null byte and `symbol` is a valid C
            // identifier (no interior nulls).
            let c_str = unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(&buf[..=bytes.len()]) };
            gl_config.display().get_proc_address(c_str).cast()
        });
    };

    #[cfg(target_os = "macos")]
    crate::app::platform::redirect_stderr(load_gl, "stderr_gl_load_app.log");

    #[cfg(not(target_os = "macos"))]
    load_gl();

    log::info!(log::channel!("app"), "Winit Window + OpenGL Context created.");
    log::info!(log::channel!("app"), "Window Inner Size: {:?}, Outer Size: {:?}", window.inner_size(), window.outer_size());

    Some((window, gl_context, gl_surface))
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

// Creates window + GL context in resumed().
struct WinitInitHandler<'a> {
    params: &'a ApplicationInitParams<'a>,
    result: Option<(Window, PossiblyCurrentContext, Surface<WindowSurface>)>,
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

        self.result = create_window_and_gl_context(event_loop, self.params);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window_id: WindowId, _event: WindowEvent) {}
}
