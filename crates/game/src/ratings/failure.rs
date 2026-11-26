// How a mission is lost (docs/plans/ratings.md, §4; GDD 7): its debt runs out its months
// (Finance::is_debt_overdue), Legitimacy stays near zero for a year (the court recalls the
// governor: RatingRules::recall_below for recall_months month-ends in a row), or its time limit
// (MissionRequirements::time_limit_months) runs out with the goals unmet. A mission's `failure`
// block turns the first two off (CityRatings::fails_on); a mission already finished can't be lost.
// A custom game with its Failure switch on (docs/plans/ratings.md, §7) is lost the same ways, its
// goal set's time limit included, until its goals are met; with it off, it can't be lost.
//
// Checked at each month's start, once the ratings are recomputed (ratings::on_new_month). The
// failure is kept on CityRatings, so a save of a lost game loads lost; campaign::request_failure_screen
// opens the failure screen (MissionFailed). Warnings come first, as notices the message log posts
// (GameLoop::notify_ratings): when Legitimacy falls below the mark, again in the clock's last three
// months, and when 12, 6, 3 and 1 months of the time limit are left. The debt's own warnings are
// Finance's.

use common::mem::SingleThreadStatic;
use engine::ui::{
    sound::UiSoundKey,
    text::{self, UiTextCategory},
};

use super::{FailureReason, RatingKind, rating_name};
use crate::{
    campaign,
    config::GameConfigs,
    messages::{Message, MessageKind, MessageTarget},
    realm::Realm,
    sim::SimContextMut,
};

// The months left of a time limit that are warned of.
pub const TIME_WARNINGS: [u32; 4] = [12, 6, 3, 1];

// The Legitimacy clock's last months, warned of each month.
const LEGITIMACY_LAST_MONTHS: u32 = 3;

// ----------------------------------------------
// The mission's terms
// ----------------------------------------------

// What the failures read of the mission being played.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MissionTerms {
    pub completed: bool, // Its goals were met: it can't be lost any more.
    pub time_limit_months: Option<u32>,
    pub goals_met: bool,
}

// The active campaign mission's terms, or a custom game's with its Failure switch on; None for any
// other game, where nothing is lost.
pub fn mission_terms(context: &SimContextMut) -> Option<MissionTerms> {
    let stats = context.world().stats();
    if let Some(active) = campaign::active_mission() {
        let requirements = campaign::active_requirements()?;
        return Some(MissionTerms {
            completed: active.completed,
            time_limit_months: requirements.time_limit_months,
            goals_met: requirements.all_met(stats),
        });
    }
    let game = crate::custom_game::active().filter(|game| game.record.failure)?;
    let requirements = game.requirements();
    Some(MissionTerms {
        completed: game.record.completed,
        time_limit_months: requirements.and_then(|requirements| requirements.time_limit_months),
        goals_met: requirements.is_some_and(|requirements| requirements.all_met(stats)),
    })
}

// ----------------------------------------------
// The check
// ----------------------------------------------

// A month's start, the ratings just recomputed: moves the Legitimacy clock, and loses the game if a
// failure is due (kept on CityRatings, and returned), else queues the warnings due. Nothing happens
// without terms, once the mission is finished, or once it's lost.
pub fn check_failure(context: &SimContextMut, terms: Option<&MissionTerms>) -> Option<FailureReason> {
    let terms = terms?;
    if terms.completed || context.ratings().failure().is_some() {
        return None;
    }
    let configs = GameConfigs::get();
    let rules = &configs.sim.ratings;
    let fails_on = context.ratings().fails_on();

    // The Legitimacy clock: month-ends in a row below the mark.
    let low = context.ratings().value(RatingKind::Legitimacy) < rules.recall_below;
    let low_months = if fails_on.legitimacy && low { context.ratings().legitimacy_low_months() + 1 } else { 0 };
    context.ratings_mut().set_legitimacy_low_months(low_months);

    let months_elapsed = context.calendar().months_elapsed();
    let time_left = terms.time_limit_months.map(|limit| limit.saturating_sub(months_elapsed));
    let reason = if fails_on.debt && context.finance().is_debt_overdue(&configs.sim.finance) {
        Some(FailureReason::Debt)
    } else if fails_on.legitimacy && low_months >= rules.recall_months {
        Some(FailureReason::Legitimacy)
    } else if time_left == Some(0) && !terms.goals_met {
        Some(FailureReason::Time)
    } else {
        None
    };
    if let Some(reason) = reason {
        context.ratings_mut().set_failure(Some(reason));
        return Some(reason);
    }

    // The warnings.
    if low_months > 0 {
        let months_left = rules.recall_months.saturating_sub(low_months);
        if low_months == 1 {
            notify(FailureNotice::LegitimacyLow { months_left, started: true });
        } else if months_left <= LEGITIMACY_LAST_MONTHS {
            notify(FailureNotice::LegitimacyLow { months_left, started: false });
        }
    }
    if let Some(left) = time_left
        && !terms.goals_met
        && TIME_WARNINGS.contains(&left)
    {
        notify(FailureNotice::TimeLeft { months_left: left });
    }
    None
}

// ----------------------------------------------
// Notices
// ----------------------------------------------

// A warning that the mission may be lost.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FailureNotice {
    // Legitimacy is below the mark: the clock started (`started`), or is in its last months.
    LegitimacyLow { months_left: u32, started: bool },
    // The time limit's months left.
    TimeLeft { months_left: u32 },
}

static NOTICES: SingleThreadStatic<Vec<FailureNotice>> = SingleThreadStatic::new(Vec::new());

fn notify(notice: FailureNotice) {
    NOTICES.as_mut().push(notice);
}

// The warnings since the last call, for the message log.
pub fn take_notices() -> Vec<FailureNotice> {
    std::mem::take(NOTICES.as_mut())
}

// A warning's message, in `realm`'s words: a Warning with the Alert sound.
pub fn notice_message(notice: &FailureNotice, realm: Realm) -> (Message, UiSoundKey) {
    let months = |count: u32| text::format_count(UiTextCategory::Messages, "failure.months", i64::from(count), &[]);
    let rules = &GameConfigs::get().sim.ratings;
    let text = match notice {
        FailureNotice::LegitimacyLow { months_left, started } => {
            let key = if *started { "failure.warning.legitimacy_low" } else { "failure.warning.legitimacy_left" };
            text::format(UiTextCategory::Messages, key, &[
                ("rating", &rating_name(realm, RatingKind::Legitimacy)),
                ("below", &rules.recall_below.to_string()),
                ("months", &months(*months_left)),
            ])
        }
        FailureNotice::TimeLeft { months_left } => {
            text::format(UiTextCategory::Messages, "failure.warning.time", &[("months", &months(*months_left))])
        }
    };
    // Legitimacy's on the Ratings page; the time limit's on the Overview, with the goals.
    let page = match notice {
        FailureNotice::LegitimacyLow { .. } => "ratings",
        FailureNotice::TimeLeft { .. } => "overview",
    };
    (Message::new(MessageKind::Warning, text).with_target(MessageTarget::CityPage(page)), UiSoundKey::Alert)
}
