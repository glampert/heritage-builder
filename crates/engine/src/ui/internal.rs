// Layout helpers for the retained widgets (widgets.rs): measuring text and widgets, and drawing
// groups of them centered in a menu. Groups place each widget at a position they compute, in a
// child Ui at that rect (UiSystem::draw_at), then move the menu's cursor past the whole group.

use egui::{Color32, TextStyle, pos2, vec2};
use smallvec::SmallVec;

use common::Vec2;

use super::{UiFontScale, UiSystem, immediate::color32, widgets::*};

// ----------------------------------------------
// Measuring
// ----------------------------------------------

// The current theme's plain text font at `font_scale`.
pub fn font(ui_sys: &UiSystem, font_scale: UiFontScale) -> egui::FontId {
    let mut font = ui_sys.style_font(&TextStyle::Body);
    font.size *= font_scale.0;
    font
}

// The size `text` takes in the current theme's font at `font_scale`, and that font's size.
pub fn calc_text_size(ui_sys: &UiSystem, font_scale: UiFontScale, text: &str) -> (Vec2, f32) {
    let font = font(ui_sys, font_scale);
    let font_size = font.size;

    if text.is_empty() {
        return (Vec2::zero(), font_size);
    }

    let size = ui_sys.ctx().fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font, Color32::WHITE).size());
    (Vec2::new(size.x, size.y), font_size)
}

// The size of `text` wrapped at `wrap_width`.
pub fn calc_wrapped_text_size(ui_sys: &UiSystem, font_scale: UiFontScale, text: &str, wrap_width: f32) -> Vec2 {
    let font = font(ui_sys, font_scale);
    let size = ui_sys.ctx().fonts_mut(|fonts| fonts.layout(text.to_owned(), font, Color32::WHITE, wrap_width).size());
    Vec2::new(size.x, size.y)
}

pub fn calc_text_line_height(ui_sys: &UiSystem, font_scale: UiFontScale) -> f32 {
    let font = font(ui_sys, font_scale);
    ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&font))
}

pub fn calc_text_line_height_with_spacing(ui_sys: &UiSystem, font_scale: UiFontScale) -> f32 {
    calc_text_line_height(ui_sys, font_scale) + item_spacing(ui_sys).y
}

// The spacing between items in the current container (a menu's widget spacing), or the style's
// outside one.
pub fn item_spacing(ui_sys: &UiSystem) -> egui::Vec2 {
    if ui_sys.has_ui() { ui_sys.with_ui(|ui| ui.spacing().item_spacing) } else { ui_sys.egui_style().spacing.item_spacing }
}

// The height of a framed widget, such as a button or an input, with text at `font_scale`.
pub fn calc_frame_height(ui_sys: &UiSystem, font_scale: UiFontScale) -> f32 {
    let style = ui_sys.egui_style();
    (calc_text_line_height(ui_sys, font_scale) + style.spacing.button_padding.y * 2.0).max(style.spacing.interact_size.y)
}

// The width of a slider, input or dropdown: the one the group it's in gives it, or the default.
pub fn item_width(ui_sys: &UiSystem) -> f32 {
    ui_sys.item_width().unwrap_or(ui_sys.egui_style().spacing.slider_width)
}

// Works for most labeled widgets (input text, combo, slider): the field, then the label.
pub fn calc_labeled_widget_size(ui_sys: &UiSystem, font_scale: UiFontScale, label: &str) -> Vec2 {
    let style = ui_sys.egui_style();
    let height = calc_frame_height(ui_sys, font_scale);
    let mut width = item_width(ui_sys);

    if !label.is_empty() {
        let (label_size, _) = calc_text_size(ui_sys, font_scale, label);
        width += style.spacing.icon_spacing + label_size.x;
    }

    Vec2::new(width, height)
}

// The room left in the current container, or none outside one.
pub fn available_size(ui_sys: &UiSystem) -> Vec2 {
    if !ui_sys.has_ui() {
        return Vec2::zero();
    }
    let size = ui_sys.with_ui(super::immediate::fill_size);
    Vec2::new(size.x, size.y)
}

