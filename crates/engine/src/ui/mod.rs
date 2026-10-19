// The UI system, on top of egui. Only this module uses egui:
// - immediate.rs: immediate-mode functions, re-exported here (ui::button, ui::text, ui::window, ...),
//   for the developer UI and DrawDebugUi.
// - widgets.rs: retained widgets for the game's menus and HUD (UiMenu, UiTextButton, ...).
// - style.rs: styles and fonts, from JSON files under assets/configs/ui/styles/.
// - backend.rs and renderer.rs: egui's frame, the engine's input, and what egui draws, handed to
//   the RenderSystem as UiDrawData.
// - registry.rs: the widget registry for UI automation (ui.tree, ui.click, ui.set).
//
// The functions that draw widgets take `&UiSystem` and add to the innermost open container: a
// window, a menu, a tooltip... UiSystem keeps a stack of egui Uis, one per open container. A
// container takes its parent Ui off the stack while egui runs it, and pushes a child for its
// contents; widgets borrow the top one. See with_ui, with_parent_ui and ui_scope.

use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ops::{Deref, DerefMut},
};

use egui::Color32;
use smallvec::SmallVec;

use common::{Color, Vec2, time::Seconds};

use crate::{
    app::{
        Application,
        input::{InputAction, InputKey, InputModifiers, InputSystem, MouseButton},
    },
    file_sys::paths::{self, AssetPath, PathRef},
    log,
    platform,
    render::{
        texture::{TextureCache, TextureFilter, TextureSettings},
        ui_draw::UiDrawData,
    },
};

pub mod debug_ui;
pub mod icons;
pub mod sound;
pub mod tests;
pub mod text;
pub mod widgets;

pub use debug_ui::DrawDebugUi;

pub use immediate::*;

// Internal implementation.
mod backend;
mod immediate;
mod internal;
mod registry;
mod renderer;
mod style;
mod toast;
use backend::UiBackend;
use registry::UiWidgetRegistry;
pub use registry::UiWidgetInfo;
use style::{UiStyle, UiStyles};
use toast::UiToasts;

// ----------------------------------------------
// UiInputEvent
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UiInputEvent {
    Handled,    // Input event was handled/consumed and should not propagate.
    NotHandled, // Input event wasn't handled and should propagate to other widgets.
}

impl UiInputEvent {
    #[inline]
    pub fn is_handled(self) -> bool {
        self == UiInputEvent::Handled
    }

    #[inline]
    pub fn not_handled(self) -> bool {
        self == UiInputEvent::NotHandled
    }
}

// ----------------------------------------------
// UiTheme
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum UiTheme {
    #[default]
    Dev,
    InGame,
}

impl UiTheme {
    pub const COUNT: usize = 2;

    #[inline]
    pub fn is_dev(self) -> bool {
        self == Self::Dev
    }

    #[inline]
    pub fn is_in_game(self) -> bool {
        self == Self::InGame
    }

    // Prefixes the names of the style's fonts and font families.
    #[inline]
    fn style_name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::InGame => "game",
        }
    }
}

// Where the game's styles are, relative to assets/configs/.
pub const UI_STYLE_PATHS: [(UiTheme, &str); UiTheme::COUNT] =
    [(UiTheme::Dev, "ui/styles/dev.json"), (UiTheme::InGame, "ui/styles/game.json")];

// ----------------------------------------------
// UiSystem
// ----------------------------------------------

pub struct UiSystem {
    backend: UiBackend,
    styles: Option<UiStyles>,
    style_paths: [(UiTheme, String); UiTheme::COUNT],
    theme: Cell<UiTheme>,
    modal_backdrop: Cell<Option<Color32>>, // Replaces the style's, from set_modal_backdrop_color.
    font_scale: Cell<Option<UiFontScale>>,   // For the text drawn next, until reset.
    item_width: Cell<Option<f32>>,           // For the slider, input or dropdown drawn next, from its group.
    show_debug_window: Cell<bool>,
    delta_time_secs: Seconds,
    toasts: RefCell<UiToasts>,

