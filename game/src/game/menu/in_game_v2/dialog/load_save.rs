use std::path::Path;
use bitflags::bitflags;

use super::*;
use crate::{
    implement_dialog_menu,
    game::{
        GameLoop,
        AUTOSAVE_FILE_NAME,
        DEFAULT_SAVE_FILE_NAME,
        menu::TEXT_BUTTON_HOVERED_SPRITE,
    },
};

// ----------------------------------------------
// LoadGame
// ----------------------------------------------

pub struct LoadGame {
    helper: SaveGameHelper,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { LoadGame, "Load Game" }

impl LoadGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
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

implement_dialog_menu! { SaveGame, "Save Game" }

impl SaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
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

implement_dialog_menu! { LoadOrSaveGame, "Load / Save" }

impl LoadOrSaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
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

    fn default_save_file_name(actions: SaveGameActions) -> String {
        let default_file_name = {
            if actions.intersects(SaveGameActions::Load) {
                AUTOSAVE_FILE_NAME
            } else {
                DEFAULT_SAVE_FILE_NAME
            }
        };

        // Remove extension.
        Path::new(default_file_name)
            .with_extension("")
            .to_str().unwrap().into()
    }

    fn save_files_list() -> Vec<String> {
        // List of available save files, without extension.
        GameLoop::get()
            .save_files_list()
            .iter()
            .map(|path| path.with_extension("").to_str().unwrap().into())
            .collect()
    }

    fn current_save_file_selection(menu: &UiMenu) -> (&str, &[String]) {
        let (_, save_files_list) = menu
            .find_widget_of_type::<UiItemList>()
            .unwrap();

        let save_file_name = save_files_list.current_selection().unwrap_or_else(|| {
            save_files_list.current_text_input_field().unwrap_or_default()
        });

        (save_file_name, save_files_list.items())
    }

