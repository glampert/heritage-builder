use strum::EnumCount;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::{
    utils::{Vec2, mem::RcMut},
    app::input::{
        InputSystemBackend,
        InputAction, InputKey, InputModifiers, MouseButton,
    },
};

// ----------------------------------------------
// WinitInputState
// ----------------------------------------------

const INPUT_KEY_COUNT: usize = InputKey::COUNT;
const MOUSE_BUTTON_COUNT: usize = MouseButton::COUNT;

// Tracks current input state, updated by WinitApplication during poll_events().
pub struct WinitInputState {
    cursor_pos: Vec2,
    modifiers: InputModifiers,
    key_states: [bool; INPUT_KEY_COUNT],
    mouse_button_states: [bool; MOUSE_BUTTON_COUNT],
}

impl WinitInputState {
    pub fn new() -> RcMut<Self> {
        RcMut::new(Self {
            cursor_pos: Vec2::zero(),
            modifiers: InputModifiers::empty(),
            key_states: [false; INPUT_KEY_COUNT],
            mouse_button_states: [false; MOUSE_BUTTON_COUNT],
        })
    }

    #[inline]
    pub fn modifiers(&self) -> InputModifiers {
        self.modifiers
    }

    #[inline]
    pub fn set_modifiers(&mut self, modifiers: InputModifiers) {
        self.modifiers = modifiers;
    }

    #[inline]
    pub fn cursor_pos(&self) -> Vec2 {
        self.cursor_pos
    }

    #[inline]
    pub fn set_cursor_pos(&mut self, pos: Vec2) {
        self.cursor_pos = pos;
    }

    #[inline]
    pub fn set_key(&mut self, key: InputKey, pressed: bool) {
        self.key_states[key as usize] = pressed;
    }

    #[inline]
    pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        self.mouse_button_states[button as usize] = pressed;
    }

    #[inline]
    pub fn is_key_pressed(&self, key: InputKey) -> bool {
        self.key_states[key as usize]
    }

    #[inline]
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.mouse_button_states[button as usize]
    }
}

// ----------------------------------------------
// WinitInputSystemBackend
// ----------------------------------------------

pub struct WinitInputSystemBackend {
    state: RcMut<WinitInputState>,
}

impl WinitInputSystemBackend {
    pub fn new(state: RcMut<WinitInputState>) -> Self {
        Self { state }
    }

    // Direct access to the underlying input state for platform-specific mutation (WebRunner).
    #[cfg(feature = "web")]
    #[inline] pub(crate) fn input_state_mut(&mut self) -> &mut WinitInputState {
        &mut self.state
    }
}

impl InputSystemBackend for WinitInputSystemBackend {
    #[inline]
    fn cursor_pos(&self) -> Vec2 {
        self.state.cursor_pos
    }

    #[inline]
    fn mouse_button_state(&self, button: MouseButton) -> InputAction {
        if self.state.is_mouse_button_pressed(button) {
            InputAction::Press
        } else {
            InputAction::Release
        }
    }

    #[inline]
    fn key_state(&self, key: InputKey) -> InputAction {
        if self.state.is_key_pressed(key) {
            InputAction::Press
        } else {
            InputAction::Release
        }
    }
}

// ----------------------------------------------
// Conversion helpers
// ----------------------------------------------

