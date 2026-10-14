// Simulation, cheats and time commands.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use common::time::Seconds;

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params, pending},
    cheats::{self, CheatsLookup},
};

pub(super) const COMMANDS: &[CommandDef] = &[
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
    CommandDef {
        name: "time.fixed_dt",
        help: "Advance every frame by a fixed time step, or by the measured frame time again.",
        params: &[ParamDef { name: "dt", kind: "number", required: false, help: "Seconds per frame. Omit for real time." }],
        run: cmd_fixed_dt,
    },
    CommandDef {
        name: "wait.frames",
        help: "Finish after this many frames.",
        params: &[ParamDef { name: "frames", kind: "int", required: true, help: "Frames to wait." }],
        run: cmd_wait_frames,
    },
];

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

fn cmd_fixed_dt(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        dt: Option<Seconds>,
    }

    let params: Params = parse_params(params)?;

    if let Some(dt) = params.dt {
        if !(dt > 0.0 && dt <= 1.0) {
            return Err(CommandError::invalid_params(format!("dt must be more than 0 and at most 1 second, got {dt}.")));
        }
    }

    context.engine.set_fixed_delta_time(params.dt);
    done(json!({ "dt": params.dt }))
}

fn cmd_wait_frames(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        frames: u64,
    }

    let params: Params = parse_params(params)?;
    let target_frame = context.frame().saturating_add(params.frames);

    pending(move |context| (context.frame() >= target_frame).then(|| Ok(json!({ "frame": context.frame() }))))
}
