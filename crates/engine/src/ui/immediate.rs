// Immediate-mode UI: the functions the developer and debug UI draws with, re-exported at
// `engine::ui::` (ui::button, ui::text, ui::window, ...). They hide egui: only this module and the
// retained widgets (widgets.rs, internal.rs) call it directly.
//
// - Layout: widgets stack vertically. ui::horizontal lays out the widgets drawn in its closure
//   in a row, ui::indent indents them and ui::grid puts them in a table.
// - Containers that show their contents only while open take a closure: window, overlay, menu,
//   scroll_area, tooltip. Collapsing headers and tab bars return their state instead, and the
//   caller draws the contents after them.
// - Text takes a &str, a String, an ArrayString or format_args!(...), which formats on the stack.
//   egui then copies it into a String of its own.
//
// Widgets record themselves in the widget registry (registry.rs), as the retained widgets do, so
// UI automation can list them (ui.tree), click them (ui.click) and set their values (ui.set).
// While the registry is on, each widget records itself and applies a value ui.set queued for it;
// while it's off, that costs a flag check.
//
// - Widgets scrolled or clipped out of their window's view are recorded as not visible. ui.set
//   still reaches them. ui.click first asks one to scroll into view (UiSystem::request_widget_reveal),
//   since a click at its center would land on whatever is drawn there instead.
// - Paths: windows, menus, tabs, collapsing_header_scope and path_scope add a segment for the
//   widgets drawn inside them, as the retained menus and labeled groups do.
//
// Each container draws its contents in a child of the egui Ui it's given (UiSystem::ui_scope), so
// the functions here only ever see the innermost container's Ui.

use std::fmt;

use arrayvec::ArrayString;
use egui::{Align, Align2, Color32, Id, Key, Pos2, RectAlign, Sense, Stroke, StrokeKind, TextStyle, pos2, vec2};
use serde_json::Value;

use common::{Color, FieldAccessorXY, Rect, Vec2, format_fixed_string};

use super::{UiFontScale, UiSystem, UiWidgetInfo, renderer::egui_texture, widgets::list_index};
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

// The text a label shows: the "label##id" convention keeps the id after "##" out of sight.
#[inline]
pub fn label_text(label: &str) -> &str {
    label.split_once("##").map_or(label, |(text, _)| text)
}

// The part of a label that identifies it: all of it, or what follows "###", so the text before can
// change while the id stays.
#[inline]
fn label_id(label: &str) -> &str {
    label.split_once("###").map_or(label, |(_, id)| id)
}

// ----------------------------------------------
// Conversions
// ----------------------------------------------

#[inline]
pub(super) fn color32(color: Color) -> Color32 {
    let to_u8 = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(to_u8(color.r), to_u8(color.g), to_u8(color.b), to_u8(color.a))
}

#[inline]
pub(super) fn to_rect(rect: egui::Rect) -> Rect {
    Rect::from_extents(Vec2::new(rect.min.x, rect.min.y), Vec2::new(rect.max.x, rect.max.y))
}

#[inline]
pub(super) fn egui_rect(rect: Rect) -> egui::Rect {
    egui::Rect::from_min_max(pos2(rect.min.x, rect.min.y), pos2(rect.max.x, rect.max.y))
}

#[inline]
pub(super) fn egui_vec2(v: Vec2) -> egui::Vec2 {
    vec2(v.x, v.y)
}

#[inline]
pub(super) fn egui_pos2(v: Vec2) -> Pos2 {
    pos2(v.x, v.y)
}

// `text` in the current font scale, in `ui`'s text style.
pub(super) fn rich_text(ui_sys: &UiSystem, ui: &egui::Ui, text: &str) -> egui::RichText {
    let rich_text = egui::RichText::new(text);
    let scale = ui_sys.current_window_front_scale().0;
    if scale == 1.0 {
        return rich_text;
    }

    let text_style = ui.style().override_text_style.clone().unwrap_or(TextStyle::Body);
    let mut font = text_style.resolve(ui.style());
    font.size *= scale;
    rich_text.font(font)
}

// ----------------------------------------------
// Recording
// ----------------------------------------------

// The room left in `ui` for a widget that fills it, such as a separator. None in a sizing pass (the
// first frame of a window or menu), so such widgets don't decide its size, as with egui's own.
pub(super) fn fill_size(ui: &mut egui::Ui) -> egui::Vec2 {
    if ui.is_sizing_pass() { egui::Vec2::ZERO } else { ui.available_size_before_wrap().max(egui::Vec2::ZERO) }
}

// Records the widget `response` is from, just drawn into `ui`. Returns a value ui.set queued for
// it, if it's enabled.
pub(super) fn record(
    ui_sys: &UiSystem,
    ui: &egui::Ui,
    response: &egui::Response,
    kind: &'static str,
    label: &str,
    tooltip: Option<&str>,
    enabled: bool,
    value: Option<Value>,
) -> Option<Value> {
    ui_sys.set_last_response(response);

    if !ui_sys.is_widget_registry_enabled() {
        return None;
    }

    let visible = ui.clip_rect().contains(response.rect.center()); // Where ui.click would click.

    let path = ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: tooltip.map(str::to_string),
        rect: to_rect(response.rect),
        enabled,
        hovered: response.hovered(),
        visible,
        value,
    })?;

    if !visible && ui_sys.take_widget_reveal(&path) {
        // Instantly: ui.click waits only a few frames for it, not egui's scroll animation.
        response.scroll_to_me_animation(Some(Align::Center), egui::style::ScrollAnimation::none());
    }

    ui_sys.take_widget_value(&path).filter(|_| enabled)
}

// Records a widget that isn't an egui widget, such as a window, by its rect.
pub(super) fn record_rect(ui_sys: &UiSystem, kind: &'static str, label: &str, rect: egui::Rect, hovered: bool, value: Option<Value>) -> Option<String> {
    ui_sys.record_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: None,
        rect: to_rect(rect),
        enabled: true,
        hovered,
        visible: true,
        value,
    })
}

// Records a widget whose rect is known only once what's inside it has drawn (a menu bar's menu
// and its items, a retail menu and its contents), so it's listed before them; `update_rect` fills
// it in. None while recording is off.
pub(super) fn reserve_rect(ui_sys: &UiSystem, kind: &'static str, label: &str) -> Option<usize> {
    ui_sys.reserve_widget_info(UiWidgetInfo {
        path: String::new(),
        kind,
        label: label.to_string(),
        tooltip: None,
        rect: Rect::default(),
        enabled: true,
        hovered: false,
        visible: true,
        value: None,
    })
}

