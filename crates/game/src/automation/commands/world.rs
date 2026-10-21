// World commands: place and clear tiles as a player would, spawn objects for test setup, and read
// buildings, units, props and tiles back.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use common::coords::Cell;

use super::{CellParam, ObjectId, archetype_building_kinds, cell_json, refresh_world_stats};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::{Building, BuildingArchetypeKind},
    menu::{PlaceOrClearResult, TilePlacement},
    prop::Prop,
    save_context::{PostSaveContext, PreSaveContext},
    sim::resources::{ResourceStock, StockItem, Workers},
    tile::{
        Tile,
        TileKind,
        TileMapLayerKind,
        road::{self, RoadKind},
        sets::{TileCategory, TileDef, TileSets},
    },
    undo_redo,
    unit::Unit,
    world::{object::{GameObject, Spawner}, stats::WorldStats},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "tiles.list",
        help: "List the tiles that world.place takes: name, category, layer, size and cost.",
        params: &[
            ParamDef { name: "category", kind: "string", required: false, help: "Only this category, e.g. buildings." },
            ParamDef { name: "layer", kind: "string", required: false, help: "Only this layer: terrain or objects." },
        ],
        run: cmd_tiles_list,
    },
    CommandDef {
        name: "world.place",
        help: "Place a tile (building, road, terrain, ...) at a cell as a player would: pays its cost, records undo.",
        params: &[
            ParamDef { name: "tile", kind: "string", required: true, help: "Tile name, see tiles.list." },
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]. For bigger tiles, the cell with the lowest x and y." },
        ],
        run: cmd_place,
    },
    CommandDef {
        name: "world.clear",
        help: "Clear the object, road or vacant lot at a cell as a player would. Other terrain can't be cleared.",
        params: &[
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]. Any cell of a multi-cell building." },
            ParamDef { name: "layer", kind: "string", required: false, help: "objects or terrain. Default: the topmost." },
        ],
        run: cmd_clear,
    },
    CommandDef {
        name: "world.road",
        help: "Build a road between two cells as a player's drag does: straight legs when possible, all or nothing.",
        params: &[
            ParamDef { name: "from", kind: "cell", required: true, help: "[x, y]." },
            ParamDef { name: "to", kind: "cell", required: true, help: "[x, y]." },
            ParamDef { name: "kind", kind: "string", required: false, help: "dirt (default) or paved." },
        ],
        run: cmd_road,
    },
    CommandDef {
        name: "world.spawn_unit",
        help: "Spawn a unit for test setup: free, no undo.",
        params: &[
            ParamDef { name: "unit", kind: "string", required: true, help: "Unit name, e.g. peasant. See tiles.list category=units." },
            ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." },
        ],
        run: cmd_spawn_unit,
    },
    CommandDef {
        name: "world.despawn",
        help: "Remove a building, unit or prop by id, for test setup: no refund, no undo.",
        params: &[ParamDef { name: "id", kind: "string", required: true, help: "Object id from world.list, e.g. unit:12:40." }],
        run: cmd_despawn,
    },
    CommandDef {
        name: "world.list",
        help: "List buildings, units or props with their ids and cells.",
        params: &[
            ParamDef { name: "type", kind: "string", required: true, help: "building, unit or prop." },
            ParamDef { name: "name", kind: "string", required: false, help: "Only objects with this name or kind." },
            ParamDef { name: "limit", kind: "int", required: false, help: "Most objects to return. Default 200." },
        ],
        run: cmd_list,
    },
    CommandDef {
        name: "world.building_at",
        help: "The building covering a cell: id, kind, population, workers, stock and road link.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
        run: cmd_building_at,
    },
    CommandDef {
        name: "world.inspect",
        help: "A building, unit or prop as a save stores it, all of it or some fields.",
        params: &[
            ParamDef { name: "id", kind: "string", required: false, help: "Object id from world.list. Or give a cell." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "The object at this cell (buildings, then units, then props)." },
            ParamDef { name: "fields", kind: "string[]", required: false, help: "JSON pointers to return, e.g. [\"/id\"]." },
        ],
        run: cmd_inspect,
    },
    CommandDef {
        name: "world.stats",
        help: "World totals: population, workers, treasury, housing and resources.",
        params: &[],
        run: cmd_stats,
    },
    CommandDef {
        name: "tile.at",
        help: "The terrain and object tiles at a cell.",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
        run: cmd_tile_at,
    },
    CommandDef { name: "undo", help: "Undo the last placement or clearing, as Ctrl+Z does.", params: &[], run: cmd_undo },
    CommandDef { name: "redo", help: "Redo the last undone edit, as Ctrl+Shift+Z does.", params: &[], run: cmd_redo },
];

