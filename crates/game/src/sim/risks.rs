// The city's fire and collapse risks, as a whole (docs/plans/city-risks.md). Each building keeps
// its own risks (building::risks); this decides whether they build up at all, and keeps what they
// cost the city: the deaths from sickness and, month by month, the buildings lost (for Security's
// Disasters and the distinctions, docs/plans/ratings.md). Saved in `Simulation` and read through
// `SimContext::risks()`.

use serde::{Deserialize, Serialize};

use crate::{campaign::config::MissionRisksDef, cheats, config::RiskConfigs};

// What the city lost in a month: buildings burnt down or collapsed (houses among them) and
// residents who died of sickness. Only months with a loss are kept.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DisasterMonth {
    pub ordinal_month: u32, // GameCalendar's: year x 12 + month - 1.
    pub burnt: u32,
    pub collapsed: u32,
    pub houses_lost: u32, // Of the burnt and collapsed.
    pub deaths: u32,
}

impl DisasterMonth {
    #[inline]
    pub fn buildings_lost(&self) -> u32 {
        self.burnt + self.collapsed
    }
}

// What was lost, for CityRisks::record_disaster.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Disaster {
    BurntDown { house: bool },
    Collapsed { house: bool },
    Deaths(u32),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CityRisks {
    // Off for a mission that comes before its city can fight fires (a mission's `risks` block).
    enabled: bool,

    // Whether houses can fall sick, while risks are on: a mission can keep fires and collapses
    // but leave disease out (its `risks` block's `disease`).
    disease: bool,

    // Residents who have died of sickness in this game.
    disease_deaths: u32,

    // The months with a loss, oldest first.
    disasters: Vec<DisasterMonth>,
}

impl CityRisks {
    pub fn new(configs: &RiskConfigs) -> Self {
        Self { enabled: configs.enabled, disease: true, disease_deaths: 0, disasters: Vec::new() }
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

    // Residents who died of sickness in the month `ordinal_month`.
    pub fn add_disease_deaths(&mut self, count: u32, ordinal_month: u32) {
        self.disease_deaths = self.disease_deaths.saturating_add(count);
        self.record_disaster(Disaster::Deaths(count), ordinal_month);
    }

    // A loss in the month `ordinal_month`, into that month's record.
    pub fn record_disaster(&mut self, disaster: Disaster, ordinal_month: u32) {
        if disaster == Disaster::Deaths(0) {
            return;
        }
        if self.disasters.last().is_none_or(|last| last.ordinal_month != ordinal_month) {
            self.disasters.push(DisasterMonth { ordinal_month, ..Default::default() });
        }
        let month = self.disasters.last_mut().unwrap();
        match disaster {
            Disaster::BurntDown { house } => {
                month.burnt += 1;
                month.houses_lost += u32::from(house);
            }
            Disaster::Collapsed { house } => {
                month.collapsed += 1;
                month.houses_lost += u32::from(house);
            }
            Disaster::Deaths(count) => month.deaths = month.deaths.saturating_add(count),
        }
    }

    // The months with a loss, oldest first.
    #[inline]
    pub fn disasters(&self) -> &[DisasterMonth] {
        &self.disasters
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
