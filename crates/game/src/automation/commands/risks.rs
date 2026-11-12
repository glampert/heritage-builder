// City risks commands: each building's fire, collapse and disease risk, whether they build up at
// all, and fires, collapses and outbreaks on demand (building::risks, sim::risks,
// docs/plans/city-risks.md).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json, refresh_world_stats};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::{
        Building,
        BuildingKindAndId,
        destruction::{self, Destruction},
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
        help: "Whether fire, collapse and disease risks build up, how many buildings are at high risk (75 or more), \
               the highest risks, the riskiest buildings with their risks (0 to 100), the fires burning, the sick \
               houses, and how many residents have died of sickness.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "risks.set",
        help: "Turn risks, or disease alone, on or off for this game, or set a building's fire, collapse and \
               disease risk (0 to 100). With a cell, reports that building's risks; otherwise risks.state.",
        params: &[
            ParamDef { name: "enabled", kind: "bool", required: false, help: "Whether risks build up in this game." },
            ParamDef {
                name: "disease_enabled",
                kind: "bool",
                required: false,
                help: "Whether houses can fall sick in this game (while risks are on).",
            },
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y]: any cell of the building." },
            ParamDef { name: "fire", kind: "number", required: false, help: "Fire risk, 0 to 100. Needs cell." },
            ParamDef { name: "collapse", kind: "number", required: false, help: "Collapse risk, 0 to 100. Needs cell." },
            ParamDef { name: "disease", kind: "number", required: false, help: "Disease risk, 0 to 100. Needs cell." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "risks.infect",
        help: "Make a house sick, as an outbreak roll does. No one dies for death_grace_secs; then its residents \
               die a growing share at a time until a medic visits or no one is left, and it spreads disease risk \
               to the houses around it meanwhile; all only while risks and disease are on.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: any cell of the house." }],
        run: cmd_infect,
    },
    CommandDef {
        name: "risks.ignite",
        help: "Set a building on fire, as a fire risk at 100 does. It burns down after burn_secs unless a fire \
               watcher puts it out, and raises its neighbours' fire risk meanwhile; both only while risks are on.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: any cell of the building." }],
        run: cmd_ignite,
    },
    CommandDef {
        name: "risks.collapse",
        help: "Collapse a building now, as a collapse risk at 100 does: it leaves rubble on its cells, and what it \
               held is lost.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: any cell of the building." }],
        run: cmd_collapse,
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
        disease_enabled: Option<bool>,
        cell: Option<CellParam>,
        fire: Option<f32>,
        collapse: Option<f32>,
        disease: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    for (name, value) in [("fire", params.fire), ("collapse", params.collapse), ("disease", params.disease)] {
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
        Some(CellParam(cell)) => Some(building_at(context, cell)?),
        None => None,
    };

    if let Some(enabled) = params.enabled {
        context.session.sim_mut().risks_mut().set_enabled(enabled);
    }
    if let Some(disease_enabled) = params.disease_enabled {
        context.session.sim_mut().risks_mut().set_disease_enabled(disease_enabled);
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
    if let Some(disease) = params.disease {
        building.risks_mut().set(RiskKind::Disease, disease);
    }

    let mut reply = json!({ "name": building.name(), "cell": cell_json(building.base_cell()) });
    reply["risks"] = building_risks_json(building);
    done(reply)
}

fn cmd_ignite(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let target = building_at(context, params.cell.0)?;
    check_can(context, target, "catch fire", Building::can_catch_fire, "fire")?;

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        let building = sim_context.find_building_mut(target.kind, target.id).unwrap();
        building.ignite_now(sim_context);
    });

    let building = context.session.world().find_building(target.kind, target.id).unwrap();
    let mut reply = json!({ "name": building.name(), "cell": cell_json(building.base_cell()) });
    reply["risks"] = building_risks_json(building);
    done(reply)
}

