#![allow(clippy::enum_variant_names)]
#![allow(clippy::type_complexity)]

// Retained widgets for the game's menus and HUD, drawn with egui: a UiMenu holds widgets, which it
// draws in an egui Area (or a Modal) each frame. Widgets keep their own state and report changes
// through callbacks. Callbacks run after the widget has drawn, outside egui's borrow of the Ui,
// since they may open dialogs or draw other widgets.
//
// Widgets record themselves in the widget registry (registry.rs) as they draw, for UI automation.

use std::{any::Any, cell::RefMut, fmt::Display};
use smallbox::{SmallBox, smallbox};
use enum_dispatch::enum_dispatch;
use egui::{Color32, Sense, Stroke, StrokeKind, pos2, vec2};
use strum::{EnumCount, EnumIter, EnumProperty, IntoEnumIterator};

use common::{
    Color,
    RectTexCoords,
    Size,
    Vec2,
    bitflags_with_display,
    format_fixed_string,
    hash::FNV1aHash,
    mem::{self, RawPtr, RcMut, WeakMut, WeakRef},
    time::{CountdownTimer, Seconds},
};

use super::{
    UiFontScale,
    UiSystem,
    UiTooltipPlacement,
    immediate::{self, egui_pos2, egui_vec2, rich_text, uv_full},
    internal,
    renderer::egui_texture,
    sound::{self, UiButtonSoundsEnabled, UiSoundKey},
};
use crate::{
    file_sys::paths::{AssetPath, PathRef},
    render::{RenderSystem, texture::TextureHandle},
    sound::SoundSystem,
};

// ----------------------------------------------
// Widget ids
// ----------------------------------------------

// An egui id for a widget that keeps state in egui (an Area, a combo box, a scroll area): its type,
// label and address. Widgets don't move once they're in a menu.
#[inline]
fn widget_id<W>(widget: &W, label: &str) -> egui::Id {
    egui::Id::unique((std::any::type_name::<W>(), label, widget as *const W as usize))
}

// ----------------------------------------------
// UiWidgetContext
// ----------------------------------------------

// Base UI widget context; implemented by the game layer with extended game context.
pub trait UiWidgetContext<'game> {
    fn runtime_type_id(&self) -> FNV1aHash;

    fn ui_sys(&self) -> &'game UiSystem;
    fn render_sys(&mut self) -> RefMut<'_, &'game mut RenderSystem>;
    fn sound_sys(&mut self) -> RefMut<'_, &'game mut SoundSystem>;
    fn viewport_size(&self) -> Size;
    fn delta_time_secs(&self) -> Seconds;
    fn cursor_screen_pos(&self) -> Vec2;

    fn begin_widget_window(&mut self);
    fn end_widget_window(&mut self);
    fn is_inside_widget_window(&self) -> bool;

    fn begin_side_by_side_layout(&mut self);
    fn end_side_by_side_layout(&mut self);
    fn is_side_by_side_layout(&self) -> bool;

    fn pause_simulation(&mut self);
    fn resume_simulation(&mut self);

    #[inline]
    fn set_window_font_scale(&self, font_scale: UiFontScale) {
        debug_assert!(self.is_inside_widget_window());
        self.ui_sys().set_window_font_scale(font_scale);
    }

    #[inline]
    fn calc_text_size(&self, font_scale: UiFontScale, text: &str) -> Vec2 {
        internal::calc_text_size(self.ui_sys(), font_scale, text).0
    }

    #[inline]
    fn calc_text_and_font_size(&self, font_scale: UiFontScale, text: &str) -> (Vec2, f32) {
        internal::calc_text_size(self.ui_sys(), font_scale, text)
    }

    #[inline]
    fn load_texture(&mut self, path: PathRef) -> TextureHandle {
        let file_path = super::assets_path().join(path);
        self.render_sys()
            .texture_cache_mut()
            .load_texture_with_settings((&file_path).into(), Some(super::texture_settings()))
    }
}

// ----------------------------------------------
// Checked UiWidgetContext up-casting
// ----------------------------------------------

pub trait UiWidgetContextStaticTypeId {
    const STATIC_TYPE_ID: FNV1aHash;
}

#[inline]
pub fn context_as<'game, T>(context: &dyn UiWidgetContext<'game>) -> &'game T
where
    T: UiWidgetContext<'game> + UiWidgetContextStaticTypeId,
{
    // SAFETY: Assert guarantees T is the concrete type we are casting to.
    debug_assert_eq!(context.runtime_type_id(), T::STATIC_TYPE_ID, "Casting UiWidgetContext to incorrect type!");
    unsafe { &*(context as *const dyn UiWidgetContext as *const T) }
}

#[inline]
pub fn context_as_mut<'game, T>(context: &mut dyn UiWidgetContext<'game>) -> &'game mut T
where
    T: UiWidgetContext<'game> + UiWidgetContextStaticTypeId,
{
    debug_assert_eq!(context.runtime_type_id(), T::STATIC_TYPE_ID, "Casting UiWidgetContext to incorrect type!");
    unsafe { &mut *(context as *mut dyn UiWidgetContext as *mut T) }
}

// ----------------------------------------------
// UiWidgetCallbackRef
// ----------------------------------------------

pub trait UiWidgetCallbackRef<Widget: UiWidget> {
    type Ref<'a>;
    fn from_ref<'a>(widget: &'a Widget) -> Self::Ref<'a>;
}

pub struct UiReadOnly;
pub struct UiMutable;

impl<Widget: UiWidget> UiWidgetCallbackRef<Widget> for UiReadOnly {
    type Ref<'a> = &'a Widget;

    #[inline]
    fn from_ref<'a>(widget: &'a Widget) -> Self::Ref<'a> {
        widget
    }
}

impl<Widget: UiWidget> UiWidgetCallbackRef<Widget> for UiMutable {
    type Ref<'a> = &'a mut Widget;

    #[inline]
    fn from_ref<'a>(widget: &'a Widget) -> Self::Ref<'a> {
        mem::mut_ref_cast(widget)
    }
}

// ----------------------------------------------
// UiCallbackArg
// ----------------------------------------------

// Mirrors what UiWidgetCallbackRef does for Widget, but for arguments.
pub trait UiCallbackArg {
    type Arg<'a>;
}

// For owned / 'static arguments where no lifetime is involved.
pub struct UiValue<T>(std::marker::PhantomData<T>);
impl<T> UiCallbackArg for UiValue<T> {
    type Arg<'a> = T;
}

// For string references.
pub struct UiStrRef(RawPtr<str>);
impl UiCallbackArg for UiStrRef {
    type Arg<'a> = &'a str;
}

impl UiStrRef {
    #[inline]
    pub fn new(s: &str) -> Self {
        Self(RawPtr::from_ref(s))
    }
}

// ----------------------------------------------
// UiWidgetCallback
// ----------------------------------------------

#[derive(Default)]
pub enum UiWidgetCallback<Widget, Access, Output = ()>
where
    Widget: UiWidget,
    Access: UiWidgetCallbackRef<Widget>,
{
    #[default]
    None,

    // With plain function pointer, no capture, no memory allocation.
    Fn(for<'a> fn(Access::Ref<'a>, &mut dyn UiWidgetContext) -> Output),

    // With closure/capture. Allocates memory, most flexible.
    Closure(SmallBox<dyn for<'a> Fn(Access::Ref<'a>, &mut dyn UiWidgetContext) -> Output + 'static, smallbox::space::S2>),
}

impl<Widget, Access, Output> UiWidgetCallback<Widget, Access, Output>
where
    Widget: UiWidget,
    Access: UiWidgetCallbackRef<Widget>,
{
    pub fn with_fn(f: for<'a> fn(Access::Ref<'a>, &mut dyn UiWidgetContext) -> Output) -> Self {
        Self::Fn(f)
    }

    pub fn with_closure<C>(c: C) -> Self
    where
        C: for<'a> Fn(Access::Ref<'a>, &mut dyn UiWidgetContext) -> Output + 'static,
    {
        Self::Closure(smallbox!(c))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    fn invoke(&self, widget: &Widget, context: &mut dyn UiWidgetContext) -> Option<Output> {
        match self {
            Self::Fn(f) => Some(f(Access::from_ref(widget), context)),
            Self::Closure(c) => Some(c(Access::from_ref(widget), context)),
            Self::None => None,
        }
    }
}

// ----------------------------------------------
// UiWidgetCallbackWithArg
// ----------------------------------------------

#[derive(Default)]
pub enum UiWidgetCallbackWithArg<Widget, Access, Arg, Output = ()>
where
    Widget: UiWidget,
    Access: UiWidgetCallbackRef<Widget>,
    Arg: UiCallbackArg,
{
    #[default]
    None,

    // With plain function pointer, no capture, no memory allocation.
    Fn(for<'a> fn(Access::Ref<'a>, &mut dyn UiWidgetContext, Arg::Arg<'a>) -> Output),

    // With closure/capture. Allocates memory, most flexible.
    Closure(SmallBox<dyn for<'a> Fn(Access::Ref<'a>, &mut dyn UiWidgetContext, Arg::Arg<'a>) -> Output + 'static, smallbox::space::S2>),
}

impl<Widget, Access, Arg, Output> UiWidgetCallbackWithArg<Widget, Access, Arg, Output>
where
    Widget: UiWidget,
    Access: UiWidgetCallbackRef<Widget>,
    Arg: UiCallbackArg,
{
    pub fn with_fn(f: for<'a> fn(Access::Ref<'a>, &mut dyn UiWidgetContext, Arg::Arg<'a>) -> Output) -> Self {
        Self::Fn(f)
    }

    pub fn with_closure<C>(c: C) -> Self
    where
        C: for<'a> Fn(Access::Ref<'a>, &mut dyn UiWidgetContext, Arg::Arg<'a>) -> Output + 'static,
    {
        Self::Closure(smallbox!(c))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    fn invoke<'a>(&self, widget: &'a Widget, context: &mut dyn UiWidgetContext, arg: Arg::Arg<'a>) -> Option<Output> {
        match self {
            Self::Fn(f) => Some(f(Access::from_ref(widget), context, arg)),
            Self::Closure(c) => Some(c(Access::from_ref(widget), context, arg)),
            Self::None => None,
        }
    }
}

// ----------------------------------------------
// UiWidget Trait
// ----------------------------------------------

#[enum_dispatch(UiWidgetImpl)]
pub trait UiWidget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any {
        mem::mut_ref_cast(self.as_any())
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext);
    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2;

    fn label(&self) -> &str {
        ""
    }

    fn font_scale(&self) -> UiFontScale {
        UiFontScale::default()
    }
}

// ----------------------------------------------
// Widget registry helpers (UI automation)
// ----------------------------------------------

