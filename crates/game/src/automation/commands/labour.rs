// Labour commands (docs/plans/messages.md, M11 Phase 2): the sectors' priorities and the workers
// the allocation hands out by them (sim::labour, crate::labour), and a workplace's Pause work.

use std::str::FromStr;

use serde::Deserialize;
use serde_json::{Value, json};

use super::CellParam;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    cheats,
    labour::{self, LabourReport},
    sim::labour::LabourSector,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "labour.state",
        help: "The labour priorities: the sectors in `order` (priority 1 first), and for each (`sectors`, in that \
               order) its `priority`, `name`, the jobs `needed` and `employed` in its workplaces that can work \
               (finished, not paused, not burning, by a road), `short`, how many are `paused`, and its `buildings` \
               (building, cell, name, min, needed, employed, paused, working); the houses' `workforce`, `employed`, \
               `looking_for_work` and the `jobs_not_filled`; `frozen` while the freeze_labour cheat stops the \
               allocation.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "labour.set",
        help: "Move a sector to a priority (the sectors between moving one place to make room), or set the whole \
               order; the workers are handed out again at once. Then report labour.state.",
        params: &[
            ParamDef {
                name: "sector",
                kind: "string",
                required: false,
                help: "food_and_farming, industry, services, culture_and_religion, infrastructure or trade. Needs \
                       priority.",
            },
            ParamDef { name: "priority", kind: "int", required: false, help: "1 (filled first) to 6." },
            ParamDef { name: "order", kind: "array", required: false, help: "Every sector once, priority 1 first." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "labour.allocate",
        help: "Hand the houses' workers out to the workplaces now, as the allocation does every 10 simulated \
               seconds (nothing while the freeze_labour cheat is on). Then report labour.state.",
        params: &[],
        run: cmd_allocate,
    },
    CommandDef {
        name: "building.pause",
        help: "Pause a workplace (its workers go to other work and it gets none, so it stops) or resume it, as the \
               inspector's Pause work and Resume work do; toggles without `paused`. Reports the building's `name`, \
               `paused` and `workers` (employees, min, max).",
        params: &[
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: any cell of the building." },
            ParamDef { name: "paused", kind: "bool", required: false, help: "true pauses, false resumes." },
        ],
        run: cmd_pause,
    },
];

fn sector_name(sector: LabourSector) -> String {
    engine::ui::text::get(engine::ui::text::UiTextCategory::Hud, &sector.name_key()).to_string()
}

fn report_json(report: &LabourReport) -> Value {
    let sectors: Vec<Value> = report
        .sectors
        .iter()
        .map(|sector| {
            let mut value = serde_json::to_value(sector).unwrap_or(Value::Null);
            value["name"] = json!(sector_name(sector.sector));
            value
        })
        .collect();
    json!({
        "order": report.sectors.iter().map(|sector| sector.sector).collect::<Vec<_>>(),
        "sectors": sectors,
        "workforce": report.workforce,
        "employed": report.employed,
        "looking_for_work": report.looking_for_work,
        "jobs_not_filled": report.jobs_not_filled,
        "frozen": cheats::get().freeze_labour,
    })
}

fn state_value(context: &mut CommandContext) -> Value {
    let report = context.session.with_sim_context(0.0, |sim| labour::report(sim));
    report_json(&report)
}

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(state_value(context))
}

fn parse_sector(name: &str) -> Result<LabourSector, CommandError> {
    LabourSector::from_str(name).map_err(|_| {
        CommandError::invalid_params(format!(
            "Unknown sector '{name}': food_and_farming, industry, services, culture_and_religion, infrastructure or \
             trade."
        ))
    })
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        sector: Option<String>,
        priority: Option<usize>,
        order: Option<Vec<String>>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    match (params.sector, params.priority, params.order) {
        (Some(sector), Some(priority), None) => {
            let sector = parse_sector(&sector)?;
            if !(1..=crate::sim::labour::LABOUR_SECTOR_COUNT).contains(&priority) {
                return Err(CommandError::invalid_params(format!("priority {priority} isn't 1 to 6.")));
            }
            context.session.with_sim_context(0.0, |sim| labour::set_priority(sim, sector, priority));
        }
        (None, None, Some(order)) => {
            let order = order.iter().map(|name| parse_sector(name)).collect::<Result<Vec<_>, _>>()?;
            let set = context.session.with_sim_context(0.0, |sim| {
                let set = sim.labour_mut().set_order(&order);
                if set {
                    labour::allocate(sim);
                }
                set
            });
            if !set {
                return Err(CommandError::invalid_params("order must name every sector once."));
            }
        }
        _ => return Err(CommandError::invalid_params("Give sector and priority, or order.")),
    }
    done(state_value(context))
}

fn cmd_allocate(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    context.session.with_sim_context(0.0, labour::allocate);
    done(state_value(context))
}

fn cmd_pause(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
        paused: Option<bool>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let CellParam(cell) = params.cell;
    context.require_cell_in_map(cell)?;

    let session = &mut *context.session;
    let building = session
        .world()
        .find_building_for_cell(cell, session.tile_map())
        .ok_or_else(|| CommandError::not_found(format!("No building at [{}, {}].", cell.x, cell.y)))?;
    if !building.has_jobs() {
        return Err(CommandError::invalid_params(format!("The {} has no workers to pause.", building.name())));
    }
    let handle = building.kind_and_id();
    let paused = params.paused.unwrap_or(!building.is_paused());
    session.with_sim_context(0.0, |sim| labour::set_paused(sim, handle, paused));

    let building = session.world().find_building(handle.kind, handle.id).expect("found above");
    let employer = building.workers().and_then(|workers| workers.as_employer()).expect("has jobs");
    done(json!({
        "name": building.name(),
        "paused": building.is_paused(),
        "workers": {
            "employees": employer.employee_count(),
            "min": employer.min_employees(),
            "max": employer.max_employees(),
        },
    }))
}