// Resolve child window container size.
// requested:
//   > 0.0 -> fixed size
//   = 0.0 -> use remaining host window size
//   < 0.0 -> use remaining host window size minus abs(size)
pub fn calc_child_window_size(ui_sys: &UiSystem, requested: Vec2) -> Vec2 {
    let region_avail = available_size(ui_sys);
    let resolve = |requested: f32, available: f32| {
        if requested > 0.0 {
            requested
        } else if requested == 0.0 {
            available
        } else {
            (available + requested).max(0.0)
        }
    };
    Vec2::new(resolve(requested.x, region_avail.x), resolve(requested.y, region_avail.y))
}

// The size of a separator line.
//  - `horizontal = true`  -> horizontal separator, across the container
//  - `horizontal = false` -> vertical separator
//  - `thickness` -> the line's
pub fn calc_separator_size(ui_sys: &UiSystem, horizontal: bool, thickness: f32) -> Vec2 {
    let region_avail = available_size(ui_sys);
    let spacing = item_spacing(ui_sys);

    if horizontal {
        Vec2::new(region_avail.x, thickness + spacing.y * 2.0)
    } else {
        Vec2::new(thickness + spacing.x * 2.0, region_avail.y)
    }
}

// ----------------------------------------------
// Centered groups
// ----------------------------------------------

// The top left corner of the room left in the current container, and its size (none in a sizing
// pass, so centering doesn't decide the container's size).
fn cursor_and_available(ui_sys: &UiSystem) -> (egui::Pos2, egui::Vec2) {
    ui_sys.with_ui(|ui| (ui.cursor().min, super::immediate::fill_size(ui)))
}

// Where a group of `size` starts to be centered in `available` room from `start`. A group larger
// than the room, or without room, starts at `start` and overflows (or scrolls) past its end.
#[inline]
fn centered_start(start: f32, available: f32, size: f32) -> f32 {
    if available > 0.0 { start + ((available - size) * 0.5).max(0.0) } else { start }
}

// Moves the current container's cursor past a group drawn at `rect`.
fn advance_past(ui_sys: &UiSystem, rect: egui::Rect) {
    ui_sys.with_ui(|ui| {
        ui.advance_cursor_after_rect(rect);
    });
}

// Lines of text, each centered on the widest. Returns the group's rect, in window points.
pub fn draw_centered_text_group(
    context: &mut dyn UiWidgetContext,
    lines: &[UiText],
    vertical: bool,
    horizontal: bool,
) -> egui::Rect {
    let ui_sys = context.ui_sys();

    if lines.is_empty() {
        return egui::Rect::NOTHING;
    }

    // Measure text sizes:
    let text_sizes: SmallVec<[Vec2; 16]> = lines
        .iter()
        .map(|line| match line.wrap_width {
            Some(width) => calc_wrapped_text_size(ui_sys, line.font_scale, &line.string, width),
            None => context.calc_text_size(line.font_scale, &line.string),
        })
        .collect();

    // A line is as tall as its font, with spacing; a wrapped one, as tall as its rows.
    let line_heights: SmallVec<[f32; 16]> = lines
        .iter()
        .zip(&text_sizes)
        .map(|(line, size)| {
            if line.string.is_empty() {
                0.0
            } else if line.wrap_width.is_some() {
                size.y + item_spacing(ui_sys).y
            } else {
                calc_text_line_height_with_spacing(ui_sys, line.font_scale)
            }
        })
        .collect();

    let total_height: f32 = line_heights.iter().sum();
    if total_height == 0.0 {
        // All lines were blank. Early out.
        return egui::Rect::NOTHING;
    }

    let max_width = text_sizes.iter().map(|size| size.x).fold(0.0, f32::max);

    let (cursor, region_avail) = cursor_and_available(ui_sys);

    // Compute group origin (top-left):
    let start_x = if horizontal && !context.is_side_by_side_layout() {
        centered_start(cursor.x, region_avail.x, max_width)
    } else {
        cursor.x
    };
    let start_y = if vertical { centered_start(cursor.y, region_avail.y, total_height) } else { cursor.y };

    // Draw each line:
    ui_sys.with_ui(|ui| {
        let default_color = ui.visuals().text_color();
        let mut offset_y = 0.0;

        for ((line, size), line_height) in lines.iter().zip(text_sizes).zip(line_heights) {
            if line.string.is_empty() {
                continue;
            }

            let x = start_x + (max_width - size.x) * 0.5;
            let y = start_y + offset_y;

            let color = line.color.map_or(default_color, color32);
            let wrap_width = line.wrap_width.unwrap_or(f32::INFINITY);
            let galley = ui.painter().layout(line.string.clone(), font(ui_sys, line.font_scale), color, wrap_width);
            ui.painter().galley(pos2(x, y), galley, color);

            offset_y += line_height;
        }
    });

    let group = egui::Rect::from_min_size(pos2(start_x, start_y), vec2(max_width, total_height));
    advance_past(ui_sys, egui::Rect::from_min_max(pos2(cursor.x, start_y), pos2(cursor.x.max(group.right()), group.bottom())));
    group
}

