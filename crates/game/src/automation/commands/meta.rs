// Meta commands: discovery, liveness and quitting.

use serde::Deserialize;
use serde_json::{Value, json};

use engine::platform;

use crate::automation::{
    CommandContext,
    CommandDef,
    CommandError,
    CommandResult,
    ErrorCode,
    ParamDef,
    all_commands,
    done,
    find_command,
    parse_params,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "help",
        help: "List the commands, or describe one command and its parameters.",
        params: &[ParamDef { name: "command", kind: "string", required: false, help: "Command to describe." }],
        run: cmd_help,
    },
    CommandDef { name: "ping", help: "Check that the game responds. Returns the frame number.", params: &[], run: cmd_ping },
    CommandDef { name: "version", help: "Game version, build profile and backends.", params: &[], run: cmd_version },
    CommandDef {
        name: "quit",
        help: "Quit the game after this frame.",
        params: &[ParamDef { name: "exit_code", kind: "int", required: false, help: "Process exit status (desktop). Default 0." }],
        run: cmd_quit,
    },
];

fn cmd_help(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        command: Option<String>,
    }

    let params: Params = parse_params(params)?;

    let Some(name) = params.command else {
        let commands: Vec<Value> =
            all_commands().map(|command| json!({ "name": command.name, "help": command.help })).collect();
        return done(json!({ "commands": commands }));
    };

    let command = find_command(&name)
        .ok_or_else(|| CommandError::new(ErrorCode::UnknownCommand, format!("Unknown command '{name}'.")))?;

    let params: Vec<Value> = command
        .params
        .iter()
        .map(|param| json!({ "name": param.name, "type": param.kind, "required": param.required, "help": param.help }))
        .collect();

    done(json!({ "name": command.name, "help": command.help, "params": params }))
}

fn cmd_ping(context: &mut CommandContext, _params: Value) -> CommandResult {
    done(json!({ "pong": true, "frame": context.frame() }))
}

fn cmd_version(context: &mut CommandContext, _params: Value) -> CommandResult {
    done(json!({
        "version": crate::version(),
        "build": platform::build_profile().to_string(),
        "app_api": context.engine.app().app_api().to_string(),
        "render_api": context.engine.render_system().render_api().to_string(),
    }))
}

fn cmd_quit(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        #[serde(default)]
        exit_code: i32,
    }

    let params: Params = parse_params(params)?;
    context.engine.request_exit(params.exit_code);
    done(Value::Null)
}