    fn open_overwrite_save_game_message_box(menu: &mut UiMenuRcMut, context: &mut UiWidgetContext) {
        let menu_rc = menu.clone();

        menu.open_message_box(context, |context: &mut UiWidgetContext| {
            let yes_button_menu_weak_ref = menu_rc.downgrade();
            let no_button_menu_weak_ref  = menu_rc.downgrade();

            UiMessageBoxParams {
                label: Some("Overwrite Save Game Popup".into()),
                background: Some(DEFAULT_DIALOG_POPUP_BACKGROUND_SPRITE),
                contents: vec![
                    UiWidgetImpl::from(UiMenuHeading::new(
                        context,
                        UiMenuHeadingParams {
                            lines: vec!["Overwrite existing save game?".into()],
                            font_scale: DEFAULT_DIALOG_POPUP_FONT_SCALE,
                            separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            margin_top: 2.0,
                            ..Default::default()
                        }
                    ))
                ],
                buttons: vec![
                    UiWidgetImpl::from(UiTextButton::new(
                        context,
                        UiTextButtonParams {
                            label: "Yes".into(),
                            size: UiTextButtonSize::Normal,
                            hover: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            enabled: true,
                            on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                                let mut save_game_menu = yes_button_menu_weak_ref.upgrade().unwrap();
                                let (save_file_name, _) = Self::current_save_file_selection(&save_game_menu);
                                GameLoop::get_mut().save_game(save_file_name);
                                save_game_menu.close_message_box(context);
                            }),
                            ..Default::default()
                        }
                    )),
                    UiWidgetImpl::from(UiTextButton::new(
                        context,
                        UiTextButtonParams {
                            label: "No".into(),
                            size: UiTextButtonSize::Normal,
                            hover: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            enabled: true,
                            on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                                let mut save_game_menu = no_button_menu_weak_ref.upgrade().unwrap();
                                save_game_menu.close_message_box(context);
                            }),
                            ..Default::default()
                        }
                    )),
                ],
                ..Default::default()
            }
        });
    }

    fn build_menu(&self,
                  context: &mut UiWidgetContext,
                  dialog_menu_kind: DialogMenuKind,
                  heading_title: &str)
                  -> UiMenuRcMut
    {
        // -------------
        // Menu:
        // -------------

        let mut menu = make_default_dialog_menu_layout(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        // -------------
        // Widgets:
        // -------------

        let save_files_list = UiItemList::new(
            context,
            UiItemListParams {
                font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                size: Some(Vec2::new(0.0, 250.0)), // Use whole parent window width minus margin, fixed height.
                margin_left: 95.0,
                margin_right: 100.0,
                items: Self::save_files_list(),
                flags: UiItemListFlags::Border | 
                       UiItemListFlags::TextInputField |
                       UiItemListFlags::Scrollbars |
                       UiItemListFlags::Scrollable,
                ..Default::default()
            }
        );

        // -------------
        // Buttons:
        // -------------

        let mut side_by_side_button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0,
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
            }
        );

        if self.actions.intersects(SaveGameActions::Load) {
            let menu_weak_ref = menu.downgrade().into_not_mut();

            let load_game_button = UiTextButton::new(
                context,
                UiTextButtonParams {
                    label: "Load Game".into(),
                    hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                    enabled: true,
                    on_pressed: UiTextButtonPressed::with_closure(
                        move |_, _context| {
                            let menu_rc = menu_weak_ref.upgrade().unwrap();
                            let (save_file_name, _) = Self::current_save_file_selection(&menu_rc);
                            GameLoop::get_mut().load_save_game(save_file_name);
                        }
                    ),
                    ..Default::default()
                }
            );

            side_by_side_button_group.add_widget(load_game_button);
        }

        if self.actions.intersects(SaveGameActions::Save) {
            let menu_weak_ref = menu.downgrade();

            let save_game_button = UiTextButton::new(
                context,
                UiTextButtonParams {
                    label: "Save Game".into(),
                    hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                    enabled: true,
                    on_pressed: UiTextButtonPressed::with_closure(
                        move |_, context| {
                            let mut menu_rc = menu_weak_ref.upgrade().unwrap();

                            let (save_file_name, existing_save_files) =
                                Self::current_save_file_selection(&menu_rc);

                            let save_file_already_exits = existing_save_files
                                .iter()
                                .any(|file| file.eq_ignore_ascii_case(save_file_name));

                            if save_file_already_exits {
                                // Prompt the user about overwriting an existing save file.
                                Self::open_overwrite_save_game_message_box(&mut menu_rc, context);
                            } else {
                                // Creating a new save. No prompt required.
                                GameLoop::get_mut().save_game(save_file_name);
                            }
                        }
                    ),
                    ..Default::default()
                }
            );

            side_by_side_button_group.add_widget(save_game_button);
        }

        let cancel_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Cancel".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(
                    |_, context| {
                        DialogMenusSingleton::get_mut().close_current(context);
                    }
                ),
                ..Default::default()
            }
        );

        side_by_side_button_group.add_widget(cancel_button);

        // -------------
        // Link all:
        // -------------

        // When menu opens, refresh list of available save game files:
        let save_game_actions = self.actions;
        menu.set_open_close_callback(UiMenuOpenClose::with_closure(
            move |save_game_menu, _context, is_open| {
                if is_open {
                    let (_, save_files_list) = save_game_menu
                        .find_widget_of_type_mut::<UiItemList>()
                        .unwrap();

                    let available_save_files = Self::save_files_list();
                    let current_selection_index = save_files_list.current_selection_index();

                    if let Some(index) = current_selection_index {
                        let current_item = index.min(available_save_files.len() - 1);
                        save_files_list.reset_items(Some(current_item), available_save_files);
                    } else {
                        let default_save_file_name = Self::default_save_file_name(save_game_actions);
                        save_files_list.reset_items(None, available_save_files);
                        save_files_list.reset_text_input_field(default_save_file_name);
                    }
                }
            }
        ));

        let spacing = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(DEFAULT_DIALOG_MENU_WIDGET_SPACING),
                ..Default::default()
            }
        );

        menu.add_widget(save_files_list);
        menu.add_widget(spacing);
        menu.add_widget(side_by_side_button_group);

        menu
    }
}