    // The innermost open container's egui Ui is the last one.
    ui_stack: RefCell<SmallVec<[egui::Ui; 8]>>,

    // The widget drawn last, for tooltips that go with it (ui::item_tooltip).
    last_response: RefCell<Option<egui::Response>>,

    // Whether a widget has been hovered this frame, so far, and over the whole of the last one.
    widget_hovered: Cell<bool>,
    widget_hovered_last_frame: bool,

    // UI automation (registry.rs): the widgets being recorded this frame, and last frame's.
    registry: RefCell<UiWidgetRegistry>,
    widgets: Vec<UiWidgetInfo>,

    // The value ui.set queued for the window being drawn (ui::take_window_set_value).
    window_set_value: RefCell<Option<serde_json::Value>>,
}

impl UiSystem {
    pub fn new() -> Self {
        text::initialize();

        Self {
            backend: UiBackend::new(),
            styles: None,
            style_paths: UI_STYLE_PATHS.map(|(theme, path)| (theme, path.to_string())),
            theme: Cell::new(UiTheme::Dev),
            modal_backdrop: Cell::new(None),
            font_scale: Cell::new(None),
            item_width: Cell::new(None),
            show_debug_window: Cell::new(false),
            delta_time_secs: 0.0,
            toasts: RefCell::new(UiToasts::default()),
            ui_stack: RefCell::new(SmallVec::new()),
            last_response: RefCell::new(None),
            widget_hovered: Cell::new(false),
            widget_hovered_last_frame: false,
            registry: RefCell::new(UiWidgetRegistry::default()),
            widgets: Vec::new(),
            window_set_value: RefCell::new(None),
        }
    }

    // ----------------------
    // Styles:
    // ----------------------

    // Loads a style per theme, with its fonts, from files under assets/configs/. Call before
    // drawing anything, and before creating widgets, which read the style and measure text.
    // `pixels_per_point`: the window's content scale, which fonts are rasterized for.
    pub fn load_styles(&mut self, paths: &[(UiTheme, &str); UiTheme::COUNT], pixels_per_point: f32) -> Result<(), String> {
        let path_refs = paths.map(|(theme, path)| (theme, PathRef::from_str(path)));
        let styles = UiStyles::load(&path_refs)?;

        self.backend.ctx().set_fonts(styles.fonts.clone());
        self.styles = Some(styles);
        self.style_paths = paths.map(|(theme, path)| (theme, path.to_string()));
        self.apply_theme_style();

        if !self.frame_started() {
            self.backend.warm_up(pixels_per_point); // Sets up the fonts for measuring text.
        }

        log::info!(log::channel!("ui"), "Loaded UI styles: {}.", paths.map(|(_, path)| path).join(", "));
        Ok(())
    }

    // Loads the style files again, e.g. after editing them (ui.reload_styles). Keeps the current
    // styles if one fails to load. Takes effect from the next frame.
    pub fn reload_styles(&mut self) -> Result<(), String> {
        let paths = self.style_paths.clone();
        let paths = [(paths[0].0, paths[0].1.as_str()), (paths[1].0, paths[1].1.as_str())];
        let pixels_per_point = self.ctx().pixels_per_point();
        self.load_styles(&paths, pixels_per_point)
    }

    // The current theme's style.
    #[inline]
    fn style(&self) -> &UiStyle {
        self.styles.as_ref().expect("UiSystem::load_styles must be called first!").style(self.theme.get())
    }

    fn apply_theme_style(&self) {
        if let Some(styles) = &self.styles {
            self.backend.ctx().set_global_style(styles.style(self.theme.get()).egui.clone());
        }
    }

    // ----------------------
    // Frame:
    // ----------------------

    pub fn begin_frame(&mut self, app: &Application, input_sys: &InputSystem, delta_time_secs: Seconds) {
        debug_assert!(!self.frame_started());
        debug_assert!(self.styles.is_some(), "UiSystem::load_styles must be called before the first frame!");

        self.registry.get_mut().begin_frame(&mut self.widgets);
        self.widget_hovered_last_frame = self.widget_hovered.replace(false);
        self.delta_time_secs = delta_time_secs;
        self.font_scale.set(None);

        self.backend.begin_frame(app, input_sys, delta_time_secs);
        self.apply_theme_style();
    }