pub(super) fn update_rect(ui_sys: &UiSystem, index: usize, rect: egui::Rect, hovered: bool, value: Option<Value>) {
    ui_sys.update_widget_info(index, |info| {
        info.rect = to_rect(rect);
        info.hovered = hovered;
        info.value = value;
    });
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
    text.with_str(|text| {
        ui_sys.with_ui(|ui| {
            let text = rich_text(ui_sys, ui, text);
            ui.add(egui::Label::new(text));
        });
    });
}

pub fn text_colored(ui_sys: &UiSystem, color: Color, text: impl UiTextArg) {
    text.with_str(|text| {
        ui_sys.with_ui(|ui| {
            let text = rich_text(ui_sys, ui, text).color(color32(color));
            ui.add(egui::Label::new(text));
        });
    });
}

pub fn bullet_text(ui_sys: &UiSystem, text: impl UiTextArg) {
    text.with_str(|text| {
        ui_sys.with_ui(|ui| {
            ui.horizontal(|ui| {
                let size = ui.text_style_height(&TextStyle::Body);
                let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                let color = ui.visuals().text_color();
                ui.painter().circle_filled(rect.center(), size * 0.15, color);
                let text = rich_text(ui_sys, ui, text);
                ui.add(egui::Label::new(text));
            });
        });
    });
}

// The size `text` takes in the current theme's font, at the current font scale.
pub fn calc_text_size(ui_sys: &UiSystem, text: impl UiTextArg) -> Vec2 {
    let font = ui_sys.text_font();
    text.with_str(|text| {
        let size = ui_sys.ctx().fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font, Color32::WHITE).size());
        Vec2::new(size.x, size.y)
    })
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
    ui_sys.with_ui(|ui| ui.separator());
}

// A little vertical space.
pub fn spacing(ui_sys: &UiSystem) {
    ui_sys.with_ui(|ui| {
        let space = ui.spacing().item_spacing.y;
        ui.add_space(space);
    });
}

// An empty line.
pub fn new_line(ui_sys: &UiSystem) {
    ui_sys.with_ui(|ui| {
        let height = ui.text_style_height(&TextStyle::Body);
        ui.add_space(height);
    });
}

// Empty space of `size`.
pub fn dummy(ui_sys: &UiSystem, size: Vec2) {
    ui_sys.with_ui(|ui| ui.allocate_exact_size(egui_vec2(size), Sense::hover()));
}

// Lays out the widgets drawn in `contents` in a row, with the default spacing between them.
pub fn horizontal<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> R {
    ui_sys.with_parent_ui(|parent| parent.horizontal(|ui| ui_sys.ui_scope(ui, contents)).inner)
}

// Lays out the widgets drawn in `contents` in a row, `spacing` apart.
pub fn horizontal_spaced<R>(ui_sys: &UiSystem, spacing: f32, contents: impl FnOnce(&UiSystem) -> R) -> R {
    ui_sys.with_parent_ui(|parent| {
        parent
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = spacing;
                ui_sys.ui_scope(ui, contents)
            })
            .inner
    })
}

// Indents the widgets drawn in `contents` by `amount` points.
pub fn indent<R>(ui_sys: &UiSystem, amount: f32, contents: impl FnOnce(&UiSystem) -> R) -> R {
    ui_sys.with_parent_ui(|parent| {
        let default_indent = parent.spacing().indent;
        parent.spacing_mut().indent = amount;
        let result = parent
            .indent("indent", |ui| {
                ui.spacing_mut().indent = default_indent;
                ui_sys.ui_scope(ui, contents)
            })
            .inner;
        parent.spacing_mut().indent = default_indent;
        result
    })
}

// Scopes the ids of the widgets drawn in `contents`, so equal labels under different `id`s don't
// collide.
pub fn id_scope<R>(ui_sys: &UiSystem, id: usize, contents: impl FnOnce(&UiSystem) -> R) -> R {
    ui_sys.with_parent_ui(|parent| parent.push_id(id, |ui| ui_sys.ui_scope(ui, contents)).inner)
}

// A table of `columns` columns: each cell holds what one UiGrid::cell (or text) draws.
pub fn grid<R>(ui_sys: &UiSystem, id: &str, columns: usize, contents: impl FnOnce(&mut UiGrid) -> R) -> R {
    debug_assert!(columns > 0);
    ui_sys.with_parent_ui(|parent| {
        egui::Grid::new(id)
            .num_columns(columns)
            .striped(true)
            .show(parent, |grid_ui| contents(&mut UiGrid { ui_sys, grid_ui, columns, cell: 0 }))
            .inner
    })
}

pub struct UiGrid<'a> {
    ui_sys: &'a UiSystem,
    grid_ui: &'a mut egui::Ui,
    columns: usize,
    cell: usize, // In the current row.
}

