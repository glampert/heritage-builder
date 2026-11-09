use std::any::Any;
use smallbox::smallbox;
use egui::{Color32, Sense, vec2};
use strum::{EnumCount, EnumIter, EnumProperty, IntoEnumIterator};

use common::{Vec2, format_fixed_string, mem, time::{CountdownTimer, Seconds}};

use super::{UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext};
use crate::{
    file_sys::paths::{AssetPath, PathRef},
    render::texture::TextureHandle,
    ui::{
        UiFontScale,
        immediate::{self, egui_vec2, rich_text, uv_full},
        renderer::egui_texture,
        sound::{self, UiButtonSoundsEnabled, UiSoundKey},
    },
};

// ----------------------------------------------
// UiTextButtonSize
// ----------------------------------------------

// Dictates text button font scale.
#[derive(Copy, Clone, Default)]
pub enum UiTextButtonSize {
    #[default]
    Normal,
    Small,
    ExtraSmall,
    Large,
}

impl UiTextButtonSize {
    pub const fn font_scale(self) -> UiFontScale {
        match self {
            UiTextButtonSize::Normal     => UiFontScale(1.2),
            UiTextButtonSize::Small      => UiFontScale(1.0),
            UiTextButtonSize::ExtraSmall => UiFontScale(0.8),
            UiTextButtonSize::Large      => UiFontScale(1.5),
        }
    }
}

// ----------------------------------------------
// UiTextButton
// ----------------------------------------------

pub type UiTextButtonPressed = UiWidgetCallback<dyn Fn(&UiTextButton, &mut dyn UiWidgetContext)>;

// Simple text label button. State does not "stick",
// one click equals one call to `on_pressed`, then
// immediately back to unpressed state.
pub struct UiTextButton {
    label: String,
    tooltip: Option<UiTooltipText>,
    font_scale: UiFontScale,
    hover: Option<TextureHandle>,
    sounds_enabled: UiButtonSoundsEnabled,
    enabled: bool,
    hovered: bool,
    on_pressed: UiTextButtonPressed,
}

impl UiWidget for UiTextButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let enabled = self.is_enabled();

        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            // Faded text if disabled. It still takes the mouse, for its tooltip.
            let text_color = ui.visuals().text_color();
            let text_color = if enabled { text_color } else { text_color.gamma_multiply(0.5) };
            let text = rich_text(ui_sys, ui, &self.label).color(text_color);

            // With a hover effect texture (underline effect) the button draws fully transparent.
            let button = egui::Button::new(text).frame(self.hover.is_none());
            let response = ui.add(button);

            // Draw underline effect when hovered / active:
            if let Some(hover) = self.hover
                && response.hovered()
            {
                let rect = response.rect;
                let underline_rect = egui::Rect::from_min_size(rect.min + vec2(0.0, rect.height() * 0.5 + 1.0), rect.size());
                let tint = if response.is_pointer_button_down_on() || !enabled {
                    Color32::from_white_alpha(128)
                } else {
                    Color32::WHITE
                };
                ui.painter().image(egui_texture(hover), underline_rect, uv_full(), tint);
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.as_str());
            immediate::record(ui_sys, ui, &response, "text_button", &self.label, tooltip, enabled, None);

            (response.clicked(), response.hovered())
        });

        if hovered {
            // Play sound on transition to hovered state.
            if self.sounds_enabled.intersects(UiButtonSoundsEnabled::Hovered) && !self.hovered && !pressed && enabled {
                sound::play(*context.sound_sys(), UiSoundKey::ButtonHovered);
            }

            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        // Invoke on pressed callback.
        if pressed && enabled {
            if self.sounds_enabled.intersects(UiButtonSoundsEnabled::Pressed) {
                sound::play(*context.sound_sys(), UiSoundKey::ButtonPressed);
            }

            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
            }
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let padding = context.ui_sys().egui_style().spacing.button_padding;

        // Compute scaled font size (window-independent).
        let (text_size, font_size) = context.calc_text_and_font_size(self.font_scale, &self.label);

        let width = text_size.x + (padding.x * 2.0);
        let height = text_size.y.max(font_size) + (padding.y * 2.0);

        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiTextButton {
    // A normal-size button with its sounds on. It draws as the style's framed button unless
    // given a hover sprite.
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());

        Self {
            label,
            tooltip: None,
            font_scale: UiTextButtonSize::default().font_scale(),
            hover: None,
            sounds_enabled: UiButtonSoundsEnabled::all(),
            enabled: true,
            hovered: false,
            on_pressed: None,
        }
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn with_size(mut self, size: UiTextButtonSize) -> Self {
        self.font_scale = size.font_scale();
        self
    }

    // Drawn under the text while hovered, in place of the style's button frame.
    pub fn with_hover_sprite<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.hover = Some(context.load_texture(sprite.into()));
        self
    }

    pub fn with_sounds(mut self, sounds: UiButtonSoundsEnabled) -> Self {
        self.sounds_enabled = sounds;
        self
    }

    // A disabled button fades its text and doesn't press, but still shows its tooltip.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_pressed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiTextButton, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_pressed = Some(smallbox!(f));
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn enable(&mut self, enable: bool) {
        self.enabled = enable;
    }
}

