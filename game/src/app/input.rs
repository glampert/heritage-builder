use std::any::Any;
use crate::utils::Vec2;

pub use super::{
    // Expose them here so we don't have to duplicate these enums.
    glfw::input::{InputAction, InputKey, InputModifiers, MouseButton}
};

// ----------------------------------------------
// InputSystem
// ----------------------------------------------

pub trait InputSystem: Any {
    fn as_any(&self) -> &dyn Any;
    fn cursor_pos(&self) -> Vec2;
    fn mouse_button_state(&self, button: MouseButton) -> InputAction;
    fn key_state(&self, key: InputKey) -> InputAction;
}
