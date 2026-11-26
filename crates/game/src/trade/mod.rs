// Trade (docs/plans/trade.md): the partners a game trades with and their terms, read from the world
// map files and the mission's (or custom map's) `trade` block. The state is sim::trade::CityTrade.
//
// Phase 0 holds the data and the state; caravans (Phase 1) trade at the stores, store by store
// (`trade_at_store`), and ships (Phase 2) at the docks (`ships`), sent each month by `on_new_month`.
// Prices (Phase 3) and court requests (Phase 4) build on it.

pub mod requests;
pub mod ships;

use common::{coords::Cell, mem::SingleThreadStatic};
use engine::{
    log,
    ui::{
        sound::UiSoundKey,
        text::{self, UiTextCategory},
    },
};

use crate::{
    building::Building,
    campaign::config::{CampaignConfigs, MissionPartnerDef, MissionRequestDef, MissionTradeDef},
    cheats,
    config::{GameConfigs, TradeRules},
    messages::{Message, MessageKind, MessageTarget},
    pathfind::Node,
    realm::Realm,
    sim::{
        SimContext,
        SimContextMut,
        calendar::CalendarDate,
        finance::LedgerCategory,
        resources::{RESOURCE_KIND_COUNT, ResourceKind},
        trade::{CityTrade, TradeLimit, TradePartner},
    },
    tile::{TileFlags, TileKind},
    unit::{
        Unit,
        config::UnitConfigKey,
        task::{TRADE_STORES, TRADER_NODE_KINDS, UnitTaskShip, UnitTaskTrade},
    },
    world::{city_stock::CityStock, object::Spawner},
    world_map::{
        WorldMapConfigs,
        config::{RouteKind, TradeGoodDef, WorldCityDef, WorldMapDef},
        provider::TradeOffer,
    },
};

// ----------------------------------------------
// Terms
// ----------------------------------------------

// A partner's terms as the game trades on them: the world map's, with the mission's overrides.
#[derive(Clone, PartialEq, Debug)]
pub struct PartnerTerms {
    pub city: String,
    pub name_key: String, // Realm key.
    pub relation: u8,
    pub route: String, // The route's id on the world map.
    pub route_kind: RouteKind,
    pub from: String,    // The player's city the route runs from.
    pub line: String,    // Menu key with {city}: "Hill track from {city}".
    pub carrier: String, // Menu key: "caravans".
    pub every_months: u32,
    pub open_cost: u32,
    // The goods the game trades with it; goods it sells or buys that the game doesn't have are
    // shown on the world map only.
    pub sells: Vec<GoodTerms>,
    pub buys: Vec<GoodTerms>,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct GoodTerms {
    pub good: ResourceKind,
    pub price: u32, // Its base price, gold a unit.
    pub per_year: u32,
}

impl PartnerTerms {
    // A caravan comes by land; a ship by river or sea, and only to a dock.
    #[inline]
    pub fn by_water(&self) -> bool {
        self.route_kind != RouteKind::Land
    }

    pub fn sells(&self, good: ResourceKind) -> Option<&GoodTerms> {
        self.sells.iter().find(|terms| terms.good == good)
    }

    pub fn buys(&self, good: ResourceKind) -> Option<&GoodTerms> {
        self.buys.iter().find(|terms| terms.good == good)
    }
}

fn traded(goods: &[TradeGoodDef]) -> Vec<GoodTerms> {
    goods
        .iter()
        .filter_map(|good| {
            Some(GoodTerms { good: good.resource()?, price: good.price, per_year: good.per_year.unwrap_or(0) })
        })
        .collect()
}

// A partner's terms on `map`: its city's trade terms with `def`'s overrides. None for a city that
// isn't on the map, has no trade terms or no route, or whose route the map lacks.
pub fn terms_on(map: &WorldMapDef, def: &MissionPartnerDef) -> Option<PartnerTerms> {
    let city: &WorldCityDef = map.city(&def.city)?;
    let trade = city.trade.as_ref()?;
    let route = trade.route.as_ref()?;
    let route_kind = map.route(&route.route)?.kind;
    Some(PartnerTerms {
        city: city.id.clone(),
        name_key: city.name.clone(),
        relation: trade.relation,
        route: route.route.clone(),
        route_kind,
        from: route.from.clone(),
        line: route.line.clone(),
        carrier: route.carrier.clone(),
        every_months: def.every_months.unwrap_or(route.every_months).max(1),
        open_cost: def.open_cost.unwrap_or(route.open_cost),
        sells: traded(def.sells.as_deref().unwrap_or(&trade.sells)),
        buys: traded(def.buys.as_deref().unwrap_or(&trade.buys)),
    })
}

// The world map file of that name, once the configs have loaded.
pub fn world_map(name: &str) -> Option<&'static WorldMapDef> {
    if !WorldMapConfigs::is_initialized() {
        return None;
    }
    WorldMapConfigs::get().maps.get(name)
}

