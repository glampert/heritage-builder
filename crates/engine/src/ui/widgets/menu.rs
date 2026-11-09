use std::any::Any;
use smallbox::smallbox;
use egui::Color32;

use common::{Vec2, bitflags_with_display, mem::{self, RawPtr, RcMut, WeakMut, WeakRef}};

use super::{UiMessageBox, UiWidget, UiWidgetCallback, UiWidgetClosure, UiWidgetContext, UiWidgetImpl, widget_id};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{UiSystem, immediate::{self, egui_pos2, egui_vec2, uv_full}, renderer::egui_texture},
};

// ----------------------------------------------
// UiMenuFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiMenuFlags: u16 {
        const IsOpen                 = 1 << 0;
        const PauseSimIfOpen         = 1 << 1;
        const Fullscreen             = 1 << 2;
        const AlignCenter            = 1 << 3;
        const AlignCenterTop         = 1 << 4;
        const AlignLeft              = 1 << 5;
        const AlignRight             = 1 << 6;
        const Modal                  = 1 << 7;
        const CloseModalOnEscape     = 1 << 8;
        const HideWhenMessageBoxOpen = 1 << 9;
        const AdjustSizeToContents   = 1 << 10; // Even if explicit size given, adjust to contents on menu opening.
        const NoTitleBar             = 1 << 11;
    }
}

// ----------------------------------------------
// UiMenuPosition
// ----------------------------------------------

// Where a menu's window goes, before its alignment flags apply.
#[derive(Default)]
pub enum UiMenuPosition {
    #[default]
    None,
    Fixed(Vec2),
    Callback(UiMenuCalcPosition), // Computed each frame.
}

impl UiMenuPosition {
    // A position computed each frame, e.g. beside a widget of another menu.
    pub fn callback<F>(f: F) -> Self
    where
        F: Fn(&UiMenu, &mut dyn UiWidgetContext) -> Vec2 + 'static,
    {
        Self::Callback(smallbox!(f))
    }
}

impl From<Vec2> for UiMenuPosition {
    fn from(position: Vec2) -> Self {
        Self::Fixed(position)
    }
}

// ----------------------------------------------
// UiMenuInsets
// ----------------------------------------------

// The borders of a menu's background sprite: its contents stay inside them. In points, or, for a
// menu with a size, as fractions of that size, since the sprite is stretched to it (a scroll's
// rollers take the same share of the scroll at any size).
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct UiMenuInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub relative: bool, // Fractions of the menu's size instead of points.
}

impl UiMenuInsets {
    pub const fn same(points: f32) -> Self {
        Self::points(points, points, points, points)
    }

    pub const fn points(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom, relative: false }
    }

    // Fractions of the menu's width (left, right) and height (top, bottom), for a menu with a size.
    pub const fn fractions(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom, relative: true }
    }

    // The padding of a menu of `size`, or None for relative insets on a menu sized to its contents.
    // egui frames pad in whole points (i8), enough for a sprite's borders.
    fn to_margin(self, size: Option<Vec2>) -> Option<egui::Margin> {
        let scale = if self.relative { size? } else { Vec2::one() };
        let points = |inset: f32| inset.round().clamp(f32::from(i8::MIN), f32::from(i8::MAX)) as i8;
        Some(egui::Margin {
            left: points(self.left * scale.x),
            right: points(self.right * scale.x),
            top: points(self.top * scale.y),
            bottom: points(self.bottom * scale.y),
        })
    }
}

// ----------------------------------------------
// UiMenu Types
// ----------------------------------------------

pub type UiMenuRcMut   = RcMut<UiMenu>;
pub type UiMenuWeakMut = WeakMut<UiMenu>;
pub type UiMenuWeakRef = WeakRef<UiMenu>;

pub type UiMenuOpenClose    = UiWidgetCallback<dyn Fn(&mut UiMenu, &mut dyn UiWidgetContext, bool)>;
pub type UiMenuCalcPosition = UiWidgetClosure<dyn Fn(&UiMenu, &mut dyn UiWidgetContext) -> Vec2>;

#[derive(Copy, Clone)]
pub struct UiMenuWidgetIndex(usize);

// ----------------------------------------------
// UiMenu
// ----------------------------------------------

// The default width of inputs, sliders and dropdowns in a menu with a size, as a fraction of it.
const MENU_ITEM_WIDTH_FRACTION: f32 = 0.65;

