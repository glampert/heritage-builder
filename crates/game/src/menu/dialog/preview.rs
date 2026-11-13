// Previews of the retail screens that are hard to reach in play, for the developer editor's Screens
// menu (debug::screens): a campaign mission's briefing (with its choice point's sites), its
// debrief, the campaign-complete screen, the world map after or during a mission (its next
// mission's box or choice, and their popups), and any story sequence.
//
// The developer editor draws no retail dialogs, so a preview switches to the in-game menus (the
// session command queue's menus toggle), opens its screen there, and switches back to the editor
// once every dialog has closed, the simulation paused again, in the same frame, if it was. A preview
// belongs to the session it was asked in: one that another session replaces before it opens (a
// switch queued in the same frame) is dropped. While it shows, the screens
// read its mission (campaign::preview) in place of the one being played, and their buttons that
// would record progress in the profile or start a session only close them; the sequences play
// without being recorded seen. Nothing it shows changes the profile or the session.

use common::mem::SingleThreadStatic;
use engine::log;

use super::*;
use crate::{
    GameLoop,
    campaign::{self, preview::MissionPreview},
    session,
};

// What a preview shows.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PreviewScreen {
    Briefing,                   // The mission's briefing, as Begin shows it (the Game menu's).
    BriefingAtStart,            // As it opens when the mission starts: a choice point's sites in place of Begin.
    Debrief,                    // MissionComplete, the mission just finished.
    CampaignComplete,           // The campaign finished at the mission (its last step).
    WorldMapAfter,              // The world map once the mission is finished: its next mission or choice.
    WorldMapDuring,             // The world map while the mission is played: its choice closed.
    Popup(world_map::MapPopup), // The world map with one of its popups.
    Story(String),              // A sequence of the campaign, by id.
}

impl PreviewScreen {
    // Whether the mission shows as finished (the world map offers the next mission then).
    fn completed(&self) -> bool {
        match self {
            Self::Briefing | Self::BriefingAtStart | Self::WorldMapDuring | Self::Story(_) => false,
            Self::Popup(popup) => *popup != world_map::MapPopup::FinishFirst,
            Self::Debrief | Self::CampaignComplete | Self::WorldMapAfter => true,
        }
    }
}

// A preview: the campaign (by index in campaigns.json), the mission and what to show of them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PreviewRequest {
    pub campaign_index: usize,
    pub mission_index: usize,
    pub screen: PreviewScreen,
}

struct PreviewState {
    pending: Option<PreviewRequest>, // Asked for in the editor: opened once the in-game menus are up.
    generation: u64,                 // The session it was asked in (session::generation).
    showing: Option<PreviewRequest>, // Its screens are open (or failed to).
    was_paused: bool,                // The simulation, when it was asked for.
    returning: bool,                 // Back to the editor: the menus toggle is queued.
}

static STATE: SingleThreadStatic<PreviewState> = SingleThreadStatic::new(PreviewState {
    pending: None,
    generation: 0,
    showing: None,
    was_paused: false,
    returning: false,
});

// Asks for a preview from the developer editor: its mission is shown from now on (the UI realm
// follows its campaign, unless ui.realm overrides it), and the in-game menus open it from the next
// frame. False if one is under way already.
pub fn request(request: PreviewRequest, context: &mut GameUiContext) -> bool {
    let state = STATE.as_mut();
    if state.pending.is_some() && state.generation != session::generation() {
        state.pending = None; // Its session was replaced before the in-game menus came up.
    }
    if state.pending.is_some() || state.showing.is_some() {
        return false;
    }
    log::info!(
        log::channel!("debug"),
        "Previewing {:?} of campaign {} mission {} ...",
        request.screen,
        request.campaign_index,
        request.mission_index + 1
    );
    campaign::preview::set(Some(MissionPreview {
        campaign_index: request.campaign_index,
        mission_index: request.mission_index,
        completed: request.screen.completed(),
    }));
    state.was_paused = context.sim.is_paused();
    state.generation = session::generation();
    state.pending = Some(request);
    GameLoop::get_mut().toggle_menus_mode();
    true
}

// The in-game menus' part, at the start of their end_frame: opens a preview asked for. True while a
// preview is under way, so the menus open no campaign prompt or briefing meanwhile (the switch back
// would drop it).
pub fn update(context: &mut GameUiContext) -> bool {
    let state = STATE.as_mut();
    if let Some(request) = state.pending.take() {
        if state.generation != session::generation() {
            // Another session came up before the in-game menus did: its menus aren't the preview's.
            log::warning!(log::channel!("debug"), "Preview {:?} dropped: the session changed.", request.screen);
            campaign::preview::set(None);
            return false;
        }
        if !open(&request, context) {
            log::error!(log::channel!("debug"), "Preview {:?} could not open.", request.screen);
        }
        state.showing = Some(request);
    }
    state.showing.is_some()
}

// The in-game menus' part, once the dialogs are drawn (end_frame) or a key has closed one (Escape,
// before the simulation's update): when the preview's screens have all closed, goes back to the
// editor, pausing the simulation again at once if it was, so no step runs between.
pub fn return_if_closed(context: &mut GameUiContext) {
    let state = STATE.as_mut();
    if state.showing.is_none() {
        return;
    }
    if !state.returning && super::current().is_none() && !is_story_playing() {
        state.returning = true;
        campaign::preview::set(None);
        if state.was_paused {
            context.sim.pause();
        }
        GameLoop::get_mut().toggle_menus_mode();
    }
}

// Ends the preview: the in-game menus are going (back to the editor, or a session switch).
pub fn reset() {
    let state = STATE.as_mut();
    if state.showing.is_some() || state.pending.is_some() {
        campaign::preview::set(None);
    }
    state.pending = None;
    state.showing = None;
    state.returning = false;
}

fn open(request: &PreviewRequest, context: &mut GameUiContext) -> bool {
    match &request.screen {
        PreviewScreen::Briefing => super::open(DialogMenuKind::MissionBriefing, true, context),
        PreviewScreen::BriefingAtStart => {
            super::close_all(context);
            mission_briefing::open_at_mission_start(context)
        }
        PreviewScreen::Debrief => super::open(DialogMenuKind::MissionComplete, true, context),
        PreviewScreen::CampaignComplete => super::open(DialogMenuKind::CampaignComplete, true, context),
        PreviewScreen::WorldMapAfter | PreviewScreen::WorldMapDuring => super::open(DialogMenuKind::WorldMap, true, context),
        PreviewScreen::Popup(popup) => {
            super::open(DialogMenuKind::WorldMap, true, context) && world_map::open_popup(context, *popup)
        }
        PreviewScreen::Story(sequence_id) => {
            super::close_all(context);
            let request = StoryRequest {
                campaign_index: request.campaign_index,
                sequence_id: sequence_id.clone(),
                panel: 0,
                records_seen: false,
                on_end: None,
            };
            super::play_story(request, context)
        }
    }
}
