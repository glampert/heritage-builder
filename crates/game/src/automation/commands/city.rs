// The City book's figures (crate::city, docs/plans/realm-ui.md, Phase 6): the report its pages
// read and the history they draw their trends from.

use serde_json::{Value, json};

use super::refresh_world_stats;
use crate::{
    automation::{CommandContext, CommandDef, CommandResult, done},
    city::{CityHistory, CityReport},
};

pub(super) const COMMANDS: &[CommandDef] = &[CommandDef {
    name: "city.state",
    help: "What the City book shows of the city: population and workforce (jobs_not_filled, not_working_age), housing by \
           level (houses, residents, next_needs), goods by group (stored, room, change since the month began), \
           storage_yards and granaries (buildings, stored, capacity), food (stored, a month, months), balance, \
           net_this_year, taxes_owed, warnings; and history: a sample at the start of each month kept (ordinal_month, \
           population, employed, balance). A new or loaded game starts the history at its month.",
    params: &[],
    run: cmd_state,
}];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    refresh_world_stats(context);
    // This month's sample, if the game hasn't run a frame in it yet (after a fast forward).
    crate::city::history::sample_if_new_month(context.session.world(), context.session.sim());
    let report = CityReport::read(context.session.world(), context.session.sim());
    let history: Vec<_> = CityHistory::get().months().collect();
    let mut value = serde_json::to_value(&report).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("history".to_string(), json!(history));
    }
    done(value)
}