fn cmd_infect(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let target = building_at(context, params.cell.0)?;
    check_can(context, target, "fall sick", Building::can_fall_sick, "disease")?;

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        let building = sim_context.find_building_mut(target.kind, target.id).unwrap();
        building.start_outbreak_now(sim_context);
    });

    let building = context.session.world().find_building(target.kind, target.id).unwrap();
    let mut reply = json!({ "name": building.name(), "cell": cell_json(building.base_cell()) });
    reply["risks"] = building_risks_json(building);
    done(reply)
}

fn cmd_collapse(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let target = building_at(context, params.cell.0)?;
    check_can(context, target, "collapse", Building::can_collapse, "collapse")?;

    let (name, footprint) = {
        let building = context.session.world().find_building(target.kind, target.id).unwrap();
        (building.name(), building.cell_range())
    };

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        destruction::destroy_building(sim_context, target, Destruction::Collapsed)
    });
    refresh_world_stats(context);

    done(json!({
        "name": name,
        "cell": cell_json(footprint.start),
        "rubble": [cell_json(footprint.start), cell_json(footprint.end)],
    }))
}

// The building covering `cell`.
fn building_at(context: &CommandContext, cell: common::coords::Cell) -> Result<BuildingKindAndId, CommandError> {
    context.require_cell_in_map(cell)?;
    let session = &*context.session;
    session
        .world()
        .find_building_for_cell(cell, session.tile_map())
        .map(Building::kind_and_id)
        .ok_or_else(|| CommandError::not_found(format!("No building at [{}, {}].", cell.x, cell.y)))
}

// Fails with the reason when the building can't `what` now (see Building::can_catch_fire and
// can_collapse); `risk` names the config rate it needs.
fn check_can(
    context: &CommandContext,
    target: BuildingKindAndId,
    what: &str,
    can: fn(&Building) -> bool,
    risk: &str,
) -> Result<(), CommandError> {
    let building = context.session.world().find_building(target.kind, target.id).unwrap();
    if can(building) {
        return Ok(());
    }
    let reason = if building.is_under_construction() {
        "it's a construction site".to_string()
    } else if building.is_burning() {
        "it's burning".to_string()
    } else if building.has_outbreak() && risk == "disease" {
        "it's sick already".to_string()
    } else if building.configs().risk_rates().get(risk_kind(risk)) <= 0.0 {
        format!("its config gives it no {risk} risk")
    } else if risk == "disease" && building.population_count() == 0 {
        "no one lives in it".to_string()
    } else {
        "it's being destroyed".to_string()
    };
    Err(CommandError::failed(format!("The {} can't {what}: {reason}.", building.name())))
}

fn risk_kind(risk: &str) -> RiskKind {
    match risk {
        "collapse" => RiskKind::Collapse,
        "disease" => RiskKind::Disease,
        _ => RiskKind::Fire,
    }
}

// Risks are fractional; two decimals are plenty to read and compare.
fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

// A building's risks, for world.building_at, risks.set, risks.ignite and risks.infect: the
// three risks, whether a well (and for a house, an apothecary and food) reaches it, the rates
// they build up at now (points per month), and its fire and outbreak, if any.
pub(super) fn building_risks_json(building: &Building) -> Value {
    let risks = building.risks();
    let rates = building.risk_rates();
    json!({
        "fire": round(risks.get(RiskKind::Fire)),
        "collapse": round(risks.get(RiskKind::Collapse)),
        "disease": round(risks.get(RiskKind::Disease)),
        "near_water": risks.is_near_water(),
        "near_apothecary": risks.is_near_apothecary(),
        "has_food": risks.has_food(),
        "burning": risks.burning().map_or(Value::Null, |burning| json!({ "secs_left": round(burning.secs_left) })),
        "outbreak": risks.outbreak().map_or(Value::Null, |outbreak| json!({
            "secs": round(outbreak.secs),
            "next_death_secs": round(outbreak.next_death_secs),
            "fraction": round(outbreak.fraction),
            "deaths": outbreak.deaths,
        })),
        "rates": { "fire": round(rates.fire), "collapse": round(rates.collapse), "disease": round(rates.disease) },
    })
}

