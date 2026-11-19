// Floods commands: the river's flood, the land's fertility and the dykes (sim::floods,
// docs/plans/seasons-floods.md).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    config::GameConfigs,
    floods,
    sim::floods::{MAX_FERTILITY, MAX_WEAR},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "floods.state",
        help: "Whether the river floods in this game, and its flood: month, reach (the least and most cells from \
               its water), months under water, sources (a cell of each water body that floods), river and \
               ordinary_reach (the cells a flood at its largest reach would cover now); the flood under way \
               (cells, months_left, recedes_in: the ordinal month the water goes, severity) or null; the next \
               flood's severity (faith's gentle and severe floods); the log of floods and how many breached no \
               dyke (survived); the dykes (count, worn: at worn_wear or more, and each one's cell, wear and \
               walked: the ordinal month a builder last walked it); and the land's fertility (kept while floods are on: mean; else \
               every cell reads the neutral), at `cell` if given with whether it's under water.",
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
               out afresh, as at a start), set a cell's fertility (0 to 100, while floods are on) or a dyke \
               cell's wear (0 to 100: a dyke the flood reaches breaches with chance (wear / 100)² × severity), or \
               the next flood's severity. Reports floods.state.",
        params: &[
            ParamDef { name: "enabled", kind: "bool", required: false, help: "Whether the river floods." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "The cell whose fertility to set." },
            ParamDef { name: "fertility", kind: "number", required: false, help: "0 to 100. Needs cell." },
            ParamDef { name: "wear", kind: "number", required: false, help: "0 to 100, a dyke's. Needs cell." },
            ParamDef { name: "severity", kind: "number", required: false, help: "The next flood's severity, 0 or more." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "floods.flood_now",
        help: "The river floods now, as it does at its month: out to `reach` cells from its water if given, else to \
               a reach drawn from its range (farther by a severe flood's severity), damaging what it reaches \
               through the risks, taking the crops under it and leaving silt. Fails while floods are off or a \
               flood is under way. Reports floods.state.",
        params: &[ParamDef { name: "reach", kind: "number", required: false, help: "Cells from the water, 1 or more." }],
        run: cmd_flood_now,
    },
    CommandDef {
        name: "floods.recede",
        help: "The water under way goes now, as it does at the start of the month after its last. Fails without a \
               flood under way. Reports floods.state.",
        params: &[],
        run: cmd_recede,
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
        wear: Option<f32>,
        severity: Option<f32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    if let Some(wear) = params.wear {
        if !(0.0..=MAX_WEAR).contains(&wear) {
            return Err(CommandError::invalid_params(format!("wear must be from 0 to 100, got {wear}.")));
        }
        let Some(CellParam(cell)) = params.cell else {
            return Err(CommandError::invalid_params("wear needs a cell.".to_string()));
        };
        if !crate::sim::floods::is_dyke(context.session.tile_map(), cell) {
            return Err(CommandError::invalid_params(format!("No dyke at {cell}.")));
        }
    }
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
    let mut fertility_kept = true;
    let mut wear_kept = true;
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        if let Some(enabled) = params.enabled {
            floods::set_enabled(sim_context, enabled);
        }
        let floods = sim_context.floods_mut();
        if let (Some(CellParam(cell)), Some(wear)) = (params.cell, params.wear) {
            floods.sync_dykes(sim_context.tile_map());
            wear_kept = floods.set_wear(cell, wear);
        }
        if let (Some(CellParam(cell)), Some(fertility)) = (params.cell, params.fertility) {
            fertility_kept = floods.set_fertility(cell, fertility.round() as u8);
        }
        if let Some(severity) = params.severity {
            floods.set_next_severity(severity);
        }
    });
    if !wear_kept {
        return Err(CommandError::invalid_params(
            "The dykes' wear is kept only while floods are on (floods.set enabled=true).".to_string(),
        ));
    }
    if !fertility_kept && params.fertility.is_some() {
        return Err(CommandError::invalid_params(
            "The land's fertility is kept only while floods are on (floods.set enabled=true).".to_string(),
        ));
    }
    done(state_json(context, params.cell.map(|cell| cell.0)))
}

fn cmd_flood_now(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        reach: Option<u32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    if params.reach == Some(0) {
        return Err(CommandError::invalid_params("reach must be 1 or more.".to_string()));
    }
    let floods = context.session.sim().floods();
    if !floods.is_enabled() {
        return Err(CommandError::invalid_params("Floods are off in this game (floods.set enabled=true).".to_string()));
    }
    if floods.flood().is_some() {
        return Err(CommandError::invalid_params("A flood is under way (floods.recede).".to_string()));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        floods::flood_now(sim_context, params.reach);
    });
    done(state_json(context, None))
}

fn cmd_recede(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {}

    let _params: Params = parse_params(params)?;
    context.require_map()?;
    if context.session.sim().floods().flood().is_none() {
        return Err(CommandError::invalid_params("No flood is under way.".to_string()));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        floods::recede(sim_context);
    });
    done(state_json(context, None))
}

fn state_json(context: &CommandContext, cell: Option<common::coords::Cell>) -> Value {
    let floods = context.session.sim().floods();
    let profile = floods.profile();
    let fertility = floods.fertility();
    let recedes_in = floods.flood().map(|flood| {
        let sim = &GameConfigs::get().sim;
        let now = context.session.sim().calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month();
        now + flood.months_left
    });
    json!({
        "enabled": floods.is_enabled(),
        "month": profile.month,
        "reach": profile.reach,
        "months": profile.months,
        "sources": profile.sources.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>(),
        "river": profile.river,
        "ordinary_reach": floods.ordinary_reach(context.session.tile_map()).len(),
        "flood": floods.flood().map(|flood| json!({
            "cells": flood.cells.len(),
            "months_left": flood.months_left,
            "recedes_in": recedes_in,
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
            "worn": floods.worn_dykes(GameConfigs::get().sim.floods.worn_wear),
            "wear": floods.dyke_wear().iter().map(|dyke| json!({
                "cell": cell_json(dyke.cell),
                "wear": dyke.wear,
                "walked": dyke.walked,
            })).collect::<Vec<_>>(),
        },
        "fertility": {
            "kept": !fertility.is_empty(),
            "neutral": floods.neutral_fertility(),
            "mean": fertility.mean(),
            "at": cell.map(|cell| json!({
                "cell": cell_json(cell),
                "fertility": floods.fertility_at(cell),
                "yield_factor": floods.yield_factor_at(cell),
                "under_water": floods.is_flooded(cell),
            })),
        },
    })
}
