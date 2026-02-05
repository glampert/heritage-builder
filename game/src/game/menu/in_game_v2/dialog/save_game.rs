use super::*;

// ----------------------------------------------
// SaveGame
// ----------------------------------------------

const SAVE_GAME_MENU_HEADING_TITLE: &str = "Load / Save";
const SAVE_GAME_MENU_WIDGET_SPACING: f32 = 5.0;

pub struct SaveGame {
    menu: UiMenuRcMut,
}

impl DialogMenu for SaveGame {
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
                SAVE_GAME_MENU_WIDGET_SPACING,
                widgets
            )
        }
    }
}
