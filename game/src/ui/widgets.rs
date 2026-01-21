use std::any::Any;
use smallvec::SmallVec;
use bitflags::bitflags;
use enum_dispatch::enum_dispatch;

use super::{
    UiSystem,
    UiTextureHandle,
    assets_path,
    texture_settings,
};

use crate::{
    bitflags_with_display,
    tile::TileMap,
    render::TextureCache,
    engine::{Engine, time::Seconds},
    game::{world::World, sim::Simulation},
    utils::{Size, Vec2, Rect, mem::{self, RawPtr}},
};

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

    fn on_child_menu_opened(&mut self, _child_menu: &mut UiMenu) {}
    fn on_child_menu_closed(&mut self, _child_menu: &mut UiMenu) {}

    fn draw(&mut self, context: &mut UiWidgetContext);
}

#[enum_dispatch]
pub enum UiWidgetImpl {
    UiMenu,
    UiMenuHeading,
    UiTextButton,
    UiTextButtonGroup,
}

// ----------------------------------------------
// UiMenu
// ----------------------------------------------

pub struct UiMenu {
    title: String,
    flags: UiMenuFlags,
    size: Option<Vec2>,
    position: Option<Vec2>,
    background: Option<UiTextureHandle>,
    parent: Option<RawPtr<dyn UiWidget>>,
    widgets: Vec<UiWidgetImpl>,
}

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

        let window_name = {
            if self.title.is_empty() {
                // Use widget memory address as unique id if no title.
                &format!("##UiMenu @ {:p}", self)
            } else {
                &self.title
            }
        };

        let mut is_open = self.is_open();

        helpers::set_next_widget_window_pos(window_pos, window_pivot, window_pos_cond);

        ui.window(window_name)
            .opened(&mut is_open)
            .size(window_size.to_array(), window_size_cond)
            .flags(window_flags)
            .build(|| {
                if let Some(background) = self.background {
                    helpers::draw_widget_window_background(ui, background);
                }

                for widget in &mut self.widgets {
                    widget.draw(context);
                }
            });

        self.flags.set(UiMenuFlags::IsOpen, is_open);
    }
}

impl UiMenu {
    pub fn new(context: &mut UiWidgetContext,
               title: String,
               flags: UiMenuFlags,
               size: Option<Vec2>,
               position: Option<Vec2>,
               background: Option<&str>,
               parent: Option<&dyn UiWidget>) -> Self {
        Self {
            title,
            flags,
            size,
            position,
            background: background.map(|path| helpers::load_ui_texture(context, path)),
            parent: parent.map(RawPtr::from_ref),
            widgets: Vec::new(),
        }
    }

    pub fn has_flags(&self, flags: UiMenuFlags) -> bool {
        self.flags.intersects(flags)
    }

    pub fn parent(&self) -> Option<&dyn UiWidget> {
        self.parent.as_ref().map(|p| p.as_ref())
    }

    pub fn parent_mut(&mut self) -> Option<&mut dyn UiWidget> {
        self.parent.as_mut().map(|p| p.as_mut())
    }

    pub fn is_open(&self) -> bool {
        self.has_flags(UiMenuFlags::IsOpen)
    }

    pub fn open(&mut self, context: &mut UiWidgetContext) {
        self.flags.insert(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.pause();
        }

        if self.parent.is_some() {
            self.parent.unwrap().on_child_menu_opened(self);
        }
    }

