use std::{path::PathBuf, sync::atomic::{AtomicBool, Ordering}};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    game::sim::Simulation,
    utils::{Size, Vec2, Rect, Color},
    engine::time::{Seconds, CountdownTimer},
    render::{TextureCache, TextureSettings, TextureFilter},
    imgui_ui::{UiSystem, UiTextureHandle, INVALID_UI_TEXTURE_HANDLE},
};

// ----------------------------------------------
// ButtonState
// ----------------------------------------------

const BUTTON_STATE_COUNT: usize = ButtonState::COUNT;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum ButtonState {
    #[strum(props(Suffix = "idle"))]
    Idle,

    #[strum(props(Suffix = "disabled"))]
    Disabled,

    #[strum(props(Suffix = "hovered"))]
    Hovered,

    #[strum(props(Suffix = "pressed"))]
    Pressed,
}

impl ButtonState {
    fn asset_path(self, name: &str) -> PathBuf {
        debug_assert!(!name.is_empty());
        let sprite_suffix = self.get_str("Suffix").unwrap();
        let sprite_name = format!("{name}_{sprite_suffix}.png");
        super::ui_assets_path().join("buttons").join(sprite_name)
    }

    fn load_texture(self, name: &str, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> UiTextureHandle {
        let settings = TextureSettings {
            filter: TextureFilter::Linear,
            gen_mipmaps: false,
            ..Default::default()
        };

        let sprite_path = self.asset_path(name);
        let tex_handle = tex_cache.load_texture_with_settings(sprite_path.to_str().unwrap(), Some(settings));
        ui_sys.to_ui_texture(tex_cache, tex_handle)
    }
}

// ----------------------------------------------
// ButtonSprites
// ----------------------------------------------

struct ButtonSprites {
    tex_handles: [UiTextureHandle; BUTTON_STATE_COUNT],
}

impl ButtonSprites {
    fn new(name: &str, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let mut sprites = Self { tex_handles: [INVALID_UI_TEXTURE_HANDLE; BUTTON_STATE_COUNT] };
        sprites.load_textures(name, tex_cache, ui_sys);
        sprites
    }

    fn load_textures(&mut self, name: &str, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        for state in ButtonState::iter() {
            self.tex_handles[state as usize] = state.load_texture(name, tex_cache, ui_sys);
        }
    }

    #[inline]
    fn are_textures_loaded(&self) -> bool {
        self.tex_handles[0] != INVALID_UI_TEXTURE_HANDLE
    }

    #[inline]
    fn texture_for_state(&self, state: ButtonState) -> UiTextureHandle {
        debug_assert!(self.tex_handles[state as usize] != INVALID_UI_TEXTURE_HANDLE);
        self.tex_handles[state as usize]
    }
}

// ----------------------------------------------
// ButtonDef
// ----------------------------------------------

pub struct ButtonDef {
    pub name: &'static str,
    pub size: Size,
    pub tooltip: Option<String>,
    pub show_tooltip_when_pressed: bool,
    pub state_transition_secs: Seconds,
}

// ----------------------------------------------
// Button
// ----------------------------------------------

pub struct Button {
    pub def: ButtonDef,
    pub rect: Rect, // NOTE: Cached from ImGui on every draw().

    sprites: ButtonSprites,
    logical_state: ButtonState,

    visual_state: ButtonState,
    visual_state_transition_timer: CountdownTimer,
}

impl Button {
    pub fn new(tex_cache: &mut dyn TextureCache,
               ui_sys: &UiSystem,
               def: ButtonDef,
               initial_state: ButtonState) -> Self {
        let name = def.name;
        let countdown = def.state_transition_secs;
        Self {
            def,
            rect: Rect::default(),
            sprites: ButtonSprites::new(name, tex_cache, ui_sys),
            logical_state: initial_state,
            visual_state: initial_state,
            visual_state_transition_timer: CountdownTimer::new(countdown),
        }
    }

