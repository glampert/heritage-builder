// The city's fire and collapse risks, as a whole (docs/plans/city-risks.md). Each building keeps
// its own risks (building::risks); this decides whether they build up at all. Saved in
// `Simulation` and read through `SimContext::risks()`.

use serde::{Deserialize, Serialize};

use crate::{
    campaign::config::MissionRisksDef,
    cheats,
    config::RiskConfigs,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct CityRisks {
    // Off for a mission that comes before its city can fight fires (a mission's `risks` block).
    enabled: bool,

    // Whether houses can fall sick, while risks are on: a mission can keep fires and collapses
    // but leave disease out (its `risks` block's `disease`).
    disease: bool,

    // Residents who have died of sickness in this game.
    disease_deaths: u32,
}

impl CityRisks {
    pub fn new(configs: &RiskConfigs) -> Self {
        Self { enabled: configs.enabled, disease: true, disease_deaths: 0 }
    }

    // A campaign mission's start: its own settings, or the defaults.
    pub fn start_mission(&mut self, mission: &MissionRisksDef, configs: &RiskConfigs) {
        self.enabled = mission.enabled.unwrap_or(configs.enabled);
        self.disease = mission.disease.unwrap_or(true);
    }

    #[inline]
    pub fn is_disease_enabled(&self) -> bool {
        self.disease
    }

    #[inline]
    pub fn set_disease_enabled(&mut self, disease: bool) {
        self.disease = disease;
    }

    #[inline]
    pub fn disease_deaths(&self) -> u32 {
        self.disease_deaths
    }

    #[inline]
    pub fn add_disease_deaths(&mut self, count: u32) {
        self.disease_deaths = self.disease_deaths.saturating_add(count);
    }

    // Whether disease builds up and outbreaks happen now: risks active, and disease on.
    #[inline]
    pub fn is_disease_active(&self) -> bool {
        self.is_active() && self.disease
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    // Whether risks build up now: enabled, and not stopped by the `no_city_risks` cheat.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.enabled && !cheats::get().no_city_risks
    }
}
