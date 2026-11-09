// City risks commands: each building's fire and collapse risk, and whether they build up at all
// (building::risks, sim::risks, docs/plans/city-risks.md).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::{
        Building,
        risks::{MAX_RISK, RISK_HIGH, RiskKind},
    },
    cheats,
    config::GameConfigs,
};

// How many of the riskiest buildings risks.state lists.
const RISKIEST_LISTED: usize = 20;

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "risks.state",
        help: "Whether fire and collapse risks build up, how many buildings are at high risk (75 or more), the \
               highest risks, and the riskiest buildings with their risks (0 to 100).",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "risks.set",
        help: "Turn risks on or off for this game, or set a building's fire and collapse risk (0 to 100). With a \
               cell, reports that building's risks; otherwise risks.state.",
        params: &[
            ParamDef { name: "enabled", kind: "bool", required: false, help: "Whether risks build up in this game." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y]: any cell of the building." },
            ParamDef { name: "fire", kind: "number", required: false, help: "Fire risk, 0 to 100. Needs cell." },
            ParamDef { name: "collapse", kind: "number", required: false, help: "Collapse risk, 0 to 100. Needs cell." },
        ],
        run: cmd_set,
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
        enabled: Option<bool>,
        cell: Option<CellParam>,
        fire: Option<f32>,
        collapse: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    for (name, value) in [("fire", params.fire), ("collapse", params.collapse)] {
        if let Some(value) = value {
            if !(0.0..=MAX_RISK).contains(&value) {
                return Err(CommandError::invalid_params(format!("{name} must be from 0 to {MAX_RISK}, got {value}.")));
            }
            if params.cell.is_none() {
                return Err(CommandError::invalid_params(format!("{name} needs the cell of a building.")));
            }
        }
    }
    let target = match params.cell {
        Some(CellParam(cell)) => {
            context.require_cell_in_map(cell)?;
            let session = &*context.session;
            let building = session
                .world()
                .find_building_for_cell(cell, session.tile_map())
                .ok_or_else(|| CommandError::not_found(format!("No building at [{}, {}].", cell.x, cell.y)))?;
            Some(building.kind_and_id())
        }
        None => None,
    };

    if let Some(enabled) = params.enabled {
        context.session.sim_mut().risks_mut().set_enabled(enabled);
    }

    let Some(target) = target else {
        return done(state_json(context));
    };

    let building = context.session.world_mut().find_building_mut(target.kind, target.id).unwrap();
    if let Some(fire) = params.fire {
        building.risks_mut().set(RiskKind::Fire, fire);
    }
    if let Some(collapse) = params.collapse {
        building.risks_mut().set(RiskKind::Collapse, collapse);
    }

    let mut reply = json!({ "name": building.name(), "cell": cell_json(building.base_cell()) });
    reply["risks"] = building_risks_json(building);
    done(reply)
}

// Risks are fractional; two decimals are plenty to read and compare.
fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

// A building's risks, for world.building_at and risks.set: the two risks, whether a well reaches
// it, the rates they build up at now (points per month), and the fire's state (Phase 1).
pub(super) fn building_risks_json(building: &Building) -> Value {
    let risks = building.risks();
    let rates = building.risk_rates();
    json!({
        "fire": round(risks.get(RiskKind::Fire)),
        "collapse": round(risks.get(RiskKind::Collapse)),
        "near_water": risks.is_near_water(),
        "burning": risks.burning().map_or(Value::Null, |burning| json!({ "secs_left": round(burning.secs_left) })),
        "rates": { "fire": round(rates.fire), "collapse": round(rates.collapse) },
    })
}

fn state_json(context: &CommandContext) -> Value {
    let session = &*context.session;
    let city = session.sim().risks();
    let configs = &GameConfigs::get().sim.risks;

    let mut buildings = 0;
    let (mut high_fire, mut high_collapse, mut burning) = (0, 0, 0);
    let (mut max_fire, mut max_collapse) = (0.0_f32, 0.0_f32);
    // (highest risk, cell, entry) per building, sorted below.
    let mut riskiest: Vec<(f32, (i32, i32), Value)> = Vec::new();

    session.world().for_each_building_of_any_kind(|building| {
        // Sites and buildings whose configs give them no risks don't count.
        if building.is_under_construction() || building.configs().risk_rates().is_zero() {
            return true;
        }
        let risks = building.risks();
        let (fire, collapse) = (risks.get(RiskKind::Fire), risks.get(RiskKind::Collapse));
        buildings += 1;
        high_fire += usize::from(fire >= RISK_HIGH);
        high_collapse += usize::from(collapse >= RISK_HIGH);
        burning += usize::from(risks.is_burning());
        max_fire = max_fire.max(fire);
        max_collapse = max_collapse.max(collapse);

        let cell = building.base_cell();
        riskiest.push((risks.highest(), (cell.y, cell.x), json!({
            "name": building.name(),
            "kind": building.kind().to_string(),
            "cell": cell_json(cell),
            "fire": round(fire),
            "collapse": round(collapse),
            "near_water": risks.is_near_water(),
        })));
        true
    });

    // Highest risk first; ties by cell, so the list reads the same on every run.
    riskiest.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let riskiest: Vec<Value> = riskiest.into_iter().take(RISKIEST_LISTED).map(|(_, _, entry)| entry).collect();

    json!({
        "enabled": city.is_enabled(),
        "active": city.is_active(),
        "no_city_risks_cheat": cheats::get().no_city_risks,
        "near_water_fire_scale": configs.near_water_fire_scale,
        "rate_jitter": configs.rate_jitter,
        "buildings": buildings,
        "high": { "fire": high_fire, "collapse": high_collapse },
        "burning": burning,
        "highest": { "fire": round(max_fire), "collapse": round(max_collapse) },
        "riskiest": riskiest,
    })
}
