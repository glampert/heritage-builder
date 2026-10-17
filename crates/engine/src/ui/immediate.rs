// Immediate-mode UI: the functions the developer and debug UI draws with, re-exported at
// `engine::ui::` (ui::button, ui::text, ui::window, ...). They hide the UI library: only this
// module and the retained widgets (widgets.rs, internal.rs) call it directly. See
// docs/plans/egui-migration.md: the library is ImGui for now, egui after the migration.
//
// - Layout: widgets stack vertically. ui::horizontal lays out the widgets drawn in its closure
//   in a row, ui::indent indents them and ui::grid puts them in a table.
// - Containers that show their contents only while open take a closure: window, overlay, menu,
//   scroll_area, tooltip. Collapsing headers and tab bars return their state instead, and the
//   caller draws the contents after them.
// - Text takes a &str, a String, an ArrayString or format_args!(...), which formats on the stack.
//
// Widgets record themselves in the widget registry (registry.rs), as the retained widgets do, so
// UI automation can list them (ui.tree), click them (ui.click) and set their values (ui.set).
// While the registry is on, each widget records the item it just drew and applies a value ui.set
// queued for it; while it's off, that costs a flag check.
//
// - Items scrolled or clipped out of their window's view are recorded as not visible. ui.set still
//   reaches them. ui.click first asks one to scroll into view (UiSystem::request_widget_reveal),
//   since a click at its center would land on whatever is drawn there instead.
// - Paths: windows, menus, tabs, collapsing_header_scope and path_scope add a segment for the
//   widgets drawn inside them, as the retained menus and labeled groups do.

use std::fmt;

use arrayvec::ArrayString;
use serde_json::Value;

use common::{Color, FieldAccessorXY, Rect, Vec2, format_fixed_string};

use super::{UiFontScale, UiSystem, UiWidgetInfo, widgets::list_index};
use crate::render::texture::TextureHandle;

// ----------------------------------------------
// Text arguments
// ----------------------------------------------

// Text to show: a &str, a String, an ArrayString or format_args!(...). Formatted text up to
// FORMAT_BUFFER_SIZE bytes is formatted on the stack; longer text falls back to a String.
pub trait UiTextArg {
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R;
}

const FORMAT_BUFFER_SIZE: usize = 256;

impl UiTextArg for &str {
    #[inline]
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        f(self)
    }
}

impl UiTextArg for &String {
    #[inline]
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        f(self)
    }
}

impl UiTextArg for String {
    #[inline]
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        f(&self)
    }
}

impl<const N: usize> UiTextArg for ArrayString<N> {
    #[inline]
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        f(&self)
    }
}

impl<const N: usize> UiTextArg for &ArrayString<N> {
    #[inline]
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        f(self)
    }
}

impl UiTextArg for fmt::Arguments<'_> {
    fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> R {
        if let Some(text) = self.as_str() {
            return f(text); // No arguments to format.
        }

        let mut buffer = ArrayString::<FORMAT_BUFFER_SIZE>::new();
        if fmt::write(&mut buffer, self).is_ok() {
            f(&buffer)
        } else {
            f(&self.to_string()) // Too long for the stack buffer.
        }
    }
}

// The text a label shows: ImGui's "label##id" convention keeps the id after "##" out of sight.
#[inline]
pub fn label_text(label: &str) -> &str {
    label.split_once("##").map_or(label, |(text, _)| text)
}

// ----------------------------------------------
// Layout state
// ----------------------------------------------

// How the container being drawn lays out its widgets, one entry per open container.
#[derive(Copy, Clone)]
pub(super) enum UiLayout {
    // One under the other, or however the library lays them out (menu bars).
    Stack,
    // In a row: every widget after the first goes on the same line as the one before.
    Row { items: u32, spacing: Option<f32> },
}

// Every widget calls this first: in a row, it moves the widget onto the line of the one before.
fn begin_item(ui_sys: &UiSystem) {
    let mut layout = ui_sys.layout.borrow_mut();
    if let Some(UiLayout::Row { items, spacing }) = layout.last_mut() {
        if *items > 0 {
            let ui = ui_sys.ui();
            match spacing {
                Some(spacing) => ui.same_line_with_spacing(0.0, *spacing),
                None => ui.same_line(),
            }
        }
        *items += 1;
    }
}

// Runs `contents` with `layout` for the widgets it draws.
fn with_layout<R>(ui_sys: &UiSystem, layout: UiLayout, contents: impl FnOnce(&UiSystem) -> R) -> R {
    ui_sys.layout.borrow_mut().push(layout);
    let result = contents(ui_sys);
    ui_sys.layout.borrow_mut().pop();
    result
}

// ----------------------------------------------
// Recording
// ----------------------------------------------

// Records the last ImGui item. Returns a value ui.set queued for it, if it's enabled.
fn record_item(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    tooltip: Option<&str>,
    enabled: bool,
    value: Option<Value>,
) -> Option<Value> {
    if !ui_sys.is_widget_registry_enabled() {
        return None;
    }

    let ui = ui_sys.ui();
    let rect = Rect::from_extents(Vec2::from_array(ui.item_rect_min()), Vec2::from_array(ui.item_rect_max()));
    let center = rect.center().to_array();
    let visible = ui.is_rect_visible(center, center); // Where ui.click would click.

    let path = ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: tooltip.map(str::to_string),
        rect,
        enabled,
        hovered: ui.is_item_hovered(),
        visible,
        value,
    })?;

    // Scrolls the window to center the item; ImGui applies it next frame.
    if !visible && ui_sys.take_widget_reveal(&path) {
        ui.set_scroll_from_pos_y_with_ratio(center[1] - ui.window_pos()[1], 0.5);
    }

    ui_sys.take_widget_value(&path).filter(|_| enabled)
}

// A widget that isn't the last ImGui item, such as a window.
fn record_rect(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    rect: Rect,
    hovered: bool,
    value: Option<Value>,
) -> Option<String> {
    ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: None,
        rect,
        enabled: true,
        hovered,
        visible: true,
        value,
    })
}

// For buttons drawn some other way, e.g. by the retained widgets: records the last item.
pub(super) fn record_button(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    tooltip: Option<&str>,
    value: Option<Value>,
) {
    record_item(ui_sys, kind, label, tooltip, true, value);
}

// Adds a segment to the paths of the widgets recorded until it's dropped.
#[must_use]
pub struct UiWidgetPathScope<'a> {
    ui_sys: &'a UiSystem,
}

pub fn path_scope<'a>(ui_sys: &'a UiSystem, segment: &str) -> UiWidgetPathScope<'a> {
    ui_sys.push_widget_path(segment);
    UiWidgetPathScope { ui_sys }
}

impl Drop for UiWidgetPathScope<'_> {
    fn drop(&mut self) {
        self.ui_sys.pop_widget_path();
    }
}

// ----------------------------------------------
// Text
// ----------------------------------------------

pub fn text(ui_sys: &UiSystem, text: impl UiTextArg) {
    begin_item(ui_sys);
    text.with_str(|text| ui_sys.ui().text(text));
}

