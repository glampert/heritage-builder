use std::{any::Any, fmt::Display, ops::RangeInclusive};
use smallbox::smallbox;
use egui::{Align2, Color32, Sense, pos2, vec2};

use common::{Color, Vec2};

use super::{UiWidget, UiWidgetCallback, UiWidgetContext, left_label, list_index, record_settable, widget_id};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiNineSlice, UiString, immediate::{self, color32, nine_slice_shape, uv_full}, internal,
        renderer::egui_texture,
        sound::{self, UiSoundEvent, UiSoundKey, UiWidgetSounds},
    },
};

// ----------------------------------------------
// Control looks
// ----------------------------------------------

// The art of the controls below, from the caller (a skin's controls block): without one, egui's
// own widget draws in the style's visuals. Either way the widget records and takes ui.set alike.

// A checkbox's sprites, off and on, at `size` points.
#[derive(Copy, Clone, Debug)]
pub struct UiCheckboxLook {
    pub off: TextureHandle,
    pub on: TextureHandle,
    pub size: Vec2,
}

// A slider's rail and the filled part of it (9-slices, `rail_height` and `fill_height` points
// tall, centred on the knob), its knob sprite at `knob_size` points, and the colour of the value
// shown to its right.
#[derive(Copy, Clone, Debug)]
pub struct UiSliderLook {
    pub rail: UiNineSlice,
    pub fill: UiNineSlice,
    pub rail_height: f32,
    pub fill_height: f32,
    pub knob: TextureHandle,
    pub knob_size: Vec2,
    pub text_color: Option<Color>,
}

// A dropdown's field (a 9-slice behind its current item), its arrow sprite at `arrow_size` points,
// its text colour, and the room left of its text (the style's button padding if 0). Its list is
// the style's popup.
#[derive(Copy, Clone, Debug)]
pub struct UiDropdownLook {
    pub field: UiNineSlice,
    pub arrow: TextureHandle,
    pub arrow_size: Vec2,
    pub text_color: Option<Color>,
    pub text_inset: f32,
}

// A text field's panel (a 9-slice behind the text), the room between its edges and the text, the
// text's colour, and the colour of the line drawn round the field while it has keyboard focus.
#[derive(Copy, Clone, Debug)]
pub struct UiTextInputLook {
    pub field: UiNineSlice,
    pub padding: Vec2,
    pub text_color: Option<Color>,
    pub focus_color: Option<Color>,
}

// Between a settings row's slider value and its rail (UiControlRow), in points.
const VALUE_GAP: f32 = 8.0;

// The width and corner radius of a focused field's line, in points.
const FOCUS_LINE: (f32, f32) = (2.5, 5.0);

// A light wash over a hovered control's art.
const HOVER_WASH: Color32 = Color32::from_rgba_premultiplied(28, 28, 28, 28);

// `texture` stretched over `rect`.
fn paint_sprite(ui: &egui::Ui, texture: TextureHandle, rect: egui::Rect) {
    ui.painter().image(egui_texture(texture), rect, uv_full(), Color32::WHITE);
}

// ----------------------------------------------
// UiControlRow
// ----------------------------------------------

// A slider or dropdown laid out as a settings row, as the settings mockups draw them: its label at
// the left edge and the control, `control_width` points wide, at the right edge of the room it's
// in, the row at least `min_height` points tall with both centred in it. Such a control is as
// wide as that room (a UiToggle with_fill_width lines up with it).
#[derive(Copy, Clone, Debug)]
pub struct UiControlRow {
    pub control_width: f32,
    pub min_height: f32,
}

impl UiControlRow {
    pub fn new(control_width: f32, min_height: f32) -> Self {
        Self { control_width: control_width.max(0.0), min_height: min_height.max(0.0) }
    }

    fn begin(&self, ui: &mut egui::Ui) {
        ui.set_min_height(self.min_height);
    }

    // After the label: the room up to where a control `width` points wide ends at the right edge.
    fn push_to_right(&self, ui: &mut egui::Ui, width: f32) {
        let gap = ui.available_width() - width;
        if gap > 0.0 {
            ui.add_space(gap);
        }
    }

    fn measure(&self, ui_sys: &crate::ui::UiSystem, font_scale: UiFontScale, label: &str, height: f32) -> Vec2 {
        let label_width = internal::calc_text_size(ui_sys, font_scale, label).0.x;
        let spacing = ui_sys.egui_style().spacing.item_spacing.x;
        let width = (label_width + spacing * 2.0 + self.control_width).max(internal::available_size(ui_sys).x);
        Vec2::new(width, height.max(self.min_height))
    }
}

// ----------------------------------------------
// UiSliderValue
// ----------------------------------------------