// A partner's terms in this game. None for one the game doesn't trade with.
pub fn terms(trade: &CityTrade, city: &str) -> Option<PartnerTerms> {
    let partner = trade.partner(city)?;
    terms_on(world_map(trade.world_map())?, &partner.def)
}

// A city's name on a world map: a world city's, or a mission city's of a campaign that shows that
// map. The id itself if neither has it.
pub fn place_name(world_map_name: &str, city: &str) -> String {
    let key = world_map(world_map_name).and_then(|map| map.city(city)).map(|city| city.name.clone()).or_else(|| {
        if !CampaignConfigs::is_initialized() {
            return None;
        }
        CampaignConfigs::get()
            .campaigns
            .iter()
            .filter(|campaign| campaign.world_map_name() == world_map_name)
            .flat_map(|campaign| campaign.missions.iter().filter_map(|mission| mission.city.as_ref()))
            .find(|mission_city| mission_city.id == city)
            .map(|mission_city| mission_city.name.clone())
    });
    key.map_or_else(|| city.to_string(), |key| text::get(UiTextCategory::Realm, &key).to_string())
}

// A partner's goods on the world map's side panel (docs/plans/trade.md, §9): what it sells and what it
// buys in this game (the mission's goods, else its city's), those the game trades at their price now
// with the way trade has moved it, the others (shown only) without a price. None for a city the game
// doesn't trade with.
pub fn panel_offers(trade: &CityTrade, city: &str, faith_scale: f32) -> Option<(Vec<TradeOffer>, Vec<TradeOffer>)> {
    let partner = trade.partner(city)?;
    let map = world_map(trade.world_map())?;
    let terms = terms_on(map, &partner.def)?;
    let city_trade = map.city(city)?.trade.as_ref()?;
    let offers = |goods: &[TradeGoodDef], side: TradeSide| -> Vec<TradeOffer> {
        goods
            .iter()
            .map(|def| {
                let good = def.resource().and_then(|kind| match side {
                    TradeSide::Import => terms.sells(kind),
                    TradeSide::Export => terms.buys(kind),
                });
                let (price, trend) = good.map_or((None, PriceTrend::Steady), |good| {
                    let price = price_now(side, faith_scale, &terms, partner, good);
                    (Some(price), price_trend(side, faith_scale, &terms, partner, good))
                });
                TradeOffer { good: def.display_name(), price, trend }
            })
            .collect()
    };
    let sells = offers(partner.def.sells.as_deref().unwrap_or(&city_trade.sells), TradeSide::Import);
    let buys = offers(partner.def.buys.as_deref().unwrap_or(&city_trade.buys), TradeSide::Export);
    Some((sells, buys))
}

// A partner's route as the player reads it: "Hill track from Chợ Đèo".
pub fn route_line(trade: &CityTrade, city: &str) -> Option<String> {
    let terms = terms(trade, city)?;
    let from = place_name(trade.world_map(), &terms.from);
    Some(text::format(UiTextCategory::Menu, &terms.line, &[("city", &from)]))
}

// Its route's state: "Open · caravans every 4 months", "Closed · opens for 400 gold", "Closed · needs a
// dock" (a river or sea route while the city has no dock: `has_dock`, ships::has_dock).
pub fn route_detail(trade: &CityTrade, city: &str, has_dock: bool) -> Option<String> {
    let partner = trade.partner(city)?;
    let terms = terms(trade, city)?;
    Some(if partner.open {
        let carrier = text::get(UiTextCategory::Menu, &terms.carrier);
        text::format_count(UiTextCategory::Menu, "world_map.route_detail.open", i64::from(terms.every_months), &[(
            "carrier", carrier,
        )])
    } else if terms.by_water() && !has_dock {
        text::get(UiTextCategory::Menu, "world_map.route_detail.needs_dock").to_string()
    } else if terms.open_cost == 0 {
        text::get(UiTextCategory::Menu, "world_map.route_detail.closed_free").to_string()
    } else {
        text::format(UiTextCategory::Menu, "world_map.route_detail.closed_cost", &[("gold", &terms.open_cost.to_string())])
    })
}

// ----------------------------------------------
// Starting a game's trade
// ----------------------------------------------

// Starts a mission's or a custom game's trade on the world map `world_map` (an ordinal month `now`),
// leaving out, with a warning, the partners it has no terms for.
pub fn start(state: &mut CityTrade, world_map_name: &str, block: &MissionTradeDef, now: u32, rules: &TradeRules) {
    let map = world_map(world_map_name);
    // A request that gives no Legitimacy takes the rules' if the court asks, none if another city
    // does (docs/plans/ratings.md, Q4).
    let court_legitimacy = GameConfigs::get().sim.ratings.request_legitimacy;
    let legitimacy = |def: &MissionRequestDef| {
        if map.is_some_and(|map| requests::is_court_on(map, &def.from)) { court_legitimacy } else { 0 }
    };
    let has_terms = |def: &MissionPartnerDef| map.is_some_and(|map| terms_on(map, def).is_some());
    let left_out = state.start(world_map_name, block, has_terms, legitimacy, now, rules);
    for city in left_out {
        log::warning!(
            log::channel!("trade"),
            "Trade partner '{city}' is left out: the world map '{world_map_name}' has no trade terms for it, or it's named twice."
        );
    }
}

