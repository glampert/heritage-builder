use std::any::Any;
use crate::utils::Vec2;

// Expose them here so we don't have to duplicate these enums.
pub use super::glfw::InputAction;
pub use super::glfw::InputKey;
pub use super::glfw::InputModifiers;
pub use super::glfw::MouseButton;

// ----------------------------------------------
// InputSystem
// ----------------------------------------------

pub trait InputSystem: Any {
    fn as_any(&self) -> &dyn Any;
    fn cursor_pos(&self) -> Vec2;
    fn mouse_button_state(&self, button: MouseButton) -> InputAction;
    fn key_state(&self, key: InputKey) -> InputAction;
}