// A slider's value type: an integer or a float. The slider holds its range and value as f64 and
// converts for its callbacks.
pub trait UiSliderValue: Copy + 'static {
    const INTEGRAL: bool;
    fn to_f64(self) -> f64;
    fn from_f64(value: f64) -> Self;
}

macro_rules! impl_ui_slider_value {
    ($($t:ty => $integral:literal),* $(,)?) => {
        $(
            impl UiSliderValue for $t {
                const INTEGRAL: bool = $integral;

                #[inline]
                fn to_f64(self) -> f64 {
                    self as f64
                }

                #[inline]
                fn from_f64(value: f64) -> Self {
                    value as $t
                }
            }
        )*
    };
}

impl_ui_slider_value! { i32 => true, u32 => true, i64 => true, u64 => true, f32 => false, f64 => false }

// ----------------------------------------------
// UiSlider
// ----------------------------------------------

pub type UiSliderReadValue   = UiWidgetCallback<dyn Fn(&UiSlider, &mut dyn UiWidgetContext) -> f64>;
pub type UiSliderUpdateValue = UiWidgetCallback<dyn Fn(&UiSlider, &mut dyn UiWidgetContext, f64)>;

pub struct UiSlider {
    label: UiString,
    font_scale: UiFontScale,
    min: f64,
    max: f64,
    integral: bool, // Shown and set in whole numbers.
    look: Option<UiSliderLook>,
    value_suffix: String, // After the value shown: "%".
    row: Option<UiControlRow>,
    sounds: UiWidgetSounds, // Changed: SliderChanged, at most once per its cooldown.
    on_read_value: UiSliderReadValue,
    on_update_value: UiSliderUpdateValue,
}

impl UiWidget for UiSlider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);

        let value = match &self.on_read_value {
            Some(on_read_value) => on_read_value(self, context),
            None => 0.0,
        };
        let (new_value, set_value) = self.draw_slider(context, value);

        if let Some(new_value) = new_value {
            self.sounds.play(context, UiSoundEvent::Changed, Some(UiSoundKey::SliderChanged));
            self.update_value(context, new_value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_f64()) {
            let new_value = if self.integral { new_value.round() } else { new_value };
            if new_value.clamp(self.min, self.max) != value {
                self.sounds.play(context, UiSoundEvent::Changed, Some(UiSoundKey::SliderChanged));
            }
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let size = internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, self.label.resolve());
        let size = match &self.look {
            Some(look) => Vec2::new(size.x, size.y.max(look.knob_size.y)),
            None => size,
        };
        match self.row {
            Some(row) => row.measure(context.ui_sys(), self.font_scale, self.label.resolve(), size.y),
            None => size,
        }
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiSlider {
    // A slider over `range`, shown in whole numbers for an integer range and with two decimals
    // for a float one. It shows the value on_read_value gives and reports moves to on_update_value.
    pub fn new<T: UiSliderValue>(range: RangeInclusive<T>) -> Self {
        let min = range.start().to_f64();
        let max = range.end().to_f64();
        debug_assert!(min <= max);

        Self {
            label: UiString::default(),
            font_scale: UiFontScale::default(),
            min,
            max,
            integral: T::INTEGRAL,
            look: None,
            value_suffix: String::new(),
            row: None,
            sounds: UiWidgetSounds::new(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // Its rail, fill and knob sprites, in place of egui's slider.
    pub fn with_look(mut self, look: UiSliderLook) -> Self {
        self.look = Some(look);
        self
    }

    // To the slider's left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // Text after the value it shows, e.g. "%".
    pub fn with_value_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.value_suffix = suffix.into();
        self
    }

    // Laid out as a settings row (UiControlRow): its label at the left, the value then the slider
    // at the right edge.
    pub fn with_row(mut self, row: UiControlRow) -> Self {
        self.row = Some(row);
        self
    }

    // Its sounds: SliderChanged when moved (event Changed), unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // Reads the value the slider shows, each frame.
    pub fn on_read_value<T, F>(mut self, f: F) -> Self
    where
        T: UiSliderValue,
        F: Fn(&UiSlider, &mut dyn UiWidgetContext) -> T + 'static,
    {
        self.on_read_value = Some(smallbox!(move |slider: &UiSlider, context: &mut dyn UiWidgetContext| {
            f(slider, context).to_f64()
        }));
        self
    }

    // Called with the new value, clamped to the range, when the slider is moved.
    pub fn on_update_value<T, F>(mut self, f: F) -> Self
    where
        T: UiSliderValue,
        F: Fn(&UiSlider, &mut dyn UiWidgetContext, T) + 'static,
    {
        self.on_update_value = Some(smallbox!(move |slider: &UiSlider, context: &mut dyn UiWidgetContext, value: f64| {
            f(slider, context, T::from_f64(value))
        }));
        self
    }

    // Gives on_update_value the value, clamped to the range.
    fn update_value(&self, context: &mut dyn UiWidgetContext, value: f64) {
        if let Some(on_update_value) = &self.on_update_value {
            on_update_value(self, context, value.clamp(self.min, self.max));
        }
    }

    // A left label, then the slider. Returns the value the user set, if changed, and a value ui.set
    // queued for it.
    fn draw_slider(&self, context: &mut dyn UiWidgetContext, mut value: f64) -> (Option<f64>, Option<serde_json::Value>) {
        let ui_sys = context.ui_sys();
        let width = self.row.map_or_else(|| internal::item_width(ui_sys), |row| row.control_width);
        // A row shows the value, its widest, to the slider's left; otherwise it shows to its right.
        let value_width = match self.row {
            Some(_) => {
                let widest = [self.min, self.max].map(|value| self.value_text(value));
                let text_width = widest.iter().map(|text| internal::calc_text_size(ui_sys, self.font_scale, text).0.x).fold(0.0, f32::max);
                text_width + VALUE_GAP
            }
            None => ui_sys.egui_style().spacing.interact_size.x,
        };

        ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    if let Some(row) = self.row {
                        row.begin(ui);
                    }
                    left_label(ui_sys, ui, self.label.resolve());
                    if let Some(row) = self.row {
                        row.push_to_right(ui, width);
                    }

                    let slider_width = (width - value_width - ui.spacing().item_spacing.x).max(40.0);
                    if let Some(look) = &self.look {
                        return self.draw_skinned_slider(ui_sys, ui, look, slider_width, value_width, &mut value);
                    }
                    ui.spacing_mut().slider_width = slider_width;
                    let slider = egui::Slider::new(&mut value, self.min..=self.max).clamping(egui::SliderClamping::Always);
                    let slider = if self.integral { slider.integer() } else { slider.fixed_decimals(2) };
                    ui.add(slider)
                })
                .inner;

            let changed = response.changed().then_some(value);
            let json_value = if self.integral { serde_json::json!(value as i64) } else { serde_json::json!(value) };
            let set_value = record_settable(ui_sys, ui, &response, "slider", self.label.resolve(), json_value);
            (changed, set_value)
        })
    }
}

