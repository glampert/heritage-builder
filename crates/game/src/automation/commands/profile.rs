// Profile commands (docs/plans/realm-ui.md, Realm and progress): read and edit the player's
// profile, e.g. to reach a campaign or set the last save before a scenario looks at the menus.

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use super::validate_save_name;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    campaign::config::CampaignConfigs,
    profile,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "profile.state",
        help: "The player's profile as JSON (profile: campaigns reached, missions finished, last save, Custom Game \
               realms), the UI realm and why (ui_realm, as ui.realm gives it), whether changes are written to the \
               file this run (writes_file: false under --fresh-profile, in scripts and replays), and the file.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "profile.set",
        help: "Edit the profile: each field given replaces the profile's. Written to the file unless this run keeps \
               the profile in memory. Returns the state, as profile.state does.",
        params: &[
            ParamDef {
                name: "reached",
                kind: "string[]",
                required: false,
                help: "Ids of the campaigns reached (campaigns.json), e.g. [\"dragon_legacy\"]. The others are no \
                       longer reached.",
            },
            ParamDef {
                name: "campaigns",
                kind: "array",
                required: false,
                help: "Every campaign's record, as profile.state lists them: {id, reached, current_mission, \
                       missions_finished, distinctions, finished}. Fields left out take their defaults.",
            },
            ParamDef {
                name: "last_save",
                kind: "string|null",
                required: false,
                help: "The save Continue loads: a name in saves/, without its extension. null clears it.",
            },
            ParamDef {
                name: "custom_realms",
                kind: "string[]",
                required: false,
                help: "The realms Custom Game offers: dailong, kambara, tampadesa.",
            },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "profile.reset",
        help: "Start a fresh profile: nothing reached, no last save. Written to the file unless this run keeps the \
               profile in memory.",
        params: &[],
        run: cmd_reset,
    },
];

fn state_json(context: &CommandContext) -> Value {
    json!({
        "profile": profile::get().to_value(),
        "ui_realm": super::ui::ui_realm_json(context),
        "writes_file": profile::writes_file(),
        "file": profile::file_path(),
    })
}

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    done(state_json(context))
}

// A field that's there, even as null: Some(Value::Null). Left out: None.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        reached: Option<Vec<String>>,
        campaigns: Option<Vec<Value>>,
        #[serde(default, deserialize_with = "present")]
        last_save: Option<Value>,
        custom_realms: Option<Vec<String>>,
    }

    let params: Params = parse_params(params)?;
    let configs = CampaignConfigs::get();
    let mut edited = profile::get().clone();

    if let Some(records) = params.campaigns {
        edited.set_campaign_records(configs, records).map_err(CommandError::invalid_params)?;
    }
    if let Some(reached) = params.reached {
        edited.set_reached(configs, &reached).map_err(CommandError::invalid_params)?;
    }

    match params.last_save {
        None => {}
        Some(Value::Null) => edited.last_save = None,
        Some(Value::String(name)) => {
            validate_save_name(&name)?;
            edited.last_save = Some(name);
        }
        Some(other) => return Err(CommandError::invalid_params(format!("last_save is a save name or null, not {other}."))),
    }

    if let Some(names) = params.custom_realms {
        edited.set_custom_realms(&names).map_err(CommandError::invalid_params)?;
    }

    profile::replace(edited);
    done(state_json(context))
}

fn cmd_reset(context: &mut CommandContext, _params: Value) -> CommandResult {
    profile::reset();
    done(state_json(context))
}
