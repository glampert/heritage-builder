use std::{path::PathBuf, sync::atomic::{AtomicBool, Ordering}};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    render::TextureCache,
    utils::{self, Size, Vec2, Rect, Color},
    imgui_ui::{UiSystem, UiTextureHandle, INVALID_UI_TEXTURE_HANDLE},
};

// ----------------------------------------------
// ButtonState
// ----------------------------------------------

const BUTTON_STATE_COUNT: usize = ButtonState::COUNT;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum ButtonState {
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

struct ButtonDef {
    name: &'static str,
    size: Size,
    tooltip: Option<String>,
}

// ----------------------------------------------
// Button
// ----------------------------------------------

struct Button {
    def: ButtonDef,
    sprites: ButtonSprites,
    state: ButtonState,
    rect: Rect,
}

impl Button {
    fn new(def: ButtonDef, initial_state: ButtonState) -> Self {
        Self {
            def,
            sprites: ButtonSprites::new(),
            state: initial_state,
            rect: Rect::default(), // Cached from ImGui on every draw().
        }
    }

    fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        if !self.sprites.are_textures_loaded() {
            self.sprites.load_textures(self.name(), tex_cache, ui_sys);
        }

        let ui = ui_sys.ui();
        let ui_texture = self.sprites.texture_for_state(self.state);

        let flags = imgui::ButtonFlags::MOUSE_BUTTON_LEFT;
        let pressed = ui.invisible_button_flags(self.name(), self.size(), flags);
        let hovered = ui.is_item_hovered();

        let rect_min = ui.item_rect_min();
        let rect_max = ui.item_rect_max();

        let draw_list = ui.get_window_draw_list();
        draw_list.add_image(ui_texture,
                            rect_min,
                            rect_max)
                            .build();

        self.rect = Rect::from_extents(Vec2::from_array(rect_min), Vec2::from_array(rect_max));
        self.update_state(pressed, hovered);

        if hovered && !self.is_pressed() && let Some(tooltip) = &self.def.tooltip {
            ui.tooltip_text(tooltip);
        }

        if is_debug_draw_enabled() {
            draw_debug_rect(&draw_list, &self.rect, Color::magenta());
        }
    }

    fn disable(&mut self) {
        self.state = ButtonState::Disabled;
    }

    fn enable(&mut self) {
        self.state = ButtonState::Idle;
    }

    fn unpress(&mut self) {
        if self.state == ButtonState::Pressed {
            self.state = ButtonState::Idle;
        }
    }

    fn is_pressed(&self) -> bool {
        self.state == ButtonState::Pressed
    }

    fn name(&self) -> &'static str {
        self.def.name
    }

    fn size(&self) -> [f32; 2] {
        self.def.size.to_vec2().to_array()
    }

    fn update_state(&mut self, pressed: bool, hovered: bool) {
        match self.state {
            ButtonState::Idle | ButtonState::Hovered => {
                if pressed {
                    self.state = ButtonState::Pressed;
                } else if hovered {
                    self.state = ButtonState::Hovered;
                } else {
                    self.state = ButtonState::Idle;
                }
            }
            ButtonState::Pressed => {
                // Press again toggles back to idle.
                if pressed {
                    self.state = ButtonState::Idle;
                }
            }
            ButtonState::Disabled => {}
        }
    }
}

// ----------------------------------------------
// TilePaletteButton / TilePaletteButtonDef
// ----------------------------------------------

const TILE_PALETTE_BUTTON_COUNT: usize = TilePaletteButtonDef::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TilePaletteButtonDef {
    #[strum(props(Sprite = "palette/clear"))]
    Clear,

    #[strum(props(Sprite = "palette/road"))]
    Road,

    #[strum(props(Sprite = "palette/housing"))]
    Housing,

    #[strum(props(Sprite = "palette/food_and_farming"))]
    FoodAndFarming,

    #[strum(props(Sprite = "palette/industry_and_resources"))]
    IndustryAndResources,

    #[strum(props(Sprite = "palette/services"))]
    Services,

    #[strum(props(Sprite = "palette/infrastructure"))]
    Infrastructure,

    #[strum(props(Sprite = "palette/culture_and_religion"))]
    CultureAndReligion,

    #[strum(props(Sprite = "palette/trade_and_economy"))]
    TradeAndEconomy,

    #[strum(props(Sprite = "palette/beautification"))]
    Beautification,
}

impl TilePaletteButtonDef {
    const BUTTON_SIZE: Size = Size::new(50, 50);

    fn new_button(self, enabled: bool) -> TilePaletteButton {
        let initial_state = if enabled {
            ButtonState::Idle
        } else {
            ButtonState::Disabled
        };

        // Take the base sprite name following "palette/":
        let name = self.get_str("Sprite").unwrap();
        let (_left, right) = name.split_at(name.find("/").unwrap() + 1);
        let tooltip = utils::snake_case_to_title::<64>(right).to_string();

        TilePaletteButton {
            def: self,
            btn: Button::new(
                ButtonDef { name, size: Self::BUTTON_SIZE, tooltip: Some(tooltip) },
                initial_state
            )
        }
    }

    fn create_all() -> [TilePaletteButton; TILE_PALETTE_BUTTON_COUNT] {
        [
            Self::Clear.new_button(true),
            Self::Road.new_button(true),
            Self::Housing.new_button(true),
            Self::FoodAndFarming.new_button(true),
            Self::IndustryAndResources.new_button(true),
            Self::Services.new_button(true),
            Self::Infrastructure.new_button(true),
            Self::CultureAndReligion.new_button(true),
            Self::TradeAndEconomy.new_button(true),
            Self::Beautification.new_button(true),
        ]
    }
}

