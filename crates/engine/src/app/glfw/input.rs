use super::window::GlfwWindowManagerRcRef;
use common::Vec2;
use crate::app::input::{
    InputSystemBackend,
    InputAction, InputKey, InputModifiers, MouseButton,
};

// ----------------------------------------------
// GlfwInputSystemBackend
// ----------------------------------------------

pub struct GlfwInputSystemBackend {
    window_manager: GlfwWindowManagerRcRef,
}

impl GlfwInputSystemBackend {
    pub fn new(window_manager: GlfwWindowManagerRcRef) -> Self {
        Self { window_manager }
    }
}

impl InputSystemBackend for GlfwInputSystemBackend {
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
        if let Some(glfw_button) = mouse_button_to_glfw(button) {
            glfw_action_to_input_action(self.window_manager.window().get_mouse_button(glfw_button))
        } else {
            InputAction::default()
        }
    }

    #[inline]
    fn key_state(&self, key: InputKey) -> InputAction {
        let glfw_key = input_key_to_glfw(key);
        glfw_action_to_input_action(self.window_manager.window().get_key(glfw_key))
    }
}

// ----------------------------------------------
// Conversion helpers (pub(super) for glfw/mod.rs)
// ----------------------------------------------

pub(super) fn glfw_key_to_input_key(key: glfw::Key) -> InputKey {
    match key {
        glfw::Key::Space        => InputKey::Space,
        glfw::Key::Apostrophe   => InputKey::Apostrophe,
        glfw::Key::Comma        => InputKey::Comma,
        glfw::Key::Minus        => InputKey::Minus,
        glfw::Key::Period       => InputKey::Period,
        glfw::Key::Slash        => InputKey::Slash,
        glfw::Key::Num0         => InputKey::Num0,
        glfw::Key::Num1         => InputKey::Num1,
        glfw::Key::Num2         => InputKey::Num2,
        glfw::Key::Num3         => InputKey::Num3,
        glfw::Key::Num4         => InputKey::Num4,
        glfw::Key::Num5         => InputKey::Num5,
        glfw::Key::Num6         => InputKey::Num6,
        glfw::Key::Num7         => InputKey::Num7,
        glfw::Key::Num8         => InputKey::Num8,
        glfw::Key::Num9         => InputKey::Num9,
        glfw::Key::Semicolon    => InputKey::Semicolon,
        glfw::Key::Equal        => InputKey::Equal,
        glfw::Key::A            => InputKey::A,
        glfw::Key::B            => InputKey::B,
        glfw::Key::C            => InputKey::C,
        glfw::Key::D            => InputKey::D,
        glfw::Key::E            => InputKey::E,
        glfw::Key::F            => InputKey::F,
        glfw::Key::G            => InputKey::G,
        glfw::Key::H            => InputKey::H,
        glfw::Key::I            => InputKey::I,
        glfw::Key::J            => InputKey::J,
        glfw::Key::K            => InputKey::K,
        glfw::Key::L            => InputKey::L,
        glfw::Key::M            => InputKey::M,
        glfw::Key::N            => InputKey::N,
        glfw::Key::O            => InputKey::O,
        glfw::Key::P            => InputKey::P,
        glfw::Key::Q            => InputKey::Q,
        glfw::Key::R            => InputKey::R,
        glfw::Key::S            => InputKey::S,
        glfw::Key::T            => InputKey::T,
        glfw::Key::U            => InputKey::U,
        glfw::Key::V            => InputKey::V,
        glfw::Key::W            => InputKey::W,
        glfw::Key::X            => InputKey::X,
        glfw::Key::Y            => InputKey::Y,
        glfw::Key::Z            => InputKey::Z,
        glfw::Key::LeftBracket  => InputKey::LeftBracket,
        glfw::Key::Backslash    => InputKey::Backslash,
        glfw::Key::RightBracket => InputKey::RightBracket,
        glfw::Key::GraveAccent  => InputKey::GraveAccent,
        glfw::Key::Escape       => InputKey::Escape,
        glfw::Key::Enter        => InputKey::Enter,
        glfw::Key::Tab          => InputKey::Tab,
        glfw::Key::Backspace    => InputKey::Backspace,
        glfw::Key::Insert       => InputKey::Insert,
        glfw::Key::Delete       => InputKey::Delete,
        glfw::Key::Right        => InputKey::Right,
        glfw::Key::Left         => InputKey::Left,
        glfw::Key::Down         => InputKey::Down,
        glfw::Key::Up           => InputKey::Up,
        glfw::Key::PageUp       => InputKey::PageUp,
        glfw::Key::PageDown     => InputKey::PageDown,
        glfw::Key::Home         => InputKey::Home,
        glfw::Key::End          => InputKey::End,
        glfw::Key::CapsLock     => InputKey::CapsLock,
        glfw::Key::ScrollLock   => InputKey::ScrollLock,
        glfw::Key::NumLock      => InputKey::NumLock,
        glfw::Key::PrintScreen  => InputKey::PrintScreen,
        glfw::Key::Pause        => InputKey::Pause,
        glfw::Key::F1           => InputKey::F1,
        glfw::Key::F2           => InputKey::F2,
        glfw::Key::F3           => InputKey::F3,
        glfw::Key::F4           => InputKey::F4,
        glfw::Key::F5           => InputKey::F5,
        glfw::Key::F6           => InputKey::F6,
        glfw::Key::F7           => InputKey::F7,
        glfw::Key::F8           => InputKey::F8,
        glfw::Key::F9           => InputKey::F9,
        glfw::Key::F10          => InputKey::F10,
        glfw::Key::F11          => InputKey::F11,
        glfw::Key::F12          => InputKey::F12,
        glfw::Key::F13          => InputKey::F13,
        glfw::Key::F14          => InputKey::F14,
        glfw::Key::F15          => InputKey::F15,
        glfw::Key::F16          => InputKey::F16,
        glfw::Key::F17          => InputKey::F17,
        glfw::Key::F18          => InputKey::F18,
        glfw::Key::F19          => InputKey::F19,
        glfw::Key::F20          => InputKey::F20,
        glfw::Key::F21          => InputKey::F21,
        glfw::Key::F22          => InputKey::F22,
        glfw::Key::F23          => InputKey::F23,
        glfw::Key::F24          => InputKey::F24,
        glfw::Key::F25          => InputKey::F25,
        glfw::Key::Kp0          => InputKey::Kp0,
        glfw::Key::Kp1          => InputKey::Kp1,
        glfw::Key::Kp2          => InputKey::Kp2,
        glfw::Key::Kp3          => InputKey::Kp3,
        glfw::Key::Kp4          => InputKey::Kp4,
        glfw::Key::Kp5          => InputKey::Kp5,
        glfw::Key::Kp6          => InputKey::Kp6,
        glfw::Key::Kp7          => InputKey::Kp7,
        glfw::Key::Kp8          => InputKey::Kp8,
        glfw::Key::Kp9          => InputKey::Kp9,
        glfw::Key::KpDecimal    => InputKey::KpDecimal,
        glfw::Key::KpDivide     => InputKey::KpDivide,
        glfw::Key::KpMultiply   => InputKey::KpMultiply,
        glfw::Key::KpSubtract   => InputKey::KpSubtract,
        glfw::Key::KpAdd        => InputKey::KpAdd,
        glfw::Key::KpEnter      => InputKey::KpEnter,
        glfw::Key::KpEqual      => InputKey::KpEqual,
        glfw::Key::LeftShift    => InputKey::LeftShift,
        glfw::Key::LeftControl  => InputKey::LeftControl,
        glfw::Key::LeftAlt      => InputKey::LeftAlt,
        glfw::Key::LeftSuper    => InputKey::LeftSuper,
        glfw::Key::RightShift   => InputKey::RightShift,
        glfw::Key::RightControl => InputKey::RightControl,
        glfw::Key::RightAlt     => InputKey::RightAlt,
        glfw::Key::RightSuper   => InputKey::RightSuper,
        glfw::Key::Menu         => InputKey::Menu,
        _                       => InputKey::Unknown,
    }
}

