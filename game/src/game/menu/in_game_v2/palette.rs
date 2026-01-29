use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    render::TextureHandle,
    app::input::{InputAction, MouseButton},
    utils::{
        Vec2, Color, Rect, RectTexCoords,
        coords::WorldToScreenTransform,
        constants::BASE_TILE_SIZE_F32,
    },
    tile::{
        TileKind,
        sets::TileDefHandle,
        rendering::INVALID_TILE_COLOR,
    },
    game::menu::{
        ButtonDef,
        TilePalette, TilePaletteSelection,
        TILE_PALETTE_BACKGROUND_SPRITE,
        SMALL_SEPARATOR_SPRITE,
    },
    ui::{
        UiInputEvent,
        widgets::{
            UiWidget, UiWidgetContext,
            UiSeparator, UiSeparatorParams,
            UiSpriteButton, UiSpriteButtonState,
            UiMenu, UiMenuParams, UiMenuFlags, UiMenuStrongRef,
        },
    }
};

// ----------------------------------------------
// TilePaletteMainButtonDef
// ----------------------------------------------

const TILE_PALETTE_MAIN_BUTTON_COUNT: usize = TilePaletteMainButtonDef::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TilePaletteMainButtonDef {
    #[strum(props(Label = "palette/clear_land"))]
    ClearLand,

    #[strum(props(Label = "palette/housing", Tooltip = "Vacant Lot\nCost: 1 gold"))]
    Housing,

    #[strum(props(Label = "palette/roads", SeparatorFollows = true))]
    Roads,

    #[strum(props(Label = "palette/food_and_farming", DisabledIfEmpty = true))]
    FoodAndFarming,

    #[strum(props(Label = "palette/industry_and_resources", DisabledIfEmpty = true))]
    IndustryAndResources,

    #[strum(props(Label = "palette/services", DisabledIfEmpty = true))]
    Services,

    #[strum(props(Label = "palette/infrastructure", DisabledIfEmpty = true))]
    Infrastructure,

    #[strum(props(Label = "palette/culture_and_religion", DisabledIfEmpty = true))]
    CultureAndReligion,

    #[strum(props(Label = "palette/trade_and_economy", DisabledIfEmpty = true))]
    TradeAndEconomy,

    #[strum(props(Label = "palette/beautification", DisabledIfEmpty = true))]
    Beautification,
}

impl TilePaletteMainButtonDef {
    const SIZE: Vec2 = Vec2::new(50.0, 50.0);  // In pixels.
    const SPACING: Vec2 = Vec2::new(4.0, 4.0); // Vertical spacing between buttons, in pixels.
    const STATE_TRANSITION_SECS: f32 = 0.0;    // No transition.
    const SHOW_TOOLTIP_WHEN_PRESSED: bool = false;

    fn separator_follows(self) -> bool {
        self.get_bool("SeparatorFollows").is_some_and(|val| val)
    }

    fn disabled_if_empty(self) -> bool {
        self.get_bool("DisabledIfEmpty").is_some_and(|val| val)
    }

    fn initial_state(self, has_children: bool) -> UiSpriteButtonState {
        if self.disabled_if_empty() && !has_children {
            UiSpriteButtonState::Disabled
        } else {
            UiSpriteButtonState::Idle
        }
    }

    fn create_all(context: &mut UiWidgetContext) -> (ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
                                                     ArrayVec<UiSpriteButton, TILE_PALETTE_MAIN_BUTTON_COUNT>)
    {
        let mut main_buttons = ArrayVec::new();
        let mut ui_buttons = ArrayVec::new();

        for def in Self::iter() {
            let main_button = TilePaletteMainButton {
                def,
                children: Vec::new(),
            };

            let ui_button = def.new_sprite_button(
                context,
                Self::SHOW_TOOLTIP_WHEN_PRESSED,
                Self::SIZE,
                def.initial_state(main_button.has_children()),
                Self::STATE_TRANSITION_SECS
            );

            main_buttons.push(main_button);
            ui_buttons.push(ui_button);
        }

        (main_buttons, ui_buttons)
    }
}

