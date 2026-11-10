use std::any::Any;
use smallbox::smallbox;
use egui::Color32;

use common::{Color, Vec2, bitflags_with_display, mem::{self, RawPtr, RcMut, WeakMut, WeakRef}};

use super::{UiMessageBox, UiWidget, UiWidgetCallback, UiWidgetClosure, UiWidgetContext, UiWidgetImpl, widget_id};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{
        UiNineSlice, UiSystem,
        immediate::{self, egui_pos2, egui_vec2, nine_slice_shape, uv_full},
        renderer::egui_texture,
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
    },
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

// The borders of a menu's background sprite or panel: its contents stay inside them. In points, or,
// for a menu with a size, as fractions of that size, since a sprite is stretched to it (a scroll's
// rollers take the same share of the scroll at any size). The menu lays its contents out in its
// inner rect, its rect minus these, so they can be as large as the menu needs.
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

    // The insets of a menu of `size` in points, rounded to whole points so the contents stay on
    // the pixel grid. None for relative insets on a menu sized to its contents.
    pub fn to_points(self, size: Option<Vec2>) -> Option<Self> {
        let scale = if self.relative { size? } else { Vec2::one() };
        Some(Self::points(
            (self.left * scale.x).round(),
            (self.top * scale.y).round(),
            (self.right * scale.x).round(),
            (self.bottom * scale.y).round(),
        ))
    }

    // Left plus right, top plus bottom.
    #[inline]
    pub fn sum(&self) -> Vec2 {
        Vec2::new(self.left + self.right, self.top + self.bottom)
    }

    #[inline]
    fn left_top(&self) -> egui::Vec2 {
        egui::vec2(self.left, self.top)
    }

    #[inline]
    fn right_bottom(&self) -> egui::Vec2 {
        egui::vec2(self.right, self.bottom)
    }

    fn from_margin(margin: egui::Margin) -> Self {
        Self::points(f32::from(margin.left), f32::from(margin.top), f32::from(margin.right), f32::from(margin.bottom))
    }
}

// ----------------------------------------------
// UiMenuBackground
// ----------------------------------------------

// What a menu draws behind its contents in place of the style's window frame.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UiMenuBackground {
    Sprite(TextureHandle), // Stretched over the menu.
    Panel(UiNineSlice),    // A 9-slice panel over the menu.
}

impl UiMenuBackground {
    fn shape(&self, rect: egui::Rect) -> egui::Shape {
        match self {
            Self::Sprite(texture) => egui::Shape::image(egui_texture(*texture), rect, uv_full(), Color32::WHITE),
            Self::Panel(panel) => nine_slice_shape(rect, panel, Color32::WHITE),
        }
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
    pub(super) background: Option<UiMenuBackground>,
    pub(super) content_insets: Option<UiMenuInsets>, // The style's window margin if None.
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Option<Vec2>, // The style's item spacing if None.
    pub(super) backdrop_color: Option<Color>, // A modal menu's; UiSystem::modal_backdrop_color if None.
    message_box: UiMessageBox,
    sounds: UiWidgetSounds, // Opened and Closed: nothing by default; a dialog's or popup's role.
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

        let border = style.visuals.window_stroke.width;
        size += self.insets_points(self.size, &style).sum();
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
            backdrop_color: None,
            message_box: UiMessageBox::default(),
            sounds: UiWidgetSounds::new(),
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

    // The colour a modal menu dims the screen behind it with, in place of the UI system's
    // (UiSystem::set_modal_backdrop_color), e.g. a popup's darker one.
    pub fn with_backdrop_color(mut self, color: Color) -> Self {
        self.backdrop_color = Some(color);
        self
    }

    // A sprite stretched over the window, in place of the style's frame and title bar.
    pub fn with_background<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.background = Some(UiMenuBackground::Sprite(context.load_texture(sprite.into())));
        self
    }

    // A loaded texture stretched over the window, as with_background.
    pub fn with_background_texture(mut self, texture: TextureHandle) -> Self {
        self.background = Some(UiMenuBackground::Sprite(texture));
        self
    }

