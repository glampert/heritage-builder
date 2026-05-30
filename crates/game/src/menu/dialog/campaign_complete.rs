use strum::{EnumCount, EnumIter, EnumProperty};

use super::*;
use crate::{GameLoop, menu::ButtonDef};

// ----------------------------------------------
// CampaignCompleteButtonKind
// ----------------------------------------------

const CAMPAIGN_COMPLETE_BUTTON_COUNT: usize = CampaignCompleteButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum CampaignCompleteButtonKind {
    #[strum(props(Label = "Back to Main Menu"))]
    BackToMainMenu,
}

impl ButtonDef for CampaignCompleteButtonKind {
    fn on_pressed(self, _context: &mut GameUiContext) -> bool {
        match self {
            // quit_to_main_menu also resets the campaign (see cmd_quit_to_main_menu).
            Self::BackToMainMenu => {
                GameLoop::get_mut().quit_to_main_menu();
                true
            }
        }
    }
}

// ----------------------------------------------
// CampaignComplete
// ----------------------------------------------

// Placeholder prompt shown after the final mission of a campaign is completed.
pub struct CampaignComplete {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { CampaignComplete, ["Campaign Complete!", "Congratulations!"] }

impl CampaignComplete {
    pub fn new(context: &mut GameUiContext) -> Self {
        let buttons = make_dialog_button_widgets::<CampaignCompleteButtonKind, CAMPAIGN_COMPLETE_BUTTON_COUNT>(context);

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
