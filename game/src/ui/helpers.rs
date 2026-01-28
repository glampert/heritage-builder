use smallvec::SmallVec;
use arrayvec::ArrayString;
use super::{*, widgets::*};

// ----------------------------------------------
// Internal ImGui helpers
// ----------------------------------------------

#[inline]
pub fn base_widget_window_flags() -> imgui::WindowFlags {
    imgui::WindowFlags::ALWAYS_AUTO_RESIZE
    | imgui::WindowFlags::NO_RESIZE
    | imgui::WindowFlags::NO_DECORATION
    | imgui::WindowFlags::NO_SCROLLBAR
    | imgui::WindowFlags::NO_TITLE_BAR
    | imgui::WindowFlags::NO_MOVE
    | imgui::WindowFlags::NO_COLLAPSE
}

#[inline]
pub fn set_next_widget_window_pos(pos: Vec2, pivot: Vec2, cond: imgui::Condition) {
    unsafe {
        imgui::sys::igSetNextWindowPos(
            imgui::sys::ImVec2 { x: pos.x, y: pos.y },
            cond as _,
            imgui::sys::ImVec2 { x: pivot.x, y: pivot.y },
        );
    }
}

#[inline]
pub fn draw_widget_window_background(ui: &imgui::Ui, background: UiTextureHandle) {
    let window_rect = Rect::from_pos_and_size(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    );

    ui.get_window_draw_list()
        .add_image(background, window_rect.min.to_array(), window_rect.max.to_array())
        .build();
}

pub fn draw_centered_text_group(ui: &imgui::Ui,
                                lines: &[String],
                                vertical: bool,
                                horizontal: bool) -> Rect {
    if lines.is_empty() {
        return Rect::zero();
    }

    // Measure text sizes:
    let text_sizes: SmallVec<[[f32; 2]; 16]> = lines
        .iter()
        .map(|s| ui.calc_text_size(s))
        .collect();

    let max_width = text_sizes
        .iter()
        .map(|s| s[0])
        .fold(0.0, f32::max);

    let line_height  = ui.text_line_height_with_spacing();
    let total_height = line_height * lines.len() as f32;

    let region_avail = ui.content_region_avail();
    let cursor_start = ui.cursor_pos();

    // Compute group origin (top-left):
    let start_x = if horizontal { cursor_start[0] + ((region_avail[0] - max_width)    * 0.5) } else { cursor_start[0] };
    let start_y = if vertical   { cursor_start[1] + ((region_avail[1] - total_height) * 0.5) } else { cursor_start[1] };

    // Draw each line:
    for (i, (line, size)) in lines.iter().zip(text_sizes.iter()).enumerate() {
        let x = start_x + (max_width - size[0]) * 0.5;
        let y = start_y + (i as f32 * line_height);

        ui.set_cursor_pos([x, y]);
        ui.text(line);
    }

    // Restore cursor so layout continues correctly.
    ui.set_cursor_pos([cursor_start[0], start_y + total_height]);

    // Return window relative position of group start + group size.
    Rect::from_pos_and_size(Vec2::new(start_x, start_y), Vec2::new(max_width, total_height))
}

pub fn draw_centered_widget_group(ui: &imgui::Ui,
                                  context: &mut UiWidgetContext,
                                  widgets: &mut [UiWidgetImpl],
                                  vertical: bool,
                                  horizontal: bool,
                                  stack_vertically: bool) -> Rect {
    if widgets.is_empty() {
        return Rect::zero();
    }

    let item_spacing = unsafe { Vec2::from_array(ui.style().item_spacing) };

    // Measure widget sizes:
    let widget_sizes: SmallVec<[Vec2; 16]> = widgets
        .iter()
        .map(|widget| widget.measure(context))
        .collect();

    let mut max_width: f32 = 0.0;
    let mut max_height: f32 = 0.0;
    let mut total_size = item_spacing * (widgets.len() - 1) as f32;

    for widget_size in &widget_sizes {
        max_width  = max_width.max(widget_size.x);
        max_height = max_height.max(widget_size.y);
        total_size += *widget_size;
    }

    let region_avail = ui.content_region_avail();
    let cursor_start = ui.cursor_pos();

    let total_width  = if stack_vertically  { max_width  } else { total_size.x };
    let total_height = if !stack_vertically { max_height } else { total_size.y };

    // Compute group origin (top-left):
    let start_x = if horizontal { cursor_start[0] + ((region_avail[0] - total_width)  * 0.5) } else { cursor_start[0] };
    let start_y = if vertical   { cursor_start[1] + ((region_avail[1] - total_height) * 0.5) } else { cursor_start[1] };

    // Draw each widget:
    let mut offset = 0.0;
    for (widget, widget_size) in widgets.iter_mut().zip(widget_sizes.iter()) {
        let widget_pos = {
            if stack_vertically {
                // Stack widgets vertically.
                let x = start_x + (max_width - widget_size.x) * 0.5;
                let y = start_y + offset;
                offset += widget_size.y + item_spacing.y;
                [x, y]
            } else {
                // Position widgets side-by-side.
                let x = start_x + offset;
                offset += widget_size.x + item_spacing.x;
                [x, start_y]
            }
        };

        ui.set_cursor_pos(widget_pos);
        widget.draw(context);
    }

    // Restore cursor so layout continues correctly.
    ui.set_cursor_pos([cursor_start[0], start_y + total_height]);

    // Return window relative position of group start + group size.
    Rect::from_pos_and_size(Vec2::new(start_x, start_y), Vec2::new(total_width, total_height))
}

