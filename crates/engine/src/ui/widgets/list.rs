use std::{any::Any, fmt::Display};
use smallbox::smallbox;
use egui::{Align2, Color32, Sense, pos2, vec2};

use common::{Color, Vec2, bitflags_with_display};

use super::{UiWidget, UiWidgetCallback, UiWidgetContext, list_index, record_settable, widget_id};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiNineSlice, UiString, UiSystem,
        immediate::{self, color32, nine_slice_shape, uv_full},
        internal,
        renderer::egui_texture,
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
    },
};

// ----------------------------------------------
// UiListRow
// ----------------------------------------------

// A row of an item list drawn from a template (UiItemList::with_rows) rather than as plain text:
// an icon or emblem on the left, a title with a subtitle under it, and a title and subtitle on the
// right, e.g. a save ("Thăng Long", "mission 6 · autosave" | "12 March", "14:02") or a realm
// ("Kambara", "Start Campaign II to unlock", with a locked emblem). A disabled row can't be picked.
#[derive(Clone, Debug)]
pub struct UiListRow {
    pub title: String,
    pub subtitle: Option<String>,
    pub right_title: Option<String>,
    pub right_subtitle: Option<String>,
    pub icon: Option<TextureHandle>,
    pub enabled: bool,
}

impl UiListRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), subtitle: None, right_title: None, right_subtitle: None, icon: None, enabled: true }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    // On the right, right-aligned: a title and, under it, an optional subtitle.
    pub fn with_right(mut self, title: impl Into<String>, subtitle: Option<String>) -> Self {
        self.right_title = Some(title.into());
        self.right_subtitle = subtitle;
        self
    }

    pub fn with_icon(mut self, icon: TextureHandle) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

// How template rows look: a panel behind every row (e.g. a rule under it) and one behind the
// selected row (also, faded, behind a hovered one), text colours (the style's if None), font
// scales relative to the list's, the icon's size, and the room round the text. The right-hand
// texts are right-aligned at the row's end, or, with a `right_column`, left-aligned from that
// share of the row's text width (a second column, as the save list's time and date).
#[derive(Copy, Clone, Debug)]
pub struct UiListRowLook {
    pub panel: Option<UiNineSlice>,
    pub selected_panel: Option<UiNineSlice>,
    pub title_color: Option<Color>,
    pub selected_title_color: Option<Color>,
    pub subtitle_color: Option<Color>,
    pub right_title_color: Option<Color>, // The subtitle colour if None.
    pub right_column: Option<f32>,        // From 0 to 1.
    pub title_scale: f32,
    pub subtitle_scale: f32,
    pub icon_size: f32,
    pub padding: Vec2,
    pub spacing: f32, // Between rows.
}

impl Default for UiListRowLook {
    fn default() -> Self {
        Self {
            panel: None,
            selected_panel: None,
            title_color: None,
            selected_title_color: None,
            subtitle_color: None,
            right_title_color: None,
            right_column: None,
            title_scale: 1.0,
            subtitle_scale: 0.8,
            icon_size: 36.0,
            padding: Vec2::new(10.0, 6.0),
            spacing: 2.0,
        }
    }
}

