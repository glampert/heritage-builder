use std::{sync::atomic::{AtomicBool, Ordering}};

use crate::{
    utils::{Size, Vec2, Rect, Color},
    imgui_ui::{UiSystem, UiTextureHandle},
};

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

pub struct UiStyleOverrides<'ui> {
    // Main Windows:
    window_bg_color: imgui::ColorStackToken<'ui>,
    window_title_bg_color_active: imgui::ColorStackToken<'ui>,
    window_title_bg_color_inactive: imgui::ColorStackToken<'ui>,
    window_title_bg_color_collapsed: imgui::ColorStackToken<'ui>,

    // Text:
    text_color: imgui::ColorStackToken<'ui>,
    text_font: imgui::FontStackToken<'ui>,

    // Buttons:
    button_color: imgui::ColorStackToken<'ui>,
    button_color_hovered: imgui::ColorStackToken<'ui>,
    button_color_active: imgui::ColorStackToken<'ui>,

    // Tooltips:
    popup_bg_color: imgui::ColorStackToken<'ui>,
    popup_border_size: imgui::StyleStackToken<'ui>,

    // Child Windows:
    child_bg_color: imgui::ColorStackToken<'ui>,

    // InputText:
    frame_bg_color: imgui::ColorStackToken<'ui>,
    frame_bg_color_hovered: imgui::ColorStackToken<'ui>,
    frame_bg_color_active: imgui::ColorStackToken<'ui>,

    // Selectable / TreeNode / Collapsing Header:
    header_color: imgui::ColorStackToken<'ui>,
    header_color_hovered: imgui::ColorStackToken<'ui>,
    header_color_active: imgui::ColorStackToken<'ui>,

    // Scrollbar:
    scrollbar_bg_color: imgui::ColorStackToken<'ui>,
    scrollbar_grab_color: imgui::ColorStackToken<'ui>,
    scrollbar_grab_color_hovered: imgui::ColorStackToken<'ui>,
    scrollbar_grab_color_active: imgui::ColorStackToken<'ui>,
}

impl<'ui> UiStyleOverrides<'ui> {
    #[inline]
    #[must_use]
    pub fn dev_editor_menus(_ui_sys: &UiSystem) -> Option<Self> {
        None // Default style is already the dev/editor menus style.
    }

    #[inline]
    #[must_use]
    pub fn in_game_hud_menus(ui_sys: &UiSystem) -> Option<Self> {
        let ui = unsafe { &*ui_sys.raw_ui_ptr() };

        let bg_color    = [0.93, 0.91, 0.77, 1.0];
        let btn_hovered = [0.98, 0.95, 0.83, 1.0];
        let btn_active  = [0.88, 0.83, 0.68, 1.0];

        Some(Self {
            // Main Windows:
            window_bg_color: ui.push_style_color(imgui::StyleColor::WindowBg, bg_color),
            window_title_bg_color_active: ui.push_style_color(imgui::StyleColor::TitleBgActive, bg_color),
            window_title_bg_color_inactive: ui.push_style_color(imgui::StyleColor::TitleBg, bg_color),
            window_title_bg_color_collapsed: ui.push_style_color(imgui::StyleColor::TitleBgCollapsed, bg_color),

            // Text:
            text_color: ui.push_style_color(imgui::StyleColor::Text, Color::black().to_array()),
            text_font: ui.push_font(ui_sys.fonts().game_hud_normal),

            // Buttons:
            button_color: ui.push_style_color(imgui::StyleColor::Button, bg_color),
            button_color_hovered: ui.push_style_color(imgui::StyleColor::ButtonHovered, btn_hovered),
            button_color_active: ui.push_style_color(imgui::StyleColor::ButtonActive, btn_active),

            // Tooltips:
            popup_bg_color: ui.push_style_color(imgui::StyleColor::PopupBg, bg_color),
            popup_border_size: ui.push_style_var(imgui::StyleVar::PopupBorderSize(1.0)), // No border

            // Child Windows:
            child_bg_color: ui.push_style_color(imgui::StyleColor::ChildBg, btn_active),

            // InputText:
            frame_bg_color: ui.push_style_color(imgui::StyleColor::FrameBg, btn_active),
            frame_bg_color_hovered: ui.push_style_color(imgui::StyleColor::FrameBgHovered, btn_hovered),
            frame_bg_color_active: ui.push_style_color(imgui::StyleColor::FrameBgActive, btn_active),

            // Selectable / TreeNode / Collapsing Header:
            header_color: ui.push_style_color(imgui::StyleColor::Header, btn_active),
            header_color_hovered: ui.push_style_color(imgui::StyleColor::HeaderHovered, [0.83, 0.78, 0.62, 1.0]),
            header_color_active: ui.push_style_color(imgui::StyleColor::HeaderActive, btn_active),

            // Scrollbar:
            scrollbar_bg_color: ui.push_style_color(imgui::StyleColor::ScrollbarBg, [0.78, 0.73, 0.60, 1.0]),
            scrollbar_grab_color: ui.push_style_color(imgui::StyleColor::ScrollbarGrab, [0.55, 0.50, 0.38, 1.0]),
            scrollbar_grab_color_hovered: ui.push_style_color(imgui::StyleColor::ScrollbarGrabHovered, [0.62, 0.56, 0.42, 1.0]),
            scrollbar_grab_color_active: ui.push_style_color(imgui::StyleColor::ScrollbarGrabActive, [0.68, 0.61, 0.45, 1.0]),
        })
    }

