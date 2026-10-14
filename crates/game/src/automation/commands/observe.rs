// Observation commands: screenshots and the console itself.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

use engine::{
    cli,
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
    CommandDef { name: "console.clear", help: "Clear the console and log view.", params: &[], run: cmd_console_clear },
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
            })
        }))
    })
}

fn cmd_console_clear(_context: &mut CommandContext, _params: Value) -> CommandResult {
    LogViewer::clear();
    done(Value::Null)
}