struct TilePaletteButton {
    def: TilePaletteButtonDef,
    btn: Button,
}

impl TilePaletteButton {
    fn unpress(&mut self) {
        self.btn.unpress();
    }

    fn is_pressed(&self) -> bool {
        self.btn.is_pressed()
    }

    fn draw_main_button(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        self.btn.draw(tex_cache, ui_sys);
    }

    fn draw_child_buttons(&mut self, _tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let child_window_width = ui.calc_text_size("Button Three")[0] + 25.0; // TEMP
        let main_button_pos = self.btn.rect.position();

        let window_position = [
            main_button_pos.x - child_window_width,
            main_button_pos.y,
        ];

        ui.window(format!("Child Window {:?}", self.def))
            .position(window_position, imgui::Condition::Always)
            .flags(invisible_window_flags())
            .build(|| {
                ui.button("Button One");
                ui.button("Button Two");
                ui.button("Button Three");

                if is_debug_draw_enabled() {
                    // NOTE: Shrink the rect so it falls within the window bounds,
                    // otherwise ImGui would cull it.
                    let window_rect = Rect::new(
                        Vec2::from_array(ui.window_pos()),
                        Vec2::from_array(ui.window_size())
                    ).shrunk(Vec2::new(4.0, 4.0));

                    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
                }
            });
    }
}

// ----------------------------------------------
// TilePaletteWidget
// ----------------------------------------------

pub struct TilePaletteWidget {
    main_buttons: [TilePaletteButton; TILE_PALETTE_BUTTON_COUNT],
    pressed_main_button: Option<usize>,
}

impl TilePaletteWidget {
    pub fn new() -> Self {
        Self {
            main_buttons: TilePaletteButtonDef::create_all(),
            pressed_main_button: None,
        }
    }

    pub fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        const BUTTON_SIZE: Vec2 = TilePaletteButtonDef::BUTTON_SIZE.to_vec2();
        const BUTTON_SPACING: Vec2 = Vec2::new(4.0, 4.0);

        const WINDOW_TOP_MARGIN: f32 = 15.0;
        const WINDOW_RIGHT_MARGIN: f32 = 15.0;
        const WINDOW_WIDTH: f32 = BUTTON_SIZE.x + BUTTON_SPACING.x;

        // X position = screen width - estimated window width - margin
        let window_position = [
            ui.io().display_size[0] - WINDOW_WIDTH - WINDOW_RIGHT_MARGIN,
            WINDOW_TOP_MARGIN
        ];

        let _item_spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing(BUTTON_SPACING.to_array()));

        ui.window("Tile Palette Widget")
            .position(window_position, imgui::Condition::Always)
            .flags(invisible_window_flags())
            .build(|| {
                let previously_pressed_button = self.pressed_main_button;

                for (index, button) in self.main_buttons.iter_mut().enumerate() {
                    button.draw_main_button(tex_cache, ui_sys);

                    // If a different button is now pressed, we'll reset all the others.
                    if button.is_pressed()
                        && self.pressed_main_button != Some(index)
                        && self.pressed_main_button == previously_pressed_button
                    {
                        self.pressed_main_button = Some(index);
                    }
                }

                // New button pressed, unpress other button:
                if previously_pressed_button != self.pressed_main_button {
                    if let Some(pressed_index) = previously_pressed_button {
                        let pressed_button = &mut self.main_buttons[pressed_index];
                        debug_assert!(pressed_button.is_pressed());
                        pressed_button.unpress();
                    }
                }

                // Draw child window of pressed button:
                if let Some(pressed_index) = self.pressed_main_button {
                    let pressed_button = &mut self.main_buttons[pressed_index];
                    if pressed_button.is_pressed() {
                        pressed_button.draw_child_buttons(tex_cache, ui_sys);
                    } else {
                        self.pressed_main_button = None;
                    }
                }

                if is_debug_draw_enabled() {
                    // NOTE: Shrink the rect so it falls within the window bounds,
                    // otherwise ImGui would cull it.
                    let window_rect = Rect::new(
                        Vec2::from_array(ui.window_pos()),
                        Vec2::from_array(ui.window_size())
                    ).shrunk(Vec2::new(4.0, 4.0));

                    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
                }
            });
    }
}

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

#[inline]
fn invisible_window_flags() -> imgui::WindowFlags {
    imgui::WindowFlags::NO_DECORATION
    | imgui::WindowFlags::NO_BACKGROUND
    | imgui::WindowFlags::NO_RESIZE
    | imgui::WindowFlags::NO_SCROLLBAR
    | imgui::WindowFlags::NO_MOVE
    | imgui::WindowFlags::NO_COLLAPSE
    | imgui::WindowFlags::ALWAYS_AUTO_RESIZE
}

// ----------------------------------------------
// Global debug switches / helpers
// ----------------------------------------------

static ENABLE_WIDGETS_DEBUG_DRAW: AtomicBool = AtomicBool::new(true);

#[inline]
fn enable_debug_draw(enable: bool) {
    ENABLE_WIDGETS_DEBUG_DRAW.store(enable, Ordering::Relaxed);
}

#[inline]
fn is_debug_draw_enabled() -> bool {
    ENABLE_WIDGETS_DEBUG_DRAW.load(Ordering::Relaxed)
}

fn draw_debug_rect(draw_list: &imgui::DrawListMut<'_>, rect: &Rect, color: Color) {
    draw_list.add_rect(rect.min.to_array(),
                       rect.max.to_array(),
                       imgui::ImColor32::from_rgba_f32s(color.r, color.g, color.b, color.a))
                       .build();
}
