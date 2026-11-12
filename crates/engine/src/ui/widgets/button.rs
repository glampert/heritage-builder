use std::any::Any;
use smallbox::smallbox;
use egui::{Align2, Color32, Sense, vec2};
use strum::{EnumCount, EnumIter, EnumProperty, IntoEnumIterator};

use common::{Color, Vec2, mem, time::{CountdownTimer, Seconds}};

use super::{UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiNineSlice, UiNineSliceCenter, UiString,
        immediate::{self, color32, egui_vec2, nine_slice_shape, rich_text, uv_full},
        internal,
        renderer::egui_texture,
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
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
// UiTextButtonWidth
// ----------------------------------------------

// A text button's width: its text's plus padding (the default), at least so wide, or exactly so
// wide (its text centered, cut if longer), e.g. a dialog's row of equal buttons.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum UiTextButtonWidth {
    #[default]
    Content,
    Min(f32),
    Fixed(f32),
}

impl UiTextButtonWidth {
    // The width of a button whose content is `content_width` wide.
    pub fn resolve(self, content_width: f32) -> f32 {
        match self {
            Self::Content => content_width,
            Self::Min(width) => content_width.max(width),
            Self::Fixed(width) => width,
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
    label: UiString,
    tooltip: Option<UiTooltipText>,
    font_scale: UiFontScale,
    hover: Option<TextureHandle>,
    hover_panel: Option<(UiNineSlice, Vec2)>, // Drawn behind the text while hovered, grown by the Vec2.
    text_color: Option<Color>,
    hover_text_color: Option<Color>,
    frame: Option<UiTextButtonFrame>,
    width: UiTextButtonWidth,
    min_height: f32,
    sounds: UiWidgetSounds, // Hovered: ButtonHovered, Pressed: ButtonPressed.
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
            // Faded text if disabled. It still takes the mouse, for its tooltip. The hover color
            // follows last frame's hover, as the text is drawn before this frame's is known.
            let text_color = match &self.frame {
                Some(frame) => frame.text_color.map_or_else(|| ui.visuals().text_color(), color32),
                None => {
                    let color = if self.hovered && enabled { self.hover_text_color.or(self.text_color) } else { self.text_color };
                    color.map_or_else(|| ui.visuals().text_color(), color32)
                }
            };
            let text_color = if enabled { text_color } else { text_color.gamma_multiply(0.5) };

            let response = match &self.frame {
                // A framed button: its state's panel, with the text centered on it.
                Some(frame) => {
                    let (text_size, font_size) = internal::calc_text_size(ui_sys, self.font_scale, self.label.resolve());
                    let size = frame.button_size(text_size, font_size);
                    let size = Vec2::new(self.width.resolve(size.x), size.y.max(self.min_height));
                    let (rect, response) = ui.allocate_exact_size(egui_vec2(size), Sense::click());

                    if ui.is_rect_visible(rect) {
                        let state = if !enabled {
                            UiSpriteButtonState::Disabled
                        } else if response.is_pointer_button_down_on() {
                            UiSpriteButtonState::Pressed
                        } else if response.hovered() {
                            UiSpriteButtonState::Hovered
                        } else {
                            UiSpriteButtonState::Idle
                        };

                        let (panel, tint) = frame.panel(state);
                        ui.painter().add(nine_slice_shape(rect, panel, color32(tint)));

                        let font = internal::font(ui_sys, self.font_scale);
                        ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            self.label.resolve(),
                            font,
                            text_color,
                        );
                    }
                    response
                }
                None => {
                    let text = rich_text(ui_sys, ui, self.label.resolve()).color(text_color);

                    // With a hover effect texture (underline effect) or panel the button draws fully
                    // transparent. The panel goes in a slot under the text, filled once the
                    // button's rect is known.
                    let frameless = self.hover.is_some() || self.hover_panel.is_some();
                    let panel_slot = self.hover_panel.is_some().then(|| ui.painter().add(egui::Shape::Noop));
                    let mut button = egui::Button::new(text).frame(!frameless);
                    let min_width = match self.width {
                        UiTextButtonWidth::Min(width) | UiTextButtonWidth::Fixed(width) => width,
                        UiTextButtonWidth::Content => 0.0,
                    };
                    button = button.min_size(vec2(min_width, self.min_height));
                    let response = ui.add(button);

                    if let (Some(slot), Some((panel, grow))) = (panel_slot, &self.hover_panel)
                        && response.hovered()
                    {
                        let rect = response.rect.expand2(egui_vec2(*grow));
                        let tint = if response.is_pointer_button_down_on() || !enabled {
                            Color32::from_white_alpha(160)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().set(slot, nine_slice_shape(rect, panel, tint));
                    }

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
                    response
                }
            };

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            immediate::record(ui_sys, ui, &response, "text_button", self.label.resolve(), tooltip, enabled, None);

            (response.clicked(), response.hovered())
        });

