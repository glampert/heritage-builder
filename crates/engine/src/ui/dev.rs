// ImGui widgets for the developer and debug menus that record themselves in the widget registry
// (registry.rs), as the retained widgets in widgets.rs do, so UI automation can list them
// (ui.tree), click them (ui.click) and set their values (ui.set). Each draws the plain ImGui
// widget. While the registry is on, it then records the item ImGui just drew and applies a value
// ui.set queued for it; while it's off, that costs a flag check.
//
// - Items scrolled or clipped out of their window's view are recorded as not visible. ui.set
//   still reaches them. ui.click first asks one to scroll into view (UiSystem::request_widget_reveal),
//   since a click at its center would land on whatever is drawn there instead.
// - Paths: begin_menu, tab_item, collapsing_header_scope and path_scope add a segment for the
//   widgets drawn inside them, as the retained menus and labeled groups do. Windows record
//   themselves with record_window.

use serde_json::Value;

use common::{Rect, Vec2};

use super::{UiSystem, UiWidgetInfo, widgets::list_index};

// ----------------------------------------------
// Recording
// ----------------------------------------------

// Records the last ImGui item. Returns a value ui.set queued for it, if it's enabled.
fn record_item(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    tooltip: Option<&str>,
    enabled: bool,
    value: Option<Value>,
) -> Option<Value> {
    if !ui_sys.is_widget_registry_enabled() {
        return None;
    }

    let ui = ui_sys.ui();
    let rect = Rect::from_extents(Vec2::from_array(ui.item_rect_min()), Vec2::from_array(ui.item_rect_max()));
    let center = rect.center().to_array();
    let visible = ui.is_rect_visible(center, center); // Where ui.click would click.

    let path = ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: tooltip.map(str::to_string),
        rect,
        enabled,
        hovered: ui.is_item_hovered(),
        visible,
        value,
    })?;

    // Scrolls the window to center the item; ImGui applies it next frame.
    if !visible && ui_sys.take_widget_reveal(&path) {
        ui.set_scroll_from_pos_y_with_ratio(center[1] - ui.window_pos()[1], 0.5);
    }

    ui_sys.take_widget_value(&path).filter(|_| enabled)
}

// A widget that isn't the last ImGui item, such as a window.
fn record_rect(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    rect: Rect,
    hovered: bool,
    value: Option<Value>,
) -> Option<String> {
    ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: None,
        rect,
        enabled: true,
        hovered,
        visible: true,
        value,
    })
}

// Adds a segment to the paths of the widgets recorded until it's dropped.
#[must_use]
pub struct UiWidgetPathScope<'a> {
    ui_sys: &'a UiSystem,
}

pub fn path_scope<'a>(ui_sys: &'a UiSystem, segment: &str) -> UiWidgetPathScope<'a> {
    ui_sys.push_widget_path(segment);
    UiWidgetPathScope { ui_sys }
}

impl Drop for UiWidgetPathScope<'_> {
    fn drop(&mut self) {
        self.ui_sys.pop_widget_path();
    }
}

// Records the current window, from inside its build closure, as a widget of `kind` ("window", or
// a kind ui.set takes), and its title bar's close button as "<label>/Close" if it has one.
// Returns a value ui.set queued for the window.
pub fn record_window(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    value: Option<Value>,
    has_close_button: bool,
) -> Option<Value> {
    if !ui_sys.is_widget_registry_enabled() {
        return None;
    }

    let ui = ui_sys.ui();
    let window_rect = Rect::from_pos_and_size(Vec2::from_array(ui.window_pos()), Vec2::from_array(ui.window_size()));
    let path = record_rect(ui_sys, kind, label, window_rect, ui.is_window_hovered(), value)?;

    if has_close_button {
        // Where ImGui 1.89 puts it: at the right end of the title bar (RenderWindowTitleBarContents),
        // a font-sized square plus frame padding (CloseButton).
        let padding = Vec2::from_array(ui_sys.current_ui_style().frame_padding);
        let font_size = ui.current_font_size();
        let position = Vec2::new(window_rect.max.x - font_size - padding.x * 2.0, window_rect.min.y);
        let size = Vec2::new(font_size + padding.x * 2.0, font_size + padding.y * 2.0);

        let rect = Rect::from_pos_and_size(position, size);
        let hovered = ui.is_window_hovered() && rect.contains_point(Vec2::from_array(ui.io().mouse_pos));

        let _path = path_scope(ui_sys, label);
        record_rect(ui_sys, "button", "Close", rect, hovered, None);
    }

    ui_sys.take_widget_value(&path)
}

