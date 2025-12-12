use std::{path::PathBuf, sync::atomic::{AtomicBool, Ordering}};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    TilePaletteSelection
};

use crate::{
    render::TextureCache,
    utils::{self, Size, Vec2, Rect, Color},
    imgui_ui::{UiSystem, UiTextureHandle, INVALID_UI_TEXTURE_HANDLE},
    tile::sets::{TileSets, TileDefHandle, PresetTiles, OBJECTS_BUILDINGS_CATEGORY, TERRAIN_LAND_CATEGORY},
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

    fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> bool {
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

        pressed
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
            ButtonState::Pressed  => {}
            ButtonState::Disabled => {}
        }
    }
}

// ----------------------------------------------
// TilePaletteMainButtonKind
// ----------------------------------------------

const TILE_PALETTE_MAIN_BUTTON_COUNT: usize = TilePaletteMainButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TilePaletteMainButtonKind {
    #[strum(props(Sprite = "palette/clear_land"))]
    ClearLand,

    #[strum(props(Sprite = "palette/housing", Tooltip = "Vacant Lot\nCost: 1 gold"))]
    Housing,

    #[strum(props(Sprite = "palette/roads", SeparatorFollows = true))]
    Roads,

    #[strum(props(Sprite = "palette/food_and_farming", DisableIfEmpty = true))]
    FoodAndFarming,

    #[strum(props(Sprite = "palette/industry_and_resources", DisableIfEmpty = true))]
    IndustryAndResources,

    #[strum(props(Sprite = "palette/services", DisableIfEmpty = true))]
    Services,

    #[strum(props(Sprite = "palette/infrastructure", DisableIfEmpty = true))]
    Infrastructure,

    #[strum(props(Sprite = "palette/culture_and_religion", DisableIfEmpty = true))]
    CultureAndReligion,

    #[strum(props(Sprite = "palette/trade_and_economy", DisableIfEmpty = true))]
    TradeAndEconomy,

    #[strum(props(Sprite = "palette/beautification", DisableIfEmpty = true))]
    Beautification,
}

impl TilePaletteMainButtonKind {
    const BUTTON_SIZE: Size = Size::new(50, 50);

    fn name(self) -> &'static str {
        let sprite_path = self.sprite_path();
        // Take the base sprite name following "palette/":
        let (_left, right) = sprite_path.split_at(sprite_path.find("/").unwrap() + 1);
        right
    }

    fn sprite_path(self) -> &'static str {
        self.get_str("Sprite").unwrap()
    }

    fn separator_follows(self) -> bool {
        self.get_bool("SeparatorFollows").is_some_and(|val| val)
    }

    fn initial_state(self, children: &[TilePaletteChildButton]) -> ButtonState {
        let disable_if_empty = self.get_bool("DisableIfEmpty").is_some_and(|val| val);
        if disable_if_empty && children.is_empty() {
            ButtonState::Disabled
        } else {
            ButtonState::Idle
        }
    }

    fn tooltip(self) -> String {
        if let Some(tooltip) = self.get_str("Tooltip") {
            tooltip.to_string()
        } else {
            utils::snake_case_to_title::<64>(self.name()).to_string()
        }
    }

    fn build_child_button_list(self) -> Vec<TilePaletteChildButton> {
        let mut children = Vec::new();
        let button_name = self.name();

        TileSets::get().for_each_category(|_, category| {
            if category.hash == OBJECTS_BUILDINGS_CATEGORY.hash ||
               category.hash == TERRAIN_LAND_CATEGORY.hash
            {
                category.for_each_tile_def(|tile_def| {
                    if tile_def.palette_button == button_name {
                        let label = utils::snake_case_to_title::<64>(&tile_def.name).to_string();
                        children.push(TilePaletteChildButton {
                            label,
                            tooltip: if tile_def.cost != 0 { format!("Cost: {} gold", tile_def.cost) } else { String::new() },
                            tile_def_handle: TileDefHandle::from_tile_def(tile_def)
                        });
                    }
                    true
                });
            }
            true
        });

        children
    }

    fn new_button(self) -> TilePaletteMainButton {
        let children = self.build_child_button_list();
        TilePaletteMainButton {
            btn: Button::new(
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip())
                },
                self.initial_state(&children),
            ),
            kind: self,
            children,
        }
    }

    fn create_all() -> [TilePaletteMainButton; TILE_PALETTE_MAIN_BUTTON_COUNT] {
        [
            Self::ClearLand.new_button(),
            Self::Housing.new_button(),
            Self::Roads.new_button(),
            Self::FoodAndFarming.new_button(),
            Self::IndustryAndResources.new_button(),
            Self::Services.new_button(),
            Self::Infrastructure.new_button(),
            Self::CultureAndReligion.new_button(),
            Self::TradeAndEconomy.new_button(),
            Self::Beautification.new_button(),
        ]
    }
}

