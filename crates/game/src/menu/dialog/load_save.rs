use bitflags::bitflags;
use engine::{file_sys::paths::PathRef, save};

use super::*;
use crate::GameLoop;

// ----------------------------------------------
// LoadGame
// ----------------------------------------------

pub struct LoadGame {
    helper: SaveGameHelper,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { LoadGame, ["Load Game"] }

impl LoadGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        let helper = SaveGameHelper::new(SaveGameActions::Load);
        let menu = helper.build_menu(context, Self::KIND, Self::TITLE);
        Self { helper, menu }
    }
}

// ----------------------------------------------
// SaveGame
// ----------------------------------------------

pub struct SaveGame {
    helper: SaveGameHelper,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SaveGame, ["Save Game"] }

impl SaveGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        let helper = SaveGameHelper::new(SaveGameActions::Save);
        let menu = helper.build_menu(context, Self::KIND, Self::TITLE);
        Self { helper, menu }
    }
}

// ----------------------------------------------
// LoadOrSaveGame
// ----------------------------------------------

pub struct LoadOrSaveGame {
    helper: SaveGameHelper,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { LoadOrSaveGame, ["Load / Save"] }

impl LoadOrSaveGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        let helper = SaveGameHelper::new(SaveGameActions::Load | SaveGameActions::Save);
        let menu = helper.build_menu(context, Self::KIND, Self::TITLE);
        Self { helper, menu }
    }
}

// ----------------------------------------------
// SaveGameActions / SaveGameHelper
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    struct SaveGameActions: u8 {
        const Save = 1 << 0;
        const Load = 1 << 1;
    }
}

struct SaveGameHelper {
    actions: SaveGameActions,
}

impl SaveGameHelper {
    fn new(actions: SaveGameActions) -> Self {
        Self { actions }
    }

    fn default_save_file_name(actions: SaveGameActions) -> PathRef<'static> {
        if actions.intersects(SaveGameActions::Load) {
            save::storage::AUTOSAVE_FILE_NAME
        } else {
            save::storage::DEFAULT_SAVE_FILE_NAME
        }
    }

    fn list_save_files() -> Vec<String> {
        // List of available save files, without extension.
        let files = save::storage::list_save_files();
        files.iter().map(|file| file.to_str().unwrap().to_string()).collect()
    }

    fn current_save_file_selection(menu: &UiMenu) -> (PathRef<'_>, &[String]) {
        let (_, save_files_list) = menu.find_widget_of_type::<UiItemList>().unwrap();

        let save_file_name = save_files_list
            .current_selection()
            .unwrap_or_else(|| save_files_list.current_text_input_field().unwrap_or_default());

        (PathRef::from_str(save_file_name), save_files_list.items())
    }

    fn open_overwrite_save_game_message_box(menu: &mut UiMenuRcMut, context: &mut GameUiContext) {
        let yes_button_menu_weak_ref = menu.downgrade();
        let no_button_menu_weak_ref = menu.downgrade();

        menu.open_message_box(context, |context| {
            make_dialog_popup(context, "Overwrite Save Game Popup", ["Overwrite existing save game?"]).with_buttons([
                make_text_button(context, "Yes", move |_, context| {
                    let mut save_game_menu = yes_button_menu_weak_ref.upgrade().unwrap();
                    let (save_file_name, _) = Self::current_save_file_selection(&save_game_menu);
                    GameLoop::get_mut().save_game(save_file_name);
                    save_game_menu.close_message_box(context);
                }),
                make_text_button(context, "No", move |_, context| {
                    let mut save_game_menu = no_button_menu_weak_ref.upgrade().unwrap();
                    save_game_menu.close_message_box(context);
                }),
            ])
        });
    }

    fn build_menu(
        &self,
        context: &mut GameUiContext,
        dialog_menu_kind: DialogMenuKind,
        heading_title: &[&str],
    ) -> UiMenuRcMut {
        // -------------
        // Menu:
        // -------------

        let mut menu = make_default_layout_dialog_menu(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            [],
        );

        // -------------
        // Widgets:
        // -------------

        let save_files_list = UiItemList::new()
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .with_size(Vec2::new(0.0, 250.0)) // The menu's content width, fixed height.
            .with_flags(
                UiItemListFlags::Border
                    | UiItemListFlags::TextInputField
                    | UiItemListFlags::Scrollbars
                    | UiItemListFlags::Scrollable,
            );

        // -------------
        // Buttons:
        // -------------

        let mut side_by_side_button_group = UiWidgetGroup::new()
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0)
            .with_stack_vertically(false);

        if self.actions.intersects(SaveGameActions::Load) {
            let menu_weak_ref = menu.downgrade().into_not_mut();

            let load_game_button = make_text_button(context, "Load Game", move |_, _context| {
                let menu_rc = menu_weak_ref.upgrade().unwrap();
                let (save_file_name, _) = Self::current_save_file_selection(&menu_rc);
                GameLoop::get_mut().load_save_game(save_file_name);
            });

            side_by_side_button_group.add_widget(load_game_button);
        }

        if self.actions.intersects(SaveGameActions::Save) {
            let menu_weak_ref = menu.downgrade();

            let save_game_button = make_text_button(context, "Save Game", move |_, context| {
                let mut menu_rc = menu_weak_ref.upgrade().unwrap();

                let (save_file_name, existing_save_files) = Self::current_save_file_selection(&menu_rc);

                let save_file_already_exits =
                    existing_save_files.iter().any(|file| file.eq_ignore_ascii_case(save_file_name.as_str()));

                if save_file_already_exits {
                    // Prompt the user about overwriting an existing save file.
                    Self::open_overwrite_save_game_message_box(&mut menu_rc, context_as_mut::<GameUiContext>(context));
                } else {
                    // Creating a new save. No prompt required.
                    GameLoop::get_mut().save_game(save_file_name);
                }
            });

            side_by_side_button_group.add_widget(save_game_button);
        }

        side_by_side_button_group.add_widget(make_dialog_close_button(context, "Cancel"));

        // -------------
        // Link all:
        // -------------

        // When menu opens, refresh list of available save game files:
        let save_game_actions = self.actions;
        menu.set_open_close_callback(move |save_game_menu, _context, is_open| {
            if is_open {
                let (_, save_files_list) = save_game_menu.find_widget_of_type_mut::<UiItemList>().unwrap();

                let available_save_files = Self::list_save_files();
                let current_selection_index = save_files_list.current_selection_index();

                if let Some(index) = current_selection_index {
                    let current_item = index.min(available_save_files.len() - 1);
                    save_files_list.reset_items(Some(current_item), available_save_files);
                } else {
                    let default_save_file_name = Self::default_save_file_name(save_game_actions);
                    save_files_list.reset_items(None, available_save_files);
                    save_files_list.reset_text_input_field(default_save_file_name.to_string());
                }
            }
        });

        menu.add_widget(save_files_list);
        menu.add_widget(UiSeparator::new().with_thickness(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x));
        menu.add_widget(side_by_side_button_group);

        menu
    }
}
