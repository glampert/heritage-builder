// The egui side of the UI system: drives an egui Context frame by frame, feeds it the engine's
// input, and turns its output into the renderer's UiDrawData. Nothing outside the ui module sees
// egui.
//
// - Input: the mouse is polled once a frame from the InputSystem and turned into pointer events by
//   comparing with the previous frame, so simulated input and replays reach egui like real input.
//   Keys, text and the wheel arrive as events from the application, before the frame begins.
// - Whether the UI takes the mouse or keyboard is decided by the previous frame, as egui hit-tests
//   against the previous frame's layout.
// - Time comes from the engine's frame clock, so --fixed-dt also fixes tooltips and animations.
// - Copy and paste use a buffer of the game's own, not the system clipboard.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, TexturesDelta, TouchPhase, ViewportId, pos2, vec2};

use common::{Size, Vec2, time::Seconds};

use super::renderer::UiRenderer;
use crate::{
    app::{
        Application,
        input::{InputAction, InputKey, InputModifiers, InputSystem, MouseButton},
    },
    render::{texture::TextureCache, ui_draw::UiDrawData},
};

// The font atlas is as wide as this. epaint needs at least 1024, and large text sizes overflow that.
const MAX_TEXTURE_SIDE: usize = 2048;

pub(super) struct UiBackend {
    ctx: egui::Context,
    renderer: UiRenderer,
    pending_textures: TexturesDelta, // From warm_up, applied with the first frame's.

    // Input for the next frame, and what it was compared with.
    events: Vec<Event>,
    pointer_pos: Option<Pos2>, // None while the cursor is outside the window.
    buttons_down: [bool; 3],   // Primary, secondary, middle.
    modifiers: Modifiers,
    clipboard: String,

    time: f64,
    pixels_per_point: f32,
    framebuffer_size: Size,
    frame_started: bool,
}

impl UiBackend {
    pub fn new() -> Self {
        let ctx = egui::Context::default();

        ctx.options_mut(|options| {
            // UI code runs inside the game's update and can't run twice in a frame.
            options.max_passes = std::num::NonZeroUsize::MIN;
            options.zoom_with_keyboard = false;
            options.quit_shortcuts = Vec::new();
            options.theme_preference = egui::ThemePreference::Dark; // The styles go in its slot.
        });

        Self {
            ctx,
            renderer: UiRenderer::default(),
            pending_textures: TexturesDelta::default(),
            events: Vec::new(),
            pointer_pos: None,
            buttons_down: [false; 3],
            modifiers: Modifiers::NONE,
            clipboard: String::new(),
            time: 0.0,
            pixels_per_point: 1.0,
            framebuffer_size: Size::zero(),
            frame_started: false,
        }
    }

    #[inline]
    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    #[inline]
    pub fn frame_started(&self) -> bool {
        self.frame_started
    }

    // Runs an empty frame, so egui sets up its fonts with the latest ones given to it: text can be
    // measured before the first real frame, e.g. by menus being built. `pixels_per_point`: the
    // window's, which the fonts are rasterized for.
    pub fn warm_up(&mut self, pixels_per_point: f32) {
        debug_assert!(!self.frame_started);

        let mut raw_input = RawInput { max_texture_side: Some(MAX_TEXTURE_SIDE), time: Some(self.time), ..Default::default() };
        raw_input.viewports.entry(ViewportId::ROOT).or_default().native_pixels_per_point = Some(pixels_per_point);

        self.ctx.begin_pass(raw_input);
        let mut output = self.ctx.end_pass();

        self.pending_textures.append(std::mem::take(&mut output.textures_delta));
        output.platform_output.commands.clear();
    }

    // ----------------------
    // Frame:
    // ----------------------