    #[inline]
    pub fn set_item_spacing(ui_sys: &'ui UiSystem, horizontal: f32, vertical: f32) -> imgui::StyleStackToken<'ui> {
        ui_sys.ui().push_style_var(imgui::StyleVar::ItemSpacing([horizontal, vertical]))
    }
}

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

pub fn draw_centered_button_group(ui: &imgui::Ui,
                                  draw_list: &imgui::DrawListMut<'_>,
                                  labels: &[&str],
                                  size: Option<Size>) -> Option<usize> {
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

    let spacing_y = style.item_spacing[1];

    let total_height =
        labels.len() as f32 * button_size[1] +
        (labels.len().saturating_sub(1) as f32 * spacing_y);

    let avail = ui.content_region_avail();
    let start_x = (avail[0] - button_size[0]) * 0.5;
    let start_y = (avail[1] - total_height) * 0.5;
    ui.set_cursor_pos([0.0, start_y]);

    let mut pressed_index = None;

    for (index, label) in labels.iter().enumerate() {
        let cursor = ui.cursor_pos();
        ui.set_cursor_pos([start_x, cursor[1]]);

        if ui.button_with_size(label, button_size) {
            pressed_index = Some(index);
        }

        draw_last_item_debug_rect(ui, draw_list, Color::blue());
    }

    pressed_index
}

// Draws a vertical separator immediately after the last submitted item.
// Call this *after* an item (Button, Text, Image, etc.) and typically before
// calling `same_line()` again.
pub fn draw_vertical_separator(ui: &imgui::Ui,
                               draw_list: &imgui::DrawListMut<'_>,
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

    draw_list
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

pub fn draw_sprite(ui: &imgui::Ui,
                   draw_list: &imgui::DrawListMut<'_>,
                   id: &str,
                   size: Vec2,
                   texture: UiTextureHandle,
                   tooltip: Option<&str>) {
    ui.invisible_button_flags(id, size.to_array(), imgui::ButtonFlags::empty());
    draw_list.add_image(texture, ui.item_rect_min(), ui.item_rect_max()).build();
    draw_last_item_debug_rect(ui, draw_list, Color::blue());

    if ui.is_item_hovered() && let Some(tooltip) = tooltip {
        ui.tooltip_text(tooltip);
    }
}

// NOTE: This assumes UiStyleTextLabelInvisibleButtons overrides are already set.
pub fn draw_centered_text_label(ui: &imgui::Ui,
                                draw_list: &imgui::DrawListMut<'_>,
                                label: &str,
                                size: Vec2) {
    ui.button_with_size(label, size.to_array());
    draw_last_item_debug_rect(ui, draw_list, Color::green());
}

pub fn spacing(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, size: Vec2) {
    ui.dummy(size.to_array());
    draw_last_item_debug_rect(ui, draw_list, Color::yellow());
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
    //| imgui::WindowFlags::NO_BACKGROUND // Add this back when we switch to a sprite background.
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

pub fn draw_debug_rect(draw_list: &imgui::DrawListMut<'_>, rect: &Rect, color: Color) {
    draw_list.add_rect(rect.min.to_array(),
                       rect.max.to_array(),
                       imgui::ImColor32::from_rgba_f32s(color.r, color.g, color.b, color.a))
                       .build();
}

pub fn draw_last_item_debug_rect(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, color: Color) {
    if !is_debug_draw_enabled() {
        return;
    }

    let rect = Rect::from_extents(
        Vec2::from_array(ui.item_rect_min()),
        Vec2::from_array(ui.item_rect_max())
    );

    draw_debug_rect(draw_list, &rect, color);
}

pub fn draw_current_window_debug_rect(ui: &imgui::Ui) {
    if !is_debug_draw_enabled() {
        return;
    }

    // NOTE: Shrink the rect so it falls within the window bounds,
    // otherwise ImGui would cull it.
    let window_rect = Rect::new(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    ).shrunk(Vec2::new(4.0, 4.0));

    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
}