pub fn draw_centered_labeled_widget_group(ui: &imgui::Ui,
                                          context: &mut UiWidgetContext,
                                          labels_and_widgets: &mut [(String, UiWidgetImpl)],
                                          vertical: bool,
                                          horizontal: bool) -> Rect {
    if labels_and_widgets.is_empty() {
        return Rect::zero();
    }

    let item_spacing = unsafe { Vec2::from_array(ui.style().item_spacing) };
    let mut longest_label: f32 = 0.0;

    // Measure widget sizes:
    let widget_sizes: SmallVec<[Vec2; 16]> = labels_and_widgets
        .iter()
        .map(|(label, widget)| {
            let widget_size = widget.measure(context);
            let label_size  = Vec2::from_array(ui.calc_text_size(label));

            longest_label = longest_label.max(label_size.x);

            let width  = label_size.x + item_spacing.x + widget_size.x;
            let height = label_size.y.max(widget_size.y) + item_spacing.y;
            Vec2::new(width, height)
        }).collect();

    let mut max_width: f32 = 0.0;
    let mut total_height: f32 = 0.0;

    for widget_size in &widget_sizes {
        // NOTE: Both including item spacing.
        max_width = max_width.max(widget_size.x);
        total_height += widget_size.y;
    }

    let region_avail = ui.content_region_avail();
    let cursor_start = ui.cursor_pos();

    // Compute group origin (top-left):
    let start_x = if horizontal { cursor_start[0] + ((region_avail[0] - max_width)    * 0.5) } else { cursor_start[0] };
    let start_y = if vertical   { cursor_start[1] + ((region_avail[1] - total_height) * 0.5) } else { cursor_start[1] };

    // Draw each widget:
    let mut offset_y = 0.0;
    for ((label, widget), widget_size) in labels_and_widgets.iter_mut().zip(widget_sizes.iter()) {
        context.ui_sys.set_font_scale(widget.font_scale());

        let mut x = start_x;
        let y = start_y + offset_y;

        ui.set_cursor_pos([x, y]);
        ui.text(&*label);

        let label_width = ui.item_rect_size()[0];
        let padding = longest_label - label_width;
        x += label_width + item_spacing.x + padding;

        ui.same_line();
        ui.set_cursor_pos([x, y]);
        widget.draw(context);

        offset_y += widget_size.y;
    }

    // Restore cursor so layout continues correctly.
    ui.set_cursor_pos([cursor_start[0], start_y + total_height]);

    // Return window relative position of group start + group size.
    Rect::from_pos_and_size(Vec2::new(start_x, start_y), Vec2::new(max_width, total_height))
}

// Works for most labeled widgets (input text, combo, slider).
pub fn calc_labeled_widget_size(context: &UiWidgetContext, font_scale: UiFontScale, label: &str) -> Vec2 {
    context.ui_sys.set_font_scale(font_scale);
    let ui = context.ui_sys.ui();

    let style = unsafe { ui.style() };
    let height = ui.text_line_height() + (style.frame_padding[1] * 2.0);
    let mut width = ui.calc_item_width();

    if !label.is_empty() {
        let label_size = ui.calc_text_size(label);
        width += style.item_inner_spacing[0] + label_size[0];
    }

    Vec2::new(width, height)
}

