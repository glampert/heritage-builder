// The Food page's figures (docs/plans/messages.md, Phase 3; city-food-<realm>.jpg): what the
// granaries hold of each food, what was grown and eaten of it last month (the history's counts,
// history::record_made and record_used), and how long it lasts without harvests at that rate;
// the food in the granaries over the last twelve months.

use serde::Serialize;

use super::history::{self, CityHistory};
use crate::{
    sim::{Simulation, resources::ResourceKind},
    world::World,
};

// A food that lasts fewer months than this without harvests is short (the page's red).
pub const FOOD_SHORT_MONTHS: f32 = 3.0;

// The chart's months: a year of them.
pub const TREND_MONTHS: usize = 12;

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct FoodRow {
    pub good: &'static str, // Its name in data: "Rice".
    pub name: String,       // As the player reads it.
    pub stored: u32,        // In the granaries.
    pub grown: u32,
    pub eaten: u32,
    pub change: i64,               // Grown less eaten.
    pub lasts_months: Option<f32>, // Stored over eaten a month; None while none is eaten.
    pub short: bool,               // Lasts fewer than FOOD_SHORT_MONTHS.
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct FoodReport {
    pub stored: u32,
    pub grown: u32,
    pub eaten: u32,
    // How long the granaries' food lasts at what was eaten (or, before anything was, at what the
    // houses eat a month at their levels); None while nobody eats.
    pub lasts_months: Option<f32>,
    // Which month grown and eaten count: "last_month" (whole), "last_month_part" (a game loaded
    // partway through it) or "this_month" (so far, before a month is over).
    pub period: &'static str,
    pub rows: Vec<FoodRow>, // Each food stored, grown or eaten, in the goods' order.
    pub trend: Vec<f32>,    // The granaries' food at each month's start, then now.
    pub trend_months: Vec<u32>,
}

// Reads the page's figures; `food_a_month` is what the houses eat a month at their levels
// (CityReport's estimate), for how long the food lasts before anything was eaten.
pub fn read(world: &World, sim: &Simulation, food_a_month: f32) -> FoodReport {
    let stats = world.stats();
    let history = CityHistory::get();
    let month = history::current_month(sim);
    let (flow, whole) = history.last_month_flow(month);
    let period = match whole {
        Some(true) => "last_month",
        Some(false) => "last_month_part",
        None => "this_month",
    };

    let mut stored_of = [0u32; crate::sim::resources::RESOURCE_KIND_COUNT];
    stats.resources.granaries.for_each(|_, item| stored_of[item.kind.index()] += item.count);

    let mut rows = Vec::new();
    for kind in ResourceKind::foods().iter() {
        let (stored, grown, eaten) = (stored_of[kind.index()], flow.made(kind), flow.used(kind));
        if stored == 0 && grown == 0 && eaten == 0 {
            continue;
        }
        let lasts_months = (eaten > 0).then(|| stored as f32 / eaten as f32);
        rows.push(FoodRow {
            good: kind.name(),
            name: kind.display_name().to_string(),
            stored,
            grown,
            eaten,
            change: i64::from(grown) - i64::from(eaten),
            lasts_months,
            short: lasts_months.is_some_and(|months| months < FOOD_SHORT_MONTHS),
        });
    }

    let stored: u32 = rows.iter().map(|row| row.stored).sum();
    let grown: u32 = rows.iter().map(|row| row.grown).sum();
    let eaten: u32 = rows.iter().map(|row| row.eaten).sum();
    let a_month = if eaten > 0 { eaten as f32 } else { food_a_month };
    let lasts_months = (a_month > 0.0).then(|| stored as f32 / a_month);

    FoodReport {
        stored,
        grown,
        eaten,
        lasts_months,
        period,
        rows,
        trend: history.granary_food_trend(TREND_MONTHS, month, stored),
        trend_months: history.trend_months(TREND_MONTHS, month),
    }
}
