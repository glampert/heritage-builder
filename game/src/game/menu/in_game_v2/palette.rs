use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    game::menu::*,
    render::TextureHandle,
    ui::{UiInputEvent, widgets::*},
    app::input::{InputAction, MouseButton},
    utils::{
        self,
        Vec2, Color, Rect, RectTexCoords,
        coords::WorldToScreenTransform,
        constants::BASE_TILE_SIZE_F32,
        mem::{RcMut, WeakMut, WeakRef},
    },
    tile::{
        TileKind,
        rendering::INVALID_TILE_COLOR,
        sets::{
            TileDef, TileDefHandle, TileSets, TileSector, PresetTiles,
            OBJECTS_BUILDINGS_CATEGORY, TERRAIN_LAND_CATEGORY,
        },
    },
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const TILE_PALETTE_BACKGROUND_SPRITE: &str = "misc/tall_page_bg.png";
const TILE_PALETTE_CHILD_MENU_BACKGROUND_SPRITE: &str = "misc/wide_page_bg.png";

const TILE_PALETTE_BUTTON_SPACING: Vec2 = Vec2::new(4.0, 4.0); // Vertical spacing between buttons, in pixels.
const TILE_PALETTE_MAIN_BUTTON_SIZE: Vec2 = Vec2::new(50.0, 50.0); // In pixels.

const TILE_PALETTE_MAIN_BUTTON_STATE_TRANSITION_SECS: Seconds = 0.0; // No timed transition.
const TILE_PALETTE_MAIN_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED: bool = false;

const TILE_PALETTE_MAIN_BUTTON_COUNT: usize = TilePaletteMainButtonDef::COUNT;

// ----------------------------------------------
// TilePaletteMainButtonDef
// ----------------------------------------------

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

    fn build_child_button_defs(self) -> Vec<TilePaletteChildButtonDef> {
        let mut children = Vec::new();
        let button_name = self.name();

        TileSets::get().for_each_category(|_, category| {
            if category.hash == OBJECTS_BUILDINGS_CATEGORY.hash ||
               category.hash == TERRAIN_LAND_CATEGORY.hash
            {
                category.for_each_tile_def(|tile_def| {
                    if tile_def.sector != TileSector::Housing && // NOTE: No child menu for housing; selects vacant lot instead.
                       tile_def.sector.name() == button_name
                    {
                        children.push(TilePaletteChildButtonDef::new(tile_def));
                    }
                    true
                });
            }
            true
        });

        children
    }

    fn to_tile_selection(self, tile_def_handle: Option<TileDefHandle>) -> TilePaletteSelection {
        match self {
            TilePaletteMainButtonDef::ClearLand => {
                TilePaletteSelection::Clear
            }
            TilePaletteMainButtonDef::Housing => {
                if let Some(tile_def) = PresetTiles::VacantLot.find_tile_def() {
                    TilePaletteSelection::Tile(TileDefHandle::from_tile_def(tile_def))
                } else {
                    TilePaletteSelection::None
                }
            }
            _ => TilePaletteSelection::Tile(tile_def_handle.unwrap())
        }
    }
}

impl ButtonDef for TilePaletteMainButtonDef {}

// ----------------------------------------------
// TilePaletteChildButtonDef
// ----------------------------------------------

struct TilePaletteChildButtonDef {
    label: String,
    tooltip: Option<String>,
    tile_def_handle: TileDefHandle,
}

impl TilePaletteChildButtonDef {
    fn new(tile_def: &TileDef) -> Self {
        let label = utils::snake_case_to_title::<128>(&tile_def.name).to_string();

        let tooltip = {
            if tile_def.cost != 0 {
                Some(format!("Cost: {} gold", tile_def.cost))
            } else {
                None
            }
        };

        Self {
            label,
            tooltip,
            tile_def_handle: TileDefHandle::from_tile_def(tile_def),
        }
    }
}

// ----------------------------------------------
// TilePaletteMainButtonsBuilder
// ----------------------------------------------

struct TilePaletteMainButtonsBuilder {
    main: ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    ui: ArrayVec<UiSpriteButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
}

