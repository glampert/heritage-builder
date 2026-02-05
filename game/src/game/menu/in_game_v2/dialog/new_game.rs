use super::*;

// ----------------------------------------------
// NewGame
// ----------------------------------------------

const NEW_GAME_MENU_HEADING_TITLE: &str = "New Game";
const NEW_GAME_MENU_WIDGET_SPACING: f32 = 5.0;

pub struct NewGame {
    menu: UiMenuRcMut,
}

impl DialogMenu for NewGame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::NewGame
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl NewGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let widgets = ArrayVec::<UiWidgetImpl, 1>::new();

        // TODO: Add widgets

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::NewGame,
                NEW_GAME_MENU_HEADING_TITLE,
                NEW_GAME_MENU_WIDGET_SPACING,
                widgets
            )
        }
    }
}