    pub fn draw(&mut self, ui_sys: &UiSystem, delta_time_secs: Seconds) -> bool {
        debug_assert!(self.sprites.are_textures_loaded());

        let ui = ui_sys.ui();
        let ui_texture = self.sprites.texture_for_state(self.visual_state);

        let flags = imgui::ButtonFlags::MOUSE_BUTTON_LEFT | imgui::ButtonFlags::MOUSE_BUTTON_RIGHT;
        ui.invisible_button_flags(self.name(), self.size(), flags);

        let hovered = ui.is_item_hovered();
        let left_click = ui.is_item_clicked_with_button(imgui::MouseButton::Left);
        let right_click = ui.is_item_clicked_with_button(imgui::MouseButton::Right);

        let rect_min = ui.item_rect_min();
        let rect_max = ui.item_rect_max();

        let draw_list = ui.get_window_draw_list();
        draw_list.add_image(ui_texture,
                            rect_min,
                            rect_max)
                            .build();

        self.rect = Rect::from_extents(Vec2::from_array(rect_min), Vec2::from_array(rect_max));
        self.update_state(hovered, left_click, right_click, delta_time_secs);

        let show_tooltip = hovered && (!self.is_pressed() || self.def.show_tooltip_when_pressed);

        if show_tooltip && let Some(tooltip) = &self.def.tooltip {
            ui.tooltip_text(tooltip);
        }

        if is_debug_draw_enabled() {
            draw_debug_rect(&draw_list, &self.rect, Color::magenta());
        }

        left_click // Only left click counts as "pressed".
    }

    #[inline]
    pub fn disable(&mut self) {
        self.logical_state = ButtonState::Disabled;
    }

    #[inline]
    pub fn enable(&mut self) {
        self.logical_state = ButtonState::Idle;
    }

    #[inline]
    pub fn unpress(&mut self) {
        if self.logical_state == ButtonState::Pressed {
            self.logical_state = ButtonState::Idle;
        }
    }

    #[inline]
    pub fn is_pressed(&self) -> bool {
        self.logical_state == ButtonState::Pressed
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        self.def.name
    }

    #[inline]
    pub fn size(&self) -> [f32; 2] {
        self.def.size.to_vec2().to_array()
    }

    #[inline]
    fn update_state(&mut self, hovered: bool, left_click: bool, right_click: bool, delta_time_secs: Seconds) {
        match self.logical_state {
            ButtonState::Idle | ButtonState::Hovered => {
                // Left click selects/presses button.
                if left_click {
                    self.logical_state = ButtonState::Pressed;
                } else if hovered {
                    self.logical_state = ButtonState::Hovered;
                } else {
                    self.logical_state = ButtonState::Idle;
                }
            }
            ButtonState::Pressed => {
                // Right click deselects/unpresses.
                if right_click {
                    self.logical_state = ButtonState::Idle;
                }
            }
            ButtonState::Disabled => {}
        }

        if left_click {
            // Reset transition if pressed.
            self.visual_state_transition_timer.reset(self.def.state_transition_secs);
        }

        if self.visual_state == ButtonState::Pressed {
            // Run a timed transition between idle/hovered and pressed.
            if self.visual_state_transition_timer.tick(delta_time_secs) {
                self.visual_state_transition_timer.reset(self.def.state_transition_secs);
                self.visual_state = self.logical_state;
            }
        } else {
            self.visual_state = self.logical_state;
        }
    }
}

// ----------------------------------------------
// ModalMenu
// ----------------------------------------------

// A modal popup window / dialog menu that pauses the game while open.
pub trait ModalMenu {
    fn is_open(&self) -> bool;
    fn open(&mut self, sim: &mut Simulation);
    fn close(&mut self, sim: &mut Simulation);
    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem);
}

pub struct BasicModalMenu {
    title: String,
    size: Option<Size>,
    is_open: bool,
}