pub struct UiMenu {
    label: String,
    flags: UiMenuFlags,
    size: Option<Vec2>,
    position: UiMenuPosition,
    pub(super) background: Option<TextureHandle>,
    pub(super) content_insets: Option<UiMenuInsets>, // The style's window margin if None.
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Option<Vec2>, // The style's item spacing if None.
    message_box: UiMessageBox,
    on_open_close: UiMenuOpenClose,
}

impl UiWidget for UiMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        self.draw_custom(context, self.flags, Self::close, Self::message_box, Self::draw_menu_contents);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let style = context.ui_sys().egui_style().clone();
        let mut size = Vec2::zero();

        for widget in &self.widgets {
            let widget_size = widget.measure(context);
            size.x = size.x.max(widget_size.x); // Max width.
            size.y += widget_size.y; // Total height.
        }

        if !self.widgets.is_empty() {
            // Add inter-widget spacing.
            size.y += self.widget_spacing(context.ui_sys()).y * (self.widgets.len() - 1) as f32;
        }

        let padding = self.content_insets.and_then(|insets| insets.to_margin(self.size)).unwrap_or(style.spacing.window_margin);
        let border = style.visuals.window_stroke.width;
        size += Vec2::new(f32::from(padding.left) + f32::from(padding.right), f32::from(padding.top) + f32::from(padding.bottom));
        size += Vec2::new(border, border) * 2.0;

        size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiMenu {
    // A closed menu sized to its contents, with the style's frame and widget spacing. `label`
    // names it in widget paths and, without a background sprite, titles its window.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            flags: UiMenuFlags::empty(),
            size: None,
            position: UiMenuPosition::None,
            background: None,
            content_insets: None,
            widgets: Vec::new(),
            widget_spacing: None,
            message_box: UiMessageBox::default(),
            on_open_close: None,
        }
    }

    // Menus are shared: a dialog holds its menu, and so do the callbacks that open and close it.
    #[inline]
    pub fn into_rc(self) -> UiMenuRcMut {
        UiMenuRcMut::new(self)
    }

    pub fn with_flags(mut self, flags: UiMenuFlags) -> Self {
        self.flags = flags;
        self
    }

    // A fixed size: the menu scrolls its contents when they don't fit it.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_position(mut self, position: impl Into<UiMenuPosition>) -> Self {
        self.position = position.into();
        self
    }

    // Between its widgets, and the lines of its text.
    pub fn with_widget_spacing(mut self, spacing: Vec2) -> Self {
        self.widget_spacing = Some(spacing);
        self
    }

    // A sprite stretched over the window, in place of the style's frame and title bar.
    pub fn with_background<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.background = Some(context.load_texture(sprite.into()));
        self
    }

    // The borders of the background sprite: the contents stay inside them.
    pub fn with_content_insets(mut self, insets: UiMenuInsets) -> Self {
        self.content_insets = Some(insets);
        self
    }

    pub fn with_widget(mut self, widget: impl Into<UiWidgetImpl>) -> Self {
        self.add_widget(widget);
        self
    }

    // Called with `true` when the menu opens and `false` when it closes.
    pub fn on_open_close<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut UiMenu, &mut dyn UiWidgetContext, bool) + 'static,
    {
        self.set_open_close_callback(f);
        self
    }

    pub fn set_open_close_callback<F>(&mut self, f: F)
    where
        F: Fn(&mut UiMenu, &mut dyn UiWidgetContext, bool) + 'static,
    {
        self.on_open_close = Some(smallbox!(f));
    }

    #[inline]
    pub fn set_position(&mut self, position: impl Into<UiMenuPosition>) {
        self.position = position.into();
    }

    // A fixed size from now on (see with_size).
    pub fn set_size(&mut self, size: Vec2) {
        self.size = Some(size);
    }

    #[inline]
    pub fn set_flags(&mut self, new_flags: UiMenuFlags, value: bool) {
        self.flags.set(new_flags, value);
    }

    #[inline]
    pub fn reset_flags(&mut self, new_flags: UiMenuFlags) {
        self.flags = new_flags;
    }

    #[inline]
    pub fn has_flags(&self, flags: UiMenuFlags) -> bool {
        self.flags.intersects(flags)
    }

    #[inline]
    pub fn flags(&self) -> UiMenuFlags {
        self.flags
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.has_flags(UiMenuFlags::IsOpen)
    }

    pub fn open(&mut self, context: &mut dyn UiWidgetContext) {
        if self.is_open() {
            return;
        }

        self.flags.insert(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.pause_simulation();
        }

        const IS_OPEN: bool = true;
        self.notify_open_close(context, IS_OPEN);
    }

    pub fn close(&mut self, context: &mut dyn UiWidgetContext) {
        if !self.is_open() {
            return;
        }

        self.flags.remove(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.resume_simulation();
        }

        const IS_OPEN: bool = false;
        self.notify_open_close(context, IS_OPEN);
    }

    fn notify_open_close(&mut self, context: &mut dyn UiWidgetContext, is_open: bool) {
        if let Some(on_open_close) = &self.on_open_close {
            // The callback lives in this menu and gets it mutably: see UiWidgetCallback.
            on_open_close(mem::mut_ref_cast(self), context, is_open);
        }
    }

    pub fn add_widget(&mut self, widget: impl Into<UiWidgetImpl>) -> UiMenuWidgetIndex {
        let index = self.widgets.len();
        self.widgets.push(widget.into());
        UiMenuWidgetIndex(index)
    }

    #[inline]
    pub fn widgets(&self) -> &[UiWidgetImpl] {
        &self.widgets
    }

    #[inline]
    pub fn widgets_mut(&mut self) -> &mut [UiWidgetImpl] {
        &mut self.widgets
    }

    #[inline]
    pub fn widget_as<Widget: UiWidget>(&self, index: UiMenuWidgetIndex) -> Option<&Widget> {
        self.widgets[index.0].as_any().downcast_ref::<Widget>()
    }

    #[inline]
    pub fn widget_as_mut<Widget: UiWidget>(&mut self, index: UiMenuWidgetIndex) -> Option<&mut Widget> {
        self.widgets[index.0].as_any_mut().downcast_mut::<Widget>()
    }

    pub fn find_widget_of_type<Widget: UiWidget>(&self) -> Option<(UiMenuWidgetIndex, &Widget)> {
        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                return Some((UiMenuWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_of_type_mut<Widget: UiWidget>(&mut self) -> Option<(UiMenuWidgetIndex, &mut Widget)> {
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                return Some((UiMenuWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_with_label<Widget: UiWidget>(&self, label: &str) -> Option<(UiMenuWidgetIndex, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if w.label() == label {
                    return Some((UiMenuWidgetIndex(index), w));
                }
            }
        }

        None
    }

    pub fn find_widget_with_label_mut<Widget: UiWidget>(&mut self, label: &str) -> Option<(UiMenuWidgetIndex, &mut Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                if w.label() == label {
                    return Some((UiMenuWidgetIndex(index), w));
                }
            }
        }

        None
    }

    // ----------------------
    // Modal Message Box:
    // ----------------------

    pub fn is_message_box_open(&self) -> bool {
        self.message_box.is_open()
    }

    // Opens the menu's message box, building it with `build` the first time. Later calls reopen
    // it as built, until reset_message_box drops it.
    pub fn open_message_box<F>(&mut self, context: &mut dyn UiWidgetContext, build: F)
    where
        F: FnOnce(&mut dyn UiWidgetContext) -> UiMessageBox,
    {
        if self.message_box.menu.is_none() {
            self.message_box = build(context);
        }
        self.message_box.open(context);
    }

    pub fn close_message_box(&mut self, context: &mut dyn UiWidgetContext) {
        self.message_box.close(context);
    }

    pub fn reset_message_box(&mut self) {
        self.message_box.reset();
    }

    #[inline]
    pub fn message_box(&mut self) -> RawPtr<UiMessageBox> {
        RawPtr::from_ref(&self.message_box)
    }

    // ----------------------
    // Custom Menu Drawing:
    // ----------------------

    pub fn draw_menu_contents(&mut self, context: &mut dyn UiWidgetContext) {
        // Default widget spacing, for this menu's widgets.
        let ui_sys = context.ui_sys();
        let spacing = egui_vec2(self.widget_spacing(ui_sys));
        let previous_spacing = ui_sys.with_ui(|ui| std::mem::replace(&mut ui.spacing_mut().item_spacing, spacing));

        for widget in &mut self.widgets {
            widget.draw(context);
        }

        ui_sys.with_ui(|ui| ui.spacing_mut().item_spacing = previous_spacing);
    }

    pub fn draw_custom<OnClose, OnGetMsgBox, OnDrawContents>(
        &mut self,
        context: &mut dyn UiWidgetContext,
        flags: UiMenuFlags,
        on_close: OnClose,
        on_get_msg_box: OnGetMsgBox,
        on_draw_contents: OnDrawContents,
    ) where
        OnClose: FnOnce(&mut Self, &mut dyn UiWidgetContext),
        OnGetMsgBox: FnOnce(&mut Self) -> RawPtr<UiMessageBox>,
        OnDrawContents: FnOnce(&mut Self, &mut dyn UiWidgetContext),
    {
        let mut is_open = flags.intersects(UiMenuFlags::IsOpen);
        if !is_open {
            return;
        }

        let mut message_box = on_get_msg_box(self);
        if message_box.is_open()
            && (flags.intersects(UiMenuFlags::HideWhenMessageBoxOpen) || flags.intersects(UiMenuFlags::Modal))
        {
            message_box.as_mut().draw(context);
            return;
        }

        let ui_sys = context.ui_sys();
        let ctx = ui_sys.ctx().clone();
        let style = ui_sys.egui_style().clone();

        let window_size = self.calc_window_size(ui_sys);
        let (window_pos, window_pivot) = self.calc_window_pos(context, ui_sys);
        let id = widget_id(self, &self.label);

        let padding = self.content_insets.and_then(|insets| insets.to_margin(window_size)).unwrap_or(style.spacing.window_margin);
        let has_title_bar = self.background.is_none() && !self.label.is_empty() && !self.has_flags(UiMenuFlags::NoTitleBar);

        // A texture background replaces the window's frame.
        let frame = if self.background.is_some() {
            egui::Frame::NONE.inner_margin(padding)
        } else {
            egui::Frame::window(&style).inner_margin(padding).shadow(egui::Shadow::NONE)
        };

        let pivot = egui::Align2([align(window_pivot.x), align(window_pivot.y)]);
        let modal = flags.intersects(UiMenuFlags::Modal);

        // A menu's content area is its window minus `padding`: its background sprite's insets, or the
        // style's window margin.
        let draw_window = |ui: &mut egui::Ui| -> bool {
            let background_slot = self.background.map(|_| ui.painter().add(egui::Shape::Noop));

            // Its content area, for the layout check: listed before its widgets, filled in once the
            // frame has drawn.
            let content_index = immediate::reserve_rect(ui_sys, "menu_content", &self.label);

            let frame_response = frame.show(ui, |ui| {
                let content_size = window_size.map(|size| (egui_vec2(size) - padding.sum()).max(egui::Vec2::ZERO));
                if let Some(content_size) = content_size {
                    ui.set_min_size(content_size);
                    ui.set_max_size(content_size);
                }

                let mut close_clicked = false;
                if has_title_bar {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&self.label).strong());
                        close_clicked = ui.small_button("x").clicked();
                    });
                }

                // Inputs, sliders and dropdowns take a fraction of a sized menu's content width by default.
                let previous_item_width = ui_sys.set_item_width(content_size.map(|size| size.x * MENU_ITEM_WIDTH_FRACTION));

                let draw_contents = |ui: &mut egui::Ui| {
                    ui_sys.ui_scope(ui, |ui_sys| {
                        context.begin_widget_window();
                        ui_sys.push_widget_path(&self.label);
                        on_draw_contents(self, context);
                        ui_sys.pop_widget_path();
                        context.end_widget_window();
                    });
                };

                // A menu with a size scrolls its contents when they don't fit it.
                if content_size.is_some() {
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("contents"))
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                        .show(ui, draw_contents);
                } else {
                    draw_contents(ui);
                }

                ui_sys.set_item_width(previous_item_width);

                close_clicked
            });

            if let (Some(slot), Some(background)) = (background_slot, self.background) {
                let image = egui::Shape::image(egui_texture(background), frame_response.response.rect, uv_full(), Color32::WHITE);
                ui.painter().set(slot, image);
            }

            if let Some(index) = content_index {
                let outer = frame_response.response.rect;
                let content = egui::Rect::from_min_max(outer.min + padding.left_top(), outer.max - padding.right_bottom());
                immediate::update_rect(ui_sys, index, content, false, None);
            }

            frame_response.inner
        };

        immediate::close_popups_on_open(&ctx, id);
        let area = egui::Area::new(id).fixed_pos(egui_pos2(window_pos)).pivot(pivot).constrain(false);

        if modal {
            // Modal window has exclusive input focus (e.g.: popup message box).
            let close_on_escape = flags.intersects(UiMenuFlags::CloseModalOnEscape);

            let response = egui::Modal::new(id)
                .area(area.order(egui::Order::Foreground))
                .frame(egui::Frame::NONE)
                .backdrop_color(ui_sys.modal_backdrop_color())
                .show(&ctx, |ui| draw_window(ui));

            let escape_pressed = close_on_escape && response.is_top_modal && ctx.input(|input| input.key_pressed(egui::Key::Escape));
            if response.inner || escape_pressed {
                is_open = false;
            }
        } else {
            // Regular window.
            let response = area.order(egui::Order::Middle).show(&ctx, |ui| draw_window(ui));
            if response.inner {
                is_open = false;
            }
        }

        let closed = flags.intersects(UiMenuFlags::IsOpen) && !is_open;
        if closed {
            // Window was closed. Raise event with receiver.
            on_close(self, context);
        }

        // Each menu can have one message box. This is a no-op if one is not open.
        message_box.as_mut().draw(context);
    }

    // ----------------------
    // Internal:
    // ----------------------

    // Between the menu's widgets: its own, or the style's.
    fn widget_spacing(&self, ui_sys: &UiSystem) -> Vec2 {
        self.widget_spacing.unwrap_or_else(|| {
            let spacing = ui_sys.egui_style().spacing.item_spacing;
            Vec2::new(spacing.x, spacing.y)
        })
    }

    // The menu's size, or None to size it to its contents.
    fn calc_window_size(&self, ui_sys: &UiSystem) -> Option<Vec2> {
        if self.has_flags(UiMenuFlags::AdjustSizeToContents) {
            None // Its size only applies when it opens, and it sizes to its contents after that.
        } else if let Some(size) = self.size {
            Some(size)
        } else if self.has_flags(UiMenuFlags::Fullscreen) {
            Some(crate::ui::screen_size(ui_sys))
        } else {
            None // Sized to contents.
        }
    }

    fn calc_window_pos(&self, context: &mut dyn UiWidgetContext, ui_sys: &UiSystem) -> (Vec2, Vec2) {
        let display_size = crate::ui::screen_size(ui_sys);

        let mut position = Vec2::zero();
        let mut pivot = Vec2::zero();

        match &self.position {
            UiMenuPosition::Fixed(fixed) => {
                position = *fixed;
            }
            UiMenuPosition::Callback(calc_position) => {
                position = calc_position(self, context);
            }
            UiMenuPosition::None => {}
        }

        if self.has_flags(UiMenuFlags::AlignCenter) && self.has_flags(UiMenuFlags::AlignLeft) {
            // Center-left
            position = Vec2::new(0.0, display_size.y * 0.5);
            pivot = Vec2::new(0.0, 0.5);
        } else if self.has_flags(UiMenuFlags::AlignCenter) && self.has_flags(UiMenuFlags::AlignRight) {
            // Center-right
            position = Vec2::new(display_size.x, display_size.y * 0.5);
            pivot = Vec2::new(1.0, 0.5);
        } else if self.has_flags(UiMenuFlags::AlignCenter) {
            // Screen center
            position = Vec2::new(display_size.x * 0.5, display_size.y * 0.5);
            pivot = Vec2::new(0.5, 0.5);
        } else if self.has_flags(UiMenuFlags::AlignCenterTop) {
            // Screen center top
            position = Vec2::new(display_size.x * 0.5, 0.0);
            pivot = Vec2::new(0.5, 0.0);
        } else if self.has_flags(UiMenuFlags::AlignLeft) {
            // Top-left
            position.x = 0.0;
            pivot = Vec2::new(0.0, 0.0);
        } else if self.has_flags(UiMenuFlags::AlignRight) {
            // Top-right
            position.x = display_size.x;
            pivot = Vec2::new(1.0, 0.0);
        }

        (position, pivot)
    }
}

// A pivot of 0, 0.5 or 1 as egui's alignment.
fn align(pivot: f32) -> egui::Align {
    if pivot <= 0.25 {
        egui::Align::Min
    } else if pivot >= 0.75 {
        egui::Align::Max
    } else {
        egui::Align::Center
    }
}