impl UiGrid<'_> {
    // The next cell, left to right, then on to the next row.
    pub fn cell<R>(&mut self, contents: impl FnOnce(&UiSystem) -> R) -> R {
        let ui_sys = self.ui_sys;
        let result = self.grid_ui.vertical(|ui| ui_sys.ui_scope(ui, contents)).inner;
        self.next_cell();
        result
    }

    pub fn text(&mut self, text: impl UiTextArg) {
        self.cell(|ui_sys| self::text(ui_sys, text));
    }

    // A line under the current row, across all columns.
    pub fn separator(&mut self) {
        if self.cell != 0 {
            self.grid_ui.end_row();
            self.cell = 0;
        }
        for _ in 0..self.columns {
            self.grid_ui.separator();
        }
        self.grid_ui.end_row();
    }

    fn next_cell(&mut self) {
        self.cell += 1;
        if self.cell == self.columns {
            self.grid_ui.end_row();
            self.cell = 0;
        }
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

    // Sizes itself to its contents every frame, up to the bottom of the screen; what doesn't fit
    // scrolls.
    pub fn auto_resize(mut self) -> Self {
        self.auto_resize = true;
        self
    }

    pub fn no_resize(mut self) -> Self {
        self.resizable = false;
        self
    }

    // Doesn't scroll its contents: they fill it, and scroll themselves if they need to (e.g. with
    // ui::scroll_area).
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

// Keeps a window's bottom this far from the screen's.
const WINDOW_SCREEN_MARGIN: f32 = 4.0;

// Draws `window` with `contents`. Returns what `contents` returned, or None while it's closed or
// collapsed.
pub fn window<R>(ui_sys: &UiSystem, window: UiWindow, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
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
        menu_bar: _, // The menu bar draws with the contents.
        fit_on_screen,
        item_spacing,
        monospace,
    } = window;

    if open.as_deref() == Some(&false) {
        return None;
    }

    let title = label_text(label_id_free(label));
    let name = name.unwrap_or(title);
    let id = Id::unique(("ui_window", label_id(label)));

    let mut egui_window = egui::Window::new(title).id(id).title_bar(false).collapsible(false).constrain(false);

    if let Some((position, condition)) = position {
        egui_window = match condition {
            UiCondition::Always => egui_window.current_pos(egui_pos2(position)),
            UiCondition::FirstUse => egui_window.default_pos(egui_pos2(position)),
        };
    }

    if auto_resize {
        egui_window = egui_window.auto_sized();
    } else {
        egui_window = egui_window.resizable(resizable);
        if let Some((size, condition)) = size {
            egui_window = match condition {
                UiCondition::Always => egui_window.fixed_size(egui_vec2(size)),
                UiCondition::FirstUse => egui_window.default_size(egui_vec2(size)),
            };
        }
    }

    let ctx = ui_sys.ctx().clone();
    let collapsed_id = id.with("collapsed");
    let mut is_collapsed = match collapsed {
        Some((collapsed, UiCondition::Always)) => collapsed,
        Some((collapsed, UiCondition::FirstUse)) => ctx.data_mut(|data| *data.get_temp_mut_or(collapsed_id, collapsed)),
        None => ctx.data(|data| data.get_temp(collapsed_id)).unwrap_or(false),
    };

    close_popups_on_open(&ctx, id);

    let has_close_button = open.is_some();
    let mut title_bar_clicks = UiTitleBarClicks::default();

    let shown = egui_window.show(&ctx, |ui| {
        if monospace {
            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
        }
        if let Some(spacing) = item_spacing {
            ui.spacing_mut().item_spacing = egui_vec2(spacing);
        }
        ui_sys.set_window_font_scale(UiFontScale::identity());

        // egui offers an auto-sized window the whole screen's width after its first frame, which
        // widgets that fill their room would take. Offer last frame's width instead.
        let width_id = id.with("content_width");
        if auto_resize
            && !ui.is_sizing_pass()
            && let Some(width) = ui.ctx().data(|data| data.get_temp::<f32>(width_id))
        {
            ui.set_max_width(width);
        }

        // Before its title bar and contents, so it comes first in the registry.
        let window_rect = ui.ctx().memory(|memory| memory.area_rect(id)).unwrap_or_else(|| ui.max_rect());
        let set_value = if ui_sys.is_widget_registry_enabled() {
            let hovered = ui.ctx().rect_contains_pointer(ui.layer_id(), window_rect);
            record_rect(ui_sys, kind, name, window_rect, hovered, value).and_then(|path| ui_sys.take_widget_value(&path))
        } else {
            None
        };
        *ui_sys.window_set_value.borrow_mut() = set_value;

        title_bar_clicks = title_bar(ui_sys, ui, title, name, collapsible, has_close_button, is_collapsed);

        let result = {
            let _path = path_scope(ui_sys, name);

            if is_collapsed {
                None
            } else if scrollbar {
                // Auto-sized windows stop at the bottom of the screen.
                let max_height = if auto_resize || fit_on_screen {
                    // Below the contents: the window frame's margin and stroke, then the screen margin.
                    let frame_bottom = f32::from(ui.spacing().window_margin.bottom) + ui.visuals().window_stroke.width;
                    (ui.ctx().content_rect().max.y - ui.cursor().min.y - frame_bottom - WINDOW_SCREEN_MARGIN).max(100.0)
                } else {
                    f32::INFINITY
                };

                // The scroll area takes its contents' width (it only scrolls vertically), so a
                // resizable window grows to the width of its rows.
                Some(
                    egui::ScrollArea::vertical()
                        .id_salt("window_contents")
                        .auto_shrink([true, auto_resize])
                        .max_height(max_height)
                        .show(ui, |ui| ui_sys.ui_scope(ui, contents))
                        .inner,
                )
            } else {
                Some(ui_sys.ui_scope(ui, contents))
            }
        };

        ui_sys.window_set_value.borrow_mut().take();

        if auto_resize {
            let width = ui.min_rect().width();
            ui.ctx().data_mut(|data| data.insert_temp(width_id, width));
        }
        result
    });

    if title_bar_clicks.collapse {
        is_collapsed = !is_collapsed;
        ctx.data_mut(|data| data.insert_temp(collapsed_id, is_collapsed));
    }
    if title_bar_clicks.close
        && let Some(open) = open
    {
        *open = false;
    }

    shown.and_then(|response| response.inner).flatten()
}

// The label without a "###id" part: the title shown.
#[inline]
fn label_id_free(label: &str) -> &str {
    label.split_once("###").map_or(label, |(text, _)| text)
}

// The value ui.set queued for the window being drawn, if it records itself with a kind ui.set
// takes (UiWindow::record_as).
pub fn take_window_set_value(ui_sys: &UiSystem) -> Option<Value> {
    ui_sys.window_set_value.borrow_mut().take()
}

// A window or menu that opens (wasn't drawn last frame) closes the open menus and combo boxes, as
// if it took the focus.
pub(super) fn close_popups_on_open(ctx: &egui::Context, id: Id) {
    let pass = ctx.cumulative_pass_nr();
    let last_pass_id = id.with("last_pass");
    let last_pass = ctx.data_mut(|data| std::mem::replace(data.get_temp_mut_or(last_pass_id, 0u64), pass));
    if last_pass + 1 != pass {
        egui::Popup::close_all(ctx);
    }
}

// A window's title bar, drawn by ui::window rather than egui so its close button is a widget the
// registry can record (egui's has no stable id). It spans the room the window offers, which is last
// frame's width for an auto-sized window, so it's drawn and recorded before the contents.
#[derive(Default)]
struct UiTitleBarClicks {
    collapse: bool,
    close: bool,
}