// Records the widget just drawn and returns a value ui.set queued for it, which the widget then
// applies through its own update callback.
#[inline]
fn record_settable(
    ui_sys: &UiSystem,
    ui: &egui::Ui,
    response: &egui::Response,
    kind: &'static str,
    label: &str,
    value: serde_json::Value,
) -> Option<serde_json::Value> {
    immediate::record(ui_sys, ui, response, kind, label, None, true, Some(value))
}

// An item of a list or dropdown, given by text or index.
pub(super) fn list_index(value: &serde_json::Value, items: &[impl AsRef<str>]) -> Option<usize> {
    match value {
        serde_json::Value::String(text) => items.iter().position(|item| item.as_ref() == text),
        serde_json::Value::Number(index) => index.as_u64().map(|index| index as usize).filter(|index| *index < items.len()),
        _ => None,
    }
}

// A label to the left of a framed widget, centered on its height.
fn left_label(ui_sys: &UiSystem, ui: &mut egui::Ui, label: &str) {
    if !label.is_empty() {
        ui.add(egui::Label::new(rich_text(ui_sys, ui, label)).selectable(false));
    }
}

// The UVs of `tex_coords` (V up), from the rect's top left corner to its bottom right.
fn tex_coords_uv(tex_coords: &RectTexCoords) -> egui::Rect {
    let top_left = tex_coords.top_left();
    let bottom_right = tex_coords.bottom_right();
    egui::Rect::from_min_max(pos2(top_left.x, 1.0 - bottom_right.y), pos2(bottom_right.x, 1.0 - top_left.y))
}

// ----------------------------------------------
// UiWidgetImpl
// ----------------------------------------------

#[enum_dispatch]
pub enum UiWidgetImpl {
    UiMenu,
    UiMenuHeading,
    UiSizedTextLabel,
    UiWidgetGroup,
    UiLabeledWidgetGroup,
    UiTextButton,
    UiSpriteButton,
    UiSeparator,
    UiSpriteIcon,
    UiSlider,
    UiCheckbox,
    UiIntInput,
    UiTextInput,
    UiDropdown,
    UiItemList,
    UiMessageBox,
    UiSlideshow,
}

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

#[derive(Default)]
pub enum UiMenuPosition {
    #[default]
    None,
    Vec2(f32, f32),
    Callback(UiMenuCalcPosition),
}

// ----------------------------------------------
// UiMenuParams
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

#[derive(Default)]
pub struct UiMenuParams<'a> {
    pub label: Option<String>,
    pub flags: UiMenuFlags,
    pub size: Option<Vec2>,
    pub position: UiMenuPosition,
    pub widget_spacing: Option<Vec2>,
    pub background: Option<PathRef<'a>>,
    pub content_insets: Option<UiMenuInsets>, // The style's window margin if None.
    pub on_open_close: UiMenuOpenClose,
}

// ----------------------------------------------
// UiMenu Types
// ----------------------------------------------

pub type UiMenuRcMut   = RcMut<UiMenu>;
pub type UiMenuWeakMut = WeakMut<UiMenu>;
pub type UiMenuWeakRef = WeakRef<UiMenu>;

pub type UiMenuOpenClose    = UiWidgetCallbackWithArg<UiMenu, UiMutable, UiValue<bool>>;
pub type UiMenuCalcPosition = UiWidgetCallback<UiMenu, UiReadOnly, Vec2>;

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
    background: Option<TextureHandle>,
    content_insets: Option<UiMenuInsets>,
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Vec2,
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
            size.y += self.widget_spacing.y * (self.widgets.len() - 1) as f32;
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
    pub fn new(context: &mut dyn UiWidgetContext, params: UiMenuParams) -> UiMenuRcMut {
        let widget_spacing = params.widget_spacing.unwrap_or_else(|| {
            let spacing = context.ui_sys().egui_style().spacing.item_spacing;
            Vec2::new(spacing.x, spacing.y)
        });

        UiMenuRcMut::new(Self {
            label: params.label.unwrap_or_default(),
            flags: params.flags,
            size: params.size,
            position: params.position,
            background: params.background.map(|path| context.load_texture(path)),
            content_insets: params.content_insets,
            widgets: Vec::new(),
            widget_spacing,
            message_box: UiMessageBox::default(),
            on_open_close: params.on_open_close,
        })
    }

    #[inline]
    pub fn set_open_close_callback(&mut self, on_open_close: UiMenuOpenClose) {
        self.on_open_close = on_open_close;
    }

    #[inline]
    pub fn set_position(&mut self, position: UiMenuPosition) {
        self.position = position;
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
        self.on_open_close.invoke(self, context, IS_OPEN);
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
        self.on_open_close.invoke(self, context, IS_OPEN);
    }

    pub fn add_widget<Widget>(&mut self, widget: Widget) -> UiMenuWidgetIndex
    where
        Widget: UiWidget + 'static,
        UiWidgetImpl: From<Widget>,
    {
        let index = self.widgets.len();
        self.widgets.push(UiWidgetImpl::from(widget));
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

    pub fn open_message_box<'a, F>(&mut self, context: &mut dyn UiWidgetContext, params_fn: F)
    where
        F: FnMut(&mut dyn UiWidgetContext) -> UiMessageBoxParams<'a>,
    {
        self.message_box.open(context, params_fn);
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
        let spacing = egui_vec2(self.widget_spacing);
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

    // The menu's size, or None to size it to its contents.
    fn calc_window_size(&self, ui_sys: &UiSystem) -> Option<Vec2> {
        if self.has_flags(UiMenuFlags::AdjustSizeToContents) {
            None // Its size only applies when it opens, and it sizes to its contents after that.
        } else if let Some(size) = self.size {
            Some(size)
        } else if self.has_flags(UiMenuFlags::Fullscreen) {
            Some(super::screen_size(ui_sys))
        } else {
            None // Sized to contents.
        }
    }

    fn calc_window_pos(&self, context: &mut dyn UiWidgetContext, ui_sys: &UiSystem) -> (Vec2, Vec2) {
        let display_size = super::screen_size(ui_sys);

        let mut position = Vec2::zero();
        let mut pivot = Vec2::zero();

        match &self.position {
            UiMenuPosition::Vec2(x, y) => {
                position = Vec2::new(*x, *y);
            }
            UiMenuPosition::Callback(cb) => {
                position = cb.invoke(self, context).unwrap();
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

// ----------------------------------------------
// UiText
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct UiText {
    pub string: String,
    pub font_scale: UiFontScale,
    pub color: Option<Color>,
    pub wrap_width: Option<f32>, // Wraps at this width, in points; one line if None.
}

impl UiText {
    pub const fn new(string: String, font_scale: UiFontScale) -> Self {
        Self { string, font_scale, color: None, wrap_width: None }
    }

    pub const fn colored(string: String, font_scale: UiFontScale, color: Color) -> Self {
        Self { string, font_scale, color: Some(color), wrap_width: None }
    }

    pub const fn empty(font_scale: UiFontScale) -> Self {
        Self { string: String::new(), font_scale, color: None, wrap_width: None }
    }

    pub fn with_wrap_width(mut self, width: f32) -> Self {
        self.wrap_width = Some(width);
        self
    }
}

// ----------------------------------------------
// UiMenuHeadingParams
// ----------------------------------------------

pub struct UiMenuHeadingParams<'a> {
    pub lines: Vec<UiText>,
    pub separator: Option<PathRef<'a>>,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub center_vertically: bool,
    pub center_horizontally: bool,
}

impl<'a> Default for UiMenuHeadingParams<'a> {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            separator: None,
            margin_top: 0.0,
            margin_bottom: 0.0,
            center_vertically: false,
            center_horizontally: true, // Center horizontally only (along the x-axis).
        }
    }
}

// ----------------------------------------------
// UiMenuHeading
// ----------------------------------------------

// Centered window heading.
// Can consist of multiple lines and an optional separator sprite at the end.
pub struct UiMenuHeading {
    lines: Vec<UiText>,
    separator: Option<TextureHandle>,
    margin_top: f32,
    margin_bottom: f32,
    center_vertically: bool,
    center_horizontally: bool,
}

impl UiWidget for UiMenuHeading {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());
        let ui_sys = context.ui_sys();

        if self.margin_top > 0.0 {
            ui_sys.with_ui(|ui| ui.add_space(self.margin_top));
        }

        let group = internal::draw_centered_text_group(context, &self.lines, self.center_vertically, self.center_horizontally);

        if ui_sys.is_widget_registry_enabled() && group.is_positive() {
            let text: Vec<&str> = self.lines.iter().map(|line| line.string.as_str()).filter(|line| !line.is_empty()).collect();
            immediate::record_rect(ui_sys, "heading", &text.join(" "), group, false, None);
        }

        if let Some(separator) = self.separator
            && group.is_positive()
        {
            // As tall as the last line's font.
            let separator_height = internal::font(ui_sys, ui_sys.current_window_front_scale()).size;

            ui_sys.with_ui(|ui| {
                let separator_rect = egui::Rect::from_min_size(pos2(group.left(), ui.cursor().top()), vec2(group.width(), separator_height));
                ui.painter().image(egui_texture(separator), separator_rect, uv_full(), Color32::WHITE);
                ui.advance_cursor_after_rect(separator_rect);
            });
        }

        if self.margin_bottom > 0.0 {
            ui_sys.with_ui(|ui| ui.add_space(self.margin_bottom));
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let mut size = Vec2::zero();
        let mut non_empty_lines_count = 0;

        for line in &self.lines {
            if line.string.is_empty() {
                continue;
            }

            let line_size = context.calc_text_size(line.font_scale, &line.string);
            size.x = size.x.max(line_size.x); // Max width.
            size.y += line_size.y; // Total height.

            non_empty_lines_count += 1;
        }

        if non_empty_lines_count > 0 {
            // Add inter-line spacing.
            size.y += internal::item_spacing(context.ui_sys()).y * (non_empty_lines_count - 1) as f32;
        }

        size
    }

    fn font_scale(&self) -> UiFontScale {
        self.lines[0].font_scale
    }
}

impl UiMenuHeading {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiMenuHeadingParams) -> Self {
        debug_assert!(!params.lines.is_empty());
        debug_assert!(params.margin_top >= 0.0);
        debug_assert!(params.margin_bottom >= 0.0);

        Self {
            lines: params.lines,
            separator: params.separator.map(|path| context.load_texture(path)),
            margin_top: params.margin_top,
            margin_bottom: params.margin_bottom,
            center_vertically: params.center_vertically,
            center_horizontally: params.center_horizontally,
        }
    }

    #[inline]
    pub fn lines(&self) -> &Vec<UiText> {
        &self.lines
    }

    #[inline]
    pub fn lines_mut(&mut self) -> &mut Vec<UiText> {
        &mut self.lines
    }

    #[inline]
    pub fn set_line(&mut self, index: usize, text: UiText) {
        self.lines[index] = text;
    }

    #[inline]
    pub fn set_line_string(&mut self, index: usize, string: &str) {
        self.lines[index].string.clear();
        self.lines[index].string.push_str(string);
    }

    #[inline]
    pub fn set_line_font_scale(&mut self, index: usize, font_scale: UiFontScale) {
        self.lines[index].font_scale = font_scale;
    }

    #[inline]
    pub fn set_line_color(&mut self, index: usize, color: Option<Color>) {
        self.lines[index].color = color;
    }

    #[inline]
    pub fn clear_all_lines(&mut self) {
        for line in &mut self.lines {
            line.string.clear();
        }
    }
}

