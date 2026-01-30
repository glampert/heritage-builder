#![allow(clippy::enum_variant_names)]
#![allow(clippy::type_complexity)]

use std::{any::Any, fmt::Display, path::PathBuf};
use std::rc::{Rc, Weak};

use bitflags::bitflags;
use arrayvec::ArrayString;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    helpers,
    UiSystem,
    UiFontScale,
    UiTextureHandle,
    assets_path,
    custom_tooltip,
    INVALID_UI_TEXTURE_HANDLE,
};

use crate::{
    bitflags_with_display,
    game::{sim::Simulation, world::World},
    engine::{Engine, time::{CountdownTimer, Seconds}},
    render::{RenderSystem, TextureHandle, TextureCache},
    utils::{Rect, Size, Vec2, mem::{self, Mutable, RawPtr}},
};

// ----------------------------------------------
// Macros: make_imgui_id / make_imgui_labeled_id
// ----------------------------------------------

macro_rules! make_imgui_id {
    ($self:expr, $widget_type:ty, $widget_label:expr) => {{
        if $self.imgui_id.is_empty() {
            // Compute id once and cache it.
            $self.imgui_id = {
                if $widget_label.is_empty() {
                    // NOTE: Use widget memory address as unique id if no label.
                    format!("##{} @ {:p}", stringify!($widget_type), $self)
                } else {
                    $widget_label.clone()
                }
            };
        }
        // Use cached id.
        &$self.imgui_id
    }}
}

macro_rules! make_imgui_labeled_id {
    ($self:expr, $widget_type:ty, $widget_label:expr) => {{
        if $self.imgui_id.is_empty() {
            // Compute id once and cache it, prefixed by the widget label.
            // (Use widget memory address as unique id).
            debug_assert!(!$widget_label.is_empty());
            $self.imgui_id = format!("{}##{} @ {:p}", $widget_label, stringify!($widget_type), $self);
        }
        // Use cached id.
        &$self.imgui_id
    }}
}

// ----------------------------------------------
// UiWidgetContext
// ----------------------------------------------

pub struct UiWidgetContext<'game> {
    pub sim: &'game mut Simulation,
    pub world: &'game World,

    pub ui_sys: &'game UiSystem,
    pub render_sys: &'game mut dyn RenderSystem,
    pub tex_cache: &'game mut dyn TextureCache,

    pub viewport_size: Size,
    pub delta_time_secs: Seconds,
    pub cursor_screen_pos: Vec2,

    in_window_count: u32,
}

impl<'game> UiWidgetContext<'game> {
    #[inline]
    pub fn new(sim: &'game mut Simulation,
               world: &'game World,
               engine: &'game dyn Engine) -> Self {
        Self {
            sim,
            world,
            ui_sys: engine.ui_system(),
            render_sys: engine.render_system(),
            tex_cache: engine.texture_cache(),
            viewport_size: engine.viewport().integer_size(),
            delta_time_secs: engine.frame_clock().delta_time(),
            cursor_screen_pos: engine.input_system().cursor_pos(),
            in_window_count: 0,
        }
    }

    #[inline]
    fn begin_widget_window(&mut self) {
        self.in_window_count += 1;
    }

    #[inline]
    fn end_widget_window(&mut self) {
        debug_assert!(self.is_inside_widget_window());
        self.in_window_count -= 1;

        // Restore default font scale when ending a window.
        self.ui_sys.set_font_scale(UiFontScale::default());
    }

    #[inline]
    fn is_inside_widget_window(&self) -> bool {
        self.in_window_count != 0
    }

    #[inline]
    pub fn load_texture(&mut self, path: &str) -> TextureHandle {
        let file_path = super::assets_path().join(path);
        self.tex_cache.load_texture_with_settings(
            file_path.to_str().unwrap(),
            Some(super::texture_settings()))
    }

    #[inline]
    pub fn load_ui_texture(&mut self, path: &str) -> UiTextureHandle {
        let tex_handle = self.load_texture(path);
        self.ui_sys.to_ui_texture(self.tex_cache, tex_handle)
    }
}

// ----------------------------------------------
// UiWidgetCallback
// ----------------------------------------------