fn title_bar(ui_sys: &UiSystem, ui: &mut egui::Ui, title: &str, name: &str, collapsible: bool, closable: bool, collapsed: bool) -> UiTitleBarClicks {
    let font = TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().strong_text_color();
    let title = ui.painter().layout_no_wrap(title.to_owned(), font, text_color);

    let height = ui.spacing().interact_size.y.max(title.size().y);
    let button_size = vec2(height, height);
    let button_width = height + ui.spacing().item_spacing.x;
    let min_width = title.size().x + button_width * (u32::from(collapsible) + u32::from(closable)) as f32;

    // Takes only its minimum width, so it doesn't widen the window.
    let (min_row, _) = ui.allocate_exact_size(vec2(min_width, height), Sense::hover());
    let width = if ui.is_sizing_pass() { min_width } else { ui.max_rect().width().max(min_width) };
    let row = egui::Rect::from_min_size(min_row.min, vec2(width, height));
    ui.separator();

    let mut clicks = UiTitleBarClicks::default();
    let mut title_left = row.left();

    if collapsible {
        let rect = egui::Rect::from_min_size(row.min, button_size);
        let response = ui.interact(rect, ui.scope_id().with("collapse"), Sense::click());
        let color = ui.style().interact(&response).fg_stroke.color;
        let rotation = if collapsed { -std::f32::consts::FRAC_PI_2 } else { 0.0 };
        ui.painter().add(egui::Shape::rotated_triangle(rect.shrink(rect.height() * 0.3), rotation, color));
        clicks.collapse = response.clicked();
        title_left = rect.right() + ui.spacing().item_spacing.x * 0.5;
    }

    let title_pos = pos2(title_left, row.center().y - title.size().y * 0.5);
    ui.painter().galley(title_pos, title, text_color);

    if closable {
        let rect = egui::Rect::from_min_size(pos2(row.right() - button_size.x, row.top()), button_size);
        let response = ui.interact(rect, ui.scope_id().with("close"), Sense::click());
        let visuals = ui.style().interact(&response);
        let cross = rect.shrink(rect.height() * 0.3);
        let stroke = Stroke::new(1.5, visuals.fg_stroke.color);
        if response.hovered() {
            ui.painter().rect_filled(rect.shrink(1.0), visuals.corner_radius, visuals.weak_bg_fill);
        }
        ui.painter().line_segment([cross.left_top(), cross.right_bottom()], stroke);
        ui.painter().line_segment([cross.right_top(), cross.left_bottom()], stroke);
        clicks.close = response.clicked();

        let _path = path_scope(ui_sys, name);
        record(ui_sys, ui, &response, "button", "Close", None, true, None);
    }

    clicks
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
    let ctx = ui_sys.ctx().clone();
    let style = ui_sys.egui_style().clone();

    let fill = match overlay.bg_color {
        Some(color) => color32(color),
        None => style.visuals.window_fill,
    };
    let fill = fill.gamma_multiply(overlay.bg_alpha);
    let frame = egui::Frame::window(&style).fill(fill).stroke(Stroke::NONE).shadow(egui::Shadow::NONE);

    let response = egui::Area::new(Id::unique(("ui_overlay", overlay.label)))
        .order(egui::Order::Foreground)
        .fixed_pos(egui_pos2(overlay.position))
        .interactable(false)
        .constrain(false)
        .show(&ctx, |ui| {
            frame
                .show(ui, |ui| {
                    if let Some(color) = overlay.text_color {
                        ui.visuals_mut().override_text_color = Some(color32(color));
                    }
                    match overlay.font_scale {
                        Some(scale) => with_font_scale(ui_sys, scale, |ui_sys| ui_sys.ui_scope(ui, contents)),
                        None => ui_sys.ui_scope(ui, contents),
                    }
                })
                .inner
        });

    Some(response.inner)
}

// A scrolling area inside a window.
pub struct UiScrollArea<'a> {
    id: &'a str,
    leave_at_bottom: f32,
    horizontal_scrollbar: bool,
}

impl<'a> UiScrollArea<'a> {
    // Fills the rest of the window.
    pub fn new(id: &'a str) -> Self {
        Self { id, leave_at_bottom: 0.0, horizontal_scrollbar: false }
    }

    // Leaves `height` free at the bottom of the window, e.g. for a row of widgets under it.
    pub fn leave_at_bottom(mut self, height: f32) -> Self {
        self.leave_at_bottom = height;
        self
    }

    pub fn horizontal_scrollbar(mut self) -> Self {
        self.horizontal_scrollbar = true;
        self
    }
}

pub fn scroll_area<R>(ui_sys: &UiSystem, area: UiScrollArea, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    ui_sys.with_parent_ui(|parent| {
        let height = (parent.available_height() - area.leave_at_bottom).max(0.0);
        let scroll_area = if area.horizontal_scrollbar { egui::ScrollArea::both() } else { egui::ScrollArea::vertical() };

        // It takes all the room (auto_shrink off), bars included, so it never grows its window.
        let response = scroll_area
            .id_salt(area.id)
            .auto_shrink([false, false])
            .max_height(height)
            .show(parent, |ui| ui_sys.ui_scope(ui, contents));

        Some(response.inner)
    })
}

// Inside a scroll area: scrolls it to show its bottom.
pub fn scroll_to_bottom(ui_sys: &UiSystem) {
    ui_sys.with_ui(|ui| ui.scroll_to_cursor(Some(Align::BOTTOM)));
}

// ----------------------------------------------
// Tooltips
// ----------------------------------------------

// Where a tooltip shows.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum UiTooltipPlacement {
    #[default]
    AtPointer,    // Next to the mouse pointer.
    BesideWidget, // Under or beside the widget it's for, where it doesn't cover it.
}

// A tooltip at the pointer, not tied to a widget: shows while this is called.
pub fn tooltip<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> R {
    let (layer_id, widget_id) = ui_sys.with_ui(|ui| (ui.layer_id(), ui.scope_id().with("tooltip")));
    let tooltip = egui::Tooltip::always_open(ui_sys.ctx().clone(), layer_id, widget_id, egui::PopupAnchor::Pointer);

    let mut result = None;
    tooltip.show(|ui| result = Some(ui_sys.ui_scope(ui, contents)));
    result.expect("An always open tooltip shows")
}

// A tooltip for the widget drawn last, while it's hovered.
pub fn item_tooltip(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem)) {
    item_tooltip_placed(ui_sys, UiTooltipPlacement::AtPointer, contents);
}

pub fn item_tooltip_placed(ui_sys: &UiSystem, placement: UiTooltipPlacement, contents: impl FnOnce(&UiSystem)) {
    let Some(response) = ui_sys.last_response() else {
        return;
    };

    placed_tooltip(&response, placement).show(|ui| ui_sys.ui_scope(ui, contents));
}

// Where a tooltip beside its widget goes when the widget's left has no room for it.
const BESIDE_WIDGET_ALIGNS: [RectAlign; 4] = [RectAlign::RIGHT_START, RectAlign::LEFT_END, RectAlign::RIGHT_END, RectAlign::TOP_START];

// A tooltip for `response`'s widget, shown while it's hovered, at `placement`.
fn placed_tooltip(response: &egui::Response, placement: UiTooltipPlacement) -> egui::Tooltip<'_> {
    let mut tooltip = egui::Tooltip::for_enabled(response);
    match placement {
        UiTooltipPlacement::AtPointer => tooltip = tooltip.at_pointer().gap(12.0),
        UiTooltipPlacement::BesideWidget => {
            // To the widget's left, or wherever else it fits: never over the widget or its neighbors.
            tooltip.popup = tooltip.popup.align(RectAlign::LEFT_START).align_alternatives(&BESIDE_WIDGET_ALIGNS).gap(8.0);
        }
    }
    tooltip
}

pub fn item_tooltip_text(ui_sys: &UiSystem, tooltip_text: impl UiTextArg) {
    tooltip_text.with_str(|tooltip_text| item_tooltip(ui_sys, |ui_sys| text(ui_sys, tooltip_text)));
}