pub fn text_colored(ui_sys: &UiSystem, color: Color, text: impl UiTextArg) {
    begin_item(ui_sys);
    text.with_str(|text| ui_sys.ui().text_colored(color.to_array(), text));
}

pub fn bullet_text(ui_sys: &UiSystem, text: impl UiTextArg) {
    begin_item(ui_sys);
    text.with_str(|text| ui_sys.ui().bullet_text(text));
}

// The size `text` takes with the current font.
pub fn calc_text_size(ui_sys: &UiSystem, text: impl UiTextArg) -> Vec2 {
    text.with_str(|text| Vec2::from_array(ui_sys.ui().calc_text_size(text)))
}

// Draws the text in `contents` at `scale` times the current font size.
pub fn with_font_scale<R>(ui_sys: &UiSystem, scale: f32, contents: impl FnOnce(&UiSystem) -> R) -> R {
    let previous = ui_sys.current_window_front_scale();
    ui_sys.set_window_font_scale(UiFontScale(scale));
    let result = contents(ui_sys);
    ui_sys.set_window_font_scale(previous);
    result
}

// ----------------------------------------------
// Layout
// ----------------------------------------------

pub fn separator(ui_sys: &UiSystem) {
    // A separator ends a row: it always spans the container.
    ui_sys.ui().separator();
}

// A little vertical space.
pub fn spacing(ui_sys: &UiSystem) {
    begin_item(ui_sys);
    ui_sys.ui().spacing();
}

// An empty line.
pub fn new_line(ui_sys: &UiSystem) {
    ui_sys.ui().new_line();
}

// Empty space of `size`.
pub fn dummy(ui_sys: &UiSystem, size: Vec2) {
    begin_item(ui_sys);
    ui_sys.ui().dummy(size.to_array());
}

// Lays out the widgets drawn in `contents` in a row, with the default spacing between them.
pub fn horizontal<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> R {
    row(ui_sys, None, contents)
}

// Lays out the widgets drawn in `contents` in a row, `spacing` apart.
pub fn horizontal_spaced<R>(ui_sys: &UiSystem, spacing: f32, contents: impl FnOnce(&UiSystem) -> R) -> R {
    row(ui_sys, Some(spacing), contents)
}

fn row<R>(ui_sys: &UiSystem, spacing: Option<f32>, contents: impl FnOnce(&UiSystem) -> R) -> R {
    begin_item(ui_sys);
    let group = ui_sys.ui().begin_group();
    let result = with_layout(ui_sys, UiLayout::Row { items: 0, spacing }, contents);
    group.end();
    result
}

// Indents the widgets drawn in `contents` by `amount` points.
pub fn indent<R>(ui_sys: &UiSystem, amount: f32, contents: impl FnOnce(&UiSystem) -> R) -> R {
    let ui = ui_sys.ui();
    ui.indent_by(amount);
    let result = with_layout(ui_sys, UiLayout::Stack, contents);
    ui.unindent_by(amount);
    result
}

// Scopes the ids of the widgets drawn in `contents`, so equal labels under different `id`s don't
// collide.
pub fn id_scope<R>(ui_sys: &UiSystem, id: usize, contents: impl FnOnce(&UiSystem) -> R) -> R {
    let _id = ui_sys.ui().push_id_usize(id);
    contents(ui_sys)
}

// A table of `columns` columns: each cell holds what one UiGrid::cell (or text) draws.
pub fn grid<R>(ui_sys: &UiSystem, id: &str, columns: usize, contents: impl FnOnce(&mut UiGrid) -> R) -> R {
    begin_item(ui_sys);
    let ui = ui_sys.ui();
    ui.columns(columns as i32, id, true);
    let result = with_layout(ui_sys, UiLayout::Stack, |ui_sys| contents(&mut UiGrid { ui_sys }));
    ui.columns(1, "", false);
    result
}

pub struct UiGrid<'a> {
    ui_sys: &'a UiSystem,
}

impl UiGrid<'_> {
    // The next cell, left to right, then on to the next row.
    pub fn cell<R>(&mut self, contents: impl FnOnce(&UiSystem) -> R) -> R {
        let result = contents(self.ui_sys);
        self.ui_sys.ui().next_column();
        result
    }

    pub fn text(&mut self, text: impl UiTextArg) {
        self.cell(|ui_sys| self::text(ui_sys, text));
    }

    // A line under the current row, across all columns.
    pub fn separator(&mut self) {
        self.ui_sys.ui().separator();
    }
}

// ----------------------------------------------
// Windows
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UiCondition {
    Always,   // Every frame.
    FirstUse, // Only the first time the window shows.
}

impl UiCondition {
    fn to_imgui(self) -> imgui::Condition {
        match self {
            Self::Always => imgui::Condition::Always,
            Self::FirstUse => imgui::Condition::FirstUseEver,
        }
    }
}

// A movable window with a title bar. Built with UiWindow::new(title) and its options, drawn with
// ui::window. It records itself in the widget registry, and its contents get its name in their
// paths: "Tile Inspector/Close" for its close button.
pub struct UiWindow<'a> {
    label: &'a str,
    name: Option<&'a str>,
    kind: &'static str,
    value: Option<Value>,
    open: Option<&'a mut bool>,
    position: Option<(Vec2, UiCondition)>,
    size: Option<(Vec2, UiCondition)>,
    collapsed: Option<(bool, UiCondition)>,
    auto_resize: bool,
    resizable: bool,
    scrollbar: bool,
    collapsible: bool,
    menu_bar: bool,
    fit_on_screen: bool,
    item_spacing: Option<Vec2>,
    monospace: bool,
}

impl<'a> UiWindow<'a> {
    // `label`: the title, which is also the window's id: "Title##id" shows "Title", and with "###id"
    // the title can change while the id stays.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            name: None,
            kind: "window",
            value: None,
            open: None,
            position: None,
            size: None,
            collapsed: None,
            auto_resize: false,
            resizable: true,
            scrollbar: true,
            collapsible: true,
            menu_bar: false,
            fit_on_screen: false,
            item_spacing: None,
            monospace: false,
        }
    }

    // The name it records itself with, if not its title, e.g. "Tile Inspector".
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    // Records it as a widget of `kind` ("window", or a kind ui.set takes) with `value`. A value
    // ui.set queues for it is available to its contents, from ui::take_window_set_value.
    pub fn record_as(mut self, kind: &'static str, value: Option<Value>) -> Self {
        self.kind = kind;
        self.value = value;
        self
    }

    // Shows a close button that sets `open` to false.
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    pub fn position(mut self, position: Vec2, condition: UiCondition) -> Self {
        self.position = Some((position, condition));
        self
    }

    pub fn size(mut self, size: Vec2, condition: UiCondition) -> Self {
        self.size = Some((size, condition));
        self
    }

    pub fn collapsed(mut self, collapsed: bool, condition: UiCondition) -> Self {
        self.collapsed = Some((collapsed, condition));
        self
    }

    // Sizes itself to its contents every frame.
    pub fn auto_resize(mut self) -> Self {
        self.auto_resize = true;
        self
    }

    pub fn no_resize(mut self) -> Self {
        self.resizable = false;
        self
    }

    pub fn no_scrollbar(mut self) -> Self {
        self.scrollbar = false;
        self
    }

    pub fn no_collapse(mut self) -> Self {
        self.collapsible = false;
        self
    }

    // Has a menu bar under its title bar, drawn with ui::window_menu_bar.
    pub fn menu_bar(mut self) -> Self {
        self.menu_bar = true;
        self
    }

    // Keeps its bottom on screen: a window taller than the room below its top, such as an
    // auto-resized one opened low on the screen, scrolls its contents instead of running off it.
    pub fn fit_on_screen(mut self) -> Self {
        self.fit_on_screen = true;
        self
    }

    // Space between its widgets.
    pub fn item_spacing(mut self, spacing: Vec2) -> Self {
        self.item_spacing = Some(spacing);
        self
    }

    // Draws its text in the developer UI's monospace font, whatever the current UI theme.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }
}