    // Returns what the UI drew, for RenderSystem::end_frame. Textures egui manages are created,
    // updated and released in `tex_cache` here. `draw_overlays` false keeps toasts and the badge
    // off this frame, e.g. while it's being captured. `badge`: a status line at the top of the
    // screen, such as automation's.
    pub fn end_frame(&mut self, tex_cache: &mut TextureCache, draw_overlays: bool, badge: Option<&str>) -> &UiDrawData {
        debug_assert!(self.frame_started());
        debug_assert!(self.ui_stack.get_mut().is_empty(), "Unbalanced UI container stack!");

        // Last, so toasts draw over everything else.
        let mut toasts = std::mem::take(self.toasts.get_mut()); // Out, so they can draw with `self`.
        toasts.update(self, self.delta_time_secs, draw_overlays);
        *self.toasts.get_mut() = toasts;

        if let Some(badge) = badge.filter(|_| draw_overlays) {
            toast::draw_badge(self, badge);
        }

        if self.show_debug_window.get() {
            self.draw_debug_window();
        }

        self.last_response.get_mut().take();
        self.font_scale.set(None);

        self.backend.end_frame(tex_cache)
    }

    #[inline]
    pub fn frame_started(&self) -> bool {
        self.backend.frame_started()
    }

    // Shows a short message at the bottom of the screen for a few seconds.
    #[inline]
    pub fn show_toast(&self, text: impl Into<String>) {
        self.toasts.borrow_mut().push(text.into());
    }

    // egui's own inspection windows, for debugging the UI.
    fn draw_debug_window(&self) {
        let ctx = self.backend.ctx().clone();
        let mut open = true;

        let previous_style = ctx.global_style();
        if let Some(styles) = &self.styles {
            ctx.set_global_style(styles.style(UiTheme::Dev).egui.clone());
        }

        egui::Window::new("UI Debug").open(&mut open).default_size([420.0, 520.0]).vscroll(true).show(&ctx, |ui| {
            egui::CollapsingHeader::new("Settings").show(ui, |ui| ctx.settings_ui(ui));
            egui::CollapsingHeader::new("Inspection").show(ui, |ui| ctx.inspection_ui(ui));
            egui::CollapsingHeader::new("Memory").show(ui, |ui| ctx.memory_ui(ui));
            egui::CollapsingHeader::new("Textures").show(ui, |ui| ctx.texture_ui(ui));
        });

        ctx.set_global_style(previous_style);
        self.show_debug_window.set(open);
    }

    // ----------------------
    // Widget registry (UI automation, see registry.rs):
    // ----------------------

    #[inline]
    pub fn set_widget_registry_enabled(&self, enabled: bool) {
        self.registry.borrow_mut().set_enabled(enabled);
    }

    #[inline]
    pub fn is_widget_registry_enabled(&self) -> bool {
        self.registry.borrow().is_enabled()
    }

    // The widgets drawn last frame, in drawing order. Empty while the registry is off.
    #[inline]
    pub fn widgets(&self) -> &[UiWidgetInfo] {
        &self.widgets
    }

    // Sets a widget's value on its next draw, through its own update callback (ui.set).
    #[inline]
    pub fn queue_widget_value(&self, path: String, value: serde_json::Value) {
        self.registry.borrow_mut().queue_value(path, value);
    }

    #[inline]
    pub(crate) fn push_widget_path(&self, segment: &str) {
        self.registry.borrow_mut().push_path(segment);
    }

    #[inline]
    pub(crate) fn pop_widget_path(&self) {
        self.registry.borrow_mut().pop_path();
    }

    // Records a widget described in full. Returns its path, or None while the registry is off.
    #[inline]
    pub(crate) fn record_widget_info(&self, info: UiWidgetInfo) -> Option<String> {
        self.registry.borrow_mut().record(info)
    }

