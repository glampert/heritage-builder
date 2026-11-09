use std::any::Any;
use smallbox::smallbox;
use egui::{Align2, Sense, pos2, vec2};

use common::{Color, Vec2};

use super::{UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiNineSlice, UiString,
        immediate::{self, color32, egui_vec2, nine_slice_shape, uv_full},
        internal,
        renderer::egui_texture,
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
    },
};

// ----------------------------------------------
// UiMenuItem
// ----------------------------------------------

pub type UiMenuItemPressed = UiWidgetCallback<dyn Fn(&UiMenuItem, &mut dyn UiWidgetContext)>;

// An entry of a dialog's list of choices, as the home menu's: its label, an optional subline
// under it ("Campaign I · mission 6 of 10"), centered across the menu. While hovered or selected
// it draws a fill panel behind it (grown past it) and a marker at its left, e.g. a realm's seal.
// Recorded as kind "menu_entry" ("menu_item" is the developer menu bar's), its value
// {"subline": ..., "selected": ...}. Plays ItemHovered and ItemSelected.
pub struct UiMenuItem {
    label: UiString,
    subline: Option<UiString>,
    font_scale: UiFontScale,
    subline_font_scale: UiFontScale,
    hover_panel: Option<(UiNineSlice, Vec2)>,
    marker: Option<(TextureHandle, Vec2)>,
    marker_margin: f32,
    text_color: Option<Color>,
    hover_text_color: Option<Color>,
    subline_color: Option<Color>,
    width: Option<f32>, // The menu's width if None.
    padding: f32,       // Above and below the text.
    selected: bool,
    enabled: bool,
    tooltip: Option<UiTooltipText>,
    sounds: UiWidgetSounds, // Hovered: ItemHovered, Selected: ItemSelected.
    hovered: bool,
    on_pressed: UiMenuItemPressed,
}