#[derive(Default)]
pub enum UiWidgetCallback<Widget, Output = ()>
    where Widget: UiWidget
{
    #[default]
    None,

    // With plain function pointer, no capture, no memory allocation.
    Fn(fn(&mut Widget, &mut UiWidgetContext) -> Output),

    // With closure/capture. Allocates memory, most flexible.
    Closure(Box<dyn Fn(&mut Widget, &mut UiWidgetContext) -> Output + 'static>),
}

impl<Widget, Output> UiWidgetCallback<Widget, Output>
    where Widget: UiWidget
{
    pub fn with_fn(f: fn(&mut Widget, &mut UiWidgetContext) -> Output) -> Self {
        Self::Fn(f)
    }

    pub fn with_closure<C>(c: C) -> Self
        where C: Fn(&mut Widget, &mut UiWidgetContext) -> Output + 'static
    {
        Self::Closure(Box::new(c))
    }

    #[inline]
    fn invoke(&self, widget: &Widget, context: &mut UiWidgetContext) -> Option<Output> {
        match self {
            Self::Fn(f) => {
                Some(f(mem::mut_ref_cast(widget), context))
            }
            Self::Closure(c) => {
                Some(c(mem::mut_ref_cast(widget), context))
            }
            Self::None => None,
        }
    }
}

// ----------------------------------------------
// UiWidgetCallbackWithArg
// ----------------------------------------------

#[derive(Default)]
pub enum UiWidgetCallbackWithArg<Widget, Arg, Output = ()>
    where Widget: UiWidget
{
    #[default]
    None,

    // With plain function pointer, no capture, no memory allocation.
    Fn(fn(&mut Widget, &mut UiWidgetContext, Arg) -> Output),

    // With closure/capture. Allocates memory, most flexible.
    Closure(Box<dyn Fn(&mut Widget, &mut UiWidgetContext, Arg) -> Output + 'static>),
}

impl<Widget, Arg, Output> UiWidgetCallbackWithArg<Widget, Arg, Output>
    where Widget: UiWidget
{
    pub fn with_fn(f: fn(&mut Widget, &mut UiWidgetContext, Arg) -> Output) -> Self {
        Self::Fn(f)
    }

    pub fn with_closure<C>(c: C) -> Self
        where C: Fn(&mut Widget, &mut UiWidgetContext, Arg) -> Output + 'static
    {
        Self::Closure(Box::new(c))
    }

    #[inline]
    fn invoke(&self, widget: &Widget, context: &mut UiWidgetContext, arg: Arg) -> Option<Output> {
        match self {
            Self::Fn(f) => {
                Some(f(mem::mut_ref_cast(widget), context, arg))
            }
            Self::Closure(c) => {
                Some(c(mem::mut_ref_cast(widget), context, arg))
            }
            Self::None => None,
        }
    }
}

// ----------------------------------------------
// UiWidget / UiWidgetImpl
// ----------------------------------------------

#[enum_dispatch(UiWidgetImpl)]
pub trait UiWidget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any {
        mem::mut_ref_cast(self.as_any())
    }

    fn draw(&mut self, context: &mut UiWidgetContext);
    fn measure(&self, context: &UiWidgetContext) -> Vec2;

    fn label(&self) -> &str {
        ""
    }

    fn font_scale(&self) -> UiFontScale {
        UiFontScale::default()
    }
}

#[enum_dispatch]
pub enum UiWidgetImpl {
    UiMenu,
    UiMenuHeading,
    UiWidgetGroup,
    UiLabeledWidgetGroup,
    UiTextButton,
    UiSpriteButton,
    UiSeparator,
    UiSlider,
    UiCheckbox,
    UiTextInput,
    UiDropdown,
    UiItemList,
    UiMessageBox,
    UiSlideshow,
}

// ----------------------------------------------
// UiMenu
// ----------------------------------------------

pub struct UiMenu {
    label: String,
    imgui_id: String,
    flags: UiMenuFlags,
    size: Option<Vec2>,
    position: UiMenuPosition,
    background: Option<UiTextureHandle>,
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Vec2,
    message_box: UiMessageBox,
    on_open_close: UiMenuOpenClose,
}

pub type UiMenuStrongRef = Rc<Mutable<UiMenu>>;
pub type UiMenuWeakRef   = Weak<Mutable<UiMenu>>;

pub type UiMenuOpenClose    = UiWidgetCallbackWithArg<UiMenu, bool>;
pub type UiMenuCalcPosition = UiWidgetCallback<UiMenu, Vec2>;

impl UiWidget for UiMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        let mut is_open = self.is_open();

        if !is_open {
            return;
        }

        let ui = context.ui_sys.ui();

        let (window_size, window_size_cond) = self.calc_window_size(ui);
        let (window_pos, window_pivot) = self.calc_window_pos(context, ui);

        let window_flags = self.calc_window_flags();
        let window_name = make_imgui_id!(self, UiMenu, self.label);

        helpers::set_next_widget_window_pos(window_pos, window_pivot, imgui::Condition::Always);

        ui.window(window_name)
            .opened(&mut is_open)
            .size(window_size.to_array(), window_size_cond)
            .flags(window_flags)
            .build(|| {
                context.begin_widget_window();

                // Set default widget spacing.
                let _spacing =
                    ui.push_style_var(imgui::StyleVar::ItemSpacing(self.widget_spacing.to_array()));

                if let Some(background) = self.background {
                    helpers::draw_widget_window_background(ui, background);
                }

                for widget in &mut self.widgets {
                    widget.draw(context);
                }

                context.end_widget_window();
            });

        self.flags.set(UiMenuFlags::IsOpen, is_open && self.is_open());

        // Each menu can have one message box.
        if self.message_box.is_open() {
            self.message_box.draw(context);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        let ui = context.ui_sys.ui();
        let style = unsafe { ui.style() };

        let mut size = Vec2::zero();

        for widget in &self.widgets {
            let widget_size = widget.measure(context);
            size.x = size.x.max(widget_size.x); // Max width.
            size.y += widget_size.y; // Total height.
        }

        if !self.widgets.is_empty() { // Add inter-widget spacing.
            size.y += style.item_spacing[1] * (self.widgets.len() - 1) as f32;
        }

        size += Vec2::from_array(style.window_padding) * 2.0;
        size += Vec2::new(style.window_border_size, style.window_border_size) * 2.0;

        size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiMenu {
    pub fn new(context: &mut UiWidgetContext, params: UiMenuParams) -> UiMenuStrongRef {
        Rc::new(
            Mutable::new(
                Self {
                    label: params.label.unwrap_or_default(),
                    imgui_id: String::new(),
                    flags: params.flags,
                    size: params.size,
                    position: params.position,
                    background: params.background.map(|path| context.load_ui_texture(path)),
                    widgets: Vec::new(),
                    widget_spacing: params.widget_spacing.unwrap_or(Vec2::new(6.0, 6.0)),
                    message_box: UiMessageBox::default(),
                    on_open_close: params.on_open_close,
                }
            )
        )
    }

    #[inline]
    pub fn set_position(&mut self, position: UiMenuPosition) {
        self.position = position;
    }

    #[inline]
    pub fn has_flags(&self, flags: UiMenuFlags) -> bool {
        self.flags.intersects(flags)
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.has_flags(UiMenuFlags::IsOpen)
    }

    pub fn open(&mut self, context: &mut UiWidgetContext) {
        if self.is_open() {
            return;
        }

        self.flags.insert(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.pause();
        }

        const IS_OPEN: bool = true;
        self.on_open_close.invoke(self, context, IS_OPEN);
    }

    pub fn close(&mut self, context: &mut UiWidgetContext) {
        if !self.is_open() {
            return;
        }

        self.flags.remove(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.resume();
        }

        const IS_OPEN: bool = false;
        self.on_open_close.invoke(self, context, IS_OPEN);
    }

    pub fn add_widget<Widget>(&mut self, widget: Widget) -> &mut Self
        where Widget: UiWidget + 'static,
              UiWidgetImpl: From<Widget>
    {
        self.widgets.push(UiWidgetImpl::from(widget));
        self
    }

    #[inline]
    pub fn widgets(&self) -> &[UiWidgetImpl] {
        &self.widgets
    }

    #[inline]
    pub fn widgets_mut(&mut self) -> &mut [UiWidgetImpl] {
        &mut self.widgets
    }

    pub fn find_widget_with_label<Widget: UiWidget>(&self, label: &str) -> Option<(usize, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if w.label() == label {
                    return Some((index, w));
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

    pub fn open_message_box(&mut self, context: &mut UiWidgetContext, params: UiMessageBoxParams) {
        self.message_box.open(context, params);
    }

    pub fn close_message_box(&mut self, context: &mut UiWidgetContext) {
        self.message_box.close(context);
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn calc_window_size(&self, ui: &imgui::Ui) -> (Vec2, imgui::Condition) {
        if let Some(size) = self.size {
            (size, imgui::Condition::Always)
        } else if self.has_flags(UiMenuFlags::Fullscreen) {
            (Vec2::from_array(ui.io().display_size), imgui::Condition::Always)
        } else {
            (Vec2::zero(), imgui::Condition::Never) // Sized to contents.
        }
    }

    fn calc_window_pos(&self, context: &mut UiWidgetContext, ui: &imgui::Ui) -> (Vec2, Vec2) {
        let display_size = Vec2::from_array(ui.io().display_size);

        let mut position = Vec2::zero();
        let mut pivot = Vec2::zero();

        match &self.position {
            UiMenuPosition::Vec2(pos) => {
                position = *pos;
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

    fn calc_window_flags(&self) -> imgui::WindowFlags {
        let mut window_flags = helpers::base_widget_window_flags();

        if self.background.is_some() {
            window_flags |= imgui::WindowFlags::NO_BACKGROUND;
        }

        if self.background.is_none() && !self.label.is_empty() {
            window_flags.remove(imgui::WindowFlags::NO_TITLE_BAR);
        }

        window_flags
    }
}

// ----------------------------------------------
// UiMenuFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiMenuFlags: u8 {
        const IsOpen         = 1 << 0;
        const PauseSimIfOpen = 1 << 1;
        const Fullscreen     = 1 << 2;
        const AlignCenter    = 1 << 3;
        const AlignLeft      = 1 << 4;
        const AlignRight     = 1 << 5;
    }
}

// ----------------------------------------------
// UiMenuPosition
// ----------------------------------------------

#[derive(Default)]
pub enum UiMenuPosition {
    #[default]
    None,
    Vec2(Vec2),
    Callback(UiMenuCalcPosition),
}

// ----------------------------------------------
// UiMenuParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiMenuParams<'a> {
    pub label: Option<String>,
    pub flags: UiMenuFlags,
    pub size: Option<Vec2>,
    pub position: UiMenuPosition,
    pub widget_spacing: Option<Vec2>,
    pub background: Option<&'a str>,
    pub on_open_close: UiMenuOpenClose,
}

// ----------------------------------------------
// UiMenuHeading
// ----------------------------------------------

// Centered window heading.
// Can consist of multiple lines and an optional separator sprite at the end.
pub struct UiMenuHeading {
    font_scale: UiFontScale,
    lines: Vec<String>,
    separator: Option<UiTextureHandle>,
    margin_top: f32,
    margin_bottom: f32,
}

impl UiWidget for UiMenuHeading {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        if self.margin_top > 0.0 {
            ui.dummy([0.0, self.margin_top]);
        }

        // Center horizontally only (along the x-axis).
        const VERTICAL: bool = false;
        const HORIZONTAL: bool = true;
        let group = helpers::draw_centered_text_group(ui, &self.lines, VERTICAL, HORIZONTAL);

        if let Some(separator) = self.separator {
            let separator_height = ui.text_line_height();

            let separator_rect = Rect::from_pos_and_size(
                Vec2::new(group.x() + ui.window_pos()[0], ui.cursor_screen_pos()[1]),
                Vec2::new(group.width(), separator_height)
            );

            ui.get_window_draw_list()
                .add_image(separator,
                           separator_rect.min.to_array(),
                           separator_rect.max.to_array())
                           .build();

            // Move cursor down to after the separator and reset.
            let mut cursor = ui.cursor_pos();
            cursor[1] += separator_rect.height();
            ui.set_cursor_pos(cursor);
        }

        if self.margin_bottom > 0.0 {
            ui.dummy([0.0, self.margin_bottom]);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let mut size = Vec2::zero();

        for line in &self.lines {
            let line_size = ui.calc_text_size(line);
            size.x = size.x.max(line_size[0]); // Max width.
            size.y += line_size[1]; // Total height.
        }

        if !self.lines.is_empty() { // Add inter-line spacing.
            let style = unsafe { ui.style() };
            size.y += style.item_spacing[1] * (self.lines.len() - 1) as f32;
        }

        size
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiMenuHeading {
    pub fn new(context: &mut UiWidgetContext, params: UiMenuHeadingParams) -> Self {                
        debug_assert!(params.font_scale.is_valid());
        debug_assert!(!params.lines.is_empty());
        debug_assert!(params.margin_top >= 0.0);
        debug_assert!(params.margin_bottom >= 0.0);

        Self {
            font_scale: params.font_scale,
            lines: params.lines,
            separator: params.separator.map(|path| context.load_ui_texture(path)),
            margin_top: params.margin_top,
            margin_bottom: params.margin_bottom,
        }
    }
}

// ----------------------------------------------
// UiMenuHeadingParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiMenuHeadingParams<'a> {
    pub font_scale: UiFontScale,
    pub lines: Vec<String>,
    pub separator: Option<&'a str>,
    pub margin_top: f32,
    pub margin_bottom: f32,
}

// ----------------------------------------------
// UiWidgetGroup
// ----------------------------------------------

// Groups UiWidgets to draw them centered/aligned.
// Supports vertical and horizontal alignment and custom item spacing.
pub struct UiWidgetGroup {
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: f32,
    center_vertically: bool,
    center_horizontally: bool,
    stack_vertically: bool,
}

impl UiWidget for UiWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui = context.ui_sys.ui();

        let _spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing([self.widget_spacing, self.widget_spacing]));

        helpers::draw_centered_widget_group(
            ui,
            context,
            &mut self.widgets,
            self.center_vertically,
            self.center_horizontally,
            self.stack_vertically);
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
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

        if !self.widgets.is_empty() { // Add inter-widget spacing
            let ui = context.ui_sys.ui();
            let style = unsafe { ui.style() };

            if self.stack_vertically {
                size.y += style.item_spacing[1] * (self.widgets.len() - 1) as f32; // v-spacing
            } else {
                size.x += style.item_spacing[0] * (self.widgets.len() - 1) as f32; // h-spacing
            }
        }

        size
    }
}

impl UiWidgetGroup {
    pub fn new(_context: &mut UiWidgetContext, params: UiWidgetGroupParams) -> Self {
        debug_assert!(params.widget_spacing >= 0.0);

        Self {
            widgets: Vec::new(),
            widget_spacing: params.widget_spacing,
            center_vertically: params.center_vertically,
            center_horizontally: params.center_horizontally,
            stack_vertically: params.stack_vertically,
        }
    }

    pub fn add_widget<Widget>(&mut self, widget: Widget) -> &mut Self
        where Widget: UiWidget + 'static,
              UiWidgetImpl: From<Widget>
    {
        self.widgets.push(UiWidgetImpl::from(widget));
        self
    }
}

// ----------------------------------------------
// UiWidgetGroupParams
// ----------------------------------------------

pub struct UiWidgetGroupParams {
    pub widget_spacing: f32,
    pub center_vertically: bool,
    pub center_horizontally: bool,
    pub stack_vertically: bool,
}

impl Default for UiWidgetGroupParams {
    fn default() -> Self {
        Self {
            widget_spacing: 0.0,
            center_vertically: true,
            center_horizontally: true,
            stack_vertically: true,
        }
    }
}

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
}

impl UiWidget for UiLabeledWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui = context.ui_sys.ui();

        let _spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing([self.label_spacing, self.widget_spacing]));

        helpers::draw_centered_labeled_widget_group(
            ui,
            context,
            &mut self.labels_and_widgets,
            self.center_vertically,
            self.center_horizontally);
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        let ui = context.ui_sys.ui();
        let style = unsafe { ui.style() };
        let mut size = Vec2::zero();

        for (label, widget) in &self.labels_and_widgets {
            let widget_size = widget.measure(context);
            let label_size = ui.calc_text_size(label);

            size.x = size.x.max(label_size[0] + style.item_spacing[0] + widget_size.x); // Max width (label + widget).
            size.y += label_size[1].max(widget_size.y); // Total height (largest of the two).
        }

        if !self.labels_and_widgets.is_empty() { // Add inter-widget spacing
            size.y += style.item_spacing[1] * (self.labels_and_widgets.len() - 1) as f32;
        }

        size
    }
}

