use strum::EnumCount;
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    implement_dialog_menu,
    game::menu::ButtonDef,
};

// ----------------------------------------------
// MainSettingsButtonKind
// ----------------------------------------------

const MAIN_SETTINGS_BUTTON_COUNT: usize = MainSettingsButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum MainSettingsButtonKind {
    #[strum(props(Label = "Game"))]
    Game,

    #[strum(props(Label = "Sound"))]
    Sound,

    #[strum(props(Label = "Graphics"))]
    Graphics,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for MainSettingsButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::Game     => super::open(DialogMenuKind::GameSettings, CLOSE_ALL_OTHERS, context),
            Self::Sound    => super::open(DialogMenuKind::SoundSettings, CLOSE_ALL_OTHERS, context),
            Self::Graphics => super::open(DialogMenuKind::GraphicsSettings, CLOSE_ALL_OTHERS, context),
            Self::Back     => super::close_current(context),
        }
    }
}

// ----------------------------------------------
// MainSettings
// ----------------------------------------------

// Main settings menu / settings entry point.
pub struct MainSettings {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { MainSettings, ["Settings"] }

impl MainSettings {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let buttons = make_dialog_button_widgets::<MainSettingsButtonKind, MAIN_SETTINGS_BUTTON_COUNT>(context);

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