// The world map a realm's custom games trade on: its first visible campaign's.
pub fn realm_world_map(realm: Realm) -> Option<String> {
    if !CampaignConfigs::is_initialized() {
        return None;
    }
    let configs = CampaignConfigs::get();
    configs
        .visible()
        .find(|(_, campaign)| campaign.realm == realm)
        .map(|(_, campaign)| campaign.world_map_name().to_string())
}

// A custom game's trade block when its map names none: every partner of the world map with a
// route, by land, river or sea, closed (docs/plans/trade.md, §1). River and sea routes open once the
// city has a dock.
pub fn map_partners(map: &WorldMapDef) -> MissionTradeDef {
    let partners = map
        .cities
        .iter()
        .filter_map(|city| {
            let route = city.trade.as_ref()?.route.as_ref()?;
            map.route(&route.route)?;
            Some(MissionPartnerDef {
                city: city.id.clone(),
                open: false,
                entry: None,
                open_cost: None,
                every_months: None,
                sells: None,
                buys: None,
            })
        })
        .collect();
    MissionTradeDef { enabled: None, partners, requests: Vec::new() }
}

// A custom game's trade: its world map (the realm's) and its block, the map's own `trade` (maps.json)
// or else the realm's partners (map_partners). None for a realm without a world map.
pub fn custom_game_trade(realm: Realm, map_trade: Option<&MissionTradeDef>) -> Option<(String, MissionTradeDef)> {
    let name = realm_world_map(realm)?;
    let block = match map_trade {
        Some(block) => block.clone(),
        None => world_map(&name).map(map_partners).unwrap_or_default(),
    };
    Some((name, block))
}

// After a load: drops the partners the world map no longer has terms for, with a warning.
pub fn sync_on_load(state: &mut CityTrade) {
    if state.partners().is_empty() {
        return;
    }
    let world_map_name = state.world_map().to_string();
    // Before the configs load (a tool that loads a save alone), the partners stay as saved.
    if !WorldMapConfigs::is_initialized() {
        return;
    }
    let map = world_map(&world_map_name);
    let dropped = state.sync_partners(|def| map.is_some_and(|map| terms_on(map, def).is_some()));
    for city in dropped {
        log::warning!(
            log::channel!("trade"),
            "Trade partner '{city}' is dropped: the world map '{world_map_name}' has no trade terms for it any more."
        );
    }
}

// ----------------------------------------------
// What the city holds, and prices
// ----------------------------------------------

// What the city holds of each good, for the limits: its storage yards and granaries, staffed or not.
pub type Holdings = [u32; RESOURCE_KIND_COUNT];

pub fn holdings(context: &SimContext) -> Holdings {
    let mut held = [0; RESOURCE_KIND_COUNT];
    for good in ResourceKind::all().iter().filter(|good| *good != ResourceKind::Gold) {
        held[good.index()] = CityStock::available_in(context.world(), TRADE_STORES, good);
    }
    held
}

// Which way a good goes: the city imports what a partner sells, and exports what it buys.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TradeSide {
    Import,
    Export,
}

impl TradeSide {
    pub fn name(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Export => "export",
        }
    }
}

// Which way trade has moved a price: up, down or not at all, as the player would read it (the price
// shown against the one it would be without its offset).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum PriceTrend {
    #[default]
    Steady,
    Up,
    Down,
}

impl PriceTrend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    // The mark shown after a price: none while it's steady.
    pub fn arrow(self) -> &'static str {
        match self {
            Self::Steady => "",
            Self::Up => "▲",
            Self::Down => "▼",
        }
    }
}

// A good's price as `side` trades it (docs/plans/trade.md, §7), gold a unit, at least 1: its base
// price × (1 + `offset`); better for the city by `relation_step` a relations level above the middle
// (3), worse below it; and better again while trade comes easily (`faith_scale`,
// CityFaith::trade_scale). Rounded once, at the end.
fn unit_price(side: TradeSide, faith_scale: f32, relation: u8, offset: f32, base: u32) -> u32 {
    let rules = &GameConfigs::get().sim.trade;
    let levels = f32::from(relation) - f32::from(MIDDLE_RELATION);
    let price = base as f32 * (1.0 + offset);
    let price = match side {
        TradeSide::Import => price * (1.0 - rules.relation_step * levels) / faith_scale.max(0.01),
        TradeSide::Export => price * (1.0 + rules.relation_step * levels) * faith_scale.max(0.0),
    };
    price.round().max(1.0) as u32
}