impl UiLabeledWidgetGroup {
    pub fn new(_context: &mut UiWidgetContext, params: UiLabeledWidgetGroupParams) -> Self {
        debug_assert!(params.label_spacing  >= 0.0);
        debug_assert!(params.widget_spacing >= 0.0);

        Self {
            labels_and_widgets: Vec::new(),
            label_spacing: params.label_spacing,
            widget_spacing: params.widget_spacing,
            center_vertically: params.center_vertically,
            center_horizontally: params.center_horizontally,
        }
    }

    pub fn add_widget<Widget>(&mut self, label: String, widget: Widget) -> &mut Self
        where Widget: UiWidget + 'static,
              UiWidgetImpl: From<Widget>
    {
        debug_assert!(!label.is_empty(), "UiLabeledWidgetGroup requires a non-empty label!");
        debug_assert!(widget.label().is_empty(), "Widgets added to UiLabeledWidgetGroup should not have a label!");

        self.labels_and_widgets.push((label, UiWidgetImpl::from(widget)));
        self
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
}

impl Default for UiLabeledWidgetGroupParams {
    fn default() -> Self {
        Self {
            label_spacing: 0.0,
            widget_spacing: 0.0,
            center_vertically: true,
            center_horizontally: true,
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
    imgui_id: String,
    tooltip: Option<UiTooltipText>,
    font_scale: UiFontScale,
    size: UiTextButtonSize,
    hover: Option<UiTextureHandle>,
    enabled: bool,
    on_pressed: UiTextButtonPressed,
}

pub type UiTextButtonPressed = UiWidgetCallback<UiTextButton>;

impl UiWidget for UiTextButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_labeled_id!(self, UiTextButton, self.label);

        // Faded text if disabled.
        let mut text_color = ui.style_color(imgui::StyleColor::Text);
        if self.is_enabled() {
            text_color[3] = 1.0;
        } else {
            text_color[3] = 0.5;
        }

        let _btn_text_color = ui.push_style_color(imgui::StyleColor::Text, text_color);

        let pressed = if let Some(hover) = self.hover {
            // If we have a hover effect texture (underline effect), the button
            // will draw fully transparent. We change the text color to indicate
            // enabled/disabled buttons.

            // No border.
            let _border_size = ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0));

