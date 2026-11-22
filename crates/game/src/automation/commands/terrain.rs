// Terrain height commands (docs/plans/terrain-height.md): read and set cells' levels and shapes,
// and paint with the developer palette's brushes.

use common::coords::Cell;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    menu::{ElevationPlacement, HeightBrush, TilePlacement, end_height_stroke},
    tile::elevation::{self, Dir, HeightTool, Shape},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "terrain.elevation",
        help: "The terrain's heights: with a cell, its level, shape, ground height and any problem; \
               without, the map's raised cells, highest ground and problems.",
        params: &[ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y]." }],
        run: cmd_elevation,
    },
    CommandDef {
        name: "terrain.set_elevation",
        help: "Set a cell's (or a rectangle's) level and shape, as the editor does: refused under buildings \
               and on water; one undo step.",
        params: &[
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y]." },
            ParamDef { name: "from", kind: "cell", required: false, help: "A rectangle's corner, with to." },
            ParamDef { name: "to", kind: "cell", required: false, help: "Its opposite corner." },
            ParamDef { name: "level", kind: "integer", required: true, help: "0 to 7." },
            ParamDef {
                name: "shape",
                kind: "string",
                required: false,
                help: "flat (default), or ramp_n_low, ramp_n_high, ... (n, e, s, w): the foot or head of a ramp climbing that way.",
            },
        ],
        run: cmd_set_elevation,
    },
    CommandDef {
        name: "terrain.brush",
        help: "One stroke of a height brush at a cell, as the developer palette's Height tools paint: one undo step.",
        params: &[
            ParamDef {
                name: "tool",
                kind: "string",
                required: true,
                help: "raise, lower, terrace, smooth, flatten, or ramp_n, ramp_e, ramp_s, ramp_w (a two-cell ramp from the cell up that way).",
            },
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: the brush's centre." },
            ParamDef { name: "size", kind: "integer", required: false, help: "The brush's width: 1 (default), 3 or 5." },
            ParamDef { name: "level", kind: "integer", required: false, help: "terrace's level." },
        ],
        run: cmd_brush,
    },
];

fn map_json(context: &CommandContext) -> Value {
    let map = context.session.tile_map().elevation();
    let problems = map.problems();
    json!({
        "flat": map.is_flat(),
        "raised_cells": map.raised_count(),
        "max_height": map.max_height(),
        "problems": problems
            .iter()
            .take(50)
            .map(|(cell, problem)| json!({ "cell": cell_json(*cell), "problem": problem.name() }))
            .collect::<Vec<_>>(),
        "problem_count": problems.len(),
    })
}

fn placement_json(context: &CommandContext, placed: &ElevationPlacement) -> Value {
    let mut reply = map_json(context);
    reply["changed"] = json!(placed.changed.len());
    reply["refused"] =
        placed.refused.iter().map(|(cell, reason)| json!({ "cell": cell_json(*cell), "reason": reason.name() })).collect();
    reply
}

fn cmd_elevation(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: Option<CellParam>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let Some(CellParam(cell)) = params.cell else {
        return done(map_json(context));
    };
    context.require_cell_in_map(cell)?;

    let map = context.session.tile_map().elevation();
    let (level, shape) = map.get(cell);
    done(json!({
        "cell": cell_json(cell),
        "level": level,
        "shape": shape.name(),
        "height": map.centre_height(cell),
        "problem": map.problem_at(cell).map(|problem| problem.name()),
    }))
}

fn cmd_set_elevation(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: Option<CellParam>,
        from: Option<CellParam>,
        to: Option<CellParam>,
        level: u8,
        shape: Option<String>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let shape_name = params.shape.as_deref().unwrap_or("flat");
    let shape = Shape::parse(shape_name).ok_or_else(|| {
        CommandError::invalid_params(format!("Unknown shape '{shape_name}': flat, or ramp_<n|e|s|w>_<low|high>."))
    })?;
    if params.level > elevation::MAX_LEVEL {
        return Err(CommandError::invalid_params(format!("level must be from 0 to {}.", elevation::MAX_LEVEL)));
    }
    let (from, to) = match (params.cell, params.from, params.to) {
        (Some(cell), None, None) => (cell.0, cell.0),
        (None, Some(from), Some(to)) => (from.0, to.0),
        _ => return Err(CommandError::invalid_params("Give cell, or from and to.")),
    };
    context.require_cell_in_map(from)?;
    context.require_cell_in_map(to)?;

    let byte = elevation::encode(params.level, shape);
    let mut bytes = Vec::new();
    for y in from.y.min(to.y)..=from.y.max(to.y) {
        for x in from.x.min(to.x)..=from.x.max(to.x) {
            bytes.push((Cell::new(x, y), byte));
        }
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let placed = context.session.with_sim_context(delta_time_secs, |sim| TilePlacement::set_elevation(sim, &bytes, true));
    done(placement_json(context, &placed))
}

fn cmd_brush(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        tool: String,
        cell: CellParam,
        size: Option<i32>,
        level: Option<u8>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let cell = params.cell.0;
    context.require_cell_in_map(cell)?;

    let tool = match params.tool.as_str() {
        "raise" => HeightTool::Raise,
        "lower" => HeightTool::Lower,
        "smooth" => HeightTool::Smooth,
        "flatten" => HeightTool::Flatten,
        "terrace" => {
            let level = params.level.ok_or_else(|| CommandError::invalid_params("terrace needs a level."))?;
            if level > elevation::MAX_LEVEL {
                return Err(CommandError::invalid_params(format!("level must be from 0 to {}.", elevation::MAX_LEVEL)));
            }
            HeightTool::Terrace(level)
        }
        other => other
            .strip_prefix("ramp_")
            .and_then(Dir::parse)
            .map(HeightTool::Ramp)
            .ok_or_else(|| CommandError::invalid_params(format!("Unknown tool '{other}'.")))?,
    };
    let size = params.size.unwrap_or(1);
    if !matches!(size, 1 | 3 | 5) {
        return Err(CommandError::invalid_params("size must be 1, 3 or 5."));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let changed = context
        .session
        .with_sim_context(delta_time_secs, |sim| TilePlacement::paint_height(sim, HeightBrush { tool, size }, cell));
    end_height_stroke();

    let mut reply = map_json(context);
    reply["painted"] = json!(changed);
    done(reply)
}
