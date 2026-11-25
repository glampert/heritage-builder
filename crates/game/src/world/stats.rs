use crate::{
    building::HouseLevel,
    sim::{
        ratings::{RATING_COUNT, RatingKind},
        resources::{ResourceKind, ResourceStock},
    },
};

// ----------------------------------------------
// WorldStats & Helper Types
// ----------------------------------------------

#[derive(Default)]
pub struct PopulationStats {
    pub total: u32, // Entire population. Workforce may be less than total.
    pub employed: u32,
    pub unemployed: u32,
}

impl PopulationStats {
    // Returns normalized [0,1] ratio.
    pub fn employment_ratio(&self) -> f32 {
        debug_assert!(self.is_valid());

        let workforce = self.workforce();
        if workforce == 0 {
            0.0
        } else {
            let inv_workforce = 1.0 / workforce as f32;
            self.employed as f32 * inv_workforce
        }
    }

    // Returns normalized [0,1] ratio.
    pub fn unemployment_ratio(&self) -> f32 {
        debug_assert!(self.is_valid());

        let workforce = self.workforce();
        if workforce == 0 {
            0.0
        } else {
            let inv_workforce = 1.0 / workforce as f32;
            self.unemployed as f32 * inv_workforce
        }
    }

    pub fn workforce(&self) -> u32 {
        self.employed + self.unemployed
    }

    pub fn is_valid(&self) -> bool {
        self.workforce() <= self.total
    }
}

#[derive(Default)]
pub struct WorkerStats {
    pub total: u32,
    pub min_required: u32,
    pub max_employed: u32,
    pub buildings_below_min: u32,
    pub buildings_below_max: u32,
}

#[derive(Default)]
pub struct TreasuryStats {
    pub gold_units_total: i64, // The treasury's balance (sim::Finance); negative is debt.
    pub tax_generated: u32,
    pub tax_available: u32,
    pub tax_collected: u32,
}

// What the city's faith adds to the stats (sim::CityFaith::tally).
#[derive(Default)]
pub struct FaithStats {
    pub festivals_held: u32, // In this city: the mission's goals count them.
}

// What the river's floods add to the stats (sim::CityFloods::tally).
#[derive(Default)]
pub struct FloodStats {
    pub floods: u32,   // Since the game's start.
    pub survived: u32, // Of those, the floods that breached no dyke: the FloodsSurvived goal.
}

// What the city's trade adds to the stats (sim::Finance::tally, sim::CityTrade::tally).
#[derive(Default)]
pub struct TradeStats {
    pub income: u32,       // The exports' gold this calendar year or last, whichever is higher.
    pub requests_met: u32, // Since the game's start: the RequestsFulfilled goal.
}

pub struct HousingStats {
    pub total: u32,
    pub lowest_level: HouseLevel,
    pub highest_level: HouseLevel,
    pub at_level: [u32; HouseLevel::count()], // Finished houses by level, from HouseLevel::Level0.
}

impl HousingStats {
    // The houses at `level` or above, the level as the player reads it, from 1 (MissionGoal::
    // HousesAtLevel); 0 for a level past the top.
    pub fn at_or_above(&self, level: u32) -> u32 {
        let first = (level.max(1) - 1) as usize;
        self.at_level.iter().skip(first).sum()
    }
}

// The four ratings as the goals read them (sim::CityRatings::tally): their values at the last
// recompute, or one set by hand since.
#[derive(Default)]
pub struct RatingStats {
    pub values: [u32; RATING_COUNT], // In RatingKind's order.
}

impl RatingStats {
    #[inline]
    pub fn value(&self, kind: RatingKind) -> u32 {
        self.values[kind as usize]
    }
}

pub struct GlobalResourceCounts {
    // Combined sum of resources (all units + all buildings).
    pub all: ResourceStock,

    // Resources held by spawned units.
    pub units: ResourceStock,

    // Resources held by each kind of building.
    pub storage_yards: ResourceStock,
    pub granaries: ResourceStock,
    pub houses: ResourceStock,
    pub markets: ResourceStock,
    pub producers: ResourceStock,
    pub services: ResourceStock,
}

impl GlobalResourceCounts {
    // A good held in the storage yards and granaries (MissionGoal::Resource).
    #[inline]
    pub fn in_storage(&self, kind: ResourceKind) -> u32 {
        self.storage_yards.count(kind) + self.granaries.count(kind)
    }
}

pub struct WorldStats {
    // Global counts:
    pub population: PopulationStats,
    pub workers: WorkerStats,
    pub treasury: TreasuryStats,
    pub faith: FaithStats,
    pub floods: FloodStats,
    pub trade: TradeStats,
    pub ratings: RatingStats,

    // Housing stats:
    pub houses: HousingStats,

    // Global resource tally:
    pub resources: GlobalResourceCounts,
}

impl Default for WorldStats {
    fn default() -> Self {
        Self {
            population: PopulationStats::default(),
            workers: WorkerStats::default(),
            treasury: TreasuryStats::default(),
            faith: FaithStats::default(),
            floods: FloodStats::default(),
            trade: TradeStats::default(),
            ratings: RatingStats::default(),
            houses: HousingStats {
                total: 0,
                lowest_level: HouseLevel::max(),
                highest_level: HouseLevel::min(),
                at_level: [0; HouseLevel::count()],
            },
            resources: GlobalResourceCounts {
                all: ResourceStock::accept_all(),
                units: ResourceStock::accept_all(),
                storage_yards: ResourceStock::accept_all_except(ResourceKind::Gold),
                granaries: ResourceStock::with_accepted_kinds(ResourceKind::foods()),
                houses: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                markets: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                producers: ResourceStock::accept_all(),
                services: ResourceStock::accept_all(),
            },
        }
    }
}

impl WorldStats {
    pub fn reset(&mut self) {
        // Reset all counts to zero.
        *self = Self::default();
    }

    pub fn add_unit_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.units.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_storage_yard_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.storage_yards.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_granary_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.granaries.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_house_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.houses.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_market_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.markets.add(kind, count);
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_producer_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.producers.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_service_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn update_housing_stats(&mut self, level: HouseLevel) {
        if level < self.houses.lowest_level {
            self.houses.lowest_level = level;
        }
        if level > self.houses.highest_level {
            self.houses.highest_level = level;
        }
        self.houses.at_level[level as usize] += 1;
        self.houses.total += 1;
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn employment_rate_basic() {
        let stats = PopulationStats { total: 100, employed: 75, unemployed: 25 };

        assert!((stats.employment_ratio() - 0.75).abs() < f32::EPSILON);
        assert!((stats.unemployment_ratio() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn employment_rate_zero_population() {
        let stats = PopulationStats { total: 0, employed: 0, unemployed: 0 };

        assert_eq!(stats.employment_ratio(), 0.0);
        assert_eq!(stats.unemployment_ratio(), 0.0);
    }
}