pub fn winit_key_code_to_input_key(code: KeyCode) -> InputKey {
    match code {
        KeyCode::Space        => InputKey::Space,
        KeyCode::Quote        => InputKey::Apostrophe,
        KeyCode::Comma        => InputKey::Comma,
        KeyCode::Minus        => InputKey::Minus,
        KeyCode::Period       => InputKey::Period,
        KeyCode::Slash        => InputKey::Slash,
        KeyCode::Digit0       => InputKey::Num0,
        KeyCode::Digit1       => InputKey::Num1,
        KeyCode::Digit2       => InputKey::Num2,
        KeyCode::Digit3       => InputKey::Num3,
        KeyCode::Digit4       => InputKey::Num4,
        KeyCode::Digit5       => InputKey::Num5,
        KeyCode::Digit6       => InputKey::Num6,
        KeyCode::Digit7       => InputKey::Num7,
        KeyCode::Digit8       => InputKey::Num8,
        KeyCode::Digit9       => InputKey::Num9,
        KeyCode::Semicolon    => InputKey::Semicolon,
        KeyCode::Equal        => InputKey::Equal,
        KeyCode::KeyA         => InputKey::A,
        KeyCode::KeyB         => InputKey::B,
        KeyCode::KeyC         => InputKey::C,
        KeyCode::KeyD         => InputKey::D,
        KeyCode::KeyE         => InputKey::E,
        KeyCode::KeyF         => InputKey::F,
        KeyCode::KeyG         => InputKey::G,
        KeyCode::KeyH         => InputKey::H,
        KeyCode::KeyI         => InputKey::I,
        KeyCode::KeyJ         => InputKey::J,
        KeyCode::KeyK         => InputKey::K,
        KeyCode::KeyL         => InputKey::L,
        KeyCode::KeyM         => InputKey::M,
        KeyCode::KeyN         => InputKey::N,
        KeyCode::KeyO         => InputKey::O,
        KeyCode::KeyP         => InputKey::P,
        KeyCode::KeyQ         => InputKey::Q,
        KeyCode::KeyR         => InputKey::R,
        KeyCode::KeyS         => InputKey::S,
        KeyCode::KeyT         => InputKey::T,
        KeyCode::KeyU         => InputKey::U,
        KeyCode::KeyV         => InputKey::V,
        KeyCode::KeyW         => InputKey::W,
        KeyCode::KeyX         => InputKey::X,
        KeyCode::KeyY         => InputKey::Y,
        KeyCode::KeyZ         => InputKey::Z,
        KeyCode::BracketLeft  => InputKey::LeftBracket,
        KeyCode::Backslash    => InputKey::Backslash,
        KeyCode::BracketRight => InputKey::RightBracket,
        KeyCode::Backquote    => InputKey::GraveAccent,
        KeyCode::Escape       => InputKey::Escape,
        KeyCode::Enter        => InputKey::Enter,
        KeyCode::Tab          => InputKey::Tab,
        KeyCode::Backspace    => InputKey::Backspace,
        KeyCode::Insert       => InputKey::Insert,
        KeyCode::Delete       => InputKey::Delete,
        KeyCode::ArrowRight   => InputKey::Right,
        KeyCode::ArrowLeft    => InputKey::Left,
        KeyCode::ArrowDown    => InputKey::Down,
        KeyCode::ArrowUp      => InputKey::Up,
        KeyCode::PageUp       => InputKey::PageUp,
        KeyCode::PageDown     => InputKey::PageDown,
        KeyCode::Home         => InputKey::Home,
        KeyCode::End          => InputKey::End,
        KeyCode::CapsLock     => InputKey::CapsLock,
        KeyCode::ScrollLock   => InputKey::ScrollLock,
        KeyCode::NumLock      => InputKey::NumLock,
        KeyCode::PrintScreen  => InputKey::PrintScreen,
        KeyCode::Pause        => InputKey::Pause,
        KeyCode::F1           => InputKey::F1,
        KeyCode::F2           => InputKey::F2,
        KeyCode::F3           => InputKey::F3,
        KeyCode::F4           => InputKey::F4,
        KeyCode::F5           => InputKey::F5,
        KeyCode::F6           => InputKey::F6,
        KeyCode::F7           => InputKey::F7,
        KeyCode::F8           => InputKey::F8,
        KeyCode::F9           => InputKey::F9,
        KeyCode::F10          => InputKey::F10,
        KeyCode::F11          => InputKey::F11,
        KeyCode::F12          => InputKey::F12,
        KeyCode::F13          => InputKey::F13,
        KeyCode::F14          => InputKey::F14,
        KeyCode::F15          => InputKey::F15,
        KeyCode::F16          => InputKey::F16,
        KeyCode::F17          => InputKey::F17,
        KeyCode::F18          => InputKey::F18,
        KeyCode::F19          => InputKey::F19,
        KeyCode::F20          => InputKey::F20,
        KeyCode::F21          => InputKey::F21,
        KeyCode::F22          => InputKey::F22,
        KeyCode::F23          => InputKey::F23,
        KeyCode::F24          => InputKey::F24,
        KeyCode::F25          => InputKey::F25,
        KeyCode::Numpad0      => InputKey::Kp0,
        KeyCode::Numpad1      => InputKey::Kp1,
        KeyCode::Numpad2      => InputKey::Kp2,
        KeyCode::Numpad3      => InputKey::Kp3,
        KeyCode::Numpad4      => InputKey::Kp4,
        KeyCode::Numpad5      => InputKey::Kp5,
        KeyCode::Numpad6      => InputKey::Kp6,
        KeyCode::Numpad7      => InputKey::Kp7,
        KeyCode::Numpad8      => InputKey::Kp8,
        KeyCode::Numpad9      => InputKey::Kp9,
        KeyCode::NumpadDecimal  => InputKey::KpDecimal,
        KeyCode::NumpadDivide   => InputKey::KpDivide,
        KeyCode::NumpadMultiply => InputKey::KpMultiply,
        KeyCode::NumpadSubtract => InputKey::KpSubtract,
        KeyCode::NumpadAdd      => InputKey::KpAdd,
        KeyCode::NumpadEnter    => InputKey::KpEnter,
        KeyCode::NumpadEqual    => InputKey::KpEqual,
        KeyCode::ShiftLeft      => InputKey::LeftShift,
        KeyCode::ControlLeft    => InputKey::LeftControl,
        KeyCode::AltLeft        => InputKey::LeftAlt,
        KeyCode::SuperLeft      => InputKey::LeftSuper,
        KeyCode::ShiftRight     => InputKey::RightShift,
        KeyCode::ControlRight   => InputKey::RightControl,
        KeyCode::AltRight       => InputKey::RightAlt,
        KeyCode::SuperRight     => InputKey::RightSuper,
        KeyCode::ContextMenu    => InputKey::Menu,
        _                       => InputKey::Unknown,
    }
}

