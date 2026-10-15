use bitflags::bitflags;
use common::Vec2;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

// ----------------------------------------------
// Internal backend implementations
// ----------------------------------------------

#[enum_dispatch]
pub(crate) enum InputSystemBackendImpl {
    Winit(super::winit::WinitInputSystemBackend),

    #[cfg(feature = "desktop")]
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
    virtual_input: Option<VirtualInput>, // Replaces the real devices while set (automation).

    // The real devices' state for the frame, while --record or --replay runs: sampled from the
    // backend once a frame, or read from the recording. Every read in a frame sees the same state.
    device_state: Option<VirtualInput>,
}

impl InputSystem {
    pub(super) fn new(backend: InputSystemBackendImpl) -> Self {
        Self { backend, virtual_input: None, device_state: None }
    }

    #[inline]
    pub fn cursor_pos(&self) -> Vec2 {
        match self.input_state() {
            Some(state) => state.cursor_pos,
            None => self.backend.cursor_pos(),
        }
    }

    #[inline]
    pub fn mouse_button_state(&self, button: MouseButton) -> InputAction {
        match self.input_state() {
            Some(state) => InputAction::from_pressed(state.is_mouse_button_pressed(button)),
            None => self.backend.mouse_button_state(button),
        }
    }

    #[inline]
    pub fn key_state(&self, key: InputKey) -> InputAction {
        match self.input_state() {
            Some(state) => InputAction::from_pressed(state.is_key_pressed(key)),
            None => self.backend.key_state(key),
        }
    }

    #[inline]
    fn input_state(&self) -> Option<&VirtualInput> {
        self.virtual_input.as_ref().or(self.device_state.as_ref())
    }

    // The real mouse and keyboard as the backend reports them now.
    pub(crate) fn sample_devices(&self) -> VirtualInput {
        let mut state = VirtualInput::new();
        state.cursor_pos = self.backend.cursor_pos();

        for button in MouseButton::iter() {
            state.set_mouse_button(button, self.backend.mouse_button_state(button) != InputAction::Release);
        }
        for key in InputKey::iter() {
            state.set_key(key, self.backend.key_state(key) != InputAction::Release);
        }
        state
    }

    #[inline]
    pub(crate) fn set_device_state(&mut self, device_state: Option<VirtualInput>) {
        self.device_state = device_state;
    }

    // Set while automation drives the input (see Engine::set_input_capture).
    #[inline]
    pub fn virtual_input(&self) -> Option<&VirtualInput> {
        self.virtual_input.as_ref()
    }

    #[inline]
    pub(crate) fn virtual_input_mut(&mut self) -> Option<&mut VirtualInput> {
        self.virtual_input.as_mut()
    }

    #[inline]
    pub(crate) fn set_virtual_input(&mut self, virtual_input: Option<VirtualInput>) {
        self.virtual_input = virtual_input;
    }

    // Direct backend access for platform-specific input mutation (e.g. WebRunner).
    #[cfg(feature = "web")]
    #[inline]
    pub(crate) fn backend_mut(&mut self) -> &mut InputSystemBackendImpl {
        &mut self.backend
    }
}

// ----------------------------------------------
// VirtualInput
// ----------------------------------------------

// A simulated mouse and keyboard, driven by automation's input timeline (automation/input.rs).
// Also holds a snapshot of the real devices while a run is recorded or replayed.
#[derive(Clone, PartialEq)]
pub struct VirtualInput {
    pub cursor_pos: Vec2,
    mouse_buttons: [bool; MouseButton::COUNT],
    keys: [bool; InputKey::COUNT],
}

impl VirtualInput {
    // Off the window: hovers nothing, and doesn't scroll the map.
    pub const PARKED_CURSOR_POS: Vec2 = Vec2 { x: -1000.0, y: -1000.0 };

    pub fn new() -> Self {
        Self {
            cursor_pos: Self::PARKED_CURSOR_POS,
            mouse_buttons: [false; MouseButton::COUNT],
            keys: [false; InputKey::COUNT],
        }
    }

    #[inline]
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons[button as usize]
    }

    #[inline]
    pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        self.mouse_buttons[button as usize] = pressed;
    }

    #[inline]
    pub fn is_key_pressed(&self, key: InputKey) -> bool {
        self.keys[key as usize]
    }

    #[inline]
    pub fn set_key(&mut self, key: InputKey, pressed: bool) {
        self.keys[key as usize] = pressed;
    }

    // Modifiers from the modifier keys held down.
    pub fn modifiers(&self) -> InputModifiers {
        let mut modifiers = InputModifiers::empty();
        let held = |keys: [InputKey; 2]| keys.iter().any(|key| self.is_key_pressed(*key));

        modifiers.set(InputModifiers::Shift, held([InputKey::LeftShift, InputKey::RightShift]));
        modifiers.set(InputModifiers::Control, held([InputKey::LeftControl, InputKey::RightControl]));
        modifiers.set(InputModifiers::Alt, held([InputKey::LeftAlt, InputKey::RightAlt]));
        modifiers.set(InputModifiers::Super, held([InputKey::LeftSuper, InputKey::RightSuper]));
        modifiers
    }

    pub fn release_all(&mut self) {
        self.mouse_buttons = [false; MouseButton::COUNT];
        self.keys = [false; InputKey::COUNT];
    }

    pub fn pressed_mouse_buttons(&self) -> impl Iterator<Item = MouseButton> + '_ {
        MouseButton::iter().filter(|button| self.is_mouse_button_pressed(*button))
    }

    pub fn pressed_keys(&self) -> impl Iterator<Item = InputKey> + '_ {
        InputKey::iter().filter(|key| self.is_key_pressed(*key))
    }
}

impl Default for VirtualInput {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------
// InputAction
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum InputAction {
    #[default]
    Release,
    Repeat,
    Press,
}

impl InputAction {
    #[inline]
    pub fn from_pressed(pressed: bool) -> Self {
        if pressed { Self::Press } else { Self::Release }
    }
}

// ----------------------------------------------
// InputModifiers
// ----------------------------------------------

bitflags! {
    // Serializes as names, e.g. "Shift | Control".
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumCount, EnumIter, EnumString, IntoStaticStr)]
#[strum(ascii_case_insensitive)]
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

// Names parse case-insensitively from the variant names, e.g. "Escape", "f12", "LeftShift".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumCount, EnumIter, EnumString, IntoStaticStr)]
#[strum(ascii_case_insensitive)]
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
