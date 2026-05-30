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

use config::{CampaignConfigs, MissionMap};
use serde::{Deserialize, Serialize};

use crate::sim::SimContext;

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
}

impl CampaignManager {
    fn new() -> Self {
        Self {
            progress: CampaignProgress::default(),
            pending_prompt: None,
            suppress_snapshot_restore: false,
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

// Clear all campaign state (e.g. quitting to main menu or starting a sandbox game).
pub fn reset() {
    *CampaignManager::get_mut() = CampaignManager::new();
}

pub fn active_mission() -> Option<ActiveMission> {
    CampaignManager::get().progress.active.clone()
}

// Begin a campaign at its first mission. Returns the map to load, or None if the
// campaign id is invalid / has no missions.
pub fn start_campaign(campaign_id: usize) -> Option<MissionMap> {
    let map = CampaignConfigs::get().mission(campaign_id, 0)?.map.clone();

    let mgr = CampaignManager::get_mut();
    mgr.progress.active = Some(ActiveMission { campaign_id, mission_index: 0, completed: false });
    mgr.pending_prompt = None;
    mgr.suppress_snapshot_restore = true;

    Some(map)
}

// Advance to the next mission. Returns its map to load, or None when there is no
// next mission (in which case a CampaignComplete prompt is raised).
pub fn advance_to_next_mission() -> Option<MissionMap> {
    let mgr = CampaignManager::get_mut();

    let (campaign_id, next_index) = match mgr.progress.active.as_ref() {
        Some(active) => (active.campaign_id, active.mission_index + 1),
        None => return None,
    };

    if let Some(mission) = CampaignConfigs::get().mission(campaign_id, next_index) {
        let map = mission.map.clone();
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

// Snapshot for the session save.
pub fn capture_snapshot() -> CampaignProgress {
    CampaignManager::get().progress.clone()
}

// Restore from a session load. No-op (clearing the flag) for campaign-driven
// mission loads, so the intended active mission survives the map swap.
pub fn restore_snapshot(progress: CampaignProgress) {
    let mgr = CampaignManager::get_mut();
    if mgr.suppress_snapshot_restore {
        mgr.suppress_snapshot_restore = false;
        return;
    }
    mgr.progress = progress;
    mgr.pending_prompt = None;
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
    }
}