pub(super) fn glfw_action_to_input_action(action: glfw::Action) -> InputAction {
    match action {
        glfw::Action::Press   => InputAction::Press,
        glfw::Action::Release => InputAction::Release,
        glfw::Action::Repeat  => InputAction::Repeat,
    }
}

pub(super) fn glfw_modifiers_to_input_modifiers(mods: glfw::Modifiers) -> InputModifiers {
    let mut result = InputModifiers::empty();
    if mods.contains(glfw::Modifiers::Shift)    { result |= InputModifiers::Shift;    }
    if mods.contains(glfw::Modifiers::Control)  { result |= InputModifiers::Control;  }
    if mods.contains(glfw::Modifiers::Alt)      { result |= InputModifiers::Alt;      }
    if mods.contains(glfw::Modifiers::Super)    { result |= InputModifiers::Super;    }
    if mods.contains(glfw::Modifiers::CapsLock) { result |= InputModifiers::CapsLock; }
    if mods.contains(glfw::Modifiers::NumLock)  { result |= InputModifiers::NumLock;  }
    result
}

pub(super) fn glfw_mouse_button_to_mouse_button(button: glfw::MouseButton) -> MouseButton {
    match button {
        glfw::MouseButton::Button1 => MouseButton::Left,
        glfw::MouseButton::Button2 => MouseButton::Right,
        glfw::MouseButton::Button3 => MouseButton::Middle,
        glfw::MouseButton::Button4 => MouseButton::Back,
        glfw::MouseButton::Button5 => MouseButton::Forward,
        _ => MouseButton::Unknown,
    }
}

