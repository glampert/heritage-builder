// Floods commands: the river's flood, the land's fertility and the dykes (sim::floods,
// docs/plans/seasons-floods.md).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    config::GameConfigs,
    sim::floods::MAX_FERTILITY,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "floods.state",
        help: "Whether the river floods in this game, and its flood: month, reach (the least and most cells from \
               its water), months under water, sources (a cell of each water body that floods) and river; the \
               flood under way (cells, months_left, severity) or null; the next flood's severity (faith's gentle \
               and severe floods); the log of floods and how many breached no dyke (survived); the dykes' wear; \
               and the land's fertility (kept while floods are on: mean; else every cell reads the neutral), at \
               `cell` if given.",
        params: &[ParamDef {
            name: "cell",
            kind: "cell",
            required: false,
            help: "A cell to report the fertility and yield factor of.",
        }],
        run: cmd_state,
    },
    CommandDef {
        name: "floods.set",
        help: "Turn floods on or off for this game (on: the water that floods and the land's fertility are laid \
               out afresh, as at a start), set a cell's fertility (0 to 100, while floods are on), or the next \
               flood's severity. Reports floods.state.",
        params: &[
            ParamDef { name: "enabled", kind: "bool", required: false, help: "Whether the river floods." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "The cell whose fertility to set." },
            ParamDef { name: "fertility", kind: "number", required: false, help: "0 to 100. Needs cell." },
            ParamDef { name: "severity", kind: "number", required: false, help: "The next flood's severity, 0 or more." },
        ],
        run: cmd_set,
    },
];

fn cmd_state(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: Option<CellParam>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    done(state_json(context, params.cell.map(|cell| cell.0)))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        enabled: Option<bool>,
        cell: Option<CellParam>,
        fertility: Option<f32>,
        severity: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    if let Some(fertility) = params.fertility {
        if !(0.0..=MAX_FERTILITY as f32).contains(&fertility) {
            return Err(CommandError::invalid_params(format!("fertility must be from 0 to 100, got {fertility}.")));
        }
        if params.cell.is_none() {
            return Err(CommandError::invalid_params("fertility needs a cell.".to_string()));
        }
    }
    if let Some(severity) = params.severity
        && !(severity >= 0.0 && severity.is_finite())
    {
        return Err(CommandError::invalid_params(format!("severity must be 0 or more, got {severity}.")));
    }
    if let Some(CellParam(cell)) = params.cell
        && !context.session.tile_map().is_cell_within_playable_area(cell)
    {
        return Err(CommandError::invalid_params(format!("Cell {cell} is off the map.")));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let rules = &GameConfigs::get().sim.floods;
    let mut fertility_kept = true;
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        let floods = sim_context.floods_mut();
        if let Some(enabled) = params.enabled {
            floods.set_enabled(enabled, rules, sim_context.tile_map());
        }
        if let (Some(CellParam(cell)), Some(fertility)) = (params.cell, params.fertility) {
            fertility_kept = floods.set_fertility(cell, fertility.round() as u8);
        }
        if let Some(severity) = params.severity {
            floods.set_next_severity(severity);
        }
    });
    if !fertility_kept {
        return Err(CommandError::invalid_params(
            "The land's fertility is kept only while floods are on (floods.set enabled=true).".to_string(),
        ));
    }
    done(state_json(context, params.cell.map(|cell| cell.0)))
}

fn state_json(context: &CommandContext, cell: Option<common::coords::Cell>) -> Value {
    let floods = context.session.sim().floods();
    let profile = floods.profile();
    let fertility = floods.fertility();
    json!({
        "enabled": floods.is_enabled(),
        "month": profile.month,
        "reach": profile.reach,
        "months": profile.months,
        "sources": profile.sources.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>(),
        "river": profile.river,
        "flood": floods.flood().map(|flood| json!({
            "cells": flood.cells.len(),
            "months_left": flood.months_left,
            "severity": flood.severity,
        })),
        "next_severity": floods.next_severity(),
        "log": floods.log().iter().map(|record| json!({
            "ordinal_month": record.ordinal_month,
            "reach": record.reach,
            "severity": record.severity,
            "cells": record.cells,
            "buildings_damaged": record.buildings_damaged,
            "fields_flooded": record.fields_flooded,
            "breaches": record.breaches,
        })).collect::<Vec<_>>(),
        "survived": floods.floods_survived(),
        "dykes": {
            "count": floods.dyke_wear().len(),
            "wear": floods.dyke_wear().iter().map(|dyke| json!({ "cell": cell_json(dyke.cell), "wear": dyke.wear })).collect::<Vec<_>>(),
        },
        "fertility": {
            "kept": !fertility.is_empty(),
            "neutral": floods.neutral_fertility(),
            "mean": fertility.mean(),
            "at": cell.map(|cell| json!({
                "cell": cell_json(cell),
                "fertility": floods.fertility_at(cell),
                "yield_factor": floods.yield_factor_at(cell),
            })),
        },
    })
}