// The relations level at which prices are their base (Neutral).
const MIDDLE_RELATION: u8 = 3;

// A good's price with a partner now, as `side` trades it.
pub fn price_now(side: TradeSide, faith_scale: f32, terms: &PartnerTerms, partner: &TradePartner, good: &GoodTerms) -> u32 {
    unit_price(side, faith_scale, terms.relation, partner.price_offset(good.good), good.price)
}

// What the city pays a partner for one of its goods now.
pub fn import_price(faith_scale: f32, terms: &PartnerTerms, partner: &TradePartner, good: &GoodTerms) -> u32 {
    price_now(TradeSide::Import, faith_scale, terms, partner, good)
}

// What a partner pays for one of the city's goods now.
pub fn export_price(faith_scale: f32, terms: &PartnerTerms, partner: &TradePartner, good: &GoodTerms) -> u32 {
    price_now(TradeSide::Export, faith_scale, terms, partner, good)
}

// Which way trade has moved a good's price with a partner: its price now against the one it would be
// without its offset.
pub fn price_trend(
    side: TradeSide,
    faith_scale: f32,
    terms: &PartnerTerms,
    partner: &TradePartner,
    good: &GoodTerms,
) -> PriceTrend {
    let now = price_now(side, faith_scale, terms, partner, good);
    let unmoved = unit_price(side, faith_scale, terms.relation, 0.0, good.price);
    match now.cmp(&unmoved) {
        std::cmp::Ordering::Greater => PriceTrend::Up,
        std::cmp::Ordering::Less => PriceTrend::Down,
        std::cmp::Ordering::Equal => PriceTrend::Steady,
    }
}

// How many of a good the city still wants from a partner: below its import limit, within what the
// partner sells it this year.
pub fn import_wanted(partner: &TradePartner, terms: &GoodTerms, holdings: &Holdings) -> u32 {
    match partner.limit(terms.good) {
        TradeLimit::Import(limit) => {
            let left = terms.per_year.saturating_sub(partner.sold[terms.good.index()]);
            limit.saturating_sub(holdings[terms.good.index()]).min(left)
        }
        _ => 0,
    }
}

// How many of a good the city has to sell a partner: above its export limit, within what the partner
// buys this year.
pub fn export_offered(partner: &TradePartner, terms: &GoodTerms, holdings: &Holdings) -> u32 {
    match partner.limit(terms.good) {
        TradeLimit::Export(limit) => {
            let left = terms.per_year.saturating_sub(partner.bought[terms.good.index()]);
            holdings[terms.good.index()].saturating_sub(limit).min(left)
        }
        _ => 0,
    }
}

// How many goods at `price` each the treasury can pay for (none while in debt).
pub(crate) fn affordable(context: &SimContext, price: u32) -> u32 {
    if cheats::get().ignore_tile_cost {
        return u32::MAX;
    }
    (context.finance().balance().max(0) / i64::from(price.max(1))).min(i64::from(u32::MAX)) as u32
}

// ----------------------------------------------
// Caravans
// ----------------------------------------------

// Whether the game trades with that partner now: trade on and its route open.
pub fn is_trading_with(context: &SimContext, city: &str) -> bool {
    let trade = context.trade();
    trade.is_enabled() && trade.partner(city).is_some_and(|partner| partner.open)
}

// What a caravan from `partner` sets out with: each good it sells that the city wants, within its
// capacity.
pub fn caravan_cargo(
    partner: &TradePartner,
    terms: &PartnerTerms,
    holdings: &Holdings,
    capacity: u32,
) -> [u32; RESOURCE_KIND_COUNT] {
    let mut cargo = [0; RESOURCE_KIND_COUNT];
    let mut room = capacity;
    for good in &terms.sells {
        let take = import_wanted(partner, good, holdings).min(room);
        cargo[good.good.index()] = take;
        room -= take;
    }
    cargo
}

// Whether the city has something to sell a partner now.
pub fn has_exports(partner: &TradePartner, terms: &PartnerTerms, holdings: &Holdings) -> bool {
    terms.buys.iter().any(|good| export_offered(partner, good, holdings) != 0)
}

// Whether a caravan can sell or buy something at `store` now.
pub fn has_business_at(context: &SimContext, caravan: &UnitTaskTrade, store: &Building, holdings: &Holdings) -> bool {
    let trade = context.trade();
    let (Some(partner), Some(terms)) = (trade.partner(&caravan.partner), terms(trade, &caravan.partner)) else {
        return false;
    };
    if !partner.open || !trade.is_enabled() {
        return false;
    }
    let sells = terms.sells.iter().any(|good| {
        caravan.carries(good.good) != 0
            && (caravan.gift || import_wanted(partner, good, holdings) != 0)
            && store.receivable_resources(good.good) != 0
    });
    let buys = caravan.room() != 0
        && terms
            .buys
            .iter()
            .any(|good| export_offered(partner, good, holdings) != 0 && store.available_resources(good.good) != 0);
    sells || buys
}

