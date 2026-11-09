use std::any::Any;
use smallbox::smallbox;
use egui::{Align2, Sense, StrokeKind, pos2, vec2};

use common::Vec2;

use super::{
    UiSpriteButtonState, UiTextButtonFrame, UiWidget, UiWidgetCallback, UiWidgetContext, UiWidgetImpl, list_index, widget_id,
};
use crate::ui::{
    UiFontScale, UiString, UiSystem,
    immediate::{self, color32, nine_slice_shape},
    internal,
    sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
};

// ----------------------------------------------
// UiTabLook
// ----------------------------------------------

// How a tab bar's tabs look: framed (a panel per state for the other tabs and for the selected
// one, a skin's `tab` and `tab_selected` buttons) or painted in the style's colours; their font,
// and their width: each its text's plus padding, but at least `min_tab_width`, so a longer label
// (a localized one) widens its tab instead of being clipped.
#[derive(Copy, Clone, Debug)]
pub struct UiTabLook {
    pub tab: Option<UiTextButtonFrame>,
    pub selected: Option<UiTextButtonFrame>,
    pub font_scale: UiFontScale,
    pub min_tab_width: Option<f32>,
    pub spacing: f32,      // Between tabs.
    pub margin_below: f32, // Between the tabs and the selected tab's contents.
}

impl Default for UiTabLook {
    fn default() -> Self {
        Self { tab: None, selected: None, font_scale: UiFontScale::default(), min_tab_width: None, spacing: 6.0, margin_below: 10.0 }
    }
}

// ----------------------------------------------
// UiTabBar
// ----------------------------------------------

pub type UiTabBarTabChanged = UiWidgetCallback<dyn Fn(&UiTabBar, &mut dyn UiWidgetContext, usize)>;

// A row of tabs, centered across the menu, and under it the selected tab's contents, if it has
// any (a widget, often a group). Click a tab to select it. The selected tab's label is in the
// paths of its contents' widgets (`<menu>/<tab>/<widget>`), as ui::tab_bar does for the developer
// UI. Each tab records itself (kind "tab", value: whether it's selected), and the bar too (kind
// "tab_bar", value: the selected tab's label), which ui.set takes a tab's label or index on.
// Plays TabSelected when a tab is picked.
pub struct UiTabBar {
    label: UiString,
    tabs: Vec<(UiString, Option<UiWidgetImpl>)>,
    selected: usize,
    look: UiTabLook,
    sounds: UiWidgetSounds, // Hovered: nothing, Selected: TabSelected.
    hovered: Option<usize>, // Last frame.
    on_tab_changed: UiTabBarTabChanged,
}

