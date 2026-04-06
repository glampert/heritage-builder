use common::{
    Size,
    Vec2,
    mem::{RcMut, RcRef},
};
use glfw::Context;
use smallvec::SmallVec;

use super::{ApplicationContentScale, ApplicationInitParams, ApplicationWindowMode};
use crate::{app::platform, log};

type GlfwEventReceiver = glfw::GlfwReceiver<(f64, glfw::WindowEvent)>;

// ----------------------------------------------
// GlfwWindowManager
// ----------------------------------------------

pub struct GlfwWindowManager {
    window_mode: ApplicationWindowMode,
    resizable_window: bool,
    confine_cursor: bool,
    content_scale: ApplicationContentScale,
    window: glfw::PWindow,
    event_receiver: GlfwEventReceiver,
}

pub type GlfwWindowManagerRcRef = RcRef<GlfwWindowManager>;
pub type GlfwWindowManagerRcMut = RcMut<GlfwWindowManager>;

impl GlfwWindowManager {
    pub fn new(glfw_instance: &mut glfw::Glfw, params: &ApplicationInitParams) -> GlfwWindowManagerRcMut {
        debug_assert!(params.window_size.is_valid());

        glfw_instance.window_hint(glfw::WindowHint::ContextVersion(3, 3));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

        // MacOS specific. Ignored otherwise.
        glfw_instance.window_hint(glfw::WindowHint::CocoaRetinaFramebuffer(true));  // We want High-DPI retina display always.
        glfw_instance.window_hint(glfw::WindowHint::CocoaGraphicsSwitching(false)); // Prefer the discrete GPU and avoid low-power integrated graphics.

        if params.window_mode.is_fullscreen() {
            // NOTE: FullScreen window mode requires resizable window on MacOS.
            glfw_instance.window_hint(glfw::WindowHint::Resizable(true));
        } else {
            glfw_instance.window_hint(glfw::WindowHint::Resizable(params.resizable_window));
        }

        let (window, event_receiver) = match params.window_mode {
            ApplicationWindowMode::Windowed => {
                create_windowed_window(glfw_instance, params.window_title, params.window_size)
            }
            ApplicationWindowMode::FullScreen => {
                create_fullscreen_window(glfw_instance, params.window_title, params.window_size)
            }
            ApplicationWindowMode::ExclusiveFullScreen => {
                create_exclusive_fullscreen_window(glfw_instance, params.window_title, params.window_size)
            }
        };

        let mut manager = Self {
            window_mode: params.window_mode,
            resizable_window: params.resizable_window,
            confine_cursor: params.confine_cursor,
            content_scale: params.content_scale,
            window,
            event_receiver,
        };

        manager.window.make_current();

        // Listen to these application events:
        manager.window.set_size_polling(params.resizable_window);
        manager.window.set_close_polling(true);
        manager.window.set_key_polling(true);
        manager.window.set_char_polling(true);
        manager.window.set_scroll_polling(true);
        manager.window.set_mouse_button_polling(true);

        // On MacOS `gl::load_with` generates a lot of TTY spam about missing
        // OpenGL functions that we don't need or care about. This is a workaround
        // to stop the TTY spam but still keep a record of the errors if ever
        // required for inspection.
        #[cfg(target_os = "macos")]
        {
            platform::redirect_stderr(
                || gl::load_with(|symbol| manager.window.get_proc_address(symbol)),
                "stderr_gl_load_app.log",
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            gl::load_with(|symbol| manager.window.get_proc_address(symbol));
        }

        GlfwWindowManagerRcMut::new(manager)
    }

    pub fn confine_cursor_to_window(&mut self) {
        if !self.confine_cursor {
            return;
        }

        let (x, y) = self.window.get_cursor_pos();
        let (width, height) = self.window.get_size();

        let mut new_x = x;
        let mut new_y = y;
        let mut changed = false;

        if x < 0.0 {
            new_x = 0.0;
            changed = true;
        } else if x > width as f64 {
            new_x = width as f64;
            changed = true;
        }

        if y < 0.0 {
            new_y = 0.0;
            changed = true;
        } else if y > height as f64 {
            new_y = height as f64;
            changed = true;
        }

        if changed {
            self.window.set_cursor_pos(new_x, new_y);
        }
    }

    #[inline]
    pub fn event_receiver(&self) -> &GlfwEventReceiver {
        &self.event_receiver
    }

    #[inline]
    pub fn window(&self) -> &glfw::Window {
        &self.window
    }

    #[inline]
    pub fn window_mut(&mut self) -> &mut glfw::Window {
        &mut self.window
    }

    #[inline]
    pub fn window_mode(&self) -> ApplicationWindowMode {
        self.window_mode
    }

    #[inline]
    pub fn content_scale(&self) -> Vec2 {
        match self.content_scale {
            ApplicationContentScale::System => {
                let (x_scale, y_scale) = self.window.get_content_scale();
                Vec2::new(x_scale, y_scale)
            }
            ApplicationContentScale::Custom(scale) => Vec2::new(scale, scale),
        }
    }

    #[inline]
    pub fn has_custom_content_scale(&self) -> bool {
        matches!(self.content_scale, ApplicationContentScale::Custom(_))
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn create_windowed_window(
    glfw_instance: &mut glfw::Glfw,
    window_title: &str,
    window_size: Size,
) -> (glfw::PWindow, GlfwEventReceiver) {
    glfw_instance
        .create_window(window_size.width as _, window_size.height as _, window_title, glfw::WindowMode::Windowed)
        .expect("Failed to create GLFW window in windowed mode!")
}

fn create_fullscreen_window(
    glfw_instance: &mut glfw::Glfw,
    window_title: &str,
    window_size: Size,
) -> (glfw::PWindow, GlfwEventReceiver) {
    // On MacOS, normal fullscreen is windowed with "kiosk" mode. I.e., a regular window
    // without decorations and hidden dock and system menu.
    #[cfg(target_os = "macos")]
    {
        let (mut window, event_receiver) = create_windowed_window(glfw_instance, window_title, window_size);

        let result: Result<(), &str> = glfw_instance.with_primary_monitor(|_, monitor_opt| {
            let monitor = monitor_opt.ok_or("No primary monitor found")?;
            let video_mode = monitor.get_video_mode().ok_or("No video mode available")?;

            window.set_decorated(false);
            window.set_pos(0, 0);
            window.set_size(video_mode.width as _, video_mode.height as _);
            Ok(())
        });

        match result {
            Ok(_) => {
                let ns_window_ptr = unsafe { glfw::ffi::glfwGetCocoaWindow(window.window_ptr()) };
                platform::toggle_native_fullscreen(ns_window_ptr);
                platform::enable_kiosk_mode();
            }
            Err(err) => {
                // App stays in windowed mode.
                log::error!(log::channel!("app"), "Failed to toggle fullscreen: {err}");
            }
        }

        (window, event_receiver)
    }

    // For other platforms assume exclusive fullscreen.
    #[cfg(not(target_os = "macos"))]
    {
        create_exclusive_fullscreen_window(glfw_instance, window_title, window_size)
    }
}

fn create_exclusive_fullscreen_window(
    glfw_instance: &mut glfw::Glfw,
    window_title: &str,
    window_size: Size,
) -> (glfw::PWindow, GlfwEventReceiver) {
    let result = glfw_instance.with_primary_monitor(|glfw_instance, monitor_opt| {
        let monitor = monitor_opt.ok_or("No primary monitor found")?;
        let video_modes = monitor.get_video_modes();

        log::verbose!(log::channel!("app"), "Fullscreen video modes available:");
        for mode in &video_modes {
            log::verbose!(log::channel!("app"), "{}x{} @ {}hz", mode.width, mode.height, mode.refresh_rate);
        }

        let best_video_mode = select_best_video_mode(&video_modes).ok_or("No suitable video mode available")?;

        log::info!(
            log::channel!("app"),
            "Attempting to create exclusive fullscreen window with video mode: {}x{} @ {}hz",
            best_video_mode.width,
            best_video_mode.height,
            best_video_mode.refresh_rate
        );

        glfw_instance
            .create_window(
                best_video_mode.width,
                best_video_mode.height,
                window_title,
                glfw::WindowMode::FullScreen(monitor),
            )
            .ok_or("Failed to create GLFW window")
    });

    match result {
        Ok((window, event_receiver)) => {
            let (ww, wh)   = window.get_size();
            let (fbw, fbh) = window.get_framebuffer_size();
            let (sx, sy)   = window.get_content_scale();
            log::info!(log::channel!("app"), "Fullscreen window OK - Win:({ww}x{wh}), Fb:({fbw}x{fbh}), Scale:({sx},{sy})");
            (window, event_receiver)
        }
        Err(err) => {
            log::error!(log::channel!("app"), "Failed to create fullscreen window: {err}");
            log::warning!(log::channel!("app"), "Falling back to windowed mode...");

            // Windowed fallback:
            create_windowed_window(glfw_instance, window_title, window_size)
        }
    }
}

// Selects the best fullscreen video mode for a monitor.
//  - Prefer highest pixel area (width * height)
//  - Prefer 60Hz if available at that resolution
//  - Otherwise prefer highest refresh rate
fn select_best_video_mode(modes: &[glfw::VidMode]) -> Option<glfw::VidMode> {
    if modes.is_empty() {
        return None;
    }

    // First, find the maximum resolution (by pixel area):
    let max_area = modes.iter().map(|mode| mode.width * mode.height).max()?;

    // Filter only modes with that resolution:
    let mut best_modes: SmallVec<[&glfw::VidMode; 16]> =
        modes.iter().filter(|mode| (mode.width * mode.height) == max_area).collect();

    // Prefer 60Hz exactly if available.
    if let Some(mode_60hz) = best_modes.iter().find(|mode| mode.refresh_rate == 60) {
        return Some(**mode_60hz);
    }

    // Otherwise pick highest refresh rate.
    best_modes.sort_by(|a, b| {
        a.refresh_rate
            .cmp(&b.refresh_rate)
            .then(a.width.cmp(&b.width))
            .then(a.height.cmp(&b.height))
    });
    best_modes.last().map(|mode| **mode)
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(w: u32, h: u32, r: u32) -> glfw::VidMode {
        glfw::VidMode {
            width: w,
            height: h,
            red_bits: 8,
            green_bits: 8,
            blue_bits: 8,
            refresh_rate: r,
        }
    }

    #[test]
    fn prefers_highest_resolution() {
        let modes = [
            vm(1920, 1080, 60),
            vm(2560, 1440, 60),
            vm(1920, 1080, 144),
        ];

        let best = select_best_video_mode(&modes).unwrap();
        assert_eq!(best.width, 2560);
        assert_eq!(best.height, 1440);
    }

    #[test]
    fn prefers_60hz_when_same_resolution() {
        let modes = [
            vm(2560, 1440, 144),
            vm(2560, 1440, 60),
        ];

        let best = select_best_video_mode(&modes).unwrap();
        assert_eq!(best.refresh_rate, 60);
    }
}
