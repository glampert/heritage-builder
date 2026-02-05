use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::game::menu::ButtonDef;

// ----------------------------------------------
// MainMenuButtonKind
// ----------------------------------------------

const MAIN_MENU_BUTTON_SPACING: f32 = 8.0;
const MAIN_MENU_BUTTON_COUNT: usize = MainMenuButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum MainMenuButtonKind {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Load Game"))]
    LoadGame,

    #[strum(props(Label = "Save Game"))]
    SaveGame,

    #[strum(props(Label = "Settings"))]
    Settings,

    #[strum(props(Label = "Quit"))]
    Quit,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for MainMenuButtonKind {}

// ----------------------------------------------
// MainMenu
// ----------------------------------------------

const MAIN_MENU_HEADING_TITLE: &str = "Game";

pub struct MainMenu {
    menu: UiMenuRcMut,
}

impl DialogMenu for MainMenu {
    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::MainMenu
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl MainMenu {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut buttons = ArrayVec::<UiWidgetImpl, MAIN_MENU_BUTTON_COUNT>::new();

        for button_kind in MainMenuButtonKind::iter() {
            let on_pressed = UiTextButtonPressed::with_fn(
                |_button, _context| {
                    // TODO: button action
                }
            );

            buttons.push(UiWidgetImpl::from(
                button_kind.new_text_button(
                    context,
                    UiTextButtonSize::Large,
                    true,
                    on_pressed
                )
            ));
        }

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::MainMenu,
                MAIN_MENU_HEADING_TITLE,
                MAIN_MENU_BUTTON_SPACING,
                buttons
            )
        }
    }
}