// ----------------------------------------------
// UiSizedTextLabelParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSizedTextLabelParams {
    pub font_scale: UiFontScale,
    pub label: String,
    pub size: Vec2,
}

// ----------------------------------------------
// UiSizedTextLabel
// ----------------------------------------------

// Text centered in a box of a given size.
pub struct UiSizedTextLabel {
    font_scale: UiFontScale,
    label: String,
    size: Vec2,
}

impl UiWidget for UiSizedTextLabel {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let size = self.measure(context);
        let ui_sys = context.ui_sys();

        ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            let color = ui.visuals().text_color();
            let galley = ui.painter().layout_no_wrap(self.label.clone(), internal::font(ui_sys, self.font_scale), color);
            ui.painter().galley(rect.center() - galley.size() * 0.5, galley, color);

            immediate::record(ui_sys, ui, &response, "text", &self.label, None, true, None);
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        if self.size != Vec2::zero() {
            return self.size;
        }

        // Sized to the label, with a framed widget's padding.
        let (text_size, font_size) = context.calc_text_and_font_size(self.font_scale, &self.label);

        let padding = context.ui_sys().egui_style().spacing.button_padding;
        let width = text_size.x + (padding.x * 2.0);
        let height = text_size.y.max(font_size) + (padding.y * 2.0);

        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiSizedTextLabel {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiSizedTextLabelParams) -> Self {
        debug_assert!(params.font_scale.is_valid());
        debug_assert!(!params.label.is_empty());

        Self { font_scale: params.font_scale, label: params.label, size: params.size }
    }

    #[inline]
    pub fn set_label(&mut self, label: String) {
        self.label = label;
    }

    #[inline]
    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }
}

// ----------------------------------------------
// UiWidgetGroupParams
// ----------------------------------------------

pub struct UiWidgetGroupParams {
    pub widget_spacing: Vec2,
    pub center_vertically: bool,
    pub center_horizontally: bool,
    pub stack_vertically: bool,
    pub margin_left: f32,
    pub margin_right: f32,
}

impl Default for UiWidgetGroupParams {
    fn default() -> Self {
        Self {
            widget_spacing: Vec2::zero(),
            center_vertically: true,
            center_horizontally: true,
            stack_vertically: true,
            margin_left: 0.0,
            margin_right: 0.0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct UiWidgetGroupWidgetIndex(usize);

// ----------------------------------------------
// UiWidgetGroup
// ----------------------------------------------

// Groups UiWidgets to draw them centered/aligned.
// Supports vertical and horizontal alignment and custom item spacing.
pub struct UiWidgetGroup {
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Vec2,
    center_vertically: bool,
    center_horizontally: bool,
    stack_vertically: bool,
    margin_left: f32,
    margin_right: f32,
}

impl UiWidget for UiWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let spacing = egui_vec2(self.widget_spacing);
        let previous_spacing = ui_sys.with_ui(|ui| std::mem::replace(&mut ui.spacing_mut().item_spacing, spacing));

        internal::draw_centered_widget_group(
            context,
            &mut self.widgets,
            self.center_vertically,
            self.center_horizontally,
            self.stack_vertically,
            (self.margin_left, self.margin_right),
        );

        ui_sys.with_ui(|ui| ui.spacing_mut().item_spacing = previous_spacing);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let mut size = Vec2::zero();

        for widget in &self.widgets {
            let widget_size = widget.measure(context);

            if self.stack_vertically {
                size.x = size.x.max(widget_size.x); // Max width.
                size.y += widget_size.y; // Total height.
            } else {
                size.x += widget_size.x; // Total width.
                size.y = size.y.max(widget_size.y); // Max height.
            }
        }

        if !self.widgets.is_empty() {
            // Add inter-widget spacing
            let spacing = self.widget_spacing;

            if self.stack_vertically {
                size.y += spacing.y * (self.widgets.len() - 1) as f32; // v-spacing
            } else {
                size.x += spacing.x * (self.widgets.len() - 1) as f32; // h-spacing
            }
        }

        // Subtract margins.
        size.x -= self.margin_left;
        size.x -= self.margin_right;

        size
    }
}

impl UiWidgetGroup {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiWidgetGroupParams) -> Self {
        debug_assert!(params.widget_spacing.x >= 0.0 && params.widget_spacing.y >= 0.0);
        debug_assert!(params.margin_left >= 0.0 && params.margin_right >= 0.0);

        Self {
            widgets: Vec::new(),
            widget_spacing: params.widget_spacing,
            center_vertically: params.center_vertically,
            center_horizontally: params.center_horizontally,
            stack_vertically: params.stack_vertically,
            margin_left: params.margin_left,
            margin_right: params.margin_right,
        }
    }