// ----------------------------------------------
// TilePaletteMainButton
// ----------------------------------------------

struct TilePaletteMainButton {
    btn: Button,
    kind: TilePaletteMainButtonKind,
    children: Vec<TilePaletteChildButton>,
}

impl TilePaletteMainButton {
    fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    fn unpress(&mut self) {
        self.btn.unpress();
    }

    fn is_pressed(&self) -> bool {
        self.btn.is_pressed()
    }

    fn draw_main_button(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> bool {
        self.btn.draw(tex_cache, ui_sys)
    }

    fn draw_child_buttons(&mut self, _tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Option<usize> {
        if self.children.is_empty() {
            return None;
        }

        let ui = ui_sys.ui();

        const BUTTON_HEIGHT: f32 = 20.0;
        const LABEL_PADDING: f32 = 25.0;
        const VERTICAL_SPACING: f32 = 2.0;

        let mut longest_label: f32 = 0.0;
        for child in &self.children {
            let text_width = ui.calc_text_size(&child.label)[0];
            longest_label = longest_label.max(text_width);
        }

        let child_window_width = longest_label + (LABEL_PADDING * 2.0);
        let main_button_pos = self.btn.rect.position();

        let window_position = [
            main_button_pos.x - child_window_width,
            main_button_pos.y,
        ];

        let _item_spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing([0.0, VERTICAL_SPACING]));

        let _btn_color1 =
            ui.push_style_color(imgui::StyleColor::Button, [0.93, 0.91, 0.77, 1.0]);

        let _btn_color2 =
            ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.98, 0.95, 0.83, 1.0]);

        let _btn_color3 =
            ui.push_style_color(imgui::StyleColor::ButtonActive, [0.88, 0.83, 0.68, 1.0]);

        ui.window(format!("Child Window {:?}", self.kind))
            .position(window_position, imgui::Condition::Always)
            .flags(invisible_window_flags())
            .build(|| {
                let button_size = Vec2::new(longest_label + LABEL_PADDING, BUTTON_HEIGHT);
                let mut pressed_button_index: Option<usize> = None;

                for (index, child) in self.children.iter().enumerate() {
                    if ui.button_with_size(&child.label, button_size.to_array()) {
                        pressed_button_index = Some(index);
                    }

                    if ui.is_item_hovered() && !child.tooltip.is_empty() {
                        ui.tooltip_text(&child.tooltip);
                    }
                }

                if is_debug_draw_enabled() {
                    draw_current_window_debug_rect(ui);
                }

                pressed_button_index
            }).unwrap()
    }

    fn current_selection(&self, pressed_child_index: Option<usize>) -> TilePaletteSelection {
        if self.kind == TilePaletteMainButtonKind::ClearLand {
            return TilePaletteSelection::Clear;
        }

        if self.kind == TilePaletteMainButtonKind::Housing {
            if let Some(tile_def) = PresetTiles::VacantLot.find_tile_def() {
                return TilePaletteSelection::Tile(TileDefHandle::from_tile_def(tile_def));
            }
        }

        if let Some(child_index) = pressed_child_index {
            let pressed_child = &self.children[child_index];
            return TilePaletteSelection::Tile(pressed_child.tile_def_handle);
        }

        TilePaletteSelection::None
    }
}

// ----------------------------------------------
// TilePaletteChildButton
// ----------------------------------------------

struct TilePaletteChildButton {
    label: String,
    tooltip: String,
    tile_def_handle: TileDefHandle,
}

// ----------------------------------------------
// TilePaletteWidget
// ----------------------------------------------

pub struct TilePaletteWidget {
    pub current_selection: TilePaletteSelection,

    main_buttons: [TilePaletteMainButton; TILE_PALETTE_MAIN_BUTTON_COUNT],
    pressed_main_button: Option<usize>,
}

impl TilePaletteWidget {
    pub fn new() -> Self {
        Self {
            current_selection: TilePaletteSelection::None,
            main_buttons: TilePaletteMainButtonKind::create_all(),
            pressed_main_button: None,
        }
    }

