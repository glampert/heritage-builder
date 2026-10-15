// Simulation and cheats commands.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    cheats::{self, CheatsLookup},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "sim.state",
        help: "Paused, speed, simulated time, treasury and population.",
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