    // Records a widget listed before what's inside it, filled in later with update_widget_info.
    #[inline]
    pub(crate) fn reserve_widget_info(&self, info: UiWidgetInfo) -> Option<usize> {
        self.registry.borrow_mut().reserve(info)
    }

    #[inline]
    pub(crate) fn update_widget_info(&self, index: usize, update: impl FnOnce(&mut UiWidgetInfo)) {
        self.registry.borrow_mut().update(index, update);
    }

    // A value queued for the widget at `path` by ui.set.
    #[inline]
    pub(crate) fn take_widget_value(&self, path: &str) -> Option<serde_json::Value> {
        self.registry.borrow_mut().take_value(path)
    }

    // A value queued for the widget that records next with `kind` and `label`, for one that applies
    // it before drawing.
    #[inline]
    pub(crate) fn take_next_widget_value(&self, kind: &'static str, label: &str) -> Option<serde_json::Value> {
        self.registry.borrow_mut().take_next_value(kind, label)
    }

    // Asks the widget at `path`, scrolled out of its window's view, to scroll into view on its next
    // draw (ui.click).
    #[inline]
    pub fn request_widget_reveal(&self, path: String) {
        self.registry.borrow_mut().request_reveal(path);
    }

    #[inline]
    pub(crate) fn take_widget_reveal(&self, path: &str) -> bool {
        self.registry.borrow_mut().take_reveal(path)
    }

    // ----------------------
    // UI debug window:
    // ----------------------

    #[inline]
    pub fn show_ui_debug_window(&self, show: bool) {
        self.show_debug_window.set(show);
    }

    #[inline]
    pub fn is_showing_ui_debug_window(&self) -> bool {
        self.show_debug_window.get()
    }

    // ----------------------
    // Input:
    // ----------------------

    #[inline]
    pub fn on_key_input(&mut self, key: InputKey, action: InputAction, modifiers: InputModifiers) -> UiInputEvent {
        self.backend.on_key_input(key, action, modifiers);
        if self.is_handling_key_input() { UiInputEvent::Handled } else { UiInputEvent::NotHandled }
    }

    #[inline]
    pub fn on_char_input(&mut self, c: char) -> UiInputEvent {
        self.backend.on_char_input(c);
        if self.is_handling_key_input() { UiInputEvent::Handled } else { UiInputEvent::NotHandled }
    }

    #[inline]
    pub fn on_scroll(&mut self, amount: Vec2) -> UiInputEvent {
        self.backend.on_scroll(amount);
        if self.is_handling_mouse_input() { UiInputEvent::Handled } else { UiInputEvent::NotHandled }
    }

    #[inline]
    pub fn on_mouse_button(&mut self, _: MouseButton, _: InputAction, _: InputModifiers) -> UiInputEvent {
        // Mouse buttons are polled from the InputSystem instead (backend.rs). This only tells
        // whether the click is the UI's.
        if self.is_handling_mouse_input() { UiInputEvent::Handled } else { UiInputEvent::NotHandled }
    }

    // True if the mouse is over the UI, or the UI is using it (e.g. dragging a slider).
    #[inline]
    pub fn is_handling_mouse_input(&self) -> bool {
        self.backend.wants_pointer()
    }

    // True while text is being typed into the UI.
    #[inline]
    pub fn is_handling_key_input(&self) -> bool {
        self.backend.wants_keyboard()
    }

    // True if the mouse was over a widget last frame, e.g. for the game to leave the click to the
    // UI. Unlike is_handling_mouse_input, a menu's background doesn't count.
    #[inline]
    pub fn is_any_widget_hovered(&self) -> bool {
        self.widget_hovered_last_frame
    }

    // ----------------------
    // Ui Theme:
    // ----------------------

    pub fn set_ui_theme(&self, theme: UiTheme) {
        if self.theme.replace(theme) != theme {
            log::info!(log::channel!("ui"), "Setting UI theme to {theme:?}.");
            self.font_scale.set(None);
            self.apply_theme_style();
        }
    }

    #[inline]
    pub fn current_ui_theme(&self) -> UiTheme {
        self.theme.get()
    }