// A tooltip for the widget drawn last, with its own font scale and an optional background image.
pub fn custom_tooltip(
    ui_sys: &UiSystem,
    font_scale: UiFontScale,
    background: Option<TextureHandle>,
    placement: UiTooltipPlacement,
    contents: impl FnOnce(&UiSystem),
) {
    let Some(response) = ui_sys.last_response() else {
        return;
    };

    let tooltip = placed_tooltip(&response, placement);

    let previous_scale = ui_sys.current_window_front_scale();
    ui_sys.set_tooltip_font_scale(font_scale);

    tooltip.show(|ui| {
        // The background goes in a slot under the contents, filled once their size is known.
        let background_slot = background.map(|_| ui.painter().add(egui::Shape::Noop));
        ui_sys.ui_scope(ui, contents);

        if let (Some(slot), Some(texture)) = (background_slot, background) {
            let rect = ui.min_rect().expand2(ui.spacing().window_margin.left_top()); // Its frame's.
            let image = egui::Shape::image(egui_texture(texture), rect, uv_full(), Color32::WHITE);
            ui.painter().set(slot, image);
        }
    });

    ui_sys.set_tooltip_font_scale(previous_scale);
}

#[inline]
pub(super) fn uv_full() -> egui::Rect {
    egui::Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
}

// ----------------------------------------------
// Menus
// ----------------------------------------------

// For menu bars: their buttons are drawn without frames.
fn menu_config() -> egui::containers::menu::MenuConfig {
    // Clicks on checkboxes and buttons in a menu leave it open; menu_item closes it.
    egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

// For a menu's contents: its widgets look as they do in windows.
fn menu_popup_config() -> egui::containers::menu::MenuConfig {
    menu_config().style(egui::style::StyleModifier::default())
}

// The menu bar at the top of the screen. Draw its menus with ui::menu.
pub fn main_menu_bar<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    let ctx = ui_sys.ctx().clone();
    let result = ctx.root_ui(|root| {
        egui::Panel::top("ui_main_menu_bar")
            .show(root, |ui| {
                egui::containers::menu::MenuBar::new().config(menu_config()).ui(ui, |ui| ui_sys.ui_scope(ui, contents)).inner
            })
            .inner
    });
    Some(result)
}

// The menu bar of the window being drawn, which needs UiWindow::menu_bar.
pub fn window_menu_bar<R>(ui_sys: &UiSystem, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    ui_sys.with_parent_ui(|parent| {
        let result = egui::containers::menu::MenuBar::new()
            .config(menu_config())
            .ui(parent, |ui| ui_sys.ui_scope(ui, contents))
            .inner;
        parent.separator();
        Some(result)
    })
}

// A menu in a menu bar, which shows `contents` while open. The widgets drawn inside it get its
// label in their paths. A menu's items are drawn, and recorded, only while it's open: ui.click
// the menu first.
pub fn menu<R>(ui_sys: &UiSystem, label: &str, contents: impl FnOnce(&UiSystem) -> R) -> Option<R> {
    // Listed before its items, though its rect is known only after they're drawn.
    let record_index = reserve_rect(ui_sys, "menu", label);

    let (response, result) = ui_sys.with_parent_ui(|parent| {
        // The menu opens below its button and scrolls if it's taller than the room left there.
        // egui would move it up over the button instead. The button's bottom is last frame's.
        let bottom_id = parent.make_persistent_id(("ui_menu_bottom", label));
        let button_bottom = parent
            .ctx()
            .data(|data| data.get_temp::<f32>(bottom_id))
            .unwrap_or_else(|| parent.cursor().min.y + parent.spacing().interact_size.y);

        let (response, inner) = egui::containers::menu::MenuButton::new(label_text(label)).config(menu_popup_config()).ui(parent, |ui| {
            let margins = ui.style().spacing.menu_margin.sum().y + ui.style().spacing.menu_spacing;
            let max_height = (ui.ctx().content_rect().bottom() - button_bottom - margins).max(0.0);
            ui.set_max_height(max_height); // A new popup's room is egui's default area size.
            egui::ScrollArea::vertical().id_salt(label).max_height(max_height).show(ui, |ui| {
                let _path = path_scope(ui_sys, label);
                ui_sys.ui_scope(ui, contents)
            })
        });

        parent.ctx().data_mut(|data| data.insert_temp(bottom_id, response.rect.bottom()));
        (response, inner.map(|inner| inner.inner.inner))
    });

    ui_sys.set_last_response(&response);
    if let Some(index) = record_index {
        update_rect(ui_sys, index, response.rect, response.hovered(), Some(Value::Bool(result.is_some())));
    }

    result
}

// An item in a menu: click it to run it. It closes the menu.
pub fn menu_item(ui_sys: &UiSystem, label: &str) -> bool {
    ui_sys.with_ui(|ui| {
        // A row rather than a button.
        let response = ui.add(egui::Button::new(label_text(label)).frame_when_inactive(false));
        record(ui_sys, ui, &response, "menu_item", label, None, true, None);
        if response.clicked() {
            ui.close();
        }
        response.clicked()
    })
}

// ----------------------------------------------
// Tabs and collapsing headers
// ----------------------------------------------

// A tab bar with a tab per label: click a tab to select it. Returns the selected tab, whose label
// is in the paths of the widgets drawn after it until the returned value is dropped. Each tab
// records itself (kind "tab", value: whether it's selected).
pub fn tab_bar<'a>(ui_sys: &'a UiSystem, id: &str, labels: &[impl AsRef<str>]) -> Option<UiTab<'a>> {
    if labels.is_empty() {
        return None;
    }

    let index = ui_sys.with_ui(|ui| {
        let bar_id = ui.make_persistent_id(("ui_tab_bar", id));
        let mut selected = ui.ctx().data_mut(|data| *data.get_temp_mut_or(bar_id, 0usize)).min(labels.len() - 1);

        // On one row when the screen has room for it: the window grows to its rows (ui::window).
        // Wrapped on a screen too narrow for it, and until the window has grown.
        let font = TextStyle::Button.resolve(ui.style());
        let tab_padding = ui.spacing().button_padding.x * 2.0;
        let labels_width: f32 = labels
            .iter()
            .map(|label| ui.painter().layout_no_wrap(label_text(label.as_ref()).to_owned(), font.clone(), Color32::PLACEHOLDER).size().x + tab_padding)
            .sum();
        let row_width = labels_width + ui.spacing().item_spacing.x * (labels.len() - 1) as f32;
        let frame_right = f32::from(ui.spacing().window_margin.right) + ui.visuals().window_stroke.width;
        let room = ui.ctx().content_rect().max.x - ui.cursor().left() - frame_right - WINDOW_SCREEN_MARGIN;
        ui.set_min_width(row_width.min(room));

        ui.horizontal_wrapped(|ui| {
            for (index, label) in labels.iter().enumerate() {
                let label = label.as_ref();
                let response = ui.selectable_label(index == selected, label_text(label));
                record(ui_sys, ui, &response, "tab", label, None, true, Some(Value::Bool(index == selected)));
                if response.clicked() {
                    selected = index;
                }
            }
        });
        ui.separator();

        ui.ctx().data_mut(|data| data.insert_temp(bar_id, selected));
        selected
    });

    // The selected tab's contents draw after the tab bar.
    Some(UiTab { index, _path: path_scope(ui_sys, labels[index].as_ref()) })
}

