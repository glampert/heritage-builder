use std::any::Any;
use egui::{Color32, Sense, pos2, vec2};

use common::{Color, Vec2};

use super::{UiWidget, UiWidgetContext};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{UiFontScale, UiNineSlice, UiString, immediate::{self, egui_vec2, nine_slice_shape, uv_full}, internal, renderer::egui_texture},
};

// ----------------------------------------------
// UiText
// ----------------------------------------------

// A line of a heading.
#[derive(Clone, Default)]
pub struct UiText {
    pub string: UiString, // A key, or literal text.
    pub font_scale: UiFontScale,
    pub color: Option<Color>,    // The style's text color if None.
    pub wrap_width: Option<f32>, // Wraps at this width, in points; one line if None.
}

impl UiText {
    pub fn new(string: impl Into<UiString>, font_scale: UiFontScale) -> Self {
        Self { string: string.into(), font_scale, color: None, wrap_width: None }
    }

    // A placeholder line, filled in later with UiMenuHeading::set_line_string.
    pub const fn empty(font_scale: UiFontScale) -> Self {
        Self { string: UiString::Literal(String::new()), font_scale, color: None, wrap_width: None }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_wrap_width(mut self, width: f32) -> Self {
        self.wrap_width = Some(width);
        self
    }
}

// ----------------------------------------------
// UiMenuHeading
// ----------------------------------------------

// Centered window heading.
// Can consist of multiple lines and an optional separator sprite at the end.
pub struct UiMenuHeading {
    lines: Vec<UiText>,
    separator: Option<TextureHandle>,
    separator_art: Option<UiSeparatorArt>, // In place of `separator`.
    margin_top: f32,
    margin_bottom: f32,
    center_vertically: bool,
    center_horizontally: bool,
}

impl UiWidget for UiMenuHeading {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());
        let ui_sys = context.ui_sys();

        if self.margin_top > 0.0 {
            ui_sys.with_ui(|ui| ui.add_space(self.margin_top));
        }

        let group = internal::draw_centered_text_group(context, &self.lines, self.center_vertically, self.center_horizontally);

        if ui_sys.is_widget_registry_enabled() && group.is_positive() {
            let text: Vec<&str> = self.lines.iter().map(|line| line.string.resolve()).filter(|line| !line.is_empty()).collect();
            let visible = ui_sys.with_ui(|ui| ui.clip_rect().contains(group.center())); // As ui.click sees it.
            immediate::record_rect_visible(ui_sys, "heading", &text.join(" "), group, false, visible, None);
        }

        if let Some(art) = &self.separator_art
            && group.is_positive()
        {
            ui_sys.with_ui(|ui| {
                // Room for the ornament too, as UiSeparator gives it; the rule along its middle.
                let rect = egui::Rect::from_min_size(pos2(group.left(), ui.cursor().top()), vec2(group.width(), art.total_height()));
                art.paint(ui, rect);
                ui.advance_cursor_after_rect(rect);
            });
        } else if let Some(separator) = self.separator
            && group.is_positive()
        {
            // As tall as the last line's font.
            let separator_height = internal::font(ui_sys, ui_sys.current_window_front_scale()).size;

            ui_sys.with_ui(|ui| {
                let separator_rect = egui::Rect::from_min_size(pos2(group.left(), ui.cursor().top()), vec2(group.width(), separator_height));
                ui.painter().image(egui_texture(separator), separator_rect, uv_full(), Color32::WHITE);
                ui.advance_cursor_after_rect(separator_rect);
            });
        }

        if self.margin_bottom > 0.0 {
            ui_sys.with_ui(|ui| ui.add_space(self.margin_bottom));
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let mut size = Vec2::zero();
        let mut non_empty_lines_count = 0;

        for line in &self.lines {
            if line.string.is_empty() {
                continue;
            }

            let line_size = context.calc_text_size(line.font_scale, line.string.resolve());
            size.x = size.x.max(line_size.x); // Max width.
            size.y += line_size.y; // Total height.

            non_empty_lines_count += 1;
        }

        if non_empty_lines_count > 0 {
            // Add inter-line spacing.
            size.y += internal::item_spacing(context.ui_sys()).y * (non_empty_lines_count - 1) as f32;

            // The separator under the lines and the margins, as draw() lays them out.
            if let Some(art) = &self.separator_art {
                size.y += art.total_height();
            } else if self.separator.is_some()
                && let Some(last_line) = self.lines.iter().rev().find(|line| !line.string.is_empty())
            {
                size.y += internal::font(context.ui_sys(), last_line.font_scale).size;
            }
            size.y += self.margin_top + self.margin_bottom;
        }

        size
    }

    fn font_scale(&self) -> UiFontScale {
        self.lines[0].font_scale
    }
}

impl UiMenuHeading {
    // Lines of text, each centered on the widest. A line can be empty, as a placeholder for
    // set_line_string. The first line's font scale is the heading's.
    pub fn new(lines: impl IntoIterator<Item = UiText>) -> Self {
        let lines: Vec<UiText> = lines.into_iter().collect();
        debug_assert!(!lines.is_empty());

        Self {
            lines,
            separator: None,
            separator_art: None,
            margin_top: 0.0,
            margin_bottom: 0.0,
            center_vertically: false,
            center_horizontally: true,
        }
    }

