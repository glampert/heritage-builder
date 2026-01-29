use std::{sync::atomic::{AtomicBool, Ordering}};

use crate::{
    ui::{self, UiSystem, UiTextureHandle, UiFontScale},
    utils::{Size, Vec2, Rect, Color},
};

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

pub struct UiStyleTextLabelInvisibleButtons<'ui> {
    border_size: imgui::StyleStackToken<'ui>,
    button_color: imgui::ColorStackToken<'ui>,
    button_color_hovered: imgui::ColorStackToken<'ui>,
    button_color_active: imgui::ColorStackToken<'ui>,
}

impl<'ui> UiStyleTextLabelInvisibleButtons<'ui> {
    #[inline]
    #[must_use]
    pub fn apply_overrides(ui_sys: &UiSystem) -> Self {
        let ui = unsafe { &*ui_sys.raw_ui_ptr() };

        Self {
            // We use buttons for text items so that the text label is centered automatically.
            // Make all button backgrounds and frames transparent/invisible.
            border_size: ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0)),
            button_color: ui.push_style_color(imgui::StyleColor::Button, [0.0, 0.0, 0.0, 0.0]),
            button_color_hovered: ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.0, 0.0, 0.0, 0.0]),
            button_color_active: ui.push_style_color(imgui::StyleColor::ButtonActive, [0.0, 0.0, 0.0, 0.0]),
        }
    }
}

pub const DEFAULT_ON_HOVER: Option<fn(&imgui::Ui, usize)> = None;
pub const ALWAYS_ENABLED: Option<fn(usize) -> bool> = None;

#[inline]
pub fn draw_centered_button_group(ui: &imgui::Ui,
                                  labels: &[&str],
                                  size: Option<Size>) -> Option<usize> {
    draw_centered_button_group_ex::<fn(&imgui::Ui, usize), fn(usize) -> bool>(
        ui,
        labels,
        size,
        None,
        DEFAULT_ON_HOVER,
        ALWAYS_ENABLED)
}

#[inline]
pub fn draw_centered_button_group_with_offsets(ui: &imgui::Ui,
                                               labels: &[&str],
                                               size: Option<Size>,
                                               offsets: Option<Vec2>) -> Option<usize> {
    draw_centered_button_group_ex::<fn(&imgui::Ui, usize), fn(usize) -> bool>(
        ui,
        labels,
        size,
        offsets,
        DEFAULT_ON_HOVER,
        ALWAYS_ENABLED)
}

pub fn draw_centered_button_group_ex<OnHovered, IsEnabled>(ui: &imgui::Ui,
                                                           labels: &[&str],
                                                           size: Option<Size>,
                                                           offsets: Option<Vec2>,
                                                           on_hovered: Option<OnHovered>,
                                                           is_enabled: Option<IsEnabled>) -> Option<usize>
    where
        OnHovered: Fn(&imgui::Ui, usize),
        IsEnabled: Fn(usize) -> bool
{
    if labels.is_empty() {
        return None;
    }

    let style = unsafe { ui.style() };

    let button_size = {
        if let Some(size) = size {
            size.to_vec2().to_array()
        } else {
            // Compute from labels:
            let mut longest_label = 0.0;
            let mut highest_label = 0.0;
            for label in labels {
                let label_size = ui.calc_text_size(label);
                if label_size[0] > longest_label {
                    longest_label = label_size[0];
                }
                if label_size[1] > highest_label {
                    highest_label = label_size[1];
                }
            }
            [longest_label + style.frame_padding[0], highest_label + style.frame_padding[1]]
        }
    };

    let start_offset = offsets.unwrap_or_default();
    let spacing_y = style.item_spacing[1];

    let total_height =
        labels.len() as f32 * button_size[1] +
        (labels.len().saturating_sub(1) as f32 * spacing_y);

    let avail = ui.content_region_avail();
    let start_x = ((avail[0] - button_size[0]) * 0.5) + start_offset.x;
    let start_y = ((avail[1] - total_height)   * 0.5) + start_offset.y;
    ui.set_cursor_pos([0.0, start_y]);

    let mut pressed_index = None;

    for (index, label) in labels.iter().enumerate() {
        let cursor = ui.cursor_pos();
        ui.set_cursor_pos([start_x, cursor[1]]);

        let enabled = if let Some(is_enabled) = &is_enabled {
            is_enabled(index)
        } else {
            true
        };

        let _btn_text_color = ui.push_style_color(
            imgui::StyleColor::Text,
            if enabled { [0.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 0.5] }
        );

        let button_label: &str = if !label.is_empty() {
            label
        } else {
            &format!("##Btn_{index}")
        };

        if ui.button_with_size(button_label, button_size) {
            pressed_index = Some(index);
        }

        draw_last_item_debug_rect(ui, Color::blue());

        if ui.is_item_hovered() && let Some(on_hovered) = &on_hovered {
            on_hovered(ui, index);
        }
    }

    pressed_index
}

