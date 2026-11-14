// The HUD's message log (crate::messages, docs/plans/realm-ui.md, P5d): what it holds, and a message
// added by hand for tests.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    messages::{self, MessageKind},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "messages.state",
        help: "The message log's messages, newest first (kind: construction, warning or finance; text), and count. \
               A new or loaded game starts it empty.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "messages.push",
        help: "Adds a message to the log, the newest, as the game's notices do (no sound).",
        params: &[
            ParamDef { name: "kind", kind: "string", required: true, help: "construction, warning or finance." },
            ParamDef { name: "text", kind: "string", required: true, help: "The message's text." },
        ],
        run: cmd_push,
    },
];

fn cmd_state(_context: &mut CommandContext, _params: Value) -> CommandResult {
    let latest = messages::latest(messages::CAPACITY);
    done(json!({ "count": latest.len(), "messages": latest }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PushParams {
    kind: String,
    text: String,
}

fn cmd_push(_context: &mut CommandContext, params: Value) -> CommandResult {
    let params: PushParams = parse_params(params)?;
    let kind = match params.kind.as_str() {
        "construction" => MessageKind::Construction,
        "warning" => MessageKind::Warning,
        "finance" => MessageKind::Finance,
        other => return Err(CommandError::invalid_params(format!("Unknown kind '{other}': construction, warning or finance."))),
    };
    messages::push(kind, params.text);
    done(json!({ "count": messages::latest(messages::CAPACITY).len() }))
}
