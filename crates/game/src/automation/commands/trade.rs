// Trade commands: the partners a game trades with, their routes, terms and limits (sim::trade,
// docs/plans/trade.md).

use engine::ui::text::{self, UiTextCategory};
use serde::Deserialize;
use serde_json::{Value, json};

use super::cell_json;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    building::BuildingKind,
    config::GameConfigs,
    sim::{
        resources::ResourceKind,
        trade::{TradeLimit, TradePartner},
    },
    trade::{self, GoodTerms},
    unit::task::{UnitTaskShip, UnitTaskTrade},
    world_map::config::traded_good,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "trade.state",
        help: "Whether the game trades, the world map its partners are on, and each partner: its city and name, \
               relations, route (id, kind: land, river or sea; the city it runs from), whether it's open, what \
               opening it costs, how often it comes, the ordinal month its next caravan or ship sets out \
               (next_arrival, null while closed), where it comes in (entry, null: the default), and the goods \
               it sells and buys (good, price, per_year, this year's sold or bought and what's left, the \
               price's offset) with the player's limit for each (off, or {\"import\": N}: until the city \
               holds N, or {\"export\": N}: what's above N). Then the court's requests under way, their log and \
               how many were met.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "trade.set",
        help: "Set a partner's limit for a good (import=N: buy until the city holds N; export=N: sell what's above \
               N; off=true), open or close its route (open: free here, unlike the world map's Open the route), \
               a good's price offset (-0.9 to 1: its price is the base × (1 + offset)) or the ordinal month its \
               next caravan or ship sets out; or turn the game's trade on or off. Reports trade.state.",
        params: &[
            ParamDef { name: "enabled", kind: "bool", required: false, help: "Whether the game trades." },
            ParamDef { name: "partner", kind: "string", required: false, help: "The partner's city id." },
            ParamDef { name: "good", kind: "string", required: false, help: "A good's name, e.g. Wood." },
            ParamDef { name: "import", kind: "number", required: false, help: "Import until the city holds this many." },
            ParamDef { name: "export", kind: "number", required: false, help: "Export what's above this many." },
            ParamDef { name: "off", kind: "bool", required: false, help: "true: no trade in the good." },
            ParamDef { name: "open", kind: "bool", required: false, help: "Open or close the partner's route." },
            ParamDef { name: "offset", kind: "number", required: false, help: "The good's price offset." },
            ParamDef {
                name: "next_arrival",
                kind: "number",
                required: false,
                help: "The ordinal month the next caravan or ship sets out (an open route's).",
            },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "trade.open",
        help: "The player opens a partner's route, as the world map's Open the route does: it costs the route's \
               open_cost (the Routes account), and its first caravan sets out first_arrival_months later. Fails \
               while trade is off, for a city that isn't a partner or whose route is open, for a river or sea \
               route without a dock, and when the treasury can't pay. Reports the cost and trade.state.",
        params: &[ParamDef { name: "partner", kind: "string", required: true, help: "The partner's city id." }],
        run: cmd_open,
    },
    CommandDef {
        name: "trade.arrive_now",
        help: "Sends a partner's caravan, or its ship by river or sea, now, whatever its schedule (its next one stays \
               due when it was): a caravan comes in at the partner's entry with the goods the import limits want and \
               trades store by store; a ship comes in at its entry on the water (else the map's edge nearest a \
               working dock) and moors at a dock while its dockers carry the goods. Fails while trade is off or the \
               route is closed, for a ship without a working dock in reach, and when there's nothing to trade. \
               Reports trade.state, its caravan in `caravans` or its ship in `ships`.",
        params: &[ParamDef { name: "partner", kind: "string", required: true, help: "The partner's city id." }],
        run: cmd_arrive_now,
    },
];

