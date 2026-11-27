// The city's labour priorities (docs/plans/messages.md, M11 Phase 2): the six sectors of the
// Labour page in the player's order, 1 first, and the clock of the city-wide allocation that
// hands the houses' workers to the employers by it (crate::labour). Saved in `Simulation` and read
// through `SimContext::labour()`.

use common::time::{Seconds, UpdateTimer};
use engine::log;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, EnumString, IntoEnumIterator};

use crate::{config::LabourRules, tile::sets::TileSector};

// ----------------------------------------------
// LabourSector
// ----------------------------------------------

// The Labour page's sectors (city-labour-<realm>.jpg), in the mockup's order: a new game's
// priorities. Each building with workers belongs to one, by its tile's sector.
#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    Serialize,
    Deserialize,
    Display,
    EnumIter,
    EnumString,
    EnumCount,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LabourSector {
    FoodAndFarming,
    Industry,
    Services,
    CultureAndReligion,
    Infrastructure,
    Trade,
}

pub const LABOUR_SECTOR_COUNT: usize = LabourSector::COUNT;

impl LabourSector {
    // The sector a building with workers counts under: its tile's, Infrastructure for any other
    // (housing, roads, beautification; decision Q6).
    pub fn of_tile_sector(sector: TileSector) -> Self {
        match sector {
            TileSector::FoodAndFarming => Self::FoodAndFarming,
            TileSector::IndustryAndResources => Self::Industry,
            TileSector::Services => Self::Services,
            TileSector::CultureAndReligion => Self::CultureAndReligion,
            TileSector::TradeAndEconomy => Self::Trade,
            TileSector::Infrastructure
            | TileSector::None
            | TileSector::Housing
            | TileSector::Roads
            | TileSector::Beautification => Self::Infrastructure,
        }
    }

    // Its name's text key (Hud): `labour.sector.food_and_farming`.
    pub fn name_key(self) -> String {
        format!("labour.sector.{self}")
    }
}

// ----------------------------------------------
// CityLabour
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct CityLabour {
    // Every sector once, priority 1 first.
    order: Vec<LabourSector>,
    // Paces the allocation (LabourRules::allocate_secs).
    allocate_timer: UpdateTimer,
    // An allocation asked for by something the world step did (a building finished): it runs at
    // the end of the same step, so it never outlives one and needn't be saved.
    #[serde(skip)]
    requested: bool,
}

impl CityLabour {
    pub fn new(rules: &LabourRules) -> Self {
        Self { order: Self::default_order(), allocate_timer: UpdateTimer::new(rules.allocate_secs), requested: false }
    }

    fn default_order() -> Vec<LabourSector> {
        LabourSector::iter().collect()
    }

    // The sectors, priority 1 first.
    #[inline]
    pub fn order(&self) -> &[LabourSector] {
        &self.order
    }

    // A sector's priority, from 1.
    pub fn priority(&self, sector: LabourSector) -> usize {
        self.order.iter().position(|s| *s == sector).map_or(LABOUR_SECTOR_COUNT, |index| index + 1)
    }

    // Moves `sector` to `priority` (from 1, clamped), the sectors between it and there moving one
    // place to make room; −'s and +'s swap with a neighbour is a move by one. Returns whether the
    // order changed.
    pub fn set_priority(&mut self, sector: LabourSector, priority: usize) -> bool {
        let to = priority.clamp(1, LABOUR_SECTOR_COUNT) - 1;
        let Some(from) = self.order.iter().position(|s| *s == sector) else {
            return false;
        };
        if from == to {
            return false;
        }
        let moved = self.order.remove(from);
        self.order.insert(to, moved);
        true
    }

    // The whole order at once (automation); refused unless it names every sector once.
    pub fn set_order(&mut self, order: &[LabourSector]) -> bool {
        if !Self::is_valid_order(order) {
            return false;
        }
        self.order = order.to_vec();
        true
    }

    fn is_valid_order(order: &[LabourSector]) -> bool {
        order.len() == LABOUR_SECTOR_COUNT && LabourSector::iter().all(|sector| order.contains(&sector))
    }

    // Advances the clock by a world step; true when an allocation is due (its time, or asked for).
    pub fn tick(&mut self, delta_time_secs: Seconds) -> bool {
        let due = self.allocate_timer.tick(delta_time_secs).should_update();
        let requested = std::mem::take(&mut self.requested);
        due || requested
    }

    // An allocation at the end of this world step.
    #[inline]
    pub fn request_allocation(&mut self) {
        self.requested = true;
    }

    pub fn post_load(&mut self, rules: &LabourRules) {
        self.allocate_timer.post_load(rules.allocate_secs);
        if !Self::is_valid_order(&self.order) {
            log::warning!(
                log::channel!("labour"),
                "The save's labour order {:?} isn't every sector once: reset.",
                self.order
            );
            self.order = Self::default_order();
        }
    }
}