impl BasicModalMenu {
    pub fn new(title: String, size: Option<Size>) -> Self {
        Self {
            title,
            size,
            is_open: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn open(&mut self, sim: &mut Simulation) {
        self.is_open = true;
        sim.pause();
    }

    pub fn close(&mut self, sim: &mut Simulation) {
        self.is_open = false;
        sim.resume();
    }

    pub fn draw<F>(&mut self, sim: &mut Simulation, ui_sys: &UiSystem, f: F)
        where F: FnOnce(&mut Simulation)
    {
        if !self.is_open {
            return;
        }

        let ui = ui_sys.ui();
        let display_size = ui.io().display_size;

        // Center popup window to the middle of the display:
        set_next_window_pos(
            Vec2::new(display_size[0] * 0.5, display_size[1] * 0.5),
            Vec2::new(0.5, 0.5),
            imgui::Condition::Always
        );

        let window_size = self.size.unwrap_or_default().to_vec2();
        let size_cond = if self.size.is_some() { imgui::Condition::Always } else { imgui::Condition::Never };

        let mut window_flags = invisible_window_flags();
        window_flags.remove(imgui::WindowFlags::NO_TITLE_BAR);

        let mut is_open = self.is_open;

        ui.window(&self.title)
            .opened(&mut is_open)
            .size(window_size.to_array(), size_cond)
            .flags(window_flags)
            .build(|| {
                f(sim);
                draw_current_window_debug_rect(ui);
            });

        // Resume game if closed by user.
        if !is_open {
            self.is_open = false;
            sim.resume();
        }
    }
}

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

#[inline]
pub fn invisible_window_flags() -> imgui::WindowFlags {
    imgui::WindowFlags::ALWAYS_AUTO_RESIZE
    | imgui::WindowFlags::NO_RESIZE
    | imgui::WindowFlags::NO_DECORATION
    | imgui::WindowFlags::NO_SCROLLBAR
    | imgui::WindowFlags::NO_MOVE
    | imgui::WindowFlags::NO_COLLAPSE
    //| imgui::WindowFlags::NO_BACKGROUND // Add this back when we switch to a sprite background.
}

pub struct UiStyleOverrides<'ui> {
    window_bg_color: imgui::ColorStackToken<'ui>,
    window_title_bg_color_active: imgui::ColorStackToken<'ui>,
    window_title_bg_color_inactive: imgui::ColorStackToken<'ui>,
    window_title_bg_color_collapsed: imgui::ColorStackToken<'ui>,

    text_color: imgui::ColorStackToken<'ui>,
    text_font: imgui::FontStackToken<'ui>,

    button_color: imgui::ColorStackToken<'ui>,
    button_color_hovered: imgui::ColorStackToken<'ui>,
    button_color_active: imgui::ColorStackToken<'ui>,

    // Tooltips:
    popup_bg_color: imgui::ColorStackToken<'ui>,
    popup_border_size: imgui::StyleStackToken<'ui>,
}

impl<'ui> UiStyleOverrides<'ui> {
    #[inline]
    #[must_use]
    pub fn dev_editor_menus(_ui_sys: &UiSystem) -> Option<Self> {
        None // Default style is already the dev/editor menus style.
    }

    #[inline]
    #[must_use]
    pub fn in_game_hud_menus(ui_sys: &UiSystem) -> Option<Self> {
        let ui = unsafe { &*ui_sys.raw_ui_ptr() };

        let bg_color    = [0.93, 0.91, 0.77, 1.0];
        let btn_hovered = [0.98, 0.95, 0.83, 1.0];
        let btn_active  = [0.88, 0.83, 0.68, 1.0];

        Some(Self {
            window_bg_color: ui.push_style_color(imgui::StyleColor::WindowBg, bg_color),
            window_title_bg_color_active: ui.push_style_color(imgui::StyleColor::TitleBgActive, bg_color),
            window_title_bg_color_inactive: ui.push_style_color(imgui::StyleColor::TitleBg, bg_color),
            window_title_bg_color_collapsed: ui.push_style_color(imgui::StyleColor::TitleBgCollapsed, bg_color),

            text_color: ui.push_style_color(imgui::StyleColor::Text, Color::black().to_array()),
            text_font: ui.push_font(ui_sys.fonts().game_hud_normal),

            button_color: ui.push_style_color(imgui::StyleColor::Button, bg_color),
            button_color_hovered: ui.push_style_color(imgui::StyleColor::ButtonHovered, btn_hovered),
            button_color_active: ui.push_style_color(imgui::StyleColor::ButtonActive, btn_active),

            // Tooltips:
            popup_bg_color: ui.push_style_color(imgui::StyleColor::PopupBg, bg_color),
            popup_border_size: ui.push_style_var(imgui::StyleVar::PopupBorderSize(1.0)), // No border
        })
    }