    // The color modal dialogs dim the screen behind them with.
    #[inline]
    pub fn set_modal_backdrop_color(&self, color: Color) {
        self.modal_backdrop.set(Some(Color32::from_rgba_unmultiplied(
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8,
            (color.a * 255.0).round() as u8,
        )));
    }

    pub(crate) fn modal_backdrop_color(&self) -> Color32 {
        self.modal_backdrop.get().unwrap_or_else(|| self.style().modal_backdrop)
    }

    // ----------------------
    // Font Scale:
    // ----------------------

    // Draws the text that follows `font_scale` times its size, until it's set again. Widget windows
    // reset it when they end.
    #[inline]
    pub fn set_window_font_scale(&self, font_scale: UiFontScale) {
        debug_assert!(font_scale.is_valid());
        self.font_scale.set(if font_scale.is_identity() { None } else { Some(font_scale) });
    }

    #[inline]
    pub fn current_window_front_scale(&self) -> UiFontScale {
        self.font_scale.get().unwrap_or_default()
    }

    #[inline]
    pub fn set_tooltip_font_scale(&self, font_scale: UiFontScale) {
        self.set_window_font_scale(font_scale);
    }

    // ----------------------
    // Internal: the egui context and the Ui stack
    // ----------------------

    #[inline]
    pub(crate) fn ctx(&self) -> &egui::Context {
        self.backend.ctx()
    }

    // Runs `f` on the innermost open container's Ui, e.g. to add a widget to it. Don't call back
    // into the ui functions from `f`: use with_parent_ui for egui containers.
    #[inline]
    pub(crate) fn with_ui<R>(&self, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let mut stack = self.ui_stack.borrow_mut();
        let ui = stack.last_mut().expect("No open UI container: draw widgets inside a window, a menu or a tooltip.");
        f(ui)
    }

    // True if a container is open, so widgets can be drawn.
    #[inline]
    pub(crate) fn has_ui(&self) -> bool {
        !self.ui_stack.borrow().is_empty()
    }

    // Runs an egui container on the innermost open container's Ui. That Ui is off the stack while
    // `f` runs, so the container's contents can push their own (ui_scope).
    pub(crate) fn with_parent_ui<R>(&self, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let mut parent =
            self.ui_stack.borrow_mut().pop().expect("No open UI container: draw widgets inside a window, a menu or a tooltip.");
        let result = f(&mut parent);
        self.ui_stack.borrow_mut().push(parent);
        result
    }

    // Draws `contents` into a child of `ui`, which egui gave a container, then moves `ui`'s cursor
    // past what they drew.
    #[inline]
    pub(crate) fn ui_scope<R>(&self, ui: &mut egui::Ui, contents: impl FnOnce(&UiSystem) -> R) -> R {
        self.child_ui_scope(ui, egui::UiBuilder::new(), true, contents).0
    }

    // Draws `contents` into a child of `ui` made with `builder`, e.g. at a given rect. Moves `ui`'s
    // cursor past it if `advance_cursor`. Returns what `contents` returned and the rect it drew in.
    pub(crate) fn child_ui_scope<R>(
        &self,
        ui: &mut egui::Ui,
        builder: egui::UiBuilder,
        advance_cursor: bool,
        contents: impl FnOnce(&UiSystem) -> R,
    ) -> (R, egui::Rect) {
        let child = ui.new_child(builder);

        let depth = {
            let mut stack = self.ui_stack.borrow_mut();
            stack.push(child);
            stack.len()
        };

        let result = contents(self);

        let child = {
            let mut stack = self.ui_stack.borrow_mut();
            debug_assert_eq!(stack.len(), depth, "Unbalanced UI container stack!");
            stack.pop().expect("UI container stack underflow!")
        };

        let rect = child.min_rect();
        if advance_cursor {
            ui.advance_cursor_after_rect(rect);
        }
        (result, rect)
    }

    // Draws `contents` with its top left corner at `rect`'s, in a child of the current Ui at most
    // `rect` large, without moving the current Ui's cursor. Returns the rect it drew in.
    pub(crate) fn draw_at<R>(&self, rect: egui::Rect, contents: impl FnOnce(&UiSystem) -> R) -> (R, egui::Rect) {
        self.with_parent_ui(|parent| self.child_ui_scope(parent, egui::UiBuilder::new().max_rect(rect), false, contents))
    }