const DEFAULT_LIST_LIMIT: usize = 200;

// ----------------------------------------------
// Tile lookups
// ----------------------------------------------

// Finds a tile by name in every category (of one layer, if given). Tile names are unique today;
// a name in more than one category is reported as ambiguous instead of guessing.
pub(super) fn find_tile_def(
    name: &str,
    layer: Option<TileMapLayerKind>,
) -> Result<(&'static TileDef, &'static TileCategory), CommandError> {
    let mut matches: Vec<(&'static TileDef, &'static TileCategory)> = Vec::new();

    // TileCategory::find_tile_def_by_name logs an error for every category without the name.
    TileSets::get().for_each_category(|set, category| {
        if layer.is_none_or(|layer| set.layer == layer) {
            category.for_each_tile_def(|tile_def| {
                if tile_def.name == name {
                    matches.push((tile_def, category));
                }
                true
            });
        }
        true
    });

    match matches.as_slice() {
        [] => Err(CommandError::not_found(format!("No tile named '{name}'. tiles.list lists them."))),
        [single] => Ok(*single),
        _ => {
            let categories: Vec<&str> = matches.iter().map(|(_, category)| category.name.as_str()).collect();
            Err(CommandError::invalid_params(format!("Tile name '{name}' is in several categories: {}.", categories.join(", "))))
        }
    }
}

fn layer_param(layer: Option<&str>) -> Result<Option<TileMapLayerKind>, CommandError> {
    match layer {
        None => Ok(None),
        Some("terrain") => Ok(Some(TileMapLayerKind::Terrain)),
        Some("objects") => Ok(Some(TileMapLayerKind::Objects)),
        Some(other) => Err(CommandError::invalid_params(format!("layer must be terrain or objects, got '{other}'."))),
    }
}

fn layer_name(layer: TileMapLayerKind) -> &'static str {
    match layer {
        TileMapLayerKind::Terrain => "terrain",
        TileMapLayerKind::Objects => "objects",
    }
}

fn tile_def_json(tile_def: &TileDef, category: &TileCategory) -> Value {
    let size = tile_def.size_in_cells();
    json!({
        "name": tile_def.name,
        "category": category.name,
        "layer": layer_name(tile_def.layer_kind()),
        "kind": tile_def.kind().to_string(),
        "size": [size.width, size.height],
        "cost": tile_def.cost,
    })
}

fn cmd_tiles_list(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        category: Option<String>,
        layer: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let layer = layer_param(params.layer.as_deref())?;

    let mut tiles = Vec::new();
    TileSets::get().for_each_category(|set, category| {
        let wanted_layer = layer.is_none_or(|layer| set.layer == layer);
        let wanted_category = params.category.as_deref().is_none_or(|name| category.name == name);
        if wanted_layer && wanted_category {
            category.for_each_tile_def(|tile_def| {
                tiles.push(tile_def_json(tile_def, category));
                true
            });
        }
        true
    });

    done(json!({ "tiles": tiles }))
}

// ----------------------------------------------
// Editing
// ----------------------------------------------

