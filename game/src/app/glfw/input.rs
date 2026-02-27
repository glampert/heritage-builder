use std::any::Any;

use super::{
    window::GlfwWindowManagerRcRef,
};
use crate::{
    utils::Vec2,
    app::input::InputSystem,
};

// ----------------------------------------------
// These will be exposed as public types in the
// app::input module, so we don't have to
// replicate all the GLFW enums.
// ----------------------------------------------

pub type InputModifiers = glfw::Modifiers;
pub type InputAction    = glfw::Action;
pub type InputKey       = glfw::Key;
pub type MouseButton    = glfw::MouseButton;

// ----------------------------------------------
// GlfwInputSystem
// ----------------------------------------------

pub struct GlfwInputSystem {
    window_manager: GlfwWindowManagerRcRef,
}

impl GlfwInputSystem {
    pub fn new(window_manager: GlfwWindowManagerRcRef) -> Self {
        Self { window_manager }
    }
}

impl InputSystem for GlfwInputSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn cursor_pos(&self) -> Vec2 {
        let (x, y) = self.window_manager.window().get_cursor_pos();
        let mut pos = Vec2::new(x as f32, y as f32);

        if self.window_manager.has_custom_content_scale() {
            let scale = self.window_manager.content_scale();
            pos /= scale;
        }

        pos
    }

    #[inline]
    fn mouse_button_state(&self, button: MouseButton) -> InputAction {
        self.window_manager.window().get_mouse_button(button)
    }

    #[inline]
    fn key_state(&self, key: InputKey) -> InputAction {
        self.window_manager.window().get_key(key)
    }
}
