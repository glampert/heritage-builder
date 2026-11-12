// The world map files (docs/plans/realm-ui.md, Phase 2: Data): one per campaign,
// assets/configs/game/world/<name>.json, named by the campaign's `world_map` or its id
// (CampaignDef::world_map_name). Loaded once at boot, after campaigns.json (WorldMapConfigs::load).
//
// A file holds what the world map screen shows of the world around the player's realm, in the
// map's own coordinates (`extent`: the mockup generator's map space, 1000 × 672 units, north up,
// docs/design/mockups/generator/common.py, so markers line up with the base art rendered from it):
// - `cities`: the realm's own cities (owner: the campaign's realm) and the foreign ones (owner:
//   another realm, or a power listed in `powers`), each with a kind, a role and, for a trade
//   partner, its static trade data until M10 (trade);
// - `routes`: river, sea or land, open or closed, as points, and the cities they join (a world
//   city, or a city one of the campaign's missions founds: MissionDef::city);
// - `labels`: region labels as the generator draws them (size and letter spacing in points on a
//   1920 × 1080 screen); `uncharted` areas and `threats`; where the `compass` and the next
//   mission's `choice_box` go.
// The cities the player founds come from campaigns.json (MissionDef::city), not from here.
//
// Text in a file is keys (en.json): names of cities, powers and regions in the Realm category;
// route lines, carriers and trade goods in the Menu category.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use strum::Display;

use engine::{file_sys::paths::PathRef, log, ui::text::UiTextCategory};

use crate::{
    campaign::config::{CampaignConfigs, CampaignDef},
    realm::Realm,
};

// The highest relations level (Allied); 0 is Hostile.
pub const MAX_RELATION_LEVEL: u8 = 5;

// ----------------------------------------------
// World map file
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldMapDef {
    // Width and height of the map's coordinate space.
    pub extent: [f32; 2],
    pub compass: [f32; 2],
    // Where the next mission's choice box goes (its top-left corner).
    pub choice_box: [f32; 2],
    #[serde(default)]
    pub powers: Vec<PowerDef>,
    pub cities: Vec<WorldCityDef>,
    #[serde(default)]
    pub routes: Vec<RouteDef>,
    #[serde(default)]
    pub labels: Vec<MapLabelDef>,
    #[serde(default)]
    pub uncharted: Vec<UnchartedAreaDef>,
    #[serde(default)]
    pub threats: Vec<ThreatDef>,
}

// A power that isn't a realm (the Sea Kingdom, the Highland clans): a city's owner.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerDef {
    pub id: String,
    pub name: String, // Realm key, e.g. "power.sea_kingdom".
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCityDef {
    pub id: String,
    pub name: String, // Realm key, e.g. "city.vijara".
    pub position: [f32; 2],
    pub kind: CityKind,
    // A realm's name (dailong, kambara, tampadesa) or a power's id.
    pub owner: String,
    #[serde(default)]
    pub role: Option<CityRole>,
    #[serde(default)]
    pub label: LabelSide,
    // The player's from the start, such as the realm's capital and monuments. Not a city a mission
    // will found: until its mission exists, the mockup's placeholder for one is a realm city that
    // isn't the player's, and then it moves to MissionDef::city. Only for a city whose owner is the
    // realm of a campaign that shows the map.
    #[serde(default)]
    pub player: bool,
    // The line under its name in the side panel (Realm key). None: its owner's name.
    #[serde(default)]
    pub subtitle: Option<String>,
    // TODO(realm-ui): M10's trade replaces these static values (WorldMapProvider::city_panel).
    #[serde(default)]
    pub trade: Option<CityTradeDef>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CityKind {
    Capital,
    City,
    Monument,
    Village,
    Camp,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CityRole {
    TradePartner,
    Neighbour,
}

impl CityRole {
    // Its Menu key, e.g. "world_map.role.trade_partner".
    pub fn text_key(self) -> &'static str {
        match self {
            Self::TradePartner => "world_map.role.trade_partner",
            Self::Neighbour => "world_map.role.neighbour",
        }
    }
}

// Where a name goes beside its mark, as the generator's `label_pos` places it.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LabelSide {
    #[default]
    Below,
    Above,
    Left,
    Right,
}

