use std::{any::Any, ffi::c_void};
use smallvec::SmallVec;

use glfw::Context;

use super::{
    input::InputSystem, Application, ApplicationEvent, ApplicationEventList, ApplicationFactory,
};
use crate::{
    log,
    utils::{self, mem, Size, Vec2},
};

// ----------------------------------------------
// These will be exposed as public types in the
// app::input module, so we don't have to
// replicate all the GLFW enums.
// ----------------------------------------------

pub type InputModifiers = glfw::Modifiers;
pub type InputAction = glfw::Action;
pub type InputKey = glfw::Key;
pub type MouseButton = glfw::MouseButton;

// ----------------------------------------------
// GlfwApplication
// ----------------------------------------------

pub struct GlfwApplication {
    fullscreen: bool,
    confine_cursor: bool,
    should_quit: bool,
    glfw_instance: glfw::Glfw,
    window: glfw::PWindow,
    event_receiver: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
    input_system: GlfwInputSystem,
}

impl GlfwApplication {
    // For the ImGui OpenGL backend.
    pub fn load_gl_func(&self, func_name: &'static str) -> *const c_void {
        let app = mem::mut_ref_cast(self);
        app.window.get_proc_address(func_name)
    }
}

impl ApplicationFactory for GlfwApplication {
    fn new(title: &str, window_size: Size, mut fullscreen: bool, confine_cursor: bool, resizable_window: bool) -> Self {
        debug_assert!(window_size.is_valid());

        let mut glfw_instance = glfw::init(glfw::fail_on_errors)
            .expect("Failed to initialize GLFW!");

        glfw_instance.window_hint(glfw::WindowHint::ContextVersion(3, 3));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
        glfw_instance.window_hint(glfw::WindowHint::Resizable(resizable_window));

        // MacOS specific. Ignored otherwise.
        glfw_instance.window_hint(glfw::WindowHint::CocoaRetinaFramebuffer(true));  // We want High-DPI retina display always.
        glfw_instance.window_hint(glfw::WindowHint::CocoaGraphicsSwitching(false)); // Prefer the discrete GPU and avoid low-power integrated graphics.

        let result = if fullscreen {
            glfw_instance.with_primary_monitor(|glfw_instance, monitor_opt| {
                let monitor = monitor_opt.ok_or("No primary monitor found")?;
                let video_modes = monitor.get_video_modes();

                log::verbose!(log::channel!("app"), "Fullscreen Video Modes available:");
                for mode in &video_modes {
                    log::verbose!("{}x{} @ {}hz", mode.width, mode.height, mode.refresh_rate);
                }

                let best_video_mode = select_best_video_mode(&video_modes)
                    .ok_or("No video mode available")?;

                log::info!(log::channel!("app"),
                           "Attempting to create fullscreen window with video mode: {}x{} @ {}hz",
                           best_video_mode.width,
                           best_video_mode.height,
                           best_video_mode.refresh_rate);

                glfw_instance.create_window(best_video_mode.width,
                                            best_video_mode.height,
                                            title,
                                            glfw::WindowMode::FullScreen(monitor))
                                            .ok_or("Failed to create GLFW window")
            }).inspect(|(window, _)| {
                let (ww, wh)   = window.get_size();
                let (fbw, fbh) = window.get_framebuffer_size();
                let (sx, sy)   = window.get_content_scale();
                log::info!(log::channel!("app"), "Fullscreen window OK - Size:({ww}x{wh}), Fb:({fbw}x{fbh}), Scale:({sx},{sy})");
            }).inspect_err(|err| {
                log::error!(log::channel!("app"), "Failed to create fullscreen window: {err}");
            }).ok()
        } else {
            None
        };

        let (mut window, event_receiver) =
            result.unwrap_or_else(|| {
                // Windowed fallback.
                fullscreen = false;
                glfw_instance.create_window(window_size.width  as u32,
                                            window_size.height as u32,
                                            title,
                                            glfw::WindowMode::Windowed)
                                            .expect("Failed to create GLFW window in windowed mode!")
            });

        window.make_current();

        // Listen to these application events:
        window.set_size_polling(resizable_window);
        window.set_close_polling(true);
        window.set_key_polling(true);
        window.set_char_polling(true);
        window.set_scroll_polling(true);
        window.set_mouse_button_polling(true);

        // On MacOS this generates a lot of TTY spam about missing
        // OpenGL functions that we don't need or care about. This
        // is a hack to stop the TTY spamming but still keep a record
        // of the errors if ever required for inspection.
        utils::platform::macos_redirect_stderr(|| {
            gl::load_with(|symbol| {
                window.get_proc_address(symbol)
            })
        },
        "stderr_gl_load_app.log");

        // NOTE: PWindow is a Box<Window>, so the address is stable.
        let window_ptr = mem::RawPtr::from_ref(&*window);

        Self {
            fullscreen,
            confine_cursor,
            should_quit: false,
            glfw_instance,
            window,
            event_receiver,
            input_system: GlfwInputSystem { window_ptr },
        }
    }
}

