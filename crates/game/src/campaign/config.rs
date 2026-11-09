use serde::{Deserialize, Serialize};

use engine::log;

use crate::{
    realm::Realm,
    sim::{calendar::EraDef, finance::TaxLevel, resources::ResourceKind},
    world::stats::WorldStats,
};

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
    // Names the campaign in the player's profile, so their progress survives reordering this
    // file. One left out becomes "campaign_<index>" when the configs load.
    #[serde(default)]
    pub id: String,
    pub name: String,
    // The realm whose look the UI takes while the campaign is played, and in the menus once the
    // campaign is the latest reached (see realm::ui_realm).
    #[serde(default)]
    pub realm: Realm,
    pub missions: Vec<MissionDef>,
    // The realm's eras, in the order they start: the calendar counts years from the latest.
    #[serde(default)]
    pub eras: Vec<EraDef>,
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
    // When the mission starts, and its seasons. None: the defaults from SimConfigs::calendar.
    #[serde(default)]
    pub calendar: Option<MissionCalendarDef>,
    // The treasury, tax level and wage the mission starts with. None: the map's own.
    #[serde(default)]
    pub finance: Option<MissionFinanceDef>,
}

// The calendar a mission starts with. Fields left out keep SimConfigs::calendar's.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MissionCalendarDef {
    // World Year and month (1 to 12) of the mission's first day.
    pub start_year: Option<u32>,
    pub start_month: Option<u32>,
    // Months (1 to 12) of the wet season.
    pub wet_months: Option<Vec<u32>>,
}

// The finances a mission starts with: a new treasury with fresh accounts and no debt. Fields
// left out take FinanceConfigs' defaults, and the map's balance if `starting_gold` is.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MissionFinanceDef {
    pub starting_gold: Option<u32>,
    pub tax_level: Option<TaxLevel>,
    // Gold per employed worker per year.
    pub wage: Option<u32>,
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
            Self::Treasury   { min_gold } => stats.treasury.gold_units_total >= i64::from(*min_gold),
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

    // The index of the campaign with this id (CampaignDef::id).
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.campaigns.iter().position(|campaign| campaign.id == id)
    }

    // Called once the configs load (shadows engine::config::Configs::post_load): every campaign
    // gets an id, since the profile records progress by id.
    fn post_load(&'static mut self) {
        for (index, campaign) in self.campaigns.iter_mut().enumerate() {
            if campaign.id.is_empty() {
                campaign.id = format!("campaign_{index}");
                log::warning!(log::channel!("campaign"), "Campaign '{}' has no id: using '{}'.", campaign.name, campaign.id);
            }
        }
        for (index, campaign) in self.campaigns.iter().enumerate() {
            if self.campaigns[..index].iter().any(|other| other.id == campaign.id) {
                log::error!(log::channel!("campaign"), "Two campaigns have the id '{}': the profile can't tell them apart.", campaign.id);
            }
        }
    }
}

// ----------------------------------------------
// CampaignConfigs Global Singleton
// ----------------------------------------------

engine::configurations! { CAMPAIGN_CONFIGS_SINGLETON, CampaignConfigs, "game/campaigns" }