// ----------------------------------------------
// Buttons
// ----------------------------------------------

pub fn button(ui_sys: &UiSystem, label: &str) -> bool {
    let clicked = ui_sys.ui().button(label);
    record_item(ui_sys, "button", label, None, true, None);
    clicked
}

pub fn small_button(ui_sys: &UiSystem, label: &str) -> bool {
    let clicked = ui_sys.ui().small_button(label);
    record_item(ui_sys, "button", label, None, true, None);
    clicked
}

// For buttons drawn some other way, e.g. with an icon font: records the last item as a button.
pub(super) fn record_button(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    tooltip: Option<&str>,
    value: Option<Value>,
) {
    record_item(ui_sys, kind, label, tooltip, true, value);
}

// A menu in a menu bar. The widgets drawn inside it get its label in their paths. A menu's items
// are drawn, and recorded, only while it's open: ui.click the menu first.
pub fn begin_menu<'ui>(ui_sys: &'ui UiSystem, label: &str) -> Option<UiDevMenuToken<'ui>> {
    let token = ui_sys.ui().begin_menu(label);

    // BeginMenu leaves the menu's own item as the last item, even once its popup has begun. The
    // popup clips to itself, so record_item would call the menu clipped.
    if ui_sys.is_widget_registry_enabled() {
        ui_sys.record_widget("menu", label, None, true, Some(Value::Bool(token.is_some())));
    }

    let token = token?;
    Some(UiDevMenuToken { _path: path_scope(ui_sys, label), _token: token })
}

// Ends the menu when dropped. Fields drop in order: the path segment, then the menu.
pub struct UiDevMenuToken<'ui> {
    _path: UiWidgetPathScope<'ui>,
    _token: imgui::MenuToken<'ui>,
}

// An item in a menu: click it to run it.
pub fn menu_item(ui_sys: &UiSystem, label: &str) -> bool {
    let clicked = ui_sys.ui().menu_item(label);
    record_item(ui_sys, "menu_item", label, None, true, None);
    clicked
}

// A tab in a tab bar: click it to select it. `value` is whether it's selected. The selected tab's
// widgets are drawn, and recorded, with its label in their paths.
pub fn tab_item<'ui>(ui_sys: &'ui UiSystem, label: &str) -> Option<UiDevTabToken<'ui>> {
    let token = ui_sys.ui().tab_item(label);
    record_item(ui_sys, "tab", label, None, true, Some(Value::Bool(token.is_some())));

    let token = token?;
    Some(UiDevTabToken { _path: path_scope(ui_sys, label), _token: token })
}

// Ends the tab when dropped. Fields drop in order: the path segment, then the tab.
pub struct UiDevTabToken<'ui> {
    _path: UiWidgetPathScope<'ui>,
    _token: imgui::TabItemToken<'ui>,
}

// A collapsing header: click it to open or close it. `value` is whether it's open.
pub fn collapsing_header(ui_sys: &UiSystem, label: &str, flags: imgui::TreeNodeFlags) -> bool {
    let open = ui_sys.ui().collapsing_header(label, flags);
    record_item(ui_sys, "header", label, None, true, Some(Value::Bool(open)));
    open
}

// A collapsing header that, while open, puts its label in the paths of the widgets drawn under it,
// until the returned scope is dropped. None while it's closed.
pub fn collapsing_header_scope<'a>(
    ui_sys: &'a UiSystem,
    label: &str,
    flags: imgui::TreeNodeFlags,
) -> Option<UiWidgetPathScope<'a>> {
    collapsing_header(ui_sys, label, flags).then(|| path_scope(ui_sys, label))
}

