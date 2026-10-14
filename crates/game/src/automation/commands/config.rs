// Config commands: read GameConfigs as JSON and change settings for this run.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, config_patches, done, parse_params};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "config.get",
        help: "The game configs (configs.json) as JSON, or the value at a JSON pointer.",
        params: &[ParamDef { name: "path", kind: "string", required: false, help: "JSON pointer, e.g. /sim/starting_gold_units." }],
        run: cmd_get,
    },
    CommandDef {
        name: "config.set",
        help: "Change a config for this run only; configs.json keeps its value. Many settings are only read at startup.",
        params: &[
            ParamDef { name: "path", kind: "string", required: true, help: "JSON pointer, e.g. /debug/show_popups." },
            ParamDef { name: "value", kind: "any", required: true, help: "New value, of the setting's type." },
        ],
        run: cmd_set,
    },
];

fn pointer_param(path: &str) -> Result<String, CommandError> {
    if path.is_empty() || path == "/" {
        return Err(CommandError::invalid_params("Give the path of one setting or group, e.g. /sim/random_seed."));
    }
    Ok(if path.starts_with('/') { path.to_string() } else { format!("/{path}") })
}

fn cmd_get(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: Option<String>,
    }

    let params: Params = parse_params(params)?;

    let pointer = match params.path.as_deref() {
        None | Some("") | Some("/") => String::new(),
        Some(path) => pointer_param(path)?,
    };

    let value = config_patches::get(&pointer)
        .ok_or_else(|| CommandError::not_found(format!("No config at '{pointer}'. config.get with no path lists them all.")))?;

    done(value)
}

fn cmd_set(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: String,
        value: Value,
    }

    let params: Params = parse_params(params)?;
    let pointer = pointer_param(&params.path)?;

    let (previous, current) = config_patches::set(&pointer, params.value).map_err(|err| {
        if err.starts_with("No config") { CommandError::not_found(err) } else { CommandError::invalid_params(err) }
    })?;

    done(json!({ "path": pointer, "previous": previous, "value": current }))
}
