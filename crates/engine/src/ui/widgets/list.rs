use std::{any::Any, fmt::Display};
use smallbox::smallbox;
use egui::{Sense, pos2, vec2};

use common::{Vec2, bitflags_with_display};

use super::{UiWidget, UiWidgetCallback, UiWidgetContext, list_index, record_settable, widget_id};
use crate::ui::{UiFontScale, UiString, immediate, internal};

// ----------------------------------------------
// UiItemListFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiItemListFlags: u8 {
        const Border         = 1 << 0;
        const Scrollable     = 1 << 1;
        const Scrollbars     = 1 << 2;
        const TextInputField = 1 << 3;
    }
}

// ----------------------------------------------
// UiItemList
// ----------------------------------------------

pub type UiItemListSelectionChanged = UiWidgetCallback<dyn Fn(&UiItemList, &mut dyn UiWidgetContext)>;

pub struct UiItemList {
    label: UiString,
    font_scale: UiFontScale,
    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,
    flags: UiItemListFlags,
    current_item: Option<usize>,
    items: Vec<String>,
    text_input_field_buffer: Option<String>,
    on_selection_changed: UiItemListSelectionChanged,
}

impl UiWidget for UiItemList {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();

        // The list's size:
        //  > 0.0 -> fixed size
        //  = 0.0 -> use remaining host window size
        //  < 0.0 -> use remaining host window size minus abs(size)
        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);
            requested_size.x -= self.margin_right - padding;
        }

        let id = widget_id(self, self.label.id_source());
        let font = internal::font(ui_sys, self.font_scale);
        let margin_left = self.margin_left;
        let border = self.flags.intersects(UiItemListFlags::Border);
        let scrollable = self.flags.intersects(UiItemListFlags::Scrollable);
        let scroll_bars = self.flags.intersects(UiItemListFlags::Scrollbars);

        let mut current_item = self.current_item;
        let mut text_input_field_changed = false;

        let list_response = ui_sys.with_ui(|ui| {
            let left = ui.cursor().left() + margin_left;

            // Optional label:
            if !self.label.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(margin_left);
                    ui.label(egui::RichText::new(self.label.resolve()).font(font.clone()));
                });
            }

            let available = immediate::fill_size(ui);
            let resolve = |requested: f32, available: f32| {
                if requested > 0.0 { requested } else { (available + requested).max(0.0) }
            };
            let width = resolve(requested_size.x, available.x - margin_left);

            // Optional text input field:
            if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
                let response = ui
                    .horizontal(|ui| {
                        ui.add_space(margin_left);
                        ui.add(egui::TextEdit::singleline(text_input_field_buffer).id(id.with("input_field")).font(font.clone()).desired_width(width))
                    })
                    .inner;
                text_input_field_changed = response.changed();
            }

            let height = resolve(requested_size.y, immediate::fill_size(ui).y);
            let rect = egui::Rect::from_min_size(pos2(left, ui.cursor().top()), vec2(width, height));
            let list_response = ui.allocate_rect(rect, Sense::hover());

            let frame = if border {
                egui::Frame::group(ui.style()).inner_margin(ui.spacing().item_spacing.x * 0.5)
            } else {
                egui::Frame::NONE
            };

            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            frame.show(&mut list_ui, |ui| {
                ui.set_min_size((rect.size() - frame.total_margin().sum()).max(egui::Vec2::ZERO));

                let scroll_area = egui::ScrollArea::vertical()
                    .id_salt(id)
                    .auto_shrink([false, false])
                    .vscroll(scrollable)
                    .scroll_bar_visibility(if scroll_bars {
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                    } else {
                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
                    });

                scroll_area.show(ui, |ui| {
                    let _path = immediate::path_scope(ui_sys, self.label.resolve());
                    ui.style_mut().spacing.item_spacing.y = 0.0;
                    // Rows span the list, with their text on the left.
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        for (index, item) in self.items.iter().enumerate() {
                            let is_selected = current_item == Some(index);
                            let text = egui::RichText::new(item).font(font.clone());
                            let response = ui.add(egui::Button::selectable(is_selected, text));
                            if response.clicked() {
                                current_item = Some(index);
                            }
                            immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(is_selected)));
                        }
                    });
                });
            });

            list_response
        });

        if text_input_field_changed && self.text_input_field_buffer.is_some() {
            current_item = None;
            self.current_item = None;
            self.notify_selection_changed(context);
        }

        if current_item != self.current_item
            && let Some(selected_index) = current_item
        {
            self.current_item = current_item;
            self.select_item(context, selected_index);
        }

        // ui.set on the list selects an item, by text or index.
        let current = self.current_item.and_then(|index| self.items.get(index)).cloned();
        let set_value = ui_sys.with_ui(|ui| {
            record_settable(ui_sys, ui, &list_response, "item_list", self.label.resolve(), current.map_or(serde_json::Value::Null, serde_json::Value::String))
        });
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)) {
            self.current_item = Some(index);
            self.select_item(context, index);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - padding;
        }

        let size = internal::calc_child_window_size(ui_sys, requested_size);

        let input_field_height =
            if self.text_input_field_buffer.is_some() { internal::calc_frame_height(ui_sys, self.font_scale) } else { 0.0 };

        Vec2::new(size.x, size.y + input_field_height)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiItemList {
    fn default() -> Self {
        Self::new()
    }
}

