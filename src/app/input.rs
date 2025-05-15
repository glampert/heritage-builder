// Internal implementation.
use super::glfw::GlfwInputSystem;

// Expose them here so we don't have to duplicate these enums.
pub use super::glfw::InputModifiers;
pub use super::glfw::InputAction;
pub use super::glfw::InputKey;
pub use super::glfw::MouseButton;

use super::Application;
use crate::utils::Point2D;

// ----------------------------------------------
// InputSystem
// ----------------------------------------------

pub trait InputSystem {
    fn cursor_pos(&self) -> Point2D;
    fn mouse_button_state(&self, button: MouseButton) -> InputAction;
    fn key_state(&self, key: InputKey) -> InputAction;
}

// ----------------------------------------------
// new_input_system() factory function
// ----------------------------------------------

pub fn new_input_system<T: Application>(app: &T) -> impl InputSystem + use<T> {
    GlfwInputSystem::new(app)
}