    pub fn begin_frame(&mut self, app: &Application, input_sys: &InputSystem, delta_time_secs: Seconds) {
        debug_assert!(!self.frame_started);
        self.frame_started = true;

        self.framebuffer_size = app.framebuffer_size();
        self.pixels_per_point = app.content_scale().x.max(f32::EPSILON);

        // In points, like the cursor position and everything the UI draws.
        let fb_size = self.framebuffer_size.to_vec2();
        let screen_rect = egui::Rect::from_min_size(
            Pos2::ZERO,
            vec2(fb_size.x / self.pixels_per_point, fb_size.y / self.pixels_per_point),
        );

        self.poll_modifiers(input_sys);
        self.poll_pointer(input_sys, screen_rect);

        self.time += f64::from(delta_time_secs);

        let mut raw_input = RawInput {
            screen_rect: Some(screen_rect),
            max_texture_side: Some(MAX_TEXTURE_SIDE),
            time: Some(self.time),
            predicted_dt: delta_time_secs,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        raw_input.viewports.entry(ViewportId::ROOT).or_default().native_pixels_per_point = Some(self.pixels_per_point);

        self.ctx.begin_pass(raw_input);
    }

    // Ends egui's frame and returns what it drew. Textures egui manages are created, updated and
    // released in `tex_cache`.
    pub fn end_frame(&mut self, tex_cache: &mut TextureCache) -> &UiDrawData {
        debug_assert!(self.frame_started);
        self.frame_started = false;

        // egui counts the pointer as over the UI on the background layer unless the root Ui was
        // used this frame, which tells it the room panels left free. Retail menus don't use it.
        self.ctx.root_ui(|_| {});

        let mut output = self.ctx.end_pass();

        for command in output.platform_output.commands.drain(..) {
            if let egui::OutputCommand::CopyText(text) = command {
                self.clipboard = text;
            }
        }

        // Textures egui freed last frame, which has been drawn since; then this frame's new pixels.
        let mut textures_delta = std::mem::take(&mut self.pending_textures);
        textures_delta.append(std::mem::take(&mut output.textures_delta));

        self.renderer.textures_mut().release_freed(tex_cache);
        self.renderer.apply_texture_updates(tex_cache, &mut textures_delta);

        let primitives = self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        self.renderer.build_draw_data(&primitives, output.pixels_per_point, self.framebuffer_size);

        // Freed after this frame's drawing, which may still use them.
        self.renderer.free_textures(&mut textures_delta);

        self.renderer.draw_data()
    }

    // ----------------------
    // Input:
    // ----------------------

    // True if the mouse is over the UI or using it, e.g. dragging a slider.
    pub fn wants_pointer(&self) -> bool {
        self.ctx.is_pointer_over_egui() || self.ctx.egui_is_using_pointer()
    }

    // True while text is being typed into the UI.
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.text_edit_focused()
    }

    pub fn on_key_input(&mut self, key: InputKey, action: InputAction, modifiers: InputModifiers) {
        let modifiers = egui_modifiers(modifiers);
        let pressed = action != InputAction::Release;

        let Some(egui_key) = egui_key(key) else {
            return;
        };

        self.events.push(Event::Key {
            key: egui_key,
            physical_key: None,
            pressed,
            repeat: action == InputAction::Repeat,
            modifiers,
        });

        // Copy, cut and paste, which egui takes as events of their own.
        if pressed && modifiers.command {
            match egui_key {
                Key::C => self.events.push(Event::Copy),
                Key::X => self.events.push(Event::Cut),
                Key::V if !self.clipboard.is_empty() => self.events.push(Event::Paste(self.clipboard.clone())),
                _ => {}
            }
        }
    }

    pub fn on_char_input(&mut self, c: char) {
        // Control characters arrive as keys, and macOS sends function keys in the private use area.
        if c.is_control() || ('\u{F700}'..='\u{F8FF}').contains(&c) {
            return;
        }
        self.events.push(Event::Text(c.to_string()));
    }

    // `amount` in lines, positive y scrolling up, as the application reports it.
    pub fn on_scroll(&mut self, amount: Vec2) {
        self.events.push(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(amount.x, amount.y),
            phase: TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: self.modifiers,
        });
    }

    fn poll_modifiers(&mut self, input_sys: &InputSystem) {
        let down = |left: InputKey, right: InputKey| {
            input_sys.key_state(left) == InputAction::Press || input_sys.key_state(right) == InputAction::Press
        };

        let mut modifiers = InputModifiers::empty();
        modifiers.set(InputModifiers::Shift, down(InputKey::LeftShift, InputKey::RightShift));
        modifiers.set(InputModifiers::Control, down(InputKey::LeftControl, InputKey::RightControl));
        modifiers.set(InputModifiers::Alt, down(InputKey::LeftAlt, InputKey::RightAlt));
        modifiers.set(InputModifiers::Super, down(InputKey::LeftSuper, InputKey::RightSuper));

        let modifiers = egui_modifiers(modifiers);
        if modifiers != self.modifiers {
            self.modifiers = modifiers;
            self.events.push(Event::ModifiersChanged(modifiers));
        }
    }

    // Turns the polled cursor and buttons into egui's pointer events.
    fn poll_pointer(&mut self, input_sys: &InputSystem, screen_rect: egui::Rect) {
        let cursor = input_sys.cursor_pos();
        let pos = pos2(cursor.x, cursor.y);

        if screen_rect.contains(pos) {
            if self.pointer_pos != Some(pos) {
                self.pointer_pos = Some(pos);
                self.events.push(Event::PointerMoved(pos));
            }
        } else if self.pointer_pos.take().is_some() {
            self.events.push(Event::PointerGone); // E.g. parked outside the window by automation.
        }

        const BUTTONS: [(MouseButton, PointerButton); 3] = [
            (MouseButton::Left, PointerButton::Primary),
            (MouseButton::Right, PointerButton::Secondary),
            (MouseButton::Middle, PointerButton::Middle),
        ];

        for (index, (button, egui_button)) in BUTTONS.into_iter().enumerate() {
            let down = input_sys.mouse_button_state(button) == InputAction::Press;
            if down != self.buttons_down[index] {
                self.buttons_down[index] = down;
                self.events.push(Event::PointerButton { pos, button: egui_button, pressed: down, modifiers: self.modifiers });
            }
        }
    }
}

