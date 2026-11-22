// The inspector's figures (crate::inspect, docs/plans/realm-ui.md, Phase 8): the report its panel
// paints for what's at a cell, so scenarios check what the player reads.

use serde::Deserialize;
use serde_json::Value;

use super::CellParam;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::BuildingKind,
    inspect::{self, StorageOrder},
    menu::GameMenusMode,
    realm,
    sim::resources::ResourceKind,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "storage.orders",
        help: "A storage yard's or granary's order for each good it takes (refuse, accept, get or empty), as its \
               inspector shows them; sets one good's (`good` and `order`) or every good's (`all`) first. Refuse and \
               Empty take in no more of the good; Get and Empty send no runners until roadmap M11.",
        params: &[
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]: any cell of the store." },
            ParamDef {
                name: "good",
                kind: "string",
                required: false,
                help: "A good by name (Rice, RoofTiles). Needs order.",
            },
            ParamDef { name: "order", kind: "string", required: false, help: "refuse, accept, get or empty." },
            ParamDef { name: "all", kind: "string", required: false, help: "An order for every good." },
        ],
        run: cmd_orders,
    },
    CommandDef {
        name: "inspect.at",
        help: "What the inspector shows of what's on top at a cell (a walker, a building, a prop, else the terrain): \
           target, panel (workshop, house, storage, service, market, site, unit, prop, terrain), icon, name, \
           subtitle, status (word, tone, summary), sections (each with its `section` kind: workers, residents, \
           production, stock, storage, ticks, values, progress, errand, risks, costs_risks, text) and actions \
           (kind, label, enabled, tooltip), in the UI realm's words.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
        run: cmd_at,
    },
];

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
    let report =
        report.ok_or_else(|| CommandError::not_found(format!("Nothing to inspect at [{}, {}].", cell.x, cell.y)))?;
    let mut value = serde_json::to_value(&report).unwrap_or(Value::Null);
    // The ground's height at the cell (docs/plans/terrain-height.md).
    if let Some(object) = value.as_object_mut() {
        let (level, shape) = context.session.tile_map().elevation().get(cell);
        object.insert("level".into(), serde_json::json!(level));
        object.insert("shape".into(), serde_json::json!(shape.name()));
    }
    done(value)
}

fn cmd_orders(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
        good: Option<String>,
        order: Option<StorageOrder>,
        all: Option<StorageOrder>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let CellParam(cell) = params.cell;
    context.require_cell_in_map(cell)?;
    let good = match &params.good {
        Some(name) => Some(
            ResourceKind::all()
                .iter()
                .find(|kind| kind.name().eq_ignore_ascii_case(name))
                .ok_or_else(|| CommandError::invalid_params(format!("Unknown good '{name}'.")))?,
        ),
        None => None,
    };
    if good.is_some() != params.order.is_some() {
        return Err(CommandError::invalid_params("good and order go together."));
    }

    let session = &mut *context.session;
    let handle = session
        .world()
        .find_building_for_cell(cell, session.tile_map())
        .filter(|building| building.is(BuildingKind::storage()))
        .map(|building| building.kind_and_id())
        .ok_or_else(|| CommandError::not_found(format!("No storage building at [{}, {}].", cell.x, cell.y)))?;
    let building = session.world_mut().find_building_mut(handle.kind, handle.id).expect("found above");
    let slots = &mut building.as_storage_mut().storage_slots;
    if let Some(order) = params.all {
        slots.set_orders(ResourceKind::all(), order);
    }
    if let (Some(kind), Some(order)) = (good, params.order) {
        slots.set_orders(kind, order);
    }

    let config = crate::building::config::BuildingConfigs::get().find_storage_config(building.kind());
    let orders: serde_json::Map<String, Value> = config
        .resources_accepted
        .iter()
        .flat_map(|group| group.iter())
        .map(|kind| {
            (
                kind.name().to_string(),
                serde_json::to_value(building.as_storage().storage_slots.order(kind)).unwrap_or(Value::Null),
            )
        })
        .collect();
    done(serde_json::json!({ "orders": orders }))
}
