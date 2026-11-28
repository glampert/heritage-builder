// Defence commands: the raids to come, on the map and logged, the walls, and units' health
// (sim::defence, docs/plans/defence.md).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, ObjectId, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    config::GameConfigs,
    sim::defence::{RaidFrom, RaidLosses, RaidRecord, RaidUnderWay, ScheduledRaid, WallCondition},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "defence.state",
        help: "The game's raids: whether they come at random (a custom game's Raids switch) and when the next \
               does (an ordinal month, and months from now); the raids scheduled (id, kind, from: a side or a \
               cell, size, enemy, arrives and warn_from as ordinal months, months_left, warnings_sent), those \
               under way and the log of those that ended (their losses: walkers and militia killed, residents \
               lost, buildings and houses destroyed, goods and gold taken; repelled, palace_lost), how many the \
               city survived and repelled, and the walls: their count, how many are damaged, and each piece \
               (cell, tier, condition, strength, damaged, repaired: the ordinal month a builder last mended it).",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "defence.set",
        help: "Turn a custom game's random raids on or off (the next drawn afresh), move a scheduled raid to \
               arrive in N months (its warning as far ahead as before), set a unit's health (0 to its max), or a \
               wall piece's condition (0 to its strength; 0 breaches it: rubble in its place). Reports \
               defence.state, with the unit's health and max if one was given.",
        params: &[
            ParamDef { name: "random_raids", kind: "bool", required: false, help: "Whether raids come at random." },
            ParamDef { name: "raid", kind: "string", required: false, help: "A scheduled raid's id. Needs months." },
            ParamDef { name: "months", kind: "int", required: false, help: "Months from now it arrives in, 0 or more." },
            ParamDef { name: "unit", kind: "string", required: false, help: "A unit's id. Needs health." },
            ParamDef { name: "health", kind: "int", required: false, help: "Its health, 0 to its max." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "A wall piece's cell. Needs condition." },
            ParamDef { name: "condition", kind: "number", required: false, help: "Its condition, 0 to its strength." },
        ],
        run: cmd_set,
    },
];

fn now_month(context: &CommandContext) -> u32 {
    context.session.sim().calendar().ordinal_month()
}

fn from_json(from: RaidFrom) -> Value {
    match from {
        RaidFrom::Side(side) => json!(side.name()),
        RaidFrom::Cell([x, y]) => json!([x, y]),
    }
}

fn losses_json(losses: &RaidLosses) -> Value {
    let goods: serde_json::Map<String, Value> = crate::sim::resources::ResourceKind::all()
        .iter()
        .filter(|good| losses.goods_taken[good.index()] != 0)
        .map(|good| (good.name().to_string(), json!(losses.goods_taken[good.index()])))
        .collect();
    json!({
        "walkers_killed": losses.walkers_killed,
        "militia_killed": losses.militia_killed,
        "residents_lost": losses.residents_lost,
        "buildings_destroyed": losses.buildings_destroyed,
        "houses_destroyed": losses.houses_destroyed,
        "goods_taken": goods,
        "gold_taken": losses.gold_taken,
    })
}

fn scheduled_json(raid: &ScheduledRaid, now: u32) -> Value {
    json!({
        "id": raid.id,
        "kind": raid.kind.name(),
        "from": from_json(raid.from),
        "size": raid.size,
        "enemy": raid.enemy,
        "arrives": raid.arrives,
        "warn_from": raid.warn_from,
        "months_left": raid.months_left(now),
        "warnings_sent": raid.warnings_sent,
    })
}

fn under_way_json(raid: &RaidUnderWay) -> Value {
    json!({
        "id": raid.id,
        "kind": raid.kind.name(),
        "entry": cell_json(raid.entry),
        "size": raid.size,
        "arrived": raid.arrived,
        "spawned": raid.spawned,
        "raiders": raid.raiders.iter().map(|id| ObjectId::Unit(*id).to_json()).collect::<Vec<_>>(),
        "fallen": raid.fallen,
        "losses": losses_json(&raid.losses),
    })
}

fn record_json(raid: &RaidRecord) -> Value {
    json!({
        "id": raid.id,
        "kind": raid.kind.name(),
        "size": raid.size,
        "arrived": raid.arrived,
        "ended": raid.ended,
        "fallen": raid.fallen,
        "losses": losses_json(&raid.losses),
        "repelled": raid.repelled(),
        "palace_lost": raid.palace_lost,
    })
}

