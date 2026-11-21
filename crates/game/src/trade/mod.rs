// Trade (docs/plans/trade.md): the partners a game trades with and their terms, read from the world
// map files and the mission's (or custom map's) `trade` block. The state is sim::trade::CityTrade.
//
// Phase 0 holds the data and the state; caravans (Phase 1), docks and ships (Phase 2), prices
// (Phase 3) and court requests (Phase 4) build on it.

use engine::log;

use crate::{
    campaign::config::{CampaignConfigs, MissionPartnerDef, MissionTradeDef},
    config::TradeRules,
    realm::Realm,
    sim::{resources::ResourceKind, trade::CityTrade},
    world_map::{
        WorldMapConfigs,
        config::{RouteKind, TradeGoodDef, WorldCityDef, WorldMapDef},
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
    pub from: String, // The player's city the route runs from.
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

// ----------------------------------------------
// Starting a game's trade
// ----------------------------------------------

// Starts a mission's or a custom game's trade on the world map `world_map` (an ordinal month `now`),
// leaving out, with a warning, the partners it has no terms for.
pub fn start(state: &mut CityTrade, world_map_name: &str, block: &MissionTradeDef, now: u32, rules: &TradeRules) {
    let map = world_map(world_map_name);
    let left_out = state.start(world_map_name, block, |def| map.is_some_and(|map| terms_on(map, def).is_some()), now, rules);
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

// A custom game's trade block when its map names none: every partner of the world map with a land
// route, closed (docs/plans/trade.md, §1).
pub fn land_partners(map: &WorldMapDef) -> MissionTradeDef {
    let partners = map
        .cities
        .iter()
        .filter_map(|city| {
            let route = city.trade.as_ref()?.route.as_ref()?;
            (map.route(&route.route)?.kind == RouteKind::Land).then(|| MissionPartnerDef {
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
    MissionTradeDef { enabled: None, partners }
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
