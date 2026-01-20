use arrayvec::ArrayVec;
use smallvec::SmallVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    TilePaletteSelection,
    button::{SpriteButton, ButtonState, ButtonDef},
    widgets::{self, UiStyleTextLabelInvisibleButtons},
};
use crate::{
    ui::{self, UiTextureHandle, UiWidgetContext},
    render::{RenderSystem, TextureHandle},
    utils::{
        self,
        constants::*,
        Size, Vec2, Color, Rect, RectTexCoords,
        coords::WorldToScreenTransform
    },
    tile::{
        TileKind, rendering::INVALID_TILE_COLOR,
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
                    if tile_def.sector.name() == button_name {
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

    fn new_button(self, context: &mut UiWidgetContext) -> TilePaletteMainButton {
        let children = self.build_child_button_list();
        TilePaletteMainButton {
            btn: SpriteButton::new(
                context,
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip()),
                    show_tooltip_when_pressed: false,
                    state_transition_secs: 0.0,
                    hidden: false,
                },
                self.initial_state(&children),
            ),
            kind: self,
            children,
        }
    }

    fn create_all(context: &mut UiWidgetContext) -> ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT> {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(context));
        }
        buttons
    }
}

// ----------------------------------------------
// TilePaletteMainButton
// ----------------------------------------------

struct TilePaletteMainButton {
    btn: SpriteButton,
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

    fn draw_main_button(&mut self, context: &mut UiWidgetContext, background_sprite: UiTextureHandle) -> bool {
        self.btn.draw(context, Some(background_sprite))
    }