pub fn draw_centered_button_group_custom_hover<IsEnabled>(ui_sys: &UiSystem,
                                                          labels: &[&str],
                                                          size: Option<Size>,
                                                          offsets: Option<Vec2>,
                                                          hover_ui_texture: UiTextureHandle,
                                                          is_enabled: Option<IsEnabled>) -> Option<usize>
    where IsEnabled: Clone + Fn(usize) -> bool
{
    // Make button background transparent and borderless.
    let _button_style_overrides =
        UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

    let is_enabled_copy = is_enabled.clone();
    draw_centered_button_group_ex(
        ui_sys.ui(),
        labels,
        size,
        offsets,
        Some(|ui: &imgui::Ui, button_index: usize| {
            // Draw underline effect when hovered / active:
            let button_rect = Rect::from_extents(
                Vec2::from_array(ui.item_rect_min()),
                Vec2::from_array(ui.item_rect_max())
            ).translated(Vec2::new(0.0, ui.text_line_height() - 5.0));

            let enabled = if let Some(is_enabled) = &is_enabled {
                is_enabled(button_index)
            } else {
                true
            };

            let underline_tint_color = if ui.is_item_active() || !enabled {
                imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
            } else {
                imgui::ImColor32::WHITE
            };

            ui.get_window_draw_list()
                .add_image(hover_ui_texture,
                           button_rect.min.to_array(),
                           button_rect.max.to_array())
                           .col(underline_tint_color)
                           .build();
        }),
        is_enabled_copy
    )
}

pub fn draw_button_custom_hover(ui_sys: &UiSystem,
                                id: &str,
                                text: &str,
                                enabled: bool,
                                hover_ui_texture: UiTextureHandle) -> bool {
    debug_assert!(!id.is_empty() && !text.is_empty());

    let _btn_style_overrides =
        UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

    let ui = ui_sys.ui();
    let _btn_text_color = ui.push_style_color(
        imgui::StyleColor::Text,
        if enabled { [0.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 0.5] }
    );

    let pressed = ui.button(format!("{text}##{id}"));
    draw_last_item_debug_rect(ui, Color::blue());

    if ui.is_item_hovered() {
        // Draw underline effect when hovered / active:
        let button_rect = Rect::from_extents(
            Vec2::from_array(ui.item_rect_min()),
            Vec2::from_array(ui.item_rect_max())
        ).translated(Vec2::new(0.0, 20.0));

        let underline_tint_color = if ui.is_item_active() || !enabled {
            imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
        } else {
            imgui::ImColor32::WHITE
        };

        ui.get_window_draw_list()
            .add_image(hover_ui_texture,
                       button_rect.min.to_array(),
                       button_rect.max.to_array())
                       .col(underline_tint_color)
                       .build();
    }

    pressed
}

pub fn draw_menu_heading(ui_sys: &UiSystem,
                         labels: &[&str],
                         offsets: Option<Vec2>,
                         separator_rect_offsets: Option<Rect>,
                         separator_ui_texture: UiTextureHandle) {
    // Draw heading as buttons, so everything is properly centered.
    draw_centered_text_label_group(
        ui_sys,
        labels,
        offsets
    );

    let ui = ui_sys.ui();

    let mut heading_separator_width = 0.0;
    for label in labels {
        let width = ui.calc_text_size(label)[0];
        if width > heading_separator_width {
            heading_separator_width = width;
        }
    }

    // Draw separator:
    let rect_offsets = separator_rect_offsets.unwrap_or_default();

    let heading_separator_rect = Rect::from_extents(
        Vec2::from_array([ui.item_rect_min()[0] - rect_offsets.min.x, ui.item_rect_min()[1] - rect_offsets.min.y]),
        Vec2::from_array([ui.item_rect_min()[0] + heading_separator_width + rect_offsets.max.x, ui.item_rect_max()[1] + rect_offsets.max.y])
    ).translated(Vec2::new(0.0, 40.0));

    ui.get_window_draw_list()
        .add_image(separator_ui_texture,
                   heading_separator_rect.min.to_array(),
                   heading_separator_rect.max.to_array())
                   .build();
}

// Draws a vertical separator immediately after the last submitted item.
// Call this *after* an item (Button, Text, Image, etc.) and typically before
// calling `same_line()` again.
pub fn draw_vertical_separator(ui: &imgui::Ui,
                               thickness: f32,
                               spacing_left: f32,
                               spacing_right: f32) {
    let item_min = ui.item_rect_min();
    let item_max = ui.item_rect_max();

    // Height matches the previous item.
    let y1 = item_min[1];
    let y2 = item_max[1];

    // X position is just to the right of the item.
    let x = item_max[0] + (spacing_left * 0.5);

    let color = ui.style_color(imgui::StyleColor::Separator);

    ui.get_window_draw_list()
        .add_line([x, y1], [x, y2], color)
        .thickness(thickness)
        .build();

    // Advance cursor so following items don't overlap the separator.
    ui.dummy([spacing_right, 0.0]);
}