impl TilePaletteMainButtonsBuilder {
    fn build_all(context: &mut UiWidgetContext, tile_palette: &TilePaletteMenuWeakMut) -> Self {
        let mut buttons = Self {
            main: ArrayVec::new(),
            ui: ArrayVec::new(),
        };

        for main_button_def in TilePaletteMainButtonDef::iter() {
            let mut main_button = TilePaletteMainButton::new(context, main_button_def, tile_palette);

            let tile_palette_weak_ref = tile_palette.clone();
            let child_menu_weak_ref = main_button.child_menu_weak_mut();

            let on_main_button_state_changed = UiSpriteButtonStateChanged::with_closure(
                move |button, context, prev_state| {
                    let mut tile_palette_rc = tile_palette_weak_ref.upgrade().unwrap();

                    let is_pressed = button.is_pressed();
                    let was_unpressed = prev_state == UiSpriteButtonState::Pressed
                        && (button.state() == UiSpriteButtonState::Idle || button.state() == UiSpriteButtonState::Disabled);

                    if is_pressed || was_unpressed {
                        // Reset all other main buttons / close open child menus.
                        tile_palette_rc.reset_selection_internal(context);
                    }

                    if is_pressed {
                        // Open new child menu for pressed main button, if any.
                        if let Some(child_menu_weak_ref) = &child_menu_weak_ref {
                            let mut child_menu_rc = child_menu_weak_ref.upgrade().unwrap();
                            child_menu_rc.open(context);
                        } else {
                            // If parent button has no child menu, choose tile directly here (e.g.: Housing, ClearLand).
                            let selection = main_button_def.to_tile_selection(None);
                            tile_palette_rc.set_selection_internal(selection);
                        }

                        // Stay pressed (reset_selection_internal above would have unpressed all).
                        button.press(true);
                    }
                }
            );

            let ui_button = main_button_def.new_sprite_button(
                context,
                TILE_PALETTE_MAIN_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED,
                TILE_PALETTE_MAIN_BUTTON_SIZE,
                TILE_PALETTE_MAIN_BUTTON_STATE_TRANSITION_SECS,
                main_button_def.initial_state(main_button.has_children()),
                on_main_button_state_changed
            );

            buttons.main.push(main_button);
            buttons.ui.push(ui_button);
        }

        buttons
    }
}

// ----------------------------------------------
// TilePaletteMainButton
// ----------------------------------------------

struct TilePaletteMainButton {
    def: TilePaletteMainButtonDef,
    child_menu: Option<UiMenuRcMut>,
}

impl TilePaletteMainButton {
    fn new(context: &mut UiWidgetContext,
           main_button_def: TilePaletteMainButtonDef,
           tile_palette: &TilePaletteMenuWeakMut) -> Self {
        let children = main_button_def.build_child_button_defs();

        let child_menu = {
            if children.is_empty() {
                None
            } else {
                Some(Self::build_child_menu(context, main_button_def, tile_palette, children))
            }
        };

        Self { def: main_button_def, child_menu }
    }

    fn build_child_menu(context: &mut UiWidgetContext,
                        main_button_def: TilePaletteMainButtonDef,
                        tile_palette: &TilePaletteMenuWeakMut,
                        children: Vec<TilePaletteChildButtonDef>) -> UiMenuRcMut {
        let mut child_menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some(format!("TilePaletteChildMenu: {}", main_button_def.name())),
                background: Some(TILE_PALETTE_CHILD_MENU_BACKGROUND_SPRITE),
                widget_spacing: Some(TILE_PALETTE_BUTTON_SPACING),
                ..Default::default()
            }
        );

        let mut child_button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: TILE_PALETTE_BUTTON_SPACING.y,
                ..Default::default()
            }
        );

        for child_def in children {
            let child_tooltip =
                child_def.tooltip.map(|tooltip_text| {
                    UiTooltipText::new(
                        context,
                        UiTooltipTextParams {
                            text: tooltip_text,
                            font_scale: TOOLTIP_FONT_SCALE,
                            background: Some(TOOLTIP_BACKGROUND_SPRITE),
                        }
                    )
                });

            let parent_button_def = main_button_def;
            let child_button_tile_def_handle = child_def.tile_def_handle;

            let tile_palette_weak_ref = tile_palette.clone();
            let child_menu_weak_ref = child_menu.downgrade();

            let on_child_button_pressed = UiTextButtonPressed::with_closure(
                move |_button, context| {
                    let mut tile_palette_rc = tile_palette_weak_ref.upgrade().unwrap();
                    let mut child_menu_rc = child_menu_weak_ref.upgrade().unwrap();

                    let selection = parent_button_def.to_tile_selection(Some(child_button_tile_def_handle));
                    tile_palette_rc.set_selection_internal(selection);

                    // Keep the parent button pressed but close the child menu when we have a selection.
                    child_menu_rc.close(context);
                }
            );

            let child_button = UiTextButton::new(
                context,
                UiTextButtonParams {
                    label: child_def.label,
                    tooltip: child_tooltip,
                    hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                    size: UiTextButtonSize::ExtraSmall,
                    enabled: true,
                    on_pressed: on_child_button_pressed,
                }
            );

            child_button_group.add_widget(child_button);
        }

        child_menu.add_widget(child_button_group);
        child_menu
    }

    fn draw_child_menu(&mut self, context: &mut UiWidgetContext) {
        if let Some(child_menu) = &mut self.child_menu && child_menu.is_open() {
            child_menu.draw(context);
        }
    }

    fn open_child_menu(&mut self, context: &mut UiWidgetContext) {
        if let Some(child_menu) = &mut self.child_menu {
            child_menu.open(context);
        }
    }

    fn close_child_menu(&mut self, context: &mut UiWidgetContext) {
        if let Some(child_menu) = &mut self.child_menu {
            child_menu.close(context);
        }
    }

    fn child_menu_weak_mut(&mut self) -> Option<UiMenuWeakMut> {
        self.child_menu
            .as_ref()
            .map(|menu| menu.downgrade())
    }

    fn has_children(&self) -> bool {
        self.child_menu.is_some()
    }
}

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

