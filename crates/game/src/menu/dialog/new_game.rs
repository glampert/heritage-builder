use common::Size;

use super::*;
use crate::{GameLoop, tile::sets::PresetTiles};

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

implement_dialog_menu! { NewGame, ["New Game"] }

impl NewGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        // -------------
        // Widgets:
        // -------------

        let new_map_width_input = UiIntInput::new()
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .with_range(MIN_MAP_SIZE..=MAX_MAP_SIZE)
            .with_step(MAP_SIZE_STEP)
            .on_read_value(|_input, _context| super::find::<NewGame>().new_map_size.width)
            .on_update_value(|_input, _context, new_width| super::find::<NewGame>().new_map_size.width = new_width);

        let new_map_height_input = UiIntInput::new()
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .with_range(MIN_MAP_SIZE..=MAX_MAP_SIZE)
            .with_step(MAP_SIZE_STEP)
            .on_read_value(|_input, _context| super::find::<NewGame>().new_map_size.height)
            .on_update_value(|_input, _context, new_height| super::find::<NewGame>().new_map_size.height = new_height);

        let terrain_kind_dropdown = UiDropdown::new(TERRAIN_TILE_PRESETS)
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .on_selection_changed(|dropdown, _context| {
                super::find::<NewGame>().terrain_tile_preset_index = dropdown.current_selection_index();
            });

        let labeled_widget_group = UiLabeledWidgetGroup::new()
            .with_label_spacing(DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING)
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING.y)
            .with_widget("New Map Width", new_map_width_input)
            .with_widget("New Map Height", new_map_height_input)
            .with_widget("Terrain Kind", terrain_kind_dropdown);

        // -------------
        // Buttons:
        // -------------

        let start_new_game_button = make_text_button(context, "Start New Game", |_button, _context| {
            let new_game_menu = super::find::<NewGame>();

            let new_map_size = Some(new_game_menu.new_map_size);
            let terrain_tile_preset = TERRAIN_TILE_PRESETS[new_game_menu.terrain_tile_preset_index];
            let reset_map_with_tile_def = terrain_tile_preset.find_tile_def();

            GameLoop::get_mut().reset_session(reset_map_with_tile_def, new_map_size);
        });

        let side_by_side_button_group = UiWidgetGroup::new()
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0)
            .with_stack_vertically(false)
            .with_widget(start_new_game_button)
            .with_widget(make_dialog_close_button(context, "Cancel"));

        // -------------
        // Menu:
        // -------------

        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_WIDGET_SPACING, []);

        menu.add_widget(labeled_widget_group);
        menu.add_widget(UiSeparator::new().with_thickness(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x));
        menu.add_widget(side_by_side_button_group);

        Self { new_map_size: Size::new(MIN_MAP_SIZE, MIN_MAP_SIZE), terrain_tile_preset_index: 0, menu }
    }
}