fn wall_json(wall: &WallCondition) -> Value {
    let rules = &GameConfigs::get().sim.defence;
    json!({
        "cell": cell_json(wall.cell),
        "tier": wall.tier.name(),
        "condition": wall.condition,
        "strength": wall.strength(rules),
        "damaged": wall.is_damaged(rules),
        "repaired": wall.repaired,
    })
}

fn state_json(context: &CommandContext) -> Value {
    let now = now_month(context);
    let defence = context.session.sim().defence();
    let rules = &GameConfigs::get().sim.defence;
    json!({
        "now": now,
        "random_raids": defence.random_raids(),
        "next_random_raid": defence.next_random_raid(),
        "months_to_next_random_raid": defence.next_random_raid().map(|month| month.saturating_sub(now)),
        "scheduled": defence.scheduled().iter().map(|raid| scheduled_json(raid, now)).collect::<Vec<_>>(),
        "under_way": defence.under_way().iter().map(under_way_json).collect::<Vec<_>>(),
        "log": defence.log().iter().map(record_json).collect::<Vec<_>>(),
        "survived": defence.survived(),
        "repelled": defence.repelled(),
        "walls": {
            "count": defence.walls().len(),
            "damaged": defence.walls().iter().filter(|wall| wall.is_damaged(rules)).count(),
            "pieces": defence.walls().iter().map(wall_json).collect::<Vec<_>>(),
        },
    })
}

fn cmd_state(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {}

    let _: Params = parse_params(params)?;
    context.require_map()?;
    done(state_json(context))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        random_raids: Option<bool>,
        raid: Option<String>,
        months: Option<u32>,
        unit: Option<String>,
        health: Option<u16>,
        cell: Option<CellParam>,
        condition: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    if params.raid.is_some() != params.months.is_some() {
        return Err(CommandError::invalid_params("raid and months go together."));
    }
    if let Some(id) = &params.raid
        && !context.session.sim().defence().scheduled().iter().any(|raid| raid.id == *id)
    {
        return Err(CommandError::not_found(format!("No raid '{id}' is scheduled.")));
    }
    if params.unit.is_some() != params.health.is_some() {
        return Err(CommandError::invalid_params("unit and health go together."));
    }
    let unit = match &params.unit {
        Some(id) => {
            let ObjectId::Unit(unit_id) = ObjectId::parse(id)? else {
                return Err(CommandError::invalid_params(format!("'{id}' isn't a unit's id.")));
            };
            let Some(unit) = context.session.world().find_unit(unit_id) else {
                return Err(CommandError::not_found(format!("No unit with id '{id}'.")));
            };
            let health = params.health.unwrap_or_default();
            if health > unit.max_health() {
                return Err(CommandError::invalid_params(format!(
                    "health must be from 0 to {}, got {health}.",
                    unit.max_health()
                )));
            }
            Some((unit_id, health))
        }
        None => None,
    };
    if params.cell.is_some() != params.condition.is_some() {
        return Err(CommandError::invalid_params("cell and condition go together."));
    }
    let wall = match (&params.cell, params.condition) {
        (Some(cell), Some(condition)) => {
            let cell = cell.0;
            context.require_cell_in_map(cell)?;
            let Some(wall) = context.session.sim().defence().wall_at(cell) else {
                return Err(CommandError::not_found(format!("No wall piece at {cell}.")));
            };
            let strength = wall.strength(&GameConfigs::get().sim.defence);
            if !(0.0..=strength).contains(&condition) {
                return Err(CommandError::invalid_params(format!(
                    "condition must be from 0 to {strength}, got {condition}."
                )));
            }
            Some((cell, condition))
        }
        _ => None,
    };

    let now = now_month(context);
    let rules = &GameConfigs::get().sim.defence;
    if let Some(on) = params.random_raids {
        context.session.sim_mut().set_random_raids(on, now, rules);
    }
    if let (Some(id), Some(months)) = (&params.raid, params.months) {
        context.session.sim_mut().defence_mut().set_raid_arrival(id, now + months);
    }
    if let Some((cell, condition)) = wall {
        let delta_time_secs = context.engine.frame_clock().delta_time();
        context.session.with_sim_context(delta_time_secs, |sim_context| {
            crate::defence::set_wall_condition(sim_context, cell, condition);
        });
    }
    let mut reply = state_json(context);
    if let Some((unit_id, health)) = unit {
        let unit = context.session.world_mut().find_unit_mut(unit_id).expect("found above");
        unit.set_health(health);
        reply["unit"] = json!({ "health": unit.health(), "max_health": unit.max_health() });
    }
    done(reply)
}