pub fn draw_window_style_rect(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, min: Vec2, max: Vec2) {
    // Match window visuals:
    let style = unsafe { ui.style() };
    let rounding = style.window_rounding;
    let border_thickness = style.window_border_size;
    let bg_color = ui.style_color(imgui::StyleColor::WindowBg);
    let border_color = ui.style_color(imgui::StyleColor::Border);

    // Background:
    draw_list
        .add_rect(min.to_array(), max.to_array(), bg_color)
        .filled(true)
        .rounding(rounding)
        .build();

    // Border (pixel-aligned):
    if border_thickness > 0.0 {
        let inset = border_thickness * 0.5;
        let border_min = [min.x - inset, min.y - inset];
        let border_max = [max.x + inset, max.y + inset];

        draw_list
            .add_rect(border_min, border_max, border_color)
            .filled(false)
            .rounding(rounding)
            .thickness(border_thickness)
            .build();
    }
}

pub fn draw_sprite(ui_sys: &UiSystem,
                   id: &str,
                   size: Vec2,
                   sprite_texture: UiTextureHandle,
                   tooltip_bg_texture: UiTextureHandle,
                   tooltip: Option<&str>) {
    let ui = ui_sys.ui();
    ui.invisible_button_flags(id, size.to_array(), imgui::ButtonFlags::empty());
    ui.get_window_draw_list().add_image(sprite_texture, ui.item_rect_min(), ui.item_rect_max()).build();
    draw_last_item_debug_rect(ui, Color::blue());

    if ui.is_item_hovered() && let Some(tooltip_text) = tooltip {
        ui::custom_tooltip(ui_sys, UiFontScale(0.8), Some(tooltip_bg_texture), || ui.text(tooltip_text));
    }
}

pub fn draw_centered_text_label_group(ui_sys: &UiSystem,
                                      labels: &[&str],
                                      offsets: Option<Vec2>) {
    // Make button background transparent and borderless.
    let _button_style_overrides =
        UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

    draw_centered_button_group_with_offsets(
        ui_sys.ui(),
        labels,
        None,
        offsets,
    );
}

// NOTE: This assumes UiStyleTextLabelInvisibleButtons overrides are already set.
pub fn draw_centered_text_label(ui: &imgui::Ui,
                                label: &str,
                                size: Vec2) {
    ui.button_with_size(label, size.to_array());
    draw_last_item_debug_rect(ui, Color::green());
}

pub fn spacing(ui: &imgui::Ui, size: Vec2) {
    ui.dummy(size.to_array());
    draw_last_item_debug_rect(ui, Color::yellow());
}

pub fn push_item_spacing(ui: &imgui::Ui, horizontal: f32, vertical: f32) -> imgui::StyleStackToken<'_> {
    ui.push_style_var(imgui::StyleVar::ItemSpacing([horizontal, vertical]))
}

pub fn set_next_window_pos(pos: Vec2, pivot: Vec2, cond: imgui::Condition) {
    unsafe {
        imgui::sys::igSetNextWindowPos(
            imgui::sys::ImVec2 { x: pos.x, y: pos.y },
            cond as _,
            imgui::sys::ImVec2 { x: pivot.x, y: pivot.y },
        );
    }
}

#[inline]
pub fn window_flags() -> imgui::WindowFlags {
    imgui::WindowFlags::ALWAYS_AUTO_RESIZE
    | imgui::WindowFlags::NO_RESIZE
    | imgui::WindowFlags::NO_DECORATION
    | imgui::WindowFlags::NO_SCROLLBAR
    | imgui::WindowFlags::NO_MOVE
    | imgui::WindowFlags::NO_COLLAPSE
}

// ----------------------------------------------
// ImGui debug helpers
// ----------------------------------------------

static ENABLE_WIDGETS_DEBUG_DRAW: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn enable_debug_draw(enable: bool) {
    ENABLE_WIDGETS_DEBUG_DRAW.store(enable, Ordering::Relaxed);
}

#[inline]
pub fn is_debug_draw_enabled() -> bool {
    ENABLE_WIDGETS_DEBUG_DRAW.load(Ordering::Relaxed)
}

pub fn draw_debug_rect(ui: &imgui::Ui, rect: &Rect, color: Color) {
    ui.get_window_draw_list()
        .add_rect(rect.min.to_array(),
                  rect.max.to_array(),
                  imgui::ImColor32::from_rgba_f32s(color.r, color.g, color.b, color.a))
                  .build();
}

pub fn draw_last_item_debug_rect(ui: &imgui::Ui, color: Color) {
    if !is_debug_draw_enabled() {
        return;
    }

    let rect = Rect::from_extents(
        Vec2::from_array(ui.item_rect_min()),
        Vec2::from_array(ui.item_rect_max())
    );

    draw_debug_rect(ui, &rect, color);
}

pub fn draw_current_window_debug_rect(ui: &imgui::Ui) {
    if !is_debug_draw_enabled() {
        return;
    }

    // NOTE: Shrink the rect so it falls within the window bounds,
    // otherwise ImGui would cull it.
    let window_rect = Rect::from_pos_and_size(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    ).shrunk(Vec2::new(4.0, 4.0));

    draw_debug_rect(ui, &window_rect, Color::cyan());
}
