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

    #[inline(always)]
    fn get_window(&self) -> &glfw::Window {
        self.window_manager.window()
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