impl UiWidget for UiMenuItem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let size = self.measure(context);
        let font = internal::font(ui_sys, self.font_scale);
        let subline_font = internal::font(ui_sys, self.subline_font_scale);
        let label_height = internal::calc_text_line_height(ui_sys, self.font_scale);
        let enabled = self.enabled;

        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x).max(size.x);
            let sense = if enabled { Sense::click() } else { Sense::hover() };
            let (rect, response) = ui.allocate_exact_size(vec2(width, size.y), sense);
            let lit = enabled && (response.hovered() || self.selected);

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                if lit {
                    match &self.hover_panel {
                        Some((panel, grow)) => {
                            let tint = if response.is_pointer_button_down_on() { egui::Color32::from_white_alpha(170) } else { egui::Color32::WHITE };
                            painter.add(nine_slice_shape(rect.expand2(egui_vec2(*grow)), panel, tint));
                        }
                        None => {
                            let fill = ui.visuals().widgets.hovered.weak_bg_fill;
                            painter.rect_filled(rect, ui.visuals().widgets.hovered.corner_radius, fill);
                        }
                    }
                    if let Some((marker, marker_size)) = self.marker {
                        let marker_rect =
                            egui::Rect::from_min_size(pos2(rect.left() + self.marker_margin, rect.center().y - marker_size.y * 0.5), egui_vec2(marker_size));
                        painter.image(egui_texture(marker), marker_rect, uv_full(), egui::Color32::WHITE);
                    }
                }

                let base = self.text_color.map_or_else(|| ui.visuals().text_color(), color32);
                let mut text_color = if lit { self.hover_text_color.map_or(base, color32) } else { base };
                let mut subline_color = self.subline_color.map_or_else(|| ui.visuals().weak_text_color(), color32);
                if !enabled {
                    text_color = text_color.gamma_multiply(0.5);
                    subline_color = subline_color.gamma_multiply(0.5);
                }

                let top = rect.top() + self.padding;
                painter.text(pos2(rect.center().x, top), Align2::CENTER_TOP, self.label.resolve(), font.clone(), text_color);
                if let Some(subline) = &self.subline {
                    painter.text(pos2(rect.center().x, top + label_height), Align2::CENTER_TOP, subline.resolve(), subline_font.clone(), subline_color);
                }
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let value = serde_json::json!({
                "subline": self.subline.as_ref().map(UiString::resolve),
                "selected": self.selected,
            });
            immediate::record(ui_sys, ui, &response, "menu_entry", self.label.resolve(), tooltip, enabled, Some(value));
            (response.clicked() && enabled, response.hovered())
        });

        if hovered {
            if !self.hovered && enabled && !pressed {
                self.sounds.play(context, UiSoundEvent::Hovered, Some(UiSoundKey::ItemHovered));
            }
            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        if pressed {
            self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
            }
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let label = internal::calc_text_size(ui_sys, self.font_scale, self.label.resolve()).0;
        let mut height = internal::calc_text_line_height(ui_sys, self.font_scale);
        let mut text_width = label.x;
        if let Some(subline) = &self.subline {
            let size = internal::calc_text_size(ui_sys, self.subline_font_scale, subline.resolve()).0;
            text_width = text_width.max(size.x);
            height += internal::calc_text_line_height(ui_sys, self.subline_font_scale);
        }

        // Room for the marker on both sides, so the text stays centered.
        let marker = self.marker.map_or(0.0, |(_, size)| size.x + self.marker_margin * 2.0);
        let min_width = text_width + marker * 2.0;
        let width = self.width.unwrap_or_else(|| internal::available_size(ui_sys).x).max(min_width);
        Vec2::new(width, height + self.padding * 2.0)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiMenuItem {
    pub fn new(label: impl Into<UiString>) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());
        Self {
            label,
            subline: None,
            font_scale: UiFontScale(1.5),
            subline_font_scale: UiFontScale(0.8),
            hover_panel: None,
            marker: None,
            marker_margin: 8.0,
            text_color: None,
            hover_text_color: None,
            subline_color: None,
            width: None,
            padding: 4.0,
            selected: false,
            enabled: true,
            tooltip: None,
            sounds: UiWidgetSounds::new(),
            hovered: false,
            on_pressed: None,
        }
    }

    // A smaller line under the label.
    pub fn with_subline(mut self, subline: impl Into<UiString>) -> Self {
        self.subline = Some(subline.into());
        self
    }

    // The label's and the subline's font scales (1.5 and 0.8 by default).
    pub fn with_font_scales(mut self, label: UiFontScale, subline: UiFontScale) -> Self {
        debug_assert!(label.is_valid() && subline.is_valid());
        self.font_scale = label;
        self.subline_font_scale = subline;
        self
    }

    // Behind it while hovered or selected, `grow` points past it on each side.
    pub fn with_hover_panel(mut self, panel: UiNineSlice, grow: Vec2) -> Self {
        self.hover_panel = Some((panel, grow));
        self
    }

    // At its left while hovered or selected, `size` points large.
    pub fn with_marker(mut self, marker: TextureHandle, size: Vec2) -> Self {
        self.marker = Some((marker, size));
        self
    }

    // The label's colour, its colour while hovered or selected, and the subline's; the style's
    // (text, text, weak text) if None.
    pub fn with_text_colors(mut self, text: Option<Color>, hovered: Option<Color>, subline: Option<Color>) -> Self {
        self.text_color = text;
        self.hover_text_color = hovered;
        self.subline_color = subline;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    // Above and below the text, in points.
    pub fn with_padding(mut self, padding: f32) -> Self {
        debug_assert!(padding >= 0.0);
        self.padding = padding;
        self
    }

    // Drawn lit, as if hovered: e.g. the entry the keyboard is on.
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    // A disabled item fades and doesn't press, but still shows its tooltip.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // `event` (Hovered or Selected) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    pub fn on_pressed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiMenuItem, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_pressed = Some(smallbox!(f));
        self
    }

    pub fn set_subline(&mut self, subline: Option<UiString>) {
        self.subline = subline;
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn enable(&mut self, enable: bool) {
        self.enabled = enable;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