// Reverse conversions for key_state / mouse_button_state polling:

fn input_key_to_glfw(key: InputKey) -> glfw::Key {
    match key {
        InputKey::Space        => glfw::Key::Space,
        InputKey::Apostrophe   => glfw::Key::Apostrophe,
        InputKey::Comma        => glfw::Key::Comma,
        InputKey::Minus        => glfw::Key::Minus,
        InputKey::Period       => glfw::Key::Period,
        InputKey::Slash        => glfw::Key::Slash,
        InputKey::Num0         => glfw::Key::Num0,
        InputKey::Num1         => glfw::Key::Num1,
        InputKey::Num2         => glfw::Key::Num2,
        InputKey::Num3         => glfw::Key::Num3,
        InputKey::Num4         => glfw::Key::Num4,
        InputKey::Num5         => glfw::Key::Num5,
        InputKey::Num6         => glfw::Key::Num6,
        InputKey::Num7         => glfw::Key::Num7,
        InputKey::Num8         => glfw::Key::Num8,
        InputKey::Num9         => glfw::Key::Num9,
        InputKey::Semicolon    => glfw::Key::Semicolon,
        InputKey::Equal        => glfw::Key::Equal,
        InputKey::A            => glfw::Key::A,
        InputKey::B            => glfw::Key::B,
        InputKey::C            => glfw::Key::C,
        InputKey::D            => glfw::Key::D,
        InputKey::E            => glfw::Key::E,
        InputKey::F            => glfw::Key::F,
        InputKey::G            => glfw::Key::G,
        InputKey::H            => glfw::Key::H,
        InputKey::I            => glfw::Key::I,
        InputKey::J            => glfw::Key::J,
        InputKey::K            => glfw::Key::K,
        InputKey::L            => glfw::Key::L,
        InputKey::M            => glfw::Key::M,
        InputKey::N            => glfw::Key::N,
        InputKey::O            => glfw::Key::O,
        InputKey::P            => glfw::Key::P,
        InputKey::Q            => glfw::Key::Q,
        InputKey::R            => glfw::Key::R,
        InputKey::S            => glfw::Key::S,
        InputKey::T            => glfw::Key::T,
        InputKey::U            => glfw::Key::U,
        InputKey::V            => glfw::Key::V,
        InputKey::W            => glfw::Key::W,
        InputKey::X            => glfw::Key::X,
        InputKey::Y            => glfw::Key::Y,
        InputKey::Z            => glfw::Key::Z,
        InputKey::LeftBracket  => glfw::Key::LeftBracket,
        InputKey::Backslash    => glfw::Key::Backslash,
        InputKey::RightBracket => glfw::Key::RightBracket,
        InputKey::GraveAccent  => glfw::Key::GraveAccent,
        InputKey::Escape       => glfw::Key::Escape,
        InputKey::Enter        => glfw::Key::Enter,
        InputKey::Tab          => glfw::Key::Tab,
        InputKey::Backspace    => glfw::Key::Backspace,
        InputKey::Insert       => glfw::Key::Insert,
        InputKey::Delete       => glfw::Key::Delete,
        InputKey::Right        => glfw::Key::Right,
        InputKey::Left         => glfw::Key::Left,
        InputKey::Down         => glfw::Key::Down,
        InputKey::Up           => glfw::Key::Up,
        InputKey::PageUp       => glfw::Key::PageUp,
        InputKey::PageDown     => glfw::Key::PageDown,
        InputKey::Home         => glfw::Key::Home,
        InputKey::End          => glfw::Key::End,
        InputKey::CapsLock     => glfw::Key::CapsLock,
        InputKey::ScrollLock   => glfw::Key::ScrollLock,
        InputKey::NumLock      => glfw::Key::NumLock,
        InputKey::PrintScreen  => glfw::Key::PrintScreen,
        InputKey::Pause        => glfw::Key::Pause,
        InputKey::F1           => glfw::Key::F1,
        InputKey::F2           => glfw::Key::F2,
        InputKey::F3           => glfw::Key::F3,
        InputKey::F4           => glfw::Key::F4,
        InputKey::F5           => glfw::Key::F5,
        InputKey::F6           => glfw::Key::F6,
        InputKey::F7           => glfw::Key::F7,
        InputKey::F8           => glfw::Key::F8,
        InputKey::F9           => glfw::Key::F9,
        InputKey::F10          => glfw::Key::F10,
        InputKey::F11          => glfw::Key::F11,
        InputKey::F12          => glfw::Key::F12,
        InputKey::F13          => glfw::Key::F13,
        InputKey::F14          => glfw::Key::F14,
        InputKey::F15          => glfw::Key::F15,
        InputKey::F16          => glfw::Key::F16,
        InputKey::F17          => glfw::Key::F17,
        InputKey::F18          => glfw::Key::F18,
        InputKey::F19          => glfw::Key::F19,
        InputKey::F20          => glfw::Key::F20,
        InputKey::F21          => glfw::Key::F21,
        InputKey::F22          => glfw::Key::F22,
        InputKey::F23          => glfw::Key::F23,
        InputKey::F24          => glfw::Key::F24,
        InputKey::F25          => glfw::Key::F25,
        InputKey::Kp0          => glfw::Key::Kp0,
        InputKey::Kp1          => glfw::Key::Kp1,
        InputKey::Kp2          => glfw::Key::Kp2,
        InputKey::Kp3          => glfw::Key::Kp3,
        InputKey::Kp4          => glfw::Key::Kp4,
        InputKey::Kp5          => glfw::Key::Kp5,
        InputKey::Kp6          => glfw::Key::Kp6,
        InputKey::Kp7          => glfw::Key::Kp7,
        InputKey::Kp8          => glfw::Key::Kp8,
        InputKey::Kp9          => glfw::Key::Kp9,
        InputKey::KpDecimal    => glfw::Key::KpDecimal,
        InputKey::KpDivide     => glfw::Key::KpDivide,
        InputKey::KpMultiply   => glfw::Key::KpMultiply,
        InputKey::KpSubtract   => glfw::Key::KpSubtract,
        InputKey::KpAdd        => glfw::Key::KpAdd,
        InputKey::KpEnter      => glfw::Key::KpEnter,
        InputKey::KpEqual      => glfw::Key::KpEqual,
        InputKey::LeftShift    => glfw::Key::LeftShift,
        InputKey::LeftControl  => glfw::Key::LeftControl,
        InputKey::LeftAlt      => glfw::Key::LeftAlt,
        InputKey::LeftSuper    => glfw::Key::LeftSuper,
        InputKey::RightShift   => glfw::Key::RightShift,
        InputKey::RightControl => glfw::Key::RightControl,
        InputKey::RightAlt     => glfw::Key::RightAlt,
        InputKey::RightSuper   => glfw::Key::RightSuper,
        InputKey::Menu         => glfw::Key::Menu,
        InputKey::Unknown      => glfw::Key::Unknown,
    }
}

fn mouse_button_to_glfw(button: MouseButton) -> Option<glfw::MouseButton> {
    Some(match button {
        MouseButton::Left    => glfw::MouseButton::Button1,
        MouseButton::Right   => glfw::MouseButton::Button2,
        MouseButton::Middle  => glfw::MouseButton::Button3,
        MouseButton::Back    => glfw::MouseButton::Button4,
        MouseButton::Forward => glfw::MouseButton::Button5,
        MouseButton::Unknown => return None,
    })
}
