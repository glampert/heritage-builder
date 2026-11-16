// Faith commands: the game's patrons, their favour and mood, the festival year and what's been held
// (sim::faith, faith::config, docs/plans/faith.md).

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::refresh_world_stats;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::BuildingKind,
    faith::{FaithConfigs, Mood, config, events},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "faith.state",
        help: "The game's realm and its patrons (name, favour 0 to 100, mood, months since their last blessing or \
               misfortune), whether favour moves and festivals can be planned, the festival year (month, patron), \
               the festival planned, the festivals held, the effects under way (patron, event, months left) and \
               what they do now (production_scale of farms, fishing wharves and mines; upkeep_scale).",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "faith.set",
        help: "Set a patron's favour (0 to 100), or turn favour or festivals on or off for this game. Reports \
               faith.state.",
        params: &[
            ParamDef { name: "patron", kind: "string", required: false, help: "A patron id of the game's realm." },
            ParamDef { name: "favour", kind: "number", required: false, help: "0 to 100. Needs patron." },
            ParamDef {
                name: "favour_enabled",
                kind: "bool",
                required: false,
                help: "Whether favour moves and blessings and misfortunes come.",
            },
            ParamDef { name: "festivals_enabled", kind: "bool", required: false, help: "Whether festivals can be planned." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "faith.event",
        help: "Set off one of a patron's blessings or misfortunes now, whatever its favour, cooldown and the dice: \
               a harvest or blight, a windfall into the Offerings account, a fire, a collapse, a sickness or an \
               upkeep change. It starts the patron's cooldown and posts its message. Fails for an event whose \
               system the game doesn't have yet, or one with nothing to act on (no farms, risks off). Reports \
               the message and faith.state.",
        params: &[
            ParamDef { name: "patron", kind: "string", required: true, help: "A patron id of the game's realm." },
            ParamDef { name: "event", kind: "string", required: true, help: "One of its blessings or misfortunes, by id." },
        ],
        run: cmd_event,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(state_json(context))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        patron: Option<String>,
        favour: Option<f32>,
        favour_enabled: Option<bool>,
        festivals_enabled: Option<bool>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    let faith = context.session.sim().faith();
    if let Some(patron) = &params.patron
        && faith.patron(patron).is_none()
    {
        let ids: Vec<&String> = faith.patrons().keys().collect();
        return Err(CommandError::invalid_params(format!(
            "No patron '{patron}' in {}. Patrons: {}.",
            faith.realm(),
            ids.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(", ")
        )));
    }
    if let Some(favour) = params.favour {
        if !(0.0..=100.0).contains(&favour) {
            return Err(CommandError::invalid_params(format!("favour must be from 0 to 100, got {favour}.")));
        }
        if params.patron.is_none() {
            return Err(CommandError::invalid_params("favour needs a patron.".to_string()));
        }
    }

    let faith = context.session.sim_mut().faith_mut();
    if let (Some(patron), Some(favour)) = (&params.patron, params.favour) {
        faith.set_favour(patron, favour);
    }
    if let Some(enabled) = params.favour_enabled {
        faith.set_favour_enabled(enabled);
    }
    if let Some(enabled) = params.festivals_enabled {
        faith.set_festivals_enabled(enabled);
    }
    done(state_json(context))
}

fn cmd_event(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        patron: String,
        event: String,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context
        .session
        .with_sim_context(delta_time_secs, |sim_context| events::set_off_now(sim_context, &params.patron, &params.event))
        .map_err(CommandError::failed)?;
    refresh_world_stats(context);

    let notice = context.session.sim().faith().notices().last().cloned();
    let mut reply = state_json(context);
    reply["came"] = json!(notice.map(|notice| json!({
        "patron": notice.patron,
        "event": notice.event,
        "blessing": notice.blessing,
        "message": notice.text,
    })));
    done(reply)
}

fn state_json(context: &CommandContext) -> Value {
    let faith = context.session.sim().faith();
    let realm = faith.realm();
    let def = FaithConfigs::get().for_realm(realm);

    // In the file's order, which the rolls follow.
    let patrons: Vec<Value> = def
        .map(|def| def.patrons.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|patron| {
            let state = faith.patron(&patron.id)?;
            Some(json!({
                "id": patron.id,
                "name": config::patron_name(realm, &patron.id),
                "favour": round(state.favour),
                "mood": Mood::from_favour(state.favour).to_string(),
                "months_since_event": state.months_since_event,
            }))
        })
        .collect();

    let festivals: Vec<Value> = def
        .map(|def| def.festivals.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|festival| {
            let mut goods = Map::new();
            for (kind, count) in festival.goods() {
                goods.insert(kind.name().to_string(), json!(count));
            }
            json!({
                "id": festival.id,
                "month": festival.month,
                "patron": festival.patron,
                "gold": festival.gold,
                "goods": goods,
            })
        })
        .collect();

    json!({
        "realm": realm.to_string(),
        "favour_enabled": faith.is_favour_enabled(),
        "festivals_enabled": faith.is_festivals_enabled(),
        "patrons": patrons,
        "festivals": festivals,
        "plan": faith.plan().map(|plan| json!({ "festival": plan.festival, "scale": plan.scale })),
        "log": faith.log().iter().map(|record| json!({
            "festival": record.festival,
            "scale": record.scale,
            "ordinal_month": record.ordinal_month,
        })).collect::<Vec<_>>(),
        "effects": faith.effects().iter().map(|effect| json!({
            "patron": effect.patron,
            "event": effect.event,
            "months_left": effect.months_left,
        })).collect::<Vec<_>>(),
        // What the effects under way do now: farms', fishing wharves' and mines' production, and
        // the upkeep.
        "production_scale": {
            "Farm": round(faith.production_scale(BuildingKind::Farm)),
            "FishingWharf": round(faith.production_scale(BuildingKind::FishingWharf)),
            "Mine": round(faith.production_scale(BuildingKind::Mine)),
        },
        "upkeep_scale": round(faith.upkeep_scale()),
    })
}

// Favour is fractional; two decimals are plenty to read and compare.
fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}