impl ButtonDef for TilePaletteMainButtonDef {}

// ----------------------------------------------
// TilePaletteMainButton
// ----------------------------------------------

struct TilePaletteMainButton {
    def: TilePaletteMainButtonDef,
    children: Vec<TilePaletteChildButton>,
}

impl TilePaletteMainButton {
    fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

// ----------------------------------------------
// TilePaletteChildButton
// ----------------------------------------------

struct TilePaletteChildButton {
    tile_def_handle: TileDefHandle,
}

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

pub struct TilePaletteMenu {
    left_mouse_button_pressed: bool,
    current_selection: TilePaletteSelection,
    selection_renderer: TileSelectionRenderer,
    main_buttons: ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    menu: UiMenuStrongRef,
}

impl TilePalette for TilePaletteMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_pressed = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_pressed = false;
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.current_selection
    }

    fn clear_selection(&mut self) {
        self.left_mouse_button_pressed = false;
        self.current_selection = TilePaletteSelection::None;
    }
}

impl TilePaletteMenu {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let (main_buttons, ui_buttons) =
            TilePaletteMainButtonDef::create_all(context);

        let menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("TilePaletteMenu".into()),
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignRight,
                background: Some(TILE_PALETTE_BACKGROUND_SPRITE),
                widget_spacing: Some(TilePaletteMainButtonDef::SPACING),
                ..Default::default()
            }
        );

        for (index, ui_button) in ui_buttons.into_iter().enumerate() {
            menu.as_mut().add_widget(ui_button);

            if main_buttons[index].def.separator_follows() {
                menu.as_mut().add_widget(UiSeparator::new(
                    context,
                    UiSeparatorParams {
                        separator: Some(SMALL_SEPARATOR_SPRITE),
                        thickness: Some(2.0),
                        ..Default::default()
                    }
                ));
            }
        }

        Self {
            left_mouse_button_pressed: false,
            current_selection: TilePaletteSelection::None,
            selection_renderer: TileSelectionRenderer::new(context),
            main_buttons,
            menu,
        }
    }

    pub fn draw(&mut self,
                context: &mut UiWidgetContext,
                transform: WorldToScreenTransform,
                has_valid_placement: bool)
    {
        self.menu.as_mut().draw(context);
        self.selection_renderer.draw(context,
                                     transform,
                                     has_valid_placement,
                                     self.current_selection);
    }
}

// ----------------------------------------------
// TileSelectionRenderer
// ----------------------------------------------

struct TileSelectionRenderer {
    clear_icon: TextureHandle,
}

impl TileSelectionRenderer {
    fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            clear_icon: context.load_texture("icons/red_x_icon.png"),
        }
    }

    fn draw(&self,
            context: &mut UiWidgetContext,
            transform: WorldToScreenTransform,
            has_valid_placement: bool,
            current_selection: TilePaletteSelection) {
        if current_selection.is_none() {
            return;
        }

        // Draw clear icon under the cursor:
        if current_selection.is_clear() {
            const CLEAR_ICON_SIZE: Vec2 = BASE_TILE_SIZE_F32;

            let rect = Rect::from_pos_and_size(
                context.cursor_screen_pos - (CLEAR_ICON_SIZE * 0.5),
                CLEAR_ICON_SIZE
            );

            context.render_sys.draw_textured_colored_rect(rect,
                                                          &RectTexCoords::DEFAULT,
                                                          self.clear_icon,
                                                          Color::white());
        } else {
            let selected_tile = current_selection.as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(context.cursor_screen_pos, selected_tile.draw_size.to_vec2());

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

                context.render_sys.draw_textured_colored_rect(cursor_transform.scale_and_offset_rect(rect),
                                                              &sprite_frame.tex_info.coords,
                                                              sprite_frame.tex_info.texture,
                                                              tile_color * highlight_color);
            }
        }
    }    
}