pub struct TilePaletteMenu {
    left_mouse_button_pressed: bool,
    current_selection: TilePaletteSelection,
    selection_renderer: TileSelectionRenderer,
    main_buttons: ArrayVec<TilePaletteMainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    menu: UiMenuRcMut,
}

pub type TilePaletteMenuRcMut   = RcMut<TilePaletteMenu>;
pub type TilePaletteMenuWeakMut = WeakMut<TilePaletteMenu>;
pub type TilePaletteMenuWeakRef = WeakRef<TilePaletteMenu>;

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

    fn clear_selection(&mut self, context: &mut GameMenusContext) {
        let mut ui_context = UiWidgetContext::new(
            context.sim,
            context.world,
            context.engine
        );
        self.reset_selection_internal(&mut ui_context);
    }
}

impl TilePaletteMenu {
    pub fn new(context: &mut UiWidgetContext) -> TilePaletteMenuRcMut {
        TilePaletteMenuRcMut::new_cyclic(|tile_palette_weak_ref| {
            let buttons =
                TilePaletteMainButtonsBuilder::build_all(context, &tile_palette_weak_ref);

            let mut palette_menu = UiMenu::new(
                context,
                UiMenuParams {
                    label: Some("TilePaletteMenu".into()),
                    flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignRight,
                    background: Some(TILE_PALETTE_BACKGROUND_SPRITE),
                    widget_spacing: Some(TILE_PALETTE_BUTTON_SPACING),
                    ..Default::default()
                }
            );

            for (index, ui_button) in buttons.ui.into_iter().enumerate() {
                palette_menu.add_widget(ui_button);

                if buttons.main[index].def.separator_follows() {
                    palette_menu.add_widget(UiSeparator::new(
                        context,
                        UiSeparatorParams {
                            separator: Some(SMALL_HORIZONTAL_SEPARATOR_SPRITE),
                            thickness: Some(2.0),
                            ..Default::default()
                        }
                    ));
                }
            }

            let mut tile_palette = Self {
                left_mouse_button_pressed: false,
                current_selection: TilePaletteSelection::None,
                selection_renderer: TileSelectionRenderer::new(context),
                main_buttons: buttons.main,
                menu: palette_menu,
            };

            tile_palette.set_child_menu_position_callbacks(context);
            tile_palette
        })
    }

    pub fn draw(&mut self,
                context: &mut UiWidgetContext,
                transform: WorldToScreenTransform,
                has_valid_placement: bool) {
        // Draw menu & main buttons:
        self.menu.draw(context);

        // Draw the open child menu, if any:
        for button in &mut self.main_buttons {
            button.draw_child_menu(context);
        }

        // Draw selected tile cursor overlay:
        self.selection_renderer.draw(context,
                                     transform,
                                     has_valid_placement,
                                     self.current_selection);
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn set_child_menu_position_callbacks(&mut self, context: &UiWidgetContext) {
        for main_button in &mut self.main_buttons {
            if let Some(child_menu) = &mut main_button.child_menu {
                let child_menu_width = child_menu.measure(context).x;
                let palette_menu_weak_ref = self.menu.downgrade();
                let main_button_index =
                    self.menu.find_widget_with_label::<UiSpriteButton>(&main_button.def.label())
                        .expect("Couldn't find UiSpriteButton widget in palette menu!").0;

                child_menu.set_position(UiMenuPosition::Callback(
                    UiMenuCalcPosition::with_closure(move |_, _| {
                        let palette_menu_rc = palette_menu_weak_ref.upgrade().unwrap();
                        let main_button_widget = &palette_menu_rc.widgets()[main_button_index];
                        let main_button = main_button_widget.as_any().downcast_ref::<UiSpriteButton>().unwrap();
                        let main_button_pos = main_button.position();
                        Vec2::new(main_button_pos.x - child_menu_width, main_button_pos.y)
                    })
                ));
            }
        }
    }

    fn reset_selection_internal(&mut self, context: &mut UiWidgetContext) {
        self.left_mouse_button_pressed = false;
        self.current_selection = TilePaletteSelection::None;

        for button in &mut self.main_buttons {
            button.close_child_menu(context);
        }

        for widget in self.menu.widgets_mut() {
            if let Some(button) = widget.as_any_mut().downcast_mut::<UiSpriteButton>() {
                button.press(false);
            }
        }
    }

    fn set_selection_internal(&mut self, selection: TilePaletteSelection) {
        self.current_selection = selection;
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