fn egui_modifiers(modifiers: InputModifiers) -> Modifiers {
    let ctrl = modifiers.intersects(InputModifiers::Control);
    let mac_cmd = cfg!(target_os = "macos") && modifiers.intersects(InputModifiers::Super);

    Modifiers {
        alt: modifiers.intersects(InputModifiers::Alt),
        ctrl,
        shift: modifiers.intersects(InputModifiers::Shift),
        mac_cmd,
        command: if cfg!(target_os = "macos") { mac_cmd } else { ctrl },
    }
}

fn egui_key(key: InputKey) -> Option<Key> {
    Some(match key {
        InputKey::Space => Key::Space,
        InputKey::Apostrophe => Key::Quote,
        InputKey::Comma => Key::Comma,
        InputKey::Minus | InputKey::KpSubtract => Key::Minus,
        InputKey::Period | InputKey::KpDecimal => Key::Period,
        InputKey::Slash | InputKey::KpDivide => Key::Slash,
        InputKey::Semicolon => Key::Semicolon,
        InputKey::Equal | InputKey::KpEqual => Key::Equals,
        InputKey::LeftBracket => Key::OpenBracket,
        InputKey::Backslash => Key::Backslash,
        InputKey::RightBracket => Key::CloseBracket,
        InputKey::GraveAccent => Key::Backtick,
        InputKey::KpAdd => Key::Plus,
        InputKey::Num0 | InputKey::Kp0 => Key::Num0,
        InputKey::Num1 | InputKey::Kp1 => Key::Num1,
        InputKey::Num2 | InputKey::Kp2 => Key::Num2,
        InputKey::Num3 | InputKey::Kp3 => Key::Num3,
        InputKey::Num4 | InputKey::Kp4 => Key::Num4,
        InputKey::Num5 | InputKey::Kp5 => Key::Num5,
        InputKey::Num6 | InputKey::Kp6 => Key::Num6,
        InputKey::Num7 | InputKey::Kp7 => Key::Num7,
        InputKey::Num8 | InputKey::Kp8 => Key::Num8,
        InputKey::Num9 | InputKey::Kp9 => Key::Num9,
        InputKey::A => Key::A,
        InputKey::B => Key::B,
        InputKey::C => Key::C,
        InputKey::D => Key::D,
        InputKey::E => Key::E,
        InputKey::F => Key::F,
        InputKey::G => Key::G,
        InputKey::H => Key::H,
        InputKey::I => Key::I,
        InputKey::J => Key::J,
        InputKey::K => Key::K,
        InputKey::L => Key::L,
        InputKey::M => Key::M,
        InputKey::N => Key::N,
        InputKey::O => Key::O,
        InputKey::P => Key::P,
        InputKey::Q => Key::Q,
        InputKey::R => Key::R,
        InputKey::S => Key::S,
        InputKey::T => Key::T,
        InputKey::U => Key::U,
        InputKey::V => Key::V,
        InputKey::W => Key::W,
        InputKey::X => Key::X,
        InputKey::Y => Key::Y,
        InputKey::Z => Key::Z,
        InputKey::Escape => Key::Escape,
        InputKey::Enter | InputKey::KpEnter => Key::Enter,
        InputKey::Tab => Key::Tab,
        InputKey::Backspace => Key::Backspace,
        InputKey::Insert => Key::Insert,
        InputKey::Delete => Key::Delete,
        InputKey::Right => Key::ArrowRight,
        InputKey::Left => Key::ArrowLeft,
        InputKey::Down => Key::ArrowDown,
        InputKey::Up => Key::ArrowUp,
        InputKey::PageUp => Key::PageUp,
        InputKey::PageDown => Key::PageDown,
        InputKey::Home => Key::Home,
        InputKey::End => Key::End,
        InputKey::F1 => Key::F1,
        InputKey::F2 => Key::F2,
        InputKey::F3 => Key::F3,
        InputKey::F4 => Key::F4,
        InputKey::F5 => Key::F5,
        InputKey::F6 => Key::F6,
        InputKey::F7 => Key::F7,
        InputKey::F8 => Key::F8,
        InputKey::F9 => Key::F9,
        InputKey::F10 => Key::F10,
        InputKey::F11 => Key::F11,
        InputKey::F12 => Key::F12,
        InputKey::F13 => Key::F13,
        InputKey::F14 => Key::F14,
        InputKey::F15 => Key::F15,
        InputKey::F16 => Key::F16,
        InputKey::F17 => Key::F17,
        InputKey::F18 => Key::F18,
        InputKey::F19 => Key::F19,
        InputKey::F20 => Key::F20,
        InputKey::F21 => Key::F21,
        InputKey::F22 => Key::F22,
        InputKey::F23 => Key::F23,
        InputKey::F24 => Key::F24,
        InputKey::F25 => Key::F25,
        _ => return None, // Modifiers and lock keys: egui gets the modifiers from ModifiersChanged.
    })
}

impl Drop for UiBackend {
    fn drop(&mut self) {
        self.pending_textures.clear(); // Never drawn: nothing to free.
    }
}
