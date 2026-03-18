use std::num::NonZeroU32;
use smallvec::SmallVec;

use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version},
    display::{GetGlDisplay, GlDisplay},
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Fullscreen, Window, WindowAttributes},
    raw_window_handle::HasWindowHandle,
};

use super::{
    ApplicationWindowMode,
    ApplicationContentScale,
};
use crate::{
    log,
    app::platform,
    utils::{Size, Vec2},
};

// ----------------------------------------------
// WinitWindowManager
// ----------------------------------------------

pub struct WinitWindowManager {
    pub window: Window,
    pub resizable: bool,
    pub gl_context: PossiblyCurrentContext,
    pub gl_surface: Surface<WindowSurface>,

    window_mode: ApplicationWindowMode,
    confine_cursor: bool,
    content_scale: ApplicationContentScale,
}

impl WinitWindowManager {
    // Called from inside `ApplicationHandler::resumed()` during init.
    pub fn create(event_loop: &ActiveEventLoop,
                  window_title: &str,
                  window_size: Size,
                  window_mode: ApplicationWindowMode,
                  resizable: bool,
                  confine_cursor: bool,
                  content_scale: ApplicationContentScale) -> Self
    {
        debug_assert!(window_size.is_valid());

        // Fullscreen mode requires a resizable window attribute on some platforms.
        let needs_resizable = resizable || window_mode.is_fullscreen();
        let fullscreen = select_fullscreen(event_loop, window_mode);

        let window_attributes = WindowAttributes::default()
            .with_title(window_title)
            .with_inner_size(LogicalSize::new(window_size.width as f64, window_size.height as f64))
            .with_resizable(needs_resizable)
            .with_fullscreen(fullscreen);

        let config_template = ConfigTemplateBuilder::new()
            .with_alpha_size(8);

        let display_builder = DisplayBuilder::new()
            .with_window_attributes(Some(window_attributes));

        let (window_opt, gl_config) = display_builder
            .build(event_loop, config_template, |configs| {
                configs
                    .reduce(|best, config| {
                        if config.num_samples() > best.num_samples() { config } else { best }
                    })
                    .expect("No suitable GL config found!")
            })
            .expect("Failed to build winit window with GL display!");

        let window = window_opt.expect("Window was not created during display build!");

        let raw_window_handle = window.window_handle()
            .expect("Failed to get window handle!")
            .as_raw();

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .with_profile(GlProfile::Core)
            .build(Some(raw_window_handle));

        let not_current_context = unsafe {
            gl_config.display()
                .create_context(&gl_config, &context_attributes)
                .expect("Failed to create OpenGL context!")
        };

        // Build surface attributes using the glutin_winit helper.
        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::new())
            .expect("Failed to build surface attributes!");

        let gl_surface = unsafe {
            gl_config.display()
                .create_window_surface(&gl_config, &surface_attributes)
                .expect("Failed to create GL window surface!")
        };

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("Failed to make GL context current!");

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
        platform::redirect_stderr(load_gl, "stderr_gl_load_app.log");

        #[cfg(not(target_os = "macos"))]
        load_gl();

        log::info!(log::channel!("app"), "WinitWindowManager initialized.");
        log::info!(log::channel!("app"), "Window Size: {window_size}");

