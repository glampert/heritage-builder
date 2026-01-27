#![allow(clippy::enum_variant_names)]
#![allow(clippy::type_complexity)]

use std::{any::Any, fmt::Display, path::PathBuf};
use std::rc::{Rc, Weak};

use arrayvec::ArrayString;
use bitflags::bitflags;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    helpers,
    UiSystem,
    UiTextureHandle,
    assets_path,
    custom_tooltip,
    INVALID_UI_TEXTURE_HANDLE,
};

use crate::{
    bitflags_with_display,
    tile::TileMap,
    render::TextureCache,
    game::{sim::Simulation, world::World},
    utils::{Rect, Size, Vec2, mem::{self, Mutable}},
    engine::{Engine, time::{CountdownTimer, Seconds}},
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
    pub tile_map: &'game mut TileMap,

    pub ui_sys: &'game UiSystem,
    pub tex_cache: &'game mut dyn TextureCache,

    pub viewport_size: Size,
    pub delta_time_secs: Seconds,

    in_window_count: u32,
}

impl<'game> UiWidgetContext<'game> {
    #[inline]
    pub fn new(sim: &'game mut Simulation,
               world: &'game World,
               tile_map: &'game mut TileMap,
               engine: &'game dyn Engine) -> Self {
        Self {
            sim,
            world,
            tile_map,
            ui_sys: engine.ui_system(),
            tex_cache: engine.texture_cache(),
            viewport_size: engine.viewport().integer_size(),
            delta_time_secs: engine.frame_clock().delta_time(),
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
        self.ui_sys.set_font_scale(1.0);
    }

    #[inline]
    fn is_inside_widget_window(&self) -> bool {
        self.in_window_count != 0
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

    fn font_scale(&self) -> f32 {
        1.0
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
    position: Option<Vec2>,
    background: Option<UiTextureHandle>,
    widgets: Vec<UiWidgetImpl>,
    message_box: UiMessageBox,
    on_open_close: Option<Box<dyn Fn(&UiMenu, &mut UiWidgetContext, bool) + 'static>>,
}

pub type UiMenuStrongRef = Rc<Mutable<UiMenu>>;
pub type UiMenuWeakRef   = Weak<Mutable<UiMenu>>;

impl UiWidget for UiMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        if !self.is_open() {
            return;
        }

        let ui = context.ui_sys.ui();

        let (window_size, window_size_cond) = self.calc_window_size(ui);
        let (window_pos, window_pivot, window_pos_cond) = self.calc_window_pos(ui);

        let window_flags = self.calc_window_flags();
        let window_name = make_imgui_id!(self, UiMenu, self.label);

        let mut is_open = self.is_open();

        helpers::set_next_widget_window_pos(window_pos, window_pivot, window_pos_cond);

        ui.window(window_name)
            .opened(&mut is_open)
            .size(window_size.to_array(), window_size_cond)
            .flags(window_flags)
            .build(|| {
                context.begin_widget_window();

                if let Some(background) = self.background {
                    helpers::draw_widget_window_background(ui, background);
                }

                for widget in &mut self.widgets {
                    widget.draw(context);
                }

                context.end_widget_window();
            });

        self.flags.set(UiMenuFlags::IsOpen, is_open);

        // Each menu can have one message box.
        if self.message_box.is_open() {
            self.message_box.draw(context);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        let mut size = Vec2::zero();

        for widget in &self.widgets {
            let widget_size = widget.measure(context);
            size.x = size.x.max(widget_size.x); // Max width.
            size.y += widget_size.y; // Total height.
        }

        if !self.widgets.is_empty() { // Add inter-widget spacing.
            let ui = context.ui_sys.ui();
            let style = unsafe { ui.style() };
            size.y += style.item_spacing[1] * (self.widgets.len() - 1) as f32;
        }

        size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiMenu {
    pub const NO_OPEN_CLOSE_CALLBACK: Option<fn(&UiMenu, &mut UiWidgetContext, bool)> = None;

    pub fn new<OnOpenClose>(context: &mut UiWidgetContext,
                            label: Option<String>,
                            flags: UiMenuFlags,
                            size: Option<Vec2>,
                            position: Option<Vec2>,
                            background: Option<&str>,
                            on_open_close: Option<OnOpenClose>) -> UiMenuStrongRef
        where OnOpenClose: Fn(&UiMenu, &mut UiWidgetContext, bool) + 'static
    {
        Rc::new(
            Mutable::new(
                Self {
                    label: label.unwrap_or_default(),
                    imgui_id: String::new(),
                    flags,
                    size,
                    position,
                    background: background.map(|path| helpers::load_ui_texture(context, path)),
                    widgets: Vec::new(),
                    message_box: UiMessageBox::default(),
                    on_open_close: on_open_close.map_or(None, |f| Some(Box::new(f))),
                }
            )
        )
    }

    pub fn has_flags(&self, flags: UiMenuFlags) -> bool {
        self.flags.intersects(flags)
    }

    pub fn is_open(&self) -> bool {
        self.has_flags(UiMenuFlags::IsOpen)
    }

    pub fn open(&mut self, context: &mut UiWidgetContext) {
        self.flags.insert(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.pause();
        }

        if let Some(on_open_close) = &self.on_open_close {
            const IS_OPEN: bool = true;
            on_open_close(self, context, IS_OPEN);
        }
    }

    pub fn close(&mut self, context: &mut UiWidgetContext) {
        self.flags.remove(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.resume();
        }

        if let Some(on_open_close) = &self.on_open_close {
            const IS_OPEN: bool = false;
            on_open_close(self, context, IS_OPEN);
        }
    }

    pub fn add_widget<Widget>(&mut self, widget: Widget) -> &mut Self
        where Widget: UiWidget + 'static,
              UiWidgetImpl: From<Widget>
    {
        self.widgets.push(UiWidgetImpl::from(widget));
        self
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
            (Vec2::zero(), imgui::Condition::Never)
        }
    }

    fn calc_window_pos(&self, ui: &imgui::Ui) -> (Vec2, Vec2, imgui::Condition) {
        if let Some(position) = self.position {
            // AlignLeft/Right can be combined with AlignCenter.
            let pivot_y = if self.has_flags(UiMenuFlags::AlignCenter) { 0.5 } else { 0.0 };
            if self.has_flags(UiMenuFlags::AlignRight) {
                (position, Vec2::new(1.0, pivot_y), imgui::Condition::Always)
            } else {
                // AlignLeft/default.
                (position, Vec2::new(0.0, pivot_y), imgui::Condition::Always)
            }
        } else if self.has_flags(UiMenuFlags::AlignCenter) {
            // Center to screen.
            let position = Vec2::new(ui.io().display_size[0] * 0.5, ui.io().display_size[1] * 0.5);
            (position, Vec2::new(0.5, 0.5), imgui::Condition::Always)
        } else if self.has_flags(UiMenuFlags::AlignRight) {
            // Alight to top-left right corner.
            let position = Vec2::new(ui.io().display_size[0], 0.0);
            (position, Vec2::new(1.0, 0.0), imgui::Condition::Always)
        } else {
            // AlignLeft/default.
            (Vec2::zero(), Vec2::zero(), imgui::Condition::Always)
        }
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
// UiMenuParams
// ----------------------------------------------

pub struct UiMenuParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiMenuHeading
// ----------------------------------------------

// Centered window heading.
// Can consist of multiple lines and an optional separator sprite at the end.
pub struct UiMenuHeading {
    font_scale: f32,
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

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiMenuHeading {
    pub fn new(context: &mut UiWidgetContext,
               font_scale: f32,
               lines: Vec<String>,
               separator: Option<&str>,
               margin_top: f32,
               margin_bottom: f32) -> Self {                
        debug_assert!(font_scale > 0.0);
        debug_assert!(!lines.is_empty());
        Self {
            font_scale,
            lines,
            separator: separator.map(|path| helpers::load_ui_texture(context, path)),
            margin_top,
            margin_bottom,
        }
    }
}

// ----------------------------------------------
// UiMenuHeadingParams
// ----------------------------------------------

pub struct UiMenuHeadingParams {
    // TODO: Replace new() args with this struct. Provide defaults.
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
    pub fn new(widget_spacing: f32, center_vertically: bool, center_horizontally: bool, stack_vertically: bool) -> Self {
        debug_assert!(widget_spacing >= 0.0);
        Self {
            widgets: Vec::new(),
            widget_spacing,
            center_vertically,
            center_horizontally,
            stack_vertically,
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
    // TODO: Replace new() args with this struct. Provide defaults.
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
    pub fn new(label_spacing: f32,
               widget_spacing: f32,
               center_vertically: bool,
               center_horizontally: bool) -> Self {
        debug_assert!(label_spacing  >= 0.0);
        debug_assert!(widget_spacing >= 0.0);
        Self {
            labels_and_widgets: Vec::new(),
            label_spacing,
            widget_spacing,
            center_vertically,
            center_horizontally,
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
    // TODO: Replace new() args with this struct. Provide defaults.
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
    font_scale: f32,
    size: UiTextButtonSize,
    hover: Option<UiTextureHandle>,
    enabled: bool,
    on_pressed: Box<dyn Fn(&UiTextButton, &mut UiWidgetContext) + 'static>,
}

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
        let text_color = if self.is_enabled() { [0.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 0.5] };
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

        // Invoke on pressed callback.
        if pressed && self.is_enabled() {
            (self.on_pressed)(self, context);
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

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiTextButton {
    pub fn new<OnPressed>(context: &mut UiWidgetContext,
                          label: String,
                          size: UiTextButtonSize,
                          hover: Option<&str>,
                          enabled: bool,
                          on_pressed: OnPressed) -> Self
        where OnPressed: Fn(&UiTextButton, &mut UiWidgetContext) + 'static
    {
        debug_assert!(!label.is_empty());
        Self {
            label,
            imgui_id: String::new(),
            font_scale: size.font_scale(),
            size,
            hover: hover.map(|path| helpers::load_ui_texture(context, path)),
            enabled,
            on_pressed: Box::new(on_pressed),
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
    Large,
}

impl UiTextButtonSize {
    pub const fn font_scale(self) -> f32 {
        match self {
            UiTextButtonSize::Normal => 1.2,
            UiTextButtonSize::Small  => 1.0,
            UiTextButtonSize::Large  => 1.5,
        }
    }
}

// ----------------------------------------------
// UiTextButtonParams
// ----------------------------------------------

pub struct UiTextButtonParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiSpriteButton
// ----------------------------------------------

// Multi-state sprite button. Works via state polling; state persists until changed.
pub struct UiSpriteButton {
    label: String,

    tooltip: Option<UiTooltipText>,
    show_tooltip_when_pressed: bool,

    size: Vec2,
    position: Vec2, // NOTE: Cached from ImGui on every draw().
    textures: UiSpriteButtonTextures,

    logical_state: UiSpriteButtonState,
    visual_state: UiSpriteButtonState,
    visual_state_transition_timer: CountdownTimer,
    state_transition_secs: Seconds,
}

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

        // NOTE: Only left click counts as "pressed".
        self.update_state(hovered, left_click, right_click, context.delta_time_secs);
        self.position = Vec2::from_array(rect_min);

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
    pub fn new(context: &mut UiWidgetContext,
               label: String,
               tooltip: Option<UiTooltipText>,
               show_tooltip_when_pressed: bool,
               size: Vec2,
               initial_state: UiSpriteButtonState,
               state_transition_secs: Seconds) -> Self {
        debug_assert!(!label.is_empty());
        debug_assert!(size.x > 0.0 && size.y > 0.0);
        debug_assert!(state_transition_secs >= 0.0);

        let textures = UiSpriteButtonTextures::load(&label, context);
        let visual_state_transition_timer = CountdownTimer::new(state_transition_secs);

        Self {
            label, 
            tooltip,
            show_tooltip_when_pressed,
            size,
            position: Vec2::zero(), // NOTE: Only valid after first draw().
            textures,
            logical_state: initial_state,
            visual_state: initial_state,
            visual_state_transition_timer,
            state_transition_secs,
        }
    }

    pub fn position(&self) -> Vec2 {
        self.position
    }

    pub fn is_enabled(&self) -> bool {
        self.logical_state != UiSpriteButtonState::Disabled
    }

    pub fn enable(&mut self, enable: bool) {
        if enable {
            self.logical_state = UiSpriteButtonState::Idle;
        } else {
            self.logical_state = UiSpriteButtonState::Disabled;
        }
    }

    pub fn is_pressed(&self) -> bool {
        self.logical_state == UiSpriteButtonState::Pressed
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

    fn update_state(&mut self, hovered: bool, left_click: bool, right_click: bool, delta_time_secs: Seconds) {
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

    fn load(name: &str, context: &mut UiWidgetContext) -> Self {
        let mut sprites = Self::unloaded();
        sprites.load_textures(name, context);
        sprites
    }

    fn load_textures(&mut self, name: &str, context: &mut UiWidgetContext) {
        for state in UiSpriteButtonState::iter() {
            self.textures[state as usize] = state.load_texture(name, context);
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

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum UiSpriteButtonState {
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
    fn asset_path(self, name: &str) -> PathBuf {
        debug_assert!(!name.is_empty());
        let sprite_suffix = self.get_str("Suffix").unwrap();

        // {name}_{sprite_suffix}.png
        let mut sprite_name = ArrayString::<128>::new();
        sprite_name.push_str(name);
        sprite_name.push_str("_");
        sprite_name.push_str(sprite_suffix);
        sprite_name.push_str(".png");

        assets_path().join("buttons").join(sprite_name)
    }

    fn load_texture(self, name: &str, context: &mut UiWidgetContext) -> UiTextureHandle {
        helpers::load_ui_texture(context, self.asset_path(name).to_str().unwrap())
    }
}

// ----------------------------------------------
// UiSpriteButtonParams
// ----------------------------------------------

pub struct UiSpriteButtonParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiTooltipText
// ----------------------------------------------

#[derive(Clone)]
pub struct UiTooltipText {
    text: String,
    font_scale: Option<f32>,
    background: Option<UiTextureHandle>,
}

impl UiTooltipText {
    pub fn new(context: &mut UiWidgetContext,
               text: String,
               font_scale: f32,
               background: Option<&str>) -> Self {
        debug_assert!(!text.is_empty());
        debug_assert!(font_scale > 0.0);
        Self {
            text,
            font_scale: if font_scale != 1.0 { Some(font_scale) } else { None },
            background: background.map(|path| helpers::load_ui_texture(context, path))
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

pub struct UiTooltipTextParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiSliderValue
// ----------------------------------------------

enum UiSliderValue {
    I32 {
        min: i32,
        max: i32,
        on_read_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext) -> i32 + 'static>,
        on_update_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext, i32) + 'static>,
    },
    U32 {
        min: u32,
        max: u32,
        on_read_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext) -> u32 + 'static>,
        on_update_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext, u32) + 'static>,
    },
    F32 {
        min: f32,
        max: f32,
        on_read_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext) -> f32 + 'static>,
        on_update_value: Box<dyn Fn(&UiSlider, &mut UiWidgetContext, f32) + 'static>,
    },
}

// ----------------------------------------------
// Macro: impl_slider_constructor
// ----------------------------------------------

macro_rules! impl_slider_constructor {
    ($value_type:ty, $enum_variant:ident, $func_name:ident) => {
        pub fn $func_name<OnReadVal, OnUpdateVal>(label: Option<String>,
                                                  font_scale: f32,
                                                  min: $value_type,
                                                  max: $value_type,
                                                  on_read_value: OnReadVal,
                                                  on_update_value: OnUpdateVal) -> Self
            where OnReadVal: Fn(&UiSlider, &mut UiWidgetContext) -> $value_type + 'static,
                  OnUpdateVal: Fn(&UiSlider, &mut UiWidgetContext, $value_type) + 'static
        {
            debug_assert!(font_scale > 0.0);
            Self {
                label: label.unwrap_or_default(),
                imgui_id: String::new(),
                font_scale,
                value: UiSliderValue::$enum_variant {
                    min,
                    max,
                    on_read_value: Box::new(on_read_value),
                    on_update_value: Box::new(on_update_value),
                }
            }
        }
    };
}

// ----------------------------------------------
// UiSlider
// ----------------------------------------------

pub struct UiSlider {
    label: String,
    imgui_id: String,
    font_scale: f32,
    value: UiSliderValue,
}

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
                let mut value = on_read_value(self, context);

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .build(&mut value);

                if value_changed {
                    on_update_value(self, context, value.clamp(*min, *max));
                }
            }
            UiSliderValue::U32 { min, max, on_read_value, on_update_value } => {
                let mut value = on_read_value(self, context);

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .build(&mut value);

                if value_changed {
                    on_update_value(self, context, value.clamp(*min, *max));
                }
            }
            UiSliderValue::F32 { min, max, on_read_value, on_update_value } => {
                let mut value = on_read_value(self, context);

                let (slider, _group) =
                    helpers::slider_with_left_label(ui, label, *min, *max);

                let value_changed = slider
                    .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                    .display_format("%.2f")
                    .build(&mut value);

                if value_changed {
                    on_update_value(self, context, value.clamp(*min, *max));
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

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiSlider {
    impl_slider_constructor! { i32, I32, from_i32 }
    impl_slider_constructor! { u32, U32, from_u32 }
    impl_slider_constructor! { f32, F32, from_f32 }
}

// ----------------------------------------------
// UiSliderParams
// ----------------------------------------------

pub struct UiSliderParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub struct UiCheckbox {
    label: String,
    imgui_id: String,
    font_scale: f32,
    on_read_value: Box<dyn Fn(&UiCheckbox, &mut UiWidgetContext) -> bool + 'static>,
    on_update_value: Box<dyn Fn(&UiCheckbox, &mut UiWidgetContext, bool) + 'static>,
}

impl UiWidget for UiCheckbox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiCheckbox, self.label);

        let mut value = (self.on_read_value)(self, context);

        let (value_changed, _group) =
            helpers::checkbox_with_left_label(ui, label, &mut value);

        if value_changed {
            (self.on_update_value)(self, context, value);
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

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiCheckbox {
    pub fn new<OnReadVal, OnUpdateVal>(label: Option<String>,
                                       font_scale: f32,
                                       on_read_value: OnReadVal,
                                       on_update_value: OnUpdateVal) -> Self
        where OnReadVal: Fn(&UiCheckbox, &mut UiWidgetContext) -> bool + 'static,
              OnUpdateVal: Fn(&UiCheckbox, &mut UiWidgetContext, bool) + 'static
    {
        debug_assert!(font_scale > 0.0);
        Self {
            label: label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale,
            on_read_value: Box::new(on_read_value),
            on_update_value: Box::new(on_update_value),
        }
    }
}

// ----------------------------------------------
// UiCheckboxParams
// ----------------------------------------------

pub struct UiCheckboxParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiTextInput
// ----------------------------------------------

pub struct UiTextInput {
    label: String,
    imgui_id: String,
    font_scale: f32,
    on_read_value: Box<dyn Fn(&UiTextInput, &mut UiWidgetContext) -> String + 'static>,
    on_update_value: Box<dyn Fn(&UiTextInput, &mut UiWidgetContext, String) + 'static>,
}

impl UiWidget for UiTextInput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiTextInput, self.label);

        let mut value = (self.on_read_value)(self, context);

        let (input, _group) =
            helpers::input_text_with_left_label(ui, label, &mut value);

        let value_changed = input.build();

        if value_changed {
            (self.on_update_value)(self, context, value);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        helpers::calc_labeled_widget_size(context, self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiTextInput {
    pub fn new<OnReadVal, OnUpdateVal>(label: Option<String>,
                                       font_scale: f32,
                                       on_read_value: OnReadVal,
                                       on_update_value: OnUpdateVal) -> Self
        where OnReadVal: Fn(&UiTextInput, &mut UiWidgetContext) -> String + 'static,
              OnUpdateVal: Fn(&UiTextInput, &mut UiWidgetContext, String) + 'static
    {
        debug_assert!(font_scale > 0.0);
        Self {
            label: label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale,
            on_read_value: Box::new(on_read_value),
            on_update_value: Box::new(on_update_value),
        }
    }
}

// ----------------------------------------------
// UiTextInputParams
// ----------------------------------------------

pub struct UiTextInputParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiDropdown
// ----------------------------------------------

pub struct UiDropdown {
    label: String,
    imgui_id: String,
    font_scale: f32,
    current_item: usize,
    items: Vec<String>,
    on_selection_changed: Box<dyn Fn(&UiDropdown, &mut UiWidgetContext, usize, &String) + 'static>,
}

impl UiWidget for UiDropdown {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        context.ui_sys.set_font_scale(self.font_scale);
        let ui = context.ui_sys.ui();

        let label = make_imgui_id!(self, UiDropdown, self.label);

        let (selection_changed, _group) =
            helpers::combo_with_left_label(ui, label, &mut self.current_item, &self.items);

        if selection_changed {
            (self.on_selection_changed)(self, context, self.current_item, &self.items[self.current_item]);
        }
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        helpers::calc_labeled_widget_size(context, self.font_scale, &self.label)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiDropdown {
    pub fn new<OnSelectionChanged>(label: Option<String>,
                                   font_scale: f32,
                                   on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiDropdown, &mut UiWidgetContext, usize, &String) + 'static
    {
        Self::from_strings(label, font_scale, 0, Vec::new(), on_selection_changed)
    }

    pub fn from_strings<OnSelectionChanged>(label: Option<String>,
                                            font_scale: f32,
                                            current_item: usize,
                                            items: Vec<String>,
                                            on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiDropdown, &mut UiWidgetContext, usize, &String) + 'static
    {
        debug_assert!(font_scale > 0.0);
        Self {
            label: label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale,
            current_item,
            items,
            on_selection_changed: Box::new(on_selection_changed),
        }
    }

    // From array of values implementing Display.
    pub fn from_values<OnSelectionChanged, V>(label: Option<String>,
                                              font_scale: f32,
                                              current_item: usize,
                                              values: &[V],
                                              on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiDropdown, &mut UiWidgetContext, usize, &String) + 'static,
              V: Display
    {
        let items: Vec<String> = values
            .iter()
            .map(|value| value.to_string())
            .collect();

        Self::from_strings(label, font_scale, current_item, items, on_selection_changed)
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

pub struct UiDropdownParams {
    // TODO: Replace new() args with this struct. Provide defaults.
}

// ----------------------------------------------
// UiItemList
// ----------------------------------------------

pub struct UiItemList {
    label: String,
    imgui_id: String,
    font_scale: f32,
    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,
    flags: UiItemListFlags,
    current_item: Option<usize>,
    items: Vec<String>,
    text_input_field_buffer: Option<String>,
    on_selection_changed: Box<dyn Fn(&UiItemList, &mut UiWidgetContext, Option<usize>, &String) + 'static>,
}

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

        if text_input_field_changed && let Some(text_input_field_buffer) = &self.text_input_field_buffer {
            // Invoke callback with `None` item index.
            (self.on_selection_changed)(self, context, None, text_input_field_buffer);
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
                    let selected_item = &self.items[selected_index];
                    (self.on_selection_changed)(self, context, Some(selected_index), selected_item);

                    if let Some(text_input_field_buffer) = &mut self.text_input_field_buffer {
                        text_input_field_buffer.clear();
                        text_input_field_buffer.push_str(selected_item);
                    }
                }
            });
    }

    fn measure(&self, context: &UiWidgetContext) -> Vec2 {
        context.ui_sys.set_font_scale(self.font_scale);

        let ui = context.ui_sys.ui();
        let style = unsafe { ui.style() };
        let parent_region_avail = ui.content_region_avail();

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - style.window_padding[0];
        }
        if self.margin_left > 0.0 {
            requested_size.x -= self.margin_left - style.window_padding[0];
        }

        let size = helpers::calc_child_window_size(requested_size.to_array(), parent_region_avail);

        let input_field_height = {
            if self.text_input_field_buffer.is_some() {
                ui.text_line_height() + (style.frame_padding[1] * 2.0)
            } else {
                0.0
            }
        };

        Vec2::new(size[0], size[1] + input_field_height)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn font_scale(&self) -> f32 {
        self.font_scale
    }
}

impl UiItemList {
    pub fn new<OnSelectionChanged>(label: Option<String>,
                                   font_scale: f32,
                                   size: Option<Vec2>,
                                   margin_left: f32,
                                   margin_right: f32,
                                   flags: UiItemListFlags,
                                   on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiItemList, &mut UiWidgetContext, Option<usize>, &String) + 'static
    {
        Self::from_strings(
            label,
            font_scale,
            size,
            margin_left,
            margin_right,
            flags,
            None,
            Vec::new(),
            on_selection_changed)
    }

    pub fn from_strings<OnSelectionChanged>(label: Option<String>,
                                            font_scale: f32,
                                            size: Option<Vec2>,
                                            margin_left: f32,
                                            margin_right: f32,
                                            flags: UiItemListFlags,
                                            current_item: Option<usize>,
                                            items: Vec<String>,
                                            on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiItemList, &mut UiWidgetContext, Option<usize>, &String) + 'static
    {
        debug_assert!(font_scale > 0.0);
        debug_assert!(margin_left > 0.0);
        debug_assert!(margin_right > 0.0);

        let text_input_field_buffer = {
            if flags.intersects(UiItemListFlags::TextInputField) {
                if let Some(initial_item) = current_item {
                    Some(items[initial_item].clone())
                } else {
                    Some(String::new())
                }
            } else {
                None
            }
        };

        Self {
            label: label.unwrap_or_default(),
            imgui_id: String::new(),
            font_scale,
            size,
            margin_left,
            margin_right,
            flags,
            current_item,
            items,
            text_input_field_buffer,
            on_selection_changed: Box::new(on_selection_changed),
        }
    }

    // From array of values implementing Display.
    pub fn from_values<OnSelectionChanged, V>(label: Option<String>,
                                              font_scale: f32,
                                              size: Option<Vec2>,
                                              margin_left: f32,
                                              margin_right: f32,
                                              flags: UiItemListFlags,
                                              current_item: Option<usize>,
                                              values: &[V],
                                              on_selection_changed: OnSelectionChanged) -> Self
        where OnSelectionChanged: Fn(&UiItemList, &mut UiWidgetContext, Option<usize>, &String) + 'static,
              V: Display
    {
        let items: Vec<String> = values
            .iter()
            .map(|value| value.to_string())
            .collect();

        Self::from_strings(
            label,
            font_scale,
            size,
            margin_left,
            margin_right,
            flags,
            current_item,
            items,
            on_selection_changed)
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

pub struct UiItemListParams {
    // TODO: Replace new() args with this struct. Provide defaults.
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

    fn font_scale(&self) -> f32 {
        self.menu.as_ref().map_or(1.0, |menu| menu.font_scale())
    }
}

impl UiMessageBox {
    #[inline]
    fn is_open(&self) -> bool {
        self.menu.is_some()
    }

    fn open(&mut self, context: &mut UiWidgetContext, params: UiMessageBoxParams) {
        let menu = UiMenu::new(
            context,
            params.label,
            UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter,
            params.size,
            None,
            params.background,
            UiMenu::NO_OPEN_CLOSE_CALLBACK);

        for widget in params.contents {
            menu.as_mut().add_widget(widget);
        }

        if !params.buttons.is_empty() {
            const BUTTON_SPACING: f32 = 10.0;
            const CENTER_VERTICALLY: bool = true;
            const CENTER_HORIZONTALLY: bool = true;
            const STACK_VERTICALLY: bool = false;

            let mut button_group = UiWidgetGroup::new(
                BUTTON_SPACING,
                CENTER_VERTICALLY,
                CENTER_HORIZONTALLY,
                STACK_VERTICALLY); // Render buttons side-by-side.

            for button in params.buttons {
                button_group.add_widget(button);
            }

            menu.as_mut().add_widget(button_group);
        }

        self.menu = Some(menu);
    }

    fn close(&mut self, _context: &mut UiWidgetContext) {
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
        let parent_region_avail = ui.content_region_avail();

        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            requested_size.x -= self.margin_right - style.window_padding[0];
        }
        if self.margin_left > 0.0 {
            requested_size.x -= self.margin_left - style.window_padding[0];
        }

        let size = helpers::calc_child_window_size(requested_size.to_array(), parent_region_avail);
        Vec2::from_array(size)
    }
}

impl UiSlideshow {
    pub fn new(context: &mut UiWidgetContext, params: UiSlideshowParams) -> Self {
        debug_assert!(!params.frames.is_empty());
        debug_assert!(params.frame_duration_secs > 0.0);

        let mut frames = Vec::with_capacity(params.frames.len());

        for path in params.frames {
            frames.push(helpers::load_ui_texture(context, path));
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
    pub frames: &'a [&'a str],

    // Ignored if UiSlideshowFlags::Fullscreen is set.
    pub size: Option<Vec2>,
    pub margin_left: f32,
    pub margin_right: f32,
}
