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
// TODO(realm-ui): Phase 3's debrief replaces it; its "Continue to the world map" opens the world
// map screen (dialog::open(DialogMenuKind::WorldMap, ..)), or goes to the next mission when the
// campaign has no world map (world_map::provider is None).
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
                buttons,
            ),
        }
    }

    fn on_next_mission(context: &mut GameUiContext) -> bool {
        // A next mission that offers a choice: the world map, whose choice box starts the site
        // picked (the screen shows the next step's choice: world_map::provider). The mission just
        // finished stays the one being played until then.
        if next_step_has_choice() {
            return super::open(DialogMenuKind::WorldMap, true, context);
        }
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

// Whether the step after the mission being played offers a choice the world map can show: the map
// shows that campaign (not another realm's, under a ui.realm override) and its choice box.
fn next_step_has_choice() -> bool {
    let Some(active) = campaign::active_mission() else {
        return false;
    };
    if !super::world_map::shows_campaign_in_game(active.campaign_id) {
        return false;
    }
    crate::world_map::provider(active.campaign_id).is_some_and(|provider| {
        use crate::world_map::WorldMapProvider;
        provider.next_mission_choice().is_some()
    })
}
