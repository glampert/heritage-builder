// Settings controls drawn by the engine rather than egui: a pill toggle (UiToggle) and a step
// selector (UiStepSelector). Their art comes from the caller (a skin's sprites or colours).

use std::any::Any;
use smallbox::smallbox;
use egui::{Align2, Color32, Sense, Stroke, StrokeKind, pos2, vec2};

use common::{Color, Rect, Vec2};

use super::{UiWidget, UiWidgetCallback, UiWidgetContext, list_index, widget_id};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiString, UiSystem,
        immediate::{self, color32, egui_rect, to_rect, uv_full},
        internal,
        renderer::egui_texture,
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
        text::UiTextCategory,
    },
};

// Between a control's label, its state text and its graphic.
const CONTROL_GAP: f32 = 10.0;

// ----------------------------------------------
// UiToggleLook
// ----------------------------------------------

// A toggle's pill: sprites for off and on with a knob sprite over them (a skin's
// controls.toggle), or painted from colours.
#[derive(Copy, Clone, Debug)]
pub enum UiToggleLook {
    Sprites { off: TextureHandle, on: TextureHandle, knob: TextureHandle, size: Vec2, knob_size: Vec2 },
    Painted { on: Color, off: Color, knob: Color, outline: Color, size: Vec2 },
}

impl Default for UiToggleLook {
    fn default() -> Self {
        Self::Painted {
            on: Color::new(0.66, 0.15, 0.11, 1.0),
            off: Color::new(0.95, 0.89, 0.74, 1.0),
            knob: Color::new(1.0, 0.98, 0.93, 1.0),
            outline: Color::new(0.42, 0.31, 0.2, 1.0),
            size: Vec2::new(44.0, 22.0),
        }
    }
}

impl UiToggleLook {
    pub fn size(&self) -> Vec2 {
        match *self {
            Self::Sprites { size, .. } | Self::Painted { size, .. } => size,
        }
    }
}

// Where a toggle's knob sits on its pill: at the left end when off, the right end when on, as far
// from the ends as from the top and bottom.
pub fn toggle_knob_rect(pill: Rect, knob_size: Vec2, on: bool) -> Rect {
    let inset = ((pill.height() - knob_size.y) * 0.5).max(0.0);
    let x = if on { pill.max.x - inset - knob_size.x } else { pill.min.x + inset };
    Rect::from_pos_and_size(Vec2::new(x, pill.center().y - knob_size.y * 0.5), knob_size)
}

// ----------------------------------------------
// UiToggle
// ----------------------------------------------

pub type UiToggleReadValue = UiWidgetCallback<dyn Fn(&UiToggle, &mut dyn UiWidgetContext) -> bool>;
pub type UiToggleUpdateValue = UiWidgetCallback<dyn Fn(&UiToggle, &mut dyn UiWidgetContext, bool)>;

// A pill switch: its label on the left and, on the right, its state's text ("On", "Off") then the
// pill, as the settings mockups show. Click anywhere on it to flip it. As wide as its contents,
// or the menu's width (with_fill_width) so a column of them lines up. Recorded as kind "toggle",
// its value whether it's on, which ui.set takes. Plays Toggled.
pub struct UiToggle {
    label: UiString,
    on_text: UiString,
    off_text: UiString,
    font_scale: UiFontScale,
    look: UiToggleLook,
    text_color: Option<Color>,
    state_text_color: Option<Color>,
    fill_width: bool,
    enabled: bool,
    sounds: UiWidgetSounds, // Toggled: Toggled.
    on_read_value: UiToggleReadValue,
    on_update_value: UiToggleUpdateValue,
}

impl UiWidget for UiToggle {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let value = match &self.on_read_value {
            Some(on_read_value) => on_read_value(self, context),
            None => false,
        };

        let ui_sys = context.ui_sys();
        let size = self.measure(context);
        let font = internal::font(ui_sys, self.font_scale);
        let enabled = self.enabled;

