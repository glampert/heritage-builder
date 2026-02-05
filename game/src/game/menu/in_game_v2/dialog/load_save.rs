use super::*;

const LOAD_SAVE_MENU_WIDGET_SPACING: f32 = 5.0;

// ----------------------------------------------
// LoadGame
// ----------------------------------------------

const LOAD_GAME_MENU_HEADING_TITLE: &str = "Load Game";

pub struct LoadGame {
    menu: UiMenuRcMut,
}

impl DialogMenu for LoadGame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::LoadGame
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl LoadGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::LoadGame,
                LOAD_GAME_MENU_HEADING_TITLE,
                LOAD_SAVE_MENU_WIDGET_SPACING,
                widgets
            )
        }
    }
}

// ----------------------------------------------
// SaveGame
// ----------------------------------------------

const SAVE_GAME_MENU_HEADING_TITLE: &str = "Save Game";

pub struct SaveGame {
    menu: UiMenuRcMut,
}

impl DialogMenu for SaveGame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::SaveGame
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl SaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::SaveGame,
                SAVE_GAME_MENU_HEADING_TITLE,
                LOAD_SAVE_MENU_WIDGET_SPACING,
                widgets
            )
        }
    }
}

// ----------------------------------------------
// LoadOrSaveGame
// ----------------------------------------------

const LOAD_OR_SAVE_GAME_MENU_HEADING_TITLE: &str = "Load / Save";

pub struct LoadOrSaveGame {
    menu: UiMenuRcMut,
}

impl DialogMenu for LoadOrSaveGame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::LoadOrSaveGame
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl LoadOrSaveGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::LoadGame,
                LOAD_OR_SAVE_GAME_MENU_HEADING_TITLE,
                LOAD_SAVE_MENU_WIDGET_SPACING,
                widgets
            )
        }
    }
}