            // No color change when hovered/active. Transparent background.
            let transparent = [0.0, 0.0, 0.0, 0.0];
            let _btn_color = ui.push_style_color(imgui::StyleColor::Button, transparent);
            let _btn_color_hovered = ui.push_style_color(imgui::StyleColor::ButtonHovered, transparent);
            let _btn_color_active = ui.push_style_color(imgui::StyleColor::ButtonActive, transparent);

            let pressed = ui.button(label);

            // Draw underline effect when hovered / active:
            if ui.is_item_hovered() {
                let button_pos  = Vec2::from_array(ui.item_rect_min());
                let button_size = Vec2::from_array(ui.item_rect_size());

                let button_rect = Rect::from_pos_and_size(
                    button_pos + Vec2::new(0.0, (button_size.y * 0.5) + 1.0),
                    button_size
                );

                let hover_tint_color = if ui.is_item_active() || !self.is_enabled() {
                    imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
                } else {
                    imgui::ImColor32::WHITE
                };

                ui.get_window_draw_list()
                    .add_image(hover,
                               button_rect.min.to_array(),
                               button_rect.max.to_array())
                               .col(hover_tint_color)
                               .build();
            }

            pressed
        } else {
            // Draw standard imgui text label button.
            ui.button(label)
        };

        if let Some(tooltip) = &self.tooltip && ui.is_item_hovered() {
            tooltip.draw(context);
        }

        // Invoke on pressed callback.
        if pressed && self.is_enabled() {
            self.on_pressed.invoke(self, context);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let style = unsafe { ui.style() };

        let font_size = ui.current_font_size();
        let text_size = ui.calc_text_size(&self.label);

        let width  = text_size[0] + (style.frame_padding[0] * 2.0);
        let height = text_size[1].max(font_size) + (style.frame_padding[1] * 2.0);

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
    pub fn new(context: &mut UiWidgetContext, params: UiTextButtonParams) -> Self {
        debug_assert!(!params.label.is_empty());
        Self {
            label: params.label,
            imgui_id: String::new(),
            tooltip: params.tooltip,
            font_scale: params.size.font_scale(),
            size: params.size,
            hover: params.hover.map(|path| context.load_ui_texture(path)),
            enabled: params.enabled,
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

#[derive(Default)]
pub struct UiTextButtonParams<'a> {
    pub label: String,
    pub tooltip: Option<UiTooltipText>,
    pub size: UiTextButtonSize,
    pub hover: Option<&'a str>,
    pub enabled: bool,
    pub on_pressed: UiTextButtonPressed,
}

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

    logical_state: UiSpriteButtonState,
    visual_state: UiSpriteButtonState,
    visual_state_transition_timer: CountdownTimer,
    state_transition_secs: Seconds,

    on_state_changed: UiSpriteButtonStateChanged,
}

pub type UiSpriteButtonStateChanged = UiWidgetCallbackWithArg<UiSpriteButton, UiSpriteButtonState>;

impl UiWidget for UiSpriteButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());
        debug_assert!(self.textures.are_textures_loaded());

