use super::*;
use crate::{
    implement_dialog_menu,
    utils::Size,
    tile::sets::PresetTiles,
    game::{GameLoop, menu::TEXT_BUTTON_HOVERED_SPRITE},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const MAP_SIZE_STEP: i32 = 32;
const MIN_MAP_SIZE: i32 = 64;
const MAX_MAP_SIZE: i32 = 256;

const TERRAIN_TILE_PRESETS: [PresetTiles; 3] = [
    PresetTiles::Grass,
    PresetTiles::Dirt,
    PresetTiles::Water,
];

// ----------------------------------------------
// NewGame
// ----------------------------------------------

pub struct NewGame {
    new_map_size: Size,
    terrain_tile_preset_index: usize,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { NewGame, "New Game" }

impl NewGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        // -------------
        // Widgets:
        // -------------

        let new_map_width_input = UiIntInput::new(
            context,
            UiIntInputParams {
                font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                min: Some(MIN_MAP_SIZE),
                max: Some(MAX_MAP_SIZE),
                step: Some(MAP_SIZE_STEP),
                on_read_value: UiIntInputReadValue::with_fn(
                    |_input, _context| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();
                        new_game_menu.new_map_size.width
                    }
                ),
                on_update_value: UiIntInputUpdateValue::with_fn(
                    |_input, _context, new_width| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();
                        new_game_menu.new_map_size.width = new_width;
                    }
                ),
                ..Default::default()
            }
        );

        let new_map_height_input = UiIntInput::new(
            context,
            UiIntInputParams {
                font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                min: Some(MIN_MAP_SIZE),
                max: Some(MAX_MAP_SIZE),
                step: Some(MAP_SIZE_STEP),
                on_read_value: UiIntInputReadValue::with_fn(
                    |_input, _context| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();
                        new_game_menu.new_map_size.height
                    }
                ),
                on_update_value: UiIntInputUpdateValue::with_fn(
                    |_input, _context, new_height| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();
                        new_game_menu.new_map_size.height = new_height;
                    }
                ),
                ..Default::default()
            }
        );

        let terrain_kind_dropdown = UiDropdown::with_values(
            context,
            UiDropdownParams {
                font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                current_item: 0,
                items: TERRAIN_TILE_PRESETS.into(),
                on_selection_changed: UiDropdownSelectionChanged::with_fn(
                    |dropdown, _context| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();
                        new_game_menu.terrain_tile_preset_index = dropdown.current_selection_index();
                    }
                ),
                ..Default::default()
            }
        );

        let mut labeled_widget_group = UiLabeledWidgetGroup::new(
            context,
            UiLabeledWidgetGroupParams {
                label_spacing: DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING,
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                center_vertically: false,
                center_horizontally: true,
                margin_left: 50.0,
                margin_right: 40.0,
            }
        );

        labeled_widget_group.add_widget("New Map Width".into(), new_map_width_input);
        labeled_widget_group.add_widget("New Map Height".into(), new_map_height_input);
        labeled_widget_group.add_widget("Terrain Kind".into(), terrain_kind_dropdown);

        // -------------
        // Buttons:
        // -------------

        let start_new_game_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Start New Game".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(
                    |_button, _context| {
                        let new_game_menu = DialogMenusSingleton::get_mut()
                            .find_dialog_as::<NewGame>();

                        let new_map_size = Some(new_game_menu.new_map_size);
                        let terrain_tile_preset = TERRAIN_TILE_PRESETS[new_game_menu.terrain_tile_preset_index];
                        let reset_map_with_tile_def = terrain_tile_preset.find_tile_def();

                        GameLoop::get_mut().reset_session(reset_map_with_tile_def, new_map_size);
                    }
                ),
                ..Default::default()
            }
        );

        let cancel_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Cancel".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(
                    |_button, context| {
                        DialogMenusSingleton::get_mut().close_current(context);
                    }
                ),
                ..Default::default()
            }
        );

        let mut side_by_side_button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0,
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        side_by_side_button_group.add_widget(start_new_game_button);
        side_by_side_button_group.add_widget(cancel_button);

        // -------------
        // Menu:
        // -------------

        let mut menu = make_default_dialog_menu_layout(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let spacing = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(DEFAULT_DIALOG_MENU_WIDGET_SPACING),
                ..Default::default()
            }
        );

        menu.add_widget(labeled_widget_group);
        menu.add_widget(spacing);
        menu.add_widget(side_by_side_button_group);

        Self {
            new_map_size: Size::new(MIN_MAP_SIZE, MIN_MAP_SIZE),
            terrain_tile_preset_index: 0,
            menu,
        }
    }
}
