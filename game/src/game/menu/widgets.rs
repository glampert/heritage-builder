use std::{path::PathBuf, sync::atomic::{AtomicBool, Ordering}};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    render::TextureCache,
    utils::{Size, Vec2, Rect, Color},
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
        let sprite_path = self.asset_path(name);
        let tex_handle = tex_cache.load_texture(sprite_path.to_str().unwrap());
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
    fn new() -> Self {
        Self { tex_handles: [INVALID_UI_TEXTURE_HANDLE; BUTTON_STATE_COUNT] }
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
}

// ----------------------------------------------
// Button
// ----------------------------------------------

pub struct Button {
    pub def: ButtonDef,
    pub rect: Rect, // NOTE: Cached from ImGui on every draw().
    sprites: ButtonSprites,
    state: ButtonState,
}

impl Button {
    pub fn new(def: ButtonDef, initial_state: ButtonState) -> Self {
        Self {
            def,
            rect: Rect::default(),
            sprites: ButtonSprites::new(),
            state: initial_state,
        }
    }

    pub fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> bool {
        if !self.sprites.are_textures_loaded() {
            self.sprites.load_textures(self.name(), tex_cache, ui_sys);
        }

        let ui = ui_sys.ui();
        let ui_texture = self.sprites.texture_for_state(self.state);

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
        self.update_state(hovered, left_click, right_click);

        if hovered && !self.is_pressed() && let Some(tooltip) = &self.def.tooltip {
            ui.tooltip_text(tooltip);
        }

        if is_debug_draw_enabled() {
            draw_debug_rect(&draw_list, &self.rect, Color::magenta());
        }

        left_click // Only left click counts as "pressed".
    }

    #[inline]
    pub fn disable(&mut self) {
        self.state = ButtonState::Disabled;
    }

    #[inline]
    pub fn enable(&mut self) {
        self.state = ButtonState::Idle;
    }

    #[inline]
    pub fn unpress(&mut self) {
        if self.state == ButtonState::Pressed {
            self.state = ButtonState::Idle;
        }
    }

    #[inline]
    pub fn is_pressed(&self) -> bool {
        self.state == ButtonState::Pressed
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
    fn update_state(&mut self, hovered: bool, left_click: bool, right_click: bool) {
        match self.state {
            ButtonState::Idle | ButtonState::Hovered => {
                // Left click selects/presses button.
                if left_click {
                    self.state = ButtonState::Pressed;
                } else if hovered {
                    self.state = ButtonState::Hovered;
                } else {
                    self.state = ButtonState::Idle;
                }
            }
            ButtonState::Pressed => {
                // Right click deselects/unpresses.
                if right_click {
                    self.state = ButtonState::Idle;
                }
            }
            ButtonState::Disabled => {}
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

pub fn draw_current_window_debug_rect(ui: &imgui::Ui) {
    // NOTE: Shrink the rect so it falls within the window bounds,
    // otherwise ImGui would cull it.
    let window_rect = Rect::new(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    ).shrunk(Vec2::new(4.0, 4.0));

    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
}
