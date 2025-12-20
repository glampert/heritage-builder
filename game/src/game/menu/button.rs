use std::path::PathBuf;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets,
};
use crate::{
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
    fn unloaded() -> Self {
        Self { tex_handles: [INVALID_UI_TEXTURE_HANDLE; BUTTON_STATE_COUNT] }
    }

    fn load(name: &str, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let mut sprites = Self::unloaded();
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
    pub hidden: bool,
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
        let hidden = def.hidden;
        let countdown = def.state_transition_secs;
        Self {
            def,
            rect: Rect::default(),
            sprites: if hidden { ButtonSprites::unloaded() } else { ButtonSprites::load(name, tex_cache, ui_sys) },
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

        if widgets::is_debug_draw_enabled() {
            widgets::draw_debug_rect(&draw_list, &self.rect, Color::magenta());
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