// ----------------------------------------------
// UiSpriteButton
// ----------------------------------------------

pub type UiSpriteButtonStateChanged =
    UiWidgetCallback<dyn Fn(&mut UiSpriteButton, &mut dyn UiWidgetContext, UiSpriteButtonState)>;

// Multi-state sprite button. Works via state polling or callback; state persists until changed.
pub struct UiSpriteButton {
    label: String,

    tooltip: Option<UiTooltipText>,
    show_tooltip_when_pressed: bool,

    size: Vec2,
    position: Option<Vec2>, // NOTE: Position is only known after the first call to draw().
    textures: UiSpriteButtonTextures,
    sounds_enabled: UiButtonSoundsEnabled,

    logical_state: UiSpriteButtonState,
    visual_state: UiSpriteButtonState,
    visual_state_transition_timer: CountdownTimer,
    state_transition_secs: Seconds,

    on_state_changed: UiSpriteButtonStateChanged,
}

impl UiWidget for UiSpriteButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());
        debug_assert!(self.textures.are_textures_loaded());

        let ui_sys = context.ui_sys();
        let texture = self.textures.texture_for_state(self.visual_state);

        let response = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::click());
            if ui.is_rect_visible(rect) {
                ui.painter().image(egui_texture(texture), rect, uv_full(), Color32::WHITE);
            }
            response
        });

        let hovered = response.hovered();
        let left_click = response.clicked();
        let right_click = response.secondary_clicked();

        self.position = Some(Vec2::new(response.rect.min.x, response.rect.min.y));

        // NOTE: Only left click counts as "pressed".
        let delta_time_secs = context.delta_time_secs();
        self.update_state(context, hovered, left_click, right_click, delta_time_secs);

        // The label is a sprite path, e.g. "palette/housing"; its last part names the button.
        let name = self.label.rsplit('/').next().unwrap_or(&self.label);
        let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.as_str());
        let pressed = Some(serde_json::Value::Bool(self.is_pressed()));
        let enabled = self.is_enabled();
        ui_sys.with_ui(|ui| immediate::record(ui_sys, ui, &response, "sprite_button", name, tooltip, enabled, pressed));

        if let Some(tooltip) = &self.tooltip {
            let show_tooltip = hovered && (!self.is_pressed() || self.show_tooltip_when_pressed);
            if show_tooltip {
                tooltip.draw(context);
            }
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiSpriteButton {
    // `sprite` names its state sprites, e.g. "palette/housing" for buttons/palette/housing_idle.png
    // and the other states', and labels the button. It starts idle, with its press sound on, and
    // shows a press at once.
    pub fn new(context: &mut dyn UiWidgetContext, sprite: impl Into<String>, size: Vec2) -> Self {
        let label = sprite.into();
        debug_assert!(!label.is_empty());
        debug_assert!(size.x > 0.0 && size.y > 0.0);

        let textures = UiSpriteButtonTextures::load(PathRef::from_str(&label), context);

        Self {
            label,
            tooltip: None,
            show_tooltip_when_pressed: false,
            size,
            position: None, // Set after the first draw().
            textures,
            sounds_enabled: UiButtonSoundsEnabled::Pressed,
            logical_state: UiSpriteButtonState::Idle,
            visual_state: UiSpriteButtonState::Idle,
            visual_state_transition_timer: CountdownTimer::new(0.0),
            state_transition_secs: 0.0,
            on_state_changed: None,
        }
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // The tooltip shows while the button is pressed too.
    pub fn with_show_tooltip_when_pressed(mut self, show: bool) -> Self {
        self.show_tooltip_when_pressed = show;
        self
    }

    pub fn with_sounds(mut self, sounds: UiButtonSoundsEnabled) -> Self {
        self.sounds_enabled = sounds;
        self
    }

    pub fn with_state(mut self, state: UiSpriteButtonState) -> Self {
        self.logical_state = state;
        self.visual_state = state;
        self
    }

    // The pressed sprite stays this long after a press before the button shows its state again.
    pub fn with_state_transition_secs(mut self, secs: Seconds) -> Self {
        debug_assert!(secs >= 0.0);
        self.state_transition_secs = secs;
        self.visual_state_transition_timer = CountdownTimer::new(secs);
        self
    }

    // Called with the previous state when the state changes, and on every left click.
    pub fn on_state_changed<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut UiSpriteButton, &mut dyn UiWidgetContext, UiSpriteButtonState) + 'static,
    {
        self.on_state_changed = Some(smallbox!(f));
        self
    }

    pub fn position(&self) -> Vec2 {
        self.position.expect("Called UiSpriteButton::position() before first draw()!")
    }

    pub fn state(&self) -> UiSpriteButtonState {
        self.logical_state
    }

    pub fn is_idle(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Idle
    }

    pub fn is_disabled(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Disabled
    }

    pub fn is_enabled(&self) -> bool {
        self.logical_state != UiSpriteButtonState::Disabled
    }

    pub fn is_pressed(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Pressed
    }

    pub fn enable(&mut self, enable: bool) {
        if enable {
            if self.logical_state == UiSpriteButtonState::Disabled {
                self.logical_state = UiSpriteButtonState::Idle;
            }
        } else {
            self.logical_state = UiSpriteButtonState::Disabled;
        }
    }

    pub fn press(&mut self, press: bool) {
        if press {
            self.logical_state = UiSpriteButtonState::Pressed;
        } else if self.logical_state == UiSpriteButtonState::Pressed {
            self.logical_state = UiSpriteButtonState::Idle;
        }
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn play_sound(&mut self, context: &mut dyn UiWidgetContext, sound_key: UiSoundKey, new_state: UiSpriteButtonState) {
        // Play state transition sound if moving to a different state.
        if self.logical_state != new_state {
            let mut play = false;

            if sound_key == UiSoundKey::ButtonHovered && self.sounds_enabled.intersects(UiButtonSoundsEnabled::Hovered) {
                play = true;
            }

            if sound_key == UiSoundKey::ButtonPressed && self.sounds_enabled.intersects(UiButtonSoundsEnabled::Pressed) {
                play = true;
            }

            if play {
                sound::play(*context.sound_sys(), sound_key);
            }
        }
    }

    fn update_state(
        &mut self,
        context: &mut dyn UiWidgetContext,
        hovered: bool,
        left_click: bool,
        right_click: bool,
        delta_time_secs: Seconds,
    ) {
        let prev_state = self.logical_state;

        match self.logical_state {
            UiSpriteButtonState::Idle | UiSpriteButtonState::Hovered => {
                // Left click selects/presses button.
                if left_click {
                    self.play_sound(context, UiSoundKey::ButtonPressed, UiSpriteButtonState::Pressed);
                    self.logical_state = UiSpriteButtonState::Pressed;
                } else if hovered {
                    self.play_sound(context, UiSoundKey::ButtonHovered, UiSpriteButtonState::Hovered);
                    self.logical_state = UiSpriteButtonState::Hovered;
                } else {
                    self.logical_state = UiSpriteButtonState::Idle;
                }
            }
            UiSpriteButtonState::Pressed => {
                // Right click deselects/unpresses.
                if right_click {
                    self.logical_state = UiSpriteButtonState::Idle;
                }
            }
            UiSpriteButtonState::Disabled => {}
        }

        if left_click {
            // Reset transition if pressed.
            self.visual_state_transition_timer.reset(self.state_transition_secs);
        }

        if self.visual_state == UiSpriteButtonState::Pressed {
            // Run a timed transition between idle/hovered and pressed.
            if self.visual_state_transition_timer.tick(delta_time_secs) {
                self.visual_state_transition_timer.reset(self.state_transition_secs);
                self.visual_state = self.logical_state;
            }
        } else {
            self.visual_state = self.logical_state;
        }

        // Invoke on state change or re-click with left mouse button.
        if (self.logical_state != prev_state || left_click)
            && let Some(on_state_changed) = &self.on_state_changed
        {
            // The callback lives in this button and gets it mutably: see UiWidgetCallback.
            on_state_changed(mem::mut_ref_cast(self), context, prev_state);
        }
    }
}

// ----------------------------------------------
// UiSpriteButtonTextures
// ----------------------------------------------

struct UiSpriteButtonTextures {
    textures: [TextureHandle; UI_SPRITE_BUTTON_STATE_COUNT],
}

impl UiSpriteButtonTextures {
    fn unloaded() -> Self {
        Self { textures: [TextureHandle::invalid(); UI_SPRITE_BUTTON_STATE_COUNT] }
    }

    fn load(sprite_path: PathRef, context: &mut dyn UiWidgetContext) -> Self {
        let mut sprites = Self::unloaded();
        sprites.load_textures(sprite_path, context);
        sprites
    }

    fn load_textures(&mut self, sprite_path: PathRef, context: &mut dyn UiWidgetContext) {
        for state in UiSpriteButtonState::iter() {
            self.textures[state as usize] = state.load_texture(sprite_path, context);
        }
    }

    #[inline]
    fn are_textures_loaded(&self) -> bool {
        self.textures[0].is_valid()
    }

    #[inline]
    fn texture_for_state(&self, state: UiSpriteButtonState) -> TextureHandle {
        debug_assert!(self.textures[state as usize].is_valid());
        self.textures[state as usize]
    }
}

// ----------------------------------------------
// UiSpriteButtonState
// ----------------------------------------------

const UI_SPRITE_BUTTON_STATE_COUNT: usize = UiSpriteButtonState::COUNT;

#[derive(Copy, Clone, Default, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum UiSpriteButtonState {
    #[default]
    #[strum(props(Suffix = "idle"))]
    Idle,

    #[strum(props(Suffix = "disabled"))]
    Disabled,

    #[strum(props(Suffix = "hovered"))]
    Hovered,

    #[strum(props(Suffix = "pressed"))]
    Pressed,
}

impl UiSpriteButtonState {
    fn asset_path(self, sprite_path: PathRef) -> AssetPath {
        debug_assert!(!sprite_path.is_empty());
        let sprite_suffix = self.get_str("Suffix").unwrap();
        let sprite_name = format_fixed_string!(64, "{sprite_path}_{sprite_suffix}.png");
        AssetPath::from_str("buttons").join(sprite_name)
    }

    fn load_texture(self, sprite_path: PathRef, context: &mut dyn UiWidgetContext) -> TextureHandle {
        let asset_path = self.asset_path(sprite_path);
        context.load_texture((&asset_path).into())
    }
}
