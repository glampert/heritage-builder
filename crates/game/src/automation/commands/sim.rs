// Simulation and cheats commands.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    cheats::{self, CheatsLookup},
    config::GameConfigs,
    sim::{GameCalendar, calendar::CalendarDate},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "sim.state",
        help: "Paused, speed, simulated time, date, treasury and population. Wait for a date with wait.until on \
               /date/ordinal_month (year * 12 + month - 1).",
        params: &[],
        run: cmd_state,
    },
    CommandDef { name: "sim.pause", help: "Pause the simulation.", params: &[], run: cmd_pause },
    CommandDef { name: "sim.resume", help: "Resume the simulation.", params: &[], run: cmd_resume },
    CommandDef {
        name: "sim.speed",
        help: "Get or set the simulation speed.",
        params: &[ParamDef { name: "speed", kind: "number", required: false, help: "New speed, from 1 to 10." }],
        run: cmd_speed,
    },
    CommandDef {
        name: "calendar.set",
        help: "Move the calendar to a date, without the new month, season or year events. Not before the calendar's start.",
        params: &[
            ParamDef { name: "year", kind: "int", required: true, help: "World Year." },
            ParamDef { name: "month", kind: "int", required: true, help: "1 to 12." },
            ParamDef { name: "day", kind: "int", required: false, help: "1 to days_per_month. Default 1." },
        ],
        run: cmd_calendar_set,
    },
    CommandDef { name: "cheats.list", help: "List the cheats and whether each is on.", params: &[], run: cmd_cheats_list },
    CommandDef {
        name: "cheats.set",
        help: "Turn a cheat on or off.",
        params: &[
            ParamDef { name: "name", kind: "string", required: true, help: "Cheat name, see cheats.list." },
            ParamDef { name: "value", kind: "bool", required: true, help: "True turns it on." },
        ],
        run: cmd_cheats_set,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    super::refresh_world_stats(context);

    let sim = context.session.sim();
    let stats = context.session.world().stats();

    done(json!({
        "paused": sim.is_paused(),
        "speed": sim.speed(),
        "sim_time": sim.elapsed_secs(),
        "gold": stats.treasury.gold_units_total,
        "population": stats.population.total,
        "employed": stats.population.employed,
        "houses": stats.houses.total,
        "date": date_json(context.session.sim().calendar()),
        "frame": context.frame(),
        "frame_held": context.frame_control.is_holding(),
    }))
}

fn cmd_pause(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    context.session.sim_mut().pause();
    done(json!({ "paused": true }))
}

fn cmd_resume(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    context.session.sim_mut().resume();
    done(json!({ "paused": false }))
}

fn cmd_speed(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        speed: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    if let Some(speed) = params.speed {
        let (min_speed, max_speed) = context.session.sim().speed_limits();
        if !(min_speed..=max_speed).contains(&speed) {
            return Err(CommandError::invalid_params(format!("Speed must be from {min_speed} to {max_speed}, got {speed}.")));
        }
        context.session.sim_mut().set_speed(speed);
    }

    done(json!({ "speed": context.session.sim().speed() }))
}

// The calendar as sim.state and calendar.set report it.
pub(super) fn date_json(calendar: &GameCalendar) -> Value {
    let configs = &GameConfigs::get().sim;
    let date = calendar.date(configs.days_per_month, configs.seconds_per_month);
    let era = calendar.era();

    json!({
        "year": date.year,
        "month": date.month,
        "day": date.day,
        "season": calendar.season().to_string(),
        "era": era.map(|(name, _)| name),
        "era_year": era.map(|(_, year)| year),
        "year_name": calendar.year_name().to_string(),
        "ordinal_month": date.ordinal_month(),
        "months_elapsed": calendar.months_elapsed(),
        "game_secs": calendar.elapsed_secs(configs.seconds_per_month),
    })
}

fn cmd_calendar_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        year: u32,
        month: u32,
        #[serde(default = "first_day")]
        day: u32,
    }

    fn first_day() -> u32 {
        1
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let configs = &GameConfigs::get().sim;
    let date = CalendarDate { year: params.year, month: params.month, day: params.day };

    context
        .session
        .sim_mut()
        .calendar_mut()
        .set_date(date, configs.days_per_month, configs.seconds_per_month)
        .map_err(CommandError::invalid_params)?;

    done(date_json(context.session.sim().calendar()))
}

fn cmd_cheats_list(_context: &mut CommandContext, _params: Value) -> CommandResult {
    // serde_json maps keep their keys sorted, unlike the lookup's HashMap.
    let cheats: Map<String, Value> =
        cheats::get().lookup().values().map(|(name, enabled)| (name.to_string(), Value::Bool(**enabled))).collect();

    done(Value::Object(cheats))
}

fn cmd_cheats_set(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        name: String,
        value: bool,
    }

    let params: Params = parse_params(params)?;

    cheats::get_mut()
        .try_set_by_name(&params.name, params.value)
        .map_err(|_| CommandError::invalid_params(format!("No cheat named '{}'. See cheats.list.", params.name)))?;

    done(json!({ params.name: params.value }))
}
