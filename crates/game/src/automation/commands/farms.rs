// Farms commands: farmsteads, their rice fields and their crops (crate::farms,
// docs/plans/seasons-floods.md, Phase 1).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CellParam, cell_json, refresh_world_stats, world::find_tile_def};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::{BuildingBehavior, BuildingKind},
    farms,
    menu::TilePlacement,
    tile::TileKind,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "farms.state",
        help: "The month (ordinal), each farmstead (cell, workers, its fields, how many its workers tend, how many \
               are growing, the ordinal month the next are reaped in and what they'd give at their staffing so \
               far, the harvest it holds) and each rice field on the map (cell, its farmstead's cell or null, \
               stage: fallow, sown, growing or ripe; tended, sown and reaped_in as ordinal months, irrigated, \
               water: river, gate or null, the watering sluice gate's cell or null, flooded), each sluice gate \
               (cell, site, working, workers, intake: water across its dyke, supplying, its outlets, the fields \
               its water reaches and the fields its outlet reaches through canals and fields, wherever their \
               water comes from) and the canals (count, wet, the dry cells).",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "world.place_area",
        help: "Lay a tile over every cell of the rectangle from `from` to `to` that can take it, as the build \
               palette's area tool does (rice fields: on empty land within a farmstead's reach, not under the \
               flood), each charged as it's placed; one undo step. Also lays plain terrain, such as water for a \
               river or a pond, its shores following. Reports the cells placed and refused, and farms.state.",
        params: &[
            ParamDef {
                name: "tile",
                kind: "string",
                required: true,
                help: "An area tile (rice_field), or a terrain tile that isn't a road (water, grass, dirt).",
            },
            ParamDef { name: "from", kind: "cell", required: true, help: "A corner of the rectangle." },
            ParamDef { name: "to", kind: "cell", required: true, help: "The opposite corner." },
        ],
        run: cmd_place_area,
    },
];

fn cmd_state(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {}

    let _: Params = parse_params(params)?;
    context.require_map()?;
    done(state_json(context))
}

fn cmd_place_area(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        tile: String,
        from: CellParam,
        to: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let (tile_def, _) = find_tile_def(&params.tile, None)?;
    let is_plain_terrain = tile_def.is(TileKind::Terrain) && !tile_def.is_road();
    if !farms::is_field_def(tile_def) && !is_plain_terrain {
        return Err(CommandError::invalid_params(format!(
            "'{}' isn't laid over an area: a rice field, or terrain that isn't a road (world.road draws those).",
            tile_def.name
        )));
    }
    let (from, to) = (params.from.0, params.to.0);
    context.require_cell_in_map(from)?;
    context.require_cell_in_map(to)?;

    let mut cells = Vec::new();
    for y in from.y.min(to.y)..=from.y.max(to.y) {
        for x in from.x.min(to.x)..=from.x.max(to.x) {
            cells.push(common::coords::Cell::new(x, y));
        }
    }

    refresh_world_stats(context);
    let delta_time_secs = context.engine.frame_clock().delta_time();
    let placed = context
        .session
        .with_sim_context(delta_time_secs, |sim_context| TilePlacement::place_area(sim_context, &cells, tile_def, true));
    refresh_world_stats(context);

    let mut reply = state_json(context);
    reply["placed"] = json!(placed.placed.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>());
    reply["refused"] = json!(placed.refused.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>());
    reply["gold"] = json!(context.session.world().stats().treasury.gold_units_total);
    done(reply)
}

fn state_json(context: &mut CommandContext) -> Value {
    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        let mut farmsteads = Vec::new();
        sim_context.world().for_each_building(BuildingKind::Farm, |building| {
            if let Some(report) = farms::farmstead_report(sim_context, building) {
                let producer = building.as_producer();
                let workers = producer.workers().and_then(|workers| workers.as_employer());
                let stock = producer.production_output_stock.item;
                farmsteads.push(json!({
                    "cell": cell_json(building.base_cell()),
                    "site": building.is_under_construction(),
                    "workers": workers.map(|workers| workers.employee_count()),
                    "max_workers": workers.map(|workers| workers.max_employees()),
                    "fields": report.fields,
                    "tended": report.tended,
                    "fallow_for_want_of_workers": report.fields - report.tended,
                    "growing": report.growing,
                    "next_reaped": report.next_reaped,
                    "expected": report.expected.round(),
                    "stock": { "good": stock.kind.to_string(), "count": stock.count },
                }));
            }
            true
        });
        let irrigation = farms::Irrigation::read(sim_context.world(), sim_context.tile_map());
        let fields: Vec<Value> = farms::field_cells(sim_context.tile_map())
            .into_iter()
            .filter_map(|cell| farms::field_report_with(sim_context, cell, &irrigation))
            .map(|field| {
                let (water, gate) = match field.water {
                    Some(farms::WaterSource::River) => (json!("river"), Value::Null),
                    Some(farms::WaterSource::Gate(cell)) => (json!("gate"), cell_json(cell)),
                    None => (Value::Null, Value::Null),
                };
                json!({
                    "cell": cell_json(field.cell),
                    "farmstead": field.farmstead_cell.map(cell_json),
                    "stage": field.stage.name(),
                    "tended": field.tended,
                    "sown": field.sown,
                    "reaped_in": field.reaped_in,
                    "irrigated": field.irrigated,
                    "water": water,
                    "gate": gate,
                    "flooded": field.flooded,
                })
            })
            .collect();
        let mut gates = Vec::new();
        sim_context.world().for_each_building(BuildingKind::SluiceGate, |building| {
            if let Some(supply) = irrigation.gate_at(building.base_cell()) {
                let workers = building.workers().and_then(|workers| workers.as_employer());
                let base = supply.base_cell();
                let reached = farms::irrigation::fields_reached_by(sim_context.tile_map(), supply);
                let watered = irrigation.fields_watered_by(sim_context.tile_map(), base);
                gates.push(json!({
                    "cell": cell_json(base),
                    "site": building.is_under_construction(),
                    "working": supply.gate.working,
                    "workers": workers.map(|workers| workers.employee_count()),
                    "intake": supply.intake,
                    "supplying": supply.supplies(),
                    "outlets": supply.outlets.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>(),
                    "watered": watered,
                    "reached": reached.iter().map(|&cell| cell_json(cell)).collect::<Vec<_>>(),
                }));
            }
            true
        });
        let canals = irrigation.canals();
        let dry: Vec<Value> = canals.iter().filter(|&&cell| !irrigation.is_wet(cell)).map(|&cell| cell_json(cell)).collect();
        json!({
            "month": farms::current_month(sim_context),
            "farmsteads": farmsteads,
            "fields": fields,
            "sluice_gates": gates,
            "canals": { "count": canals.len(), "wet": canals.len() - dry.len(), "dry": dry },
        })
    })
}