fn cmd_open(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        partner: String,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let delta_time_secs = context.engine.frame_clock().delta_time();
    let result =
        context.session.with_sim_context(delta_time_secs, |sim_context| trade::open_route(sim_context, &params.partner));
    let cost = result.map_err(|err| {
        let message = match err {
            trade::OpenRouteError::TradeOff => "This game doesn't trade.".to_string(),
            trade::OpenRouteError::NotAPartner => format!("'{}' isn't a trade partner of this game.", params.partner),
            trade::OpenRouteError::AlreadyOpen => format!("The route to '{}' is open already.", params.partner),
            trade::OpenRouteError::NeedsDock => format!("The route to '{}' goes by water: it needs a dock.", params.partner),
            trade::OpenRouteError::CantAfford(cost) => format!("The treasury can't pay the route's {cost} gold."),
        };
        CommandError::invalid_params(message)
    })?;
    let mut state = state_json(context);
    state["cost"] = json!(cost);
    done(state)
}

fn cmd_arrive_now(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        partner: String,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    let delta_time_secs = context.engine.frame_clock().delta_time();
    let result = context.session.with_sim_context(delta_time_secs, |sim_context| trade::send(sim_context, &params.partner));
    result.map_err(|err| {
        let message = match err {
            trade::NotSent::NotTrading => {
                format!("'{}' isn't trading now: trade is off, or its route is closed.", params.partner)
            }
            trade::NotSent::NoDock => {
                format!("'{}' trades by water, and no working dock's water reaches the map's edge.", params.partner)
            }
            trade::NotSent::NothingToTrade => {
                format!("Nothing to trade with '{}': no import below its limit, no export above it.", params.partner)
            }
            trade::NotSent::NoEntry => format!("A caravan or ship from '{}' couldn't come in at its entry.", params.partner),
        };
        CommandError::invalid_params(message)
    })?;
    done(state_json(context))
}

fn cmd_state(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {}

    let _params: Params = parse_params(params)?;
    context.require_map()?;
    done(state_json(context))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        enabled: Option<bool>,
        partner: Option<String>,
        good: Option<String>,
        import: Option<u32>,
        export: Option<u32>,
        off: Option<bool>,
        open: Option<bool>,
        offset: Option<f32>,
        next_arrival: Option<u32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    let limits = [params.import.is_some(), params.export.is_some(), params.off == Some(true)];
    if limits.iter().filter(|&&given| given).count() > 1 {
        return Err(CommandError::invalid_params("Give one of import, export and off=true.".to_string()));
    }
    let limit = match (params.import, params.export, params.off) {
        (Some(amount), _, _) => Some(TradeLimit::Import(amount)),
        (_, Some(amount), _) => Some(TradeLimit::Export(amount)),
        (_, _, Some(true)) => Some(TradeLimit::Off),
        _ => None,
    };
    let needs_partner = limit.is_some() || params.open.is_some() || params.offset.is_some() || params.next_arrival.is_some();
    let partner = match &params.partner {
        Some(city) => {
            if context.session.sim().trade().partner(city).is_none() {
                return Err(CommandError::not_found(format!("'{city}' isn't a trade partner of this game.")));
            }
            Some(city.clone())
        }
        None if needs_partner => return Err(CommandError::invalid_params("That needs a partner.".to_string())),
        None => None,
    };
    let good = match &params.good {
        Some(name) => {
            let Some(good) = traded_good(name) else {
                return Err(CommandError::invalid_params(format!("'{name}' isn't a good the game trades.")));
            };
            Some(good)
        }
        None if limit.is_some() || params.offset.is_some() => {
            return Err(CommandError::invalid_params("A limit or an offset needs a good.".to_string()));
        }
        None => None,
    };
    if let Some(offset) = params.offset
        && !(offset.is_finite() && (-0.9..=1.0).contains(&offset))
    {
        return Err(CommandError::invalid_params(format!("offset must be from -0.9 to 1, got {offset}.")));
    }
    if params.next_arrival.is_some() {
        let city = partner.as_deref().unwrap_or_default();
        let open = context.session.sim().trade().partner(city).is_some_and(|partner| partner.open);
        if !open && params.open != Some(true) {
            return Err(CommandError::invalid_params(format!("'{city}''s route is closed (open=true).")));
        }
    }

    let sim = &GameConfigs::get().sim;
    let now = context.session.sim().calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month();
    let trade_state = context.session.sim_mut().trade_mut();
    if let Some(enabled) = params.enabled {
        trade_state.set_enabled(enabled);
    }
    if let Some(city) = &partner {
        match params.open {
            Some(true) => {
                trade_state.open_route(city, now, &sim.trade);
            }
            Some(false) => {
                trade_state.close_route(city);
            }
            None => {}
        }
        if let (Some(good), Some(limit)) = (good, limit) {
            trade_state.set_limit(city, good, limit);
        }
        if let Some(partner) = trade_state.partner_mut(city) {
            if let (Some(good), Some(offset)) = (good, params.offset) {
                partner.price_offsets[good.index()] = offset;
            }
            if let Some(month) = params.next_arrival {
                partner.next_arrival = Some(month);
            }
        }
    }
    done(state_json(context))
}

