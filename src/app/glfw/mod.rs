use std::ffi::c_void;
use glfw::Context;
use crate::app::{Application, ApplicationEvent};
use crate::utils::{Size2D, Point2D, Vec2};

// These will be exposed as public types in the app module
// so we don't have to replicate all the GLFW enums.
pub type InputModifiers = glfw::Modifiers;
pub type InputAction = glfw::Action;
pub type InputKey = glfw::Key;
pub type MouseButton = glfw::MouseButton;

// For the ImGui OpenGL backend.
pub fn load_gl_func<T: Application>(app: &mut T, func_name: &'static str) -> *const c_void {
    unsafe {
        debug_assert!(std::mem::size_of::<T>() == std::mem::size_of::<GlfwApplication>());
        let glfw_app = &mut *(app as *mut T as *mut GlfwApplication);
        glfw_app.window.get_proc_address(func_name) as *const c_void
    }
}

pub struct GlfwApplication {
    title: String,
    window_size: Size2D,
    fullscreen: bool,
    should_quit: bool,
    glfw_instance: glfw::Glfw,
    window: glfw::PWindow,
    event_receiver: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
}

impl GlfwApplication {
    pub fn new(title: String, window_size: Size2D, fullscreen: bool) -> Self {
        debug_assert!(window_size.is_valid());

        let mut glfw_instance =
            glfw::init(glfw::fail_on_errors).expect("Failed to initialize GLFW!");

        glfw_instance.window_hint(glfw::WindowHint::ContextVersion(3, 3));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw_instance.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

        // TODO: Handle fullscreen window (need to select a monitor).
        let window_mode = glfw::WindowMode::Windowed;
        if fullscreen {
            eprintln!("GLFW fullscreen window support not implemented!");
        }

        let (mut window, event_receiver) = glfw_instance
            .create_window(window_size.width as u32, window_size.height as u32, title.as_str(), window_mode)
            .expect("Failed to create GLFW window!");

        window.make_current();

        // Listen to these application events:
        window.set_size_polling(true);
        window.set_close_polling(true);
        window.set_key_polling(true);
        window.set_char_polling(true);
        window.set_scroll_polling(true);

        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

        GlfwApplication {
            title: title,
            window_size: window_size,
            fullscreen: fullscreen,
            should_quit: false,
            glfw_instance: glfw_instance,
            window: window,
            event_receiver: event_receiver,
        }
    }
}

impl Application for GlfwApplication {
    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn request_quit(&mut self) {
        self.window.set_should_close(true);
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> Vec<ApplicationEvent> {
        self.glfw_instance.poll_events();

        let mut translated_events = Vec::new();

        for (_, event) in glfw::flush_messages(&self.event_receiver) {
            // NOTE: To receive events here we must call set_<event>_polling().
            // See set_size_polling/set_close_polling calls above.
            match event {
                glfw::WindowEvent::Size(width, height) => {
                    self.window_size.width = width;
                    self.window_size.height = height;
                    translated_events.push(ApplicationEvent::WindowResize(Size2D { width: width, height: height }));
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
                    translated_events.push(ApplicationEvent::Scroll(Vec2 { x: x as f32, y: y as f32 }));
                }
                unhandled_event => {
                    eprintln!("Unhandled GLFW window event: {:?}", unhandled_event);
                }
            }
        }

        translated_events
    }

    fn present(&mut self) {
        self.window.swap_buffers();
    }

    fn cursor_pos(&self) -> Point2D {
        let (x, y) = self.window.get_cursor_pos();
        Point2D::with_coords(x as i32, y as i32)
    }

    fn button_state(&self, button: MouseButton) -> InputAction {
        self.window.get_mouse_button(button)
    }

    fn key_state(&self, key: InputKey) -> InputAction {
        self.window.get_key(key)
    }

    fn window_size(&self) -> Size2D {
        self.window_size
    }

    fn framebuffer_size(&self) -> Size2D {
        let (width, height) = self.window.get_framebuffer_size();
        Size2D { width: width, height: height }
    }

    fn content_scale(&self) -> Vec2 {
        let (x_scale, y_scale) = self.window.get_content_scale();
        Vec2 { x: x_scale, y: y_scale }
    }
}
