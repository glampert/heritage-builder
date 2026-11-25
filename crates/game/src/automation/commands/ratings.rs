// Ratings commands: the four ratings and what drives them, recomputed or set by hand, and the
// failure clocks (sim::ratings, crate::ratings, docs/plans/ratings.md); and the mission's goals.

use std::str::FromStr;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use strum::IntoEnumIterator;

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    campaign,
    menu::dialog,
    ratings::{self, MAX_RATING, RatingKind},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "ratings.state",
        help: "The four ratings: each one's value, name in the game's realm, the mission's goal for it (null without \
               one) and drivers (id, name, value, in the order its breakdown lists them, and by id in \
               `driver_values`); the month they were read in; which \
               failures can end the game, the Legitimacy clock, the failure, and the distinctions out of reach.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "ratings.recompute",
        help: "Read the four ratings from the city now, as a month's start does, then report ratings.state.",
        params: &[],
        run: cmd_recompute,
    },
    CommandDef {
        name: "ratings.set",
        help: "Set a rating's value by hand, its drivers left as they were, until the next recompute (a month's \
               start or ratings.recompute); then report ratings.state.",
        params: &[
            ParamDef {
                name: "rating",
                kind: "string",
                required: true,
                help: "prosperity, culture, security or legitimacy.",
            },
            ParamDef { name: "value", kind: "int", required: true, help: "0 to 100." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "goals.state",
        help: "The goals of the mission being played (docs/plans/ratings.md, §3), read against the world stats \
               recounted now: each one's `def` (as campaigns.json writes it), its line as the player reads it in the \
               game's realm (`goal`), `target`, `achieved` and `met`; whether they're `all_met` and the mission \
               `completed`; its time limit (`time_limit_months`, `months_elapsed`, `months_left`, null without one) \
               and the game's `failure`. Outside a mission: `in_mission` false and no goals.",
        params: &[],
        run: cmd_goals_state,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(state_json(context))
}

fn cmd_recompute(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    context.session.rate_city();
    done(state_json(context))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        rating: String,
        value: u32,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let kind = parse_rating(&params.rating)?;
    if params.value > MAX_RATING {
        return Err(CommandError::invalid_params(format!("A rating is from 0 to {MAX_RATING}, got {}.", params.value)));
    }
    context.session.sim_mut().ratings_mut().set_value(kind, params.value);
    done(state_json(context))
}

fn cmd_goals_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    context.session.refresh_world_stats();
    let sim = context.session.sim();
    let realm = sim.realm();
    let stats = context.session.world().stats();
    let months_elapsed = sim.calendar().months_elapsed();
    let active = campaign::active_mission();
    let requirements = campaign::active_requirements();
    let goals: Vec<Value> = requirements
        .map(|requirements| {
            requirements
                .goals
                .iter()
                .map(|goal| {
                    let (text, target, achieved) = dialog::goal_line(goal, stats, realm);
                    json!({
                        "def": goal,
                        "goal": text,
                        "target": target,
                        "achieved": achieved,
                        "met": goal.is_met(stats),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    done(json!({
        "in_mission": active.is_some(),
        "campaign": active.as_ref().map(|active| active.campaign_id),
        "mission": active.as_ref().map(|active| active.mission_index),
        "goals": goals,
        "all_met": requirements.is_some_and(|requirements| requirements.all_met(stats)),
        "completed": active.as_ref().is_some_and(|active| active.completed),
        "time_limit_months": requirements.and_then(|requirements| requirements.time_limit_months),
        "months_elapsed": months_elapsed,
        "months_left": requirements.and_then(|requirements| requirements.months_left(months_elapsed)),
        "failure": sim.ratings().failure().map(|reason| reason.to_string()),
    }))
}

pub(super) fn parse_rating(name: &str) -> Result<RatingKind, CommandError> {
    RatingKind::from_str(name).map_err(|_| {
        let names: Vec<String> = RatingKind::iter().map(|kind| kind.to_string()).collect();
        CommandError::invalid_params(format!("Unknown rating '{name}': use one of {}.", names.join(", ")))
    })
}

fn state_json(context: &CommandContext) -> Value {
    let sim = context.session.sim();
    let realm = sim.realm();
    let state = sim.ratings();

    let mut ratings_json = Map::new();
    for kind in RatingKind::iter() {
        let rating = state.get(kind);
        let drivers: Vec<Value> = rating
            .drivers
            .iter()
            .map(|driver| json!({ "id": driver.id, "name": ratings::driver_name(realm, &driver.id), "value": driver.value }))
            .collect();
        let driver_values: Map<String, Value> =
            rating.drivers.iter().map(|driver| (driver.id.clone(), json!(driver.value))).collect();
        ratings_json.insert(
            kind.to_string(),
            json!({
                "value": rating.value,
                "name": ratings::rating_name(realm, kind),
                "goal": ratings::mission_goal(kind),
                "drivers": drivers,
                "driver_values": driver_values,
            }),
        );
    }

    let fails_on = state.fails_on();
    json!({
        "realm": realm.to_string(),
        "rated_month": state.rated_month(),
        "ratings": ratings_json,
        "fails_on": { "debt": fails_on.debt, "legitimacy": fails_on.legitimacy },
        "legitimacy_low_months": state.legitimacy_low_months(),
        "failure": state.failure().map(|reason| reason.to_string()),
        "spoiled": state.spoiled().iter().collect::<Vec<_>>(),
    })
}