pub(super) fn limit_json(limit: TradeLimit) -> Value {
    match limit {
        TradeLimit::Off => json!("off"),
        TradeLimit::Import(amount) => json!({ "import": amount }),
        TradeLimit::Export(amount) => json!({ "export": amount }),
    }
}

fn goods_json(goods: &[GoodTerms], partner: &TradePartner, traded: &[u32]) -> Vec<Value> {
    goods
        .iter()
        .map(|terms| {
            let done = traded[terms.good.index()];
            json!({
                "good": terms.good.name(),
                "price": terms.price,
                "per_year": terms.per_year,
                "traded": done,
                "left": terms.per_year.saturating_sub(done),
                "offset": partner.price_offset(terms.good),
                "limit": limit_json(partner.limit(terms.good)),
            })
        })
        .collect()
}

fn state_json(context: &CommandContext) -> Value {
    let state = context.session.sim().trade();
    let partners: Vec<Value> = state
        .partners()
        .iter()
        .map(|partner| {
            let terms = trade::terms(state, partner.city());
            let limits: serde_json::Map<String, Value> = ResourceKind::all()
                .iter()
                .filter(|good| !partner.limit(*good).is_off())
                .map(|good| (good.name().to_string(), limit_json(partner.limit(good))))
                .collect();
            json!({
                "city": partner.city(),
                "name": terms.as_ref().map(|terms| text::get(UiTextCategory::Realm, &terms.name_key).to_string()),
                "relation": terms.as_ref().map(|terms| terms.relation),
                "route": terms.as_ref().map(|terms| json!({
                    "id": terms.route,
                    "kind": terms.route_kind.to_string(),
                    "from": terms.from,
                })),
                "open": partner.open,
                "open_cost": terms.as_ref().map(|terms| terms.open_cost),
                "every_months": terms.as_ref().map(|terms| terms.every_months),
                "next_arrival": partner.next_arrival,
                "entry": partner.def.entry,
                "sells": terms.as_ref().map(|terms| goods_json(&terms.sells, partner, &partner.sold)),
                "buys": terms.as_ref().map(|terms| goods_json(&terms.buys, partner, &partner.bought)),
                "limits": limits,
            })
        })
        .collect();
    json!({
        "enabled": state.is_enabled(),
        "world_map": state.world_map(),
        "partners": partners,
        "requests": state.requests().iter().map(|request| json!({
            "id": request.id,
            "from": request.from,
            "good": request.good.name(),
            "amount": request.amount,
            "opened": request.opened,
            "due": request.due,
        })).collect::<Vec<_>>(),
        "request_log": state.request_log().iter().map(|record| json!({
            "id": record.request.id,
            "good": record.request.good.name(),
            "amount": record.request.amount,
            "closed": record.closed,
            "met": record.met,
        })).collect::<Vec<_>>(),
        "requests_met": state.requests_met(),
        "caravans": caravans_json(context),
        "ships": ships_json(context),
        "docks": docks_json(context),
    })
}