// Draws `window` with `contents`. Returns what `contents` returned, or None while it's closed or
// collapsed.
pub fn window<R>(ui_sys: &UiSystem, window: UiWindow, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    let ui = ui_sys.ui();

    let UiWindow {
        label,
        name,
        kind,
        value,
        open,
        position,
        size,
        collapsed,
        auto_resize,
        resizable,
        scrollbar,
        collapsible,
        menu_bar,
        fit_on_screen,
        item_spacing,
        monospace,
    } = window;

    let name = name.unwrap_or_else(|| label_text(label));
    let has_close_button = open.is_some();

    let mut flags = imgui::WindowFlags::empty();
    if auto_resize {
        flags |= imgui::WindowFlags::ALWAYS_AUTO_RESIZE;
    }
    if !resizable {
        flags |= imgui::WindowFlags::NO_RESIZE;
    }
    if !scrollbar {
        flags |= imgui::WindowFlags::NO_SCROLLBAR;
    }
    if !collapsible {
        flags |= imgui::WindowFlags::NO_COLLAPSE;
    }

    if fit_on_screen {
        fit_next_window_on_screen();
    }

    let _item_spacing = item_spacing.map(|spacing| ui.push_style_var(imgui::StyleVar::ItemSpacing(spacing.to_array())));
    let _font = monospace.then(|| ui.push_font(ui_sys.fonts().dev_monospace));

    let mut builder = ui.window(label).flags(flags).menu_bar(menu_bar);
    if let Some(open) = open {
        builder = builder.opened(open);
    }
    if let Some((position, condition)) = position {
        builder = builder.position(position.to_array(), condition.to_imgui());
    }
    if let Some((size, condition)) = size {
        builder = builder.size(size.to_array(), condition.to_imgui());
    }
    if let Some((collapsed, condition)) = collapsed {
        builder = builder.collapsed(collapsed, condition.to_imgui());
    }

    builder.build(|| {
        let set_value = record_window(ui_sys, kind, name, value, has_close_button);
        *ui_sys.window_set_value.borrow_mut() = set_value;

        let _path = path_scope(ui_sys, name);
        let result = with_layout(ui_sys, UiLayout::Stack, contents);

        ui_sys.window_set_value.borrow_mut().take();
        result
    })
}

// The value ui.set queued for the window being drawn, if it records itself with a kind ui.set
// takes (UiWindow::record_as).
pub fn take_window_set_value(ui_sys: &UiSystem) -> Option<Value> {
    ui_sys.window_set_value.borrow_mut().take()
}

// Records the current window, from inside its build closure, as a widget of `kind`, and its title
// bar's close button as "<label>/Close" if it has one. Returns a value ui.set queued for the window.
fn record_window(
    ui_sys: &UiSystem,
    kind: &'static str,
    label: &str,
    value: Option<Value>,
    has_close_button: bool,
) -> Option<Value> {
    if !ui_sys.is_widget_registry_enabled() {
        return None;
    }

    let ui = ui_sys.ui();
    let window_rect = Rect::from_pos_and_size(Vec2::from_array(ui.window_pos()), Vec2::from_array(ui.window_size()));
    let path = record_rect(ui_sys, kind, label, window_rect, ui.is_window_hovered(), value)?;

    if has_close_button {
        // Where ImGui 1.89 puts it: at the right end of the title bar (RenderWindowTitleBarContents),
        // a font-sized square plus frame padding (CloseButton).
        let padding = Vec2::from_array(ui_sys.current_ui_style().frame_padding);
        let font_size = ui.current_font_size();
        let position = Vec2::new(window_rect.max.x - font_size - padding.x * 2.0, window_rect.min.y);
        let size = Vec2::new(font_size + padding.x * 2.0, font_size + padding.y * 2.0);

        let rect = Rect::from_pos_and_size(position, size);
        let hovered = ui.is_window_hovered() && rect.contains_point(Vec2::from_array(ui.io().mouse_pos));

        let _path = path_scope(ui_sys, label);
        record_rect(ui_sys, "button", "Close", rect, hovered, None);
    }

    ui_sys.take_widget_value(&path)
}

// Keeps the next window's bottom on screen (UiWindow::fit_on_screen).
fn fit_next_window_on_screen() {
    unsafe extern "C" fn fit_on_screen(data: *mut imgui::sys::ImGuiSizeCallbackData) {
        const MARGIN: f32 = 4.0;
        const MIN_HEIGHT: f32 = 100.0;

        // SAFETY: ImGui passes valid data while it sizes the window, inside a frame.
        unsafe {
            let data = &mut *data;
            let display_height = (*imgui::sys::igGetIO()).DisplaySize.y;
            let max_height = (display_height - data.Pos.y - MARGIN).max(MIN_HEIGHT);
            data.DesiredSize.y = data.DesiredSize.y.min(max_height);
        }
    }

    let min_size = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    let max_size = imgui::sys::ImVec2 { x: f32::MAX, y: f32::MAX };

    // SAFETY: Only sets state for the next Begin.
    unsafe {
        imgui::sys::igSetNextWindowSizeConstraints(min_size, max_size, Some(fit_on_screen), std::ptr::null_mut());
    }
}

// A window without decorations that doesn't take the mouse, for information drawn over the game.
pub struct UiOverlay<'a> {
    label: &'a str,
    position: Vec2,
    bg_alpha: f32,
    bg_color: Option<Color>,
    text_color: Option<Color>,
    font_scale: Option<f32>,
}

impl<'a> UiOverlay<'a> {
    // `label`: its id, unique among the overlays.
    pub fn new(label: &'a str, position: Vec2) -> Self {
        Self { label, position, bg_alpha: 1.0, bg_color: None, text_color: None, font_scale: None }
    }

    // Opacity of its background.
    pub fn bg_alpha(mut self, alpha: f32) -> Self {
        self.bg_alpha = alpha;
        self
    }

    pub fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    pub fn font_scale(mut self, scale: f32) -> Self {
        self.font_scale = Some(scale);
        self
    }
}