impl UiItemList {
    fn select_item(&mut self, context: &mut dyn UiWidgetContext, index: usize) {
        if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
            text_input_field_buffer.clear();
            text_input_field_buffer.push_str(&self.items[index]);
        }

        self.notify_selection_changed(context);
    }

    fn notify_selection_changed(&self, context: &mut dyn UiWidgetContext) {
        if let Some(on_selection_changed) = &self.on_selection_changed {
            on_selection_changed(self, context);
        }
    }

    // The text input field shows the selected item.
    fn sync_text_input_field(&mut self) {
        if let Some(buffer) = &mut self.text_input_field_buffer
            && let Some(item) = self.current_item.and_then(|index| self.items.get(index))
        {
            buffer.clear();
            buffer.push_str(item);
        }
    }

    // An empty list with nothing selected, as wide as the menu's content and as tall as the room
    // left in it.
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            font_scale: UiFontScale::default(),
            size: None,
            margin_left: 0.0,
            margin_right: 0.0,
            flags: UiItemListFlags::empty(),
            current_item: None,
            items: Vec::new(),
            text_input_field_buffer: None,
            on_selection_changed: None,
        }
    }

    // Its items, shown with Display.
    pub fn with_items<I>(mut self, items: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display,
    {
        self.items = items.into_iter().map(|item| item.to_string()).collect();
        self.sync_text_input_field();
        self
    }

    pub fn with_current_item(mut self, index: usize) -> Self {
        self.current_item = Some(index);
        self.sync_text_input_field();
        self
    }

    // Above the list.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // Per axis: a positive size is fixed, zero takes the room left in the menu, and a negative one
    // takes that room less the value.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_margins(mut self, left: f32, right: f32) -> Self {
        debug_assert!(left >= 0.0 && right >= 0.0);
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn with_flags(mut self, flags: UiItemListFlags) -> Self {
        self.flags = flags;
        self.text_input_field_buffer = flags.intersects(UiItemListFlags::TextInputField).then(String::new);
        self.sync_text_input_field();
        self
    }

    // Called when the user picks an item or types in the input field; current_selection and
    // current_text_input_field give them.
    pub fn on_selection_changed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiItemList, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_selection_changed = Some(smallbox!(f));
        self
    }

    pub fn current_text_input_field(&self) -> Option<&str> {
        self.text_input_field_buffer.as_deref()
    }

    pub fn current_selection_index(&self) -> Option<usize> {
        self.current_item
    }

    pub fn current_selection(&self) -> Option<&str> {
        self.current_item.map(|index| self.items[index].as_str())
    }

    pub fn clear_selection(&mut self) {
        self.current_item = None;

        if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
            text_input_field_buffer.clear();
        }
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn add_item(&mut self, item: String) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn reset_items(&mut self, current_item: Option<usize>, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
    }

    pub fn reset_text_input_field(&mut self, value: String) {
        self.current_item = None;
        let buffer = self.text_input_field_buffer.as_mut().unwrap();
        *buffer = value;
    }

    pub fn reset_items_with<V, ToString>(&mut self, values: &[V], current_item: Option<usize>, to_str: ToString)
    where
        ToString: Fn(&V) -> String,
    {
        let items: Vec<String> = values.iter().map(to_str).collect();

        self.reset_items(current_item, items);
    }
}
