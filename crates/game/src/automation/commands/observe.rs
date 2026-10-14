// Observation commands: screenshots, the log, frame stats and the console itself.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

use engine::{
    cli,
    log,
    screenshot::{ScreenshotOutput, ScreenshotRequest},
};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params, pending},
    debug::log_viewer::LogViewer,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "screenshot",
        help: "Save a screenshot of this frame as a PNG.",
        params: &[
            ParamDef {
                name: "path",
                kind: "string",
                required: false,
                help: "PNG file, relative to the directory the game was started from. Default: a new file in screenshots/.",
            },
            ParamDef { name: "include_ui", kind: "bool", required: false, help: "False leaves the UI out. Default true." },
        ],
        run: cmd_screenshot,
    },
    CommandDef {
        name: "logs.tail",
        help: "Recent log records, oldest first. Pass the last seq you saw as since to get only newer ones.",
        params: &[
            ParamDef { name: "since", kind: "int", required: false, help: "Only records with a larger seq. Default 0." },
            ParamDef { name: "min_level", kind: "string", required: false, help: "Verbose, Info (default), Warning or Error." },
            ParamDef { name: "channel", kind: "string", required: false, help: "Only this channel, e.g. session." },
            ParamDef { name: "limit", kind: "int", required: false, help: "Most records to return, the newest. Default 100." },
        ],
        run: cmd_logs_tail,
    },
    CommandDef {
        name: "debug.stats",
        help: "Frame time breakdown, draw calls, triangles, tiles drawn and object counts.",
        params: &[],
        run: cmd_debug_stats,
    },
    CommandDef { name: "console.clear", help: "Clear the console and log view.", params: &[], run: cmd_console_clear },
    CommandDef {
        name: "debug.panic",
        help: "Crash the game on purpose, to test crash reports (logs/crash-*.txt) and the panic notification.",
        params: &[ParamDef { name: "message", kind: "string", required: false, help: "Panic message." }],
        run: cmd_debug_panic,
    },
];

fn cmd_screenshot(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: Option<PathBuf>,
        #[serde(default = "default_include_ui")]
        include_ui: bool,
    }

    fn default_include_ui() -> bool {
        true
    }

    let params: Params = parse_params(params)?;

    let path = match params.path {
        Some(path) => cli::resolve_launch_path(&path),
        None => context.engine.new_screenshot_path(),
    };

    let id = context
        .engine
        .request_screenshot(ScreenshotRequest { include_ui: params.include_ui, output: ScreenshotOutput::File(path) });

    pending(move |context| {
        let result = context.engine.take_screenshot_result(id)?;

        Some(result.map_err(CommandError::failed).map(|screenshot| {
            json!({
                "path": screenshot.path.map(|path| path.display().to_string()),
                "size": [screenshot.size.width, screenshot.size.height],
                "scale": [screenshot.scale.x, screenshot.scale.y],
            })
        }))
    })
}

// Channel names are stored as " [name]".
fn channel_name(channel: &log::Channel) -> &'static str {
    channel.name.trim().trim_start_matches('[').trim_end_matches(']')
}

fn level_param(level: &str) -> Result<log::Level, CommandError> {
    [log::Level::Verbose, log::Level::Info, log::Level::Warning, log::Level::Error]
        .into_iter()
        .find(|known| known.to_string().eq_ignore_ascii_case(level))
        .ok_or_else(|| CommandError::invalid_params(format!("min_level must be Verbose, Info, Warning or Error, got '{level}'.")))
}

fn cmd_logs_tail(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        #[serde(default)]
        since: u64,
        min_level: Option<String>,
        channel: Option<String>,
        limit: Option<usize>,
    }

    let params: Params = parse_params(params)?;
    let min_level = params.min_level.as_deref().map(level_param).transpose()?.unwrap_or(log::Level::Info);
    let limit = params.limit.unwrap_or(100);

    let records = log::recent_records(params.since);

    // Clients poll with since = last_seq, so it counts filtered-out records too.
    let last_seq = records.last().map_or(params.since, |record| record.seq);

    let matching: Vec<Value> = records
        .iter()
        .filter(|record| record.level >= min_level)
        .filter(|record| {
            params.channel.as_deref().is_none_or(|wanted| record.channel.as_ref().is_some_and(|channel| channel_name(channel) == wanted))
        })
        .map(|record| {
            json!({
                "seq": record.seq,
                "level": record.level.to_string(),
                "channel": record.channel.as_ref().map(channel_name),
                "message": record.message,
            })
        })
        .collect();

    let skip = matching.len().saturating_sub(limit);
    let records: Vec<Value> = matching.into_iter().skip(skip).collect();

    done(json!({ "records": records, "last_seq": last_seq }))
}

fn cmd_debug_stats(context: &mut CommandContext, _params: Value) -> CommandResult {
    let stats = context.stats;
    let render = context.engine.render_stats();

    let mut reply = json!({
        "frame": context.frame(),
        "fps": stats.fps,
        "frame_ms": {
            "total": stats.total_frame_time_ms,
            "sim": stats.sim_frame_time_ms,
            "anim": stats.anim_frame_time_ms,
            "sound": stats.sound_frame_time_ms,
            "draw_world": stats.draw_world_frame_time_ms,
            "ui_begin": stats.ui_begin_frame_time_ms,
            "ui_end": stats.ui_end_frame_time_ms,
            "engine_begin": stats.engine_begin_frame_time_ms,
            "engine_end": stats.engine_end_frame_time_ms,
            "present": stats.present_frame_time_ms,
        },
        "render": {
            "draw_calls": render.draw_calls,
            "triangles": render.triangles_drawn,
            "lines": render.lines_drawn,
            "points": render.points_drawn,
            "texture_changes": render.texture_changes,
            "submit_ms": render.render_submit_time_ms,
        },
    });

    if context.require_map().is_ok() {
        let tiles = context.session.tile_map_render_stats();
        let world = context.session.world();

        reply["tiles"] = json!({ "drawn": tiles.tiles_drawn, "sort_list": tiles.tile_sort_list_len });
        reply["objects"] = json!({
            "buildings": world.buildings_stats().0,
            "units": world.units_stats().0,
            "props": world.prop_stats().0,
        });
    }

    done(reply)
}

fn cmd_console_clear(_context: &mut CommandContext, _params: Value) -> CommandResult {
    LogViewer::clear();
    done(Value::Null)
}

fn cmd_debug_panic(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        message: Option<String>,
    }

    let params: Params = parse_params(params)?;
    panic!("{}", params.message.as_deref().unwrap_or("Panic requested by debug.panic."));
}