// A caravan's visit to a store (docs/plans/trade.md, §4): first it sells what the store takes and the
// city still wants, as far as the treasury can pay (`Imports`); then it buys what the city has above
// its export limits and the store holds, within its room (`Exports`). Each good counts toward the
// partner's amounts this year.
pub fn trade_at_store(context: &SimContextMut, store: &mut Building, unit: &mut Unit) {
    let Some(caravan) = unit.current_task_as_mut::<UnitTaskTrade>(context.task_manager_mut()) else {
        return;
    };
    let Some(terms) = terms(context.trade(), &caravan.partner) else {
        return;
    };
    if !is_trading_with(context, &caravan.partner) {
        return;
    }
    let mut held = holdings(context);
    let faith_scale = context.faith().trade_scale();
    let trade = context.trade_mut();
    let Some(partner) = trade.partner_mut(&caravan.partner) else {
        return;
    };

    // Envoys' gifts: whatever the stores take, free, outside the limits and the year's amounts.
    if caravan.gift {
        for good in &terms.sells {
            let kind = good.good;
            let count = caravan.carries(kind).min(store.receivable_resources(kind));
            let received = store.receive_resources(kind, count);
            caravan.cargo[kind.index()] -= received;
            caravan.sold[kind.index()] += received;
        }
        return;
    }

    let rules = &GameConfigs::get().sim.trade;
    for good in &terms.sells {
        let kind = good.good;
        let price = import_price(faith_scale, &terms, partner, good);
        let count = caravan
            .carries(kind)
            .min(import_wanted(partner, good, &held))
            .min(store.receivable_resources(kind))
            .min(affordable(context, price));
        if count == 0 {
            continue;
        }
        let received = store.receive_resources(kind, count);
        caravan.cargo[kind.index()] -= received;
        caravan.sold[kind.index()] += received;
        partner.record_sold(kind, received, rules);
        held[kind.index()] += received;
        let gold = received * price;
        context.finance_mut().spend(LedgerCategory::Imports, gold);
        caravan.paid += gold;
    }

    for good in &terms.buys {
        let kind = good.good;
        let count = caravan.room().min(export_offered(partner, good, &held)).min(store.available_resources(kind));
        if count == 0 {
            continue;
        }
        let price = export_price(faith_scale, &terms, partner, good);
        let removed = store.remove_resources(kind, count);
        caravan.bought[kind.index()] += removed;
        partner.record_bought(kind, removed, rules);
        held[kind.index()] -= removed.min(held[kind.index()]);
        let gold = removed * price;
        context.finance_mut().earn(LedgerCategory::Exports, gold);
        caravan.earned += gold;
    }
}

// The partner whose envoys bring gifts (faith's Envoys): the first partner by land whose route is
// open and that sells a good the game trades.
pub fn envoys_partner(context: &SimContext) -> Option<String> {
    let trade = context.trade();
    if !trade.is_enabled() {
        return None;
    }
    trade.partners().iter().filter(|partner| partner.open).find_map(|partner| {
        let terms = terms(trade, partner.city())?;
        (!terms.by_water() && !terms.sells.is_empty()).then(|| partner.city().to_string())
    })
}

// Envoys bring gifts: a caravan from envoys_partner carrying `goods` of the partner's goods, shared
// out among them, unloaded free at the stores. False if there's no such partner or it can't come in.
pub fn send_envoys(context: &SimContextMut, goods: u32) -> bool {
    let Some(city) = envoys_partner(context) else {
        return false;
    };
    let (Some(partner), Some(terms)) = (context.trade().partner(&city), terms(context.trade(), &city)) else {
        return false;
    };
    let mut cargo = [0; RESOURCE_KIND_COUNT];
    let share = goods / terms.sells.len() as u32;
    for (index, good) in terms.sells.iter().enumerate() {
        let extra = u32::from((index as u32) < goods % terms.sells.len() as u32);
        cargo[good.good.index()] = share + extra;
    }
    let entry = caravan_entry(context, partner);
    let rules = &GameConfigs::get().sim.trade;
    match Spawner::new(context).try_spawn_unit_with_config(entry, UnitConfigKey::Trader) {
        Ok(unit) => {
            let task = UnitTaskTrade::new(&city, entry, cargo, goods, rules.caravan_stops).as_gift();
            let task_id = context.task_manager_mut().new_task(task);
            unit.assign_task(context.task_manager_mut(), task_id);
            true
        }
        Err(err) => {
            log::error!(log::channel!("trade"), "Envoys from '{city}' couldn't come in at {entry}: {}", err.message);
            false
        }
    }
}