fn state_json(context: &CommandContext) -> Value {
    let session = &*context.session;
    let city = session.sim().risks();
    let configs = &GameConfigs::get().sim.risks;

    let mut buildings = 0;
    let (mut high_fire, mut high_collapse, mut high_disease, mut burning, mut sick) = (0, 0, 0, 0, 0);
    let (mut max_fire, mut max_collapse, mut max_disease) = (0.0_f32, 0.0_f32, 0.0_f32);
    // (highest risk, cell, entry) per building, sorted below.
    let mut riskiest: Vec<(f32, (i32, i32), Value)> = Vec::new();
    let mut fires: Vec<((i32, i32), Value)> = Vec::new();
    let mut outbreaks: Vec<((i32, i32), Value)> = Vec::new();

    session.world().for_each_building_of_any_kind(|building| {
        // Sites and buildings whose configs give them no risks don't count.
        if building.is_under_construction() || building.configs().risk_rates().is_zero() {
            return true;
        }
        let risks = building.risks();
        let (fire, collapse, disease) =
            (risks.get(RiskKind::Fire), risks.get(RiskKind::Collapse), risks.get(RiskKind::Disease));
        buildings += 1;
        high_fire += usize::from(fire >= RISK_HIGH);
        high_collapse += usize::from(collapse >= RISK_HIGH);
        high_disease += usize::from(disease >= RISK_HIGH);
        burning += usize::from(risks.is_burning());
        sick += usize::from(risks.has_outbreak());
        max_fire = max_fire.max(fire);
        max_collapse = max_collapse.max(collapse);
        max_disease = max_disease.max(disease);

        let cell = building.base_cell();
        if let Some(fire) = risks.burning() {
            fires.push(((cell.y, cell.x), json!({
                "name": building.name(),
                "cell": cell_json(cell),
                "secs_left": round(fire.secs_left),
            })));
        }
        if let Some(outbreak) = risks.outbreak() {
            outbreaks.push(((cell.y, cell.x), json!({
                "name": building.name(),
                "cell": cell_json(cell),
                "secs": round(outbreak.secs),
                "deaths": outbreak.deaths,
                "population": building.population_count(),
            })));
        }
        riskiest.push((risks.highest(), (cell.y, cell.x), json!({
            "name": building.name(),
            "kind": building.kind().to_string(),
            "cell": cell_json(cell),
            "fire": round(fire),
            "collapse": round(collapse),
            "disease": round(disease),
            "near_water": risks.is_near_water(),
        })));
        true
    });

    // Highest risk first; ties by cell, so the list reads the same on every run.
    riskiest.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let riskiest: Vec<Value> = riskiest.into_iter().take(RISKIEST_LISTED).map(|(_, _, entry)| entry).collect();
    fires.sort_by(|a, b| a.0.cmp(&b.0));
    let fires: Vec<Value> = fires.into_iter().map(|(_, entry)| entry).collect();
    outbreaks.sort_by(|a, b| a.0.cmp(&b.0));
    let outbreaks: Vec<Value> = outbreaks.into_iter().map(|(_, entry)| entry).collect();

    json!({
        "enabled": city.is_enabled(),
        "active": city.is_active(),
        "disease_enabled": city.is_disease_enabled(),
        "disease_active": city.is_disease_active(),
        "no_city_risks_cheat": cheats::get().no_city_risks,
        "near_water_fire_scale": configs.near_water_fire_scale,
        "rate_jitter": configs.rate_jitter,
        "buildings": buildings,
        "high": { "fire": high_fire, "collapse": high_collapse, "disease": high_disease },
        "burning": burning,
        "sick": sick,
        "disease_deaths": city.disease_deaths(),
        "highest": { "fire": round(max_fire), "collapse": round(max_collapse), "disease": round(max_disease) },
        "riskiest": riskiest,
        "fires": fires,
        "outbreaks": outbreaks,
    })
}