    // A 9-slice panel over the window, in place of the style's frame and title bar, with the
    // contents inside `content_insets` (see with_content_insets).
    pub fn with_panel(mut self, panel: UiNineSlice, content_insets: UiMenuInsets) -> Self {
        self.background = Some(UiMenuBackground::Panel(panel));
        self.content_insets = Some(content_insets);
        self
    }

    // The borders of the background: the contents stay inside them, in the menu's inner rect.
    pub fn with_content_insets(mut self, insets: UiMenuInsets) -> Self {
        self.content_insets = Some(insets);
        self
    }

    pub fn with_widget(mut self, widget: impl Into<UiWidgetImpl>) -> Self {
        self.add_widget(widget);
        self
    }

    // `event` (Opened or Closed) plays `role` when the menu opens or closes. A menu plays nothing
    // by default; dialogs play DialogOpened and DialogClosed, popups PopupOpened and PopupClosed.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.set_sound(event, role);
        self
    }

    pub fn set_sound(&mut self, event: UiSoundEvent, role: Option<UiSoundKey>) {
        self.sounds.set(event, role);
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
        self.sounds.play(context, UiSoundEvent::Opened, None);

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
        self.sounds.play(context, UiSoundEvent::Closed, None);

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

    // Draws the menu as draw does, but with `flags` in place of its own: e.g. without Modal, so it
    // shows as a plain window under a modal one (a dialog under the one on top of it), which takes
    // its input and dims it with its backdrop.
    pub fn draw_with_flags(&mut self, context: &mut dyn UiWidgetContext, flags: UiMenuFlags) {
        self.draw_custom(context, flags, Self::close, Self::message_box, Self::draw_menu_contents);
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

        // With its popup open, a menu is hidden (HideWhenMessageBoxOpen) or shows under it, as a
        // plain window even if it's modal: the popup is, so it takes the input and dims the menu
        // with its backdrop, as the mockups draw the Game menu under the Quit popup. A modal menu
        // keeps dimming the screen behind it, under its window.
        let mut flags = flags;
        let mut keep_backdrop = false;
        let mut message_box = on_get_msg_box(self);
        if message_box.is_open() {
            if flags.intersects(UiMenuFlags::HideWhenMessageBoxOpen) {
                message_box.as_mut().draw(context);
                return;
            }
            keep_backdrop = flags.intersects(UiMenuFlags::Modal);
            flags.remove(UiMenuFlags::Modal);
        }

        let ui_sys = context.ui_sys();
        let ctx = ui_sys.ctx().clone();

        let backdrop_color = self.backdrop_color.map_or_else(|| ui_sys.modal_backdrop_color(), immediate::color32);
        if keep_backdrop {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(ctx.content_rect(), 0.0, backdrop_color);
        }
        let style = ui_sys.egui_style().clone();

        let window_size = self.calc_window_size(ui_sys);
        let (window_pos, window_pivot) = self.calc_window_pos(context, ui_sys);
        let id = widget_id(self, &self.label);

        let insets = self.insets_points(window_size, &style);
        let has_title_bar = self.background.is_none() && !self.label.is_empty() && !self.has_flags(UiMenuFlags::NoTitleBar);

        // A background replaces the window's frame. The frame has no margin of its own: the menu
        // lays its contents out in its inner rect, its rect minus `insets`.
        let frame = if self.background.is_some() {
            egui::Frame::NONE
        } else {
            egui::Frame::window(&style).inner_margin(egui::Margin::ZERO).shadow(egui::Shadow::NONE)
        };

        let pivot = egui::Align2([align(window_pivot.x), align(window_pivot.y)]);
        let modal = flags.intersects(UiMenuFlags::Modal);

        // A menu's content area is its inner rect: its window minus `insets`, its background's
        // insets or the style's window margin.
        let draw_window = |ui: &mut egui::Ui| -> bool {
            let background_slot = self.background.map(|_| ui.painter().add(egui::Shape::Noop));

            // Its content area, for the layout check: listed before its widgets, filled in once the
            // frame has drawn.
            let content_index = immediate::reserve_rect(ui_sys, "menu_content", &self.label);

            let frame_response = frame.show(ui, |ui| {
                // The inner rect, sized to the menu's size minus its insets if it has one.
                let content_size = window_size.map(|size| egui_vec2(size - insets.sum()).max(egui::Vec2::ZERO));
                let bounds = ui.available_rect_before_wrap();
                let inner_min = bounds.min + insets.left_top();
                let inner_max = (bounds.max - insets.right_bottom()).max(inner_min);

                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(inner_min, inner_max)));
                if let Some(content_size) = content_size {
                    content_ui.set_min_size(content_size);
                    content_ui.set_max_size(content_size);
                }

                let mut close_clicked = false;
                if has_title_bar {
                    content_ui.horizontal(|ui| {
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
                let mut scrolls = false;
                if content_size.is_some() {
                    let output = egui::ScrollArea::vertical()
                        .id_salt(id.with("contents"))
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                        .show(&mut content_ui, draw_contents);
                    scrolls = output.content_size.y > output.inner_rect.height() + 0.5;
                } else {
                    draw_contents(&mut content_ui);
                }

                ui_sys.set_item_width(previous_item_width);

                // The menu takes its inner rect plus its insets.
                let inner = content_ui.min_rect();
                ui.allocate_rect(egui::Rect::from_min_max(inner.min - insets.left_top(), inner.max + insets.right_bottom()), egui::Sense::hover());

                (close_clicked, inner, scrolls)
            });

            let (close_clicked, inner, scrolls) = frame_response.inner;

            if let (Some(slot), Some(background)) = (background_slot, &self.background) {
                ui.painter().set(slot, background.shape(frame_response.response.rect));
            }

            // Its value says when its contents scroll, so the layout check knows its top and
            // bottom edges clip them.
            if let Some(index) = content_index {
                let value = scrolls.then(|| serde_json::json!({ "scrolls": true }));
                immediate::update_rect(ui_sys, index, inner, false, value);
            }

            close_clicked
        };

        immediate::close_popups_on_open(&ctx, id);
        let mut area = egui::Area::new(id).fixed_pos(egui_pos2(window_pos)).pivot(pivot).constrain(false);

        if let Some(size) = window_size {
            // A window with a size shows at once, at full opacity: egui would fade it in, and hide
            // it for a sizing pass the first time it's shown. That matters for dialogs, each its
            // own window: one replacing another would blink.
            area = area.fade_in(false);
            if egui::AreaState::load(&ctx, id).is_none() {
                let order = if modal { egui::Order::Foreground } else { egui::Order::Middle };
                let state = egui::AreaState {
                    pivot_pos: Some(egui_pos2(window_pos)),
                    pivot,
                    size: Some(egui_vec2(size)),
                    interactable: true,
                    last_became_visible_at: None,
                };
                ctx.memory_mut(|memory| memory.areas_mut().set_state(egui::LayerId::new(order, id), state));
            }
        }

        if modal {
            // Modal window has exclusive input focus (e.g.: popup message box).
            let close_on_escape = flags.intersects(UiMenuFlags::CloseModalOnEscape);

            let response = egui::Modal::new(id)
                .area(area.order(egui::Order::Foreground))
                .frame(egui::Frame::NONE)
                .backdrop_color(backdrop_color)
                .show(&ctx, |ui| draw_window(ui));

            let escape_pressed = close_on_escape && response.is_top_modal && ctx.input(crate::ui::immediate::escape_pressed_in);
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

    // Its content insets in points for a menu of `size`: its own, or the style's window margin
    // (also for relative insets on a menu sized to its contents).
    fn insets_points(&self, size: Option<Vec2>, style: &egui::Style) -> UiMenuInsets {
        self.content_insets
            .and_then(|insets| insets.to_points(size))
            .unwrap_or_else(|| UiMenuInsets::from_margin(style.spacing.window_margin))
    }

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