// The settlers' spawn point, or, on a map without one, the default the settlers would take (the
// bottom edge's middle, moved in to the playable area where the map's corners are trimmed), flagged
// as one at once, as the settlers do through a deferred update.
fn spawn_point(context: &SimContextMut) -> Cell {
    if let Some(node) = context.graph().settlers_spawn_point() {
        return node.cell;
    }
    let size = context.map_size_in_cells();
    let mut cell = Cell::new(size.width / 2 - 1, size.height - 1);
    while cell.y > 0 && !context.tile_map().is_cell_within_playable_area(cell) {
        cell.y -= 1;
    }
    context.tile_map_mut().set_tile_flags(cell, TileKind::Terrain, TileFlags::SettlersSpawnPoint, true);
    cell
}

// Where a partner's caravans come in: its `entry` cell when a caravan can stand there, else the
// settlers' spawn point.
pub fn caravan_entry(context: &SimContextMut, partner: &TradePartner) -> Cell {
    if let Some([x, y]) = partner.def.entry {
        let cell = Cell::new(x, y);
        let walkable = context.graph().node_kind(Node::new(cell)).is_some_and(|kind| kind.intersects(TRADER_NODE_KINDS));
        if walkable {
            return cell;
        }
        log::warning!(
            log::channel!("trade"),
            "Trade partner '{}': its entry {cell} isn't land a caravan can stand on; the settlers' spawn point stands in.",
            partner.city()
        );
    }
    spawn_point(context)
}

// Why a caravan or a ship didn't set out.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NotSent {
    NotTrading,     // Trade is off, or the partner isn't one or its route is closed.
    NoDock,         // A river or sea partner, and no working dock whose water reaches the map's edge.
    NothingToTrade, // No import below its limit and no export above it, within this year's amounts.
    NoEntry,        // It couldn't come in at its entry (something stands there).
}

// Sends a partner's caravan, or its ship by river or sea (ships::send_ship), now, whatever its
// schedule.
pub fn send(context: &SimContextMut, city: &str) -> Result<(), NotSent> {
    if terms(context.trade(), city).is_some_and(|terms| terms.by_water()) {
        return ships::send_ship(context, city);
    }
    send_caravan(context, city)
}

// Sends a caravan from `city` (a land partner) now.
fn send_caravan(context: &SimContextMut, city: &str) -> Result<(), NotSent> {
    if !is_trading_with(context, city) {
        return Err(NotSent::NotTrading);
    }
    let trade = context.trade();
    let (Some(partner), Some(terms)) = (trade.partner(city), terms(trade, city)) else {
        return Err(NotSent::NotTrading);
    };
    let rules = &GameConfigs::get().sim.trade;
    let held = holdings(context);
    let cargo = caravan_cargo(partner, &terms, &held, rules.caravan_capacity);
    if cargo.iter().all(|&count| count == 0) && !has_exports(partner, &terms, &held) {
        return Err(NotSent::NothingToTrade);
    }
    let entry = caravan_entry(context, partner);
    // At once: this runs outside the world step (the month's start, automation), as a collapse does.
    match Spawner::new(context).try_spawn_unit_with_config(entry, UnitConfigKey::Trader) {
        Ok(unit) => {
            let task = UnitTaskTrade::new(city, entry, cargo, rules.caravan_capacity, rules.caravan_stops);
            let task_id = context.task_manager_mut().new_task(task);
            unit.assign_task(context.task_manager_mut(), task_id);
            log::info!(log::channel!("trade"), "A caravan sets out from '{city}' at {entry}.");
            Ok(())
        }
        Err(err) => {
            log::error!(log::channel!("trade"), "A caravan from '{city}' couldn't set out at {entry}: {}", err.message);
            Err(NotSent::NoEntry)
        }
    }
}

// A new month (after the floods' and the farmsteads' months, before faith's rolls): a new year
// starts the partners' amounts again; requests fail or open (requests::on_new_month); prices drift
// back toward their base; each open route whose
// caravan or ship is due sends it, the next due `every_months` later, or next month when there's
// nothing to trade yet or no dock to sail to.
pub fn on_new_month(context: &SimContextMut, date: CalendarDate) {
    if date.month == 1 {
        context.trade_mut().new_year();
    }
    // Requests come whether the game trades or not.
    requests::on_new_month(context, date.ordinal_month());
    if !context.trade().is_enabled() {
        return;
    }
    context.trade_mut().drift_prices(&GameConfigs::get().sim.trade);
    let now = date.ordinal_month();
    let due: Vec<String> = context
        .trade()
        .partners()
        .iter()
        .filter(|partner| partner.open && partner.next_arrival.is_some_and(|month| month <= now))
        .map(|partner| partner.city().to_string())
        .collect();
    for city in due {
        let Some(terms) = terms(context.trade(), &city) else {
            continue;
        };
        let next = match send(context, &city) {
            Ok(()) => now + terms.every_months,
            Err(NotSent::NothingToTrade) => {
                log::info!(log::channel!("trade"), "Nothing comes from '{city}' this month: nothing to trade.");
                now + 1
            }
            Err(NotSent::NoDock) => {
                log::info!(log::channel!("trade"), "No ship from '{city}' this month: no working dock to sail to.");
                now + 1
            }
            Err(NotSent::NoEntry) => now + 1,
            Err(NotSent::NotTrading) => now + terms.every_months,
        };
        if let Some(partner) = context.trade_mut().partner_mut(&city) {
            partner.next_arrival = Some(next);
        }
    }
}

