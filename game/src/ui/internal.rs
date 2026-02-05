use imgui::sys::{ImVec2, ImFont};
use arrayvec::ArrayString;
use smallvec::SmallVec;
use super::{*, widgets::*};

// ----------------------------------------------
// Internal ImGui helpers
// ----------------------------------------------

#[inline]
pub fn push_font(ui: &imgui::Ui, font_handle: UiFontHandle) {
    let font_atlas = ui.fonts();
    let font = font_atlas.get_font(font_handle)
        .expect("push_font(): UI font atlas did not contain the given font!");

    unsafe { imgui::sys::igPushFont(font as *const _ as *mut _); }
}

#[inline]
pub fn pop_font(_ui: &imgui::Ui) {
    unsafe { imgui::sys::igPopFont(); }
}

#[inline]
pub fn font_atlas() -> &'static imgui::FontAtlas {
    unsafe {
        let io_ptr = &*imgui::sys::igGetIO();
        &*(io_ptr.Fonts as *const imgui::FontAtlas)
    }
}

#[inline]
pub fn current_ui_style() -> &'static imgui::Style {
    // NOTE: Bypass imgui::Ui here because we may need to query
    // the current style outside begin_frame/end_frame in some cases.
    // ImGui style is effectively a global setting, so this is safe.
    unsafe { &*(imgui::sys::igGetStyle() as *const imgui::Style) }
}

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
            ImVec2 { x: pos.x, y: pos.y },
            cond as _,
            ImVec2 { x: pivot.x, y: pivot.y },
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