    // A sprite under the last line, as wide as the heading.
    pub fn with_separator<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.separator = Some(context.load_texture(sprite.into()));
        self
    }

    // A 9-slice rule under the last line, as wide as the heading, with an optional ornament at
    // its middle: a skin's separator.
    pub fn with_separator_art(mut self, art: UiSeparatorArt) -> Self {
        self.separator_art = Some(art);
        self
    }

    pub fn with_margin_top(mut self, margin: f32) -> Self {
        debug_assert!(margin >= 0.0);
        self.margin_top = margin;
        self
    }

    pub fn with_margin_bottom(mut self, margin: f32) -> Self {
        debug_assert!(margin >= 0.0);
        self.margin_bottom = margin;
        self
    }

    // Centered in the room left in the menu, instead of starting at its top.
    pub fn with_center_vertically(mut self, center: bool) -> Self {
        self.center_vertically = center;
        self
    }

    // Centered across the menu (the default), or starting at its left.
    pub fn with_center_horizontally(mut self, center: bool) -> Self {
        self.center_horizontally = center;
        self
    }

    #[inline]
    pub fn lines(&self) -> &Vec<UiText> {
        &self.lines
    }

    #[inline]
    pub fn lines_mut(&mut self) -> &mut Vec<UiText> {
        &mut self.lines
    }

    #[inline]
    pub fn set_line(&mut self, index: usize, text: UiText) {
        self.lines[index] = text;
    }

    #[inline]
    pub fn set_line_string(&mut self, index: usize, string: &str) {
        self.lines[index].string.set_literal(string);
    }

    #[inline]
    pub fn set_line_font_scale(&mut self, index: usize, font_scale: UiFontScale) {
        self.lines[index].font_scale = font_scale;
    }

    #[inline]
    pub fn set_line_color(&mut self, index: usize, color: Option<Color>) {
        self.lines[index].color = color;
    }

    #[inline]
    pub fn clear_all_lines(&mut self) {
        for line in &mut self.lines {
            line.string.clear();
        }
    }
}

// ----------------------------------------------
// UiSeparatorArt
// ----------------------------------------------

// A separator's look: a 9-slice rule `height` points tall across its rect, with an ornament
// centred on it, e.g. a skin's divider (its rule and its diamond or lotus). Used by
// UiMenuHeading::with_separator_art and UiSeparator::with_art.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiSeparatorArt {
    pub rule: UiNineSlice,
    pub height: f32,
    pub ornament: Option<(TextureHandle, Vec2)>, // Its texture and size in points.
}

impl UiSeparatorArt {
    pub fn new(rule: UiNineSlice, height: f32) -> Self {
        Self { rule, height: height.max(0.0), ornament: None }
    }

    pub fn with_ornament(mut self, texture: TextureHandle, size: Vec2) -> Self {
        self.ornament = Some((texture, size));
        self
    }

    // The height it takes: the rule's, or the ornament's if that's taller.
    pub fn total_height(&self) -> f32 {
        self.ornament.map_or(self.height, |(_, size)| self.height.max(size.y))
    }

    // The rule along the middle of `rect`, `height` tall, and the ornament at its centre.
    pub(crate) fn paint(&self, ui: &egui::Ui, rect: egui::Rect) {
        let rule_rect = egui::Rect::from_center_size(rect.center(), vec2(rect.width(), self.height));
        ui.painter().add(nine_slice_shape(rule_rect, &self.rule, Color32::WHITE));
        if let Some((texture, size)) = self.ornament {
            let ornament_rect = egui::Rect::from_center_size(rect.center(), egui_vec2(size));
            ui.painter().image(egui_texture(texture), ornament_rect, uv_full(), Color32::WHITE);
        }
    }
}

// ----------------------------------------------
// UiSizedTextLabel
// ----------------------------------------------

// Text centered in a box of a given size.
pub struct UiSizedTextLabel {
    font_scale: UiFontScale,
    label: UiString,
    size: Vec2,
    alert: bool, // Drawn in the style's error color, e.g. a treasury in debt.
}

impl UiWidget for UiSizedTextLabel {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let size = self.measure(context);
        let ui_sys = context.ui_sys();

        ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            let color = if self.alert { ui.visuals().error_fg_color } else { ui.visuals().text_color() };
            let galley = ui.painter().layout_no_wrap(self.label.resolve().to_string(), internal::font(ui_sys, self.font_scale), color);
            ui.painter().galley(rect.center() - galley.size() * 0.5, galley, color);

            immediate::record(ui_sys, ui, &response, "text", self.label.resolve(), None, true, None);
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        if self.size != Vec2::zero() {
            return self.size;
        }

        // Sized to the label, with a framed widget's padding.
        let (text_size, font_size) = context.calc_text_and_font_size(self.font_scale, self.label.resolve());

        let padding = context.ui_sys().egui_style().spacing.button_padding;
        let width = text_size.x + (padding.x * 2.0);
        let height = text_size.y.max(font_size) + (padding.y * 2.0);

        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiSizedTextLabel {
    pub fn new(label: impl Into<UiString>) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());

        Self { font_scale: UiFontScale::default(), label, size: Vec2::zero(), alert: false }
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // The box the text is centered in; sized to the text if not given.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    #[inline]
    pub fn set_label(&mut self, label: impl Into<UiString>) {
        self.label = label.into();
    }

    #[inline]
    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }

    // Draws the text in the style's error color while on.
    #[inline]
    pub fn set_alert(&mut self, alert: bool) {
        self.alert = alert;
    }
}
