// Time commands: the frame time step, fast-forwarding, waiting, and holding the simulation still.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use common::time::Seconds;

use crate::automation::{
    CommandContext,
    CommandDef,
    CommandError,
    CommandResult,
    ErrorCode,
    ParamDef,
    done,
    parse_params,
    pending,
    query::{Check, CheckResult, CompareOp},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "time.fixed_dt",
        help: "Advance every frame by a fixed time step, or by the measured frame time again.",
        params: &[ParamDef { name: "dt", kind: "number", required: false, help: "Seconds per frame. Omit for real time." }],
        run: cmd_fixed_dt,
    },
    CommandDef {
        name: "time.fast_forward",
        help: "Run the simulation ahead without drawing, in fixed steps. Long runs continue over several frames.",
        params: &[
            ParamDef { name: "sim_secs", kind: "number", required: true, help: "Simulated seconds to run, as sim_time counts them." },
            ParamDef { name: "step", kind: "number", required: false, help: "Frame time per step, in seconds. Default 1/60." },
            ParamDef { name: "resume", kind: "bool", required: false, help: "Resume a paused simulation first. Default false." },
        ],
        run: cmd_fast_forward,
    },
    CommandDef {
        name: "wait.frames",
        help: "Finish after this many frames.",
        params: &[ParamDef { name: "frames", kind: "int", required: true, help: "Frames to wait." }],
        run: cmd_wait_frames,
    },
    CommandDef {
        name: "wait.sim_seconds",
        help: "Finish once the simulation has run this many more seconds.",
        params: &[ParamDef { name: "secs", kind: "number", required: true, help: "Simulated seconds to wait." }],
        run: cmd_wait_sim_seconds,
    },
    CommandDef {
        name: "wait.until",
        help: "Run a query every frame until a value in its result passes a comparison.",
        params: &[
            ParamDef { name: "query", kind: "string", required: true, help: "Command to run, e.g. session.state." },
            ParamDef { name: "args", kind: "object", required: false, help: "The query's parameters." },
            ParamDef { name: "path", kind: "string", required: false, help: "JSON pointer into the result, e.g. /population." },
            ParamDef { name: "op", kind: "string", required: true, help: "eq, ne, gt, ge, lt, le or exists." },
            ParamDef { name: "value", kind: "any", required: false, help: "Value to compare with. Not used by exists." },
            ParamDef { name: "timeout_frames", kind: "int", required: false, help: "Fail after this many frames. Default 3600." },
        ],
        run: cmd_wait_until,
    },
    CommandDef {
        name: "frame.hold",
        help: "Hold the simulation and animations still while rendering goes on, or let them run again.",
        params: &[ParamDef { name: "hold", kind: "bool", required: false, help: "False lets them run. Default true." }],
        run: cmd_frame_hold,
    },
    CommandDef {
        name: "frame.step",
        help: "Let the simulation run for some frames, then hold it (see frame.hold).",
        params: &[ParamDef { name: "frames", kind: "int", required: false, help: "Frames to run. Default 1." }],
        run: cmd_frame_step,
    },
];

// Simulation steps time.fast_forward runs per frame. A count, not a time budget, so a run splits
// into frames the same way every time.
const FAST_FORWARD_STEPS_PER_FRAME: u64 = 600;
const MAX_FAST_FORWARD_SECS: f64 = 24.0 * 60.0 * 60.0;
const DEFAULT_FAST_FORWARD_STEP: Seconds = 1.0 / 60.0;

const DEFAULT_WAIT_UNTIL_TIMEOUT_FRAMES: u64 = 3600;

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