pub fn overlay<R>(ui_sys: &UiSystem, overlay: UiOverlay, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    let ui = ui_sys.ui();

    let window_flags = imgui::WindowFlags::NO_DECORATION
        | imgui::WindowFlags::NO_NAV
        | imgui::WindowFlags::NO_MOVE
        | imgui::WindowFlags::NO_MOUSE_INPUTS
        | imgui::WindowFlags::NO_RESIZE
        | imgui::WindowFlags::NO_SCROLLBAR
        | imgui::WindowFlags::NO_COLLAPSE
        | imgui::WindowFlags::NO_SAVED_SETTINGS
        | imgui::WindowFlags::NO_FOCUS_ON_APPEARING
        | imgui::WindowFlags::ALWAYS_AUTO_RESIZE;

    let _bg_color = overlay.bg_color.map(|color| ui.push_style_color(imgui::StyleColor::WindowBg, color.to_array()));
    let _text_color = overlay.text_color.map(|color| ui.push_style_color(imgui::StyleColor::Text, color.to_array()));

    ui.window(overlay.label)
        .position(overlay.position.to_array(), imgui::Condition::Always)
        .flags(window_flags)
        .bg_alpha(overlay.bg_alpha)
        .build(|| match overlay.font_scale {
            Some(scale) => with_font_scale(ui_sys, scale, |ui_sys| with_layout(ui_sys, UiLayout::Stack, contents)),
            None => with_layout(ui_sys, UiLayout::Stack, contents),
        })
}

// A scrolling area inside a window.
pub struct UiScrollArea<'a> {
    id: &'a str,
    height: f32,
    horizontal_scrollbar: bool,
}

impl<'a> UiScrollArea<'a> {
    // Fills the rest of the window.
    pub fn new(id: &'a str) -> Self {
        Self { id, height: 0.0, horizontal_scrollbar: false }
    }

    // Leaves `height` free at the bottom of the window, e.g. for a row of widgets under it.
    pub fn leave_at_bottom(mut self, height: f32) -> Self {
        self.height = -height;
        self
    }

    pub fn horizontal_scrollbar(mut self) -> Self {
        self.horizontal_scrollbar = true;
        self
    }
}

pub fn scroll_area<R>(ui_sys: &UiSystem, area: UiScrollArea, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    begin_item(ui_sys);
    ui_sys
        .ui()
        .child_window(area.id)
        .size([0.0, area.height])
        .horizontal_scrollbar(area.horizontal_scrollbar)
        .build(|| with_layout(ui_sys, UiLayout::Stack, contents))
}

// Inside a scroll area: scrolls it to show its bottom.
pub fn scroll_to_bottom(ui_sys: &UiSystem) {
    ui_sys.ui().set_scroll_here_y_with_ratio(1.0);
}

// ----------------------------------------------
// Tooltips
// ----------------------------------------------

pub fn tooltip<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> R {
    let tooltip = ui_sys.ui().begin_tooltip();
    let result = with_layout(ui_sys, UiLayout::Stack, contents);
    tooltip.end();
    result
}

// A tooltip for the widget drawn last, while it's hovered.
pub fn item_tooltip(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem)) {
    if ui_sys.ui().is_item_hovered() {
        tooltip(ui_sys, contents);
    }
}

pub fn item_tooltip_text(ui_sys: &UiSystem, tooltip_text: impl UiTextArg) {
    if ui_sys.ui().is_item_hovered() {
        tooltip_text.with_str(|tooltip_text| ui_sys.ui().tooltip_text(tooltip_text));
    }
}

// A tooltip with its own font scale and an optional background image.
pub fn custom_tooltip(
    ui_sys: &UiSystem,
    font_scale: UiFontScale,
    background: Option<TextureHandle>,
    contents: impl FnOnce(&UiSystem),
) {
    let ui = ui_sys.ui();
    let tooltip = ui.begin_tooltip();

    // NOTE:
    // Always set font scale for tooltips because a tooltip is itself a window,
    // so it will not inherit the current ui window font scale.
    ui_sys.set_tooltip_font_scale(font_scale);

    if let Some(background) = background {
        let window_rect = Rect::from_pos_and_size(Vec2::from_array(ui.window_pos()), Vec2::from_array(ui.window_size()));
        painter(ui_sys, UiLayer::Window).image(background, window_rect, Color::white());
    }

    with_layout(ui_sys, UiLayout::Stack, contents);
    tooltip.end();

    ui_sys.set_tooltip_font_scale(UiFontScale::default());
}

// ----------------------------------------------
// Menus
// ----------------------------------------------

// The menu bar at the top of the screen. Draw its menus with ui::menu.
pub fn main_menu_bar<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    let _menu_bar = ui_sys.ui().begin_main_menu_bar()?;
    Some(with_layout(ui_sys, UiLayout::Stack, contents))
}

// The menu bar of the window being drawn, which needs UiWindow::menu_bar.
pub fn window_menu_bar<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    let _menu_bar = ui_sys.ui().begin_menu_bar()?;
    Some(with_layout(ui_sys, UiLayout::Stack, contents))
}

// A menu in a menu bar, which shows `contents` while open. The widgets drawn inside it get its
// label in their paths. A menu's items are drawn, and recorded, only while it's open: ui.click
// the menu first.
pub fn menu<R>(ui_sys: &UiSystem, label: &str, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    begin_item(ui_sys);
    let token = ui_sys.ui().begin_menu(label);

    // BeginMenu leaves the menu's own item as the last item, even once its popup has begun. The
    // popup clips to itself, so record_item would call the menu clipped.
    if ui_sys.is_widget_registry_enabled() {
        ui_sys.record_widget("menu", label, None, true, Some(Value::Bool(token.is_some())));
    }

    let _token = token?;
    let _path = path_scope(ui_sys, label);
    Some(with_layout(ui_sys, UiLayout::Stack, contents))
}

// An item in a menu: click it to run it. It closes the menu.
pub fn menu_item(ui_sys: &UiSystem, label: &str) -> bool {
    begin_item(ui_sys);
    let clicked = ui_sys.ui().menu_item(label);
    record_item(ui_sys, "menu_item", label, None, true, None);
    clicked
}

// ----------------------------------------------
// Tabs and collapsing headers
// ----------------------------------------------

// A tab bar with a tab per label: click a tab to select it. Returns the selected tab, whose label
// is in the paths of the widgets drawn after it until the returned value is dropped. Each tab
// records itself (kind "tab", value: whether it's selected).
pub fn tab_bar<'a>(ui_sys: &'a UiSystem, id: &str, labels: &[impl AsRef<str>]) -> Option<UiTab<'a>> {
    begin_item(ui_sys);
    let ui = ui_sys.ui();
    let _tab_bar = ui.tab_bar(id)?;

    let mut selected = None;
    for (index, label) in labels.iter().enumerate() {
        let label = label.as_ref();
        let tab = ui.tab_item(label);
        record_item(ui_sys, "tab", label, None, true, Some(Value::Bool(tab.is_some())));
        if tab.is_some() {
            selected = Some(index);
        }
    }

    // The selected tab's contents draw after the tab bar.
    let index = selected?;
    let label = labels[index].as_ref();
    Some(UiTab { index, _path: path_scope(ui_sys, label), _id: ui.push_id(label) })
}