fn cmd_place(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        tile: String,
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let (tile_def, _) = find_tile_def(&params.tile, None)?;
    let cell = params.cell.0;

    // The whole footprint must be on the map.
    let footprint = tile_def.cell_range(cell);
    context.require_cell_in_map(footprint.start)?;
    context.require_cell_in_map(footprint.end)?;

    // Placement checks the treasury through the world stats.
    refresh_world_stats(context);

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let result = context.session.with_sim_context(delta_time_secs, |sim_context| {
        TilePlacement::place(sim_context, cell, tile_def, true, true)
    });

    if let PlaceOrClearResult::Failed { message, .. } = result {
        return Err(CommandError::failed(message));
    }

    refresh_world_stats(context);

    let mut reply = json!({ "tile": tile_def.name, "cell": cell_json(cell) });
    if let Some(object) = object_summary_at(context, cell, tile_def.layer_kind()) {
        reply["object"] = object;
    }
    reply["gold"] = json!(context.session.world().stats().treasury.gold_units_total);

    done(reply)
}

fn cmd_clear(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
        layer: Option<String>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let cell = params.cell.0;
    context.require_cell_in_map(cell)?;

    let layers = match layer_param(params.layer.as_deref())? {
        Some(layer) => vec![layer],
        None => vec![TileMapLayerKind::Objects, TileMapLayerKind::Terrain], // Topmost first.
    };

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let result = context.session.with_sim_context(delta_time_secs, |sim_context| {
        let tile = layers.iter().find_map(|layer| {
            sim_context.try_tile_from_layer(cell, *layer).filter(|tile| TilePlacement::can_clear(tile))
        })?;

        let base_cell = tile.base_cell();
        Some((TilePlacement::clear(sim_context, tile, false, true), base_cell))
    });

    match result {
        None => Err(CommandError::not_found(format!(
            "Nothing to clear at [{}, {}]. Objects, roads and vacant lots can be cleared.",
            cell.x, cell.y
        ))),
        Some((PlaceOrClearResult::ClearedTile(tile_def), base_cell)) => {
            refresh_world_stats(context);
            done(json!({ "tile": tile_def.name, "cell": cell_json(base_cell) }))
        }
        Some((PlaceOrClearResult::Failed { message, .. }, _)) => Err(CommandError::failed(message)),
        Some((PlaceOrClearResult::PlacedTile(_), _)) => unreachable!("Clearing doesn't place tiles."),
    }
}

fn cmd_road(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        from: CellParam,
        to: CellParam,
        kind: Option<String>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let (from, to) = (params.from.0, params.to.0);
    context.require_cell_in_map(from)?;
    context.require_cell_in_map(to)?;

    let kind = match params.kind.as_deref() {
        None | Some("dirt") => RoadKind::Dirt,
        Some("paved") => RoadKind::Paved,
        Some(other) => return Err(CommandError::invalid_params(format!("kind must be dirt or paved, got '{other}'."))),
    };

    // Placement checks the treasury through the world stats.
    refresh_world_stats(context);

    let segment = road::build_segment(context.session.tile_map(), from, to, kind);

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context
        .session
        .with_sim_context(delta_time_secs, |sim_context| TilePlacement::place_road_segment(sim_context, &segment, true))
        .map_err(|err| CommandError::failed(err.to_string()))?;

    refresh_world_stats(context);

    let path: Vec<Value> = segment.path.iter().map(|cell| cell_json(*cell)).collect();
    done(json!({
        "tile": segment.tile_def().name,
        "cost": segment.cost(),
        "path": path,
        "gold": context.session.world().stats().treasury.gold_units_total,
    }))
}