    pub fn add_widget<Widget>(&mut self, widget: Widget) -> UiWidgetGroupWidgetIndex
    where
        Widget: UiWidget + 'static,
        UiWidgetImpl: From<Widget>,
    {
        let index = self.widgets.len();
        self.widgets.push(UiWidgetImpl::from(widget));
        UiWidgetGroupWidgetIndex(index)
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
    pub fn widget_as<Widget: UiWidget>(&self, index: UiWidgetGroupWidgetIndex) -> Option<&Widget> {
        self.widgets[index.0].as_any().downcast_ref::<Widget>()
    }

    #[inline]
    pub fn widget_as_mut<Widget: UiWidget>(&mut self, index: UiWidgetGroupWidgetIndex) -> Option<&mut Widget> {
        self.widgets[index.0].as_any_mut().downcast_mut::<Widget>()
    }

    pub fn find_widget_of_type<Widget: UiWidget>(&self) -> Option<(UiWidgetGroupWidgetIndex, &Widget)> {
        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                return Some((UiWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_of_type_mut<Widget: UiWidget>(&mut self) -> Option<(UiWidgetGroupWidgetIndex, &mut Widget)> {
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                return Some((UiWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_with_label<Widget: UiWidget>(&self, label: &str) -> Option<(UiWidgetGroupWidgetIndex, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if w.label() == label {
                    return Some((UiWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }

    pub fn find_widget_with_label_mut<Widget: UiWidget>(
        &mut self,
        label: &str,
    ) -> Option<(UiWidgetGroupWidgetIndex, &mut Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                if w.label() == label {
                    return Some((UiWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }
}

// ----------------------------------------------
// UiLabeledWidgetGroupParams
// ----------------------------------------------

pub struct UiLabeledWidgetGroupParams {
    pub label_spacing: f32,
    pub widget_spacing: f32,
    pub center_vertically: bool,
    pub center_horizontally: bool,
    pub margin_left: f32,
    pub margin_right: f32,
    pub label_column_width: Option<f32>, // The labels' column, shared with other groups; the longest label's width if None.
}

impl Default for UiLabeledWidgetGroupParams {
    fn default() -> Self {
        Self {
            label_spacing: 0.0,
            widget_spacing: 0.0,
            center_vertically: true,
            center_horizontally: true,
            margin_left: 0.0,
            margin_right: 0.0,
            label_column_width: None,
        }
    }
}

#[derive(Copy, Clone)]
pub struct UiLabeledWidgetGroupWidgetIndex(usize);

// ----------------------------------------------
// UiLabeledWidgetGroup
// ----------------------------------------------

// Groups labels + UiWidgets to draw them centered/aligned.
// Supports vertical and horizontal alignment and custom item spacing.
pub struct UiLabeledWidgetGroup {
    labels_and_widgets: Vec<(String, UiWidgetImpl)>,
    label_spacing: f32,
    widget_spacing: f32,
    center_vertically: bool,
    center_horizontally: bool,
    margin_left: f32,
    margin_right: f32,
    label_column_width: Option<f32>,
}

impl UiWidget for UiLabeledWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let spacing = vec2(self.label_spacing, self.widget_spacing);
        let previous_spacing = ui_sys.with_ui(|ui| std::mem::replace(&mut ui.spacing_mut().item_spacing, spacing));

        internal::draw_centered_labeled_widget_group(
            context,
            &mut self.labels_and_widgets,
            self.center_vertically,
            self.center_horizontally,
            (self.margin_left, self.margin_right),
            self.label_column_width,
        );

        ui_sys.with_ui(|ui| ui.spacing_mut().item_spacing = previous_spacing);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let spacing = Vec2::new(self.label_spacing, self.widget_spacing);
        let mut size = Vec2::zero();

        for (label, widget) in &self.labels_and_widgets {
            let widget_size = widget.measure(context);
            let label_size = context.calc_text_size(widget.font_scale(), label);
            let label_width = self.label_column_width.map_or(label_size.x, |width| width.max(label_size.x));

            size.x = size.x.max(label_width + spacing.x + widget_size.x); // Max width (label + widget).
            size.y += label_size.y.max(widget_size.y); // Total height (largest of the two).
        }

        if !self.labels_and_widgets.is_empty() {
            // Add inter-widget spacing
            size.y += spacing.y * (self.labels_and_widgets.len() - 1) as f32;
        }

        // Subtract margins.
        size.x -= self.margin_left;
        size.x -= self.margin_right;

        size
    }
}

impl UiLabeledWidgetGroup {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiLabeledWidgetGroupParams) -> Self {
        debug_assert!(params.label_spacing >= 0.0);
        debug_assert!(params.widget_spacing >= 0.0);
        debug_assert!(params.margin_left >= 0.0 && params.margin_right >= 0.0);

        Self {
            labels_and_widgets: Vec::new(),
            label_spacing: params.label_spacing,
            widget_spacing: params.widget_spacing,
            center_vertically: params.center_vertically,
            center_horizontally: params.center_horizontally,
            margin_left: params.margin_left,
            margin_right: params.margin_right,
            label_column_width: params.label_column_width,
        }
    }

    pub fn add_widget<Widget>(&mut self, label: String, widget: Widget) -> UiLabeledWidgetGroupWidgetIndex
    where
        Widget: UiWidget + 'static,
        UiWidgetImpl: From<Widget>,
    {
        debug_assert!(!label.is_empty(), "UiLabeledWidgetGroup requires a non-empty label!");
        debug_assert!(widget.label().is_empty(), "Widgets added to UiLabeledWidgetGroup should not have a label!");

        let index = self.labels_and_widgets.len();
        self.labels_and_widgets.push((label, UiWidgetImpl::from(widget)));
        UiLabeledWidgetGroupWidgetIndex(index)
    }

    #[inline]
    pub fn labels_and_widgets(&self) -> &[(String, UiWidgetImpl)] {
        &self.labels_and_widgets
    }

    #[inline]
    pub fn labels_and_widgets_mut(&mut self) -> &mut [(String, UiWidgetImpl)] {
        &mut self.labels_and_widgets
    }

    #[inline]
    pub fn widget_as<Widget: UiWidget>(&self, index: UiLabeledWidgetGroupWidgetIndex) -> Option<&Widget> {
        self.labels_and_widgets[index.0].1.as_any().downcast_ref::<Widget>()
    }

    #[inline]
    pub fn widget_as_mut<Widget: UiWidget>(&mut self, index: UiLabeledWidgetGroupWidgetIndex) -> Option<&mut Widget> {
        self.labels_and_widgets[index.0].1.as_any_mut().downcast_mut::<Widget>()
    }

    pub fn find_widget_of_type<Widget: UiWidget>(&self) -> Option<(UiLabeledWidgetGroupWidgetIndex, &Widget)> {
        for (index, (_, widget)) in self.labels_and_widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_of_type_mut<Widget: UiWidget>(&mut self) -> Option<(UiLabeledWidgetGroupWidgetIndex, &mut Widget)> {
        for (index, (_, widget)) in self.labels_and_widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_with_label<Widget: UiWidget>(
        &self,
        label: &str,
    ) -> Option<(UiLabeledWidgetGroupWidgetIndex, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, (widget_label, widget)) in self.labels_and_widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if widget_label == label {
                    return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }

    pub fn find_widget_with_label_mut<Widget: UiWidget>(
        &mut self,
        label: &str,
    ) -> Option<(UiLabeledWidgetGroupWidgetIndex, &mut Widget)> {
        debug_assert!(!label.is_empty());

        for (index, (widget_label, widget)) in self.labels_and_widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                if widget_label == label {
                    return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }
}

// ----------------------------------------------
// UiTextButtonSize
// ----------------------------------------------

// Dictates text button font scale.
#[derive(Copy, Clone, Default)]
pub enum UiTextButtonSize {
    #[default]
    Normal,
    Small,
    ExtraSmall,
    Large,
}

impl UiTextButtonSize {
    pub const fn font_scale(self) -> UiFontScale {
        match self {
            UiTextButtonSize::Normal     => UiFontScale(1.2),
            UiTextButtonSize::Small      => UiFontScale(1.0),
            UiTextButtonSize::ExtraSmall => UiFontScale(0.8),
            UiTextButtonSize::Large      => UiFontScale(1.5),
        }
    }
}

// ----------------------------------------------
// UiTextButtonParams
// ----------------------------------------------

pub struct UiTextButtonParams<'a> {
    pub label: String,
    pub tooltip: Option<UiTooltipText>,
    pub size: UiTextButtonSize,
    pub hover: Option<PathRef<'a>>,
    pub sounds_enabled: UiButtonSoundsEnabled,
    pub enabled: bool,
    pub on_pressed: UiTextButtonPressed,
}

pub type UiTextButtonPressed = UiWidgetCallback<UiTextButton, UiReadOnly>;

impl Default for UiTextButtonParams<'_> {
    fn default() -> Self {
        Self {
            label: String::new(),
            tooltip: None,
            size: UiTextButtonSize::default(),
            hover: None,
            sounds_enabled: UiButtonSoundsEnabled::empty(),
            enabled: true,
            on_pressed: UiTextButtonPressed::default(),
        }
    }
}

// ----------------------------------------------
// UiTextButton
// ----------------------------------------------

// Simple text label button. State does not "stick",
// one click equals one call to `on_pressed`, then
// immediately back to unpressed state.
pub struct UiTextButton {
    label: String,
    tooltip: Option<UiTooltipText>,
    font_scale: UiFontScale,
    size: UiTextButtonSize,
    hover: Option<TextureHandle>,
    sounds_enabled: UiButtonSoundsEnabled,
    enabled: bool,
    hovered: bool,
    on_pressed: UiTextButtonPressed,
}

impl UiWidget for UiTextButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let enabled = self.is_enabled();

        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            // Faded text if disabled. It still takes the mouse, for its tooltip.
            let text_color = ui.visuals().text_color();
            let text_color = if enabled { text_color } else { text_color.gamma_multiply(0.5) };
            let text = rich_text(ui_sys, ui, &self.label).color(text_color);

            // With a hover effect texture (underline effect) the button draws fully transparent.
            let button = egui::Button::new(text).frame(self.hover.is_none());
            let response = ui.add(button);

            // Draw underline effect when hovered / active:
            if let Some(hover) = self.hover
                && response.hovered()
            {
                let rect = response.rect;
                let underline_rect = egui::Rect::from_min_size(rect.min + vec2(0.0, rect.height() * 0.5 + 1.0), rect.size());
                let tint = if response.is_pointer_button_down_on() || !enabled {
                    Color32::from_white_alpha(128)
                } else {
                    Color32::WHITE
                };
                ui.painter().image(egui_texture(hover), underline_rect, uv_full(), tint);
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.as_str());
            immediate::record(ui_sys, ui, &response, "text_button", &self.label, tooltip, enabled, None);

            (response.clicked(), response.hovered())
        });

        if hovered {
            // Play sound on transition to hovered state.
            if self.sounds_enabled.intersects(UiButtonSoundsEnabled::Hovered) && !self.hovered && !pressed && enabled {
                sound::play(*context.sound_sys(), UiSoundKey::ButtonHovered);
            }

            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        // Invoke on pressed callback.
        if pressed && enabled {
            if self.sounds_enabled.intersects(UiButtonSoundsEnabled::Pressed) {
                sound::play(*context.sound_sys(), UiSoundKey::ButtonPressed);
            }

            self.on_pressed.invoke(self, context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let padding = context.ui_sys().egui_style().spacing.button_padding;

        // Compute scaled font size (window-independent).
        let (text_size, font_size) = context.calc_text_and_font_size(self.font_scale, &self.label);

        let width = text_size.x + (padding.x * 2.0);
        let height = text_size.y.max(font_size) + (padding.y * 2.0);

        Vec2::new(width, height)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiTextButton {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiTextButtonParams) -> Self {
        debug_assert!(!params.label.is_empty());

        Self {
            label: params.label,
            tooltip: params.tooltip,
            font_scale: params.size.font_scale(),
            size: params.size,
            hover: params.hover.map(|path| context.load_texture(path)),
            sounds_enabled: params.sounds_enabled,
            enabled: params.enabled,
            hovered: false,
            on_pressed: params.on_pressed,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn enable(&mut self, enable: bool) {
        self.enabled = enable;
    }
}

// ----------------------------------------------
// UiSpriteButtonParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSpriteButtonParams {
    pub label: String,
    pub tooltip: Option<UiTooltipText>,
    pub show_tooltip_when_pressed: bool,
    pub sounds_enabled: UiButtonSoundsEnabled,
    pub size: Vec2,
    pub initial_state: UiSpriteButtonState,
    pub state_transition_secs: Seconds,
    pub on_state_changed: UiSpriteButtonStateChanged,
}

pub type UiSpriteButtonStateChanged = UiWidgetCallbackWithArg<UiSpriteButton, UiMutable, UiValue<UiSpriteButtonState>>;

// ----------------------------------------------
// UiSpriteButton
// ----------------------------------------------

// Multi-state sprite button. Works via state polling or callback; state persists until changed.
pub struct UiSpriteButton {
    label: String,

    tooltip: Option<UiTooltipText>,
    show_tooltip_when_pressed: bool,

    size: Vec2,
    position: Option<Vec2>, // NOTE: Position is only known after the first call to draw().
    textures: UiSpriteButtonTextures,
    sounds_enabled: UiButtonSoundsEnabled,

    logical_state: UiSpriteButtonState,
    visual_state: UiSpriteButtonState,
    visual_state_transition_timer: CountdownTimer,
    state_transition_secs: Seconds,

    on_state_changed: UiSpriteButtonStateChanged,
}

impl UiWidget for UiSpriteButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());
        debug_assert!(self.textures.are_textures_loaded());

        let ui_sys = context.ui_sys();
        let texture = self.textures.texture_for_state(self.visual_state);

        let response = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::click());
            if ui.is_rect_visible(rect) {
                ui.painter().image(egui_texture(texture), rect, uv_full(), Color32::WHITE);
            }
            response
        });

        let hovered = response.hovered();
        let left_click = response.clicked();
        let right_click = response.secondary_clicked();

        self.position = Some(Vec2::new(response.rect.min.x, response.rect.min.y));

        // NOTE: Only left click counts as "pressed".
        let delta_time_secs = context.delta_time_secs();
        self.update_state(context, hovered, left_click, right_click, delta_time_secs);

        // The label is a sprite path, e.g. "palette/housing"; its last part names the button.
        let name = self.label.rsplit('/').next().unwrap_or(&self.label);
        let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.as_str());
        let pressed = Some(serde_json::Value::Bool(self.is_pressed()));
        ui_sys.with_ui(|ui| immediate::record(ui_sys, ui, &response, "sprite_button", name, tooltip, true, pressed));

        if let Some(tooltip) = &self.tooltip {
            let show_tooltip = hovered && (!self.is_pressed() || self.show_tooltip_when_pressed);
            if show_tooltip {
                tooltip.draw(context);
            }
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiSpriteButton {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiSpriteButtonParams) -> Self {
        debug_assert!(!params.label.is_empty());
        debug_assert!(params.size.x > 0.0 && params.size.y > 0.0);
        debug_assert!(params.state_transition_secs >= 0.0);

        let textures = UiSpriteButtonTextures::load(PathRef::from_str(&params.label), context);
        let visual_state_transition_timer = CountdownTimer::new(params.state_transition_secs);

        Self {
            label: params.label,
            tooltip: params.tooltip,
            show_tooltip_when_pressed: params.show_tooltip_when_pressed,
            size: params.size,
            position: None, // Set after the first draw().
            textures,
            sounds_enabled: params.sounds_enabled,
            logical_state: params.initial_state,
            visual_state: params.initial_state,
            visual_state_transition_timer,
            state_transition_secs: params.state_transition_secs,
            on_state_changed: params.on_state_changed,
        }
    }

    pub fn position(&self) -> Vec2 {
        self.position.expect("Called UiSpriteButton::position() before first draw()!")
    }

    pub fn state(&self) -> UiSpriteButtonState {
        self.logical_state
    }

    pub fn is_idle(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Idle
    }

    pub fn is_disabled(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Disabled
    }

    pub fn is_enabled(&self) -> bool {
        self.logical_state != UiSpriteButtonState::Disabled
    }

    pub fn is_pressed(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Pressed
    }

    pub fn enable(&mut self, enable: bool) {
        if enable {
            if self.logical_state == UiSpriteButtonState::Disabled {
                self.logical_state = UiSpriteButtonState::Idle;
            }
        } else {
            self.logical_state = UiSpriteButtonState::Disabled;
        }
    }

    pub fn press(&mut self, press: bool) {
        if press {
            self.logical_state = UiSpriteButtonState::Pressed;
        } else if self.logical_state == UiSpriteButtonState::Pressed {
            self.logical_state = UiSpriteButtonState::Idle;
        }
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn play_sound(&mut self, context: &mut dyn UiWidgetContext, sound_key: UiSoundKey, new_state: UiSpriteButtonState) {
        // Play state transition sound if moving to a different state.
        if self.logical_state != new_state {
            let mut play = false;

            if sound_key == UiSoundKey::ButtonHovered && self.sounds_enabled.intersects(UiButtonSoundsEnabled::Hovered) {
                play = true;
            }

            if sound_key == UiSoundKey::ButtonPressed && self.sounds_enabled.intersects(UiButtonSoundsEnabled::Pressed) {
                play = true;
            }

            if play {
                sound::play(*context.sound_sys(), sound_key);
            }
        }
    }

    fn update_state(
        &mut self,
        context: &mut dyn UiWidgetContext,
        hovered: bool,
        left_click: bool,
        right_click: bool,
        delta_time_secs: Seconds,
    ) {
        let prev_state = self.logical_state;

        match self.logical_state {
            UiSpriteButtonState::Idle | UiSpriteButtonState::Hovered => {
                // Left click selects/presses button.
                if left_click {
                    self.play_sound(context, UiSoundKey::ButtonPressed, UiSpriteButtonState::Pressed);
                    self.logical_state = UiSpriteButtonState::Pressed;
                } else if hovered {
                    self.play_sound(context, UiSoundKey::ButtonHovered, UiSpriteButtonState::Hovered);
                    self.logical_state = UiSpriteButtonState::Hovered;
                } else {
                    self.logical_state = UiSpriteButtonState::Idle;
                }
            }
            UiSpriteButtonState::Pressed => {
                // Right click deselects/unpresses.
                if right_click {
                    self.logical_state = UiSpriteButtonState::Idle;
                }
            }
            UiSpriteButtonState::Disabled => {}
        }

        if left_click {
            // Reset transition if pressed.
            self.visual_state_transition_timer.reset(self.state_transition_secs);
        }

        if self.visual_state == UiSpriteButtonState::Pressed {
            // Run a timed transition between idle/hovered and pressed.
            if self.visual_state_transition_timer.tick(delta_time_secs) {
                self.visual_state_transition_timer.reset(self.state_transition_secs);
                self.visual_state = self.logical_state;
            }
        } else {
            self.visual_state = self.logical_state;
        }

        // Invoke on state change or re-click with left mouse button.
        if self.logical_state != prev_state || left_click {
            self.on_state_changed.invoke(self, context, prev_state);
        }
    }
}

// ----------------------------------------------
// UiSpriteButtonTextures
// ----------------------------------------------

struct UiSpriteButtonTextures {
    textures: [TextureHandle; UI_SPRITE_BUTTON_STATE_COUNT],
}

impl UiSpriteButtonTextures {
    fn unloaded() -> Self {
        Self { textures: [TextureHandle::invalid(); UI_SPRITE_BUTTON_STATE_COUNT] }
    }

    fn load(sprite_path: PathRef, context: &mut dyn UiWidgetContext) -> Self {
        let mut sprites = Self::unloaded();
        sprites.load_textures(sprite_path, context);
        sprites
    }

    fn load_textures(&mut self, sprite_path: PathRef, context: &mut dyn UiWidgetContext) {
        for state in UiSpriteButtonState::iter() {
            self.textures[state as usize] = state.load_texture(sprite_path, context);
        }
    }

    #[inline]
    fn are_textures_loaded(&self) -> bool {
        self.textures[0].is_valid()
    }

    #[inline]
    fn texture_for_state(&self, state: UiSpriteButtonState) -> TextureHandle {
        debug_assert!(self.textures[state as usize].is_valid());
        self.textures[state as usize]
    }
}

// ----------------------------------------------
// UiSpriteButtonState
// ----------------------------------------------

const UI_SPRITE_BUTTON_STATE_COUNT: usize = UiSpriteButtonState::COUNT;

#[derive(Copy, Clone, Default, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum UiSpriteButtonState {
    #[default]
    #[strum(props(Suffix = "idle"))]
    Idle,

    #[strum(props(Suffix = "disabled"))]
    Disabled,

    #[strum(props(Suffix = "hovered"))]
    Hovered,

    #[strum(props(Suffix = "pressed"))]
    Pressed,
}

impl UiSpriteButtonState {
    fn asset_path(self, sprite_path: PathRef) -> AssetPath {
        debug_assert!(!sprite_path.is_empty());
        let sprite_suffix = self.get_str("Suffix").unwrap();
        let sprite_name = format_fixed_string!(64, "{sprite_path}_{sprite_suffix}.png");
        AssetPath::from_str("buttons").join(sprite_name)
    }

    fn load_texture(self, sprite_path: PathRef, context: &mut dyn UiWidgetContext) -> TextureHandle {
        let asset_path = self.asset_path(sprite_path);
        context.load_texture((&asset_path).into())
    }
}

// ----------------------------------------------
// UiTooltipTextParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiTooltipTextParams<'a> {
    pub text: String,
    pub font_scale: UiFontScale,
    pub background: Option<PathRef<'a>>,
}

// ----------------------------------------------
// UiTooltipText
// ----------------------------------------------

#[derive(Clone)]
pub struct UiTooltipText {
    text: String,
    font_scale: UiFontScale,
    background: Option<TextureHandle>,
    placement: UiTooltipPlacement,
}

impl UiTooltipText {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiTooltipTextParams) -> Self {
        debug_assert!(!params.text.is_empty());
        debug_assert!(params.font_scale.is_valid());

        Self {
            text: params.text,
            font_scale: params.font_scale,
            background: params.background.map(|path| context.load_texture(path)),
            placement: UiTooltipPlacement::default(),
        }
    }

    // Where it shows: at the pointer (the default), or beside its widget.
    pub fn with_placement(mut self, placement: UiTooltipPlacement) -> Self {
        self.placement = placement;
        self
    }

    // For the widget drawn last.
    fn draw(&self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        super::custom_tooltip(context.ui_sys(), self.font_scale, self.background, self.placement, |ui_sys| {
            super::text(ui_sys, &self.text);
        });
    }
}

// ----------------------------------------------
// UiSeparatorParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSeparatorParams<'a> {
    pub separator: Option<PathRef<'a>>,
    pub size: Option<Vec2>,
    pub thickness: Option<f32>, // Optional thickness used if `size = None`.
    pub vertical: bool,         // Horizontal separator by default.
}

// ----------------------------------------------
// UiSeparator
// ----------------------------------------------

#[derive(Clone)]
pub struct UiSeparator {
    separator: Option<TextureHandle>,
    size: Option<Vec2>,
    thickness: f32,
    vertical: bool,
}

impl UiWidget for UiSeparator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let size = self.size.unwrap_or_else(|| internal::calc_separator_size(ui_sys, !self.vertical, self.thickness));

        ui_sys.with_ui(|ui| {
            // Empty space, optionally with a texture over it.
            let (rect, _) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            if let Some(separator) = self.separator {
                ui.painter().image(egui_texture(separator), rect, uv_full(), Color32::WHITE);
            }
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.size.unwrap_or_else(|| internal::calc_separator_size(context.ui_sys(), !self.vertical, self.thickness))
    }
}

impl UiSeparator {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiSeparatorParams) -> Self {
        Self {
            separator: params.separator.map(|path| context.load_texture(path)),
            size: params.size,
            thickness: params.thickness.unwrap_or(1.0),
            vertical: params.vertical,
        }
    }
}

// ----------------------------------------------
// UiSpriteIconParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSpriteIconParams<'a> {
    pub sprite: Option<PathRef<'a>>,
    pub tex_coords: RectTexCoords,
    pub size: Vec2,
    pub margin_top: f32, // Margin top can be negative.
    pub margin_bottom: f32,
    pub tooltip: Option<UiTooltipText>,
    pub clip_to_parent_menu: bool,
    pub unclipped_draw_size: Option<Vec2>, // Size to use for drawing if clip_to_parent_menu = false. Defaults to same as size.
    pub outline: bool,
}

// ----------------------------------------------
// UiSpriteIcon
// ----------------------------------------------

pub struct UiSpriteIcon {
    sprite: Option<TextureHandle>,
    tex_coords: RectTexCoords,
    size: Vec2,
    margin_top: f32,
    margin_bottom: f32,
    tooltip: Option<UiTooltipText>,
    clip_to_parent_menu: bool,
    unclipped_draw_size: Vec2,
    outline: bool,
}

impl UiWidget for UiSpriteIcon {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let screen_rect = ui_sys.ctx().content_rect();

        let hovered = ui_sys.with_ui(|ui| {
            if self.margin_top != 0.0 {
                ui.add_space(self.margin_top); // NOTE: May be negative.
            }

            // Hover detection for the tooltip.
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::hover());

            // Render the actual sprite icon:
            if let Some(sprite) = self.sprite {
                let uv = tex_coords_uv(&self.tex_coords);
                let outline_stroke = Stroke::new(1.0, Color32::BLACK);

                // Unclipped, the icon can overflow the parent window's bounds.
                let (painter, icon_rect) = if self.clip_to_parent_menu {
                    (ui.painter().clone(), rect)
                } else {
                    (ui.painter().with_clip_rect(screen_rect), egui::Rect::from_min_size(rect.min, egui_vec2(self.unclipped_draw_size)))
                };

                painter.image(egui_texture(sprite), icon_rect, uv, Color32::WHITE);

                if self.outline {
                    painter.rect_stroke(rect.expand(2.0), 2.0, outline_stroke, StrokeKind::Middle);
                }
            }

            if self.margin_bottom > 0.0 {
                ui.add_space(self.margin_bottom);
            }

            ui_sys.set_last_response(&response);
            response.hovered()
        });

        if let Some(tooltip) = &self.tooltip
            && hovered
        {
            tooltip.draw(context);
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }
}

impl UiSpriteIcon {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiSpriteIconParams) -> Self {
        debug_assert!(params.size.x > 0.0 && params.size.y > 0.0);
        debug_assert!(params.margin_bottom >= 0.0);