// The selected tab of a ui::tab_bar. Fields drop in order: the path segment, then the id scope.
pub struct UiTab<'a> {
    pub index: usize,
    _path: UiWidgetPathScope<'a>,
    _id: imgui::IdStackToken<'a>,
}

// A collapsing header: click it to open or close it. Returns whether it's open, and records it
// (kind "header", value: whether it's open). Draw its contents after it, while it's open.
pub fn collapsing_header(ui_sys: &UiSystem, label: &str, default_open: bool) -> bool {
    begin_item(ui_sys);
    let flags = if default_open { imgui::TreeNodeFlags::DEFAULT_OPEN } else { imgui::TreeNodeFlags::empty() };
    let open = ui_sys.ui().collapsing_header(label, flags);
    record_item(ui_sys, "header", label, None, true, Some(Value::Bool(open)));
    open
}

// A collapsing header that, while open, puts its label in the paths of the widgets drawn under it,
// until the returned scope is dropped. None while it's closed.
pub fn collapsing_header_scope<'a>(
    ui_sys: &'a UiSystem,
    label: &str,
    default_open: bool,
) -> Option<UiWidgetPathScope<'a>> {
    collapsing_header(ui_sys, label, default_open).then(|| path_scope(ui_sys, label))
}

// ----------------------------------------------
// Buttons
// ----------------------------------------------

pub fn button(ui_sys: &UiSystem, label: &str) -> bool {
    begin_item(ui_sys);
    let clicked = ui_sys.ui().button(label);
    record_item(ui_sys, "button", label, None, true, None);
    clicked
}

pub fn small_button(ui_sys: &UiSystem, label: &str) -> bool {
    begin_item(ui_sys);
    let clicked = ui_sys.ui().small_button(label);
    record_item(ui_sys, "button", label, None, true, None);
    clicked
}

// A button showing a FontAwesome icon from ui::icons. Automation finds it by its tooltip, e.g. "Undo".
pub fn icon_button(ui_sys: &UiSystem, icon: char, tooltip: Option<&str>) -> bool {
    let clicked = draw_icon_button(ui_sys, icon, tooltip);

    if let Some(tooltip) = tooltip {
        item_tooltip_text(ui_sys, tooltip);
    }

    clicked
}

pub fn icon_button_custom_tooltip(ui_sys: &UiSystem, icon: char, tooltip: impl FnOnce(&UiSystem)) -> bool {
    let clicked = draw_icon_button(ui_sys, icon, None);
    item_tooltip(ui_sys, tooltip);
    clicked
}

fn draw_icon_button(ui_sys: &UiSystem, icon: char, tooltip: Option<&str>) -> bool {
    begin_item(ui_sys);
    let ui = ui_sys.ui();

    let mut icon_utf8 = [0; 4];
    let icon_str = icon.encode_utf8(&mut icon_utf8);

    let icon_font = ui.push_font(ui_sys.fonts().dev_icons);
    let clicked = ui.button(&*icon_str);
    icon_font.pop();

    record_item(ui_sys, "button", tooltip.unwrap_or(icon_str), None, true, None);
    clicked
}

pub struct UiImageButtonParams<'a> {
    pub id: &'a str,
    pub size: Vec2,
    pub texture: TextureHandle,
    pub tooltip: Option<&'a str>,
    pub normal_color: Option<Color>,
    pub hovered_color: Option<Color>,
    pub selected_color: Option<Color>,
    pub tint_color: Option<Color>,
    pub top_left_uvs: Option<Vec2>,
    pub bottom_right_uvs: Option<Vec2>,
    pub selected: bool,
}

impl Default for UiImageButtonParams<'_> {
    fn default() -> Self {
        Self {
            id: "",
            size: Vec2::zero(),
            texture: TextureHandle::invalid(),
            tooltip: None,
            normal_color: None,
            hovered_color: None,
            selected_color: None,
            tint_color: None,
            top_left_uvs: None,
            bottom_right_uvs: None,
            selected: false,
        }
    }
}