fn cmd_spawn_unit(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        unit: String,
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let cell = params.cell.0;
    context.require_cell_in_map(cell)?;

    let (tile_def, _) = find_tile_def(&params.unit, Some(TileMapLayerKind::Objects))?;
    if !tile_def.is(TileKind::Unit) {
        return Err(CommandError::invalid_params(format!("'{}' isn't a unit. Try tiles.list category=units.", params.unit)));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    let unit_id = context.session.with_sim_context(delta_time_secs, |sim_context| {
        let mut spawner = Spawner::new(sim_context);
        spawner.set_subtract_tile_cost(false);

        spawner.try_spawn_unit_with_tile_def(cell, tile_def).map(|unit| unit.id()).map_err(|err| err.message)
    });

    let unit_id = unit_id.map_err(CommandError::failed)?;
    done(json!({ "id": ObjectId::Unit(unit_id).to_json(), "unit": tile_def.name, "cell": cell_json(cell) }))
}

fn cmd_despawn(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        id: String,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let id = ObjectId::parse(&params.id)?;
    let name = match find_object(context, id) {
        Some(object) => object.name,
        None => return Err(CommandError::not_found(format!("No object with id '{}'.", params.id))),
    };

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, |sim_context| {
        let mut spawner = Spawner::new(sim_context);
        spawner.set_restore_tile_cost(false);

        match id {
            ObjectId::Building(archetype, id) => {
                if let Some(building) = sim_context.find_building_mut(archetype_building_kinds(archetype), id) {
                    spawner.despawn_building(building);
                }
            }
            ObjectId::Unit(id) => spawner.despawn_unit_with_id(id),
            ObjectId::Prop(id) => spawner.despawn_prop_with_id(id),
        }
    });

    refresh_world_stats(context);
    done(json!({ "id": params.id, "name": name }))
}

// ----------------------------------------------
// Queries
// ----------------------------------------------

struct ObjectRef {
    name: String,
    summary: Value,
}

fn find_object(context: &CommandContext, id: ObjectId) -> Option<ObjectRef> {
    let world = context.session.world();
    match id {
        ObjectId::Building(archetype, id) => world
            .find_building(archetype_building_kinds(archetype), id)
            .map(|building| ObjectRef { name: building.name().to_string(), summary: building_json(building) }),
        ObjectId::Unit(id) => world.find_unit(id).map(|unit| ObjectRef { name: unit.name().to_string(), summary: unit_json(unit) }),
        ObjectId::Prop(id) => world.find_prop(id).map(|prop| ObjectRef { name: prop.name().to_string(), summary: prop_json(prop) }),
    }
}

// The building, unit or prop at a cell, if any.
fn object_id_at(context: &CommandContext, cell: Cell) -> Option<ObjectId> {
    let world = context.session.world();
    let tile_map = context.session.tile_map();

    if let Some(building) = world.find_building_for_cell(cell, tile_map) {
        return Some(ObjectId::building(building.archetype_kind(), building.id()));
    }
    if let Some(unit) = world.find_unit_for_cell(cell, tile_map) {
        return Some(ObjectId::Unit(unit.id()));
    }
    world.find_prop_for_cell(cell, tile_map).map(|prop| ObjectId::Prop(prop.id()))
}

fn object_summary_at(context: &CommandContext, cell: Cell, layer: TileMapLayerKind) -> Option<Value> {
    if layer != TileMapLayerKind::Objects {
        return None;
    }
    object_id_at(context, cell).and_then(|id| find_object(context, id)).map(|object| object.summary)
}

fn building_json(building: &Building) -> Value {
    let cells = building.cell_range();
    json!({
        "id": ObjectId::building(building.archetype_kind(), building.id()).to_json(),
        "name": building.name(),
        "kind": building.kind().to_string(),
        "cell": cell_json(cells.start),
        "size": [cells.end.x - cells.start.x + 1, cells.end.y - cells.start.y + 1],
    })
}

fn unit_json(unit: &Unit) -> Value {
    json!({ "id": ObjectId::Unit(unit.id()).to_json(), "name": unit.name(), "cell": cell_json(unit.cell()) })
}

fn prop_json(prop: &Prop) -> Value {
    json!({ "id": ObjectId::Prop(prop.id()).to_json(), "name": prop.name(), "cell": cell_json(prop.cell()) })
}

fn workers_json(workers: &Workers) -> Value {
    if let Some(household) = workers.as_household_worker_pool() {
        json!({
            "employed": household.employed_count(),
            "unemployed": household.unemployed_count(),
            "total": household.total_workers(),
        })
    } else if let Some(employer) = workers.as_employer() {
        json!({ "employees": employer.employee_count(), "min": employer.min_employees(), "max": employer.max_employees() })
    } else {
        Value::Null
    }
}

