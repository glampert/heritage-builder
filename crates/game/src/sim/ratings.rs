// The city's four ratings (docs/plans/ratings.md): each rating's value and the drivers it was
// summed from at the last recompute, whether the game can be lost and how, the failure clocks, and
// the mission's distinctions already out of reach. Saved in `Simulation` and read through
// `SimContext::ratings()`. Which drivers feed which rating is data (ratings::config); what each
// driver reads is crate::ratings.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};

use crate::world::stats::WorldStats;

// ----------------------------------------------
// RatingKind
// ----------------------------------------------

// The four ratings (GDD 5.17). The fourth is Legitimacy in data and code, whatever its realm calls
// it: the Mandate, Divine Favour or Merit (`rating.<realm>.fourth`).
#[derive(
    Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RatingKind {
    Prosperity,
    Culture,
    Security,
    Legitimacy,
}

pub const RATING_COUNT: usize = 4;

pub const MAX_RATING: u32 = 100;

// ----------------------------------------------
// Rating
// ----------------------------------------------

// One driver's share of a rating at the last recompute, a whole number so the breakdown adds up to
// the value shown. `id` names it: `rating.driver.<id>` (Hud), or the realm's own name for it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DriverValue {
    pub id: String,
    pub value: i32,
}

// A rating's value, 0 to 100: the sum of its drivers, clamped.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rating {
    pub value: u32,
    pub drivers: Vec<DriverValue>,
}

impl Rating {
    // The drivers' sum, clamped to 0..=100.
    pub fn from_drivers(drivers: Vec<DriverValue>) -> Self {
        let sum: i32 = drivers.iter().map(|driver| driver.value).sum();
        Self { value: sum.clamp(0, MAX_RATING as i32) as u32, drivers }
    }
}

// ----------------------------------------------
// Failure
// ----------------------------------------------

// Why a game was lost (GDD 7): its debt ran out its months, Legitimacy stayed near zero for a year
// (the court recalls the governor), or its time limit ran out.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Display, EnumIter, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FailureReason {
    Debt,
    Legitimacy,
    Time,
}

// Which failures can end this game. A campaign mission's `failure` block sets them (both on unless
// it says not to), and so does a custom game's Failure switch; any other game (a preset or new map)
// can't be lost.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FailureSwitches {
    pub debt: bool,
    pub legitimacy: bool,
}

impl FailureSwitches {
    pub const OFF: Self = Self { debt: false, legitimacy: false };
    pub const ON: Self = Self { debt: true, legitimacy: true };
}

// ----------------------------------------------
// CityRatings
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct CityRatings {
    ratings: BTreeMap<RatingKind, Rating>,

    // The ordinal month (GameCalendar's) of the last recompute; None before the first, as in a
    // save from before ratings, which is rated as it loads.
    rated_month: Option<u32>,

    fails_on: FailureSwitches,

    // Month-ends in a row Legitimacy has been below RatingRules::recall_below.
    legitimacy_low_months: u32,

    // Set once the game is lost; a save of it loads lost.
    failure: Option<FailureReason>,

    // The ids of the mission's distinctions it can no longer earn ("never in debt" after a month in
    // debt), tracked month by month.
    spoiled: BTreeSet<String>,
}

impl Default for CityRatings {
    fn default() -> Self {
        Self::new()
    }
}

impl CityRatings {
    // A new game's: not rated yet, and it can't be lost.
    pub fn new() -> Self {
        Self {
            ratings: RatingKind::iter().map(|kind| (kind, Rating::default())).collect(),
            rated_month: None,
            fails_on: FailureSwitches::OFF,
            legitimacy_low_months: 0,
            failure: None,
            spoiled: BTreeSet::new(),
        }
    }

    // A new city: nothing rated, no clock running, nothing lost or spoiled, and the failures this
    // game can end in.
    pub fn start(&mut self, fails_on: FailureSwitches) {
        *self = Self { fails_on, ..Self::new() };
    }

    #[inline]
    pub fn get(&self, kind: RatingKind) -> &Rating {
        // Every kind is in the map: new() puts them all in, and set() never takes one out.
        self.ratings.get(&kind).expect("every rating is kept")
    }

    #[inline]
    pub fn value(&self, kind: RatingKind) -> u32 {
        self.get(kind).value
    }

    // The recompute's result for a rating.
    pub fn set(&mut self, kind: RatingKind, rating: Rating) {
        self.ratings.insert(kind, rating);
    }

    // A value set by hand (ratings.set), its drivers left as they were, until the next recompute.
    pub fn set_value(&mut self, kind: RatingKind, value: u32) {
        if let Some(rating) = self.ratings.get_mut(&kind) {
            rating.value = value.min(MAX_RATING);
        }
    }

    #[inline]
    pub fn rated_month(&self) -> Option<u32> {
        self.rated_month
    }

    #[inline]
    pub fn set_rated_month(&mut self, month: u32) {
        self.rated_month = Some(month);
    }

    #[inline]
    pub fn is_rated(&self) -> bool {
        self.rated_month.is_some()
    }

    #[inline]
    pub fn fails_on(&self) -> FailureSwitches {
        self.fails_on
    }

    #[inline]
    pub fn set_fails_on(&mut self, fails_on: FailureSwitches) {
        self.fails_on = fails_on;
    }

    #[inline]
    pub fn legitimacy_low_months(&self) -> u32 {
        self.legitimacy_low_months
    }

    #[inline]
    pub fn set_legitimacy_low_months(&mut self, months: u32) {
        self.legitimacy_low_months = months;
    }

    #[inline]
    pub fn failure(&self) -> Option<FailureReason> {
        self.failure
    }

    #[inline]
    pub fn set_failure(&mut self, failure: Option<FailureReason>) {
        self.failure = failure;
    }

    #[inline]
    pub fn spoiled(&self) -> &BTreeSet<String> {
        &self.spoiled
    }

    #[inline]
    pub fn is_spoiled(&self, distinction: &str) -> bool {
        self.spoiled.contains(distinction)
    }

    // Whether it was newly spoiled.
    pub fn spoil(&mut self, distinction: &str) -> bool {
        self.spoiled.insert(distinction.to_string())
    }

    // The world stats' ratings, recounted with them (World::update, refresh_stats), for the goals.
    pub fn tally(&self, stats: &mut WorldStats) {
        for kind in RatingKind::iter() {
            stats.ratings.values[kind as usize] = self.value(kind);
        }
    }
}