// A button showing an image, such as a tile from a texture atlas (`top_left_uvs`/`bottom_right_uvs`
// with V up). It records itself as an "image_button", with whether it's selected as its value.
pub fn image_button(ui_sys: &UiSystem, params: &UiImageButtonParams) -> bool {
    debug_assert!(!params.id.is_empty());
    debug_assert!(params.size != Vec2::zero());
    debug_assert!(params.texture.is_valid());

    fn to_imgui_uvs(uv: Vec2) -> Vec2 {
        Vec2::new(uv.x, 1.0 - uv.y) // Invert Y
    }

    let tint_col = params.tint_color.unwrap_or_default().to_array();
    let bg_col = if params.selected {
        params.selected_color.unwrap_or_default().to_array()
    } else {
        params.normal_color.unwrap_or_default().to_array()
    };

    let top_left_uvs = to_imgui_uvs(params.top_left_uvs.unwrap_or(Vec2::new(0.0, 0.0)));
    let bottom_right_uvs = to_imgui_uvs(params.bottom_right_uvs.unwrap_or(Vec2::new(1.0, 1.0)));

    begin_item(ui_sys);
    let ui = ui_sys.ui();

    // No frame padding.
    let frame_padding = ui.push_style_var(imgui::StyleVar::FramePadding([0.0, 0.0]));

    let clicked = ui
        .image_button_config(params.id, imgui_texture(params.texture), params.size.to_array())
        .background_col(bg_col)
        .tint_col(tint_col)
        .uv0([top_left_uvs.x, bottom_right_uvs.y]) // Swap Ys
        .uv1([bottom_right_uvs.x, top_left_uvs.y])
        .build();

    frame_padding.pop();

    // Its value is whether it's selected, e.g. the tile palette's current tile.
    record_item(ui_sys, "image_button", params.id, params.tooltip, true, Some(Value::Bool(params.selected)));

    if ui.is_item_hovered() {
        if let Some(hovered) = params.hovered_color {
            // NOTE: ImGui hack to highlight a hovered button.
            // Since we only know if the button was hovered *after* it has already
            // been rendered, draw an semi-transparent overlay on top of the button
            // that will be blended to simulate the hovered effect.
            let rect = Rect::from_extents(Vec2::from_array(ui.item_rect_min()), Vec2::from_array(ui.item_rect_max()));
            painter(ui_sys, UiLayer::Window).rect_filled(rect, hovered, 0.0);
        }

        // Show tooltip when hovered:
        if let Some(tooltip) = params.tooltip {
            ui.tooltip_text(tooltip);
        }
    }

    clicked
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UiDPadDirection {
    NE,
    NW,
    SE,
    SW,
}

// Four arrow buttons laid out as a diamond, one per isometric direction.
pub fn dpad_buttons(ui_sys: &UiSystem) -> Option<UiDPadDirection> {
    let ui = ui_sys.ui();
    let arrow_button = |id: &str, direction: imgui::Direction| {
        let pressed = ui.arrow_button(id, direction);
        record_item(ui_sys, "button", id, None, true, None);
        pressed
    };

    let mut direction_pressed = None;

    begin_item(ui_sys);
    let group = ui.begin_group();

    ui.dummy([35.0, 0.0]);
    ui.same_line();
    ui.text("NE");

    ui.dummy([35.0, 0.0]);
    ui.same_line();
    if arrow_button("NE", imgui::Direction::Up) {
        direction_pressed = Some(UiDPadDirection::NE);
    }

    ui.text("NW");
    ui.same_line();
    if arrow_button("NW", imgui::Direction::Left) {
        direction_pressed = Some(UiDPadDirection::NW);
    }

    ui.same_line();
    ui.dummy([13.0, 0.0]);
    ui.same_line();
    if arrow_button("SE", imgui::Direction::Right) {
        direction_pressed = Some(UiDPadDirection::SE);
    }
    ui.same_line();
    ui.text("SE");

    ui.dummy([35.0, 0.0]);
    ui.same_line();
    if arrow_button("SW", imgui::Direction::Down) {
        direction_pressed = Some(UiDPadDirection::SW);
    }
    ui.dummy([35.0, 0.0]);
    ui.same_line();
    ui.text("SW");

    group.end();
    direction_pressed
}

// ----------------------------------------------
// Values
// ----------------------------------------------

pub fn checkbox(ui_sys: &UiSystem, label: &str, value: &mut bool) -> bool {
    begin_item(ui_sys);
    let mut changed = ui_sys.ui().checkbox(label, value);

    if let Some(Value::Bool(set_value)) = record_item(ui_sys, "checkbox", label, None, true, Some(Value::Bool(*value))) {
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

// Shows `value` as a checkbox that doesn't change it.
pub fn read_only_checkbox(ui_sys: &UiSystem, label: &str, value: bool) {
    begin_item(ui_sys);
    let mut shown_value = value;
    ui_sys.ui().checkbox(label, &mut shown_value);
    record_item(ui_sys, "checkbox", label, None, false, Some(Value::Bool(value)));
}

// How a slider or number input shows its number.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum UiNumberFormat {
    #[default]
    Default, // As the number type usually shows.
    Decimals(u8),
}

impl UiNumberFormat {
    fn printf(self) -> Option<ArrayString<8>> {
        match self {
            Self::Default => None,
            Self::Decimals(decimals) => Some(format_fixed_string!(8, "%.{}f", decimals)),
        }
    }
}

mod sealed {
    // The UI library's side of UiNumber, out of sight of the users of this module.
    pub trait UiNumberBackend: imgui::internal::DataTypeKind {}
    impl<T: imgui::internal::DataTypeKind> UiNumberBackend for T {}
}

// Numbers the sliders and inputs edit: types that convert to and from JSON.
pub trait UiNumber: sealed::UiNumberBackend + PartialOrd {
    const INPUT_KIND: &'static str; // How an input of this type records itself: "int_input" or "float_input".

    fn to_json(self) -> Value;
    fn from_json(value: &Value) -> Option<Self>; // None if it's not a number or out of range.
}

macro_rules! impl_ui_int {
    ($($int:ty),*) => {$(
        impl UiNumber for $int {
            const INPUT_KIND: &'static str = "int_input";

            fn to_json(self) -> Value {
                Value::from(self)
            }

            fn from_json(value: &Value) -> Option<Self> {
                let from_i64 = value.as_i64().and_then(|value| Self::try_from(value).ok());
                from_i64.or_else(|| value.as_u64().and_then(|value| Self::try_from(value).ok()))
            }
        }
    )*};
}

macro_rules! impl_ui_float {
    ($($float:ty),*) => {$(
        impl UiNumber for $float {
            const INPUT_KIND: &'static str = "float_input";

            fn to_json(self) -> Value {
                serde_json::json!(self)
            }

            fn from_json(value: &Value) -> Option<Self> {
                value.as_f64().map(|value| value as Self)
            }
        }
    )*};
}

impl_ui_int!(i8, u8, i16, u16, i32, u32, i64, u64, usize);
impl_ui_float!(f32, f64);

pub fn slider<T: UiNumber>(ui_sys: &UiSystem, label: &str, min: T, max: T, format: UiNumberFormat, value: &mut T) -> bool {
    begin_item(ui_sys);
    let slider = ui_sys.ui().slider_config(label, min, max);
    let mut changed = match format.printf() {
        Some(display_format) => slider.display_format(&*display_format).build(value),
        None => slider.build(value),
    };

    let set_value = record_item(ui_sys, "slider", label, None, true, Some(value.to_json()));
    if let Some(set_value) = set_value.as_ref().and_then(T::from_json) {
        let set_value = if set_value < min {
            min
        } else if set_value > max {
            max
        } else {
            set_value
        };
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

// Shows `value` as a slider that doesn't change it.
pub fn read_only_slider<T: UiNumber>(ui_sys: &UiSystem, label: &str, min: T, max: T, format: UiNumberFormat, value: T) {
    begin_item(ui_sys);
    let mut shown_value = value;
    let slider = ui_sys.ui().slider_config(label, min, max);
    match format.printf() {
        Some(display_format) => slider.display_format(&*display_format).build(&mut shown_value),
        None => slider.build(&mut shown_value),
    };
    record_item(ui_sys, "slider", label, None, false, Some(value.to_json()));
}

// A number input of any number type, e.g. a usize index or a u32 count.
pub fn input_scalar<T: UiNumber>(ui_sys: &UiSystem, label: &str, value: &mut T, step: T) -> bool {
    input_number(ui_sys, label, value, step, UiNumberFormat::Default, false)
}

pub fn input_int(ui_sys: &UiSystem, label: &str, value: &mut i32, step: i32, format: UiNumberFormat, read_only: bool) -> bool {
    input_number(ui_sys, label, value, step, format, read_only)
}

fn input_number<T: UiNumber>(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut T,
    step: T,
    format: UiNumberFormat,
    read_only: bool,
) -> bool {
    begin_item(ui_sys);
    let input = ui_sys.ui().input_scalar(label, value).read_only(read_only).step(step);
    let mut changed = match format.printf() {
        Some(display_format) => input.display_format(&*display_format).build(),
        None => input.build(),
    };

    let set_value = record_item(ui_sys, T::INPUT_KIND, label, None, !read_only, Some(value.to_json()));
    if let Some(set_value) = set_value.as_ref().and_then(T::from_json) {
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

pub fn input_float(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut f32,
    step: f32,
    format: UiNumberFormat,
    read_only: bool,
) -> bool {
    begin_item(ui_sys);
    let input = ui_sys.ui().input_float(label, value).read_only(read_only).step(step);
    let mut changed = match format.printf() {
        Some(display_format) => input.display_format(&*display_format).build(),
        None => input.build(),
    };

    if let Some(set_value) = record_item(ui_sys, "float_input", label, None, !read_only, Some(value.to_json()))
        && let Some(set_value) = f32::from_json(&set_value)
    {
        changed |= set_value != *value;
        *value = set_value;
    }

    changed
}

// The input helpers below show the label as text, then their fields indented under it, and record
// the fields under the label without its colon: "Size/Width".

pub fn input_i32(ui_sys: &UiSystem, label: &str, value: &mut i32, read_only: bool, step: Option<i32>) -> bool {
    labeled_fields(ui_sys, label, |ui_sys| {
        let value_label = format_fixed_string!(128, "##_{}_value", label);
        input_int(ui_sys, &value_label, value, step.unwrap_or(1), UiNumberFormat::Default, read_only)
    })
}

pub fn input_i32_xy<T>(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut T,
    read_only: bool,
    steps: Option<[i32; 2]>,
    field_labels: Option<[&str; 2]>,
) -> bool
where
    T: FieldAccessorXY<i32>,
{
    let s = steps.unwrap_or([1, 1]);
    let l = field_labels.unwrap_or(["X", "Y"]);

    labeled_fields(ui_sys, label, |ui_sys| {
        let label_x = format_fixed_string!(128, "{}##_{}_x", l[0], label);
        let edited_x = input_int(ui_sys, &label_x, value.x_mut(), s[0], UiNumberFormat::Default, read_only);
        let label_y = format_fixed_string!(128, "{}##_{}_y", l[1], label);
        let edited_y = input_int(ui_sys, &label_y, value.y_mut(), s[1], UiNumberFormat::Default, read_only);
        edited_x | edited_y
    })
}

pub fn input_f32(ui_sys: &UiSystem, label: &str, value: &mut f32, read_only: bool, step: Option<f32>) -> bool {
    labeled_fields(ui_sys, label, |ui_sys| {
        let value_label = format_fixed_string!(128, "##_{}_value", label);
        input_float(ui_sys, &value_label, value, step.unwrap_or(1.0), UiNumberFormat::Decimals(2), read_only)
    })
}

pub fn input_f32_xy<T>(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut T,
    read_only: bool,
    steps: Option<[f32; 2]>,
    field_labels: Option<[&str; 2]>,
) -> bool
where
    T: FieldAccessorXY<f32>,
{
    let s = steps.unwrap_or([1.0, 1.0]);
    let l = field_labels.unwrap_or(["X", "Y"]);

    labeled_fields(ui_sys, label, |ui_sys| {
        let label_x = format_fixed_string!(128, "{}##_{}_x", l[0], label);
        let edited_x = input_float(ui_sys, &label_x, value.x_mut(), s[0], UiNumberFormat::Decimals(2), read_only);
        let label_y = format_fixed_string!(128, "{}##_{}_y", l[1], label);
        let edited_y = input_float(ui_sys, &label_y, value.y_mut(), s[1], UiNumberFormat::Decimals(2), read_only);
        edited_x | edited_y
    })
}

pub fn input_color(ui_sys: &UiSystem, label: &str, value: &mut Color) -> bool {
    labeled_fields(ui_sys, label, |ui_sys| {
        let mut edited = false;
        for (name, channel) in [("R", &mut value.r), ("G", &mut value.g), ("B", &mut value.b), ("A", &mut value.a)] {
            let channel_label = format_fixed_string!(128, "{}##_{}_{}", name, label, name);
            edited |= slider(ui_sys, &channel_label, 0.0, 1.0, UiNumberFormat::Decimals(2), channel);
        }
        edited
    })
}

fn labeled_fields<R>(ui_sys: &UiSystem, label: &str, fields: impl FnOnce(&UiSystem) -> R) -> R {
    text(ui_sys, label);
    indent(ui_sys, 5.0, |ui_sys| {
        let _path = path_scope(ui_sys, label.trim_end_matches(':'));
        fields(ui_sys)
    })
}

// ui.set takes an item's text or index.
pub fn combo(ui_sys: &UiSystem, label: &str, current_item: &mut usize, items: &[impl AsRef<str>]) -> bool {
    begin_item(ui_sys);
    let mut changed = ui_sys.ui().combo_simple_string(label, current_item, items);

    let current = items.get(*current_item).map_or(Value::Null, |item| Value::String(item.as_ref().to_string()));
    let set_value = record_item(ui_sys, "dropdown", label, None, true, Some(current));
    if let Some(index) = set_value.and_then(|value| list_index(&value, items)) {
        changed |= index != *current_item;
        *current_item = index;
    }

    changed
}

// ----------------------------------------------
// Text input
// ----------------------------------------------

pub fn input_text(ui_sys: &UiSystem, label: &str, value: &mut String, read_only: bool) -> bool {
    let mut input = UiTextEdit::new(label);
    if read_only {
        input = input.read_only();
    }
    text_edit(ui_sys, input, value).changed
}

// A text input with options. Built with UiTextEdit::new(label), drawn with ui::text_edit.
pub struct UiTextEdit<'a> {
    label: &'a str,
    hide_label: bool,
    hint: Option<&'a str>,
    read_only: bool,
    submit_on_enter: bool,
    full_width: bool,
    focus: bool,
    history: Option<UiTextHistory<'a>>,
}

// Earlier entries of a text input, which the up and down arrows step through.
pub struct UiTextHistory<'a> {
    pub entries: &'a [String],
    pub position: &'a mut Option<usize>, // None while editing a new entry.
}

impl<'a> UiTextEdit<'a> {
    // `label`: what it shows next to it and records itself with.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            hide_label: false,
            hint: None,
            read_only: false,
            submit_on_enter: false,
            full_width: false,
            focus: false,
            history: None,
        }
    }

    // Doesn't show its label; it still records itself with it.
    pub fn hide_label(mut self) -> Self {
        self.hide_label = true;
        self
    }

    // Shown while it's empty.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    // Reports Enter (UiTextEditResponse::submitted), as a console's input does.
    pub fn submit_on_enter(mut self) -> Self {
        self.submit_on_enter = true;
        self
    }

    // Takes the width of its container.
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    // Takes keyboard focus this frame.
    pub fn focus(mut self, focus: bool) -> Self {
        self.focus = focus;
        self
    }

    pub fn history(mut self, history: UiTextHistory<'a>) -> Self {
        self.history = Some(history);
        self
    }
}

pub struct UiTextEditResponse {
    pub changed: bool,
    pub submitted: bool, // Enter was pressed in it.
}

pub fn text_edit(ui_sys: &UiSystem, input: UiTextEdit, value: &mut String) -> UiTextEditResponse {
    let UiTextEdit { label, hide_label, hint, read_only, submit_on_enter, full_width, focus, history } = input;

    begin_item(ui_sys);
    let ui = ui_sys.ui();

    if focus {
        ui.set_keyboard_focus_here();
    }
    if full_width {
        ui.set_next_item_width(-1.0);
    }

    let hidden_label;
    let imgui_label = if hide_label {
        hidden_label = format_fixed_string!(128, "##{}", label);
        hidden_label.as_str()
    } else {
        label
    };

    let set_value = if read_only { UiTextValue::default() } else { UiTextValue::take(ui_sys, label_text(label), value) };
    let set = set_value.is_set();

    let mut callbacks = imgui::InputTextCallback::empty();
    if set {
        callbacks |= imgui::InputTextCallback::ALWAYS;
    }
    if history.is_some() {
        callbacks |= imgui::InputTextCallback::HISTORY;
    }

    // An empty hint shows nothing, as no hint would.
    let built = ui
        .input_text(imgui_label, value)
        .hint(hint.unwrap_or_default())
        .read_only(read_only)
        .enter_returns_true(submit_on_enter)
        .callback(callbacks, UiTextEditCallbacks { set_value, history })
        .build();

    // With enter_returns_true, build() reports Enter instead of edits.
    let edited = if submit_on_enter { ui.is_item_edited() } else { built };
    let submitted = submit_on_enter && built;

    record_item(ui_sys, "text_input", label_text(label), None, !read_only, Some(Value::String(value.clone())));

    UiTextEditResponse { changed: set || edited, submitted }
}

// Applies a text ui.set queued for a text input. It's taken before the input draws: while the input
// has keyboard focus, ImGui edits its own copy of the text and copies that back every frame, so a
// value set after drawing would be lost. take() puts it in the input's string, which ImGui shows
// unless the input has focus; then the input's ALWAYS callback replaces ImGui's copy.
#[derive(Default)]
struct UiTextValue {
    text: Option<String>,
}

impl UiTextValue {
    // `label` is the one the input records with.
    fn take(ui_sys: &UiSystem, label: &str, value: &mut String) -> Self {
        let text = match ui_sys.take_next_widget_value("text_input", label) {
            Some(Value::String(text)) => text,
            _ => return Self::default(),
        };
        value.clone_from(&text);
        Self { text: Some(text) }
    }

    fn is_set(&self) -> bool {
        self.text.is_some()
    }
}

// Runs inside the input widget. It mustn't log: the log viewer draws its console input under its
// own lock.
struct UiTextEditCallbacks<'a> {
    set_value: UiTextValue,
    history: Option<UiTextHistory<'a>>,
}

impl imgui::InputTextCallbackHandler for UiTextEditCallbacks<'_> {
    fn on_always(&mut self, mut data: imgui::TextCallbackData) {
        if let Some(text) = self.set_value.text.take() {
            data.clear();
            data.push_str(&text);
        }
    }

    fn on_history(&mut self, direction: imgui::HistoryDirection, mut data: imgui::TextCallbackData) {
        let Some(history) = &mut self.history else {
            return;
        };

        let previous = *history.position;

        *history.position = match (direction, previous) {
            (imgui::HistoryDirection::Up, None) => history.entries.len().checked_sub(1),
            (imgui::HistoryDirection::Up, Some(index)) => Some(index.saturating_sub(1)),
            (imgui::HistoryDirection::Down, Some(index)) if index + 1 < history.entries.len() => Some(index + 1),
            (imgui::HistoryDirection::Down, _) => None,
        };

        if *history.position != previous {
            data.clear();
            if let Some(index) = *history.position {
                data.push_str(&history.entries[index]);
            }
        }
    }
}

