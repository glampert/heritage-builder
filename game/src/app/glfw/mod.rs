use std::{any::Any, ffi::c_void};
use glfw::Context;

use super::{
    input::InputSystem,
    Application, ApplicationFactory,
    ApplicationEvent, ApplicationEventList,
    ApplicationWindowMode, ApplicationContentScale,
};
use crate::{
    log,
    utils::{mem, Size, Vec2},
};

mod window;
use window::{GlfwWindowManager, GlfwWindowManagerRcMut};

pub mod input;
use input::GlfwInputSystem;

// ----------------------------------------------
// GlfwApplication
// ----------------------------------------------

pub struct GlfwApplication {
    should_quit: bool,
    glfw_instance: glfw::Glfw,
    window_manager: GlfwWindowManagerRcMut,
    input_system: GlfwInputSystem,
}

impl GlfwApplication {
    // Used by the ImGui OpenGL backend.
    pub fn load_gl_func(&self, func_name: &'static str) -> *const c_void {
        let app = mem::mut_ref_cast(self);
        app.window_manager.load_gl_func(func_name)
    }
}

impl ApplicationFactory for GlfwApplication {
    fn new(window_title: &str,
           window_size: Size,
           window_mode: ApplicationWindowMode,
           resizable_window: bool,
           confine_cursor: bool,
           content_scale: ApplicationContentScale) -> Self
    {
        let mut glfw_instance = glfw::init(glfw::fail_on_errors)
            .expect("Failed to initialize GLFW!");

        let window_manager = GlfwWindowManager::new(
            &mut glfw_instance,
            window_title,
            window_size,
            window_mode,
            resizable_window,
            confine_cursor,
            content_scale
        );

        // NOTE: window_manager is an Rc, so its address is stable.
        let input_system = GlfwInputSystem::new(window_manager.clone().into_not_mut());

        Self {
            should_quit: false,
            glfw_instance,
            window_manager,
            input_system,
        }
    }
}

impl Application for GlfwApplication {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn request_quit(&mut self) {
        self.window_manager.window_mut().set_should_close(true);
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        self.glfw_instance.poll_events();

        let mut translated_events = ApplicationEventList::new();

        for (_, event) in glfw::flush_messages(self.window_manager.event_receiver()) {
            // NOTE: To receive events here we must call window.set_<event>_polling().
            // See set_size_polling/set_close_polling/etc calls in GlfwWindowManager.
            match event {
                glfw::WindowEvent::Size(width, height) => {
                    translated_events.push(ApplicationEvent::WindowResize {
                        window_size: Size::new(width, height),
                        framebuffer_size: self.framebuffer_size(),
                    });
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

        self.window_manager.confine_cursor_to_window();

        translated_events
    }

    #[inline]
    fn present(&mut self) {
        self.window_manager.window_mut().swap_buffers();
    }

    #[inline]
    fn window_size(&self) -> Size {
        // NOTE: Assume window size is equal to framebuffer size divided by content scale.
        let scale = self.window_manager.content_scale();
        let (width, height) = self.window_manager.window().get_framebuffer_size();
        Size::new((width as f32 / scale.x) as i32, (height as f32 / scale.y) as i32)
    }

    #[inline]
    fn framebuffer_size(&self) -> Size {
        let (width, height) = self.window_manager.window().get_framebuffer_size();
        Size::new(width, height)
    }

    #[inline]
    fn content_scale(&self) -> Vec2 {
        self.window_manager.content_scale()
    }

    #[inline]
    fn input_system(&self) -> &dyn InputSystem {
        &self.input_system
    }
}