        Self {
            window,
            resizable,
            gl_context,
            gl_surface,
            window_mode,
            confine_cursor,
            content_scale,
        }
    }

    // Clamps `cursor_pos` (in logical pixels, content-area relative) to the
    // window bounds and warps the OS cursor if it was out of bounds.
    // Returns the clamped position when a warp was needed, `None` otherwise.
    // Only active when cursor confinement is enabled.
    pub fn try_confine_cursor(&self, cursor_pos: Vec2) -> Option<Vec2> {
        if !self.confine_cursor {
            return None;
        }

        let size = self.window_size();

        let mut new_x = cursor_pos.x;
        let mut new_y = cursor_pos.y;
        let mut changed = false;

        if cursor_pos.x < 0.0 {
            new_x = 0.0;
            changed = true;
        } else if cursor_pos.x > size.width as f32 {
            new_x = size.width as f32;
            changed = true;
        }

        if cursor_pos.y < 0.0 {
            new_y = 0.0;
            changed = true;
        } else if cursor_pos.y > size.height as f32 {
            new_y = size.height as f32;
            changed = true;
        }

        if changed {
            set_cursor_position_native(&self.window, new_x as f64, new_y as f64);
            Some(Vec2::new(new_x, new_y))
        } else {
            None
        }
    }

    // Warps the OS cursor to `pos` (logical pixels, content-area relative).
    // Used to push the cursor back in when `CursorLeft` fires while
    // confinement is active (the title bar is outside the content view,
    // so winit stops sending `CursorMoved` there, bypassing normal clamping).
    pub fn warp_cursor_to_pos(&self, pos: Vec2) {
        if self.confine_cursor {
            set_cursor_position_native(&self.window, pos.x as f64, pos.y as f64);
        }
    }

    #[inline]
    pub fn window_size(&self) -> Size {
        match self.content_scale {
            ApplicationContentScale::System => {
                let logical = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
                Size::new(logical.width as i32, logical.height as i32)
            }
            ApplicationContentScale::Custom(scale) => {
                let phys = self.window.inner_size();
                Size::new((phys.width as f32 / scale) as i32, (phys.height as f32 / scale) as i32)
            }
        }
    }

    #[inline]
    pub fn framebuffer_size(&self) -> Size {
        let phys = self.window.inner_size();
        Size::new(phys.width as i32, phys.height as i32)
    }

    #[inline]
    pub fn content_scale(&self) -> Vec2 {
        match self.content_scale {
            ApplicationContentScale::System => {
                let scale = self.window.scale_factor() as f32;
                Vec2::new(scale, scale)
            }
            ApplicationContentScale::Custom(scale) => {
                Vec2::new(scale, scale)
            }
        }
    }

    #[inline]
    pub fn has_custom_content_scale(&self) -> bool {
        matches!(self.content_scale, ApplicationContentScale::Custom(_))
    }

    #[inline]
    pub fn window_mode(&self) -> ApplicationWindowMode {
        self.window_mode
    }

    pub fn resize_surface(&self, width: u32, height: u32) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            self.gl_surface.resize(&self.gl_context, w, h);
        }
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn select_fullscreen(event_loop: &ActiveEventLoop,
                     window_mode: ApplicationWindowMode)
                     -> Option<Fullscreen>
{
    match window_mode {
        ApplicationWindowMode::FullScreen => {
            // Borderless fullscreen on the primary monitor.
            Some(Fullscreen::Borderless(event_loop.primary_monitor()))
        }
        ApplicationWindowMode::ExclusiveFullScreen => {
            // Attempt to select the best video mode on the primary monitor.
            let monitor = event_loop.primary_monitor()?;
            let video_mode = select_best_video_mode(monitor.video_modes())?;
            Some(Fullscreen::Exclusive(video_mode))
        }
        ApplicationWindowMode::Windowed => None,
    }
}

// Selects the best exclusive fullscreen video mode:
//  - Prefer highest pixel area
//  - Prefer 60 Hz if available at that resolution
//  - Otherwise prefer highest refresh rate
fn select_best_video_mode<I>(modes: I) -> Option<winit::monitor::VideoModeHandle>
    where I: Iterator<Item = winit::monitor::VideoModeHandle>
{
    let all: SmallVec<[winit::monitor::VideoModeHandle; 16]> = modes.collect();
    if all.is_empty() {
        return None;
    }

    let max_area = all
        .iter()
        .map(|m| m.size().width * m.size().height)
        .max()?;

    let mut best: SmallVec<[&winit::monitor::VideoModeHandle; 16]> = all
        .iter()
        .filter(|m| m.size().width * m.size().height == max_area)
        .collect();

    if let Some(mode_60hz) = best.iter().find(|m| m.refresh_rate_millihertz() == 60_000) {
        return Some((*mode_60hz).clone());
    }

    best.sort_by_key(|m| m.refresh_rate_millihertz());
    best.last().map(|m| (*m).clone())
}

// ----------------------------------------------
// Cursor positioning
// ----------------------------------------------

// On MacOS, winit's set_cursor_position is not supported.
// CGWarpMouseCursorPosition uses CG global coordinates: top-left origin, Y-down,
// in logical points (same space as window.inner_position() / scale_factor).
// We compute the target screen position by adding the content-area offset to the
// cursor coordinates — both are already in that same top-left, Y-down space.
#[cfg(target_os = "macos")]
fn set_cursor_position_native(window: &Window, x: f64, y: f64) {
    // inner_position() is the top-left of the content area in physical pixels,
    // CG coordinate space (top-left origin, Y-down).
    let Ok(inner_pos) = window.inner_position() else { return };
    let scale = window.scale_factor();

    platform::warp_cursor(
        (inner_pos.x as f64 / scale) + x,
        (inner_pos.y as f64 / scale) + y,
    );
}

#[cfg(not(target_os = "macos"))]
fn set_cursor_position_native(window: &Window, x: f64, y: f64) {
    // winit's built-in set_cursor_position works on Windows and Linux.
    let _ = window.set_cursor_position(winit::dpi::LogicalPosition::new(x, y));
}