// Resolve child window container size.
// requested:
//   > 0.0 -> fixed size
//   = 0.0 -> use remaining host window size
//   < 0.0 -> use remaining host window size minus abs(size)
pub fn calc_child_window_size(requested: [f32; 2], region_avail: [f32; 2]) -> [f32; 2] {
    let mut size = [0.0, 0.0];
    for i in 0..size.len() {
        if requested[i] > 0.0 {
            size[i] = requested[i];
        } else if requested[i] == 0.0 {
            size[i] = region_avail[i];
        } else { // requested < 0.0
            size[i] = (region_avail[i] + requested[i]).max(0.0);
        }
    }
    size
}

pub fn slider_with_left_label<'ui, T>(ui: &'ui imgui::Ui,
                                      label: &str,
                                      min: T,
                                      max: T)
                                      -> (imgui::Slider<'ui, ArrayString<128>, T>, imgui::GroupToken<'ui>)
    where T: imgui::internal::DataTypeKind
{
    // Start a group so the layout behaves as a single item.
    let group = ui.begin_group();

    if label.starts_with('#') {
        // No label given, just generated widget id. Render standalone slider.
        (ui.slider_config(ArrayString::from(label).unwrap(), min, max), group)
    } else {
        // Vertically align text to match framed widgets.
        ui.align_text_to_frame_padding();

        // Draw lef-hand-side label.
        ui.text(label);

        // Same spacing ImGui uses between frame and label.
        let style = unsafe { ui.style() };
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw slider (with hidden label).
        (ui.slider_config(hidden_label, min, max), group)
    }
}

pub fn input_text_with_left_label<'ui, 'p>(ui: &'ui imgui::Ui,
                                           label: &str,
                                           buf: &'p mut String)
                                           -> (imgui::InputText<'ui, 'p, ArrayString<128>>, imgui::GroupToken<'ui>)
{
    // Start a group so the layout behaves as a single item.
    let group = ui.begin_group();

    if label.starts_with('#') {
        // No label given, just generated widget id. Render standalone input.
        (ui.input_text(ArrayString::from(label).unwrap(), buf), group)
    } else {
        // Vertically align text to match framed widgets.
        ui.align_text_to_frame_padding();

        // Draw lef-hand-side label.
        ui.text(label);

        // Same spacing ImGui uses between frame and label.
        let style = unsafe { ui.style() };
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw input (with hidden label).
        (ui.input_text(hidden_label, buf), group)
    }
}

pub fn checkbox_with_left_label<'ui>(ui: &'ui imgui::Ui,
                                     label: &str,
                                     value: &mut bool)
                                     -> (bool, imgui::GroupToken<'ui>)
{
    // Start a group so the layout behaves as a single item.
    let group = ui.begin_group();

    if label.starts_with('#') {
        // No label given, just generated widget id. Render standalone checkbox.
        (ui.checkbox(label, value), group)
    } else {
        // Vertically align text to match framed widgets.
        ui.align_text_to_frame_padding();

        // Draw lef-hand-side label.
        ui.text(label);

        // Same spacing ImGui uses between frame and label.
        let style = unsafe { ui.style() };
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw checkbox (with hidden label).
        (ui.checkbox(hidden_label, value), group)
    }
}

pub fn combo_with_left_label<'ui>(ui: &'ui imgui::Ui,
                                  label: &str,
                                  current_item: &mut usize,
                                  items: &[impl AsRef<str>])
                                  -> (bool, imgui::GroupToken<'ui>)
{
    // Start a group so the layout behaves as a single item.
    let group = ui.begin_group();

    if label.starts_with('#') {
        // No label given, just generated widget id. Render standalone combo list.
        (ui.combo_simple_string(label, current_item, items), group)
    } else {
        // Vertically align text to match framed widgets.
        ui.align_text_to_frame_padding();

        // Draw lef-hand-side label.
        ui.text(label);

        // Same spacing ImGui uses between frame and label.
        let style = unsafe { ui.style() };
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw combo list (with hidden label).
        (ui.combo_simple_string(hidden_label, current_item, items), group)
    }
}

pub fn load_ui_texture(context: &mut UiWidgetContext, path: &str) -> UiTextureHandle {
    let file_path = assets_path().join(path);
    let tex_handle = context.tex_cache.load_texture_with_settings(
        file_path.to_str().unwrap(),
        Some(texture_settings())
    );
    context.ui_sys.to_ui_texture(context.tex_cache, tex_handle)
}
