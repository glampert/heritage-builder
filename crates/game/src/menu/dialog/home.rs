use engine::log;
use strum::{EnumCount, EnumIter, EnumProperty};

use super::*;
use crate::{GameLoop, campaign, menu::ButtonDef};

// ----------------------------------------------
// HomeButtonKind
// ----------------------------------------------

const HOME_BUTTON_COUNT: usize = HomeButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum HomeButtonKind {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Campaign"))]
    Campaign,

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
    fn on_pressed(self, context: &mut GameUiContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::NewGame    => super::open(DialogMenuKind::NewGame, CLOSE_ALL_OTHERS, context),
            Self::Campaign   => Self::on_campaign(),
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
        GameLoop::get_mut().quit_game();
        true
    }

    // Start the first campaign (placeholder: campaign 0) by loading its first
    // mission's map. The mission map load transitions out of the home menu.
    fn on_campaign() -> bool {
        const FIRST_CAMPAIGN_ID: usize = 0;
        match campaign::start_campaign(FIRST_CAMPAIGN_ID) {
            Some(map) => {
                super::load_mission_map(&map);
                true
            }
            None => {
                log::error!(log::channel!("campaign"), "No campaign configured to start!");
                false
            }
        }
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
    pub fn new(context: &mut GameUiContext) -> Self {
        let buttons = make_dialog_button_widgets::<HomeButtonKind, HOME_BUTTON_COUNT>(context);

        Self {
            menu: make_default_layout_dialog_menu(
                context,
                Self::KIND,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Some(buttons),
            ),
        }
    }
}