impl Application for GlfwApplication {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn request_quit(&mut self) {
        self.window.set_should_close(true);
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        self.glfw_instance.poll_events();

        let mut translated_events = ApplicationEventList::new();

        for (_, event) in glfw::flush_messages(&self.event_receiver) {
            // NOTE: To receive events here we must call set_<event>_polling().
            // See set_size_polling/set_close_polling calls above.
            match event {
                glfw::WindowEvent::Size(width, height) => {
                    translated_events.push(ApplicationEvent::WindowResize(Size::new(width, height)));
                }
                glfw::WindowEvent::Close => {
                    translated_events.push(ApplicationEvent::Quit);
                }
                glfw::WindowEvent::Key(key, _scan_code, action, modifiers) => {
                    translated_events.push(ApplicationEvent::KeyInput(key, action, modifiers));
                }
                glfw::WindowEvent::Char(c) => {
                    translated_events.push(ApplicationEvent::CharInput(c));
                }
                glfw::WindowEvent::Scroll(x, y) => {
                    translated_events.push(ApplicationEvent::Scroll(Vec2::new(x as f32, y as f32)));
                }
                glfw::WindowEvent::MouseButton(button, action, modifiers) => {
                    translated_events.push(ApplicationEvent::MouseButton(button, action, modifiers));
                }
                unhandled_event => {
                    log::warning!(log::channel!("app"), "Unhandled GLFW window event: {unhandled_event:?}");
                }
            }
        }

        if self.confine_cursor {
            confine_cursor_to_window(&mut self.window);
        }

        translated_events
    }

    fn present(&mut self) {
        self.window.swap_buffers();
    }

    #[inline]
    fn window_size(&self) -> Size {
        let (width, height) = self.window.get_size();
        Size::new(width, height)
    }

    #[inline]
    fn framebuffer_size(&self) -> Size {
        let (width, height) = self.window.get_framebuffer_size();
        Size::new(width, height)
    }

    #[inline]
    fn content_scale(&self) -> Vec2 {
        let (x_scale, y_scale) = self.window.get_content_scale();
        Vec2::new(x_scale, y_scale)
    }

    #[inline]
    fn input_system(&self) -> &dyn InputSystem {
        &self.input_system
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn confine_cursor_to_window(window: &mut glfw::Window) {
    let (x, y) = window.get_cursor_pos();
    let (width, height) = window.get_size();

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
        window.set_cursor_pos(new_x, new_y);
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
    let max_area = modes
        .iter()
        .map(|mode| mode.width * mode.height)
        .max()?;

    // Filter only modes with that resolution:
    let mut best_modes: SmallVec<[&glfw::VidMode; 16]> = modes
        .iter()
        .filter(|mode| (mode.width * mode.height) == max_area)
        .collect();

    // Prefer 60Hz exactly if available.
    if let Some(mode_60hz) = best_modes
        .iter()
        .find(|mode| mode.refresh_rate == 60)
    {
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
// GlfwInputSystem
// ----------------------------------------------

pub struct GlfwInputSystem {
    // SAFETY: Application Window will persist for as long as InputSystem.
    window_ptr: mem::RawPtr<glfw::Window>,
}

impl GlfwInputSystem {
    #[inline]
    fn get_window(&self) -> &glfw::Window {
        &self.window_ptr
    }
}

impl InputSystem for GlfwInputSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn cursor_pos(&self) -> Vec2 {
        let (x, y) = self.get_window().get_cursor_pos();
        Vec2::new(x as f32, y as f32)
    }

    #[inline]
    fn mouse_button_state(&self, button: MouseButton) -> InputAction {
        self.get_window().get_mouse_button(button)
    }

    #[inline]
    fn key_state(&self, key: InputKey) -> InputAction {
        self.get_window().get_key(key)
    }
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