// The selected tab of a ui::tab_bar.
pub struct UiTab<'a> {
    pub index: usize,
    _path: UiWidgetPathScope<'a>,
}

// A collapsing header: click it to open or close it. Returns whether it's open, and records it
// (kind "header", value: whether it's open). Draw its contents after it, while it's open.
pub fn collapsing_header(ui_sys: &UiSystem, label: &str, default_open: bool) -> bool {
    ui_sys.with_ui(|ui| {
        let id = ui.make_persistent_id(("ui_header", label));
        let mut open = ui.ctx().data_mut(|data| *data.get_temp_mut_or(id, default_open));

        let font = TextStyle::Body.resolve(ui.style());
        let galley = ui.painter().layout_no_wrap(label_text(label).to_owned(), font, Color32::PLACEHOLDER);

        let padding = ui.spacing().button_padding;
        let icon_width = ui.spacing().icon_width;
        let height = ui.spacing().interact_size.y.max(galley.size().y + padding.y * 2.0);
        let width = fill_size(ui).x.max(icon_width + galley.size().x + padding.x * 3.0);

        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        if response.clicked() {
            open = !open;
            ui.ctx().data_mut(|data| data.insert_temp(id, open));
        }

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, open);
            ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);

            let icon_rect = egui::Rect::from_center_size(
                pos2(rect.left() + padding.x + icon_width * 0.5, rect.center().y),
                vec2(icon_width, icon_width) * 0.6,
            );
            let rotation = if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 };
            ui.painter().add(egui::Shape::rotated_triangle(icon_rect, rotation, visuals.text_color()));

            let text_pos = pos2(icon_rect.right() + padding.x, rect.center().y - galley.size().y * 0.5);
            ui.painter().galley(text_pos, galley, visuals.text_color());
        }

        if let Some(Value::Bool(set_open)) = record(ui_sys, ui, &response, "header", label, None, true, Some(Value::Bool(open))) {
            open = set_open;
            ui.ctx().data_mut(|data| data.insert_temp(id, open));
        }

        open
    })
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
    ui_sys.with_ui(|ui| {
        let response = ui.button(rich_text(ui_sys, ui, label_text(label)));
        record(ui_sys, ui, &response, "button", label, None, true, None);
        response.clicked()
    })
}

pub fn small_button(ui_sys: &UiSystem, label: &str) -> bool {
    ui_sys.with_ui(|ui| {
        let response = ui.small_button(label_text(label));
        record(ui_sys, ui, &response, "button", label, None, true, None);
        response.clicked()
    })
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
    let mut icon_utf8 = [0; 4];
    let icon_str = icon.encode_utf8(&mut icon_utf8);
    let icon_font = ui_sys.icon_font();

    ui_sys.with_ui(|ui| {
        let response = ui.button(egui::RichText::new(&*icon_str).font(icon_font));
        record(ui_sys, ui, &response, "button", tooltip.unwrap_or(icon_str), None, true, None);
        response.clicked()
    })
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

    // V up to V down: the UVs at the rect's top left corner, then at its bottom right.
    let top_left = params.top_left_uvs.unwrap_or(Vec2::new(0.0, 0.0));
    let bottom_right = params.bottom_right_uvs.unwrap_or(Vec2::new(1.0, 1.0));
    let uv = egui::Rect::from_min_max(pos2(top_left.x, 1.0 - bottom_right.y), pos2(bottom_right.x, 1.0 - top_left.y));

    let background = if params.selected { params.selected_color } else { params.normal_color };
    let background = color32(background.unwrap_or_default()); // White by default.
    let tint = color32(params.tint_color.unwrap_or_default());

    ui_sys.with_ui(|ui| {
        let id = ui.make_persistent_id(("ui_image_button", params.id));
        let (rect, _) = ui.allocate_exact_size(egui_vec2(params.size), Sense::hover());
        let response = ui.interact(rect, id, Sense::click());

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 0.0, background);
            ui.painter().image(egui_texture(params.texture), rect, uv, tint);

            if response.hovered()
                && let Some(hovered) = params.hovered_color
            {
                ui.painter().rect_filled(rect, 0.0, color32(hovered));
            }
        }

        // Its value is whether it's selected, e.g. the tile palette's current tile.
        record(ui_sys, ui, &response, "image_button", params.id, params.tooltip, true, Some(Value::Bool(params.selected)));

        let clicked = response.clicked();
        if let Some(tooltip) = params.tooltip {
            response.on_hover_ui_at_pointer(|ui| {
                ui.label(tooltip);
            });
        }
        clicked
    })
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
    let mut direction_pressed = None;

    let arrow_button = |ui: &mut egui::Ui, id: &str, rotation: f32| {
        let size = ui.spacing().interact_size.y;
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        let visuals = ui.style().interact(&response);
        ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);
        ui.painter().add(egui::Shape::rotated_triangle(rect.shrink(size * 0.3), rotation, visuals.text_color()));
        record(ui_sys, ui, &response, "button", id, None, true, None);
        response.clicked()
    };

    use std::f32::consts::{FRAC_PI_2, PI};
    let (up, right, down, left) = (-FRAC_PI_2, 0.0, FRAC_PI_2, PI);

    ui_sys.with_ui(|ui| {
        ui.vertical(|ui| {
            let spacing = ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
            ui.horizontal(|ui| {
                ui.add_space(spacing);
                ui.label("NE");
            });
            ui.horizontal(|ui| {
                ui.add_space(spacing);
                if arrow_button(ui, "NE", up) {
                    direction_pressed = Some(UiDPadDirection::NE);
                }
            });
            ui.horizontal(|ui| {
                ui.label("NW");
                if arrow_button(ui, "NW", left) {
                    direction_pressed = Some(UiDPadDirection::NW);
                }
                ui.add_space(spacing * 0.5);
                if arrow_button(ui, "SE", right) {
                    direction_pressed = Some(UiDPadDirection::SE);
                }
                ui.label("SE");
            });
            ui.horizontal(|ui| {
                ui.add_space(spacing);
                if arrow_button(ui, "SW", down) {
                    direction_pressed = Some(UiDPadDirection::SW);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(spacing);
                ui.label("SW");
            });
        });
    });

    direction_pressed
}

