use engine::ui::UiSystem;
use serde::{Deserialize, Serialize};

use crate::{sim::resources::ResourceKind, world::stats::WorldStats};

// ----------------------------------------------
// Campaign / Mission Definitions (data-driven)
// ----------------------------------------------

// Top-level campaign configs, loaded from `assets/configs/game/campaigns.json`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignConfigs {
    pub campaigns: Vec<CampaignDef>,
}

// A single campaign: an ordered sequence of missions.
#[derive(Serialize, Deserialize)]
pub struct CampaignDef {
    pub name: String,
    pub missions: Vec<MissionDef>,
}

// A single mission: a starting map plus the requirements to complete it.
#[derive(Serialize, Deserialize)]
pub struct MissionDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub map: MissionMap,
    #[serde(default)]
    pub requirements: MissionRequirements,
}

// How a mission's starting map is loaded.
#[derive(Clone, Serialize, Deserialize)]
pub enum MissionMap {
    // PRIMARY: load an existing map through the save-game framework.
    SaveGame { save_file: String },
    // Dev/testing fallback: a built-in preset map (see debug/preset_maps.rs).
    Preset { preset_number: usize },
}

// A mission is complete once every goal in this list is satisfied.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionRequirements {
    pub goals: Vec<MissionGoal>,
}

// A single completion goal. Extend by adding a variant + a match arm in `is_met`.
#[derive(Serialize, Deserialize)]
pub enum MissionGoal {
    Population { min: u32 },
    Employment { min_employed: u32 },
    Treasury   { min_gold: u32 },
    Resource   { kind: ResourceKind, min: u32 },
}

impl MissionGoal {
    // Evaluate this goal against the current world stats. `WorldStats` already
    // aggregates population, employment, treasury total and resource counts.
    pub fn is_met(&self, stats: &WorldStats) -> bool {
        match self {
            Self::Population { min } => stats.population.total >= *min,
            Self::Employment { min_employed } => stats.population.employed >= *min_employed,
            Self::Treasury   { min_gold } => stats.treasury.gold_units_total >= *min_gold,
            Self::Resource   { kind, min } => stats.resources.all.count(*kind) >= *min,
        }
    }
}

impl MissionRequirements {
    // All goals must be satisfied. An empty goal list is considered already met.
    pub fn all_met(&self, stats: &WorldStats) -> bool {
        self.goals.iter().all(|goal| goal.is_met(stats))
    }
}

impl CampaignConfigs {
    #[inline]
    pub fn campaign(&self, campaign_id: usize) -> Option<&CampaignDef> {
        self.campaigns.get(campaign_id)
    }

    #[inline]
    pub fn mission(&self, campaign_id: usize, mission_index: usize) -> Option<&MissionDef> {
        self.campaign(campaign_id)?.missions.get(mission_index)
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    // Hand-written to mirror the shape of `#[derive(DrawDebugUi)]`, which can't
    // render the `Vec<CampaignDef>` field. This lets us keep using the
    // `engine::configurations!` macro (it calls `draw_debug_ui_with_header`).
    pub fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if self.campaigns.is_empty() {
            ui.text("No campaigns loaded.");
            return;
        }

        for (campaign_id, campaign) in self.campaigns.iter().enumerate() {
            let header = format!("[{campaign_id}] {} ({} missions)", campaign.name, campaign.missions.len());
            if ui.collapsing_header(&header, imgui::TreeNodeFlags::empty()) {
                for (mission_index, mission) in campaign.missions.iter().enumerate() {
                    ui.text(format!("  #{mission_index}: {} ({} goals)", mission.name, mission.requirements.goals.len()));
                }
            }
        }
    }

    pub fn draw_debug_ui_with_header(&self, header: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if ui.collapsing_header(header, imgui::TreeNodeFlags::empty()) {
            self.draw_debug_ui(ui_sys);
        }
    }
}

// ----------------------------------------------
// CampaignConfigs Global Singleton
// ----------------------------------------------

engine::configurations! { CAMPAIGN_CONFIGS_SINGLETON, CampaignConfigs, "game/campaigns" }
