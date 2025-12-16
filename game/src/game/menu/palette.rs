use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    TilePaletteSelection,
    widgets::{self, Button, ButtonState, ButtonDef, UiStyleOverrides},
};
use crate::{
    imgui_ui::UiSystem,
    render::{RenderSystem, TextureCache, TextureHandle, TextureSettings, TextureFilter},
    utils::{self, Size, Vec2, Color, Rect, RectTexCoords, coords::WorldToScreenTransform},
    tile::{
        TileKind, BASE_TILE_SIZE, rendering::INVALID_TILE_COLOR,
        sets::{TileSets, TileDefHandle, PresetTiles, OBJECTS_BUILDINGS_CATEGORY, TERRAIN_LAND_CATEGORY}
    },
};

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

    fn sprite_path(self) -> &'static str {
        self.get_str("Sprite").unwrap()
    }

    fn name(self) -> &'static str {
        let sprite_path = self.sprite_path();
        // Take the base sprite name following "palette/":
        sprite_path.split_at(sprite_path.find("/").unwrap() + 1).1
    }

    fn tooltip(self) -> String {
        if let Some(tooltip) = self.get_str("Tooltip") {
            return tooltip.to_string();
        }
        utils::snake_case_to_title::<64>(self.name()).to_string()
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

    fn new_button(self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> TilePaletteMainButton {
        let children = self.build_child_button_list();
        TilePaletteMainButton {
            btn: Button::new(
                tex_cache,
                ui_sys,
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

    fn create_all(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem)
                  -> ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>
    {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(tex_cache, ui_sys));
        }
        buttons
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

    fn draw_main_button(&mut self, ui_sys: &UiSystem) -> bool {
        self.btn.draw(ui_sys)
    }

    fn draw_child_buttons(&mut self, ui_sys: &UiSystem) -> Option<usize> {
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
            UiStyleOverrides::set_item_spacing(ui_sys, 0.0, VERTICAL_SPACING);

        ui.window(format!("Child Window {:?}", self.kind))
            .position(window_position, imgui::Condition::Always)
            .flags(widgets::invisible_window_flags())
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

                if widgets::is_debug_draw_enabled() {
                    widgets::draw_current_window_debug_rect(ui);
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

    main_buttons: ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    pressed_main_button: Option<usize>,

    clear_icon_sprite: TextureHandle,
}

impl TilePaletteWidget {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let settings = TextureSettings {
            filter: TextureFilter::Linear,
            gen_mipmaps: false,
            ..Default::default()
        };

        let file_path = super::ui_assets_path().join("red_x_icon.png");
        let clear_icon_sprite = tex_cache.load_texture_with_settings(
            file_path.to_str().unwrap(),
            Some(settings)
        );

        Self {
            current_selection: TilePaletteSelection::None,
            main_buttons: TilePaletteMainButtonKind::create_all(tex_cache, ui_sys),
            pressed_main_button: None,
            clear_icon_sprite,
        }
    }

    pub fn clear_selection(&mut self) {
        if let Some(pressed_index) = self.pressed_main_button {
            self.main_buttons[pressed_index].unpress();
        }

        self.current_selection = TilePaletteSelection::None;
        self.pressed_main_button = None;
    }

    pub fn draw(&mut self, ui_sys: &UiSystem) {
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

        let _style_overrides =
            UiStyleOverrides::in_game_hud_menus(ui_sys);

        let _item_spacing =
            UiStyleOverrides::set_item_spacing(ui_sys, BUTTON_SPACING.x, BUTTON_SPACING.y);

        ui.window("Tile Palette Widget")
            .position(window_position, imgui::Condition::Always)
            .flags(widgets::invisible_window_flags())
            .build(|| {
                let previously_pressed_button = self.pressed_main_button;

                for (index, button) in self.main_buttons.iter_mut().enumerate() {
                    let was_pressed_this_frame = button.draw_main_button(ui_sys);

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
                                let pressed_child_index = pressed_button.draw_child_buttons(ui_sys);
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

                if widgets::is_debug_draw_enabled() {
                    widgets::draw_current_window_debug_rect(ui);
                }
            });
    }

    pub fn draw_selected_tile(&mut self,
                              render_sys: &mut dyn RenderSystem,
                              cursor_screen_pos: Vec2,
                              transform: WorldToScreenTransform,
                              has_valid_placement: bool) {
        if self.current_selection.is_none() {
            return;
        }

        // Draw clear icon under the cursor:
        if self.current_selection.is_clear() {
            const CLEAR_ICON_SIZE: Size = BASE_TILE_SIZE;

            let rect = Rect::from_pos_and_size(
                Vec2::new(
                    cursor_screen_pos.x - (CLEAR_ICON_SIZE.width  / 2) as f32,
                    cursor_screen_pos.y - (CLEAR_ICON_SIZE.height / 2) as f32),
                CLEAR_ICON_SIZE
            );

            render_sys.draw_textured_colored_rect(rect,
                                                  &RectTexCoords::DEFAULT,
                                                  self.clear_icon_sprite,
                                                  Color::white());
        } else {
            let selected_tile = self.current_selection.as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size);

            let offset =
                if selected_tile.is(TileKind::Building | TileKind::Rocks | TileKind::Vegetation) {
                    Vec2::new(-(selected_tile.draw_size.width  as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32))
                } else {
                    Vec2::new(-(selected_tile.draw_size.width  as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32 / 2.0))
                };

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7 // Semi-transparent
                );

                render_sys.draw_textured_colored_rect(cursor_transform.scale_and_offset_rect(rect),
                                                      &sprite_frame.tex_info.coords,
                                                      sprite_frame.tex_info.texture,
                                                      tile_color * highlight_color);
            }
        }
    }
}