    pub fn clear_selection(&mut self) {
        if let Some(pressed_index) = self.pressed_main_button {
            self.main_buttons[pressed_index].unpress();
        }

        self.current_selection = TilePaletteSelection::None;
        self.pressed_main_button = None;
    }

    pub fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        const BUTTON_SIZE: Vec2 = TilePaletteMainButtonKind::BUTTON_SIZE.to_vec2();
        const BUTTON_SPACING: Vec2 = Vec2::new(4.0, 4.0);

        const WINDOW_TOP_MARGIN: f32 = 15.0;
        const WINDOW_RIGHT_MARGIN: f32 = 15.0;
        const WINDOW_WIDTH: f32 = BUTTON_SIZE.x + BUTTON_SPACING.x;

        // X position = screen width - estimated window width - margin
        let window_position = [
            ui.io().display_size[0] - WINDOW_WIDTH - WINDOW_RIGHT_MARGIN,
            WINDOW_TOP_MARGIN
        ];

        let _window_bg_color =
            ui.push_style_color(imgui::StyleColor::WindowBg, [0.93, 0.91, 0.77, 1.0]);

        let _item_spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing(BUTTON_SPACING.to_array()));

        let _font =
            ui.push_font(ui_sys.fonts().game_hud);

        let _text_color =
            ui.push_style_color(imgui::StyleColor::Text, Color::black().to_array());

        let _tooltip_bg_color =
            ui.push_style_color(imgui::StyleColor::PopupBg, [0.93, 0.91, 0.77, 1.0]);

        let _tooltip_border =
            ui.push_style_var(imgui::StyleVar::PopupBorderSize(1.0)); // No border

        ui.window("Tile Palette Widget")
            .position(window_position, imgui::Condition::Always)
            .flags(invisible_window_flags())
            .build(|| {
                let previously_pressed_button = self.pressed_main_button;

                for (index, button) in self.main_buttons.iter_mut().enumerate() {
                    let was_pressed_this_frame = button.draw_main_button(tex_cache, ui_sys);

                    if button.kind.separator_follows() {
                        ui.separator();
                    }

                    // If a different button is now pressed, we'll reset the previous.
                    if button.is_pressed()
                        && self.pressed_main_button != Some(index)
                        && self.pressed_main_button == previously_pressed_button
                    {
                        self.pressed_main_button = Some(index);
                    }
                    // Same button pressed again.
                    else if was_pressed_this_frame
                        && self.pressed_main_button == Some(index)
                    {
                        self.current_selection = TilePaletteSelection::None;
                    }
                }

                // New button pressed, unpress other button:
                if previously_pressed_button != self.pressed_main_button {
                    if let Some(pressed_index) = previously_pressed_button {
                        let pressed_button = &mut self.main_buttons[pressed_index];
                        debug_assert!(pressed_button.is_pressed());
                        pressed_button.unpress();
                        self.current_selection = TilePaletteSelection::None;
                    }
                }

                // Draw child window of pressed button:
                if let Some(pressed_index) = self.pressed_main_button {
                    let pressed_button = &mut self.main_buttons[pressed_index];
                    if pressed_button.is_pressed() {
                        if pressed_button.has_children() {
                            // Keep the parent button pressed but close the child panel when we have a selection.
                            if self.current_selection.is_none() {
                                let pressed_child_index = pressed_button.draw_child_buttons(tex_cache, ui_sys);
                                self.current_selection = pressed_button.current_selection(pressed_child_index);
                            }
                            // Else hold current selection.
                        } else {
                            self.current_selection = pressed_button.current_selection(None);
                        }
                    } else {
                        self.current_selection = TilePaletteSelection::None;
                        self.pressed_main_button = None;
                    }
                }

                if is_debug_draw_enabled() {
                    draw_current_window_debug_rect(ui);
                }
            });
    }
}

// ----------------------------------------------
// ImGui helpers
// ----------------------------------------------

#[inline]
fn invisible_window_flags() -> imgui::WindowFlags {
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

fn draw_current_window_debug_rect(ui: &imgui::Ui) {
    // NOTE: Shrink the rect so it falls within the window bounds,
    // otherwise ImGui would cull it.
    let window_rect = Rect::new(
        Vec2::from_array(ui.window_pos()),
        Vec2::from_array(ui.window_size())
    ).shrunk(Vec2::new(4.0, 4.0));

    draw_debug_rect(&ui.get_window_draw_list(), &window_rect, Color::cyan());
}
