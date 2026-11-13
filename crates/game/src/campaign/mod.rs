// ----------------------------------------------
// Campaign / Mission Progression
// ----------------------------------------------
//
// Campaign progress lives in a session-external singleton (`CampaignManager`)
// because loading a mission map replaces the entire `GameSession` (incl. the
// `Simulation` and `GameSystems`), which would otherwise wipe progress stored
// inside the session. To still persist progress into the player's own saves,
// the session snapshots this singleton on save and restores it on load.
//
// The manager is a pure state machine: it never touches `GameLoop`/`Engine`.
// Functions that change the active mission return the `MissionMap` to load and
// the caller (UI layer) performs the actual load. This keeps the manager fully
// unit-testable in the headless test harness.

pub mod config;
pub mod stele;

use config::{CampaignConfigs, MissionFinanceDef, MissionMap, MissionRisksDef};
use serde::{Deserialize, Serialize};

use engine::log;

use crate::{
    config::GameConfigs,
    sim::{CalendarSettings, SimContext},
};

// ----------------------------------------------
// Campaign progress & prompts
// ----------------------------------------------

// Serialized via the session snapshot so a mid-mission save resumes the campaign.
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct CampaignProgress {
    pub active: Option<ActiveMission>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveMission {
    pub campaign_id: usize,
    pub mission_index: usize,
    // True once the mission goals were met and acknowledged; prevents re-prompting
    // when the player chose to keep playing the completed mission.
    pub completed: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CampaignPrompt {
    MissionComplete,
    CampaignComplete,
}

// ----------------------------------------------
// CampaignManager (singleton)
// ----------------------------------------------

pub struct CampaignManager {
    progress: CampaignProgress,
    pending_prompt: Option<CampaignPrompt>,
    // Set while a campaign-driven mission load is in flight, so the loaded base
    // map's (empty) progress snapshot does not clobber the intended active mission.
    suppress_snapshot_restore: bool,
    // The progress before that mission start, which cancel_mission_start puts back if its map
    // doesn't load.
    progress_before_start: CampaignProgress,
    // Missions whose goals were met since GameLoop last took them, as (campaign id,
    // mission index): the player's profile records them (take_finished_missions).
    finished_missions: Vec<(usize, usize)>,
}

impl CampaignManager {
    fn new() -> Self {
        Self {
            progress: CampaignProgress::default(),
            pending_prompt: None,
            suppress_snapshot_restore: false,
            progress_before_start: CampaignProgress::default(),
            finished_missions: Vec::new(),
        }
    }
}

common::singleton_late_init! { CAMPAIGN_MANAGER_SINGLETON, CampaignManager }

// ----------------------------------------------
// Public free-function API (GameLoop-free)
// ----------------------------------------------

pub fn initialize() {
    if CampaignManager::is_initialized() {
        return; // Initialize only once.
    }
    CampaignManager::initialize(CampaignManager::new());
}

// Clear all campaign state (e.g. quitting to main menu or starting a sandbox game). The missions
// finished but not yet taken stay queued for the profile: an automation command can reset the
// campaign in the frame a mission finishes (a time.fast_forward, then a session.new_map), before
// GameLoop takes them.
pub fn reset() {
    let mgr = CampaignManager::get_mut();
    let finished_missions = std::mem::take(&mut mgr.finished_missions);
    *mgr = CampaignManager { finished_missions, ..CampaignManager::new() };
}

pub fn active_mission() -> Option<ActiveMission> {
    CampaignManager::get().progress.active.clone()
}

// Begin a campaign at its first mission. Returns the map to load, or None if the
// campaign id is invalid / has no missions.
pub fn start_campaign(campaign_id: usize) -> Option<MissionMap> {
    start_mission(campaign_id, 0)
}

// Begin a campaign at one of its missions, e.g. the next one the player hasn't finished. Returns
// the map to load, or None if there's no such mission.
pub fn start_mission(campaign_id: usize, mission_index: usize) -> Option<MissionMap> {
    let map = CampaignConfigs::get().mission(campaign_id, mission_index)?.map.clone();

    let mgr = CampaignManager::get_mut();
    mgr.progress_before_start = mgr.progress.clone();
    mgr.progress.active = Some(ActiveMission { campaign_id, mission_index, completed: false });
    mgr.pending_prompt = None;
    mgr.suppress_snapshot_restore = true;

    Some(map)
}

// Begin a campaign at one of its missions as the menus and automation do: as start_mission, but a
// mission whose save map this game can't load (missing, unreadable or of another save version)
// starts nothing and leaves the campaign as it was. Returns the map to load, or why it can't start.
pub fn start_mission_if_loadable(campaign_id: usize, mission_index: usize) -> Result<MissionMap, String> {
    let Some(mission) = CampaignConfigs::get().mission(campaign_id, mission_index) else {
        return Err(format!("Campaign {campaign_id} has no mission {mission_index}."));
    };
    if let MissionMap::SaveGame { save_file } = &mission.map {
        match crate::session::probe_save(engine::file_sys::paths::PathRef::from_str(save_file)) {
            Ok(summary) if summary.loadable => {}
            Ok(_) => return Err(format!("The mission's map '{save_file}' is of another save version.")),
            Err(err) => return Err(format!("The mission's map '{save_file}' doesn't load: {err}")),
        }
    }
    start_mission(campaign_id, mission_index).ok_or_else(|| format!("Campaign {campaign_id} has no mission {mission_index}."))
}

// Advance to the next mission: the campaign's next step (a variant's sibling sites are skipped).
// Returns its map to load, or None when there is no next mission (in which case a
// CampaignComplete prompt is raised).
// The debrief's (MissionComplete) Continue goes through the world map instead when it shows the
// campaign: its choice box starts the site picked, or the next mission without a choice; without a
// world map to show it, this starts the site picked before (the profile's choice), else the choice
// point itself.
pub fn advance_to_next_mission() -> Option<MissionMap> {
    let mgr = CampaignManager::get_mut();

    let (campaign_id, next_index) = match mgr.progress.active.as_ref() {
        Some(active) => {
            let configs = CampaignConfigs::get();
            let next = configs.campaign(active.campaign_id).and_then(|campaign| {
                let step = campaign.next_step_after(active.mission_index)?;
                let picked = crate::profile::get().record(&campaign.id).and_then(|record| record.choice(step));
                Some(campaign.step_mission(step, picked))
            });
            (active.campaign_id, next.unwrap_or(usize::MAX))
        }
        None => return None,
    };

    if let Some(mission) = CampaignConfigs::get().mission(campaign_id, next_index) {
        let map = mission.map.clone();
        mgr.progress_before_start = mgr.progress.clone();
        mgr.progress.active = Some(ActiveMission { campaign_id, mission_index: next_index, completed: false });
        mgr.pending_prompt = None;
        mgr.suppress_snapshot_restore = true;
        Some(map)
    } else {
        // No more missions: the campaign is complete.
        mgr.pending_prompt = Some(CampaignPrompt::CampaignComplete);
        None
    }
}

// Player chose to keep playing the completed mission: dismiss the prompt. The
// mission stays `completed`, so it won't re-trigger.
pub fn continue_playing() {
    CampaignManager::get_mut().pending_prompt = None;
}

pub fn has_pending_prompt() -> bool {
    CampaignManager::get().pending_prompt.is_some()
}

pub fn take_pending_prompt() -> Option<CampaignPrompt> {
    CampaignManager::get_mut().pending_prompt.take()
}

// The missions finished since the last call, as (campaign id, mission index). tick() runs inside
// the simulation step, so GameLoop takes them each frame and records them in the profile.
pub fn take_finished_missions() -> Vec<(usize, usize)> {
    std::mem::take(&mut CampaignManager::get_mut().finished_missions)
}

// Snapshot for the session save.
pub fn capture_snapshot() -> CampaignProgress {
    CampaignManager::get().progress.clone()
}

// Restore from a session load. No-op (clearing the flag) for campaign-driven
// mission loads, so the intended active mission survives the map swap. Returns
// true for those: the load starts the active mission.
pub fn restore_snapshot(progress: CampaignProgress) -> bool {
    if take_mission_start() {
        return true;
    }
    let mgr = CampaignManager::get_mut();
    mgr.progress = progress;
    mgr.pending_prompt = None;
    false
}

// True once after start_campaign or advance_to_next_mission: the next session is the
// active mission's start. Save loads take it through restore_snapshot; a preset
// mission map, which isn't a save, takes it when its session is created.
pub fn take_mission_start() -> bool {
    std::mem::take(&mut CampaignManager::get_mut().suppress_snapshot_restore)
}

// A mission start whose map failed to load: the campaign goes back to the progress it had before
// start_mission or advance_to_next_mission, and the next load (the player's own save) restores its
// own progress instead of starting that mission on it. No-op without a mission start pending.
pub fn cancel_mission_start() {
    if take_mission_start() {
        let mgr = CampaignManager::get_mut();
        mgr.progress = std::mem::take(&mut mgr.progress_before_start);
        log::warning!(log::channel!("campaign"), "The mission's map didn't load: the mission doesn't start.");
    }
}

// The finances the active mission starts with, if it sets any. None outside a campaign.
pub fn active_mission_finance() -> Option<MissionFinanceDef> {
    let active = CampaignManager::get().progress.active.clone()?;
    let campaign = CampaignConfigs::get().campaign(active.campaign_id)?;
    campaign.missions.get(active.mission_index)?.finance.clone()
}

// The risks setting of the active mission, if it sets one. None outside a campaign.
pub fn active_mission_risks() -> Option<MissionRisksDef> {
    let active = CampaignManager::get().progress.active.clone()?;
    let campaign = CampaignConfigs::get().campaign(active.campaign_id)?;
    campaign.missions.get(active.mission_index)?.risks.clone()
}

// The calendar the active mission starts with: its own start and wet months over
// SimConfigs::calendar, with its campaign's eras. None outside a campaign. Invalid
// settings are logged and replaced by the defaults.
pub fn active_mission_calendar() -> Option<CalendarSettings> {
    let active = CampaignManager::get().progress.active.clone()?;
    let campaign = CampaignConfigs::get().campaign(active.campaign_id)?;
    let mission = campaign.missions.get(active.mission_index)?;

    let defaults = &GameConfigs::get().sim.calendar;
    let mut settings = defaults.clone();

    if let Some(calendar) = &mission.calendar {
        settings.start_year = calendar.start_year.unwrap_or(defaults.start_year);
        settings.start_month = calendar.start_month.unwrap_or(defaults.start_month);
        if let Some(wet_months) = &calendar.wet_months {
            settings.wet_months = wet_months.clone();
        }
    }
    if !campaign.eras.is_empty() {
        settings.eras = campaign.eras.clone();
    }

    if let Err(err) = settings.validate() {
        log::error!(log::channel!("campaign"), "Mission '{}' has an invalid calendar: {err} Using the defaults.", mission.name);
        return Some(defaults.clone());
    }

    Some(settings)
}

// Per-tick requirement evaluation. Called from `CampaignSystem::update`.
pub fn tick(context: &SimContext) {
    let mgr = CampaignManager::get_mut();

    let (campaign_id, mission_index) = match mgr.progress.active.as_ref() {
        Some(active) if !active.completed => (active.campaign_id, active.mission_index),
        _ => return,
    };

    let Some(mission) = CampaignConfigs::get().mission(campaign_id, mission_index) else {
        return;
    };

    if mission.requirements.all_met(context.world().stats()) {
        if let Some(active) = mgr.progress.active.as_mut() {
            active.completed = true;
        }
        mgr.pending_prompt = Some(CampaignPrompt::MissionComplete);
        mgr.finished_missions.push((campaign_id, mission_index));
    }
}