fn cmd_fast_forward(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        sim_secs: f64,
        step: Option<Seconds>,
        #[serde(default)]
        resume: bool,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    if !(params.sim_secs > 0.0 && params.sim_secs <= MAX_FAST_FORWARD_SECS) {
        return Err(CommandError::invalid_params(format!(
            "sim_secs must be more than 0 and at most {MAX_FAST_FORWARD_SECS}, got {}.",
            params.sim_secs
        )));
    }

    let step = params.step.unwrap_or(DEFAULT_FAST_FORWARD_STEP);
    if !(step > 0.0 && step <= 1.0) {
        return Err(CommandError::invalid_params(format!("step must be more than 0 and at most 1 second, got {step}.")));
    }

    if context.session.sim().is_paused() {
        if !params.resume {
            return Err(CommandError::failed("The simulation is paused. Resume it, or pass resume=true."));
        }
        context.session.sim_mut().resume();
    }

    // Each step advances the simulation by the step times the speed, as a frame does.
    let sim_secs_per_step = f64::from(step * context.session.sim().speed());
    let total_steps = (params.sim_secs / sim_secs_per_step).ceil().max(1.0) as u64;

    let start_sim_time = context.session.sim().elapsed_secs();
    let start_frame = context.frame();
    let mut steps_done = 0;

    pending(move |context| {
        if context.session.sim().is_paused() {
            return Some(Err(CommandError::failed(format!("The simulation was paused after {steps_done} of {total_steps} steps."))));
        }

        let steps = FAST_FORWARD_STEPS_PER_FRAME.min(total_steps - steps_done);
        for _ in 0..steps {
            context.session.update_simulation(context.engine, step);
        }
        steps_done += steps;

        (steps_done == total_steps).then(|| {
            let sim_time = context.session.sim().elapsed_secs();
            Ok(json!({
                "sim_secs": sim_time - start_sim_time,
                "sim_time": sim_time,
                "steps": total_steps,
                "frames": context.frame() - start_frame + 1,
            }))
        })
    })
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

fn cmd_wait_sim_seconds(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        secs: f64,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    if !(params.secs >= 0.0 && params.secs.is_finite()) {
        return Err(CommandError::invalid_params(format!("secs must be zero or more, got {}.", params.secs)));
    }
    if context.session.sim().is_paused() {
        return Err(CommandError::failed("The simulation is paused, so the wait would never end. Resume it first."));
    }
    if context.frame_control.is_holding() {
        return Err(CommandError::failed("The simulation is held (frame.hold). Use frame.step or time.fast_forward."));
    }

    let target_sim_time = context.session.sim().elapsed_secs() + params.secs;

    pending(move |context| {
        let sim_time = context.session.sim().elapsed_secs();
        (sim_time >= target_sim_time).then(|| Ok(json!({ "sim_time": sim_time, "frame": context.frame() })))
    })
}

fn cmd_wait_until(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        query: String,
        args: Option<Map<String, Value>>,
        #[serde(default)]
        path: String,
        op: CompareOp,
        value: Option<Value>,
        timeout_frames: Option<u64>,
    }

    let params: Params = parse_params(params)?;

    let op = params.op;
    let check = Check { query: params.query, args: params.args, path: params.path, value: params.value };
    check.validate(op)?;

    let start_frame = context.frame();
    let deadline_frame = start_frame.saturating_add(params.timeout_frames.unwrap_or(DEFAULT_WAIT_UNTIL_TIMEOUT_FRAMES));

    pending(move |context| match check.run(context, op) {
        Err(err) => Some(Err(err)),
        Ok(CheckResult::Passed(actual)) => Some(Ok(json!({ "actual": actual, "frames": context.frame() - start_frame }))),
        Ok(CheckResult::NotYet(reason)) => (context.frame() >= deadline_frame).then(|| {
            let frames = context.frame() - start_frame;
            Err(CommandError::new(ErrorCode::Timeout, format!("Gave up after {frames} frames. {reason}")))
        }),
    })
}

fn cmd_frame_hold(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        hold: Option<bool>,
    }

    let params: Params = parse_params(params)?;
    let hold = params.hold.unwrap_or(true);

    context.frame_control.set_hold(hold);
    done(json!({ "held": hold, "frame": context.frame() }))
}

fn cmd_frame_step(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        frames: Option<u64>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let frames = params.frames.unwrap_or(1);
    if !(1..=1_000_000).contains(&frames) {
        return Err(CommandError::invalid_params(format!("frames must be from 1 to 1000000, got {frames}.")));
    }

    // The simulation updates after this dispatch point, so it runs on this frame and the next
    // frames - 1, and the step is over when frame `target_frame` starts.
    context.frame_control.step(frames);
    let target_frame = context.frame() + frames;

    pending(move |context| {
        (context.frame() >= target_frame).then(|| {
            Ok(json!({ "frame": context.frame(), "sim_time": context.session.sim().elapsed_secs(), "held": true }))
        })
    })
}