pub fn draw_centered_widget_group(context: &mut UiWidgetContext,
                                  widgets: &mut [UiWidgetImpl],
                                  vertical: bool,
                                  horizontal: bool,
                                  stack_vertically: bool) -> Rect {
    if widgets.is_empty() {
        return Rect::zero();
    }

    let item_spacing = Vec2::from_array(current_ui_style().item_spacing);

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

    let ui = context.ui_sys.ui();

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

pub fn draw_centered_labeled_widget_group(context: &mut UiWidgetContext,
                                          labels_and_widgets: &mut [(String, UiWidgetImpl)],
                                          vertical: bool,
                                          horizontal: bool) -> Rect {
    if labels_and_widgets.is_empty() {
        return Rect::zero();
    }

    let item_spacing = Vec2::from_array(current_ui_style().item_spacing);
    let mut longest_label: f32 = 0.0;

    // Measure widget sizes:
    let widget_sizes: SmallVec<[Vec2; 16]> = labels_and_widgets
        .iter()
        .map(|(label, widget)| {
            let widget_size = widget.measure(context);
            let (label_size, _) = calc_text_size(context, widget.font_scale(), label);

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

    let ui = context.ui_sys.ui();

    let region_avail = ui.content_region_avail();
    let cursor_start = ui.cursor_pos();

    // Compute group origin (top-left):
    let start_x = if horizontal { cursor_start[0] + ((region_avail[0] - max_width)    * 0.5) } else { cursor_start[0] };
    let start_y = if vertical   { cursor_start[1] + ((region_avail[1] - total_height) * 0.5) } else { cursor_start[1] };

    // Draw each widget:
    let mut offset_y = 0.0;
    for ((label, widget), widget_size) in labels_and_widgets.iter_mut().zip(widget_sizes.iter()) {
        context.set_window_font_scale(widget.font_scale());

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
    let style = current_ui_style();

    let height = calc_text_line_height(context, font_scale) + (style.frame_padding[1] * 2.0);
    let mut width = context.ui_sys.ui().calc_item_width();

    if !label.is_empty() {
        let (label_size, _) = calc_text_size(context, font_scale, label);
        width += style.item_inner_spacing[0] + label_size.x;
    }

    Vec2::new(width, height)
}

// Resolve child window container size.
// requested:
//   > 0.0 -> fixed size
//   = 0.0 -> use remaining host window size
//   < 0.0 -> use remaining host window size minus abs(size)
pub fn calc_child_window_size(context: &UiWidgetContext, requested: Vec2) -> Vec2 {
    let region_avail = Vec2::from_array(context.ui_sys.ui().content_region_avail());
    let mut size = Vec2::zero();

    if requested.x > 0.0 {
        size.x = requested.x;
    } else if requested.x == 0.0 {
        size.x = region_avail.x;
    } else { // requested < 0.0
        size.x = (region_avail.x + requested.x).max(0.0);
    }

    if requested.y > 0.0 {
        size.y = requested.y;
    } else if requested.y == 0.0 {
        size.y = region_avail.y;
    } else { // requested < 0.0
        size.y = (region_avail.y + requested.y).max(0.0);
    }

    size
}

// Computes the pre-render size of an ImGui separator.
//  - `horizontal = true`  -> horizontal separator (`ui.separator()`)
//  - `horizontal = false` -> vertical separator (tables / columns)
//  - `thickness` -> ImGui default is 1.0
pub fn calc_separator_size(context: &UiWidgetContext, horizontal: bool, thickness: f32) -> Vec2 {
    let region_avail = Vec2::from_array(context.ui_sys.ui().content_region_avail());
    let style = current_ui_style();

    if horizontal {
        let width  = region_avail.x;
        let height = thickness + (style.item_spacing[1] * 2.0);
        Vec2::new(width, height)
    } else {
        let width  = thickness + (style.item_spacing[0] * 2.0);
        let height = region_avail.y;
        Vec2::new(width, height)
    }
}

pub fn calc_text_line_height(context: &UiWidgetContext, font_scale: UiFontScale) -> f32 {
    let font = context.ui_sys.current_ui_font();
    font.font_size * font_scale.0
}

// Ui/window independent font size calculation, using current font.
// Returns text size and scaled current font size.
pub fn calc_text_size(context: &UiWidgetContext, font_scale: UiFontScale, text: &str) -> (Vec2, f32) {
    let font = context.ui_sys.current_ui_font();
    let font_size = font.font_size * font_scale.0;

    // No text wrapping.
    let max_width  = f32::MAX;
    let wrap_width = -1.0;

    let mut out = ImVec2::zero();

    unsafe {
        let font_ptr = font as *const _ as *mut ImFont;
        let text_start = text.as_ptr();
        let text_end = text_start.add(text.len());
        imgui::sys::ImFont_CalcTextSizeA(
            &mut out,
            font_ptr,
            font_size,
            max_width,
            wrap_width,
            text_start as *const _,
            text_end as *const _,
            std::ptr::null::<i8>() as *mut *const _
        );
    }

    (Vec2::new(out.x, out.y), font_size)
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
        let style = current_ui_style();
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw slider (with hidden label).
        (ui.slider_config(hidden_label, min, max), group)
    }
}

pub fn input_int_with_left_label<'ui, 'p>(ui: &'ui imgui::Ui,
                                          label: &str,
                                          value: &'p mut i32)
                                          -> (imgui::InputScalar<'ui, 'p, i32, ArrayString<128>>, imgui::GroupToken<'ui>)
{
    // Start a group so the layout behaves as a single item.
    let group = ui.begin_group();

    if label.starts_with('#') {
        // No label given, just generated widget id. Render standalone input.
        (ui.input_int(ArrayString::from(label).unwrap(), value), group)
    } else {
        // Vertically align text to match framed widgets.
        ui.align_text_to_frame_padding();

        // Draw lef-hand-side label.
        ui.text(label);

        // Same spacing ImGui uses between frame and label.
        let style = current_ui_style();
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw input (with hidden label).
        (ui.input_int(hidden_label, value), group)
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
        let style = current_ui_style();
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
        let style = current_ui_style();
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
        let style = current_ui_style();
        ui.same_line_with_spacing(0.0, style.item_inner_spacing[0]);

        let mut hidden_label = ArrayString::<128>::new();
        hidden_label.push_str("##");
        hidden_label.push_str(label);

        // Draw combo list (with hidden label).
        (ui.combo_simple_string(hidden_label, current_item, items), group)
    }
}