impl UiWidget for UiTabBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let sizes = self.tab_sizes(ui_sys);
        let (row_width, row_height) = self.row_size(&sizes);
        let id = widget_id(self, self.label.id_source());
        let font = internal::font(ui_sys, self.look.font_scale);
        let labels: Vec<&str> = self.tabs.iter().map(|(label, _)| label.resolve()).collect();

        let (clicked, hovered, set_value) = ui_sys.with_ui(|ui| {
            let width = row_width.max(immediate::fill_size(ui).x);
            let (rect, response) = ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
            let selected_label = labels.get(self.selected).copied().unwrap_or_default();
            let set_value = immediate::record(
                ui_sys,
                ui,
                &response,
                "tab_bar",
                self.label.resolve(),
                None,
                true,
                Some(serde_json::Value::String(selected_label.to_string())),
            );

            let mut clicked = None;
            let mut hovered = None;
            let mut x = rect.left() + (width - row_width) * 0.5;
            for (index, (label, size)) in labels.iter().zip(&sizes).enumerate() {
                let tab_rect = egui::Rect::from_min_size(pos2(x, rect.top()), vec2(size.x, row_height));
                x += size.x + self.look.spacing;

                let response = ui.interact(tab_rect, id.with(index), Sense::click());
                let is_selected = index == self.selected;
                if ui.is_rect_visible(tab_rect) {
                    self.paint_tab(ui, tab_rect, &response, label, &font, is_selected);
                }
                immediate::record(ui_sys, ui, &response, "tab", label, None, true, Some(serde_json::Value::Bool(is_selected)));
                if response.clicked() {
                    clicked = Some(index);
                }
                if response.hovered() {
                    hovered = Some(index);
                }
            }

            ui.add_space(self.look.margin_below);
            (clicked, hovered, set_value)
        });

        if hovered.is_some() && hovered != self.hovered {
            self.sounds.play(context, UiSoundEvent::Hovered, None);
        }
        self.hovered = hovered;

        let picked = clicked.or_else(|| set_value.and_then(|value| list_index(&value, &labels)));
        if let Some(index) = picked {
            self.select(context, index);
        }

        // The selected tab's contents, under its label.
        let selected = self.selected;
        if let Some((label, Some(contents))) = self.tabs.get_mut(selected) {
            let _path = immediate::path_scope(ui_sys, label.resolve());
            contents.draw(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let (row_width, row_height) = self.row_size(&self.tab_sizes(context.ui_sys()));
        let contents = match self.tabs.get(self.selected) {
            Some((_, Some(contents))) => contents.measure(context),
            _ => Vec2::zero(),
        };
        Vec2::new(row_width.max(contents.x), row_height + self.look.margin_below + contents.y)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl Default for UiTabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTabBar {
    // No tabs yet; the first one added is selected.
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            tabs: Vec::new(),
            selected: 0,
            look: UiTabLook::default(),
            sounds: UiWidgetSounds::new(),
            hovered: None,
            on_tab_changed: None,
        }
    }

    // Names the bar in widget paths ("<menu>/tab_bar" without one). Its tabs and their contents
    // aren't under it.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    // A tab with nothing under it: on_tab_changed shows what it stands for.
    pub fn with_tab(mut self, label: impl Into<UiString>) -> Self {
        self.tabs.push((label.into(), None));
        self
    }

    // A tab whose contents draw under the bar while it's selected.
    pub fn with_tab_contents(mut self, label: impl Into<UiString>, contents: impl Into<UiWidgetImpl>) -> Self {
        self.tabs.push((label.into(), Some(contents.into())));
        self
    }

    pub fn with_look(mut self, look: UiTabLook) -> Self {
        self.look = look;
        self
    }

    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    // `event` (Hovered or Selected) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // Called with the tab selected before, when the user (or ui.set) picks another.
    pub fn on_tab_changed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiTabBar, &mut dyn UiWidgetContext, usize) + 'static,
    {
        self.on_tab_changed = Some(smallbox!(f));
        self
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.tabs.get(self.selected).map(|(label, _)| label.resolve())
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    // Selects a tab without a sound or a callback.
    pub fn set_selected(&mut self, index: usize) {
        debug_assert!(index < self.tabs.len());
        self.selected = index;
    }

    pub fn tab_contents_mut(&mut self, index: usize) -> Option<&mut UiWidgetImpl> {
        self.tabs.get_mut(index).and_then(|(_, contents)| contents.as_mut())
    }

    fn select(&mut self, context: &mut dyn UiWidgetContext, index: usize) {
        if index >= self.tabs.len() || index == self.selected {
            return;
        }
        let previous = self.selected;
        self.selected = index;
        self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::TabSelected));
        if let Some(on_tab_changed) = &self.on_tab_changed {
            on_tab_changed(self, context, previous);
        }
    }

    // Each tab's size: its text's plus its frame's padding, at least the look's minimum width.
    fn tab_sizes(&self, ui_sys: &UiSystem) -> Vec<Vec2> {
        let padding = self.look.tab.map_or_else(
            || {
                let padding = ui_sys.egui_style().spacing.button_padding;
                Vec2::new(padding.x * 2.0, padding.y)
            },
            |frame| frame.padding(),
        );
        self.tabs
            .iter()
            .map(|(label, _)| {
                let (text_size, font_size) = internal::calc_text_size(ui_sys, self.look.font_scale, label.resolve());
                let width = (text_size.x + padding.x * 2.0).max(self.look.min_tab_width.unwrap_or(0.0));
                Vec2::new(width, text_size.y.max(font_size) + padding.y * 2.0)
            })
            .collect()
    }

    fn row_size(&self, sizes: &[Vec2]) -> (f32, f32) {
        let width = sizes.iter().map(|size| size.x).sum::<f32>() + self.look.spacing * sizes.len().saturating_sub(1) as f32;
        let height = sizes.iter().map(|size| size.y).fold(0.0, f32::max);
        (width, height)
    }

    fn paint_tab(&self, ui: &egui::Ui, rect: egui::Rect, response: &egui::Response, label: &str, font: &egui::FontId, selected: bool) {
        let state = if response.is_pointer_button_down_on() {
            UiSpriteButtonState::Pressed
        } else if response.hovered() {
            UiSpriteButtonState::Hovered
        } else {
            UiSpriteButtonState::Idle
        };

        let frame = if selected { self.look.selected.or(self.look.tab) } else { self.look.tab };
        let text_color = match frame {
            Some(frame) => {
                let (panel, tint) = frame.panel(state);
                ui.painter().add(nine_slice_shape(rect, panel, color32(tint)));
                frame.text_color().map_or_else(|| ui.visuals().text_color(), color32)
            }
            None => {
                let visuals = ui.style().interact_selectable(response, selected);
                ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);
                visuals.text_color()
            }
        };

        let clipped = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        clipped.text(rect.center(), Align2::CENTER_CENTER, label, font.clone(), text_color);
    }
}