    fn draw_child_buttons(&mut self,
                          context: &mut UiWidgetContext,
                          background_sprite: UiTextureHandle,
                          button_hover_sprite: UiTextureHandle) -> Option<usize> {
        if self.children.is_empty() {
            return None;
        }

        let ui = context.ui_sys.ui();

        const BUTTON_HEIGHT: f32 = 20.0;
        const LABEL_PADDING: f32 = 25.0;
        const VERTICAL_SPACING: f32 = 4.0;

        let mut longest_label: f32 = 0.0;
        for child in &self.children {
            let label_size = ui.calc_text_size(&child.label);
            longest_label = longest_label.max(label_size[0]);
        }

        let child_window_width = longest_label + (LABEL_PADDING * 2.0);
        let main_button_pos = self.btn.rect.position();

        let window_position = [
            main_button_pos.x - child_window_width,
            main_button_pos.y,
        ];

        let window_size = [
            child_window_width,
            self.children.len() as f32 * (BUTTON_HEIGHT + VERTICAL_SPACING)
        ];

        let _item_spacing = widgets::push_item_spacing(ui, 0.0, VERTICAL_SPACING);

        ui.window(format!("Child Window {:?}", self.kind))
            .position(window_position, imgui::Condition::Always)
            .size(window_size, imgui::Condition::Always)
            .flags(widgets::window_flags() | imgui::WindowFlags::NO_BACKGROUND)
            .build(|| {                
                // Draw background:
                {
                    let window_rect = Rect::from_pos_and_size(
                        Vec2::from_array(ui.window_pos()),
                        Vec2::from_array(ui.window_size())
                    );
                    ui.get_window_draw_list()
                        .add_image(background_sprite, window_rect.min.to_array(), window_rect.max.to_array())
                        .build();
                }

                ui.set_window_font_scale(0.8);

                let mut labels = SmallVec::<[&str; 16]>::new();
                for button in &self.children {
                    labels.push(&button.label);
                }

                // Make button background transparent and borderless.
                let _button_style_overrides =
                    UiStyleTextLabelInvisibleButtons::apply_overrides(context.ui_sys);

                let pressed_button_index = widgets::draw_centered_button_group_ex(
                    ui,
                    &labels,
                    None,
                    Some(Vec2::new(8.0, 5.0)),
                    Some(|ui: &imgui::Ui, button_index: usize| {
                        // Draw underline effect when hovered / active:
                        let button_rect = Rect::from_extents(
                            Vec2::from_array(ui.item_rect_min()),
                            Vec2::from_array(ui.item_rect_max())
                        ).translated(Vec2::new(0.0, ui.text_line_height() - 5.0));

                        let underline_tint_color = if ui.is_item_active() {
                            imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
                        } else {
                            imgui::ImColor32::WHITE
                        };

                        ui.get_window_draw_list()
                            .add_image(button_hover_sprite,
                                       button_rect.min.to_array(),
                                       button_rect.max.to_array())
                                       .col(underline_tint_color)
                                       .build();

                        if !self.children[button_index].tooltip.is_empty() {
                            ui::custom_tooltip(ui, Some(0.8), Some(background_sprite), || ui.text(&self.children[button_index].tooltip));
                        }
                    }),
                    widgets::ALWAYS_ENABLED
                );

                widgets::draw_current_window_debug_rect(ui);
                ui.set_window_font_scale(1.0);

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
    background_sprite: UiTextureHandle,
    button_hover_sprite: UiTextureHandle,
}

impl TilePaletteWidget {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let clear_icon_path = ui::assets_path().join("icons/red_x_icon.png");
        let clear_icon_sprite = context.tex_cache.load_texture_with_settings(
            clear_icon_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );

        let background_sprite_path = ui::assets_path().join("misc/tall_page_bg.png");
        let background_sprite = context.tex_cache.load_texture_with_settings(
            background_sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );

        let button_hover_sprite_path = ui::assets_path().join("misc/brush_stroke_divider.png");
        let button_hover_sprite = context.tex_cache.load_texture_with_settings(
            button_hover_sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );

        Self {
            current_selection: TilePaletteSelection::None,
            main_buttons: TilePaletteMainButtonKind::create_all(context),
            pressed_main_button: None,
            clear_icon_sprite,
            background_sprite: context.ui_sys.to_ui_texture(context.tex_cache, background_sprite),
            button_hover_sprite: context.ui_sys.to_ui_texture(context.tex_cache, button_hover_sprite),
        }
    }

    pub fn clear_selection(&mut self) {
        if let Some(pressed_index) = self.pressed_main_button {
            self.main_buttons[pressed_index].unpress();
        }

        self.current_selection = TilePaletteSelection::None;
        self.pressed_main_button = None;
    }

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        let ui_sys = context.ui_sys;
        let ui = ui_sys.ui();

        const BUTTON_SIZE: Vec2 = TilePaletteMainButtonKind::BUTTON_SIZE.to_vec2();
        const BUTTON_SPACING: Vec2 = Vec2::new(4.0, 4.0);

        const WINDOW_TOP_MARGIN: f32 = 0.0;
        const WINDOW_RIGHT_MARGIN: f32 = 10.0;
        const WINDOW_WIDTH: f32 = BUTTON_SIZE.x + BUTTON_SPACING.x;

        // X position = screen width - estimated window width - margin
        let window_position = [
            ui.io().display_size[0] - WINDOW_WIDTH - WINDOW_RIGHT_MARGIN,
            WINDOW_TOP_MARGIN
        ];

        let _item_spacing = widgets::push_item_spacing(ui, BUTTON_SPACING.x, BUTTON_SPACING.y);

        ui.window("Tile Palette Widget")
            .position(window_position, imgui::Condition::Always)
            .flags(widgets::window_flags() | imgui::WindowFlags::NO_BACKGROUND)
            .build(|| {
                // Draw background:
                {
                    let window_rect = Rect::from_pos_and_size(
                        Vec2::from_array(ui.window_pos()),
                        Vec2::from_array(ui.window_size())
                    );
                    ui.get_window_draw_list()
                        .add_image(self.background_sprite, window_rect.min.to_array(), window_rect.max.to_array())
                        .build();
                }

                ui.set_window_font_scale(0.8);

                let previously_pressed_button = self.pressed_main_button;

                for (index, button) in self.main_buttons.iter_mut().enumerate() {
                    let was_pressed_this_frame = button.draw_main_button(context, self.background_sprite);

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
                                let pressed_child_index = pressed_button.draw_child_buttons(context, self.background_sprite, self.button_hover_sprite);
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

                widgets::draw_current_window_debug_rect(ui);
                ui.set_window_font_scale(1.0);
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
            const CLEAR_ICON_SIZE: Vec2 = BASE_TILE_SIZE_F32;

            let rect = Rect::from_pos_and_size(
                Vec2::new(
                    cursor_screen_pos.x - (CLEAR_ICON_SIZE.x * 0.5),
                    cursor_screen_pos.y - (CLEAR_ICON_SIZE.y * 0.5)),
                CLEAR_ICON_SIZE
            );

            render_sys.draw_textured_colored_rect(rect,
                                                  &RectTexCoords::DEFAULT,
                                                  self.clear_icon_sprite,
                                                  Color::white());
        } else {
            let selected_tile = self.current_selection.as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size.to_vec2());

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