        Self {
            sprite: params.sprite.map(|path| context.load_texture(path)),
            tex_coords: params.tex_coords,
            size: params.size,
            margin_top: params.margin_top,
            margin_bottom: params.margin_bottom,
            tooltip: params.tooltip,
            clip_to_parent_menu: params.clip_to_parent_menu,
            unclipped_draw_size: params.unclipped_draw_size.unwrap_or(params.size),
            outline: params.outline,
        }
    }

    pub fn set_sprite(&mut self, sprite: TextureHandle) {
        self.sprite = Some(sprite);
    }

    pub fn set_tex_coords(&mut self, tex_coords: RectTexCoords) {
        self.tex_coords = tex_coords;
    }

    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }
}

// ----------------------------------------------
// UiSliderParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSliderParams<T> {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub min: T,
    pub max: T,
    pub on_read_value: UiSliderReadValue<T>,
    pub on_update_value: UiSliderUpdateValue<T>,
}

pub type UiSliderReadValue<T>   = UiWidgetCallback<UiSlider, UiReadOnly, T>;
pub type UiSliderUpdateValue<T> = UiWidgetCallbackWithArg<UiSlider, UiReadOnly, UiValue<T>>;

// ----------------------------------------------
// UiSlider
// ----------------------------------------------

pub struct UiSlider {
    label: String,
    font_scale: UiFontScale,
    value: UiSliderValue,
}