impl UiSlider {
    // The slider in its look's sprites: a rail `width` points wide with the filled part up to the
    // knob, then the value as text. Click or drag anywhere on the rail to move the knob there.
    fn value_text(&self, value: f64) -> String {
        if self.integral { format!("{:.0}{}", value, self.value_suffix) } else { format!("{:.2}{}", value, self.value_suffix) }
    }

    fn draw_skinned_slider(
        &self,
        ui_sys: &crate::ui::UiSystem,
        ui: &mut egui::Ui,
        look: &UiSliderLook,
        width: f32,
        value_width: f32,
        value: &mut f64,
    ) -> egui::Response {
        let height = look.knob_size.y.max(ui.spacing().interact_size.y);
        let font = internal::font(ui_sys, self.font_scale);
        let color = look.text_color.map_or_else(|| ui.visuals().text_color(), color32);

        // In a row, the value first, right-aligned against the slider.
        let value_slot = self.row.is_some().then(|| ui.allocate_exact_size(vec2(value_width, height), Sense::hover()).0);

        let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        let knob_width = look.knob_size.x.min(rect.width());
        let travel = (rect.width() - knob_width).max(1.0);
        if let Some(pointer) = response.interact_pointer_pos().filter(|_| response.dragged() || response.clicked()) {
            let fraction = ((pointer.x - rect.left() - knob_width * 0.5) / travel).clamp(0.0, 1.0) as f64;
            let mut new_value = self.min + fraction * (self.max - self.min);
            if self.integral {
                new_value = new_value.round();
            }
            if new_value != *value {
                *value = new_value;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let span = self.max - self.min;
            let fraction = if span > 0.0 { ((*value - self.min) / span).clamp(0.0, 1.0) as f32 } else { 0.0 };
            let knob_center = rect.left() + knob_width * 0.5 + travel * fraction;

            let rail_height = look.rail_height.min(rect.height());
            let rail = egui::Rect::from_min_max(pos2(rect.left(), rect.center().y - rail_height * 0.5), pos2(rect.right(), rect.center().y + rail_height * 0.5));
            ui.painter().add(nine_slice_shape(rail, &look.rail, Color32::WHITE));
            let fill_height = look.fill_height.min(rail_height);
            let fill_inset = (rail_height - fill_height) * 0.5;
            let fill = egui::Rect::from_min_max(
                pos2(rail.left() + fill_inset, rail.top() + fill_inset),
                pos2(knob_center.max(rail.left() + fill_inset), rail.bottom() - fill_inset),
            );
            if fill.width() > 0.5 {
                ui.painter().add(nine_slice_shape(fill, &look.fill, Color32::WHITE));
            }
            let knob = egui::Rect::from_center_size(pos2(knob_center, rect.center().y), vec2(look.knob_size.x, look.knob_size.y));
            paint_sprite(ui, look.knob, knob);
            if response.hovered() || response.dragged() {
                ui.painter().circle_filled(knob.center(), knob.width().min(knob.height()) * 0.5, HOVER_WASH);
            }
        }

        // The value, in the space egui's slider gives its value field.
        let text = self.value_text(*value);
        match value_slot {
            Some(slot) => {
                ui.painter().text(slot.right_center() - vec2(VALUE_GAP, 0.0), Align2::RIGHT_CENTER, text, font, color);
            }
            None => {
                let (text_rect, _) = ui.allocate_exact_size(vec2(value_width, height), Sense::hover());
                ui.painter().text(text_rect.left_center(), Align2::LEFT_CENTER, text, font, color);
            }
        }

        response
    }
}

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub type UiCheckboxReadValue   = UiWidgetCallback<dyn Fn(&UiCheckbox, &mut dyn UiWidgetContext) -> bool>;
pub type UiCheckboxUpdateValue = UiWidgetCallback<dyn Fn(&UiCheckbox, &mut dyn UiWidgetContext, bool)>;

pub struct UiCheckbox {
    label: UiString,
    font_scale: UiFontScale,
    look: Option<UiCheckboxLook>,
    sounds: UiWidgetSounds, // Toggled: Toggled.
    on_read_value: UiCheckboxReadValue,
    on_update_value: UiCheckboxUpdateValue,
}

impl UiWidget for UiCheckbox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();