pub fn draw_centered_widget_group(
    context: &mut dyn UiWidgetContext,
    widgets: &mut [UiWidgetImpl],
    vertical: bool,
    horizontal: bool,
    stack_vertically: bool,
    margins: (f32, f32),
) -> egui::Rect {
    if widgets.is_empty() {
        return egui::Rect::NOTHING;
    }

    if !stack_vertically {
        // Position widgets side-by-side.
        context.begin_side_by_side_layout();
    }

    let (margin_left, margin_right) = margins;
    debug_assert!(margin_left >= 0.0 && margin_right >= 0.0);

    let ui_sys = context.ui_sys();
    let item_spacing = ui_sys.with_ui(|ui| ui.spacing().item_spacing);

    // Measure widget sizes:
    let widget_sizes: SmallVec<[Vec2; 16]> = widgets.iter().map(|widget| widget.measure(context)).collect();

    let count = widgets.len() as f32;
    let mut max_width: f32 = 0.0;
    let mut max_height: f32 = 0.0;
    let mut widgets_size = Vec2::zero(); // Without spacing.

    for widget_size in &widget_sizes {
        max_width = max_width.max(widget_size.x);
        max_height = max_height.max(widget_size.y);
        widgets_size += *widget_size;
    }

    let (cursor, region_avail) = cursor_and_available(ui_sys);
    let cursor_start = cursor + vec2(margin_left, 0.0);

    // A stack taller than its room closes up its spacing, down to none, so its widgets fit where
    // they can: a menu's buttons at a small window height.
    let spacing_y = if stack_vertically && widgets.len() > 1 && region_avail.y > 0.0 {
        let room = (region_avail.y - widgets_size.y) / (count - 1.0);
        item_spacing.y.min(room).max(0.0)
    } else {
        item_spacing.y
    };

    let total_width = if stack_vertically { max_width } else { widgets_size.x + item_spacing.x * (count - 1.0) };
    let total_height = if stack_vertically { widgets_size.y + spacing_y * (count - 1.0) } else { max_height };

    // Compute group origin (top-left):
    let start_x = if horizontal { centered_start(cursor_start.x, region_avail.x, total_width) } else { cursor_start.x };
    let start_y = if vertical { centered_start(cursor_start.y, region_avail.y, total_height) } else { cursor_start.y };

    // Draw each widget:
    let mut offset = 0.0;
    for (widget, widget_size) in widgets.iter_mut().zip(widget_sizes.iter()) {
        let widget_pos = if stack_vertically {
            // Stack widgets vertically.
            let position = pos2(start_x + (max_width - widget_size.x) * 0.5, start_y + offset);
            offset += widget_size.y + spacing_y;
            position
        } else {
            // Position widgets side-by-side.
            let position = pos2(start_x + offset, start_y);
            offset += widget_size.x + item_spacing.x;
            position
        };

        // Inputs, sliders and dropdowns fill the width up to the group's right margin.
        let item_width = (margin_right > 0.0).then(|| (region_avail.x - (widget_pos.x - cursor.x) - margin_left - margin_right).max(0.0));
        let size = vec2(item_width.unwrap_or(widget_size.x).max(widget_size.x), widget_size.y);

        let previous_item_width = ui_sys.set_item_width(item_width);
        ui_sys.draw_at(egui::Rect::from_min_size(widget_pos, size), |_| widget.draw(context));
        ui_sys.set_item_width(previous_item_width);
    }

    if !stack_vertically {
        context.end_side_by_side_layout();
    }

    let group = egui::Rect::from_min_size(pos2(start_x, start_y), vec2(total_width, total_height));
    advance_past(ui_sys, egui::Rect::from_min_max(pos2(cursor.x, start_y), pos2(cursor.x.max(group.right()), group.bottom())));
    group
}