impl UiWidget for UiSlider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);

        match &self.value {
            UiSliderValue::I32 { min, max, on_read_value, on_update_value } => {
                let value = on_read_value.invoke(self, context).unwrap_or_default();
                let (new_value, set_value) = self.draw_slider(context, value, *min, *max, None);

                if let Some(new_value) = new_value {
                    on_update_value.invoke(self, context, new_value.clamp(*min, *max));
                }
                if let Some(new_value) = set_value.and_then(|value| value.as_i64()) {
                    on_update_value.invoke(self, context, new_value.clamp(i64::from(*min), i64::from(*max)) as i32);
                }
            }
            UiSliderValue::U32 { min, max, on_read_value, on_update_value } => {
                let value = on_read_value.invoke(self, context).unwrap_or_default();
                let (new_value, set_value) = self.draw_slider(context, value, *min, *max, None);

                if let Some(new_value) = new_value {
                    on_update_value.invoke(self, context, new_value.clamp(*min, *max));
                }
                if let Some(new_value) = set_value.and_then(|value| value.as_u64()) {
                    on_update_value.invoke(self, context, new_value.clamp(u64::from(*min), u64::from(*max)) as u32);
                }
            }
            UiSliderValue::F32 { min, max, on_read_value, on_update_value } => {
                let value = on_read_value.invoke(self, context).unwrap_or_default();
                let (new_value, set_value) = self.draw_slider(context, value, *min, *max, Some(2));

                if let Some(new_value) = new_value {
                    on_update_value.invoke(self, context, new_value.clamp(*min, *max));
                }
                if let Some(new_value) = set_value.and_then(|value| value.as_f64()) {
                    on_update_value.invoke(self, context, (new_value as f32).clamp(*min, *max));
                }
            }
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiSlider {
    pub fn new<T>(_context: &mut dyn UiWidgetContext, params: UiSliderParams<T>) -> Self
    where
        UiSliderParams<T>: IntoUiSliderValue,
    {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.clone().unwrap_or_default(),
            font_scale: params.font_scale,
            value: params.into_slider_value(),
        }
    }

    // A left label, then the slider. Returns the value the user set, if changed, and a value ui.set
    // queued for it.
    fn draw_slider<T: egui::emath::Numeric + serde::Serialize>(
        &self,
        context: &mut dyn UiWidgetContext,
        mut value: T,
        min: T,
        max: T,
        decimals: Option<usize>,
    ) -> (Option<T>, Option<serde_json::Value>) {
        let ui_sys = context.ui_sys();
        let width = internal::item_width(ui_sys);

        ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    // The value shows to the slider's right.
                    ui.spacing_mut().slider_width = (width - ui.spacing().interact_size.x - ui.spacing().item_spacing.x).max(40.0);
                    let mut slider = egui::Slider::new(&mut value, min..=max).clamping(egui::SliderClamping::Always);
                    if let Some(decimals) = decimals {
                        slider = slider.fixed_decimals(decimals);
                    }
                    ui.add(slider)
                })
                .inner;

            let changed = response.changed().then_some(value);
            let set_value = record_settable(ui_sys, ui, &response, "slider", &self.label, serde_json::json!(value));
            (changed, set_value)
        })
    }
}

// ----------------------------------------------
// UiSliderValue
// ----------------------------------------------

pub enum UiSliderValue {
    I32 { min: i32, max: i32, on_read_value: UiSliderReadValue<i32>, on_update_value: UiSliderUpdateValue<i32> },
    U32 { min: u32, max: u32, on_read_value: UiSliderReadValue<u32>, on_update_value: UiSliderUpdateValue<u32> },
    F32 { min: f32, max: f32, on_read_value: UiSliderReadValue<f32>, on_update_value: UiSliderUpdateValue<f32> },
}

// ----------------------------------------------
// IntoUiSliderValue Trait
// ----------------------------------------------

pub trait IntoUiSliderValue {
    fn into_slider_value(self) -> UiSliderValue;
}

impl IntoUiSliderValue for UiSliderParams<i32> {
    fn into_slider_value(self) -> UiSliderValue {
        UiSliderValue::I32 {
            min: self.min,
            max: self.max,
            on_read_value: self.on_read_value,
            on_update_value: self.on_update_value,
        }
    }
}

impl IntoUiSliderValue for UiSliderParams<u32> {
    fn into_slider_value(self) -> UiSliderValue {
        UiSliderValue::U32 {
            min: self.min,
            max: self.max,
            on_read_value: self.on_read_value,
            on_update_value: self.on_update_value,
        }
    }
}

impl IntoUiSliderValue for UiSliderParams<f32> {
    fn into_slider_value(self) -> UiSliderValue {
        UiSliderValue::F32 {
            min: self.min,
            max: self.max,
            on_read_value: self.on_read_value,
            on_update_value: self.on_update_value,
        }
    }
}

// ----------------------------------------------
// UiCheckboxParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiCheckboxParams {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub on_read_value: UiCheckboxReadValue,
    pub on_update_value: UiCheckboxUpdateValue,
}

pub type UiCheckboxReadValue   = UiWidgetCallback<UiCheckbox, UiReadOnly, bool>;
pub type UiCheckboxUpdateValue = UiWidgetCallbackWithArg<UiCheckbox, UiReadOnly, UiValue<bool>>;

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub struct UiCheckbox {
    label: String,
    font_scale: UiFontScale,
    on_read_value: UiCheckboxReadValue,
    on_update_value: UiCheckboxUpdateValue,
}

impl UiWidget for UiCheckbox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();

        let mut value = self.on_read_value.invoke(self, context).unwrap_or_default();

        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);
                    ui.checkbox(&mut value, "")
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "checkbox", &self.label, serde_json::Value::Bool(value));
            (response.changed(), set_value)
        });

        if value_changed {
            self.on_update_value.invoke(self, context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_bool()) {
            self.on_update_value.invoke(self, context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let spacing = &ui_sys.egui_style().spacing;

        let checkbox_square = internal::calc_frame_height(ui_sys, self.font_scale);
        let mut width = checkbox_square;

        if !self.label.is_empty() {
            let label_size = context.calc_text_size(self.font_scale, &self.label);
            width += spacing.icon_spacing + label_size.x;
        }

        Vec2::new(width, checkbox_square)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiCheckbox {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiCheckboxParams) -> Self {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.unwrap_or_default(),
            font_scale: params.font_scale,
            on_read_value: params.on_read_value,
            on_update_value: params.on_update_value,
        }
    }
}

// ----------------------------------------------
// UiIntInputParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiIntInputParams {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub step: Option<i32>,
    pub on_read_value: UiIntInputReadValue,
    pub on_update_value: UiIntInputUpdateValue,
}

pub type UiIntInputReadValue   = UiWidgetCallback<UiIntInput, UiReadOnly, i32>;
pub type UiIntInputUpdateValue = UiWidgetCallbackWithArg<UiIntInput, UiReadOnly, UiValue<i32>>;

// ----------------------------------------------
// UiIntInput
// ----------------------------------------------

pub struct UiIntInput {
    label: String,
    font_scale: UiFontScale,
    min: i32,
    max: i32,
    step: i32,
    on_read_value: UiIntInputReadValue,
    on_update_value: UiIntInputUpdateValue,
}

impl UiWidget for UiIntInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let width = internal::item_width(ui_sys);

        let mut value = self.on_read_value.invoke(self, context).unwrap_or_default();

        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let (response, stepped) = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    // A field to drag or type into, and buttons that step it.
                    let buttons_width = (ui.spacing().interact_size.y + ui.spacing().item_spacing.x) * 2.0;
                    let drag = egui::DragValue::new(&mut value).range(self.min..=self.max).speed(f64::from(self.step) * 0.1);
                    let response = ui.add_sized(vec2((width - buttons_width).max(40.0), ui.spacing().interact_size.y), drag);

                    let mut stepped = false;
                    if ui.small_button("-").clicked() {
                        value = value.saturating_sub(self.step);
                        stepped = true;
                    }
                    if ui.small_button("+").clicked() {
                        value = value.saturating_add(self.step);
                        stepped = true;
                    }
                    (response, stepped)
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "int_input", &self.label, serde_json::json!(value));
            (response.changed() || stepped, set_value)
        });

        if value_changed {
            value = value.clamp(self.min, self.max);
            self.on_update_value.invoke(self, context, value);
        }
        if let Some(new_value) = set_value.and_then(|value| value.as_i64()) {
            let new_value = new_value.clamp(i64::from(self.min), i64::from(self.max)) as i32;
            self.on_update_value.invoke(self, context, new_value);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiIntInput {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiIntInputParams) -> Self {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.unwrap_or_default(),
            font_scale: params.font_scale,
            min: params.min.unwrap_or(i32::MIN),
            max: params.max.unwrap_or(i32::MAX),
            step: params.step.unwrap_or(1),
            on_read_value: params.on_read_value,
            on_update_value: params.on_update_value,
        }
    }
}

// ----------------------------------------------
// UiTextInputParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiTextInputParams {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub on_read_value: UiTextInputReadValue,
    pub on_update_value: UiTextInputUpdateValue,
}

pub type UiTextInputReadValue   = UiWidgetCallback<UiTextInput, UiReadOnly, UiStrRef>;
pub type UiTextInputUpdateValue = UiWidgetCallbackWithArg<UiTextInput, UiReadOnly, UiStrRef>;

// ----------------------------------------------
// UiTextInput
// ----------------------------------------------

pub struct UiTextInput {
    label: String,
    buffer: String,
    font_scale: UiFontScale,
    on_read_value: UiTextInputReadValue,
    on_update_value: UiTextInputUpdateValue,
}