// ----------------------------------------------
// Opening a route
// ----------------------------------------------

// Why a route can't be opened.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OpenRouteError {
    TradeOff,        // The game doesn't trade.
    NotAPartner,     // The city isn't one of this game's partners.
    AlreadyOpen,     // Its route is open.
    NeedsDock,       // A river or sea route, and the city has no finished dock.
    CantAfford(u32), // The treasury can't pay its cost (the cost).
}

// The calendar's ordinal month now.
pub fn now(context: &SimContext) -> u32 {
    let sim = &GameConfigs::get().sim;
    context.calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month()
}

// Whether the player can open a partner's route now, and what it costs.
pub fn can_open_route(context: &SimContext, city: &str) -> Result<u32, OpenRouteError> {
    let trade = context.trade();
    if !trade.is_enabled() {
        return Err(OpenRouteError::TradeOff);
    }
    let (Some(partner), Some(terms)) = (trade.partner(city), terms(trade, city)) else {
        return Err(OpenRouteError::NotAPartner);
    };
    if partner.open {
        return Err(OpenRouteError::AlreadyOpen);
    }
    if terms.by_water() && !ships::has_dock(context) {
        return Err(OpenRouteError::NeedsDock);
    }
    if !context.finance().can_afford(terms.open_cost) {
        return Err(OpenRouteError::CantAfford(terms.open_cost));
    }
    Ok(terms.open_cost)
}

// The player opens a partner's route (the world map's Open the route): its cost spent on `Routes`,
// its first caravan due `first_arrival_months` later. Returns the cost.
pub fn open_route(context: &SimContextMut, city: &str) -> Result<u32, OpenRouteError> {
    let cost = can_open_route(context, city)?;
    if cost != 0 && !cheats::get().ignore_tile_cost {
        context.finance_mut().spend(LedgerCategory::Routes, cost);
    }
    let now = now(context);
    context.trade_mut().open_route(city, now, &GameConfigs::get().sim.trade);
    log::info!(log::channel!("trade"), "The route to '{city}' is open, for {cost} gold.");
    Ok(cost)
}

// ----------------------------------------------
// Notices
// ----------------------------------------------

// What GameLoop posts to the message log (notify_trade).
#[derive(Clone, PartialEq, Debug)]
pub enum TradeNotice {
    // A caravan left the city's last store: what it sold the city, what it bought, and the gold.
    Caravan {
        partner: String,
        sold: Vec<(ResourceKind, u32)>,
        bought: Vec<(ResourceKind, u32)>,
        paid: u32,
        earned: u32,
    },
    // Envoys left gifts (faith's Envoys).
    Gifts {
        partner: String,
        goods: Vec<(ResourceKind, u32)>,
    },
    // A ship set out for home: what it sold the city, what it bought, and the gold; or it found no
    // dock to moor at and turned back.
    Ship {
        partner: String,
        sold: Vec<(ResourceKind, u32)>,
        bought: Vec<(ResourceKind, u32)>,
        paid: u32,
        earned: u32,
        turned_back: bool,
    },
    // A request opened, was met or failed (requests); `court` when the court asked for it.
    Request {
        event: requests::RequestEvent,
        request: crate::sim::trade::TradeRequest,
        court: bool,
    },
}

fn goods_of(counts: &[u32; RESOURCE_KIND_COUNT]) -> Vec<(ResourceKind, u32)> {
    ResourceKind::all().iter().filter(|good| counts[good.index()] != 0).map(|good| (good, counts[good.index()])).collect()
}

impl TradeNotice {
    pub fn caravan(caravan: &UnitTaskTrade) -> Self {
        if caravan.gift {
            return Self::Gifts { partner: caravan.partner.clone(), goods: goods_of(&caravan.sold) };
        }
        Self::Caravan {
            partner: caravan.partner.clone(),
            sold: goods_of(&caravan.sold),
            bought: goods_of(&caravan.bought),
            paid: caravan.paid,
            earned: caravan.earned,
        }
    }