    #[inline]
    pub fn set_item_spacing(ui_sys: &'ui UiSystem, horizontal: f32, vertical: f32) -> imgui::StyleStackToken<'ui> {
        ui_sys.ui().push_style_var(imgui::StyleVar::ItemSpacing([horizontal, vertical]))
    }
}

pub struct UiStyleTextLabelInvisibleButtons<'ui> {
    border_size: imgui::StyleStackToken<'ui>,
    button_color: imgui::ColorStackToken<'ui>,
    button_color_hovered: imgui::ColorStackToken<'ui>,
    button_color_active: imgui::ColorStackToken<'ui>,
}

impl<'ui> UiStyleTextLabelInvisibleButtons<'ui> {
    #[inline]
    #[must_use]
    pub fn apply_overrides(ui_sys: &UiSystem) -> Self {
        let ui = unsafe { &*ui_sys.raw_ui_ptr() };

        Self {
            // We use buttons for text items so that the text label is centered automatically.
            // Make all button backgrounds and frames transparent/invisible.
            border_size: ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0)),
            button_color: ui.push_style_color(imgui::StyleColor::Button, [0.0, 0.0, 0.0, 0.0]),
            button_color_hovered: ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.0, 0.0, 0.0, 0.0]),
            button_color_active: ui.push_style_color(imgui::StyleColor::ButtonActive, [0.0, 0.0, 0.0, 0.0]),
        }
    }
}

pub fn draw_centered_button_group(ui: &imgui::Ui,
                                  draw_list: &imgui::DrawListMut<'_>,
                                  labels: &[&str],
                                  size: Option<Size>) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }

    let style = unsafe { ui.style() };

    let button_size = {
        if let Some(size) = size {
            size.to_vec2().to_array()
        } else {
            // Compute from labels:
            let mut longest_label = 0.0;
            let mut highest_label = 0.0;
            for label in labels {
                let label_size = ui.calc_text_size(label);
                if label_size[0] > longest_label {
                    longest_label = label_size[0];
                }
                if label_size[1] > highest_label {
                    highest_label = label_size[1];
                }
            }
            [longest_label + style.frame_padding[0], highest_label + style.frame_padding[1]]
        }
    };

    let spacing_y = style.item_spacing[1];

    let total_height =
        labels.len() as f32 * button_size[1] +
        (labels.len().saturating_sub(1) as f32 * spacing_y);

    let avail = ui.content_region_avail();
    let start_x = (avail[0] - button_size[0]) * 0.5;
    let start_y = (avail[1] - total_height) * 0.5;
    ui.set_cursor_pos([0.0, start_y]);

    let mut pressed_index = None;

    for (index, label) in labels.iter().enumerate() {
        let cursor = ui.cursor_pos();
        ui.set_cursor_pos([start_x, cursor[1]]);

        if ui.button_with_size(label, button_size) {
            pressed_index = Some(index);
        }

        draw_last_item_debug_rect(ui, draw_list, Color::blue());
    }

    pressed_index
}

// Draws a vertical separator immediately after the last submitted item.
// Call this *after* an item (Button, Text, Image, etc.) and typically before
// calling `same_line()` again.
pub fn draw_vertical_separator(ui: &imgui::Ui,
                               draw_list: &imgui::DrawListMut<'_>,
                               thickness: f32,
                               spacing_left: f32,
                               spacing_right: f32) {
    let item_min = ui.item_rect_min();
    let item_max = ui.item_rect_max();

    // Height matches the previous item.
    let y1 = item_min[1];
    let y2 = item_max[1];

    // X position is just to the right of the item.
    let x = item_max[0] + (spacing_left * 0.5);

    let color = ui.style_color(imgui::StyleColor::Separator);

    draw_list
        .add_line([x, y1], [x, y2], color)
        .thickness(thickness)
        .build();

    // Advance cursor so following items don't overlap the separator.
    ui.dummy([spacing_right, 0.0]);
}

