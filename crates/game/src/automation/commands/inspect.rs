// The inspector's figures (crate::inspect, docs/plans/realm-ui.md, Phase 8): the report its panel
// paints for what's at a cell, so scenarios check what the player reads.

use serde::Deserialize;
use serde_json::Value;

use super::CellParam;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    inspect,
    menu::GameMenusMode,
    realm,
};

pub(super) const COMMANDS: &[CommandDef] = &[CommandDef {
    name: "inspect.at",
    help: "What the inspector shows of what's on top at a cell (a walker, a building, a prop, else the terrain): \
           target, panel (workshop, house, storage, service, market, site, unit, prop, terrain), icon, name, \
           subtitle, status (word, tone, summary), sections (each with its `section` kind: workers, residents, \
           production, stock, storage, ticks, values, progress, errand, risks, costs_risks, text) and actions \
           (kind, label, enabled, tooltip), in the UI realm's words.",
    params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
    run: cmd_at,
}];

fn cmd_at(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let CellParam(cell) = params.cell;
    context.require_cell_in_map(cell)?;
    let realm = realm::ui_realm_for(context.session.current_menus_mode() == Some(GameMenusMode::Home)).realm;
    let delta_time_secs = context.engine.frame_clock().delta_time();
    let report = context.session.with_sim_context(delta_time_secs, |sim_context| {
        let tile = sim_context.tile_map().topmost_tile_at_cell(cell)?;
        inspect::inspect_tile(sim_context, tile, realm)
    });
    let report = report.ok_or_else(|| CommandError::not_found(format!("Nothing to inspect at [{}, {}].", cell.x, cell.y)))?;
    done(serde_json::to_value(&report).unwrap_or(Value::Null))
}
