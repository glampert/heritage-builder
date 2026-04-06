use common::{Size, Vec2};
use glfw::Context;

use super::{
    ApplicationApi,
    ApplicationBackend,
    ApplicationContentScale,
    ApplicationEvent,
    ApplicationEventList,
    ApplicationInitParams,
    ApplicationWindowMode,
    input::{InputSystem, InputSystemBackendImpl},
};
use crate::{log, render::RenderApi};

mod window;
use window::{GlfwWindowManager, GlfwWindowManagerRcMut};

mod input;
pub use input::GlfwInputSystemBackend;

// ----------------------------------------------
// GlfwApplicationBackend
// ----------------------------------------------

pub struct GlfwApplicationBackend {
    should_quit: bool,
    glfw_instance: glfw::Glfw,
    window_manager: GlfwWindowManagerRcMut,
}

impl GlfwApplicationBackend {
    pub fn new(params: &ApplicationInitParams) -> Self {
        assert!(params.app_api == ApplicationApi::Glfw);
        assert!(params.render_api == RenderApi::OpenGl);

        log::info!(log::channel!("app"), "--- App Backend: GLFW ---");

        let mut glfw_instance = glfw::init(glfw::fail_on_errors).expect("Failed to initialize GLFW!");

        let window_manager = GlfwWindowManager::new(&mut glfw_instance, params);

        Self { should_quit: false, glfw_instance, window_manager }
    }
}

impl ApplicationBackend for GlfwApplicationBackend {
    fn new_input_system(&mut self) -> InputSystem {
        let input_system = GlfwInputSystemBackend::new(self.window_manager.clone().into_not_mut());

        InputSystem::new(InputSystemBackendImpl::Glfw(input_system))
    }

    fn app_context(&self) -> Option<&dyn std::any::Any> {
        None
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn request_quit(&mut self) {
        self.window_manager.window_mut().set_should_close(true);
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        self.glfw_instance.poll_events();

        let mut events = ApplicationEventList::new();

        for (_, event) in glfw::flush_messages(self.window_manager.event_receiver()) {
            // NOTE: To receive events here we must call window.set_<event>_polling().
            // See set_size_polling/set_close_polling/etc calls in GlfwWindowManager.
            match event {
                glfw::WindowEvent::Size(width, height) => {
                    events.push(ApplicationEvent::WindowResize {
                        window_size: Size::new(width, height),
                        framebuffer_size: self.framebuffer_size(),
                    });
                }
                glfw::WindowEvent::Close => {
                    events.push(ApplicationEvent::Quit);
                }
                glfw::WindowEvent::Key(key, _scan_code, action, modifiers) => {
                    events.push(ApplicationEvent::KeyInput(
                        input::glfw_key_to_input_key(key),
                        input::glfw_action_to_input_action(action),
                        input::glfw_modifiers_to_input_modifiers(modifiers),
                    ));
                }
                glfw::WindowEvent::Char(c) => {
                    events.push(ApplicationEvent::CharInput(c));
                }
                glfw::WindowEvent::Scroll(x, y) => {
                    events.push(ApplicationEvent::Scroll(Vec2::new(x as f32, y as f32)));
                }
                glfw::WindowEvent::MouseButton(button, action, modifiers) => {
                    events.push(ApplicationEvent::MouseButton(
                        input::glfw_mouse_button_to_mouse_button(button),
                        input::glfw_action_to_input_action(action),
                        input::glfw_modifiers_to_input_modifiers(modifiers),
                    ));
                }
                unhandled_event => {
                    log::warning!(log::channel!("app"), "Unhandled GLFW window event: {unhandled_event:?}");
                }
            }
        }

        self.window_manager.confine_cursor_to_window();

        events
    }

    fn present(&mut self) {
        self.window_manager.window_mut().swap_buffers();
    }

    fn window_size(&self) -> Size {
        // NOTE: Assume window size is equal to framebuffer size divided by content scale.
        let scale = self.window_manager.content_scale();
        let (width, height) = self.window_manager.window().get_framebuffer_size();
        Size::new((width as f32 / scale.x) as i32, (height as f32 / scale.y) as i32)
    }

    fn framebuffer_size(&self) -> Size {
        let (width, height) = self.window_manager.window().get_framebuffer_size();
        Size::new(width, height)
    }

    fn content_scale(&self) -> Vec2 {
        self.window_manager.content_scale()
    }
}