// A trade partner's static data, the side panel's until M10.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityTradeDef {
    // 0 (Hostile) to MAX_RELATION_LEVEL (Allied); its word is the Menu string world_map.relation.<level>.
    pub relation: u8,
    #[serde(default)]
    pub route: Option<TradeRouteDef>,
    #[serde(default)]
    pub sells: Vec<TradeGoodDef>,
    #[serde(default)]
    pub buys: Vec<TradeGoodDef>,
    #[serde(default)]
    pub request: Option<TradeRequestDef>,
}

// "Sea route from Bến Gốm", "Open · junks every 3 months".
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeRouteDef {
    pub route: String,   // A route's id.
    pub from: String,    // The player's city it starts from.
    pub line: String,    // Menu key with {city}, e.g. "world_map.route_line.sea_route".
    pub carrier: String, // Menu key, e.g. "world_map.carrier.junks".
    pub every_months: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeGoodDef {
    pub good: String, // Menu key, e.g. "trade_good.salt".
    pub price: u32,
}

// "Ceramics × 30", "within 8 months".
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeRequestDef {
    pub good: String, // Menu key.
    pub amount: u32,
    pub within_months: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDef {
    pub id: String,
    pub kind: RouteKind,
    pub state: RouteState,
    // The route's course, through which the screen draws a smooth curve (points may lie past the
    // map's edge: the map clips them).
    pub points: Vec<[f32; 2]>,
    // The cities it joins: the screen shows it once they're all on the map.
    #[serde(default)]
    pub cities: Vec<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RouteKind {
    River,
    Sea,
    Land,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RouteState {
    Open,
    Closed,
}

// Text on the map: a realm's or a region's name (drawn in capitals), or a note (as written).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapLabelDef {
    pub text: String, // Realm key.
    pub position: [f32; 2],
    pub style: MapLabelStyle,
    // The campaign's own realm: drawn larger, in the realm's accent.
    #[serde(default)]
    pub own: bool,
    // Font size and letter spacing in points on a 1920 × 1080 screen, as the generator draws them.
    pub size: f32,
    #[serde(default)]
    pub letter_spacing: f32,
    // Degrees clockwise (the generator draws none turned).
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub align: MapLabelAlign,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MapLabelStyle {
    Realm,
    Region,
    Note,
}

// Where `position` is on the text's baseline: its middle, or where it starts.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MapLabelAlign {
    #[default]
    Middle,
    Start,
}

// Land beyond the known world, veiled, with a title and a line under it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnchartedAreaDef {
    pub title: String,    // Realm key.
    pub subtitle: String, // Realm key.
    pub outline: Vec<[f32; 2]>,
    pub label: [f32; 2],
}

// A danger beyond the map's edge: a storm of clouds with a title, a line, and an arrow.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreatDef {
    pub title: String,    // Realm key.
    pub subtitle: String, // Realm key.
    pub position: [f32; 2],
    #[serde(default)]
    pub arrow: Vec<[f32; 2]>,
}

impl WorldMapDef {
    pub fn city(&self, id: &str) -> Option<&WorldCityDef> {
        self.cities.iter().find(|city| city.id == id)
    }

    pub fn route(&self, id: &str) -> Option<&RouteDef> {
        self.routes.iter().find(|route| route.id == id)
    }

    pub fn power(&self, id: &str) -> Option<&PowerDef> {
        self.powers.iter().find(|power| power.id == id)
    }

    // Whether `point` is on the map.
    pub fn contains(&self, point: [f32; 2]) -> bool {
        (0.0..=self.extent[0]).contains(&point[0]) && (0.0..=self.extent[1]).contains(&point[1])
    }

    // The Realm key of an owner's name: a realm's (realm.<name>) or a power's. None for neither.
    pub fn owner_name_key(&self, owner: &str) -> Option<String> {
        if let Ok(realm) = Realm::parse(owner) {
            return Some(format!("realm.{realm}"));
        }
        self.power(owner).map(|power| power.name.clone())
    }

    // What's wrong with the file, read for `campaigns` (those that show it): ids given twice, an
    // owner that's neither a realm nor a power, a city or label off the map, a relation past
    // MAX_RELATION_LEVEL, a route with fewer than two points, and references that don't resolve
    // (a route's cities, a trade route's route and city; a mission's city must not take a world
    // city's id). Empty when all is well. Text keys are checked by text_keys' callers.
    pub fn validate(&self, campaigns: &[&CampaignDef]) -> Vec<String> {
        let mut errors = Vec::new();
        let [width, height] = self.extent;
        if width <= 0.0 || height <= 0.0 {
            errors.push(format!("The extent {width} × {height} is empty."));
        }
        for (name, point) in [("compass", self.compass), ("choice box", self.choice_box)] {
            if !self.contains(point) {
                errors.push(format!("The {name} at {point:?} is off the map."));
            }
        }

        for (index, power) in self.powers.iter().enumerate() {
            if power.id.is_empty()
                || self.powers[..index].iter().any(|other| other.id == power.id)
                || Realm::parse(&power.id).is_ok()
            {
                errors.push(format!("The power id '{}' is empty, given twice or a realm's.", power.id));
            }
        }

        // Every city the map can show: the world's, and every mission's of the campaigns.
        let mission_cities: Vec<&str> = campaigns
            .iter()
            .flat_map(|campaign| campaign.missions.iter().filter_map(|mission| mission.city.as_ref()))
            .map(|city| city.id.as_str())
            .collect();
        let is_city = |id: &str| self.city(id).is_some() || mission_cities.contains(&id);

        for (index, city) in self.cities.iter().enumerate() {
            if city.id.is_empty() || self.cities[..index].iter().any(|other| other.id == city.id) {
                errors.push(format!("The city id '{}' is empty or given twice.", city.id));
            }
            if !self.contains(city.position) {
                errors.push(format!("City '{}' at {:?} is off the map.", city.id, city.position));
            }
            if self.owner_name_key(&city.owner).is_none() {
                errors.push(format!(
                    "City '{}' has the owner '{}', neither a realm nor a power of this map.",
                    city.id, city.owner
                ));
            }
            if city.player && !campaigns.iter().any(|campaign| campaign.realm.to_string() == city.owner) {
                errors.push(format!(
                    "City '{}' is the player's, but its owner '{}' is no realm of a campaign showing this map.",
                    city.id, city.owner
                ));
            }
            let Some(trade) = &city.trade else {
                continue;
            };
            if trade.relation > MAX_RELATION_LEVEL {
                errors.push(format!("City '{}' has relations {}, past {MAX_RELATION_LEVEL}.", city.id, trade.relation));
            }
            if let Some(route) = &trade.route {
                match self.route(&route.route) {
                    None => errors.push(format!(
                        "City '{}' trades over the route '{}', which isn't on the map.",
                        city.id, route.route
                    )),
                    Some(def)
                        if !def.cities.iter().any(|id| *id == city.id) || !def.cities.iter().any(|id| *id == route.from) =>
                    {
                        errors.push(format!(
                            "City '{}' trades over the route '{}', which doesn't join it to '{}'.",
                            city.id, route.route, route.from
                        ));
                    }
                    Some(_) => {}
                }
                if !is_city(&route.from) {
                    errors.push(format!("City '{}' trades from '{}', no city of the map.", city.id, route.from));
                }
            }
        }

        for (index, route) in self.routes.iter().enumerate() {
            if route.id.is_empty() || self.routes[..index].iter().any(|other| other.id == route.id) {
                errors.push(format!("The route id '{}' is empty or given twice.", route.id));
            }
            if route.points.len() < 2 {
                errors.push(format!("Route '{}' has {} points; a route needs two.", route.id, route.points.len()));
            }
            for id in route.cities.iter().filter(|id| !is_city(id)) {
                errors.push(format!("Route '{}' joins '{id}', no city of the map.", route.id));
            }
        }

        for label in &self.labels {
            if !self.contains(label.position) {
                errors.push(format!("The label '{}' at {:?} is off the map.", label.text, label.position));
            }
        }
        for area in &self.uncharted {
            if area.outline.len() < 3 || !self.contains(area.label) {
                errors.push(format!(
                    "The uncharted area '{}' needs an outline of three points and its label on the map.",
                    area.title
                ));
            }
        }
        for threat in &self.threats {
            if !self.contains(threat.position) {
                errors.push(format!("The threat '{}' at {:?} is off the map.", threat.title, threat.position));
            }
        }

        for campaign in campaigns {
            for (index, mission) in campaign.missions.iter().enumerate() {
                if let Some(city) = &mission.city {
                    if self.city(&city.id).is_some() {
                        errors.push(format!(
                            "Campaign '{}', mission {index}: its city '{}' has a world city's id.",
                            campaign.id, city.id
                        ));
                    }
                    if !self.contains(city.position) {
                        errors.push(format!(
                            "Campaign '{}', mission {index}: its city '{}' is off the map.",
                            campaign.id, city.id
                        ));
                    }
                }
                for choice in mission.choices.iter().filter(|choice| !self.contains(choice.position)) {
                    errors
                        .push(format!("Campaign '{}', mission {index}: site {} is off the map.", campaign.id, choice.site));
                }
            }
        }
        errors
    }

    // Every text key the file names, with its category, for the tests that check en.json has them.
    pub fn text_keys(&self) -> Vec<(UiTextCategory, String)> {
        let mut keys = Vec::new();
        let realm = |key: &String| (UiTextCategory::Realm, key.clone());
        let menu = |key: &String| (UiTextCategory::Menu, key.clone());
        keys.extend(self.powers.iter().map(|power| realm(&power.name)));
        for city in &self.cities {
            keys.push(realm(&city.name));
            keys.extend(city.subtitle.iter().map(realm));
            keys.extend(city.role.map(|role| (UiTextCategory::Menu, role.text_key().to_string())));
            if let Some(trade) = &city.trade {
                keys.push((UiTextCategory::Menu, format!("world_map.relation.{}", trade.relation)));
                if let Some(route) = &trade.route {
                    keys.push(menu(&route.line));
                    keys.push(menu(&route.carrier));
                }
                keys.extend(trade.sells.iter().chain(&trade.buys).map(|good| menu(&good.good)));
                keys.extend(trade.request.iter().map(|request| menu(&request.good)));
            }
        }
        keys.extend(self.labels.iter().map(|label| realm(&label.text)));
        for area in &self.uncharted {
            keys.push(realm(&area.title));
            keys.push(realm(&area.subtitle));
        }
        for threat in &self.threats {
            keys.push(realm(&threat.title));
            keys.push(realm(&threat.subtitle));
        }
        keys
    }
}

// ----------------------------------------------
// WorldMapConfigs
// ----------------------------------------------

// Every world map file the campaigns name, by name (CampaignDef::world_map_name).
#[derive(Default)]
pub struct WorldMapConfigs {
    pub maps: BTreeMap<String, WorldMapDef>,
}

impl WorldMapConfigs {
    // The file `assets/configs/game/world/<name>.json` would be.
    pub fn file_path(name: &str) -> String {
        format!("game/world/{name}")
    }

    // Loads the file of every campaign in `campaigns` (each once) and checks it; errors are logged
    // and a file that doesn't load leaves its campaigns without a world map.
    pub fn load_for(campaigns: &CampaignConfigs) -> Self {
        let mut maps = BTreeMap::new();
        for campaign in &campaigns.campaigns {
            let name = campaign.world_map_name().to_string();
            if maps.contains_key(&name) {
                continue;
            }
            let path = Self::file_path(&name);
            let def: WorldMapDef = engine::config::load_asset_file(PathRef::from_str(&path));
            if def.cities.is_empty() {
                log::error!(
                    log::channel!("campaign"),
                    "The world map '{name}' ({path}.json) has no cities: campaign '{}' shows none.",
                    campaign.id
                );
                continue;
            }
            maps.insert(name, def);
        }
        let configs = Self { maps };
        for error in configs.validate(campaigns) {
            log::error!(log::channel!("campaign"), "World map {error}");
        }
        configs
    }

    // Each file's errors (WorldMapDef::validate), prefixed with its name.
    pub fn validate(&self, campaigns: &CampaignConfigs) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, map) in &self.maps {
            let users: Vec<&CampaignDef> =
                campaigns.campaigns.iter().filter(|campaign| campaign.world_map_name() == name).collect();
            errors.extend(map.validate(&users).into_iter().map(|error| format!("'{name}': {error}")));
        }
        errors
    }

    // The world map of the campaign at `campaign_index`. None: it has none.
    pub fn for_campaign<'a>(&'a self, campaigns: &CampaignConfigs, campaign_index: usize) -> Option<&'a WorldMapDef> {
        self.maps.get(campaigns.campaign(campaign_index)?.world_map_name())
    }

    // Loads them for the campaigns loaded (CampaignConfigs::load first).
    pub fn load() -> &'static Self {
        Self::initialize(Self::load_for(CampaignConfigs::get()));
        Self::get()
    }
}

common::singleton_late_init! { WORLD_MAP_CONFIGS_SINGLETON, WorldMapConfigs }
