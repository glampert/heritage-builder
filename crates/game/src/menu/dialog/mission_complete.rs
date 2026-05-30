use strum::{EnumCount, EnumIter, EnumProperty};

use super::*;
use crate::{campaign, menu::ButtonDef};

// ----------------------------------------------
// MissionCompleteButtonKind
// ----------------------------------------------

const MISSION_COMPLETE_BUTTON_COUNT: usize = MissionCompleteButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum MissionCompleteButtonKind {
    #[strum(props(Label = "Next Mission"))]
    NextMission,

    #[strum(props(Label = "Continue Playing"))]
    ContinuePlaying,
}

impl ButtonDef for MissionCompleteButtonKind {
    fn on_pressed(self, context: &mut GameUiContext) -> bool {
        match self {
            Self::NextMission     => MissionComplete::on_next_mission(context),
            Self::ContinuePlaying => MissionComplete::on_continue_playing(context),
        }
    }
}

// ----------------------------------------------
// MissionComplete
// ----------------------------------------------

// Placeholder prompt shown when the active mission's requirements are met.
pub struct MissionComplete {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { MissionComplete, ["Mission Complete!"] }

impl MissionComplete {
    pub fn new(context: &mut GameUiContext) -> Self {
        let buttons = make_dialog_button_widgets::<MissionCompleteButtonKind, MISSION_COMPLETE_BUTTON_COUNT>(context);

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

    fn on_next_mission(context: &mut GameUiContext) -> bool {
        match campaign::advance_to_next_mission() {
            Some(map) => {
                // Loading the next mission's map tears down and rebuilds the
                // session, which resets the dialog stack (InGameMenus::drop).
                super::load_mission_map(&map);
                true
            }
            // No next mission: a CampaignComplete prompt was queued. Close this
            // dialog so the in-game end_frame opens the CampaignComplete dialog.
            None => super::close_current(context),
        }
    }

    fn on_continue_playing(context: &mut GameUiContext) -> bool {
        campaign::continue_playing();
        super::close_current(context)
    }
}