        let mut value = match &self.on_read_value {
            Some(on_read_value) => on_read_value(self, context),
            None => false,
        };

        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, self.label.resolve());
                    match &self.look {
                        Some(look) => {
                            let (rect, mut response) = ui.allocate_exact_size(vec2(look.size.x, look.size.y), Sense::click());
                            if response.clicked() {
                                value = !value;
                                response.mark_changed();
                            }
                            if ui.is_rect_visible(rect) {
                                paint_sprite(ui, if value { look.on } else { look.off }, rect);
                                if response.hovered() {
                                    ui.painter().rect_filled(rect.shrink(rect.width() * 0.08), 3.0, HOVER_WASH);
                                }
                            }
                            response
                        }
                        None => ui.checkbox(&mut value, ""),
                    }
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "checkbox", self.label.resolve(), serde_json::Value::Bool(value));
            (response.changed(), set_value)
        });

        if value_changed {
            self.sounds.play(context, UiSoundEvent::Toggled, Some(UiSoundKey::Toggled));
            self.update_value(context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_bool()) {
            if new_value != value {
                self.sounds.play(context, UiSoundEvent::Toggled, Some(UiSoundKey::Toggled));
            }
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let spacing = &ui_sys.egui_style().spacing;

        let frame_height = internal::calc_frame_height(ui_sys, self.font_scale);
        let (mut width, checkbox_square) = match &self.look {
            Some(look) => (look.size.x, look.size.y.max(frame_height)),
            None => (frame_height, frame_height),
        };

        if !self.label.is_empty() {
            let label_size = context.calc_text_size(self.font_scale, self.label.resolve());
            width += spacing.icon_spacing + label_size.x;
        }

        Vec2::new(width, checkbox_square)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiCheckbox {
    fn default() -> Self {
        Self::new()
    }
}

impl UiCheckbox {
    // A checkbox showing the value on_read_value gives; on_update_value gets the user's changes.
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            font_scale: UiFontScale::default(),
            look: None,
            sounds: UiWidgetSounds::new(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // Its off and on sprites, in place of egui's checkbox.
    pub fn with_look(mut self, look: UiCheckboxLook) -> Self {
        self.look = Some(look);
        self
    }

    // Its sounds: Toggled when toggled (event Toggled), unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // To the checkbox's left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    pub fn on_read_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiCheckbox, &mut dyn UiWidgetContext) -> bool + 'static,
    {
        self.on_read_value = Some(smallbox!(f));
        self
    }

    pub fn on_update_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiCheckbox, &mut dyn UiWidgetContext, bool) + 'static,
    {
        self.on_update_value = Some(smallbox!(f));
        self
    }

    fn update_value(&self, context: &mut dyn UiWidgetContext, value: bool) {
        if let Some(on_update_value) = &self.on_update_value {
            on_update_value(self, context, value);
        }
    }
}

// ----------------------------------------------
// UiIntInput
// ----------------------------------------------

pub type UiIntInputReadValue   = UiWidgetCallback<dyn Fn(&UiIntInput, &mut dyn UiWidgetContext) -> i32>;
pub type UiIntInputUpdateValue = UiWidgetCallback<dyn Fn(&UiIntInput, &mut dyn UiWidgetContext, i32)>;

pub struct UiIntInput {
    label: UiString,
    font_scale: UiFontScale,
    min: i32,
    max: i32,
    step: i32,
    sounds: UiWidgetSounds, // Changed: SliderChanged, for a step or a drag (not for typing).
    on_read_value: UiIntInputReadValue,
    on_update_value: UiIntInputUpdateValue,
}

impl UiWidget for UiIntInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let width = internal::item_width(ui_sys);

        let mut value = match &self.on_read_value {
            Some(on_read_value) => on_read_value(self, context),
            None => 0,
        };

        let (value_changed, stepped_or_dragged, set_value) = ui_sys.with_ui(|ui| {
            let (response, stepped) = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, self.label.resolve());

                    // A field to drag or type into, and buttons that step it.
                    let buttons_width = (ui.spacing().interact_size.y + ui.spacing().item_spacing.x) * 2.0;
                    let drag = egui::DragValue::new(&mut value).range(self.min..=self.max).speed(f64::from(self.step) * 0.1);
                    let response = ui.add_sized(vec2((width - buttons_width).max(40.0), ui.spacing().interact_size.y), drag);

                    // Each button is disabled at its end of the range, so a step always changes
                    // the value. They're recorded as buttons "-" and "+" under the field's label.
                    let mut stepped = false;
                    let _path = immediate::path_scope(ui_sys, self.label.resolve());
                    for (name, enabled) in [("-", value > self.min), ("+", value < self.max)] {
                        let button = ui.add_enabled(enabled, egui::Button::new(name).small());
                        immediate::record(ui_sys, ui, &button, "button", name, None, enabled, None);
                        if button.clicked() {
                            value = if name == "-" {
                                value.saturating_sub(self.step).max(self.min)
                            } else {
                                value.saturating_add(self.step).min(self.max)
                            };
                            stepped = true;
                        }
                    }
                    (response, stepped)
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "int_input", self.label.resolve(), serde_json::json!(value));
            let dragged = response.changed() && response.dragged();
            (response.changed() || stepped, stepped || dragged, set_value)
        });

        if value_changed {
            if stepped_or_dragged {
                self.sounds.play(context, UiSoundEvent::Changed, Some(UiSoundKey::SliderChanged));
            }
            self.update_value(context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_i64()) {
            let new_value = new_value.clamp(i64::from(self.min), i64::from(self.max)) as i32;
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, self.label.resolve())
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiIntInput {
    fn default() -> Self {
        Self::new()
    }
}

impl UiIntInput {
    // A field to type or drag a number into, with buttons that step it by one, over the whole i32
    // range. It shows the value on_read_value gives; on_update_value gets the user's changes.
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            font_scale: UiFontScale::default(),
            min: i32::MIN,
            max: i32::MAX,
            step: 1,
            sounds: UiWidgetSounds::new(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the field's left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // The values the field clamps to.
    pub fn with_range(mut self, range: RangeInclusive<i32>) -> Self {
        debug_assert!(range.start() <= range.end());
        self.min = *range.start();
        self.max = *range.end();
        self
    }

    // What the buttons add or subtract.
    pub fn with_step(mut self, step: i32) -> Self {
        debug_assert!(step > 0);
        self.step = step;
        self
    }

    // Its sounds: SliderChanged for a step or a drag (event Changed), unless overridden. Typing
    // plays nothing.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    pub fn on_read_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiIntInput, &mut dyn UiWidgetContext) -> i32 + 'static,
    {
        self.on_read_value = Some(smallbox!(f));
        self
    }

    // Called with the new value, clamped to the range.
    pub fn on_update_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiIntInput, &mut dyn UiWidgetContext, i32) + 'static,
    {
        self.on_update_value = Some(smallbox!(f));
        self
    }

    // Gives on_update_value the value, clamped to the range.
    fn update_value(&self, context: &mut dyn UiWidgetContext, value: i32) {
        if let Some(on_update_value) = &self.on_update_value {
            on_update_value(self, context, value.clamp(self.min, self.max));
        }
    }
}

// ----------------------------------------------
// UiTextInput
// ----------------------------------------------

pub type UiTextInputReadValue   = UiWidgetCallback<dyn for<'a> Fn(&UiTextInput, &'a mut dyn UiWidgetContext) -> &'a str>;
pub type UiTextInputUpdateValue = UiWidgetCallback<dyn Fn(&UiTextInput, &mut dyn UiWidgetContext, &str)>;

