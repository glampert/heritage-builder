// The City book's history: a sample of the city each game month, for its trend lines (the
// population's, the treasury's) and its monthly changes (the goods'). Runtime only: a new or
// loaded game starts it again from its current month (docs/plans/realm-ui.md, Phase 6: until a
// milestone needs history in saves).
//
// GameLoop samples it after the simulation's step, once in each month (sample_if_new_month), and
// clears it when the session changes.

use std::collections::VecDeque;

use common::mem::SingleThreadStatic;

use crate::{
    sim::{Simulation, resources::RESOURCE_KIND_COUNT},
    world::World,
};

// The months kept: a year of trend, and the month before it for the first one's change.
pub const MONTHS_KEPT: usize = 13;

// The city at the start of a month (the first frame that month that the game ran).
#[derive(Clone, Debug, serde::Serialize)]
pub struct MonthSample {
    pub ordinal_month: u32, // GameCalendar::ordinal_month: year × 12 + month − 1.
    pub population: u32,
    pub employed: u32,
    pub balance: i64,
    // Taken after the month began: the game's first, a new or loaded game's month. Changes "this
    // month" aren't counted from it.
    pub partial: bool,
    #[serde(skip)]
    pub stored: [u32; RESOURCE_KIND_COUNT], // Each good in the storage yards and granaries.
}

impl MonthSample {
    pub fn read(world: &World, sim: &Simulation) -> Self {
        let stats = world.stats();
        let mut stored = [0; RESOURCE_KIND_COUNT];
        stats.resources.storage_yards.for_each(|_, item| stored[item.kind.index()] += item.count);
        stats.resources.granaries.for_each(|_, item| stored[item.kind.index()] += item.count);
        Self {
            ordinal_month: current_month(sim),
            population: stats.population.total,
            employed: stats.population.employed,
            balance: sim.finance().balance(),
            partial: false,
            stored,
        }
    }
}

#[derive(Default)]
pub struct CityHistory {
    months: VecDeque<MonthSample>, // Oldest first.
}

static HISTORY: SingleThreadStatic<CityHistory> = SingleThreadStatic::new(CityHistory { months: VecDeque::new() });

impl CityHistory {
    pub fn get() -> &'static CityHistory {
        HISTORY.as_ref()
    }

    // The months sampled, oldest first: at most MONTHS_KEPT.
    pub fn months(&self) -> impl ExactSizeIterator<Item = &MonthSample> + DoubleEndedIterator {
        self.months.iter()
    }

    // The sample at the start of `month` (the current month: history::current_month), if it was
    // taken then (not one taken partway through the month).
    pub fn sample(&self, month: u32) -> Option<&MonthSample> {
        self.months.iter().rev().find(|sample| sample.ordinal_month == month && !sample.partial)
    }

    // The population over the last `count` months, oldest first: each month before `month` (the
    // current one) at its start, then `now`.
    pub fn population_trend(&self, count: usize, month: u32, now: u32) -> Vec<f32> {
        self.trend(count, month, |sample| sample.population as f32, now as f32)
    }

    // The treasury's balance over the last `count` months, as population_trend.
    pub fn balance_trend(&self, count: usize, month: u32, now: i64) -> Vec<f32> {
        self.trend(count, month, |sample| sample.balance as f32, now as f32)
    }

    // A value for each of the last months (`count` points in all): each month sampled before
    // `month` at its start, and `month` as it is now, `now`: the trend lines' points. Shorter
    // while the game is young.
    fn trend(&self, count: usize, month: u32, value: impl Fn(&MonthSample) -> f32, now: f32) -> Vec<f32> {
        let past: Vec<f32> = self.months.iter().filter(|sample| sample.ordinal_month < month).map(value).collect();
        let skip = past.len().saturating_sub(count.saturating_sub(1));
        let mut points: Vec<f32> = past.into_iter().skip(skip).collect();
        points.push(now);
        points
    }

    // The month numbers (1 to 12) of a trend's points, as population_trend gives them.
    pub fn trend_months(&self, count: usize, month: u32) -> Vec<u32> {
        let past: Vec<u32> = self.months.iter().filter(|sample| sample.ordinal_month < month).map(|sample| sample.ordinal_month % 12 + 1).collect();
        let skip = past.len().saturating_sub(count.saturating_sub(1));
        let mut months: Vec<u32> = past.into_iter().skip(skip).collect();
        months.push(month % 12 + 1);
        months
    }
}

// Records the city at the start of a new month: the first call in each month, after the
// simulation's step (GameLoop). The first call in a game records its current month.
pub fn sample_if_new_month(world: &World, sim: &Simulation) {
    let history = HISTORY.as_mut();
    let month = current_month(sim);
    if history.months.back().is_some_and(|last| last.ordinal_month == month) {
        return;
    }
    // A date set back (calendar.set) starts the history again.
    if history.months.back().is_some_and(|last| last.ordinal_month > month) {
        history.months.clear();
    }
    // The first sample of a game is taken when it starts or loads, partway through its month.
    let mut sample = MonthSample::read(world, sim);
    sample.partial = history.months.is_empty();
    history.months.push_back(sample);
    while history.months.len() > MONTHS_KEPT {
        history.months.pop_front();
    }
}

// The game's month: year × 12 + month − 1, as the calendar's date gives it.
pub fn current_month(sim: &Simulation) -> u32 {
    let configs = &crate::config::GameConfigs::get().sim;
    sim.calendar().date(configs.days_per_month, configs.seconds_per_month).ordinal_month()
}

// Empties the history: a new game, a load, the home menu.
pub fn clear() {
    HISTORY.as_mut().months.clear();
}
