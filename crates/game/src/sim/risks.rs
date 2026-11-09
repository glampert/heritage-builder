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
}

impl CityRisks {
    pub fn new(configs: &RiskConfigs) -> Self {
        Self { enabled: configs.enabled }
    }

    // A campaign mission's start: its own setting, or the default.
    pub fn start_mission(&mut self, mission: &MissionRisksDef, configs: &RiskConfigs) {
        self.enabled = mission.enabled.unwrap_or(configs.enabled);
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