// ----------------------------------------------
// Painter
// ----------------------------------------------

// Where a painter draws.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UiLayer {
    Window,     // Over the window being drawn.
    Background, // Under every window.
    Foreground, // Over every window.
}

// Draws shapes, text and images in window points. Get one with ui::painter, and keep it only
// while drawing: one per layer at a time.
pub struct UiPainter<'ui> {
    draw_list: imgui::DrawListMut<'ui>,
}

pub fn painter(ui_sys: &UiSystem, layer: UiLayer) -> UiPainter<'_> {
    let ui = ui_sys.ui();
    let draw_list = match layer {
        UiLayer::Window => ui.get_window_draw_list(),
        UiLayer::Background => ui.get_background_draw_list(),
        UiLayer::Foreground => ui.get_foreground_draw_list(),
    };
    UiPainter { draw_list }
}

impl UiPainter<'_> {
    // A one point wide outline.
    pub fn rect(&self, rect: Rect, color: Color) {
        self.draw_list.add_rect(rect.min.to_array(), rect.max.to_array(), imgui_color(color)).build();
    }

    pub fn rect_filled(&self, rect: Rect, color: Color, rounding: f32) {
        self.draw_list
            .add_rect(rect.min.to_array(), rect.max.to_array(), imgui_color(color))
            .filled(true)
            .rounding(rounding)
            .build();
    }

    // A one point wide outline.
    pub fn circle(&self, center: Vec2, radius: f32, color: Color) {
        self.draw_list.add_circle(center.to_array(), radius, imgui_color(color)).build();
    }

    pub fn circle_filled(&self, center: Vec2, radius: f32, color: Color) {
        self.draw_list.add_circle(center.to_array(), radius, imgui_color(color)).filled(true).build();
    }

    // Text with its top left corner at `position`, in the current font.
    pub fn text(&self, position: Vec2, color: Color, text: impl UiTextArg) {
        text.with_str(|text| self.draw_list.add_text(position.to_array(), imgui_color(color), text));
    }

    // The whole of `texture` stretched over `rect`, multiplied by `tint`.
    pub fn image(&self, texture: TextureHandle, rect: Rect, tint: Color) {
        self.draw_list
            .add_image(imgui_texture(texture), rect.min.to_array(), rect.max.to_array())
            .col(imgui_color(tint))
            .build();
    }

    // `texture` mapped onto a quad, e.g. a rotated one. `uvs` go with `corners`, (0, 0) being the
    // texture's top left.
    pub fn image_quad(&self, texture: TextureHandle, corners: [Vec2; 4], uvs: [Vec2; 4], tint: Color) {
        self.draw_list
            .add_image_quad(
                imgui_texture(texture),
                corners[0].to_array(),
                corners[1].to_array(),
                corners[2].to_array(),
                corners[3].to_array(),
            )
            .uv(uvs[0].to_array(), uvs[1].to_array(), uvs[2].to_array(), uvs[3].to_array())
            .col(imgui_color(tint))
            .build();
    }

    // What `draw` paints is clipped to `clip_rect`.
    pub fn clipped(&self, clip_rect: Rect, draw: impl FnOnce(&Self)) {
        self.draw_list.with_clip_rect(clip_rect.min.to_array(), clip_rect.max.to_array(), || draw(self));
    }
}

#[inline]
fn imgui_color(color: Color) -> imgui::ImColor32 {
    imgui::ImColor32::from_rgba_f32s(color.r, color.g, color.b, color.a)
}

#[inline]
pub(super) fn imgui_texture(texture: TextureHandle) -> imgui::TextureId {
    imgui::TextureId::new(texture.pack())
}

// ----------------------------------------------
// Queries
// ----------------------------------------------

// The size of the window the game draws in, in points.
pub fn screen_size(ui_sys: &UiSystem) -> Vec2 {
    Vec2::from_array(ui_sys.ui().io().display_size)
}

// The height of a framed widget, such as a button, plus the space under it.
pub fn frame_height_with_spacing(ui_sys: &UiSystem) -> f32 {
    ui_sys.ui().frame_height_with_spacing()
}

// The space between widgets.
pub fn item_spacing(ui_sys: &UiSystem) -> Vec2 {
    Vec2::from_array(ui_sys.current_ui_style().item_spacing)
}

// Whether the window being drawn, or a window inside it, has focus.
pub fn is_window_focused(ui_sys: &UiSystem) -> bool {
    ui_sys.ui().is_window_focused_with_flags(imgui::WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS)
}
