use super::*;
use crate::{declare_dialog_menu};

// ----------------------------------------------
// LoadGame
// ----------------------------------------------

pub struct LoadGame {
    menu: UiMenuRcMut,
}

declare_dialog_menu! { LoadGame, "Load Game" }

impl LoadGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::LoadGame,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                Some(widgets)
            )
        }
    }
}

// ----------------------------------------------
// SaveGame
// ----------------------------------------------

pub struct SaveGame {
    menu: UiMenuRcMut,
}

declare_dialog_menu! { SaveGame, "Save Game" }

impl SaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::SaveGame,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                Some(widgets)
            )
        }
    }
}

// ----------------------------------------------
// LoadOrSaveGame
// ----------------------------------------------

pub struct LoadOrSaveGame {
    menu: UiMenuRcMut,
}

declare_dialog_menu! { LoadOrSaveGame, "Load / Save" }

impl LoadOrSaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::LoadGame,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                Some(widgets)
            )
        }
    }
}
