use std::{any::Any, fmt::Display, ops::RangeInclusive};
use smallbox::smallbox;
use egui::vec2;

use common::Vec2;

use super::{UiWidget, UiWidgetCallback, UiWidgetContext, left_label, list_index, record_settable, widget_id};
use crate::ui::{UiFontScale, immediate, internal};

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
    label: String,
    font_scale: UiFontScale,
    min: f64,
    max: f64,
    integral: bool, // Shown and set in whole numbers.
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
            self.update_value(context, new_value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_f64()) {
            let new_value = if self.integral { new_value.round() } else { new_value };
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
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
            label: String::new(),
            font_scale: UiFontScale::default(),
            min,
            max,
            integral: T::INTEGRAL,
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the slider's left.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
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
        let width = internal::item_width(ui_sys);

        ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    // The value shows to the slider's right.
                    ui.spacing_mut().slider_width = (width - ui.spacing().interact_size.x - ui.spacing().item_spacing.x).max(40.0);
                    let slider = egui::Slider::new(&mut value, self.min..=self.max).clamping(egui::SliderClamping::Always);
                    let slider = if self.integral { slider.integer() } else { slider.fixed_decimals(2) };
                    ui.add(slider)
                })
                .inner;

            let changed = response.changed().then_some(value);
            let json_value = if self.integral { serde_json::json!(value as i64) } else { serde_json::json!(value) };
            let set_value = record_settable(ui_sys, ui, &response, "slider", &self.label, json_value);
            (changed, set_value)
        })
    }
}

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub type UiCheckboxReadValue   = UiWidgetCallback<dyn Fn(&UiCheckbox, &mut dyn UiWidgetContext) -> bool>;
pub type UiCheckboxUpdateValue = UiWidgetCallback<dyn Fn(&UiCheckbox, &mut dyn UiWidgetContext, bool)>;

pub struct UiCheckbox {
    label: String,
    font_scale: UiFontScale,
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
                    left_label(ui_sys, ui, &self.label);
                    ui.checkbox(&mut value, "")
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "checkbox", &self.label, serde_json::Value::Bool(value));
            (response.changed(), set_value)
        });

        if value_changed {
            self.update_value(context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_bool()) {
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let spacing = &ui_sys.egui_style().spacing;

        let checkbox_square = internal::calc_frame_height(ui_sys, self.font_scale);
        let mut width = checkbox_square;

        if !self.label.is_empty() {
            let label_size = context.calc_text_size(self.font_scale, &self.label);
            width += spacing.icon_spacing + label_size.x;
        }

        Vec2::new(width, checkbox_square)
    }

    fn label(&self) -> &str {
        &self.label
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
            label: String::new(),
            font_scale: UiFontScale::default(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the checkbox's left.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
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
    label: String,
    font_scale: UiFontScale,
    min: i32,
    max: i32,
    step: i32,
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

        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let (response, stepped) = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    // A field to drag or type into, and buttons that step it.
                    let buttons_width = (ui.spacing().interact_size.y + ui.spacing().item_spacing.x) * 2.0;
                    let drag = egui::DragValue::new(&mut value).range(self.min..=self.max).speed(f64::from(self.step) * 0.1);
                    let response = ui.add_sized(vec2((width - buttons_width).max(40.0), ui.spacing().interact_size.y), drag);

                    let mut stepped = false;
                    if ui.small_button("-").clicked() {
                        value = value.saturating_sub(self.step);
                        stepped = true;
                    }
                    if ui.small_button("+").clicked() {
                        value = value.saturating_add(self.step);
                        stepped = true;
                    }
                    (response, stepped)
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "int_input", &self.label, serde_json::json!(value));
            (response.changed() || stepped, set_value)
        });

        if value_changed {
            self.update_value(context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_i64()) {
            let new_value = new_value.clamp(i64::from(self.min), i64::from(self.max)) as i32;
            self.update_value(context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
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
            label: String::new(),
            font_scale: UiFontScale::default(),
            min: i32::MIN,
            max: i32::MAX,
            step: 1,
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the field's left.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
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
    label: String,
    buffer: String,
    font_scale: UiFontScale,
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
        let width = internal::item_width(ui_sys);

        self.buffer.clear();
        if let Some(on_read_value) = &self.on_read_value {
            let text = on_read_value(self, context);
            self.buffer.push_str(text);
        }

        let id = widget_id(self, &self.label);
        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);
                    let text_edit = egui::TextEdit::singleline(&mut self.buffer)
                        .id(id)
                        .font(internal::font(ui_sys, self.font_scale))
                        .desired_width(width);
                    ui.add(text_edit)
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "text_input", &self.label, serde_json::json!(self.buffer));
            (response.changed(), set_value)
        });

        if value_changed {
            self.update_value(context);
        }
        if let Some(new_value) = set_value.as_ref().and_then(|value| value.as_str()) {
            self.buffer.clear();
            self.buffer.push_str(new_value);
            self.update_value(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
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
            label: String::new(),
            buffer: String::new(),
            font_scale: UiFontScale::default(),
            on_read_value: None,
            on_update_value: None,
        }
    }

    // To the field's left.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
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
    label: String,
    font_scale: UiFontScale,
    current_item: usize,
    items: Vec<String>,
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
        let width = internal::item_width(ui_sys);

        if let Some(on_get_current_selection) = &self.on_get_current_selection {
            self.current_item = on_get_current_selection(self, context);
        }

        let id = widget_id(self, &self.label);
        let mut current_item = self.current_item;

        let (selection_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    let font = internal::font(ui_sys, self.font_scale);
                    let selected_text = self.items.get(current_item).map_or("", String::as_str);

                    egui::ComboBox::from_id_salt(id)
                        .width(width)
                        .selected_text(egui::RichText::new(selected_text).font(font.clone()))
                        .show_ui(ui, |ui| {
                            // Its items, recorded under its label while it's open.
                            let _path = immediate::path_scope(ui_sys, &self.label);
                            for (index, item) in self.items.iter().enumerate() {
                                let response = ui.selectable_value(&mut current_item, index, egui::RichText::new(item).font(font.clone()));
                                immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(current_item == index)));
                            }
                        })
                        .response
                })
                .inner;

            let current = self.items.get(current_item).cloned().unwrap_or_default();
            let set_value = record_settable(ui_sys, ui, &response, "dropdown", &self.label, serde_json::Value::String(current));
            (current_item != self.current_item, set_value)
        });

        if selection_changed {
            self.select_item(context, current_item);
        }
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)) {
            self.select_item(context, index);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
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
            label: String::new(),
            font_scale: UiFontScale::default(),
            current_item: 0,
            items,
            on_selection_changed: None,
            on_get_current_selection: None,
        }
    }

    // To the dropdown's left.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    pub fn with_current_item(mut self, index: usize) -> Self {
        debug_assert!(index < self.items.len());
        self.current_item = index;
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