fn stock_items_json<'a>(items: impl Iterator<Item = &'a StockItem>) -> Value {
    let stock: Map<String, Value> = items.map(|item| (item.kind.to_string(), json!(item.count))).collect();
    Value::Object(stock)
}

fn resource_stock_json(stock: &ResourceStock) -> Value {
    let mut items = Vec::new();
    stock.for_each(|_, item| {
        if item.count != 0 {
            items.push(StockItem { kind: item.kind, count: item.count });
        }
    });
    stock_items_json(items.iter())
}

fn cmd_list(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        r#type: String,
        name: Option<String>,
        limit: Option<usize>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let limit = params.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let name_filter = params.name.as_deref().map(str::to_lowercase);
    let matches_name = |names: &[&str]| {
        name_filter.as_deref().is_none_or(|filter| names.iter().any(|name| name.to_lowercase() == filter))
    };

    let world = context.session.world();
    let mut items = Vec::new();
    let mut total = 0;

    let mut add = |item: Value| {
        total += 1;
        if items.len() < limit {
            items.push(item);
        }
    };

    match params.r#type.as_str() {
        "building" => {
            for archetype in [
                BuildingArchetypeKind::HouseBuilding,
                BuildingArchetypeKind::ProducerBuilding,
                BuildingArchetypeKind::StorageBuilding,
                BuildingArchetypeKind::ServiceBuilding,
            ] {
                world.for_each_building(archetype_building_kinds(archetype), |building| {
                    if matches_name(&[building.name(), &building.kind().to_string()]) {
                        add(building_json(building));
                    }
                    true
                });
            }
        }
        "unit" => world.for_each_unit(|unit| {
            if matches_name(&[unit.name()]) {
                add(unit_json(unit));
            }
            true
        }),
        "prop" => world.for_each_prop(|prop| {
            if matches_name(&[prop.name()]) {
                add(prop_json(prop));
            }
            true
        }),
        other => return Err(CommandError::invalid_params(format!("type must be building, unit or prop, got '{other}'."))),
    }

    done(json!({ "total": total, "items": items }))
}

fn cmd_building_at(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let cell = params.cell.0;
    context.require_cell_in_map(cell)?;

    let session = &*context.session;
    let building = session
        .world()
        .find_building_for_cell(cell, session.tile_map())
        .ok_or_else(|| CommandError::not_found(format!("No building at [{}, {}].", cell.x, cell.y)))?;

    let mut reply = building_json(building);
    reply["operational"] = json!(building.is_operational());
    reply["road_link"] = building.road_link().map_or(Value::Null, cell_json);
    reply["population"] =
        building.population().map_or(Value::Null, |population| json!({ "count": population.count(), "max": population.max() }));
    reply["workers"] = building.workers().map_or(Value::Null, workers_json);
    reply["stock"] = stock_items_json(building.stock().iter().filter(|item| item.count != 0));

    done(reply)
}

fn cmd_inspect(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        id: Option<String>,
        cell: Option<CellParam>,
        fields: Option<Vec<String>>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let id = match (&params.id, params.cell) {
        (Some(id), None) => ObjectId::parse(id)?,
        (None, Some(cell)) => {
            context.require_cell_in_map(cell.0)?;
            object_id_at(context, cell.0)
                .ok_or_else(|| CommandError::not_found(format!("No building, unit or prop at [{}, {}].", cell.0.x, cell.0.y)))?
        }
        _ => return Err(CommandError::invalid_params("Give either id or cell.")),
    };

    // Serialized as a save would: pre_save resolves pending spawn promises first.
    let cmds = context.session.sim().cmds().clone();
    let world = context.session.world_mut();
    let state = match id {
        ObjectId::Building(archetype, id) => {
            world.find_building_mut(archetype_building_kinds(archetype), id).map(|building| save_json(building, cmds))
        }
        ObjectId::Unit(id) => world.find_unit_mut(id).map(|unit| save_json(unit, cmds)),
        ObjectId::Prop(id) => world.find_prop_mut(id).map(|prop| save_json(prop, cmds)),
    };

    let state = state
        .ok_or_else(|| CommandError::not_found(format!("No object with id '{}'.", id.to_json().as_str().unwrap_or_default())))?
        .map_err(CommandError::failed)?;

    let Some(fields) = params.fields else {
        return done(json!({ "id": id.to_json(), "state": state }));
    };

    let selected: Map<String, Value> =
        fields.into_iter().map(|pointer| (pointer.clone(), state.pointer(&pointer).cloned().unwrap_or(Value::Null))).collect();

    done(json!({ "id": id.to_json(), "fields": selected }))
}