// ----------------------------------------------
// Values
// ----------------------------------------------

pub fn checkbox(ui_sys: &UiSystem, label: &str, value: &mut bool) -> bool {
    ui_sys.with_ui(|ui| {
        let response = ui.checkbox(value, rich_text(ui_sys, ui, label_text(label)));
        let mut changed = response.changed();

        if let Some(Value::Bool(set_value)) = record(ui_sys, ui, &response, "checkbox", label, None, true, Some(Value::Bool(*value))) {
            changed |= set_value != *value;
            *value = set_value;
        }

        changed
    })
}

// Shows `value` as a checkbox that doesn't change it.
pub fn read_only_checkbox(ui_sys: &UiSystem, label: &str, value: bool) {
    ui_sys.with_ui(|ui| {
        let mut shown_value = value;
        let response = ui.checkbox(&mut shown_value, rich_text(ui_sys, ui, label_text(label)));
        record(ui_sys, ui, &response, "checkbox", label, None, false, Some(Value::Bool(value)));
    });
}

// How a slider or number input shows its number.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum UiNumberFormat {
    #[default]
    Default, // As the number type usually shows.
    Decimals(u8),
}

mod sealed {
    // The UI library's side of UiNumber, out of sight of the users of this module.
    pub trait UiNumberBackend: egui::emath::Numeric {}
    impl<T: egui::emath::Numeric> UiNumberBackend for T {}
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

fn apply_format(slider: egui::Slider<'_>, format: UiNumberFormat) -> egui::Slider<'_> {
    match format {
        UiNumberFormat::Default => slider,
        UiNumberFormat::Decimals(decimals) => slider.fixed_decimals(usize::from(decimals)),
    }
}

pub fn slider<T: UiNumber>(ui_sys: &UiSystem, label: &str, min: T, max: T, format: UiNumberFormat, value: &mut T) -> bool {
    ui_sys.with_ui(|ui| {
        let slider = egui::Slider::new(value, min..=max)
            .text(rich_text(ui_sys, ui, label_text(label)))
            .clamping(egui::SliderClamping::Always);
        let response = ui.add(apply_format(slider, format));
        let mut changed = response.changed();

        let set_value = record(ui_sys, ui, &response, "slider", label, None, true, Some(value.to_json()));
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
    })
}

// Shows `value` as a slider that doesn't change it.
pub fn read_only_slider<T: UiNumber>(ui_sys: &UiSystem, label: &str, min: T, max: T, format: UiNumberFormat, value: T) {
    ui_sys.with_ui(|ui| {
        let mut shown_value = value;
        let slider = egui::Slider::new(&mut shown_value, min..=max).text(rich_text(ui_sys, ui, label_text(label)));
        let response = ui.add(apply_format(slider, format));
        record(ui_sys, ui, &response, "slider", label, None, false, Some(value.to_json()));
    });
}

// A number input of any number type, e.g. a usize index or a u32 count.
pub fn input_scalar<T: UiNumber>(ui_sys: &UiSystem, label: &str, value: &mut T, step: T) -> bool {
    input_number(ui_sys, label, value, step, UiNumberFormat::Default, false)
}

pub fn input_int(ui_sys: &UiSystem, label: &str, value: &mut i32, step: i32, format: UiNumberFormat, read_only: bool) -> bool {
    input_number(ui_sys, label, value, step, format, read_only)
}

pub fn input_float(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut f32,
    step: f32,
    format: UiNumberFormat,
    read_only: bool,
) -> bool {
    input_number(ui_sys, label, value, step, format, read_only)
}

// A number field (drag it, or click it to type), "-" and "+" buttons that step it, and the label.
fn input_number<T: UiNumber>(
    ui_sys: &UiSystem,
    label: &str,
    value: &mut T,
    step: T,
    format: UiNumberFormat,
    read_only: bool,
) -> bool {
    ui_sys.with_ui(|ui| {
        let mut shown_value = *value;
        let edited = if read_only { &mut shown_value } else { &mut *value };

        let (response, step_change) = ui
            .horizontal(|ui| {
                let step_f64 = step.to_f64();
                let mut drag = egui::DragValue::new(edited).speed(if step_f64 > 0.0 { step_f64 * 0.1 } else { 0.1 });
                if let UiNumberFormat::Decimals(decimals) = format {
                    drag = drag.fixed_decimals(usize::from(decimals));
                }
                let response = ui.add_enabled(!read_only, drag);

                let mut step_change = 0.0;
                if step_f64 > 0.0 && !read_only {
                    if ui.small_button("-").clicked() {
                        step_change = -step_f64;
                    }
                    if ui.small_button("+").clicked() {
                        step_change = step_f64;
                    }
                }

                let text = label_text(label);
                if !text.is_empty() {
                    ui.label(rich_text(ui_sys, ui, text));
                }
                (response, step_change)
            })
            .inner;

        let mut changed = response.changed();
        if step_change != 0.0 {
            *value = T::from_f64(value.to_f64() + step_change);
            changed = true;
        }

        let set_value = record(ui_sys, ui, &response, T::INPUT_KIND, label, None, !read_only, Some(value.to_json()));
        if let Some(set_value) = set_value.as_ref().and_then(T::from_json) {
            changed |= set_value != *value;
            *value = set_value;
        }

        changed
    })
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
    let before = *current_item;
    let selected_text = items.get(*current_item).map_or("", |item| item.as_ref());

    let response = ui_sys.with_parent_ui(|parent| {
        let combo = egui::ComboBox::new(("ui_combo", label), rich_text(ui_sys, parent, label_text(label)))
            .selected_text(selected_text)
            .show_ui(parent, |ui| {
                ui_sys.ui_scope(ui, |ui_sys| {
                    let _path = path_scope(ui_sys, label);
                    ui_sys.with_ui(|ui| {
                        for (index, item) in items.iter().enumerate() {
                            let item = item.as_ref();
                            let response = ui.selectable_value(current_item, index, item);
                            record(ui_sys, ui, &response, "list_item", item, None, true, Some(Value::Bool(*current_item == index)));
                        }
                    });
                });
            });
        let visible = parent.clip_rect().contains(combo.response.rect.center());
        (combo.response, visible)
    });

    let (response, _visible) = response;
    let mut changed = *current_item != before;

    let current = items.get(*current_item).map_or(Value::Null, |item| Value::String(item.as_ref().to_string()));
    let set_value = ui_sys.with_ui(|ui| record(ui_sys, ui, &response, "dropdown", label, None, true, Some(current)));
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
    let shown_label = label_text(label);

    // A value from ui.set, applied before drawing: egui edits `value` in place, so it shows even
    // while the input has keyboard focus.
    let set = !read_only
        && match ui_sys.take_next_widget_value("text_input", shown_label) {
            Some(Value::String(text)) => {
                *value = text;
                true
            }
            _ => false,
        };

    ui_sys.with_ui(|ui| {
        let id = ui.make_persistent_id(("ui_text_edit", label));
        let mut move_cursor_to_end = set;

        // Focus asked for in a sizing pass (a window's first frame, where widgets are disabled and
        // egui ignores their focus) waits for the next frame.
        let focus_pending_id = id.with("focus_pending");
        if focus || ui.ctx().data_mut(|data| data.remove_temp::<bool>(focus_pending_id)).is_some() {
            if ui.is_enabled() && !ui.is_sizing_pass() {
                ui.memory_mut(|memory| memory.request_focus(id));
            } else {
                ui.ctx().data_mut(|data| data.insert_temp(focus_pending_id, true));
            }
        }

        // Up and down step through the history, taken before the input sees them.
        if let Some(history) = history
            && ui.memory(|memory| memory.has_focus(id))
            && !history.entries.is_empty()
        {
            let up = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
            let down = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
            let previous = *history.position;

            if up || down {
                *history.position = match (up, previous) {
                    (true, None) => history.entries.len().checked_sub(1),
                    (true, Some(index)) => Some(index.saturating_sub(1)),
                    (false, Some(index)) if index + 1 < history.entries.len() => Some(index + 1),
                    (false, _) => None,
                };

                if *history.position != previous {
                    value.clear();
                    if let Some(index) = *history.position {
                        value.push_str(&history.entries[index]);
                    }
                    move_cursor_to_end = true;
                }
            }
        }

        if move_cursor_to_end && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
            let end = egui::text::CCursor::new(value.chars().count());
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            state.store(ui.ctx(), id);
        }

        let mut text_edit = egui::TextEdit::singleline(value).id(id).interactive(!read_only);
        if let Some(hint) = hint {
            text_edit = text_edit.hint_text(hint);
        }
        if full_width {
            text_edit = text_edit.desired_width(f32::INFINITY);
        }

        let response = if hide_label || shown_label.is_empty() {
            ui.add(text_edit)
        } else {
            ui.horizontal(|ui| {
                let response = ui.add(text_edit);
                ui.label(rich_text(ui_sys, ui, shown_label));
                response
            })
            .inner
        };

        let submitted = submit_on_enter && response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        if submitted {
            response.request_focus(); // Keep typing, as in a console.
        }

        record(ui_sys, ui, &response, "text_input", shown_label, None, !read_only, Some(Value::String(value.clone())));

        UiTextEditResponse { changed: set || response.changed(), submitted }
    })
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
// while drawing.
pub struct UiPainter {
    painter: egui::Painter,
    font: egui::FontId,
}

pub fn painter(ui_sys: &UiSystem, layer: UiLayer) -> UiPainter {
    let painter = match layer {
        UiLayer::Window => ui_sys.with_ui(|ui| ui.painter().clone()),
        UiLayer::Background => ui_sys.ctx().layer_painter(egui::LayerId::background()),
        UiLayer::Foreground => ui_sys.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("ui_foreground"))),
    };
    UiPainter { painter, font: ui_sys.text_font() }
}