    pub fn ship(ship: &UnitTaskShip) -> Self {
        Self::Ship {
            partner: ship.partner.clone(),
            sold: goods_of(&ship.sold),
            bought: goods_of(&ship.bought),
            paid: ship.paid,
            earned: ship.earned,
            turned_back: ship.turned_back,
        }
    }

    // The partner's city id; a request's asker's.
    pub fn partner(&self) -> &str {
        match self {
            Self::Caravan { partner, .. } | Self::Gifts { partner, .. } | Self::Ship { partner, .. } => partner,
            Self::Request { request, .. } => &request.from,
        }
    }
}

static NOTICES: SingleThreadStatic<Vec<TradeNotice>> = SingleThreadStatic::new(Vec::new());

pub fn notify(notice: TradeNotice) {
    NOTICES.as_mut().push(notice);
}

pub fn take_notices() -> Vec<TradeNotice> {
    std::mem::take(NOTICES.as_mut())
}

// "20 wood, 8 metal".
pub(crate) fn goods_text(goods: &[(ResourceKind, u32)]) -> String {
    goods
        .iter()
        .map(|(good, count)| {
            text::format(UiTextCategory::Messages, "trade.goods_item", &[
                ("count", &count.to_string()),
                ("good", &good.display_name().to_lowercase()),
            ])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// A notice's message: its kind, sound and text. `partner_name` names the partner's city.
pub fn notice_message(notice: &TradeNotice, partner_name: &str) -> (Message, UiSoundKey) {
    let (message, sound) = notice_text(notice, partner_name);
    // A caravan's, a ship's and envoys' partner on the world map; a request on the Goods page.
    let target = match notice {
        TradeNotice::Request { .. } => MessageTarget::CityPage("goods"),
        _ => MessageTarget::WorldMap(Some(notice.partner().to_string())),
    };
    (message.with_target(target), sound)
}

fn notice_text(notice: &TradeNotice, partner_name: &str) -> (Message, UiSoundKey) {
    // "brought 20 bricks and took 8 metal: +40 gold", in a caravan's or a ship's words.
    let traded = |keys: [&str; 4], sold: &[(ResourceKind, u32)], bought: &[(ResourceKind, u32)], paid: u32, earned: u32| {
        let net = i64::from(earned) - i64::from(paid);
        let gold = if net >= 0 {
            text::format(UiTextCategory::Messages, "trade.gold_in", &[("gold", &net.to_string())])
        } else {
            text::format(UiTextCategory::Messages, "trade.gold_out", &[("gold", &(-net).to_string())])
        };
        let args =
            [("partner", partner_name), ("brought", &goods_text(sold)), ("took", &goods_text(bought)), ("gold", &gold)];
        let key = match (sold.is_empty(), bought.is_empty()) {
            (false, false) => keys[0],
            (false, true) => keys[1],
            (true, false) => keys[2],
            (true, true) => keys[3],
        };
        (Message::new(MessageKind::Trade, text::format(UiTextCategory::Messages, key, &args)), UiSoundKey::Notification)
    };
    match notice {
        TradeNotice::Caravan { sold, bought, paid, earned, .. } => traded(
            ["trade.caravan", "trade.caravan_brought", "trade.caravan_took", "trade.caravan_nothing"],
            sold,
            bought,
            *paid,
            *earned,
        ),
        TradeNotice::Ship { turned_back: true, .. } => (
            Message::new(
                MessageKind::Trade,
                text::format(UiTextCategory::Messages, "trade.ship_turned_back", &[("partner", partner_name)]),
            ),
            UiSoundKey::Notification,
        ),
        TradeNotice::Ship { sold, bought, paid, earned, .. } => traded(
            ["trade.ship", "trade.ship_brought", "trade.ship_took", "trade.ship_nothing"],
            sold,
            bought,
            *paid,
            *earned,
        ),
        TradeNotice::Gifts { goods, .. } => {
            let key = if goods.is_empty() { "trade.envoys_nothing" } else { "trade.envoys" };
            let args = [("partner", partner_name), ("goods", &goods_text(goods))];
            (Message::new(MessageKind::Trade, text::format(UiTextCategory::Messages, key, &args)), UiSoundKey::Notification)
        }
        TradeNotice::Request { event, request, court } => {
            let text = requests::notice_text(*event, request, *court, partner_name);
            match event {
                requests::RequestEvent::Failed => (Message::new(MessageKind::Warning, text), UiSoundKey::Alert),
                _ => (Message::new(MessageKind::Request, text), UiSoundKey::Notification),
            }
        }
    }
}

// A partner's name as the player reads it, from its city id in this game's world map; another
// city's (a request's asker) by its name on the map.
pub fn partner_name(trade: &CityTrade, city: &str) -> String {
    terms(trade, city).map_or_else(
        || place_name(trade.world_map(), city),
        |terms| text::get(UiTextCategory::Realm, &terms.name_key).to_string(),
    )
}