impl UiWidget for UiTextInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let width = internal::item_width(ui_sys);

        self.buffer.clear();
        if let Some(value) = self.on_read_value.invoke(self, context) {
            self.buffer.push_str(&value.0);
        }

        let id = widget_id(self, &self.label);
        let (value_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);
                    let text_edit = egui::TextEdit::singleline(&mut self.buffer)
                        .id(id)
                        .font(internal::font(ui_sys, self.font_scale))
                        .desired_width(width);
                    ui.add(text_edit)
                })
                .inner;

            let set_value = record_settable(ui_sys, ui, &response, "text_input", &self.label, serde_json::json!(self.buffer));
            (response.changed(), set_value)
        });

        if value_changed {
            self.on_update_value.invoke(self, context, &self.buffer);
        }
        if let Some(new_value) = set_value.as_ref().and_then(|value| value.as_str()) {
            self.buffer.clear();
            self.buffer.push_str(new_value);
            self.on_update_value.invoke(self, context, &self.buffer);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiTextInput {
    pub fn new(_context: &mut dyn UiWidgetContext, params: UiTextInputParams) -> Self {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.unwrap_or_default(),
            buffer: String::new(),
            font_scale: params.font_scale,
            on_read_value: params.on_read_value,
            on_update_value: params.on_update_value,
        }
    }
}

// ----------------------------------------------
// UiDropdownParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiDropdownParams<T> {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub current_item: usize,
    pub items: Vec<T>, // Must not be empty.
    pub on_selection_changed: UiDropdownSelectionChanged,
    pub on_get_current_selection: UiDropdownGetCurrentSelection, // Optional.
}

pub type UiDropdownSelectionChanged    = UiWidgetCallback<UiDropdown, UiReadOnly>;
pub type UiDropdownGetCurrentSelection = UiWidgetCallback<UiDropdown, UiReadOnly, usize>;

// ----------------------------------------------
// UiDropdown
// ----------------------------------------------

pub struct UiDropdown {
    label: String,
    font_scale: UiFontScale,
    current_item: usize,
    items: Vec<String>,
    on_selection_changed: UiDropdownSelectionChanged,
    on_get_current_selection: UiDropdownGetCurrentSelection,
}

impl UiWidget for UiDropdown {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();
        let width = internal::item_width(ui_sys);

        if !self.on_get_current_selection.is_none() {
            self.current_item = self.on_get_current_selection.invoke(self, context).unwrap();
        }

        let id = widget_id(self, &self.label);
        let mut current_item = self.current_item;

        let (selection_changed, set_value) = ui_sys.with_ui(|ui| {
            let response = ui
                .horizontal(|ui| {
                    left_label(ui_sys, ui, &self.label);

                    let font = internal::font(ui_sys, self.font_scale);
                    let selected_text = self.items.get(current_item).map_or("", String::as_str);

                    egui::ComboBox::from_id_salt(id)
                        .width(width)
                        .selected_text(egui::RichText::new(selected_text).font(font.clone()))
                        .show_ui(ui, |ui| {
                            // Its items, recorded under its label while it's open.
                            let _path = immediate::path_scope(ui_sys, &self.label);
                            for (index, item) in self.items.iter().enumerate() {
                                let response = ui.selectable_value(&mut current_item, index, egui::RichText::new(item).font(font.clone()));
                                immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(current_item == index)));
                            }
                        })
                        .response
                })
                .inner;

            let current = self.items.get(current_item).cloned().unwrap_or_default();
            let set_value = record_settable(ui_sys, ui, &response, "dropdown", &self.label, serde_json::Value::String(current));
            (current_item != self.current_item, set_value)
        });

        if selection_changed {
            self.current_item = current_item;
            self.on_selection_changed.invoke(self, context);
        }
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)) {
            self.current_item = index;
            self.on_selection_changed.invoke(self, context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        internal::calc_labeled_widget_size(context.ui_sys(), self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiDropdown {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiDropdownParams<String>) -> Self {
        Self::with_strings(context, params)
    }

    pub fn with_strings(_context: &mut dyn UiWidgetContext, params: UiDropdownParams<String>) -> Self {
        debug_assert!(params.font_scale.is_valid());
        debug_assert!(!params.items.is_empty());
        debug_assert!(params.current_item < params.items.len());

        Self {
            label: params.label.unwrap_or_default(),
            font_scale: params.font_scale,
            current_item: params.current_item,
            items: params.items,
            on_selection_changed: params.on_selection_changed,
            on_get_current_selection: params.on_get_current_selection,
        }
    }

    // From array of values implementing Display.
    pub fn with_values<T>(context: &mut dyn UiWidgetContext, params: UiDropdownParams<T>) -> Self
    where
        T: Display,
    {
        let item_strings: Vec<String> = params.items.iter().map(|item| item.to_string()).collect();

        Self::with_strings(context, UiDropdownParams {
            label: params.label,
            font_scale: params.font_scale,
            current_item: params.current_item,
            items: item_strings,
            on_selection_changed: params.on_selection_changed,
            on_get_current_selection: params.on_get_current_selection,
        })
    }

    pub fn current_selection_index(&self) -> usize {
        self.current_item
    }

    pub fn current_selection(&self) -> &str {
        &self.items[self.current_item]
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn add_item(&mut self, item: String) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn reset_items(&mut self, current_item: usize, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
    }

    pub fn reset_items_with<V, ToString>(&mut self, values: &[V], current_item: usize, to_str: ToString)
    where
        ToString: Fn(&V) -> String,
    {
        let items: Vec<String> = values.iter().map(to_str).collect();

        self.reset_items(current_item, items);
    }
}

// ----------------------------------------------
// UiItemListFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiItemListFlags: u8 {
        const Border         = 1 << 0;
        const Scrollable     = 1 << 1;
        const Scrollbars     = 1 << 2;
        const TextInputField = 1 << 3;
    }
}

// ----------------------------------------------
// UiItemListParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiItemListParams<T> {
    pub label: Option<String>,
    pub font_scale: UiFontScale,
    pub size: Option<Vec2>,
    pub margin_left: f32,
    pub margin_right: f32,
    pub flags: UiItemListFlags,
    pub current_item: Option<usize>,
    pub items: Vec<T>, // Can be empty.
    pub on_selection_changed: UiItemListSelectionChanged,
}

pub type UiItemListSelectionChanged = UiWidgetCallback<UiItemList, UiReadOnly>;

// ----------------------------------------------
// UiItemList
// ----------------------------------------------

pub struct UiItemList {
    label: String,
    font_scale: UiFontScale,
    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,
    flags: UiItemListFlags,
    current_item: Option<usize>,
    items: Vec<String>,
    text_input_field_buffer: Option<String>,
    on_selection_changed: UiItemListSelectionChanged,
}

impl UiWidget for UiItemList {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.set_window_font_scale(self.font_scale);
        let ui_sys = context.ui_sys();

        // The list's size:
        //  > 0.0 -> fixed size
        //  = 0.0 -> use remaining host window size
        //  < 0.0 -> use remaining host window size minus abs(size)
        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);
            requested_size.x -= self.margin_right - padding;
        }

        let id = widget_id(self, &self.label);
        let font = internal::font(ui_sys, self.font_scale);
        let margin_left = self.margin_left;
        let border = self.flags.intersects(UiItemListFlags::Border);
        let scrollable = self.flags.intersects(UiItemListFlags::Scrollable);
        let scroll_bars = self.flags.intersects(UiItemListFlags::Scrollbars);

        let mut current_item = self.current_item;
        let mut text_input_field_changed = false;

        let list_response = ui_sys.with_ui(|ui| {
            let left = ui.cursor().left() + margin_left;

            // Optional label:
            if !self.label.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(margin_left);
                    ui.label(egui::RichText::new(&self.label).font(font.clone()));
                });
            }

            let available = immediate::fill_size(ui);
            let resolve = |requested: f32, available: f32| {
                if requested > 0.0 { requested } else { (available + requested).max(0.0) }
            };
            let width = resolve(requested_size.x, available.x - margin_left);

            // Optional text input field:
            if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
                let response = ui
                    .horizontal(|ui| {
                        ui.add_space(margin_left);
                        ui.add(egui::TextEdit::singleline(text_input_field_buffer).id(id.with("input_field")).font(font.clone()).desired_width(width))
                    })
                    .inner;
                text_input_field_changed = response.changed();
            }

            let height = resolve(requested_size.y, immediate::fill_size(ui).y);
            let rect = egui::Rect::from_min_size(pos2(left, ui.cursor().top()), vec2(width, height));
            let list_response = ui.allocate_rect(rect, Sense::hover());

            let frame = if border {
                egui::Frame::group(ui.style()).inner_margin(ui.spacing().item_spacing.x * 0.5)
            } else {
                egui::Frame::NONE
            };

            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            frame.show(&mut list_ui, |ui| {
                ui.set_min_size((rect.size() - frame.total_margin().sum()).max(egui::Vec2::ZERO));

                let scroll_area = egui::ScrollArea::vertical()
                    .id_salt(id)
                    .auto_shrink([false, false])
                    .vscroll(scrollable)
                    .scroll_bar_visibility(if scroll_bars {
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                    } else {
                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
                    });

                scroll_area.show(ui, |ui| {
                    let _path = immediate::path_scope(ui_sys, &self.label);
                    ui.style_mut().spacing.item_spacing.y = 0.0;
                    // Rows span the list, with their text on the left.
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        for (index, item) in self.items.iter().enumerate() {
                            let is_selected = current_item == Some(index);
                            let text = egui::RichText::new(item).font(font.clone());
                            let response = ui.add(egui::Button::selectable(is_selected, text));
                            if response.clicked() {
                                current_item = Some(index);
                            }
                            immediate::record(ui_sys, ui, &response, "list_item", item, None, true, Some(serde_json::Value::Bool(is_selected)));
                        }
                    });
                });
            });

            list_response
        });

        if text_input_field_changed && self.text_input_field_buffer.is_some() {
            current_item = None;
            self.current_item = None;
            self.on_selection_changed.invoke(self, context);
        }

        if current_item != self.current_item
            && let Some(selected_index) = current_item
        {
            self.current_item = current_item;
            self.select_item(context, selected_index);
        }

        // ui.set on the list selects an item, by text or index.
        let current = self.current_item.and_then(|index| self.items.get(index)).cloned();
        let set_value = ui_sys.with_ui(|ui| {
            record_settable(ui_sys, ui, &list_response, "item_list", &self.label, current.map_or(serde_json::Value::Null, serde_json::Value::String))
        });
        if let Some(index) = set_value.and_then(|value| list_index(&value, &self.items)) {
            self.current_item = Some(index);
            self.select_item(context, index);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - padding;
        }

        let size = internal::calc_child_window_size(ui_sys, requested_size);

        let input_field_height =
            if self.text_input_field_buffer.is_some() { internal::calc_frame_height(ui_sys, self.font_scale) } else { 0.0 };

        Vec2::new(size.x, size.y + input_field_height)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiItemList {
    fn select_item(&mut self, context: &mut dyn UiWidgetContext, index: usize) {
        if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
            text_input_field_buffer.clear();
            text_input_field_buffer.push_str(&self.items[index]);
        }

        self.on_selection_changed.invoke(self, context);
    }

    pub fn new(context: &mut dyn UiWidgetContext, params: UiItemListParams<String>) -> Self {
        Self::with_strings(context, params)
    }

    pub fn with_strings(_context: &mut dyn UiWidgetContext, params: UiItemListParams<String>) -> Self {
        debug_assert!(params.font_scale.is_valid());
        debug_assert!(params.margin_left >= 0.0);
        debug_assert!(params.margin_right >= 0.0);

        let text_input_field_buffer = {
            if params.flags.intersects(UiItemListFlags::TextInputField) {
                if let Some(initial_item) = params.current_item {
                    Some(params.items[initial_item].clone())
                } else {
                    Some(String::new())
                }
            } else {
                None
            }
        };

        Self {
            label: params.label.unwrap_or_default(),
            font_scale: params.font_scale,
            size: params.size,
            margin_left: params.margin_left,
            margin_right: params.margin_right,
            flags: params.flags,
            current_item: params.current_item,
            items: params.items,
            text_input_field_buffer,
            on_selection_changed: params.on_selection_changed,
        }
    }

    // From array of values implementing Display.
    pub fn with_values<T>(context: &mut dyn UiWidgetContext, params: UiItemListParams<T>) -> Self
    where
        T: Display,
    {
        let item_strings: Vec<String> = params.items.iter().map(|item| item.to_string()).collect();

        Self::with_strings(context, UiItemListParams {
            label: params.label,
            font_scale: params.font_scale,
            size: params.size,
            margin_left: params.margin_left,
            margin_right: params.margin_right,
            flags: params.flags,
            current_item: params.current_item,
            items: item_strings,
            on_selection_changed: params.on_selection_changed,
        })
    }

    pub fn current_text_input_field(&self) -> Option<&str> {
        self.text_input_field_buffer.as_deref()
    }

    pub fn current_selection_index(&self) -> Option<usize> {
        self.current_item
    }

    pub fn current_selection(&self) -> Option<&str> {
        self.current_item.map(|index| self.items[index].as_str())
    }

    pub fn clear_selection(&mut self) {
        self.current_item = None;

        if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
            text_input_field_buffer.clear();
        }
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn add_item(&mut self, item: String) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn reset_items(&mut self, current_item: Option<usize>, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
    }

    pub fn reset_text_input_field(&mut self, value: String) {
        self.current_item = None;
        let buffer = self.text_input_field_buffer.as_mut().unwrap();
        *buffer = value;
    }

    pub fn reset_items_with<V, ToString>(&mut self, values: &[V], current_item: Option<usize>, to_str: ToString)
    where
        ToString: Fn(&V) -> String,
    {
        let items: Vec<String> = values.iter().map(to_str).collect();

        self.reset_items(current_item, items);
    }
}