pub fn draw_centered_labeled_widget_group(
    context: &mut dyn UiWidgetContext,
    labels_and_widgets: &mut [(String, UiWidgetImpl)],
    vertical: bool,
    horizontal: bool,
    margins: (f32, f32),
    label_column_width: Option<f32>,
) -> egui::Rect {
    if labels_and_widgets.is_empty() {
        return egui::Rect::NOTHING;
    }

    let (margin_left, margin_right) = margins;
    debug_assert!(margin_left >= 0.0 && margin_right >= 0.0);

    let ui_sys = context.ui_sys();
    let item_spacing = ui_sys.with_ui(|ui| ui.spacing().item_spacing);

    // Measure the labels and widgets:
    let sizes: SmallVec<[(Vec2, Vec2); 16]> = labels_and_widgets
        .iter()
        .map(|(label, widget)| (context.calc_text_size(widget.font_scale(), label), widget.measure(context)))
        .collect();

    // The labels' column: as wide as the longest, or the width given, shared with other groups.
    let longest_label = sizes.iter().map(|(label_size, _)| label_size.x).fold(0.0, f32::max);
    let label_column = label_column_width.map_or(longest_label, |width| width.max(longest_label));

    let mut max_width: f32 = 0.0;
    let mut total_height: f32 = 0.0;

    let row_sizes: SmallVec<[Vec2; 16]> = sizes
        .iter()
        .map(|(label_size, widget_size)| {
            // NOTE: Both including item spacing.
            let label_width = if label_column_width.is_some() { label_column } else { label_size.x };
            let width = label_width + item_spacing.x + widget_size.x;
            let height = label_size.y.max(widget_size.y) + item_spacing.y;
            max_width = max_width.max(width);
            total_height += height;
            Vec2::new(width, height)
        })
        .collect();

    let (cursor, region_avail) = cursor_and_available(ui_sys);
    let cursor_start = cursor + vec2(margin_left, 0.0);

    // Compute group origin (top-left):
    let start_x = if horizontal { centered_start(cursor_start.x, region_avail.x, max_width) } else { cursor_start.x };
    let start_y = if vertical { centered_start(cursor_start.y, region_avail.y, total_height) } else { cursor_start.y };

    // Draw each row: its label, then its widget after the labels' column.
    let mut offset_y = 0.0;
    for (((label, widget), row_size), (_, widget_size)) in labels_and_widgets.iter_mut().zip(row_sizes.iter()).zip(sizes.iter()) {
        let font_scale = widget.font_scale();
        let y = start_y + offset_y;
        let row_height = row_size.y - item_spacing.y;

        // The label, centered on the row's height like a framed widget's text.
        ui_sys.with_ui(|ui| {
            let color = ui.visuals().text_color();
            let galley = ui.painter().layout_no_wrap(label.clone(), font(ui_sys, font_scale), color);
            let label_y = y + (row_height - galley.size().y) * 0.5;
            ui.painter().galley(pos2(start_x, label_y), galley, color);
        });

        let x = start_x + label_column + item_spacing.x;

        // Inputs, sliders and dropdowns fill the width up to the group's right margin.
        let item_width = (margin_right > 0.0).then(|| (region_avail.x - (x - cursor.x) - margin_left - margin_right).max(0.0));
        let size = vec2(item_width.unwrap_or(widget_size.x).max(widget_size.x), row_height);

        // The group's label names the widget for UI automation.
        context.ui_sys().push_widget_path(label);
        let previous_item_width = ui_sys.set_item_width(item_width);
        ui_sys.draw_at(egui::Rect::from_min_size(pos2(x, y), size), |_| widget.draw(context));
        ui_sys.set_item_width(previous_item_width);
        context.ui_sys().pop_widget_path();

        offset_y += row_size.y;
    }

    let group = egui::Rect::from_min_size(pos2(start_x, start_y), vec2(max_width, total_height));
    advance_past(ui_sys, egui::Rect::from_min_max(pos2(cursor.x, start_y), pos2(cursor.x.max(group.right()), group.bottom())));
    group
}