pub struct UiTextInput {
    label: UiString,
    show_label: bool,
    buffer: String,
    font_scale: UiFontScale,
    width: Option<f32>, // The field's; 0 takes the room left in the row.
    max_chars: Option<usize>,
    look: Option<UiTextInputLook>,
    on_read_value: UiTextInputReadValue,
    on_update_value: UiTextInputUpdateValue,
}

impl UiWidget for UiTextInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();

        self.buffer.clear();
        if let Some(on_read_value) = &self.on_read_value {
            let text = on_read_value(self, context);
            self.buffer.push_str(text);
        }

        let id = widget_id(self, self.label.id_source());
        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    if self.show_label {
                        left_label(ui_sys, ui, self.label.resolve());
                    }
                    let width = match self.width {
                        Some(width) if width > 0.0 => width,
                        Some(_) => ui.available_width(),
                        None => internal::item_width(ui_sys),
                    };
                    let mut text_edit = egui::TextEdit::singleline(&mut self.buffer)
                        .id(id)
                        .font(internal::font(ui_sys, self.font_scale))
                        .desired_width(width);
                    if let Some(max_chars) = self.max_chars {
                        text_edit = text_edit.char_limit(max_chars);
                    }

                    // With a look, its field goes under the text, whose own frame is hidden.
                    let Some(look) = self.look else {
                        return ui.add(text_edit);
                    };
                    let padding = vec2(look.padding.x, look.padding.y);
                    // The padding is the frame's inner margin; egui counts it in the desired width.
                    let margin = egui::Margin::symmetric(padding.x.round().clamp(0.0, 127.0) as i8, padding.y.round().clamp(0.0, 127.0) as i8);
                    text_edit = text_edit.frame(egui::Frame::NONE.inner_margin(margin)).desired_width(width);
                    if let Some(color) = look.text_color {
                        text_edit = text_edit.text_color(color32(color));
                    }
                    let slot = ui.painter().add(egui::Shape::Noop);
                    let response = ui.add(text_edit);
                    ui.painter().set(slot, nine_slice_shape(response.rect, &look.field, Color32::WHITE));
                    if response.hovered() && !response.has_focus() {
                        ui.painter().rect_filled(response.rect.shrink(2.0), 3.0, HOVER_WASH);
                    }
                    if let Some(color) = look.focus_color.filter(|_| response.has_focus()) {
                        let (width, radius) = FOCUS_LINE;
                        ui.painter().rect_stroke(response.rect, radius, egui::Stroke::new(width, color32(color)), egui::StrokeKind::Inside);
                    }
                    response
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "text_input", self.label.resolve(), serde_json::json!(self.buffer));
            (response.changed(), set_value)
        });

        if value_changed {
            self.update_value(context);
        }
        if let Some(new_value) = set_value.as_ref().and_then(|value| value.as_str()) {
            self.buffer.clear();
            match self.max_chars {
                Some(max_chars) => self.buffer.extend(new_value.chars().take(max_chars)),
                None => self.buffer.push_str(new_value),
            }
            self.update_value(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let label = if self.show_label { self.label.resolve() } else { "" };
        let mut size = internal::calc_labeled_widget_size(ui_sys, self.font_scale, label);
        if let Some(width) = self.width {
            let field = if width > 0.0 { width } else { internal::available_size(ui_sys).x };
            size.x += field - internal::item_width(ui_sys);
        }
        if let Some(look) = self.look {
            size.y = internal::calc_text_line_height(ui_sys, self.font_scale) + look.padding.y * 2.0;
        }
        size
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiTextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTextInput {
    // A single-line text field. It shows the text on_read_value gives; on_update_value gets the
    // user's edits.
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            show_label: true,
            buffer: String::new(),
            font_scale: UiFontScale::default(),
            width: None,
            max_chars: None,
            look: None,
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the field's left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    // Names it in widget paths without drawing it, for a field whose label is drawn elsewhere
    // (a heading above it).
    pub fn with_hidden_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self.show_label = false;
        self
    }

    // The field's width in points; 0 takes the room left in its row. The group's item width, or
    // the style's, without one.
    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width >= 0.0);
        self.width = Some(width);
        self
    }

    // The most characters it takes, typed or set (ui.set); longer text is cut.
    pub fn with_max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = Some(max_chars);
        self
    }

    // Draws the field on a panel instead of egui's frame.
    pub fn with_look(mut self, look: UiTextInputLook) -> Self {
        self.look = Some(look);
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // Reads the text to show, each frame: a borrow of the context or of anything that outlives
    // it, such as game state. The field copies it into its own edit buffer.
    pub fn on_read_value<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(&UiTextInput, &'a mut dyn UiWidgetContext) -> &'a str + 'static,
    {
        self.on_read_value = Some(smallbox!(f));
        self
    }

    // Called with the edited text.
    pub fn on_update_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiTextInput, &mut dyn UiWidgetContext, &str) + 'static,
    {
        self.on_update_value = Some(smallbox!(f));
        self
    }

    // Gives on_update_value the buffer's text.
    fn update_value(&self, context: &mut dyn UiWidgetContext) {
        if let Some(on_update_value) = &self.on_update_value {
            on_update_value(self, context, &self.buffer);
        }
    }
}

