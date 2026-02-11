use strum::EnumCount;
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    implement_dialog_menu,
    game::{GameLoop, menu::ButtonDef},
};

// ----------------------------------------------
// HomeButtonKind
// ----------------------------------------------

const HOME_BUTTON_COUNT: usize = HomeButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum HomeButtonKind {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Continue", Enabled = false))]
    Continue,

    #[strum(props(Label = "Load Game"))]
    LoadGame,

    #[strum(props(Label = "Custom Game", Enabled = false))]
    CustomGame,

    #[strum(props(Label = "Settings"))]
    Settings,

    #[strum(props(Label = "About"))]
    About,

    #[strum(props(Label = "Quit Game"))]
    Quit,
}

impl ButtonDef for HomeButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::NewGame    => super::open(DialogMenuKind::NewGame, CLOSE_ALL_OTHERS, context),
            Self::Continue   => false, // TODO: Continue last save game.
            Self::LoadGame   => super::open(DialogMenuKind::LoadGame, CLOSE_ALL_OTHERS, context),
            Self::CustomGame => false, // TODO: Play custom game/map.
            Self::Settings   => super::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context),
            Self::About      => super::open(DialogMenuKind::About, CLOSE_ALL_OTHERS, context),
            Self::Quit       => Self::on_quit(),
        }
    }
}

impl HomeButtonKind {
    fn on_quit() -> bool {
        GameLoop::get_mut().request_quit();
        true
    }
}

// ----------------------------------------------
// Home
// ----------------------------------------------

pub struct Home {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { Home, ["Heritage Builder", "The Dragon Legacy"] }

impl Home {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let buttons = make_dialog_button_widgets::<HomeButtonKind, HOME_BUTTON_COUNT>(context);

        Self {
            menu: make_default_layout_dialog_menu(
                context,
                Self::KIND,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Some(buttons)
            )
        }
    }
}