// ----------------------------------------------
// Values
// ----------------------------------------------

pub fn checkbox(ui_sys: &UiSystem, label: &str, value: &mut bool) -> bool {
    let mut changed = ui_sys.ui().checkbox(label, value);

    if let Some(Value::Bool(set_value)) = record_item(ui_sys, "checkbox", label, None, true, Some(Value::Bool(*value))) {
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

// Shows `value` as a checkbox that doesn't change it.
pub fn read_only_checkbox(ui_sys: &UiSystem, label: &str, value: bool) {
    let mut shown_value = value;
    ui_sys.ui().checkbox(label, &mut shown_value);
    record_item(ui_sys, "checkbox", label, None, false, Some(Value::Bool(value)));
}

// Numbers the sliders and inputs edit: ImGui scalar types that convert to and from JSON.
pub trait UiDevNumber: imgui::internal::DataTypeKind + PartialOrd {
    const INPUT_KIND: &'static str; // How an input of this type records itself: "int_input" or "float_input".

    fn to_json(self) -> Value;
    fn from_json(value: &Value) -> Option<Self>; // None if it's not a number or out of range.
}

macro_rules! impl_dev_int {
    ($($int:ty),*) => {$(
        impl UiDevNumber for $int {
            const INPUT_KIND: &'static str = "int_input";

            fn to_json(self) -> Value {
                Value::from(self)
            }

            fn from_json(value: &Value) -> Option<Self> {
                let from_i64 = value.as_i64().and_then(|value| Self::try_from(value).ok());
                from_i64.or_else(|| value.as_u64().and_then(|value| Self::try_from(value).ok()))
            }
        }
    )*};
}

macro_rules! impl_dev_float {
    ($($float:ty),*) => {$(
        impl UiDevNumber for $float {
            const INPUT_KIND: &'static str = "float_input";

            fn to_json(self) -> Value {
                serde_json::json!(self)
            }

            fn from_json(value: &Value) -> Option<Self> {
                value.as_f64().map(|value| value as Self)
            }
        }
    )*};
}

impl_dev_int!(i8, u8, i16, u16, i32, u32, i64, u64, usize);
impl_dev_float!(f32, f64);

// `display_format`: printf-style, e.g. "%.1f". None for ImGui's default.
pub fn slider<T: UiDevNumber>(
    ui_sys: &UiSystem,
    label: &str,
    min: T,
    max: T,
    display_format: Option<&str>,
    value: &mut T,
) -> bool {
    let slider = ui_sys.ui().slider_config(label, min, max);
    let mut changed = match display_format {
        Some(display_format) => slider.display_format(display_format).build(value),
        None => slider.build(value),
    };

    let set_value = record_item(ui_sys, "slider", label, None, true, Some(value.to_json()));
    if let Some(set_value) = set_value.as_ref().and_then(T::from_json) {
        let set_value = if set_value < min {
            min
        } else if set_value > max {
            max
        } else {
            set_value
        };
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

// Shows `value` as a slider that doesn't change it.
pub fn read_only_slider<T: UiDevNumber>(
    ui_sys: &UiSystem,
    label: &str,
    min: T,
    max: T,
    display_format: Option<&str>,
    value: T,
) {
    let mut shown_value = value;
    let slider = ui_sys.ui().slider_config(label, min, max);
    match display_format {
        Some(display_format) => slider.display_format(display_format).build(&mut shown_value),
        None => slider.build(&mut shown_value),
    };
    record_item(ui_sys, "slider", label, None, false, Some(value.to_json()));
}

// A number input of any ImGui scalar type, e.g. a usize index or a u32 count.
pub fn input_scalar<T: UiDevNumber>(ui_sys: &UiSystem, label: &str, value: &mut T, step: T) -> bool {
    input_number(ui_sys, label, value, step, None, false)
}

pub fn input_int(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut i32,
    step: i32,
    display_format: Option<&str>,
    read_only: bool,
) -> bool {
    input_number(ui_sys, label, value, step, display_format, read_only)
}

fn input_number<T: UiDevNumber>(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut T,
    step: T,
    display_format: Option<&str>,
    read_only: bool,
) -> bool {
    let input = ui_sys.ui().input_scalar(label, value).read_only(read_only).step(step);
    let mut changed = match display_format {
        Some(display_format) => input.display_format(display_format).build(),
        None => input.build(),
    };

    let set_value = record_item(ui_sys, T::INPUT_KIND, label, None, !read_only, Some(value.to_json()));
    if let Some(set_value) = set_value.as_ref().and_then(T::from_json) {
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

pub fn input_float(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut f32,
    step: f32,
    display_format: Option<&str>,
    read_only: bool,
) -> bool {
    let input = ui_sys.ui().input_float(label, value).read_only(read_only).step(step);
    let mut changed = match display_format {
        Some(display_format) => input.display_format(display_format).build(),
        None => input.build(),
    };

    if let Some(set_value) = record_item(ui_sys, "float_input", label, None, !read_only, Some(value.to_json())) {
        if let Some(set_value) = f32::from_json(&set_value) {
            changed |= set_value != *value;
            *value = set_value;
        }
    }

    changed
}

pub fn input_text(ui_sys: &UiSystem, label: &str, value: &mut String, read_only: bool) -> bool {
    let set_value = if read_only { UiDevTextValue::default() } else { UiDevTextValue::take(ui_sys, label, value) };
    let set = set_value.is_set();

    let changed =
        ui_sys.ui().input_text(label, value).read_only(read_only).callback(set_value.callbacks(), set_value).build();

    record_text_input(ui_sys, label, value, read_only);
    changed || set
}

// Records the last item as a text input. For one drawn with options input_text doesn't take, such
// as a hint or a callback, which applies ui.set's value with UiDevTextValue.
pub fn record_text_input(ui_sys: &UiSystem, label: &str, value: &str, read_only: bool) {
    if ui_sys.is_widget_registry_enabled() {
        record_item(ui_sys, "text_input", label, None, !read_only, Some(Value::String(value.to_string())));
    }
}

// Applies a text ui.set queued for a text input. It's taken before the input draws: while the input
// has keyboard focus, ImGui edits its own copy of the text and copies that back every frame, so a
// value set after drawing would be lost. take() puts it in the input's string, which ImGui shows
// unless the input has focus; then the input's ALWAYS callback (callbacks()) replaces ImGui's copy.
#[derive(Default)]
pub struct UiDevTextValue {
    text: Option<String>,
}

impl UiDevTextValue {
    // `label` is the one the input records with record_text_input.
    pub fn take(ui_sys: &UiSystem, label: &str, value: &mut String) -> Self {
        let text = match ui_sys.take_next_widget_value("text_input", label) {
            Some(Value::String(text)) => text,
            _ => return Self::default(),
        };
        value.clone_from(&text);
        Self { text: Some(text) }
    }

    pub fn is_set(&self) -> bool {
        self.text.is_some()
    }

    // The input's callbacks this value needs.
    pub fn callbacks(&self) -> imgui::InputTextCallback {
        if self.is_set() { imgui::InputTextCallback::ALWAYS } else { imgui::InputTextCallback::empty() }
    }
}

impl imgui::InputTextCallbackHandler for UiDevTextValue {
    fn on_always(&mut self, mut data: imgui::TextCallbackData) {
        if let Some(text) = self.text.take() {
            data.clear();
            data.push_str(&text);
        }
    }
}

// ui.set takes an item's text or index.
pub fn combo(ui_sys: &UiSystem, label: &str, current_item: &mut usize, items: &[impl AsRef<str>]) -> bool {
    let mut changed = ui_sys.ui().combo_simple_string(label, current_item, items);

    let current = items.get(*current_item).map_or(Value::Null, |item| Value::String(item.as_ref().to_string()));
    let set_value = record_item(ui_sys, "dropdown", label, None, true, Some(current));
    if let Some(index) = set_value.and_then(|value| list_index(&value, items)) {
        changed |= index != *current_item;
        *current_item = index;
    }

    changed
}
