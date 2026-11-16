// Faith commands: the game's patrons, their favour and mood, the festival year and what's been held
// (sim::faith, faith::config, docs/plans/faith.md).

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::refresh_world_stats;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::BuildingKind,
    config::GameConfigs,
    faith::{
        FaithConfigs,
        Mood,
        config,
        events,
        festivals::{self, PlanOutcome},
    },
    sim::faith::FaithNoticeKind,
    tile::{TileFlags, TileKind},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "faith.state",
        help: "The game's realm and its patrons (name, favour 0 to 100, mood, months since their last blessing or \
               misfortune), whether favour moves and festivals can be planned, the festival year (each festival's \
               month, patron, its costs and favour at each scale, and its state this year: planned, held, now, \
               to_come or missed), the festival planned, the festivals held (festivals_held, and the log), the \
               festival squares, how many can hold a festival now (staffed) and how many are dressed for one \
               this month (festive), the effects under way (patron, \
               event, months left) and what they do now (production_scale of farms, fishing wharves and mines; \
               upkeep_scale).",
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
    CommandDef {
        name: "faith.plan",
        help: "Plan a festival of the game's realm at a scale, in place of any other plan: it's held at the start \
               of its month if a festival square is staffed and the treasury, the storage yards and the granaries \
               hold its costs. In its own month, while it hasn't been held this year, it's held at once instead (or \
               not: `outcome` not_held and `why`). Reports the outcome (planned, held or not_held) and faith.state.",
        params: &[
            ParamDef { name: "festival", kind: "string", required: true, help: "A festival id of the game's realm." },
            ParamDef {
                name: "scale",
                kind: "string",
                required: false,
                help: "A scale id (modest, grand, imperial or royal). Default: the middle one.",
            },
        ],
        run: cmd_plan,
    },
    CommandDef {
        name: "faith.cancel",
        help: "Drop the festival planned, if any. Reports `cancelled` and faith.state.",
        params: &[],
        run: cmd_cancel,
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
        "event": notice.id,
        "blessing": notice.kind == FaithNoticeKind::Blessing,
        "message": notice.text,
    })));
    done(reply)
}

fn cmd_plan(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        festival: String,
        scale: Option<String>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let realm = context.session.sim().faith().realm();
    let scale = match params.scale {
        Some(scale) => scale,
        None => FaithConfigs::get()
            .for_realm(realm)
            .and_then(festivals::default_scale)
            .map(|scale| scale.id.clone())
            .ok_or_else(|| CommandError::failed(format!("{realm} has no festival scales.")))?,
    };
    let delta_time_secs = context.engine.frame_clock().delta_time();
    let outcome = context
        .session
        .with_sim_context(delta_time_secs, |sim_context| festivals::plan(sim_context, &params.festival, &scale))
        .map_err(CommandError::invalid_params)?;
    refresh_world_stats(context);

    let mut reply = state_json(context);
    match outcome {
        PlanOutcome::Planned => reply["outcome"] = json!("planned"),
        PlanOutcome::Held => reply["outcome"] = json!("held"),
        PlanOutcome::NotHeld(why) => {
            reply["outcome"] = json!("not_held");
            reply["why"] = json!(why.describe());
        }
    }
    done(reply)
}

fn cmd_cancel(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    let cancelled = festivals::cancel(context.session.sim_mut().faith_mut());
    let mut reply = state_json(context);
    reply["cancelled"] = json!(cancelled);
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

    // Each festival with its state this year, and what it costs and gives at each scale.
    let today = {
        let configs = &GameConfigs::get().sim;
        context.session.sim().calendar().date(configs.days_per_month, configs.seconds_per_month)
    };
    let festivals: Vec<Value> = def
        .map(|def| def.festivals.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|festival| {
            let scales: Map<String, Value> = def
                .map(|def| def.scales.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|scale| {
                    let terms = festivals::terms(festival, scale);
                    let mut goods = Map::new();
                    for item in &terms.goods {
                        goods.insert(item.kind.name().to_string(), json!(item.count));
                    }
                    (
                        scale.id.clone(),
                        json!({
                            "gold": terms.gold,
                            "goods": goods,
                            "favour": round(terms.favour),
                            "culture": round(terms.culture),
                            "legitimacy": round(terms.legitimacy),
                        }),
                    )
                })
                .collect();
            json!({
                "id": festival.id,
                "name": config::festival_name(realm, &festival.id),
                "month": festival.month,
                "patron": festival.patron,
                "state": festivals::state(faith, festival, today).to_string(),
                "scales": scales,
            })
        })
        .collect();
    // The festival squares, and how many are dressed for a festival this month.
    let (mut squares, mut festive) = (0, 0);
    let tile_map = context.session.tile_map();
    context.session.world().for_each_building(BuildingKind::FestivalSquare, |building| {
        squares += 1;
        if tile_map
            .find_tile(building.base_cell(), TileKind::Building)
            .is_some_and(|tile| tile.has_flags(TileFlags::Festive))
        {
            festive += 1;
        }
        true
    });

    json!({
        "realm": realm.to_string(),
        "favour_enabled": faith.is_favour_enabled(),
        "festivals_enabled": faith.is_festivals_enabled(),
        "patrons": patrons,
        "festivals": festivals,
        "plan": faith.plan().map(|plan| json!({ "festival": plan.festival, "scale": plan.scale })),
        "festivals_held": faith.festivals_held(),
        "log": faith.log().iter().map(|record| json!({
            "festival": record.festival,
            "scale": record.scale,
            "ordinal_month": record.ordinal_month,
            "culture": round(record.culture),
            "legitimacy": round(record.legitimacy),
        })).collect::<Vec<_>>(),
        "squares": {
            "built": squares,
            "staffed": festivals::venues(context.session.world()).len(),
            "festive": festive,
        },
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