impl UiListRowLook {
    // A row's height, with or without subtitles and icons in the list.
    pub fn row_height(&self, ui_sys: &UiSystem, font_scale: UiFontScale, subtitles: bool, icons: bool) -> f32 {
        let mut text = internal::calc_text_line_height(ui_sys, UiFontScale(font_scale.0 * self.title_scale));
        if subtitles {
            text += internal::calc_text_line_height(ui_sys, UiFontScale(font_scale.0 * self.subtitle_scale));
        }
        let content = if icons { text.max(self.icon_size) } else { text };
        content + self.padding.y * 2.0
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(&self, ui: &egui::Ui, ui_sys: &UiSystem, rect: egui::Rect, response: &egui::Response, row: &UiListRow, font_scale: UiFontScale, selected: bool) {
        let painter = ui.painter();
        let hovered = response.hovered() && row.enabled && !selected;
        let panel = if selected { self.selected_panel.or(self.panel) } else { self.panel };
        match panel {
            Some(panel) => {
                painter.add(nine_slice_shape(rect, &panel, Color32::WHITE));
            }
            None if selected => {
                painter.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            }
            None => {}
        }
        if hovered {
            match self.selected_panel {
                Some(panel) => {
                    painter.add(nine_slice_shape(rect, &panel, Color32::from_white_alpha(110)));
                }
                None => {
                    painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
            }
        }

        let fade = |color: Color32| if row.enabled { color } else { color.gamma_multiply(0.55) };
        let title_color = if selected { self.selected_title_color.or(self.title_color) } else { self.title_color };
        let title_color = fade(title_color.map_or_else(|| ui.visuals().text_color(), color32));
        let subtitle_color = fade(self.subtitle_color.map_or_else(|| ui.visuals().weak_text_color(), color32));
        let title_font = internal::font(ui_sys, UiFontScale(font_scale.0 * self.title_scale));
        let subtitle_font = internal::font(ui_sys, UiFontScale(font_scale.0 * self.subtitle_scale));
        let title_height = internal::calc_text_line_height(ui_sys, UiFontScale(font_scale.0 * self.title_scale));
        let subtitle_height = internal::calc_text_line_height(ui_sys, UiFontScale(font_scale.0 * self.subtitle_scale));

        let inner = rect.shrink2(vec2(self.padding.x, self.padding.y));
        let mut left = inner.left();
        if let Some(icon) = row.icon {
            let icon_rect = egui::Rect::from_min_size(pos2(left, inner.center().y - self.icon_size * 0.5), vec2(self.icon_size, self.icon_size));
            painter.image(egui_texture(icon), icon_rect, uv_full(), if row.enabled { Color32::WHITE } else { Color32::from_white_alpha(170) });
            left += self.icon_size + self.padding.x;
        }

        // A title alone is centered on the row; with a subtitle, the two are.
        let text_top = |subtitle: bool| {
            let height = if subtitle { title_height + subtitle_height } else { title_height };
            inner.center().y - height * 0.5
        };

        // With a second column, the first's texts are cut short of it.
        let column_x = self.right_column.map(|column| left + (inner.right() - left) * column.clamp(0.0, 1.0));
        let left_clip = match column_x {
            Some(x) => rect.with_max_x((x - self.padding.x).max(rect.left())),
            None => rect,
        };
        let clipped = painter.with_clip_rect(left_clip.intersect(ui.clip_rect()));
        let top = text_top(row.subtitle.is_some());
        clipped.text(pos2(left, top), Align2::LEFT_TOP, &row.title, title_font.clone(), title_color);
        if let Some(subtitle) = &row.subtitle {
            clipped.text(pos2(left, top + title_height), Align2::LEFT_TOP, subtitle, subtitle_font.clone(), subtitle_color);
        }

        if let Some(right_title) = &row.right_title {
            let top = text_top(row.right_subtitle.is_some());
            let right_title_color = fade(self.right_title_color.map_or(subtitle_color, color32));
            let (x, align) = match column_x {
                Some(x) => (x, Align2::LEFT_TOP),
                None => (inner.right(), Align2::RIGHT_TOP),
            };
            let clipped = painter.with_clip_rect(rect.intersect(ui.clip_rect()));
            clipped.text(pos2(x, top), align, right_title, title_font, right_title_color);
            if let Some(right_subtitle) = &row.right_subtitle {
                clipped.text(pos2(x, top + title_height), align, right_subtitle, subtitle_font, subtitle_color);
            }
        }
    }
}

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
    show_label: bool, // False: the label names it in paths only (with_hidden_label).
    font_scale: UiFontScale,
    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,
    flags: UiItemListFlags,
    current_item: Option<usize>,
    items: Vec<String>,
    rows: Option<Vec<UiListRow>>, // Drawn from the row template if set; items are their titles.
    row_look: UiListRowLook,
    text_input_field_buffer: Option<String>,
    sounds: UiWidgetSounds,      // Hovered: ItemHovered, for a row; Selected: ItemSelected. Typing plays nothing.
    hovered_item: Option<usize>, // Last frame.
    reveal_current: bool,        // Scroll the current item into view when it's next drawn.
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
        let mut hovered_item = None;

        let list_response = ui_sys.with_ui(|ui| {
            let left = ui.cursor().left() + margin_left;

            // Optional label:
            if self.show_label && !self.label.is_empty() {
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
            if self.rows.is_some() {
                // Template rows past the end of a list that doesn't scroll are cut at its end.
                list_ui.shrink_clip_rect(rect);
            }
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

                    // Template rows: drawn by the row look, each as wide as the list.
                    if let Some(rows) = &self.rows {
                        let look = &self.row_look;
                        ui.style_mut().spacing.item_spacing.y = look.spacing;
                        let subtitles = rows.iter().any(|row| row.subtitle.is_some() || row.right_subtitle.is_some());
                        let icons = rows.iter().any(|row| row.icon.is_some());
                        let height = look.row_height(ui_sys, self.font_scale, subtitles, icons);
                        for (index, row) in rows.iter().enumerate() {
                            let is_selected = current_item == Some(index);
                            let sense = if row.enabled { Sense::click() } else { Sense::hover() };
                            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
                            if ui.is_rect_visible(rect) {
                                look.paint(ui, ui_sys, rect, &response, row, self.font_scale, is_selected);
                            }
                            if is_selected && self.reveal_current {
                                response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                            }
                            if response.clicked() && row.enabled {
                                current_item = Some(index);
                            }
                            if response.hovered() && row.enabled {
                                hovered_item = Some(index);
                            }
                            // Its texts with its state, so automation can read a row. Recorded as far as
                            // it shows: a row the list's end cuts is the part above it.
                            let mut shown = response.clone();
                            let shown_rect = response.rect.intersect(ui.clip_rect());
                            if shown_rect.is_positive() {
                                shown.rect = shown_rect;
                            }
                            immediate::record_lazy(ui_sys, ui, &shown, "list_item", &row.title, None, row.enabled, || {
                                Some(serde_json::json!({
                                    "selected": is_selected,
                                    "subtitle": row.subtitle,
                                    "right": row.right_title,
                                    "right_subtitle": row.right_subtitle,
                                }))
                            });
                        }
                        return;
                    }

                    // Rows span the list, with their text on the left.
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        for (index, item) in self.items.iter().enumerate() {
                            let is_selected = current_item == Some(index);
                            let text = egui::RichText::new(item).font(font.clone());
                            let response = ui.add(egui::Button::selectable(is_selected, text));
                            if is_selected && self.reveal_current {
                                response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                            }
                            if response.clicked() {
                                current_item = Some(index);
                            }
                            if response.hovered() {
                                hovered_item = Some(index);
                            }
                            immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(is_selected)));
                        }
                    });
                });
            });

            list_response
        });

        // Shown once: the player scrolls it away as they like. A new list (a rebuilt dialog) or
        // set_current_item shows it again.
        self.reveal_current = false;

        if hovered_item.is_some() && hovered_item != self.hovered_item {
            self.sounds.play(context, UiSoundEvent::Hovered, Some(UiSoundKey::ItemHovered));
        }
        self.hovered_item = hovered_item;

        if text_input_field_changed && self.text_input_field_buffer.is_some() {
            current_item = None;
            self.current_item = None;
            self.notify_selection_changed(context);
        }

        if current_item != self.current_item
            && let Some(selected_index) = current_item
        {
            self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            self.current_item = current_item;
            self.select_item(context, selected_index);
        }

        // ui.set on the list selects an item, by text or index.
        let current = self.current_item.and_then(|index| self.items.get(index)).cloned();
        let set_value = ui_sys.with_ui(|ui| {
            record_settable(ui_sys, ui, &list_response, "item_list", self.label.resolve(), current.map_or(serde_json::Value::Null, serde_json::Value::String))
        });
        let row_enabled = |index: usize| self.rows.as_ref().is_none_or(|rows| rows.get(index).is_some_and(|row| row.enabled));
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)).filter(|index| row_enabled(*index)) {
            if self.current_item != Some(index) {
                self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            }
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

        // The label's line above the list.
        let label_height = if !self.show_label || self.label.is_empty() {
            0.0
        } else {
            internal::calc_text_line_height(ui_sys, self.font_scale) + internal::item_spacing(ui_sys).y
        };

        Vec2::new(size.x, size.y + input_field_height + label_height)
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
            show_label: true,
            font_scale: UiFontScale::default(),
            size: None,
            margin_left: 0.0,
            margin_right: 0.0,
            flags: UiItemListFlags::empty(),
            current_item: None,
            items: Vec::new(),
            rows: None,
            row_look: UiListRowLook::default(),
            text_input_field_buffer: None,
            sounds: UiWidgetSounds::new(),
            hovered_item: None,
            reveal_current: true,
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
        self.rows = None; // Plain items replace any template rows, as reset_items does.
        self.sync_text_input_field();
        self
    }

    // Rows drawn from the row template (see UiListRow), in place of plain items: their titles are
    // the items, so selections, ui.set and paths go by title.
    pub fn with_rows(mut self, rows: Vec<UiListRow>) -> Self {
        self.reset_rows(self.current_item, rows);
        self.sync_text_input_field();
        self
    }

    // How template rows look.
    pub fn with_row_look(mut self, look: UiListRowLook) -> Self {
        self.row_look = look;
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
        self.show_label = true;
        self
    }

    // A label that names it and its rows in widget paths without being drawn above it, for a list
    // whose heading is drawn elsewhere (Custom Game's realms and maps).
    pub fn with_hidden_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self.show_label = false;
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

    // Its sounds: ItemHovered for a row hovered and ItemSelected for one picked (events Hovered
    // and Selected), unless overridden.
    pub fn with_sounds(mut self, sounds: UiWidgetSounds) -> Self {
        self.sounds = sounds;
        self
    }

    // `event` plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
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

    // Selects the item at `index` (None: none) without telling on_selection_changed, e.g. to
    // follow a selection made elsewhere. The text input field, if any, shows it.
    pub fn set_current_item(&mut self, index: Option<usize>) {
        self.current_item = index.filter(|index| *index < self.items.len());
        self.reveal_current = true;
        self.sync_text_input_field();
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
        if let Some(rows) = &mut self.rows {
            rows.push(UiListRow::new(item.clone()));
        }
        self.items.push(item);
        self
    }

    // Plain items, in place of template rows if it had them.
    pub fn reset_items(&mut self, current_item: Option<usize>, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
        self.rows = None;
    }

    // Template rows (see with_rows).
    pub fn reset_rows(&mut self, current_item: Option<usize>, rows: Vec<UiListRow>) {
        self.current_item = current_item;
        self.items = rows.iter().map(|row| row.title.clone()).collect();
        self.rows = Some(rows);
    }

    pub fn rows(&self) -> Option<&[UiListRow]> {
        self.rows.as_deref()
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