pub fn winit_physical_key_to_input_key(key: PhysicalKey) -> InputKey {
    match key {
        PhysicalKey::Code(code) => winit_key_code_to_input_key(code),
        PhysicalKey::Unidentified(_) => InputKey::Unknown,
    }
}

pub fn winit_modifiers_to_input_modifiers(state: winit::keyboard::ModifiersState) -> InputModifiers {
    let mut result = InputModifiers::empty();
    if state.shift_key()   { result |= InputModifiers::Shift;   }
    if state.control_key() { result |= InputModifiers::Control; }
    if state.alt_key()     { result |= InputModifiers::Alt;     }
    if state.super_key()   { result |= InputModifiers::Super;   }
    result
}

pub fn winit_mouse_button_to_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    Some(match button {
        winit::event::MouseButton::Left     => MouseButton::Left,
        winit::event::MouseButton::Right    => MouseButton::Right,
        winit::event::MouseButton::Middle   => MouseButton::Middle,
        winit::event::MouseButton::Back     => MouseButton::Back,
        winit::event::MouseButton::Forward  => MouseButton::Forward,
        winit::event::MouseButton::Other(_) => return None,
    })
}

pub fn winit_element_state_to_input_action(state: winit::event::ElementState, repeat: bool) -> InputAction {
    match state {
        winit::event::ElementState::Pressed if repeat => InputAction::Repeat,
        winit::event::ElementState::Pressed  => InputAction::Press,
        winit::event::ElementState::Released => InputAction::Release,
    }
}

pub fn winit_mouse_scroll_delta_to_vec2(delta: winit::event::MouseScrollDelta) -> Vec2 {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(x, y) => {
            Vec2::new(x, y)
        }
        winit::event::MouseScrollDelta::PixelDelta(pos) => {
            // Convert pixel delta to approximate line counts.
            Vec2::new(pos.x as f32 / 20.0, pos.y as f32 / 20.0)
        }
    }
}

// ----------------------------------------------
// Cursor positioning
// ----------------------------------------------

#[cfg(feature = "desktop")]
pub mod cursor {
    // On MacOS, winit's set_cursor_position is not supported.
    // CGWarpMouseCursorPosition uses CG global coordinates: top-left origin, Y-down,
    // in logical points (same space as window.inner_position() / scale_factor).
    // We compute the target screen position by adding the content-area offset to the
    // cursor coordinates — both are already in that same top-left, Y-down space.
    #[cfg(target_os = "macos")]
    pub fn set_position_native(window: &winit::window::Window, x: f64, y: f64) {
        // inner_position() is the top-left of the content area in physical pixels,
        // CG coordinate space (top-left origin, Y-down).
        let Ok(inner_pos) = window.inner_position() else { return };
        let scale = window.scale_factor();

        crate::app::platform::set_cursor_position(
            (inner_pos.x as f64 / scale) + x,
            (inner_pos.y as f64 / scale) + y,
        );
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_position_native(window: &winit::window::Window, x: f64, y: f64) {
        // winit's built-in set_cursor_position works on Windows and Linux.
        let _ = window.set_cursor_position(winit::dpi::LogicalPosition::new(x, y));
    }
}