fn save_json<T: GameObject + serde::Serialize>(object: &mut T, cmds: common::mem::RcMut<crate::sim::SimCmds>) -> Result<Value, String> {
    object.pre_save(&mut PreSaveContext::new(cmds));
    let state = serde_json::to_value(&*object).map_err(|err| err.to_string());
    object.post_save(&mut PostSaveContext::new());
    state
}

fn cmd_stats(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    refresh_world_stats(context);
    done(world_stats_json(context.session.world().stats()))
}

fn world_stats_json(stats: &WorldStats) -> Value {
    let houses = if stats.houses.total > 0 {
        json!({
            "total": stats.houses.total,
            "lowest_level": stats.houses.lowest_level.to_string(),
            "highest_level": stats.houses.highest_level.to_string(),
        })
    } else {
        json!({ "total": 0 })
    };

    json!({
        "population": {
            "total": stats.population.total,
            "employed": stats.population.employed,
            "unemployed": stats.population.unemployed,
        },
        "workers": {
            "total": stats.workers.total,
            "min_required": stats.workers.min_required,
            "max_employed": stats.workers.max_employed,
            "buildings_below_min": stats.workers.buildings_below_min,
            "buildings_below_max": stats.workers.buildings_below_max,
        },
        "treasury": {
            "gold": stats.treasury.gold_units_total,
            "gold_in_buildings": stats.treasury.gold_units_in_buildings,
            "tax_generated": stats.treasury.tax_generated,
            "tax_available": stats.treasury.tax_available,
            "tax_collected": stats.treasury.tax_collected,
        },
        "houses": houses,
        "resources": resource_stock_json(&stats.resources.all),
    })
}

fn tile_json(context: &CommandContext, tile: &Tile) -> Value {
    let mut reply = json!({
        "name": tile.name(),
        "category": tile.category_name(),
        "kind": tile.kind().to_string(),
        "cell": cell_json(tile.base_cell()),
        "variation": tile.variation_index(),
        "variation_name": tile.variation_name(),
    });

    if tile.game_object_handle().is_valid() {
        if let Some(id) = object_id_at(context, tile.base_cell()) {
            reply["object"] = id.to_json();
        }
    }

    reply
}

fn cmd_tile_at(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let cell = params.cell.0;
    context.require_cell_in_map(cell)?;

    let tile_map = context.session.tile_map();

    let terrain = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).map(|tile| tile_json(context, tile));

    // Units can stack on one cell.
    let mut objects = Vec::new();
    let mut next = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects);
    while let Some(tile) = next {
        objects.push(tile_json(context, tile));
        next = tile_map.next_tile(tile);
    }

    done(json!({ "cell": cell_json(cell), "terrain": terrain, "objects": objects }))
}

fn cmd_undo(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    if !undo_redo::can_undo() {
        return Err(CommandError::failed("Nothing to undo."));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, undo_redo::undo);

    refresh_world_stats(context);
    done(json!({ "can_undo": undo_redo::can_undo(), "can_redo": undo_redo::can_redo() }))
}

fn cmd_redo(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    if !undo_redo::can_redo() {
        return Err(CommandError::failed("Nothing to redo."));
    }

    let delta_time_secs = context.engine.frame_clock().delta_time();
    context.session.with_sim_context(delta_time_secs, undo_redo::redo);

    refresh_world_stats(context);
    done(json!({ "can_undo": undo_redo::can_undo(), "can_redo": undo_redo::can_redo() }))
}