        if hovered {
            // Play sound on transition to hovered state.
            if !self.hovered && !pressed && enabled {
                self.sounds.play(context, UiSoundEvent::Hovered, Some(UiSoundKey::ButtonHovered));
            }

            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        // Invoke on pressed callback.
        if pressed && enabled {
            self.sounds.play(context, UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed));

            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
            }
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        // Compute scaled font size (window-independent).
        let (text_size, font_size) = context.calc_text_and_font_size(self.font_scale, self.label.resolve());

        if let Some(frame) = &self.frame {
            let size = frame.button_size(text_size, font_size);
            return Vec2::new(self.width.resolve(size.x), size.y.max(self.min_height));
        }

        // A plain button grows past a fixed width to fit its text.
        let padding = context.ui_sys().egui_style().spacing.button_padding;
        let width = text_size.x + (padding.x * 2.0);
        let height = (text_size.y.max(font_size) + (padding.y * 2.0)).max(self.min_height);
        let width = match self.width {
            UiTextButtonWidth::Content => width,
            UiTextButtonWidth::Min(min) | UiTextButtonWidth::Fixed(min) => width.max(min),
        };

        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiTextButton {
    // A normal-size button with its sounds on. It draws as the style's framed button unless
    // given a hover sprite or a frame.
    pub fn new(label: impl Into<UiString>) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());

        Self {
            label,
            tooltip: None,
            font_scale: UiTextButtonSize::default().font_scale(),
            hover: None,
            hover_panel: None,
            text_color: None,
            hover_text_color: None,
            frame: None,
            width: UiTextButtonWidth::Content,
            min_height: 0.0,
            sounds: UiWidgetSounds::new(),
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

    // Its text's scale over the button text style, in place of a UiTextButtonSize's.
    pub fn with_font_scale(mut self, scale: UiFontScale) -> Self {
        self.font_scale = scale;
        self
    }

    // Drawn under the text while hovered, in place of the style's button frame.
    pub fn with_hover_sprite<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.hover = Some(context.load_texture(sprite.into()));
        self
    }

    // A 9-slice panel behind the text while hovered, `grow` points past the button on each side,
    // in place of the style's button frame, e.g. a skin's menu item highlight.
    pub fn with_hover_panel(mut self, panel: UiNineSlice, grow: Vec2) -> Self {
        self.hover_panel = Some((panel, grow));
        self
    }

    // The text's color, and its color while hovered (the text color if None); the style's by
    // default. A framed button takes its frame's.
    pub fn with_text_colors(mut self, color: Option<Color>, hovered: Option<Color>) -> Self {
        self.text_color = color;
        self.hover_text_color = hovered;
        self
    }

    // 9-slice panels per state in place of the style's button frame, with the text centered on
    // them: the button is the text plus the frame's padding. A hover sprite is ignored.
    pub fn with_frame(mut self, frame: UiTextButtonFrame) -> Self {
        self.frame = Some(frame);
        self
    }

    // Its width: its text's (the default), at least or exactly a width, e.g. for a dialog's row
    // of equal buttons. A frameless button grows past a fixed width to fit its text.
    pub fn with_width(mut self, width: UiTextButtonWidth) -> Self {
        self.width = width;
        self
    }

    // The least it's drawn tall, its text centered: a dialog's row of tall buttons.
    pub fn with_min_height(mut self, height: f32) -> Self {
        debug_assert!(height >= 0.0);
        self.min_height = height;
        self
    }

    // Its sounds: ButtonHovered when hovered and ButtonPressed when pressed, unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` (Hovered or Pressed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
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
// UiTextButtonFrame
// ----------------------------------------------

// A framed text button's look (UiTextButton::with_frame): a 9-slice panel per state, idle,
// hovered, pressed and disabled, as sprite buttons have, each multiplied by a tint; the space
// between the frame and the text; and the text's color.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiTextButtonFrame {
    panels: [UiNineSlice; UI_SPRITE_BUTTON_STATE_COUNT], // By UiSpriteButtonState.
    tints: [Color; UI_SPRITE_BUTTON_STATE_COUNT],
    padding: Option<Vec2>,      // The panels' borders if None.
    text_color: Option<Color>,  // The style's if None.
}