    // Remembers the widget drawn last, for ui::item_tooltip, and whether the mouse is on it.
    #[inline]
    pub(crate) fn set_last_response(&self, response: &egui::Response) {
        if response.hovered() {
            self.widget_hovered.set(true);
        }
        *self.last_response.borrow_mut() = Some(response.clone());
    }

    #[inline]
    pub(crate) fn last_response(&self) -> Option<egui::Response> {
        self.last_response.borrow().clone()
    }

    // The font of plain text in the current theme, at the current font scale.
    pub(crate) fn text_font(&self) -> egui::FontId {
        let mut font = self.style_font(&egui::TextStyle::Body);
        font.size *= self.current_window_front_scale().0;
        font
    }

    // The current theme's font for `text_style`.
    #[inline]
    pub(crate) fn style_font(&self, text_style: &egui::TextStyle) -> egui::FontId {
        self.style().font(text_style)
    }

    // The width the group being drawn gives its sliders, inputs and dropdowns (internal.rs).
    #[inline]
    pub(crate) fn item_width(&self) -> Option<f32> {
        self.item_width.get()
    }

    // Returns the previous width, to put back.
    #[inline]
    pub(crate) fn set_item_width(&self, width: Option<f32>) -> Option<f32> {
        self.item_width.replace(width)
    }

    // The font of FontAwesome icons (ui::icons) in the current theme.
    #[inline]
    pub(crate) fn icon_font(&self) -> egui::FontId {
        self.style().icon_font.clone()
    }

    #[inline]
    pub(crate) fn egui_style(&self) -> &std::sync::Arc<egui::Style> {
        &self.style().egui
    }
}

impl Default for UiSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------
// UiFontScale
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct UiFontScale(pub f32);

impl UiFontScale {
    #[inline]
    pub const fn identity() -> Self {
        Self(1.0)
    }

    #[inline]
    pub fn is_identity(&self) -> bool {
        self.0 == 1.0
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.0 > 0.0
    }
}

impl Default for UiFontScale {
    #[inline]
    fn default() -> Self {
        Self::identity()
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

#[inline]
pub fn assets_path() -> AssetPath {
    paths::assets_path().join("ui")
}

#[inline]
pub fn texture_settings() -> TextureSettings {
    // Use linear filter without mipmaps for all UI textures.
    TextureSettings { filter: TextureFilter::Linear, mipmaps: false, ..Default::default() }
}

// ----------------------------------------------
// UiStaticVar
// ----------------------------------------------

pub struct UiStaticVar<T> {
    value: UnsafeCell<T>,
}

impl<T> UiStaticVar<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value: UnsafeCell::new(value) }
    }

    #[inline]
    pub fn set(&'static self, value: T) {
        *self.as_mut() = value;
    }

    #[inline]
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref(&'static self) -> &'static T {
        debug_assert!(platform::is_main_thread(), "Can only use UiStaticVar from main thread!");
        unsafe { &*self.value.get() }
    }

    #[inline]
    #[allow(clippy::mut_from_ref)] // intentional.
    pub fn as_mut(&'static self) -> &'static mut T {
        debug_assert!(platform::is_main_thread(), "Can only use UiStaticVar from main thread!");
        unsafe { &mut *self.value.get() }
    }
}

// SAFETY: UI code always executes from the main thread, so we don't mind using local static config
// vars for debug UI widgets. is_main_thread() asserts prevent misuse.
unsafe impl<T> Sync for UiStaticVar<T> {}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for UiStaticVar<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        debug_assert!(platform::is_main_thread(), "Can only use UiStaticVar from main thread!");
        unsafe { &*self.value.get() }
    }
}

impl<T> DerefMut for UiStaticVar<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        debug_assert!(platform::is_main_thread(), "Can only use UiStaticVar from main thread!");
        unsafe { &mut *self.value.get() }
    }
}