fn counts_json(counts: &[u32]) -> Value {
    let map: serde_json::Map<String, Value> = ResourceKind::all()
        .iter()
        .filter(|good| counts[good.index()] != 0)
        .map(|good| (good.name().to_string(), json!(counts[good.index()])))
        .collect();
    Value::Object(map)
}

// The ships in the city: partner, state, where it is and came in, what it carries, sold and bought,
// the gold, the dock it's bound for or moored at (its base cell) and how long it's been moored.
fn ships_json(context: &CommandContext) -> Vec<Value> {
    let task_manager = context.session.sim().task_manager();
    let world = context.session.world();
    let mut ships = Vec::new();
    world.for_each_unit(|unit| {
        if let Some(ship) = unit.current_task_as::<UnitTaskShip>(task_manager) {
            let dock =
                ship.dock.and_then(|dock| world.find_building(dock.kind, dock.id)).map(|dock| cell_json(dock.base_cell()));
            ships.push(json!({
                "cell": cell_json(unit.cell()),
                "partner": ship.partner,
                "state": format!("{:?}", ship.state),
                "entry": cell_json(ship.entry),
                "cargo": counts_json(&ship.cargo),
                "sold": counts_json(&ship.sold),
                "bought": counts_json(&ship.bought),
                "paid": ship.paid,
                "earned": ship.earned,
                "dock": dock,
                "moored_secs": ship.moored_secs,
                "turned_back": ship.turned_back,
            }));
        }
        true
    });
    ships
}

// The docks: their base cell, whether they work, the ship at the berth (its partner), the dockers
// out and their errands.
fn docks_json(context: &CommandContext) -> Vec<Value> {
    let task_manager = context.session.sim().task_manager();
    let world = context.session.world();
    let tile_map = context.session.tile_map();
    let mut docks = Vec::new();
    world.for_each_building(BuildingKind::Dock, |dock| {
        let Some(berth) = trade::ships::berth_of(dock) else {
            return true;
        };
        let ship = berth
            .ship
            .and_then(|id| world.find_unit(id))
            .and_then(|unit| unit.current_task_as::<UnitTaskShip>(task_manager))
            .map(|ship| ship.partner.clone());
        docks.push(json!({
            "cell": cell_json(dock.base_cell()),
            "variation": tile_map
                .find_tile(dock.base_cell(), crate::tile::TileKind::Building)
                .map(|tile| tile.variation_name()),
            "working": dock.is_operational(),
            "ship": ship,
            "dockers_out": berth.dockers_out(),
            "errands": berth.errands().map(|errand| json!({
                "good": errand.good.name(),
                "count": errand.count,
                "unloading": errand.unloading,
            })).collect::<Vec<_>>(),
        }));
        true
    });
    docks
}

// The caravans in the city: partner, state, where it came in, what it carries, sold and bought, the
// gold, its stops left and the stores it traded at.
fn caravans_json(context: &CommandContext) -> Vec<Value> {
    let task_manager = context.session.sim().task_manager();
    let goods = |counts: &[u32]| {
        let map: serde_json::Map<String, Value> = ResourceKind::all()
            .iter()
            .filter(|good| counts[good.index()] != 0)
            .map(|good| (good.name().to_string(), json!(counts[good.index()])))
            .collect();
        Value::Object(map)
    };
    let mut caravans = Vec::new();
    context.session.world().for_each_unit(|unit| {
        if let Some(caravan) = unit.current_task_as::<UnitTaskTrade>(task_manager) {
            caravans.push(json!({
                "cell": cell_json(unit.cell()),
                "partner": caravan.partner,
                "state": format!("{:?}", caravan.state),
                "entry": cell_json(caravan.entry),
                "cargo": goods(&caravan.cargo),
                "sold": goods(&caravan.sold),
                "bought": goods(&caravan.bought),
                "paid": caravan.paid,
                "earned": caravan.earned,
                "stops_left": caravan.stops_left,
                "stores_visited": caravan.visited.len(),
            }));
        }
        true
    });
    caravans
}