pub fn draw_window_style_rect(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, min: Vec2, max: Vec2) {
    // Match window visuals:
    let style = unsafe { ui.style() };
    let rounding = style.window_rounding;
    let border_thickness = style.window_border_size;
    let bg_color = ui.style_color(imgui::StyleColor::WindowBg);
    let border_color = ui.style_color(imgui::StyleColor::Border);

    // Background:
    draw_list
        .add_rect(min.to_array(), max.to_array(), bg_color)
        .filled(true)
        .rounding(rounding)
        .build();

    // Border (pixel-aligned):
    if border_thickness > 0.0 {
        let inset = border_thickness * 0.5;
        let border_min = [min.x - inset, min.y - inset];
        let border_max = [max.x + inset, max.y + inset];

        draw_list
            .add_rect(border_min, border_max, border_color)
            .filled(false)
            .rounding(rounding)
            .thickness(border_thickness)
            .build();
    }
}

pub fn draw_sprite(ui: &imgui::Ui,
                   draw_list: &imgui::DrawListMut<'_>,
                   id: &str,
                   size: Vec2,
                   texture: UiTextureHandle,
                   tooltip: Option<&str>) {
    ui.invisible_button_flags(id, size.to_array(), imgui::ButtonFlags::empty());
    draw_list.add_image(texture, ui.item_rect_min(), ui.item_rect_max()).build();
    draw_last_item_debug_rect(ui, draw_list, Color::blue());

    if ui.is_item_hovered() && let Some(tooltip) = tooltip {
        ui.tooltip_text(tooltip);
    }
}

// NOTE: This assumes UiStyleTextLabelInvisibleButtons overrides are already set.
pub fn draw_centered_text_label(ui: &imgui::Ui,
                                draw_list: &imgui::DrawListMut<'_>,
                                label: &str,
                                size: Vec2) {
    ui.button_with_size(label, size.to_array());
    draw_last_item_debug_rect(ui, draw_list, Color::green());
}

pub fn spacing(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, size: Vec2) {
    ui.dummy(size.to_array());
    draw_last_item_debug_rect(ui, draw_list, Color::yellow());
}

pub fn set_next_window_pos(pos: Vec2, pivot: Vec2, cond: imgui::Condition) {
    unsafe {
        imgui::sys::igSetNextWindowPos(
            imgui::sys::ImVec2 { x: pos.x, y: pos.y },
            cond as _,
            imgui::sys::ImVec2 { x: pivot.x, y: pivot.y },
        );
    }
}

// ----------------------------------------------
// Debug helpers
// ----------------------------------------------

static ENABLE_WIDGETS_DEBUG_DRAW: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn enable_debug_draw(enable: bool) {
    ENABLE_WIDGETS_DEBUG_DRAW.store(enable, Ordering::Relaxed);
}

#[inline]
pub fn is_debug_draw_enabled() -> bool {
    ENABLE_WIDGETS_DEBUG_DRAW.load(Ordering::Relaxed)
}

pub fn draw_debug_rect(draw_list: &imgui::DrawListMut<'_>, rect: &Rect, color: Color) {
    draw_list.add_rect(rect.min.to_array(),
                       rect.max.to_array(),
                       imgui::ImColor32::from_rgba_f32s(color.r, color.g, color.b, color.a))
                       .build();
}

pub fn draw_last_item_debug_rect(ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, color: Color) {
    if !is_debug_draw_enabled() {
        return;
    }

    let rect = Rect::from_extents(
        Vec2::from_array(ui.item_rect_min()),
        Vec2::from_array(ui.item_rect_max())
    );

    draw_debug_rect(draw_list, &rect, color);
}

pub fn draw_current_window_debug_rect(ui: &imgui::Ui) {
    if !is_debug_draw_enabled() {
        return;
    }

    // NOTE: Shrink the rect so it falls within the window bounds,
    // otherwise ImGui would cull it.
    let window_rect = Rect::new(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    ).shrunk(Vec2::new(4.0, 4.0));

    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
}
