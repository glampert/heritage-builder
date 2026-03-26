use bitflags::bitflags;
use strum::EnumCount;
use enum_dispatch::enum_dispatch;
use crate::utils::Vec2;

// ----------------------------------------------
// Internal backend implementations
// ----------------------------------------------

#[enum_dispatch]
pub(super) enum InputSystemBackendImpl {
    Winit(super::winit::WinitInputSystemBackend),
    Glfw(super::glfw::GlfwInputSystemBackend),
}

// ----------------------------------------------
// InputSystemBackend
// ----------------------------------------------

#[enum_dispatch(InputSystemBackendImpl)]
pub(super) trait InputSystemBackend: Sized {
    fn cursor_pos(&self) -> Vec2;
    fn mouse_button_state(&self, button: MouseButton) -> InputAction;
    fn key_state(&self, key: InputKey) -> InputAction;
}

// ----------------------------------------------
// InputSystem
// ----------------------------------------------

pub struct InputSystem {
    backend: InputSystemBackendImpl,
}

impl InputSystem {
    pub(super) fn new(backend: InputSystemBackendImpl) -> Self {
        Self { backend }
    }

    #[inline]
    pub fn cursor_pos(&self) -> Vec2 {
        self.backend.cursor_pos()
    }

    #[inline]
    pub fn mouse_button_state(&self, button: MouseButton) -> InputAction {
        self.backend.mouse_button_state(button)
    }

    #[inline]
    pub fn key_state(&self, key: InputKey) -> InputAction {
        self.backend.key_state(key)
    }
}

// ----------------------------------------------
// InputAction
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputAction {
    #[default]
    Release,
    Repeat,
    Press,
}

// ----------------------------------------------
// InputModifiers
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct InputModifiers: u8 {
        const Shift    = 1 << 0;
        const Control  = 1 << 1;
        const Alt      = 1 << 2;
        const Super    = 1 << 3;
        const CapsLock = 1 << 4;
        const NumLock  = 1 << 5;
    }
}

// ----------------------------------------------
// MouseButton
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumCount)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Unknown,
}

// ----------------------------------------------
// InputKey
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumCount)]
pub enum InputKey {
    Unknown,

    // Printable characters
    Space,
    Apostrophe,
    Comma,
    Minus,
    Period,
    Slash,
    Num0, Num1, Num2, Num3, Num4,
    Num5, Num6, Num7, Num8, Num9,
    Semicolon,
    Equal,
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    LeftBracket,
    Backslash,
    RightBracket,
    GraveAccent,

    // Navigation / editing
    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Right,
    Left,
    Down,
    Up,
    PageUp,
    PageDown,
    Home,
    End,

    // Toggles
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,

    // Function keys
    F1,  F2,  F3,  F4,  F5,  F6,  F7,  F8,  F9,  F10, F11, F12,
    F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24, F25,

    // Keypad
    Kp0, Kp1, Kp2, Kp3, Kp4,
    Kp5, Kp6, Kp7, Kp8, Kp9,
    KpDecimal,
    KpDivide,
    KpMultiply,
    KpSubtract,
    KpAdd,
    KpEnter,
    KpEqual,

    // Modifiers
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    Menu,
}