// ----------------------------------------------
// UiDropdown
// ----------------------------------------------

pub type UiDropdownSelectionChanged    = UiWidgetCallback<dyn Fn(&UiDropdown, &mut dyn UiWidgetContext)>;
pub type UiDropdownGetCurrentSelection = UiWidgetCallback<dyn Fn(&UiDropdown, &mut dyn UiWidgetContext) -> usize>;

pub struct UiDropdown {
    label: UiString,
    show_label: bool,   // False: the label names it in paths only (with_hidden_label).
    width: Option<f32>, // The field's; the group's item width if None, the row's with a row.
    font_scale: UiFontScale,
    current_item: usize,
    items: Vec<String>,
    look: Option<UiDropdownLook>,
    row: Option<UiControlRow>,

    // Opened: DropdownOpened; Hovered: ItemHovered, for an item of its list; Selected: ItemSelected;
    // Closed: DropdownClosed, when closed without a pick.
    sounds: UiWidgetSounds,
    list_open_frame: Option<u64>, // The last frame its list was open (ui::sound's frame).
    hovered_item: Option<usize>,  // In its open list.
    picked_while_open: bool,

    on_selection_changed: UiDropdownSelectionChanged,
    on_get_current_selection: UiDropdownGetCurrentSelection,
}

impl UiWidget for UiDropdown {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let row = self.row;
        let width = row.map_or_else(|| self.width.unwrap_or_else(|| internal::item_width(ui_sys)), |row| row.control_width);