    pub fn close(&mut self, context: &mut UiWidgetContext) {
        self.flags.remove(UiMenuFlags::IsOpen);

        if self.has_flags(UiMenuFlags::PauseSimIfOpen) {
            context.sim.resume();
        }

        if self.parent.is_some() {
            self.parent.unwrap().on_child_menu_closed(self);
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

        if self.background.is_none() && !self.title.is_empty() {
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
        if self.lines.is_empty() {
            return;
        }

        let ui = context.ui_sys.ui();
        ui.set_window_font_scale(self.font_scale);

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
}

impl UiMenuHeading {
    pub fn new(context: &mut UiWidgetContext,
               font_scale: f32,
               lines: Vec<String>,
               separator: Option<&str>,
               margin_top: f32,
               margin_bottom: f32) -> Self {
        debug_assert!(font_scale > 0.0);
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
// UiTextButton
// ----------------------------------------------

// Simple text label button. State does not "stick",
// one click equals one call to `on_pressed`, then
// immediately back to unpressed state.
pub struct UiTextButton {
    label: String,
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
        let ui = context.ui_sys.ui();
        ui.set_window_font_scale(self.font_scale());

        // NOTE: Using widget's memory address as its unique id.
        let label = format!("{}##UiButton {} @ {:p}", self.label, self.label, self);

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
        Self {
            label,
            size,
            hover: hover.map(|path| helpers::load_ui_texture(context, path)),
            enabled,
            on_pressed: Box::new(on_pressed),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn enable(&mut self, enable: bool) {
        self.enabled = enable;
    }

    fn font_scale(&self) -> f32 {
        match self.size {
            UiTextButtonSize::Normal => 1.2,
            UiTextButtonSize::Small  => 1.0,
            UiTextButtonSize::Large  => 1.5,
        }
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

// ----------------------------------------------
// UiTextButtonGroup
// ----------------------------------------------

// Groups UiTextButtons to draw them centered/aligned.
// Supports vertical and horizontal alignment.
pub struct UiTextButtonGroup {
    buttons: Vec<UiTextButton>,
    button_spacing: f32,
    center_vertically: bool,
    center_horizontally: bool,
}

impl UiWidget for UiTextButtonGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        let ui = context.ui_sys.ui();

        let _spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing([self.button_spacing, self.button_spacing]));

        helpers::draw_centered_text_button_group(
            ui,
            context,
            &mut self.buttons,
            self.center_vertically,
            self.center_horizontally
        );
    }
}

impl UiTextButtonGroup {
    pub fn new(button_spacing: f32, center_vertically: bool, center_horizontally: bool) -> Self {
        Self {
            buttons: Vec::new(),
            button_spacing,
            center_vertically,
            center_horizontally,
        }
    }

    pub fn add_button(&mut self, button: UiTextButton) -> &mut Self {
        self.buttons.push(button);
        self
    }
}

// ----------------------------------------------
// UiSpriteButton
// ----------------------------------------------

pub struct UiSpriteButton {
    // TODO
}

// ----------------------------------------------
// UiSlider
// ----------------------------------------------

pub struct UiSlider {
    // TODO
}

// ----------------------------------------------
// UiCheckbox
// ----------------------------------------------

pub struct UiCheckbox {
    // TODO
}

// ----------------------------------------------
// UiTextInput
// ----------------------------------------------

pub struct UiTextInput {
    // TODO
}

// ----------------------------------------------
// UiDropdown
// ----------------------------------------------

pub struct UiDropdown {
    // TODO
}

// ----------------------------------------------
// UiItemList
// ----------------------------------------------

pub struct UiItemList {
    // TODO
}

// ----------------------------------------------
// UiMessageBox
// ----------------------------------------------

pub struct UiMessageBox {
    // TODO
}

// ----------------------------------------------
// UiSlideshow
// ----------------------------------------------

pub struct UiSlideshow {
    // TODO
    // To replace AnimatedFullScreenBackground
    // make it so that it can be either the background of a window or fullscreen background.
}

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

mod helpers {
    use super::*;

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

    pub fn load_ui_texture(context: &mut UiWidgetContext, path: &str) -> UiTextureHandle {
        let file_path = assets_path().join(path);
        let tex_handle = context.tex_cache.load_texture_with_settings(
            file_path.to_str().unwrap(),
            Some(texture_settings())
        );
        context.ui_sys.to_ui_texture(context.tex_cache, tex_handle)
    }

    pub fn set_next_widget_window_pos(pos: Vec2, pivot: Vec2, cond: imgui::Condition) {
        unsafe {
            imgui::sys::igSetNextWindowPos(
                imgui::sys::ImVec2 { x: pos.x, y: pos.y },
                cond as _,
                imgui::sys::ImVec2 { x: pivot.x, y: pivot.y },
            );
        }
    }

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

        let avail = ui.content_region_avail();
        let cursor_start = ui.cursor_pos();

        // Compute group origin (top-left):
        let start_x = if horizontal { cursor_start[0] + ((avail[0] - max_width)    * 0.5) } else { cursor_start[0] };
        let start_y = if vertical   { cursor_start[1] + ((avail[1] - total_height) * 0.5) } else { cursor_start[1] };

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

    pub fn draw_centered_text_button_group(ui: &imgui::Ui,
                                           context: &mut UiWidgetContext,
                                           buttons: &mut [UiTextButton],
                                           vertical: bool,
                                           horizontal: bool) -> Rect {
        if buttons.is_empty() {
            return Rect::zero();
        }

        // Measure button sizes:
        let button_sizes: SmallVec<[[f32; 2]; 16]> = buttons
            .iter()
            .map(|btn| button_size_for_label(ui, btn.label(), btn.font_scale()))
            .collect();

        let spacing = unsafe { ui.style().item_spacing };

        let max_width = button_sizes
            .iter()
            .map(|btn| btn[0])
            .fold(0.0, f32::max);

        let total_height = button_sizes
            .iter()
            .map(|btn| btn[1])
            .fold(0.0, |total, height| total + height)
            + (spacing[1] * (buttons.len() - 1) as f32);

        let avail = ui.content_region_avail();
        let cursor_start = ui.cursor_pos();

        // Compute group origin (top-left):
        let start_x = if horizontal { cursor_start[0] + ((avail[0] - max_width)    * 0.5) } else { cursor_start[0] };
        let start_y = if vertical   { cursor_start[1] + ((avail[1] - total_height) * 0.5) } else { cursor_start[1] };

        // Draw each button:
        let mut offset_y = 0.0;
        for (btn, size) in buttons.iter_mut().zip(button_sizes.iter()) {
            let x = start_x + (max_width - size[0]) * 0.5;
            let y = start_y + offset_y;

            offset_y += size[1] + spacing[1];
            ui.set_cursor_pos([x, y]);

            btn.draw(context);
        }

        // Restore cursor so layout continues correctly.
        ui.set_cursor_pos([cursor_start[0], start_y + total_height]);

        // Return window relative position of group start + group size.
        Rect::from_pos_and_size(Vec2::new(start_x, start_y), Vec2::new(max_width, total_height))
    }

    pub fn button_size_for_label(ui: &imgui::Ui, label: &str, font_scale: f32) -> [f32; 2] {
        ui.set_window_font_scale(font_scale);

        let style = unsafe { ui.style() };

        let font_size = ui.current_font_size();
        let text_size = ui.calc_text_size(label);

        let width  = text_size[0] + (style.frame_padding[0] * 2.0);
        let height = text_size[1].max(font_size) + (style.frame_padding[1] * 2.0);

        [width, height]
    }
}