impl UiTextButtonFrame {
    // The four state sprites `<sprite>_idle.png`, `_hovered`, `_pressed` and `_disabled` under
    // assets/ui/ (e.g. "skins/dailong/button_primary"), each cut by `slice_px`, its borders in
    // pixels (left, top, right, bottom).
    pub fn load(context: &mut dyn UiWidgetContext, sprite: &str, slice_px: [f32; 4]) -> Self {
        debug_assert!(!sprite.is_empty());
        let mut load = |state: UiSpriteButtonState| {
            let path = state.frame_sprite_path(sprite);
            context.load_nine_slice(PathRef::from_str(&path), slice_px)
        };
        Self::new(
            load(UiSpriteButtonState::Idle),
            load(UiSpriteButtonState::Hovered),
            load(UiSpriteButtonState::Pressed),
            load(UiSpriteButtonState::Disabled),
        )
    }

    pub fn new(idle: UiNineSlice, hovered: UiNineSlice, pressed: UiNineSlice, disabled: UiNineSlice) -> Self {
        let mut panels = [idle; UI_SPRITE_BUTTON_STATE_COUNT];
        panels[UiSpriteButtonState::Hovered as usize] = hovered;
        panels[UiSpriteButtonState::Pressed as usize] = pressed;
        panels[UiSpriteButtonState::Disabled as usize] = disabled;
        Self { panels, tints: [Color::white(); UI_SPRITE_BUTTON_STATE_COUNT], padding: None, text_color: None }
    }

    // One panel for every state, told apart by tints: idle a little darker than hovered, pressed
    // darker still, disabled faded. For art without state sprites.
    pub fn uniform(panel: UiNineSlice) -> Self {
        Self::new(panel, panel, panel, panel).with_tints(
            Color::new(0.9, 0.9, 0.9, 1.0),
            Color::white(),
            Color::new(0.7, 0.7, 0.7, 1.0),
            Color::new(0.8, 0.8, 0.8, 0.6),
        )
    }

    // Points per pixel for every state's panel, e.g. 0.5 for art drawn at 2×.
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.panels = self.panels.map(|panel| panel.with_scale(scale));
        self
    }

    pub fn with_center(mut self, center: UiNineSliceCenter) -> Self {
        self.panels = self.panels.map(|panel| panel.with_center(center));
        self
    }

    // What each state's panel is multiplied by.
    pub fn with_tints(mut self, idle: Color, hovered: Color, pressed: Color, disabled: Color) -> Self {
        self.tints[UiSpriteButtonState::Idle as usize] = idle;
        self.tints[UiSpriteButtonState::Hovered as usize] = hovered;
        self.tints[UiSpriteButtonState::Pressed as usize] = pressed;
        self.tints[UiSpriteButtonState::Disabled as usize] = disabled;
        self
    }

    // Between the frame's edges and the text, in points: the idle panel's wider border on each
    // axis by default.
    pub fn with_padding(mut self, padding: Vec2) -> Self {
        self.padding = Some(padding);
        self
    }

    // The text's color, faded on a disabled button; the style's by default.
    pub fn with_text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    pub fn padding(&self) -> Vec2 {
        self.padding.unwrap_or_else(|| {
            let [left, top, right, bottom] = self.panels[UiSpriteButtonState::Idle as usize].border_points();
            Vec2::new(left.max(right), top.max(bottom))
        })
    }

    // Its text colour, if it sets one.
    pub fn text_color(&self) -> Option<Color> {
        self.text_color
    }

    #[inline]
    pub(super) fn panel(&self, state: UiSpriteButtonState) -> (&UiNineSlice, Color) {
        (&self.panels[state as usize], self.tints[state as usize])
    }

    // A button with text of `text_size` in a font of `font_size`.
    pub(super) fn button_size(&self, text_size: Vec2, font_size: f32) -> Vec2 {
        let padding = self.padding();
        Vec2::new(text_size.x + padding.x * 2.0, text_size.y.max(font_size) + padding.y * 2.0)
    }
}

// ----------------------------------------------
// UiIconButton
// ----------------------------------------------

pub type UiIconButtonPressed = UiWidgetCallback<dyn Fn(&UiIconButton, &mut dyn UiWidgetContext)>;

// A square button with an icon on a framed panel: a skin's icon buttons (the HUD's tools, a
// lens). Its frame has a panel per state (UiTextButtonFrame); a selected button draws its
// selected panel instead, e.g. the tool in use. Toggling (with_toggle) flips its selection on
// each press. Recorded as kind "icon_button", its value whether it's selected.
pub struct UiIconButton {
    label: String,
    icon: Option<TextureHandle>,
    icon_tint: Color,
    icon_inset: f32,
    size: Vec2,
    frame: Option<UiTextButtonFrame>,
    selected_panel: Option<UiNineSlice>,
    toggle: bool,
    selected: bool,
    enabled: bool,
    tooltip: Option<UiTooltipText>,
    sounds: UiWidgetSounds, // Hovered: nothing, Pressed: ButtonPressed.
    hovered: bool,
    on_pressed: UiIconButtonPressed,
}