        if let Some(on_get_current_selection) = &self.on_get_current_selection {
            self.current_item = on_get_current_selection(self, context);
        }

        let id = widget_id(self, self.label.id_source());
        let mut current_item = self.current_item;

        let (selection_changed, set_value, list) = ui_sys.with_ui(|ui| {
            let combo = ui
                .horizontal(|ui| {
                    if let Some(row) = row {
                        row.begin(ui);
                    }
                    if self.show_label {
                        left_label(ui_sys, ui, self.label.resolve());
                    }
                    if let Some(row) = row {
                        row.push_to_right(ui, width);
                    }

                    let font = internal::font(ui_sys, self.font_scale);
                    let selected_text = self.items.get(current_item).map_or("", String::as_str);
                    let mut selected_text = egui::RichText::new(selected_text).font(font.clone());

                    // With a look, its field goes under the combo box's button, whose own frame
                    // is hidden, and its arrow in place of egui's.
                    let mut combo_box = egui::ComboBox::from_id_salt(id).width(width);
                    let mut field = None;
                    if let Some(look) = self.look {
                        if let Some(color) = look.text_color {
                            selected_text = selected_text.color(color32(color));
                        }
                        if look.text_inset > 0.0 {
                            ui.spacing_mut().button_padding.x = look.text_inset;
                        }
                        combo_box = combo_box.icon(move |ui, rect, _visuals, _open| {
                            let arrow = egui::Rect::from_center_size(rect.center(), vec2(look.arrow_size.x, look.arrow_size.y));
                            paint_sprite(ui, look.arrow, arrow);
                        });
                        let widgets = &mut ui.visuals_mut().widgets;
                        for visuals in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
                            visuals.bg_fill = Color32::TRANSPARENT;
                            visuals.weak_bg_fill = Color32::TRANSPARENT;
                            visuals.bg_stroke = egui::Stroke::NONE;
                        }
                        field = Some((look, ui.painter().add(egui::Shape::Noop)));
                    }

                    let combo = combo_box
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            // Its items, recorded under its label while it's open.
                            let _path = immediate::path_scope(ui_sys, self.label.resolve());
                            let mut hovered_item = None;
                            for (index, item) in self.items.iter().enumerate() {
                                let response = ui.selectable_value(&mut current_item, index, egui::RichText::new(item).font(font.clone()));
                                if response.hovered() {
                                    hovered_item = Some(index);
                                }
                                immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(current_item == index)));
                            }
                            hovered_item
                        });

                    if let Some((look, slot)) = field {
                        let rect = combo.response.rect;
                        ui.painter().set(slot, nine_slice_shape(rect, &look.field, Color32::WHITE));
                        if combo.response.hovered() {
                            ui.painter().rect_filled(rect.shrink(2.0), 3.0, HOVER_WASH);
                        }
                    }
                    combo
                })
                .inner;

            // Some(hovered item) while its list is open.
            let list = combo.inner;
            let current = self.items.get(current_item).cloned().unwrap_or_default();
            let set_value = record_settable(ui_sys, ui, &combo.response, "dropdown", self.label.resolve(), serde_json::Value::String(current));
            (current_item != self.current_item, set_value, list)
        });

        self.play_list_sounds(context, list);

        if selection_changed {
            self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            self.picked_while_open |= list.is_some();
            self.select_item(context, current_item);
        }
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)) {
            if index != self.current_item {
                self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            }
            self.select_item(context, index);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let label = if self.show_label { self.label.resolve() } else { "" };
        let mut size = internal::calc_labeled_widget_size(ui_sys, self.font_scale, label);
        if let Some(width) = self.width {
            size.x += width - internal::item_width(ui_sys);
        }
        match self.row {
            Some(row) => row.measure(ui_sys, self.font_scale, label, size.y),
            None => size,
        }
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiDropdown {
    // A dropdown over `items`, shown with Display, with the first selected.
    pub fn new<I>(items: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display,
    {
        let items: Vec<String> = items.into_iter().map(|item| item.to_string()).collect();
        debug_assert!(!items.is_empty());

        Self {
            label: UiString::default(),
            show_label: true,
            width: None,
            font_scale: UiFontScale::default(),
            current_item: 0,
            items,
            look: None,
            row: None,
            sounds: UiWidgetSounds::new(),
            list_open_frame: None,
            hovered_item: None,
            picked_while_open: false,
            on_selection_changed: None,
            on_get_current_selection: None,
        }
    }

    // To the dropdown's left.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self.show_label = true;
        self
    }

    // A label that names it in widget paths without being drawn, for a dropdown whose caption is
    // drawn elsewhere (above it, as Custom Game's options are).
    pub fn with_hidden_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self.show_label = false;
        self
    }

    // The field's width in points, in place of the group's item width (a row sets its own).
    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // Its field and arrow sprites, in place of egui's combo box frame.
    pub fn with_look(mut self, look: UiDropdownLook) -> Self {
        self.look = Some(look);
        self
    }

    // Laid out as a settings row (UiControlRow): its label at the left, the field at the right edge.
    pub fn with_row(mut self, row: UiControlRow) -> Self {
        self.row = Some(row);
        self
    }

    pub fn with_current_item(mut self, index: usize) -> Self {
        debug_assert!(index < self.items.len());
        self.current_item = index;
        self
    }

    // Its sounds: DropdownOpened, ItemHovered, ItemSelected and DropdownClosed (events Opened,
    // Hovered, Selected and Closed), unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // Called when the user picks an item; current_selection gives it.
    pub fn on_selection_changed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiDropdown, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_selection_changed = Some(smallbox!(f));
        self
    }

    // Reads the selected item's index each frame, for a selection kept elsewhere.
    pub fn on_get_current_selection<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiDropdown, &mut dyn UiWidgetContext) -> usize + 'static,
    {
        self.on_get_current_selection = Some(smallbox!(f));
        self
    }

    // Its list's open and close sounds, and a hovered item's. `list` is Some(hovered item) while
    // the list is open. A list open last frame and closed now closed (a pick plays ItemSelected
    // instead); one seen open longer ago was left open when its menu last closed, so it's silent.
    fn play_list_sounds(&mut self, context: &mut dyn UiWidgetContext, list: Option<Option<usize>>) {
        let frame = sound::current_frame();
        let was_open = self.list_open_frame.is_some_and(|open_frame| frame.saturating_sub(open_frame) <= 1);

        match list {
            Some(hovered_item) => {
                if !was_open {
                    self.sounds.play(context, UiSoundEvent::Opened, Some(UiSoundKey::DropdownOpened));
                    self.picked_while_open = false;
                    self.hovered_item = None;
                }
                if hovered_item.is_some() && hovered_item != self.hovered_item {
                    self.sounds.play(context, UiSoundEvent::Hovered, Some(UiSoundKey::ItemHovered));
                }
                self.hovered_item = hovered_item;
                self.list_open_frame = Some(frame);
            }
            None => {
                if was_open && !self.picked_while_open {
                    self.sounds.play(context, UiSoundEvent::Closed, Some(UiSoundKey::DropdownClosed));
                }
                self.list_open_frame = None;
                self.hovered_item = None;
                self.picked_while_open = false;
            }
        }
    }

    fn select_item(&mut self, context: &mut dyn UiWidgetContext, index: usize) {
        self.current_item = index;
        if let Some(on_selection_changed) = &self.on_selection_changed {
            on_selection_changed(self, context);
        }
    }

    pub fn current_selection_index(&self) -> usize {
        self.current_item
    }

    pub fn current_selection(&self) -> &str {
        &self.items[self.current_item]
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn add_item(&mut self, item: String) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn reset_items(&mut self, current_item: usize, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
    }

    pub fn reset_items_with<V, ToString>(&mut self, values: &[V], current_item: usize, to_str: ToString)
    where
        ToString: Fn(&V) -> String,
    {
        let items: Vec<String> = values.iter().map(to_str).collect();

        self.reset_items(current_item, items);
    }
}
