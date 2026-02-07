use std::path::Path;
use bitflags::bitflags;

use super::*;
use crate::{
    declare_dialog_menu,
    game::{GameLoop, AUTOSAVE_FILE_NAME, DEFAULT_SAVE_FILE_NAME},
};

// ----------------------------------------------
// LoadGame
// ----------------------------------------------

pub struct LoadGame {
    helper: SaveGameHelper,
    menu: UiMenuRcMut,
}

declare_dialog_menu! { LoadGame, "Load Game" }

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

declare_dialog_menu! { SaveGame, "Save Game" }

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

declare_dialog_menu! { LoadOrSaveGame, "Load / Save" }

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

    fn default_save_file_name(&self) -> String {
        let default_file_name = {
            if self.actions.intersects(SaveGameActions::Load) {
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

    fn build_menu(&self,
                  context: &mut UiWidgetContext,
                  dialog_menu_kind: DialogMenuKind,
                  heading_title: &str)
                  -> UiMenuRcMut
    {
        let save_files = GameLoop::get()
            .save_files_list()
            .iter()
            .map(|path| path.with_extension("").to_str().unwrap().into())
            .collect::<Vec<String>>();

        let save_files_list = UiItemList::new(
            context,
            UiItemListParams {
                font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                size: Some(Vec2::new(0.0, 250.0)), // Use whole parent window width minus margin, fixed height.
                margin_left: 95.0,
                margin_right: 100.0,
                flags: UiItemListFlags::Border | 
                       UiItemListFlags::TextInputField |
                       UiItemListFlags::Scrollbars |
                       UiItemListFlags::Scrollable,
                current_item: None,
                items: save_files, // TODO: need to update whenever file list changes!
                on_selection_changed: UiItemListSelectionChanged::with_fn(
                    |_save_files_list, _context| {
                        //let selection_string = item_list.current_selection().unwrap_or_else(|| {
                            //item_list.current_text_input_field().unwrap_or_default()
                        //});

                        //log::info!("Updated ItemList: '{}' [{:?}]",
                            //selection_string,
                            //item_list.current_selection_index());
                    }
                ),
                ..Default::default()
            }
        );

        let mut menu = make_default_dialog_menu_layout(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        menu.add_widget(save_files_list);
        menu
    }
}