        let (clicked, set_value) = ui_sys.with_ui(|ui| {
            let width = if self.fill_width { size.x.max(immediate::fill_size(ui).x) } else { size.x };
            let sense = if enabled { Sense::click() } else { Sense::hover() };
            let (rect, response) = ui.allocate_exact_size(vec2(width, size.y), sense);

            if ui.is_rect_visible(rect) {
                let fade = |color: Color32| if enabled { color } else { color.gamma_multiply(0.5) };
                let text_color = fade(self.text_color.map_or_else(|| ui.visuals().text_color(), color32));
                let state_color = fade(self.state_text_color.map_or(text_color, color32));

                let pill_size = self.look.size();
                let pill = egui::Rect::from_min_size(pos2(rect.right() - pill_size.x, rect.center().y - pill_size.y * 0.5), vec2(pill_size.x, pill_size.y));
                self.paint_pill(ui, pill, value, enabled);

                let state = if value { self.on_text.resolve() } else { self.off_text.resolve() };
                ui.painter().text(pos2(pill.left() - CONTROL_GAP, rect.center().y), Align2::RIGHT_CENTER, state, font.clone(), state_color);
                ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, self.label.resolve(), font.clone(), text_color);
            }

            let set_value = immediate::record(ui_sys, ui, &response, "toggle", self.label.resolve(), None, enabled, Some(serde_json::Value::Bool(value)));
            (response.clicked() && enabled, set_value)
        });

        let new_value = if clicked { Some(!value) } else { set_value.and_then(|value| value.as_bool()) };
        if let Some(new_value) = new_value {
            if new_value != value {
                self.sounds.play(context, UiSoundEvent::Toggled, Some(UiSoundKey::Toggled));
            }
            if let Some(on_update_value) = &self.on_update_value {
                on_update_value(self, context, new_value);
            }
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let pill = self.look.size();
        let label = internal::calc_text_size(ui_sys, self.font_scale, self.label.resolve()).0;
        let on = internal::calc_text_size(ui_sys, self.font_scale, self.on_text.resolve()).0;
        let off = internal::calc_text_size(ui_sys, self.font_scale, self.off_text.resolve()).0;

        let state_width = on.x.max(off.x);
        let mut width = pill.x;
        if state_width > 0.0 {
            width += state_width + CONTROL_GAP;
        }
        if label.x > 0.0 {
            width += label.x + CONTROL_GAP * 2.0;
        }
        let height = pill.y.max(internal::calc_text_line_height(ui_sys, self.font_scale));
        if self.fill_width {
            width = width.max(internal::available_size(ui_sys).x);
        }
        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiToggle {
    fn default() -> Self {
        Self::new()
    }
}

impl UiToggle {
    // Off unless on_read_value says otherwise; on_update_value gets the user's changes. Its state
    // texts are the strings' "toggle.on" and "toggle.off".
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            on_text: UiString::key(UiTextCategory::Menu, "toggle.on"),
            off_text: UiString::key(UiTextCategory::Menu, "toggle.off"),
            font_scale: UiFontScale::default(),
            look: UiToggleLook::default(),
            text_color: None,
            state_text_color: None,
            fill_width: false,
            enabled: true,
            sounds: UiWidgetSounds::new(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // On its left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    // The state texts left of the pill; empty texts show none.
    pub fn with_state_texts(mut self, on: impl Into<UiString>, off: impl Into<UiString>) -> Self {
        self.on_text = on.into();
        self.off_text = off.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    pub fn with_look(mut self, look: UiToggleLook) -> Self {
        self.look = look;
        self
    }

    // The label's colour, and the state text's (the label's if None); the style's by default.
    pub fn with_text_colors(mut self, label: Option<Color>, state: Option<Color>) -> Self {
        self.text_color = label;
        self.state_text_color = state;
        self
    }

    // As wide as the menu, its pill at the right edge.
    pub fn with_fill_width(mut self, fill: bool) -> Self {
        self.fill_width = fill;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    // `event` (Toggled) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    pub fn on_read_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiToggle, &mut dyn UiWidgetContext) -> bool + 'static,
    {
        self.on_read_value = Some(smallbox!(f));
        self
    }

    pub fn on_update_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiToggle, &mut dyn UiWidgetContext, bool) + 'static,
    {
        self.on_update_value = Some(smallbox!(f));
        self
    }

    fn paint_pill(&self, ui: &egui::Ui, pill: egui::Rect, on: bool, enabled: bool) {
        let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(128) };
        match self.look {
            UiToggleLook::Sprites { off, on: on_sprite, knob, knob_size, .. } => {
                let texture = if on { on_sprite } else { off };
                ui.painter().image(egui_texture(texture), pill, uv_full(), tint);
                let knob_rect = toggle_knob_rect(to_rect(pill), knob_size, on);
                ui.painter().image(egui_texture(knob), egui_rect(knob_rect), uv_full(), tint);
            }
            UiToggleLook::Painted { on: on_color, off, knob, outline, .. } => {
                let radius = pill.height() * 0.5;
                let fill = color32(if on { on_color } else { off }).linear_multiply(tint.a() as f32 / 255.0);
                ui.painter().rect_filled(pill, radius, fill);
                ui.painter().rect_stroke(pill, radius, Stroke::new(1.5, color32(outline)), StrokeKind::Inside);
                let knob_size = Vec2::new(pill.height() - 6.0, pill.height() - 6.0);
                let knob_rect = egui_rect(toggle_knob_rect(to_rect(pill), knob_size, on));
                ui.painter().circle_filled(knob_rect.center(), knob_rect.width() * 0.5, color32(knob));
                ui.painter().circle_stroke(knob_rect.center(), knob_rect.width() * 0.5, Stroke::new(1.0, color32(outline)));
            }
        }
    }
}

// ----------------------------------------------
// UiStepSelector
// ----------------------------------------------

// The step `delta` steps from `current` among `count`: wrapping round the ends, or held at them.
pub fn step_index(current: usize, delta: i32, count: usize, wrap: bool) -> usize {
    if count == 0 {
        return 0;
    }
    let target = current.min(count - 1) as i64 + delta as i64;
    if wrap { target.rem_euclid(count as i64) as usize } else { target.clamp(0, count as i64 - 1) as usize }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiStepSelectorStyle {
    // A row of segments under the label, filled up to the current one, with its text on the
    // right of the label: difficulty, as the Custom Game mockup shows.
    #[default]
    Segments,
    // The current step's text between a previous and a next arrow, right of the label.
    Arrows,
}

// The segments' and arrows' colours.
#[derive(Copy, Clone, Debug)]
pub struct UiStepLook {
    pub fill: Color,
    pub track: Color,
    pub outline: Option<Color>,
    pub segment_height: f32,
}

impl Default for UiStepLook {
    fn default() -> Self {
        Self {
            fill: Color::new(0.66, 0.15, 0.11, 1.0),
            track: Color::new(0.95, 0.89, 0.74, 1.0),
            outline: Some(Color::new(0.42, 0.31, 0.2, 1.0)),
            segment_height: 10.0,
        }
    }
}

pub type UiStepSelectorChanged = UiWidgetCallback<dyn Fn(&UiStepSelector, &mut dyn UiWidgetContext, usize)>;

// One of a few steps: N numbered steps or a list of labels (Easy … Very Hard). Segments (click
// one) or arrows (click previous or next; with_wrap goes round the ends). Recorded as kind
// "step_selector", its value the current step's label, which ui.set takes a step's label or index
// on; its parts are recorded under its label: segments as kind "step" (value: whether current),
// arrows as buttons "Previous" and "Next". Plays SliderChanged when the step changes.
pub struct UiStepSelector {
    label: UiString,
    steps: Vec<UiString>,
    current: usize,
    style: UiStepSelectorStyle,
    wrap: bool,
    look: UiStepLook,
    font_scale: UiFontScale,
    text_color: Option<Color>,
    value_color: Option<Color>,
    width: Option<f32>, // The menu's width if None.
    sounds: UiWidgetSounds, // Changed: SliderChanged.
    on_changed: UiStepSelectorChanged,
}

impl UiWidget for UiStepSelector {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let size = self.measure(context);
        let font = internal::font(ui_sys, self.font_scale);
        let line = internal::calc_text_line_height(ui_sys, self.font_scale);
        let id = widget_id(self, self.label.id_source());
        let labels: Vec<&str> = self.steps.iter().map(UiString::resolve).collect();
        let current = self.current.min(labels.len().saturating_sub(1));

        let (picked, set_value) = ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x).max(size.x);
            let (rect, response) = ui.allocate_exact_size(vec2(width, size.y), Sense::hover());
            let current_label = labels.get(current).copied().unwrap_or_default();
            let set_value = immediate::record(
                ui_sys,
                ui,
                &response,
                "step_selector",
                self.label.resolve(),
                None,
                true,
                Some(serde_json::Value::String(current_label.to_string())),
            );

            let text_color = self.text_color.map_or_else(|| ui.visuals().text_color(), color32);
            let value_color = self.value_color.map_or(text_color, color32);
            let label_rect = egui::Rect::from_min_size(rect.min, vec2(width, line));
            ui.painter().text(label_rect.left_center(), Align2::LEFT_CENTER, self.label.resolve(), font.clone(), text_color);

            let _path = immediate::path_scope(ui_sys, self.label.resolve());
            let mut picked = None;
            match self.style {
                UiStepSelectorStyle::Segments => {
                    ui.painter().text(label_rect.right_center(), Align2::RIGHT_CENTER, current_label, font.clone(), value_color);

                    let count = labels.len().max(1);
                    let gap = 6.0;
                    let segment_width = ((width - gap * (count - 1) as f32) / count as f32).max(1.0);
                    let top = rect.top() + line + 4.0;
                    for (index, label) in labels.iter().enumerate() {
                        let segment = egui::Rect::from_min_size(pos2(rect.left() + (segment_width + gap) * index as f32, top), vec2(segment_width, self.look.segment_height));
                        // Clicks land on the segment and a little round it.
                        let hit = segment.expand2(vec2(gap * 0.5, 3.0)).intersect(rect.expand2(vec2(0.0, 3.0)));
                        let response = ui.interact(hit, id.with(index), Sense::click());
                        let color = if index <= current { self.look.fill } else { self.look.track };
                        let fill = if response.hovered() && index != current { color32(color).gamma_multiply(0.85) } else { color32(color) };
                        ui.painter().rect_filled(segment, 2.0, fill);
                        if let Some(outline) = self.look.outline {
                            ui.painter().rect_stroke(segment, 2.0, Stroke::new(1.0, color32(outline)), StrokeKind::Inside);
                        }
                        immediate::record(ui_sys, ui, &response, "step", label, None, true, Some(serde_json::Value::Bool(index == current)));
                        if response.clicked() {
                            picked = Some(index);
                        }
                    }
                }
                UiStepSelectorStyle::Arrows => {
                    let arrow = vec2(line, line);
                    let value_width = self.value_width(ui_sys);
                    let next = egui::Rect::from_min_size(pos2(rect.right() - arrow.x, rect.top()), arrow);
                    let value = egui::Rect::from_min_size(pos2(next.left() - CONTROL_GAP - value_width, rect.top()), vec2(value_width, line));
                    let previous = egui::Rect::from_min_size(pos2(value.left() - CONTROL_GAP - arrow.x, rect.top()), arrow);
                    ui.painter().text(value.center(), Align2::CENTER_CENTER, current_label, font.clone(), value_color);

                    for (arrow_rect, delta, name) in [(previous, -1, "Previous"), (next, 1, "Next")] {
                        let target = step_index(current, delta, labels.len(), self.wrap);
                        let enabled = target != current;
                        let response = ui.interact(arrow_rect, id.with(name), if enabled { Sense::click() } else { Sense::hover() });
                        let mut color = if response.hovered() && enabled { self.look.fill } else { self.look.outline.unwrap_or(self.look.fill) };
                        if !enabled {
                            color.a *= 0.35;
                        }
                        let triangle = arrow_rect.shrink(arrow_rect.width() * 0.22);
                        // egui's triangle points down: turned a quarter to point left or right.
                        let rotation = if delta < 0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
                        ui.painter().add(egui::Shape::rotated_triangle(triangle, rotation, color32(color)));
                        immediate::record(ui_sys, ui, &response, "button", name, None, enabled, None);
                        if response.clicked() && enabled {
                            picked = Some(target);
                        }
                    }
                }
            }
            (picked, set_value)
        });

        let picked = picked.or_else(|| set_value.and_then(|value| list_index(&value, &labels)));
        if let Some(index) = picked {
            self.select(context, index);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let line = internal::calc_text_line_height(ui_sys, self.font_scale);
        let label = internal::calc_text_size(ui_sys, self.font_scale, self.label.resolve()).0.x;
        let width = self.width.unwrap_or_else(|| internal::available_size(ui_sys).x);
        match self.style {
            UiStepSelectorStyle::Segments => {
                let min_width = label + CONTROL_GAP + self.value_width(ui_sys);
                Vec2::new(width.max(min_width), line + 4.0 + self.look.segment_height)
            }
            UiStepSelectorStyle::Arrows => {
                let min_width = label + CONTROL_GAP * 3.0 + line * 2.0 + self.value_width(ui_sys);
                Vec2::new(width.max(min_width), line)
            }
        }
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiStepSelector {
    // `steps` labeled 1 to `steps`, at the first.
    pub fn numbered(label: impl Into<UiString>, steps: usize) -> Self {
        Self::new(label, (1..=steps).map(|step| UiString::literal(step.to_string())))
    }

    // A step per label, at the first. An empty list (a bug) draws nothing and reports no label.
    pub fn new(label: impl Into<UiString>, steps: impl IntoIterator<Item = UiString>) -> Self {
        let steps: Vec<UiString> = steps.into_iter().collect();
        debug_assert!(!steps.is_empty());
        Self {
            label: label.into(),
            steps,
            current: 0,
            style: UiStepSelectorStyle::Segments,
            wrap: false,
            look: UiStepLook::default(),
            font_scale: UiFontScale::default(),
            text_color: None,
            value_color: None,
            width: None,
            sounds: UiWidgetSounds::new(),
            on_changed: None,
        }
    }

    pub fn with_current(mut self, index: usize) -> Self {
        self.current = index.min(self.steps.len().saturating_sub(1));
        self
    }

    pub fn with_style(mut self, style: UiStepSelectorStyle) -> Self {
        self.style = style;
        self
    }

    // Arrows go round the ends.
    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn with_look(mut self, look: UiStepLook) -> Self {
        self.look = look;
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // The label's colour and the current step's (the label's if None); the style's by default.
    pub fn with_text_colors(mut self, label: Option<Color>, value: Option<Color>) -> Self {
        self.text_color = label;
        self.value_color = value;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    // `event` (Changed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // Called with the step before, when the user (or ui.set) picks another.
    pub fn on_changed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiStepSelector, &mut dyn UiWidgetContext, usize) + 'static,
    {
        self.on_changed = Some(smallbox!(f));
        self
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn current_label(&self) -> &str {
        self.steps.get(self.current).map_or("", UiString::resolve)
    }

    // Sets the step without a sound or a callback.
    pub fn set_current(&mut self, index: usize) {
        self.current = index.min(self.steps.len().saturating_sub(1));
    }

    // Moves `delta` steps, wrapping or held at the ends as the selector does: e.g. for keys.
    pub fn step_by(&mut self, context: &mut dyn UiWidgetContext, delta: i32) {
        let index = step_index(self.current, delta, self.steps.len(), self.wrap);
        self.select(context, index);
    }

    fn select(&mut self, context: &mut dyn UiWidgetContext, index: usize) {
        if index >= self.steps.len() || index == self.current {
            return;
        }
        let previous = self.current;
        self.current = index;
        self.sounds.play(context, UiSoundEvent::Changed, Some(UiSoundKey::SliderChanged));
        if let Some(on_changed) = &self.on_changed {
            on_changed(self, context, previous);
        }
    }

    // The widest step label.
    fn value_width(&self, ui_sys: &UiSystem) -> f32 {
        self.steps.iter().map(|step| internal::calc_text_size(ui_sys, self.font_scale, step.resolve()).0.x).fold(0.0, f32::max)
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wrap_or_hold_at_the_ends() {
        assert_eq!(step_index(2, 1, 5, false), 3);
        assert_eq!(step_index(4, 1, 5, false), 4);
        assert_eq!(step_index(0, -1, 5, false), 0);
        assert_eq!(step_index(4, 1, 5, true), 0);
        assert_eq!(step_index(0, -1, 5, true), 4);
        assert_eq!(step_index(1, -7, 5, true), 4);
        assert_eq!(step_index(9, 0, 5, false), 4); // Out of range: held at the last.
        assert_eq!(step_index(0, 1, 0, true), 0);
    }

    #[test]
    fn toggle_knob_sits_at_the_pill_ends() {
        let pill = Rect::from_pos_and_size(Vec2::new(10.0, 20.0), Vec2::new(60.0, 30.0));
        let knob = Vec2::new(24.0, 24.0);
        assert_eq!(toggle_knob_rect(pill, knob, false), Rect::from_pos_and_size(Vec2::new(13.0, 23.0), knob));
        assert_eq!(toggle_knob_rect(pill, knob, true), Rect::from_pos_and_size(Vec2::new(43.0, 23.0), knob));
    }

    #[test]
    fn step_selector_clamps_its_current_step() {
        let mut selector = UiStepSelector::numbered("Difficulty", 3).with_current(9);
        assert_eq!((selector.current(), selector.current_label()), (2, "3"));
        selector.set_current(1);
        assert_eq!(selector.current_label(), "2");
    }
}