// ----------------------------------------------
// UiMessageBoxParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiMessageBoxParams<'a> {
    pub label: Option<String>,
    pub size: Option<Vec2>,
    pub background: Option<PathRef<'a>>,
    pub content_insets: Option<UiMenuInsets>, // The background's borders; the style's window margin if None.
    pub contents: Vec<UiWidgetImpl>,
    pub buttons: Vec<UiWidgetImpl>,
}

// ----------------------------------------------
// UiMessageBox
// ----------------------------------------------

#[derive(Default)]
pub struct UiMessageBox {
    menu: Option<UiMenuRcMut>,
}

impl UiWidget for UiMessageBox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        if let Some(menu) = &self.menu
            && menu.is_open()
        {
            // NOTE: Increment the ref count here.
            // draw() may trigger a UiMessageBox::reset, which could drop `self.menu`.
            let mut strong_ref = menu.clone();
            strong_ref.draw(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.menu.as_ref().map_or(Vec2::zero(), |menu| menu.measure(context))
    }

    fn label(&self) -> &str {
        self.menu.as_ref().map_or("", |menu| menu.label())
    }

    fn font_scale(&self) -> UiFontScale {
        self.menu.as_ref().map_or(UiFontScale::default(), |menu| menu.font_scale())
    }
}

impl UiMessageBox {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.is_open())
    }

    // Message box menu is lazily created on first call to open.
    // Subsequent calls will reuse the same menu until reset.
    pub fn open<'a, F>(&mut self, context: &mut dyn UiWidgetContext, mut params_fn: F)
    where
        F: FnMut(&mut dyn UiWidgetContext) -> UiMessageBoxParams<'a>,
    {
        if let Some(menu) = &mut self.menu {
            // Reuse existing message box.
            menu.open(context);
            return;
        }

        // Create new message box:
        let params = params_fn(context);

        let mut menu = UiMenu::new(context, UiMenuParams {
            label: params.label,
            flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter | UiMenuFlags::Modal | UiMenuFlags::CloseModalOnEscape,
            size: params.size,
            background: params.background,
            content_insets: params.content_insets,
            ..Default::default()
        });

        for widget in params.contents {
            menu.add_widget(widget);
        }

        if !params.buttons.is_empty() {
            let mut button_group = UiWidgetGroup::new(context, UiWidgetGroupParams {
                widget_spacing: Vec2::new(10.0, 10.0),
                center_vertically: true,
                center_horizontally: true,
                stack_vertically: false, // Render buttons side-by-side.
                ..Default::default()
            });

            for button in params.buttons {
                button_group.add_widget(button);
            }

            menu.add_widget(button_group);
        }

        self.menu = Some(menu);
    }

    pub fn close(&mut self, context: &mut dyn UiWidgetContext) {
        if let Some(menu) = &mut self.menu {
            menu.close(context);
        }
    }

    pub fn reset(&mut self) {
        self.menu = None;
    }
}

// ----------------------------------------------
// UiSlideshowFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiSlideshowFlags: u8 {
        const Fullscreen = 1 << 0;
        const PlayedOnce = 1 << 1; // Finished playing at least once.
        const Looping    = 1 << 2; // Started playing again with one of UiSlideshowLoopMode.
    }
}

// ----------------------------------------------
// UiSlideshowLoopMode
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub enum UiSlideshowLoopMode {
    #[default]
    None,               // Doesn't loop.
    WholeAnim,          // Loop whole anim from start to finish.
    FramesFromEnd(u32), // Loop between these many frames from the end (frame count - N).
}

// ----------------------------------------------
// UiSlideshowParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSlideshowParams {
    pub flags: UiSlideshowFlags,
    pub loop_mode: UiSlideshowLoopMode,
    pub frame_duration_secs: Seconds,
    pub frames: Vec<AssetPath>,

    // Ignored if UiSlideshowFlags::Fullscreen is set.
    pub size: Option<Vec2>,
    pub margin_left: f32,
    pub margin_right: f32,
}

// ----------------------------------------------
// UiSlideshow
// ----------------------------------------------

pub struct UiSlideshow {
    flags: UiSlideshowFlags,
    loop_mode: UiSlideshowLoopMode,

    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,

    frames: Vec<TextureHandle>,
    frame_index: usize,
    frame_duration_secs: Seconds,
    frame_play_time_secs: Seconds,
}

impl UiWidget for UiSlideshow {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        self.update_anim(context.delta_time_secs());
        self.draw_current_frame(context);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        internal::calc_child_window_size(ui_sys, self.requested_size(ui_sys))
    }
}

impl UiSlideshow {
    pub fn new(context: &mut dyn UiWidgetContext, params: UiSlideshowParams) -> Self {
        debug_assert!(!params.frames.is_empty());
        debug_assert!(params.margin_left >= 0.0);
        debug_assert!(params.margin_right >= 0.0);

        let mut frames = Vec::with_capacity(params.frames.len());
        for path in &params.frames {
            frames.push(context.load_texture(path.into()));
        }

        let mut flags = params.flags;
        if frames.len() <= 1 {
            // Static background (single-frame). Mark as if already played once.
            flags |= UiSlideshowFlags::PlayedOnce;
        } else {
            debug_assert!(params.frame_duration_secs > 0.0);
        }

        Self {
            flags,
            loop_mode: params.loop_mode,
            size: params.size,
            margin_left: params.margin_left,
            margin_right: params.margin_right,
            frames,
            frame_index: 0,
            frame_duration_secs: params.frame_duration_secs,
            frame_play_time_secs: 0.0,
        }
    }

    pub fn has_flags(&self, flags: UiSlideshowFlags) -> bool {
        self.flags.intersects(flags)
    }

    // ----------------------
    // Internal:
    // ----------------------

    // The size it asks for:
    //  > 0.0 -> fixed size
    //  = 0.0 -> use remaining host window size
    //  < 0.0 -> use remaining host window size minus abs(size)
    fn requested_size(&self, ui_sys: &UiSystem) -> Vec2 {
        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);
            requested_size.x -= self.margin_right - padding;
        }
        requested_size
    }

    fn update_anim(&mut self, delta_time_secs: Seconds) {
        if self.frames.len() <= 1 {
            // Static background (single-frame).
            return;
        }

        if self.has_flags(UiSlideshowFlags::PlayedOnce) && !self.has_flags(UiSlideshowFlags::Looping) {
            // Already played once and not looping. Early out.
            return;
        }

        // Advance animation:
        self.frame_play_time_secs += delta_time_secs;

        if self.frame_play_time_secs >= self.frame_duration_secs {
            if self.frame_index < self.frames.len() - 1 {
                // Move to next frame.
                self.frame_index += 1;
            } else {
                // Played the whole anim.
                self.flags.insert(UiSlideshowFlags::PlayedOnce);

                match self.loop_mode {
                    UiSlideshowLoopMode::WholeAnim => {
                        self.frame_index = 0; // Restart from beginning.
                        self.flags.insert(UiSlideshowFlags::Looping);
                    }
                    UiSlideshowLoopMode::FramesFromEnd(count) => {
                        self.frame_index = self.frames.len() - (count as usize); // Loop the last `count` frames.
                        self.flags.insert(UiSlideshowFlags::Looping);
                    }
                    UiSlideshowLoopMode::None => {} // Doesn't loop.
                }
            }

            // Reset the clock.
            self.frame_play_time_secs = 0.0;
        }
    }

    fn draw_current_frame(&mut self, context: &mut dyn UiWidgetContext) {
        let current_frame = self.frames[self.frame_index];
        let ui_sys = context.ui_sys();

        if context.is_inside_widget_window() && !self.has_flags(UiSlideshowFlags::Fullscreen) {
            // We are drawing inside a parent window: the frame takes its room in it.
            let size = internal::calc_child_window_size(ui_sys, self.requested_size(ui_sys));
            let margin_left = self.margin_left;

            ui_sys.with_ui(|ui| {
                let rect = egui::Rect::from_min_size(ui.cursor().min + vec2(margin_left, 0.0), egui_vec2(size));
                ui.painter().image(egui_texture(current_frame), rect, uv_full(), Color32::WHITE);
                ui.advance_cursor_after_rect(rect);
            });
        } else {
            // Draw full-screen rectangle with the anim frame texture, behind any other UI.
            let screen_rect = ui_sys.ctx().content_rect();
            let painter = ui_sys.ctx().layer_painter(egui::LayerId::background());
            painter.image(egui_texture(current_frame), screen_rect, uv_full(), Color32::WHITE);
        }
    }
}
