// A mission shown without being played: the developer editor's Screens menu (debug::screens,
// menu::dialog::preview) opens the retail screens that read the mission being played (the
// briefing, the debrief, the campaign-complete screen, the world map's next mission, the UI realm)
// for any campaign and mission, and they read this one while it's set (shown_mission). A preview
// changes nothing: its screens' buttons that would record progress in the profile or start a
// session only close them (is_active). Runtime only, never saved; cleared once the preview's
// screens have closed, and tied to the session it was set in: once another session starts (a
// switch queued in the same frame as the preview), it reads as unset.

use common::mem::SingleThreadStatic;

use super::ActiveMission;
use crate::{campaign::config::CampaignConfigs, profile::Profile};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MissionPreview {
    pub campaign_index: usize,
    pub mission_index: usize,
    // Whether the mission shows as finished: the world map then offers the next mission, as after
    // the debrief's Continue.
    pub completed: bool,
}

// The preview and the session generation it was set in (session::generation).
static PREVIEW: SingleThreadStatic<Option<(MissionPreview, u64)>> = SingleThreadStatic::new(None);

// The profile the preview's screens read (shown_profile): the player's as it was when the preview
// was set, played up to its mission (profile_through, the mission finished only if it shows so).
static PREVIEW_PROFILE: SingleThreadStatic<Option<Profile>> = SingleThreadStatic::new(None);

pub fn set(preview: Option<MissionPreview>) {
    PREVIEW.set(preview.map(|preview| (preview, crate::session::generation())));
    PREVIEW_PROFILE.set(preview.filter(|_| CampaignConfigs::is_initialized()).map(|preview| {
        played_up_to(
            crate::profile::get(),
            CampaignConfigs::get(),
            preview.campaign_index,
            preview.mission_index,
            preview.completed,
        )
    }));
}

pub fn get() -> Option<MissionPreview> {
    match *PREVIEW.as_ref() {
        Some((preview, generation)) if generation == crate::session::generation() => Some(preview),
        _ => None,
    }
}

// Whether a preview shows: the screens' buttons that would change the profile or the session only
// close them then.
pub fn is_active() -> bool {
    get().is_some()
}

// The mission the retail screens show: the preview's while one is set, else the one being played.
pub fn shown_mission() -> Option<ActiveMission> {
    match get() {
        Some(preview) => Some(ActiveMission {
            campaign_id: preview.campaign_index,
            mission_index: preview.mission_index,
            completed: preview.completed,
        }),
        None => super::active_mission(),
    }
}

// The profile the retail screens read: a preview's (as if the campaign had been played up to its
// mission, the mission finished if it shows so: every screen of a preview shows the same progress,
// the stele, the world map's cities and routes, a story panel's stele lines), else the player's.
pub fn shown_profile() -> &'static Profile {
    match PREVIEW_PROFILE.as_ref() {
        Some(profile) if is_active() => profile,
        _ => crate::profile::get(),
    }
}

// The profile as it would be had the campaign been played up to the mission at `mission_index`
// and through it: `profile` with every step before its step finished (at the step's first mission,
// where the profile hasn't finished it otherwise), none after it (the steps the player finished
// later, and the picks made at their choice points, are dropped), then the mission itself.
pub fn profile_through(
    profile: &Profile,
    configs: &CampaignConfigs,
    campaign_index: usize,
    mission_index: usize,
) -> Profile {
    played_up_to(profile, configs, campaign_index, mission_index, true)
}

// profile_through, the mission itself finished only if `finished`.
fn played_up_to(
    profile: &Profile,
    configs: &CampaignConfigs,
    campaign_index: usize,
    mission_index: usize,
    finished: bool,
) -> Profile {
    let mut profile = profile.clone();
    let Some(campaign) = configs.campaign(campaign_index) else {
        return profile;
    };
    let step = campaign.step_of(mission_index);
    if let Some(record) = profile.campaigns.iter_mut().find(|record| record.id == campaign.id) {
        record.missions_finished.retain(|&mission| campaign.step_of(mission) < step);
        record.choices.retain(|choice| choice.step <= step);
        record.finished = false;
    }
    let finished_missions = profile.record(&campaign.id).map(|record| record.missions_finished.clone()).unwrap_or_default();
    for earlier in campaign.steps().into_iter().filter(|&earlier| earlier < step) {
        if !campaign.is_step_finished(earlier, &finished_missions) {
            profile.mission_finished(configs, campaign_index, earlier);
        }
    }
    if finished {
        profile.mission_finished(configs, campaign_index, mission_index);
    }
    profile
}