        let ui = context.ui_sys.ui();
        let texture = self.textures.texture_for_state(self.visual_state);

        let flags = imgui::ButtonFlags::MOUSE_BUTTON_LEFT | imgui::ButtonFlags::MOUSE_BUTTON_RIGHT;
        ui.invisible_button_flags(&self.label, self.size.to_array(), flags);

        let hovered = ui.is_item_hovered();
        let left_click = ui.is_item_clicked_with_button(imgui::MouseButton::Left);
        let right_click = ui.is_item_clicked_with_button(imgui::MouseButton::Right);

        let rect_min = ui.item_rect_min();
        let rect_max = ui.item_rect_max();

        ui.get_window_draw_list()
            .add_image(texture,
                       rect_min,
                       rect_max)
                       .build();

        self.position = Some(Vec2::from_array(rect_min));

        // NOTE: Only left click counts as "pressed".
        self.update_state(context, hovered, left_click, right_click, context.delta_time_secs);

        if let Some(tooltip) = &self.tooltip {
            let show_tooltip = hovered && (!self.is_pressed() || self.show_tooltip_when_pressed);
            if show_tooltip {
                tooltip.draw(context);
            }
        }
    }

    fn measure(&self, _context: &UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiSpriteButton {
    pub fn new(context: &mut UiWidgetContext, params: UiSpriteButtonParams) -> Self {
        debug_assert!(!params.label.is_empty());
        debug_assert!(params.size.x > 0.0 && params.size.y > 0.0);
        debug_assert!(params.state_transition_secs >= 0.0);

        let textures = UiSpriteButtonTextures::load(&params.label, context);
        let visual_state_transition_timer = CountdownTimer::new(params.state_transition_secs);

        Self {
            label: params.label,
            tooltip: params.tooltip,
            show_tooltip_when_pressed: params.show_tooltip_when_pressed,
            size: params.size,
            position: None, // Set after the first draw().
            textures,
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
            self.logical_state = UiSpriteButtonState::Idle;
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

    fn update_state(&mut self,
                    context: &mut UiWidgetContext,
                    hovered: bool,
                    left_click: bool,
                    right_click: bool,
                    delta_time_secs: Seconds) {
        let prev_state = self.logical_state;

        match self.logical_state {
            UiSpriteButtonState::Idle | UiSpriteButtonState::Hovered => {
                // Left click selects/presses button.
                if left_click {
                    self.logical_state = UiSpriteButtonState::Pressed;
                } else if hovered {
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
    textures: [UiTextureHandle; BUTTON_STATE_COUNT],
}

impl UiSpriteButtonTextures {
    fn unloaded() -> Self {
        Self { textures: [INVALID_UI_TEXTURE_HANDLE; BUTTON_STATE_COUNT] }
    }

    fn load(sprite_path: &str, context: &mut UiWidgetContext) -> Self {
        let mut sprites = Self::unloaded();
        sprites.load_textures(sprite_path, context);
        sprites
    }

    fn load_textures(&mut self, sprite_path: &str, context: &mut UiWidgetContext) {
        for state in UiSpriteButtonState::iter() {
            self.textures[state as usize] = state.load_texture(sprite_path, context);
        }
    }

    #[inline]
    fn are_textures_loaded(&self) -> bool {
        self.textures[0] != INVALID_UI_TEXTURE_HANDLE
    }

    #[inline]
    fn texture_for_state(&self, state: UiSpriteButtonState) -> UiTextureHandle {
        debug_assert!(self.textures[state as usize] != INVALID_UI_TEXTURE_HANDLE);
        self.textures[state as usize]
    }
}

// ----------------------------------------------
// UiSpriteButtonState
// ----------------------------------------------

const BUTTON_STATE_COUNT: usize = UiSpriteButtonState::COUNT;

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
    fn asset_path(self, sprite_path: &str) -> PathBuf {
        debug_assert!(!sprite_path.is_empty());
        let sprite_suffix = self.get_str("Suffix").unwrap();

        // {sprite_path}_{sprite_suffix}.png
        let mut sprite_name = ArrayString::<128>::new();
        sprite_name.push_str(sprite_path);
        sprite_name.push_str("_");
        sprite_name.push_str(sprite_suffix);
        sprite_name.push_str(".png");

        assets_path().join("buttons").join(sprite_name)
    }

    fn load_texture(self, sprite_path: &str, context: &mut UiWidgetContext) -> UiTextureHandle {
        context.load_ui_texture(self.asset_path(sprite_path).to_str().unwrap())
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
    pub size: Vec2,
    pub initial_state: UiSpriteButtonState,
    pub state_transition_secs: Seconds,
    pub on_state_changed: UiSpriteButtonStateChanged,
}

// ----------------------------------------------
// UiTooltipText
// ----------------------------------------------

#[derive(Clone)]
pub struct UiTooltipText {
    text: String,
    font_scale: UiFontScale,
    background: Option<UiTextureHandle>,
}

impl UiTooltipText {
    pub fn new(context: &mut UiWidgetContext, params: UiTooltipTextParams) -> Self {
        debug_assert!(!params.text.is_empty());
        debug_assert!(params.font_scale.is_valid());

        Self {
            text: params.text,
            font_scale: params.font_scale,
            background: params.background.map(|path| context.load_ui_texture(path))
        }
    }

    fn draw(&self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        custom_tooltip(context.ui_sys, self.font_scale, self.background, || {
            context.ui_sys.ui().text(&self.text);
        });
    }
}

// ----------------------------------------------
// UiTooltipTextParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiTooltipTextParams<'a> {
    pub text: String,
    pub font_scale: UiFontScale,
    pub background: Option<&'a str>,
}

// ----------------------------------------------
// UiSeparator
// ----------------------------------------------

#[derive(Clone)]
pub struct UiSeparator {
    separator: Option<UiTextureHandle>,
    size: Option<Vec2>,
    thickness: f32,
    vertical: bool,
}

impl UiWidget for UiSeparator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui = context.ui_sys.ui();
        let size = self.size.unwrap_or_else(
            || helpers::calc_separator_size(ui, !self.vertical, self.thickness));

        // Invisible dummy item.
        ui.dummy(size.to_array());

        // Optionally draw a texture over it.
        if let Some(separator) = self.separator {
            let separator_rect = Rect::from_extents(
                Vec2::from_array(ui.item_rect_min()),
                Vec2::from_array(ui.item_rect_max())
            );

            ui.get_window_draw_list()
                .add_image(separator,
                           separator_rect.min.to_array(),
                           separator_rect.max.to_array())
                           .build();
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        self.size.unwrap_or_else(
                || helpers::calc_separator_size(context.ui_sys.ui(), !self.vertical, self.thickness))
    }
}

impl UiSeparator {
    pub fn new(context: &mut UiWidgetContext, params: UiSeparatorParams) -> Self {                
        Self {
            separator: params.separator.map(|path| context.load_ui_texture(path)),
            size: params.size,
            thickness: params.thickness.unwrap_or(1.0),
            vertical: params.vertical,
        }
    }
}

// ----------------------------------------------
// UiSeparatorParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiSeparatorParams<'a> {
    pub separator: Option<&'a str>,
    pub size: Option<Vec2>,
    pub thickness: Option<f32>, // Optional thickness used if `size = None`.
    pub vertical: bool,         // Horizontal separator by default.
}

// ----------------------------------------------
// UiSlider
// ----------------------------------------------

pub struct UiSlider {
    label: String,
    imgui_id: String,
    font_scale: UiFontScale,
    value: UiSliderValue,
}

pub type UiSliderReadValue<T>   = UiWidgetCallback<UiSlider, T>;
pub type UiSliderUpdateValue<T> = UiWidgetCallbackWithArg<UiSlider, T>;

impl UiWidget for UiSlider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiSlider, self.label);

        match &self.value {
            UiSliderValue::I32 { min, max, on_read_value, on_update_value } => {
                let mut value = on_read_value.invoke(self, context).unwrap_or_default();

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .build(&mut value);

                if value_changed {
                    on_update_value.invoke(self, context, value.clamp(*min, *max));
                }
            }
            UiSliderValue::U32 { min, max, on_read_value, on_update_value } => {
                let mut value = on_read_value.invoke(self, context).unwrap_or_default();

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .build(&mut value);

                if value_changed {
                    on_update_value.invoke(self, context, value.clamp(*min, *max));
                }
            }
            UiSliderValue::F32 { min, max, on_read_value, on_update_value } => {
                let mut value = on_read_value.invoke(self, context).unwrap_or_default();

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .display_format("%.2f")
                    .build(&mut value);

                if value_changed {
                    on_update_value.invoke(self, context, value.clamp(*min, *max));
                }
            }
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        helpers::calc_labeled_widget_size(context, self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiSlider {
    pub fn new<T>(_context: &mut UiWidgetContext, params: UiSliderParams<T>) -> Self
        where UiSliderParams<T>: IntoUiSliderValue
    {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.clone().unwrap_or_default(),
            imgui_id: String::new(),
            font_scale: params.font_scale,
            value: params.into_slider_value(),
        }
    }
}

// ----------------------------------------------
// UiSliderValue
// ----------------------------------------------

pub enum UiSliderValue {
    I32 {
        min: i32,
        max: i32,
        on_read_value: UiSliderReadValue<i32>,
        on_update_value: UiSliderUpdateValue<i32>,
    },
    U32 {
        min: u32,
        max: u32,
        on_read_value: UiSliderReadValue<u32>,
        on_update_value: UiSliderUpdateValue<u32>,
    },
    F32 {
        min: f32,
        max: f32,
        on_read_value: UiSliderReadValue<f32>,
        on_update_value: UiSliderUpdateValue<f32>,
    },
}

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

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub struct UiCheckbox {
    label: String,
    imgui_id: String,
    font_scale: UiFontScale,
    on_read_value: UiCheckboxReadValue,
    on_update_value:UiCheckboxUpdateValue,
}

pub type UiCheckboxReadValue   = UiWidgetCallback<UiCheckbox, bool>;
pub type UiCheckboxUpdateValue = UiWidgetCallbackWithArg<UiCheckbox, bool>;

impl UiWidget for UiCheckbox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiCheckbox, self.label);

        let mut value = self.on_read_value.invoke(self, context).unwrap_or_default();

        let (value_changed, _group) =
            helpers::checkbox_with_left_label(ui, label, &mut value);

        if value_changed {
            self.on_update_value.invoke(self, context, value);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let style = unsafe { ui.style() };
        let checkbox_square = ui.text_line_height() + (style.frame_padding[1] * 2.0);
        let mut width = checkbox_square;

        if !self.label.is_empty() {
            let label_size = ui.calc_text_size(&self.label);
            width += style.item_inner_spacing[0] + label_size[0];
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
    pub fn new(_context: &mut UiWidgetContext, params: UiCheckboxParams) -> Self {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale: params.font_scale,
            on_read_value: params.on_read_value,
            on_update_value: params.on_update_value,
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

// ----------------------------------------------
// UiTextInput
// ----------------------------------------------

pub struct UiTextInput {
    label: String,
    imgui_id: String,
    buffer: String,
    font_scale: UiFontScale,
    on_read_value: UiTextInputReadValue,
    on_update_value: UiTextInputUpdateValue,
}

pub type UiTextInputReadValue   = UiWidgetCallback<UiTextInput, RawPtr<str>>;
pub type UiTextInputUpdateValue = UiWidgetCallbackWithArg<UiTextInput, RawPtr<str>>;

impl UiWidget for UiTextInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiTextInput, self.label);

        let value = self.on_read_value.invoke(self, context)
            .unwrap_or(RawPtr::from_ref(""));

        self.buffer.clear();
        self.buffer.push_str(value.as_ref());

        let (input, _group) =
            helpers::input_text_with_left_label(ui, label, &mut self.buffer);

        let value_changed = input.build();

        if value_changed {
            self.on_update_value.invoke(self, context, RawPtr::from_ref(&self.buffer));
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        helpers::calc_labeled_widget_size(context, self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiTextInput {
    pub fn new(_context: &mut UiWidgetContext, params: UiTextInputParams) -> Self {
        debug_assert!(params.font_scale.is_valid());

        Self {
            label: params.label.unwrap_or_default(),
            imgui_id: String::new(),
            buffer: String::new(),
            font_scale: params.font_scale,
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

// ----------------------------------------------
// UiDropdown
// ----------------------------------------------

pub struct UiDropdown {
    label: String,
    imgui_id: String,
    font_scale: UiFontScale,
    current_item: usize,
    items: Vec<String>,
    on_selection_changed: UiDropdownSelectionChanged,
}

pub type UiDropdownSelectionChanged = UiWidgetCallback<UiDropdown>;

impl UiWidget for UiDropdown {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let bg_color = if context.ui_sys.current_ui_theme().is_in_game() {
            [0.90, 0.80, 0.60, 1.0]
        } else {
            [0.18, 0.18, 0.20, 0.9]
        };

        let _combo_bg_color = ui.push_style_color(imgui::StyleColor::PopupBg, bg_color);

        let label = make_imgui_id!(self, UiDropdown, self.label);

        let (selection_changed, _group) =
            helpers::combo_with_left_label(ui, label, &mut self.current_item, &self.items);

        if selection_changed {
            self.on_selection_changed.invoke(self, context);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        helpers::calc_labeled_widget_size(context, self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiDropdown {
    pub fn new(context: &mut UiWidgetContext, params: UiDropdownParams<String>) -> Self {
        Self::with_strings(context, params)
    }

    pub fn with_strings(_context: &mut UiWidgetContext, params: UiDropdownParams<String>) -> Self {
        debug_assert!(params.font_scale.is_valid());
        debug_assert!(!params.items.is_empty());
        debug_assert!(params.current_item < params.items.len());

        Self {
            label: params.label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale: params.font_scale,
            current_item: params.current_item,
            items: params.items,
            on_selection_changed: params.on_selection_changed,
        }
    }

    // From array of values implementing Display.
    pub fn with_values<T>(context: &mut UiWidgetContext, params: UiDropdownParams<T>) -> Self
        where T: Display
    {
        let item_strings: Vec<String> = params.items
            .iter()
            .map(|item| item.to_string())
            .collect();

        Self::with_strings(context, UiDropdownParams {
            label: params.label,
            font_scale: params.font_scale,
            current_item: params.current_item,
            items: item_strings,
            on_selection_changed: params.on_selection_changed,
        })
    }

    pub fn current_selection_index(&self) -> usize {
        self.current_item
    }

    pub fn current_selection(&self) -> &str {
        &self.items[self.current_item]
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
        where ToString: Fn(&V) -> String
    {
        let items: Vec<String> = values
            .iter()
            .map(to_str)
            .collect();

        self.reset_items(current_item, items);
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
}

// ----------------------------------------------
// UiItemList
// ----------------------------------------------

pub struct UiItemList {
    label: String,
    imgui_id: String,
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

pub type UiItemListSelectionChanged = UiWidgetCallback<UiItemList>;

impl UiWidget for UiItemList {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let window_name = make_imgui_id!(self, UiItemList, self.label);

        // child_window size:
        //  > 0.0 -> fixed size
        //  = 0.0 -> use remaining host window size
        //  < 0.0 -> use remaining host window size minus abs(size)
        let mut window_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let style = unsafe { ui.style() };
            window_size.x -= self.margin_right - style.window_padding[0];
        }

        let set_left_margin = || {
            if self.margin_left > 0.0 {
                ui.set_cursor_pos([self.margin_left, ui.cursor_pos()[1]]);
            }
        };

        // Optional label:
        if !self.label.is_empty() {
            set_left_margin();
            ui.text(&self.label);
        }

        // Optional text input field:
        let text_input_field_changed = {
            if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
                let mut input_field_id = ArrayString::<128>::new();
                input_field_id.push_str("## ");
                input_field_id.push_str(window_name);
                input_field_id.push_str(" InputField");

                // set_next_item_width:
                //  > 0.0 -> width is item_width pixels
                //  = 0.0 -> default to ~2/3 of window width
                //  < 0.0 -> item_width pixels relative to the right of window (-1.0 always aligns width to the right side)
                ui.set_next_item_width(window_size.x);

                set_left_margin();
                ui.input_text(input_field_id, text_input_field_buffer).build()
            } else {
                false
            }
        };

        if text_input_field_changed && self.text_input_field_buffer.is_some() {
            self.current_item = None;
            self.on_selection_changed.invoke(self, context);
        }

        set_left_margin();
        ui.child_window(window_name)
            .size(window_size.to_array())
            .border(self.flags.intersects(UiItemListFlags::Border))
            .scrollable(self.flags.intersects(UiItemListFlags::Scrollable))
            .scroll_bar(self.flags.intersects(UiItemListFlags::Scrollbars))
            .build(|| {
                let mut selection_changed = false;

                for (index, item) in self.items.iter().enumerate() {
                    let is_selected = self.current_item == Some(index);

                    if ui.selectable_config(item)
                        .selected(is_selected)
                        .build()
                    {
                        if self.current_item != Some(index) {
                            self.current_item = Some(index);
                            selection_changed = true;
                        }
                    }
                }

                if selection_changed && let Some(selected_index) = self.current_item {
                    if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
                        text_input_field_buffer.clear();
                        text_input_field_buffer.push_str(&self.items[selected_index]);
                    }

                    self.on_selection_changed.invoke(self, context);
                }
            });
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        context.ui_sys.set_font_scale(self.font_scale);

        let ui = context.ui_sys.ui();
        let style = unsafe { ui.style() };

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - style.window_padding[0];
        }

        let size = helpers::calc_child_window_size(ui, requested_size);

        let input_field_height = {
            if self.text_input_field_buffer.is_some() {
                ui.text_line_height() + (style.frame_padding[1] * 2.0)
            } else {
                0.0
            }
        };

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
    pub fn new(context: &mut UiWidgetContext, params: UiItemListParams<String>) -> Self {
        Self::with_strings(context, params)
    }

    pub fn with_strings(_context: &mut UiWidgetContext, params: UiItemListParams<String>) -> Self {
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
            imgui_id: String::new(),
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
    pub fn with_values<T>(context: &mut UiWidgetContext, params: UiItemListParams<T>) -> Self
        where T: Display
    {
        let item_strings: Vec<String> = params.items
            .iter()
            .map(|item| item.to_string())
            .collect();

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

    pub fn add_item(&mut self, item: String) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn reset_items(&mut self, current_item: Option<usize>, items: Vec<String>) {
        self.current_item = current_item;
        self.items = items;
    }

    pub fn reset_items_with<V, ToString>(&mut self, values: &[V], current_item: Option<usize>, to_str: ToString)
        where ToString: Fn(&V) -> String
    {
        let items: Vec<String> = values
            .iter()
            .map(to_str)
            .collect();

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

// ----------------------------------------------
// UiMessageBox
// ----------------------------------------------

#[derive(Default)]
pub struct UiMessageBox {
    menu: Option<UiMenuStrongRef>,
}

impl UiWidget for UiMessageBox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = &self.menu {
            // NOTE: Increment the ref count here.
            // draw() may trigger a UiMessageBox::close, which would drop `self.menu`.
            let strong_ref = menu.clone();
            strong_ref.as_mut().draw(context);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
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
        self.menu.is_some()
    }

    pub fn open(&mut self, context: &mut UiWidgetContext, params: UiMessageBoxParams) {
        let menu = UiMenu::new(
            context,
            UiMenuParams {
                label: params.label,
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter,
                size: params.size,
                background: params.background,
                ..Default::default()
            }
        );

        for widget in params.contents {
            menu.as_mut().add_widget(widget);
        }

        if !params.buttons.is_empty() {
            let mut button_group = UiWidgetGroup::new(
                context,
                UiWidgetGroupParams {
                    widget_spacing: 10.0,
                    center_vertically: true,
                    center_horizontally: true,
                    stack_vertically: false, // Render buttons side-by-side.
                }
            );

            for button in params.buttons {
                button_group.add_widget(button);
            }

            menu.as_mut().add_widget(button_group);
        }

        self.menu = Some(menu);
    }

    pub fn close(&mut self, _context: &mut UiWidgetContext) {
        self.menu = None;
    }
}

// ----------------------------------------------
// UiMessageBoxParams
// ----------------------------------------------

#[derive(Default)]
pub struct UiMessageBoxParams<'a> {
    pub label: Option<String>,
    pub size: Option<Vec2>,
    pub background: Option<&'a str>,
    pub contents: Vec<UiWidgetImpl>,
    pub buttons: Vec<UiWidgetImpl>,
}

// ----------------------------------------------
// UiSlideshow
// ----------------------------------------------

pub struct UiSlideshow {
    imgui_id: String,
    flags: UiSlideshowFlags,
    loop_mode: UiSlideshowLoopMode,

    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,

    frames: Vec<UiTextureHandle>,
    frame_index: usize,
    frame_duration_secs: Seconds,
    frame_play_time_secs: Seconds,
}

impl UiWidget for UiSlideshow {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        self.update_anim(context.delta_time_secs);
        self.draw_current_frame(context);
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        let ui = context.ui_sys.ui();
        let style = unsafe { ui.style() };

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - style.window_padding[0];
        }

        helpers::calc_child_window_size(ui, requested_size)
    }
}

impl UiSlideshow {
    pub fn new(context: &mut UiWidgetContext, params: UiSlideshowParams) -> Self {
        debug_assert!(!params.frames.is_empty());
        debug_assert!(params.frame_duration_secs > 0.0);
        debug_assert!(params.margin_left >= 0.0);
        debug_assert!(params.margin_right >= 0.0);

        let mut frames = Vec::with_capacity(params.frames.len());

        for path in params.frames {
            frames.push(context.load_ui_texture(path));
        }

        Self {
            imgui_id: String::new(),
            flags: params.flags,
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

    fn update_anim(&mut self, delta_time_secs: Seconds) {
        if self.frames.len() <= 1 {
            // Static background (single-frame).
            return;
        }

        if self.has_flags(UiSlideshowFlags::PlayedOnce) &&
          !self.has_flags(UiSlideshowFlags::Looping)
        {
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

    fn draw_current_frame(&mut self, context: &mut UiWidgetContext) {
        let current_frame = self.frames[self.frame_index];

        if context.is_inside_widget_window() && !self.has_flags(UiSlideshowFlags::Fullscreen) {
            // We are drawing inside a parent window, so nest the
            // rendered anim frame inside a child window instead.
            self.draw_inside_child_window(context, current_frame);
        } else {
            // Draw full-screen rectangle with the anim frame texture.
            // Background draw list ensures it renders behind any other UI elements.
            let ui = context.ui_sys.ui();
            let draw_list = ui.get_background_draw_list();
            draw_list.add_image(current_frame,
                                [0.0, 0.0],
                                ui.io().display_size)
                                .build();
        }
    }

    fn draw_inside_child_window(&mut self, context: &mut UiWidgetContext, current_frame: UiTextureHandle) {
        let ui = context.ui_sys.ui();
        let window_name = make_imgui_id!(self, UiSlideshow, String::new());

        // child_window size:
        //  > 0.0 -> fixed size
        //  = 0.0 -> use remaining host window size
        //  < 0.0 -> use remaining host window size minus abs(size)
        let mut window_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let style = unsafe { ui.style() };
            window_size.x -= self.margin_right - style.window_padding[0];
        }

        let mut cursor = ui.cursor_pos();
        if self.margin_left > 0.0 {
            ui.set_cursor_pos([self.margin_left, cursor[1]]);
        }

        ui.child_window(window_name)
            .size(window_size.to_array())
            .flags(helpers::base_widget_window_flags())
            .build(|| {
                let draw_list = ui.get_window_draw_list();

                let child_window_rect = Rect::from_pos_and_size(
                    Vec2::from_array(ui.window_pos()),
                    Vec2::from_array(ui.window_size())
                );

                draw_list.add_image(current_frame,
                                    child_window_rect.min.to_array(),
                                    child_window_rect.max.to_array())
                                    .build();

                // Advance cursor to after the slide frame.
                cursor[1] += child_window_rect.height();
                ui.set_cursor_pos(cursor);
            });
    }
}

// ----------------------------------------------
// UiSlideshowFlags / UiSlideshowLoopMode
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiSlideshowFlags: u8 {
        const Fullscreen = 1 << 0;
        const PlayedOnce = 1 << 1; // Finished playing at least once.
        const Looping    = 1 << 2; // Started playing again with one of UiSlideshowLoopMode.
    }
}

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
pub struct UiSlideshowParams<'a> {
    pub flags: UiSlideshowFlags,
    pub loop_mode: UiSlideshowLoopMode,
    pub frame_duration_secs: Seconds,
    pub frames: &'a [String],

    // Ignored if UiSlideshowFlags::Fullscreen is set.
    pub size: Option<Vec2>,
    pub margin_left: f32,
    pub margin_right: f32,
}
