// A mission's distinctions (docs/plans/ratings.md, §5; GDD 7): one or two optional goals, each
// adding a line to the campaign's stele once earned (MissionDistinctionDef, its condition a
// DistinctionCondition).
//
// The ones that hold when the mission's goals are met are earned: campaign::tick reads them then
// (earned) and queues them with the finished mission, and the profile adds them to what an earlier
// play earned (Profile::mission_finished_with), so a replay can earn the rest. A goal is read only
// at that moment. The other forms (never in debt, never a breach, no house lost, never short of a
// good, within so many months) are kept from the mission's start: checked at each month's start,
// once the ratings are recomputed and the failure checked (ratings::on_new_month), and spoiled for
// good once broken (CityRatings::spoiled, saved, so a loaded game remembers); they're checked once
// more as the goals are met.

use engine::log;
use strum::Display;

use super::{CityRatings, failure::MissionTerms};
use crate::{
    campaign::config::{DistinctionCondition, MissionDistinctionDef},
    sim::{SimContext, SimContextMut},
};

// Where a distinction stands in the mission being played.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display)]
#[strum(serialize_all = "snake_case")]
pub enum DistinctionState {
    Open,    // The mission goes on, and it can still be earned.
    Spoiled, // The mission goes on, and it's out of reach.
    Earned,  // The mission is finished, and it was earned (in this play or an earlier one).
    Missed,  // The mission is finished, and it wasn't.
}

// Where `distinction` stands: `completed`, the mission is finished, with `earned` the distinctions
// earned (the profile's, with this play's); else the ones spoiled so far.
pub fn state(
    distinction: &MissionDistinctionDef,
    ratings: &CityRatings,
    completed: bool,
    earned: &[String],
) -> DistinctionState {
    if completed {
        if earned.contains(&distinction.id) { DistinctionState::Earned } else { DistinctionState::Missed }
    } else if ratings.is_spoiled(&distinction.id) {
        DistinctionState::Spoiled
    } else {
        DistinctionState::Open
    }
}

// The calendar's ordinal month the mission started in (its calendar restarts then).
pub fn mission_start_month(context: &SimContext) -> u32 {
    let calendar = context.calendar();
    calendar.ordinal_month().saturating_sub(calendar.months_elapsed())
}

// Whether `condition` is broken for good: a month's end in debt (the debt's clock is running), a
// breach or a house lost since the mission's start, the good out of storage while people live in
// the city, its months gone. A goal never is.
pub fn is_broken(condition: &DistinctionCondition, context: &SimContext) -> bool {
    let since = mission_start_month(context);
    match condition {
        DistinctionCondition::Goal(_) => false,
        DistinctionCondition::NoDebt => context.finance().months_in_debt() > 0,
        DistinctionCondition::NeverBreached => {
            context.floods().log().iter().any(|record| record.ordinal_month >= since && record.breaches > 0)
        }
        DistinctionCondition::NoHouseLost => {
            context.risks().disasters().iter().any(|month| month.ordinal_month >= since && month.houses_lost > 0)
        }
        DistinctionCondition::NeverShortOf { good } => {
            let stats = context.world().stats();
            stats.population.total > 0 && stats.resources.in_storage(*good) == 0
        }
        DistinctionCondition::Within { months } => context.calendar().months_elapsed() >= *months,
    }
}

// Whether `distinction` is earned now, as the mission's goals are met: not spoiled nor broken, and
// its goal met (the world stats as the goals read them).
pub fn is_earned(distinction: &MissionDistinctionDef, context: &SimContext) -> bool {
    if context.ratings().is_spoiled(&distinction.id) || is_broken(&distinction.condition, context) {
        return false;
    }
    match &distinction.condition {
        DistinctionCondition::Goal(goal) => goal.is_met(context.world().stats()),
        _ => true,
    }
}

// The ids of `distinctions` earned now, as the mission's goals are met (campaign::tick).
pub fn earned(context: &SimContext, distinctions: &[MissionDistinctionDef]) -> Vec<String> {
    distinctions
        .iter()
        .filter(|distinction| is_earned(distinction, context))
        .map(|distinction| distinction.id.clone())
        .collect()
}

// A month's start: spoils the mission's distinctions broken now (kept on CityRatings), and returns
// their ids. Nothing outside a mission (`terms` None), once it's finished, or once it's lost.
pub fn check_spoiled(
    context: &SimContextMut,
    distinctions: &[MissionDistinctionDef],
    terms: Option<&MissionTerms>,
) -> Vec<String> {
    let Some(terms) = terms else {
        return Vec::new();
    };
    if terms.completed || context.ratings().failure().is_some() {
        return Vec::new();
    }
    let mut spoiled = Vec::new();
    for distinction in distinctions {
        if !distinction.condition.can_spoil()
            || context.ratings().is_spoiled(&distinction.id)
            || !is_broken(&distinction.condition, context)
        {
            continue;
        }
        context.ratings_mut().spoil(&distinction.id);
        log::info!(log::channel!("ratings"), "The distinction '{}' is out of reach.", distinction.id);
        spoiled.push(distinction.id.clone());
    }
    spoiled
}