impl UiPainter {
    // A one point wide outline.
    pub fn rect(&self, rect: Rect, color: Color) {
        self.painter.rect_stroke(egui_rect(rect), 0.0, Stroke::new(1.0, color32(color)), StrokeKind::Inside);
    }

    pub fn rect_filled(&self, rect: Rect, color: Color, rounding: f32) {
        self.painter.rect_filled(egui_rect(rect), rounding, color32(color));
    }

    // A one point wide outline.
    pub fn circle(&self, center: Vec2, radius: f32, color: Color) {
        self.painter.circle_stroke(egui_pos2(center), radius, Stroke::new(1.0, color32(color)));
    }

    pub fn circle_filled(&self, center: Vec2, radius: f32, color: Color) {
        self.painter.circle_filled(egui_pos2(center), radius, color32(color));
    }

    // Text with its top left corner at `position`, in the current font.
    pub fn text(&self, position: Vec2, color: Color, text: impl UiTextArg) {
        text.with_str(|text| {
            self.painter.text(egui_pos2(position), Align2::LEFT_TOP, text, self.font.clone(), color32(color));
        });
    }

    // The whole of `texture` stretched over `rect`, multiplied by `tint`.
    pub fn image(&self, texture: TextureHandle, rect: Rect, tint: Color) {
        self.painter.image(egui_texture(texture), egui_rect(rect), uv_full(), color32(tint));
    }

    // `texture` mapped onto a quad, e.g. a rotated one. `uvs` go with `corners`, (0, 0) being the
    // texture's top left.
    pub fn image_quad(&self, texture: TextureHandle, corners: [Vec2; 4], uvs: [Vec2; 4], tint: Color) {
        let tint = color32(tint);
        let mut mesh = egui::Mesh::with_texture(egui_texture(texture));
        for (corner, uv) in corners.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex { pos: egui_pos2(*corner), uv: egui_pos2(uv), color: tint });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        self.painter.add(egui::Shape::mesh(mesh));
    }

    // What `draw` paints is clipped to `clip_rect`, within the current clip rect.
    pub fn clipped(&self, clip_rect: Rect, draw: impl FnOnce(&Self)) {
        let clip_rect = self.painter.clip_rect().intersect(egui_rect(clip_rect));
        let clipped = Self { painter: self.painter.with_clip_rect(clip_rect), font: self.font.clone() };
        draw(&clipped);
    }
}

// ----------------------------------------------
// Queries
// ----------------------------------------------

// The size of the window the game draws in, in points.
pub fn screen_size(ui_sys: &UiSystem) -> Vec2 {
    let size = ui_sys.ctx().content_rect().size();
    Vec2::new(size.x, size.y)
}

// The height of a framed widget, such as a button, plus the space under it.
pub fn frame_height_with_spacing(ui_sys: &UiSystem) -> f32 {
    let style = ui_sys.egui_style();
    let font_height = ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&ui_sys.text_font()));
    let frame_height = (font_height + style.spacing.button_padding.y * 2.0).max(style.spacing.interact_size.y);
    frame_height + style.spacing.item_spacing.y
}

// The space between widgets.
pub fn item_spacing(ui_sys: &UiSystem) -> Vec2 {
    let spacing = ui_sys.egui_style().spacing.item_spacing;
    Vec2::new(spacing.x, spacing.y)
}

// Whether the window being drawn is the topmost one, as after a click on it.
pub fn is_window_focused(ui_sys: &UiSystem) -> bool {
    ui_sys.with_ui(|ui| {
        let layer_id = ui.layer_id();
        ui.ctx().memory(|memory| memory.areas().top_layer_id(layer_id.order)) == Some(layer_id)
    })
}
