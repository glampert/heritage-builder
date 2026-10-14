// Camera commands.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    tile::TileMapLayerKind,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "camera.state",
        help: "Cell at the center of the screen, zoom, and the range of visible cells.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "camera.teleport",
        help: "Center the camera on a cell, as far as the map edges allow.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
        run: cmd_teleport,
    },
    CommandDef {
        name: "camera.zoom",
        help: "Set the zoom at once, without the smooth zoom.",
        params: &[ParamDef { name: "zoom", kind: "number", required: true, help: "1 is the default zoom." }],
        run: cmd_zoom,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(camera_state(context))
}

fn cmd_teleport(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    context.require_cell_in_map(params.cell.0)?;

    context.session.camera_mut().teleport(params.cell.0);
    done(camera_state(context))
}

fn cmd_zoom(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        zoom: f32,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let (min_zoom, max_zoom) = context.session.camera().zoom_limits();
    if !(min_zoom..=max_zoom).contains(&params.zoom) {
        return Err(CommandError::invalid_params(format!("Zoom must be from {min_zoom} to {max_zoom}, got {}.", params.zoom)));
    }

    context.session.camera_mut().snap_zoom(params.zoom);
    done(camera_state(context))
}

fn camera_state(context: &CommandContext) -> Value {
    let camera = context.session.camera();
    let tile_map = context.session.tile_map();

    // The cell under the screen center can be just past the map edge.
    let center_cell =
        tile_map.find_exact_cell_for_point(TileMapLayerKind::Terrain, camera.viewport_center(), camera.transform());
    let center_cell = tile_map.is_cell_within_bounds(center_cell).then(|| cell_json(center_cell));

    let visible_cells = camera.visible_cells_range();

    json!({
        "center_cell": center_cell,
        "zoom": camera.current_zoom(),
        "visible_cells": { "start": cell_json(visible_cells.start), "end": cell_json(visible_cells.end) },
    })
}