impl UiWidget for UiIconButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let enabled = self.enabled;
        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::click());

            if ui.is_rect_visible(rect) {
                let state = if !enabled {
                    UiSpriteButtonState::Disabled
                } else if response.is_pointer_button_down_on() {
                    UiSpriteButtonState::Pressed
                } else if response.hovered() {
                    UiSpriteButtonState::Hovered
                } else {
                    UiSpriteButtonState::Idle
                };

                match (&self.selected_panel, &self.frame) {
                    (Some(panel), _) if self.selected => {
                        ui.painter().add(nine_slice_shape(rect, panel, Color32::WHITE));
                    }
                    (_, Some(frame)) => {
                        let (panel, tint) = frame.panel(state);
                        ui.painter().add(nine_slice_shape(rect, panel, color32(tint)));
                    }
                    (_, None) => {
                        // Painted: the style's button look, selected like a selectable label.
                        let visuals = ui.style().interact_selectable(&response, self.selected);
                        ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
                    }
                }

                if let Some(icon) = self.icon {
                    let tint = if enabled { color32(self.icon_tint) } else { color32(self.icon_tint).gamma_multiply(0.5) };
                    ui.painter().image(egui_texture(icon), rect.shrink(self.icon_inset), uv_full(), tint);
                }
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let name = self.label.rsplit('/').next().unwrap_or(&self.label);
            immediate::record(ui_sys, ui, &response, "icon_button", name, tooltip, enabled, Some(serde_json::Value::Bool(self.selected)));
            (response.clicked(), response.hovered())
        });

        if hovered {
            if !self.hovered && enabled {
                self.sounds.play(context, UiSoundEvent::Hovered, None);
            }
            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        if pressed && enabled {
            self.sounds.play(context, UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed));
            if self.toggle {
                self.selected = !self.selected;
            }
            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
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

impl UiIconButton {
    // A button `size` points square-ish, named `label` in widget paths (its last '/' part).
    pub fn new(label: impl Into<String>, size: Vec2) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());
        debug_assert!(size.x > 0.0 && size.y > 0.0);
        Self {
            label,
            icon: None,
            icon_tint: Color::white(),
            icon_inset: 4.0,
            size,
            frame: None,
            selected_panel: None,
            toggle: false,
            selected: false,
            enabled: true,
            tooltip: None,
            sounds: UiWidgetSounds::new(),
            hovered: false,
            on_pressed: None,
        }
    }

    // The sprite under assets/ui/ drawn on the frame, `inset` points in from its edges.
    pub fn with_icon<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>, inset: f32) -> Self {
        self.icon = Some(context.load_texture(sprite.into()));
        self.icon_inset = inset;
        self
    }

    pub fn with_icon_texture(mut self, icon: TextureHandle, inset: f32) -> Self {
        self.icon = Some(icon);
        self.icon_inset = inset;
        self
    }

    pub fn with_icon_tint(mut self, tint: Color) -> Self {
        self.icon_tint = tint;
        self
    }

    // Its panels per state; the style's button look if none.
    pub fn with_frame(mut self, frame: UiTextButtonFrame) -> Self {
        self.frame = Some(frame);
        self
    }

    // Drawn in place of the frame while selected.
    pub fn with_selected_panel(mut self, panel: UiNineSlice) -> Self {
        self.selected_panel = Some(panel);
        self
    }

    // Each press flips its selection, before on_pressed runs.
    pub fn with_toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // `event` (Hovered or Pressed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    pub fn on_pressed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiIconButton, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_pressed = Some(smallbox!(f));
        self
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
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
    sounds: UiWidgetSounds, // Hovered: nothing, Pressed: ButtonPressed.

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
        let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
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
    // `sprite` names its state sprites under assets/ui/buttons/, e.g. "palette/housing" for
    // buttons/palette/housing_idle.png and the other states', and labels the button. It starts
    // idle, with its press sound on, and shows a press at once.
    pub fn new(context: &mut dyn UiWidgetContext, sprite: impl Into<String>, size: Vec2) -> Self {
        let label = sprite.into();
        let stem = format!("buttons/{label}");
        Self::with_sprite_stem(context, label, &stem, size)
    }

    // A button labeled `label` (its last '/' part names it in widget paths) whose state sprites
    // are `<stem>_idle.png` and the others' under assets/ui/, e.g. a skin's
    // "skins/kambara/icons/settings". Otherwise as new().
    pub fn with_sprite_stem(context: &mut dyn UiWidgetContext, label: impl Into<String>, stem: &str, size: Vec2) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty() && !stem.is_empty());
        debug_assert!(size.x > 0.0 && size.y > 0.0);

        let textures = UiSpriteButtonTextures::load(stem, context);

        Self {
            label,
            tooltip: None,
            show_tooltip_when_pressed: false,
            size,
            position: None, // Set after the first draw().
            textures,
            sounds: UiWidgetSounds::new(),
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

    // Its sounds: ButtonPressed when pressed and nothing when hovered, unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` (Hovered or Pressed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
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

    fn play_sound(&mut self, context: &mut dyn UiWidgetContext, new_state: UiSpriteButtonState) {
        // Play state transition sound if moving to a different state.
        if self.logical_state != new_state {
            match new_state {
                UiSpriteButtonState::Pressed => self.sounds.play(context, UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed)),
                UiSpriteButtonState::Hovered => self.sounds.play(context, UiSoundEvent::Hovered, None),
                UiSpriteButtonState::Idle | UiSpriteButtonState::Disabled => {}
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
                    self.play_sound(context, UiSpriteButtonState::Pressed);
                    self.logical_state = UiSpriteButtonState::Pressed;
                } else if hovered {
                    self.play_sound(context, UiSpriteButtonState::Hovered);
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

    // `stem`: the sprites' path under assets/ui/ without the state suffix.
    fn load(stem: &str, context: &mut dyn UiWidgetContext) -> Self {
        let mut sprites = Self::unloaded();
        for state in UiSpriteButtonState::iter() {
            sprites.textures[state as usize] = context.load_texture(PathRef::from_str(&state.frame_sprite_path(stem)));
        }
        sprites
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
    // This state's sprite of a sprite button or a framed text button (UiTextButtonFrame::load),
    // `<stem>_<state>.png`, under assets/ui/.
    fn frame_sprite_path(self, sprite: &str) -> String {
        format!("{sprite}_{}.png", self.get_str("Suffix").unwrap())
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(width: f32, slice_px: [f32; 4]) -> UiNineSlice {
        UiNineSlice::new(TextureHandle::White, Vec2::new(width, 32.0), slice_px)
    }

    #[test]
    fn button_widths() {
        assert_eq!(UiTextButtonWidth::Content.resolve(80.0), 80.0);
        assert_eq!(UiTextButtonWidth::Min(120.0).resolve(80.0), 120.0);
        assert_eq!(UiTextButtonWidth::Min(120.0).resolve(150.0), 150.0);
        assert_eq!(UiTextButtonWidth::Fixed(120.0).resolve(150.0), 120.0);
    }

    #[test]
    fn frame_loads_a_sprite_per_state() {
        let paths: Vec<_> = UiSpriteButtonState::iter().map(|state| state.frame_sprite_path("skins/dailong/button_primary")).collect();
        assert_eq!(
            paths,
            [
                "skins/dailong/button_primary_idle.png",
                "skins/dailong/button_primary_disabled.png",
                "skins/dailong/button_primary_hovered.png",
                "skins/dailong/button_primary_pressed.png",
            ]
        );
    }

    #[test]
    fn frame_keeps_each_state_panel_and_pads_by_the_idle_borders() {
        let (idle, hovered, pressed, disabled) =
            (panel(10.0, [4.0, 2.0, 6.0, 3.0]), panel(11.0, [1.0; 4]), panel(12.0, [1.0; 4]), panel(13.0, [1.0; 4]));
        let frame = UiTextButtonFrame::new(idle, hovered, pressed, disabled);

        assert_eq!(*frame.panel(UiSpriteButtonState::Idle).0, idle);
        assert_eq!(*frame.panel(UiSpriteButtonState::Hovered).0, hovered);
        assert_eq!(*frame.panel(UiSpriteButtonState::Pressed).0, pressed);
        assert_eq!(*frame.panel(UiSpriteButtonState::Disabled).0, disabled);
        assert_eq!(frame.panel(UiSpriteButtonState::Pressed).1, Color::white());

        // The idle panel's wider border on each axis, scaled to points.
        assert_eq!(frame.padding(), Vec2::new(6.0, 3.0));
        assert_eq!(frame.with_scale(0.5).padding(), Vec2::new(3.0, 1.5));
        assert_eq!(frame.with_padding(Vec2::new(8.0, 4.0)).padding(), Vec2::new(8.0, 4.0));
    }
}
