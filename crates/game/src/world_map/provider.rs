// What the world map screen reads (docs/plans/realm-ui.md, Phase 2: Hooks for M10): the cities and
// routes on the map, the player's cities, the selected city's side panel and the next mission's
// choice. WorldMapProvider is the screen's only source, so M10's trade can replace the static
// values without touching the screen; ConfigWorldMapProvider reads them from the world map file,
// campaigns.json and the player's profile.

use crate::{
    campaign::{
        ActiveMission,
        config::{CampaignConfigs, CampaignDef, ChoiceSite},
    },
    profile::Profile,
    realm::Realm,
};
use engine::ui::text::{self, UiTextCategory};

use super::config::{
    CityKind, CityRole, LabelSide, MAX_RELATION_LEVEL, RouteDef, RouteKind, RouteState, WorldCityDef, WorldMapConfigs,
    WorldMapDef,
};

// ----------------------------------------------
// What the screen shows
// ----------------------------------------------

// A city on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct MapCity {
    pub id: String,
    pub name_key: String, // Realm key.
    pub position: [f32; 2],
    pub kind: CityKind,
    pub owner: String, // A realm's name or a power's id.
    pub role: Option<CityRole>,
    pub label: LabelSide,
    // The player's: one the world map file gives the player from the start (WorldCityDef::player),
    // or one a mission founded.
    pub player_owned: bool,
    // The mission that founds it (its index in the campaign), for a mission's city.
    pub founded_by_mission: Option<usize>,
    // The city of the mission being played.
    pub current: bool,
    // Whether it has a side panel of trade data (city_panel).
    pub has_trade: bool,
}

impl MapCity {
    pub fn name(&self) -> &str {
        text::get(UiTextCategory::Realm, &self.name_key)
    }
}

// A route on the map, shown once every city it joins is.
#[derive(Clone, Debug, PartialEq)]
pub struct MapRoute {
    pub id: String,
    pub kind: RouteKind,
    pub state: RouteState,
    pub points: Vec<[f32; 2]>,
    pub cities: Vec<String>,
}

// The side panel of a city, as the player reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct CityPanel {
    pub city_id: String,
    pub name: String,
    pub subtitle: String,     // "The Sea Kingdom": the city's subtitle, else its owner's name.
    pub role: Option<String>, // "Trade partner".
    pub relation: Option<Relation>,
    pub route_line: Option<String>,   // "Sea route from Bến Gốm".
    pub route_detail: Option<String>, // "Open · junks every 3 months".
    pub sells: Vec<TradeOffer>,
    pub buys: Vec<TradeOffer>,
    pub request: Option<CourtRequest>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Relation {
    pub level: u8, // 0 to MAX_RELATION_LEVEL.
    pub max: u8,
    pub word: String, // "Friendly".
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradeOffer {
    pub good: String, // "Salt".
    pub price: u32,
}

// "Ceramics × 30", "within 8 months".
#[derive(Clone, Debug, PartialEq)]
pub struct CourtRequest {
    pub goods: String,
    pub within: String,
}

// The next mission's choice: "Next: The Southern Shore", "Choose Hội Thủy or Vân Cảng", and its sites.
#[derive(Clone, Debug, PartialEq)]
pub struct NextMissionChoice {
    pub mission_index: usize, // The choice point.
    pub title: String,
    pub subtitle: String,
    pub subtitle_key: String, // Its Menu key, {a} and {b} the sites' names.
    pub sites: Vec<ChoiceSiteInfo>,
    // Whether a site can be picked now: not while the campaign's mission being played is
    // unfinished (its goals not met yet), which the next mission follows, nor while another game
    // is being played (a custom game, another campaign's mission).
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceSiteInfo {
    pub site: ChoiceSite,
    pub name_key: String, // Realm key.
    pub name: String,
    pub phrase: String, // Its name in the choice box's line.
    // Whether the config gives that phrase: a common noun ("royal port"), which sentences put
    // "the" before, where a name ("Vân Cảng") stands alone.
    pub phrased: bool,
    pub position: [f32; 2],
    pub label: LabelSide,
    pub mission_index: usize, // The mission it starts.
}

// ----------------------------------------------
// WorldMapProvider
// ----------------------------------------------

pub trait WorldMapProvider {
    // The campaign whose map it is, by index in campaigns.json.
    fn campaign_index(&self) -> usize;

    // The map's own data: extent, labels, uncharted areas, threats, compass, choice box.
    fn map(&self) -> &WorldMapDef;

    // Every city on the map now: the world's and the missions' the player has founded.
    fn cities(&self) -> Vec<MapCity>;

    // The routes on the map now.
    fn routes(&self) -> Vec<MapRoute>;

    // The side panel of the city with this id. None for a city that isn't on the map.
    fn city_panel(&self, city_id: &str) -> Option<CityPanel>;

    // The choice the next mission offers, if it has one.
    fn next_mission_choice(&self) -> Option<NextMissionChoice>;

    // The player's cities.
    fn player_cities(&self) -> Vec<MapCity> {
        self.cities().into_iter().filter(|city| city.player_owned).collect()
    }
}

// ----------------------------------------------
// ConfigWorldMapProvider
// ----------------------------------------------

// The provider until M10: the world map file's static values, the campaign's mission cities, the
// profile's finished missions and the mission being played.
pub struct ConfigWorldMapProvider<'a> {
    campaigns: &'a CampaignConfigs,
    campaign_index: usize,
    map: &'a WorldMapDef,
    profile: &'a Profile,
    active: Option<ActiveMission>,
    // Whether a game other than this campaign's mission is being played (a custom game, another
    // campaign's mission, a preset): the choice stays closed then, since starting its mission
    // would end that game.
    other_game: bool,
}

impl<'a> ConfigWorldMapProvider<'a> {
    // The provider for the campaign at `campaign_index`, if it has a world map. `active` is the
    // mission being played (campaign::active_mission), of any campaign.
    pub fn new(
        campaigns: &'a CampaignConfigs,
        world_maps: &'a WorldMapConfigs,
        campaign_index: usize,
        profile: &'a Profile,
        active: Option<ActiveMission>,
    ) -> Option<Self> {
        let map = world_maps.for_campaign(campaigns, campaign_index)?;
        let active = active.filter(|mission| mission.campaign_id == campaign_index);
        Some(Self { campaigns, campaign_index, map, profile, active, other_game: false })
    }

    // Whether a game is being played that isn't one of this campaign's missions (custom games,
    // other campaigns' missions, presets). Only meaningful with no active mission of its own.
    pub fn with_other_game(mut self, other_game: bool) -> Self {
        self.other_game = other_game && self.active.is_none();
        self
    }

    fn campaign(&self) -> &'a CampaignDef {
        &self.campaigns.campaigns[self.campaign_index]
    }

    fn missions_finished(&self) -> &'a [usize] {
        self.profile.record(&self.campaign().id).map_or(&[][..], |record| record.missions_finished.as_slice())
    }

    // The campaign's realm owns the realm's own cities.
    fn realm(&self) -> Realm {
        self.campaign().realm
    }

    fn world_city(&self, city: &WorldCityDef) -> MapCity {
        MapCity {
            id: city.id.clone(),
            name_key: city.name.clone(),
            position: city.position,
            kind: city.kind,
            owner: city.owner.clone(),
            role: city.role,
            label: city.label,
            player_owned: city.player && city.owner == self.realm().to_string(),
            founded_by_mission: None,
            current: false,
            has_trade: city.trade.is_some(),
        }
    }

    fn is_shown(&self, route: &RouteDef, cities: &[MapCity]) -> bool {
        route.cities.iter().all(|id| cities.iter().any(|city| city.id == *id))
    }

    // The next step: the one straight after the mission being played, finished or not, as
    // campaign::advance_to_next_mission goes to; with none being played, the first step the
    // profile hasn't finished (Profile::next_mission's).
    fn next_step(&self) -> Option<usize> {
        let campaign = self.campaign();
        match &self.active {
            Some(mission) => campaign.next_step_after(mission.mission_index),
            None => {
                let finished = self.missions_finished();
                campaign.steps().into_iter().find(|&step| !campaign.is_step_finished(step, finished))
            }
        }
    }
}

impl WorldMapProvider for ConfigWorldMapProvider<'_> {
    fn campaign_index(&self) -> usize {
        self.campaign_index
    }

    fn map(&self) -> &WorldMapDef {
        self.map
    }

    fn cities(&self) -> Vec<MapCity> {
        let mut cities: Vec<MapCity> = self.map.cities.iter().map(|city| self.world_city(city)).collect();
        let finished = self.missions_finished();
        let current = self.active.as_ref().map(|mission| mission.mission_index);
        for (index, mission) in self.campaign().missions.iter().enumerate() {
            let Some(city) = &mission.city else {
                continue;
            };
            let is_current = current == Some(index);
            if !finished.contains(&index) && !is_current {
                continue;
            }
            cities.push(MapCity {
                id: city.id.clone(),
                name_key: city.name.clone(),
                position: city.position,
                kind: city.kind,
                owner: self.realm().to_string(),
                role: None,
                label: city.label,
                player_owned: true,
                founded_by_mission: Some(index),
                current: is_current,
                has_trade: false,
            });
        }
        cities
    }

    fn routes(&self) -> Vec<MapRoute> {
        let cities = self.cities();
        self.map
            .routes
            .iter()
            .filter(|route| self.is_shown(route, &cities))
            .map(|route| MapRoute {
                id: route.id.clone(),
                kind: route.kind,
                state: route.state,
                points: route.points.clone(),
                cities: route.cities.clone(),
            })
            .collect()
    }

    // TODO(realm-ui): M10's trade gives relations, prices, limits and requests here; until then
    // they're the world map file's static values.
    fn city_panel(&self, city_id: &str) -> Option<CityPanel> {
        let city = self.cities().into_iter().find(|city| city.id == city_id)?;
        let def = self.map.city(city_id);
        let subtitle_key = def.and_then(|def| def.subtitle.clone()).or_else(|| self.map.owner_name_key(&city.owner));
        let subtitle = subtitle_key.map_or_else(String::new, |key| text::get(UiTextCategory::Realm, &key).to_string());
        let mut panel = CityPanel {
            city_id: city.id.clone(),
            name: city.name().to_string(),
            subtitle,
            role: city.role.map(|role| text::get(UiTextCategory::Menu, role.text_key()).to_string()),
            relation: None,
            route_line: None,
            route_detail: None,
            sells: Vec::new(),
            buys: Vec::new(),
            request: None,
        };
        let Some(trade) = def.and_then(|def| def.trade.as_ref()) else {
            return Some(panel);
        };

        let level = trade.relation.min(MAX_RELATION_LEVEL);
        let word = text::get(UiTextCategory::Menu, &format!("world_map.relation.{level}")).to_string();
        panel.relation = Some(Relation { level, max: MAX_RELATION_LEVEL, word });

        if let Some(route) = &trade.route {
            let from = self.cities().into_iter().find(|other| other.id == route.from);
            // The city it starts from, by name, or "—" until it shows (a mission city before the
            // player founds it). A realm city the file doesn't mark `player` (Kambara's
            // Pashchima, a placeholder until its mission exists) runs its route as the mockup shows.
            // TODO(realm-ui): M10's trade decides whether a route needs a player's city.
            let from_name = from.map_or_else(
                || text::get(UiTextCategory::Menu, "world_map.none").to_string(),
                |from| from.name().to_string(),
            );
            panel.route_line = Some(text::format(UiTextCategory::Menu, &route.line, &[("city", &from_name)]));
            let state = self.map.route(&route.route).map_or(RouteState::Open, |def| def.state);
            // A route the map doesn't show yet (a city it joins isn't founded) isn't running.
            let shown = self.routes().iter().any(|shown| shown.id == route.route);
            panel.route_detail = Some(match state {
                _ if !shown => text::get(UiTextCategory::Menu, "world_map.route_detail.not_yet_open").to_string(),
                RouteState::Open => {
                    let carrier = text::get(UiTextCategory::Menu, &route.carrier);
                    text::format_count(
                        UiTextCategory::Menu,
                        "world_map.route_detail.open",
                        i64::from(route.every_months),
                        &[("carrier", carrier)],
                    )
                }
                RouteState::Closed => text::get(UiTextCategory::Menu, "world_map.route_detail.closed").to_string(),
            });
        }

        let offers = |goods: &[super::config::TradeGoodDef]| {
            goods
                .iter()
                .map(|good| TradeOffer { good: text::get(UiTextCategory::Menu, &good.good).to_string(), price: good.price })
                .collect()
        };
        panel.sells = offers(&trade.sells);
        panel.buys = offers(&trade.buys);

        panel.request = trade.request.as_ref().map(|request| {
            let good = text::get(UiTextCategory::Menu, &request.good);
            CourtRequest {
                goods: text::format(
                    UiTextCategory::Menu,
                    "world_map.request.goods",
                    &[("good", good), ("amount", &request.amount.to_string())],
                ),
                within: text::format_count(
                    UiTextCategory::Menu,
                    "world_map.request.within",
                    i64::from(request.within_months),
                    &[],
                ),
            }
        });
        Some(panel)
    }

    fn next_mission_choice(&self) -> Option<NextMissionChoice> {
        let campaign = self.campaign();
        let step = self.next_step()?;
        let mission = campaign.missions.get(step)?;
        if mission.choices.is_empty() {
            return None;
        }
        let sites: Vec<ChoiceSiteInfo> = mission
            .choices
            .iter()
            .filter_map(|choice| {
                Some(ChoiceSiteInfo {
                    site: choice.site,
                    name_key: choice.name.clone(),
                    name: text::get(UiTextCategory::Realm, &choice.name).to_string(),
                    phrase: text::get(UiTextCategory::Realm, choice.phrase.as_ref().unwrap_or(&choice.name)).to_string(),
                    phrased: choice.phrase.is_some(),
                    position: choice.position,
                    label: choice.label,
                    mission_index: campaign.resolve_mission(&choice.mission)?,
                })
            })
            .collect();
        let name = campaign.mission_display_name(step).unwrap_or_default();
        let site_name = |site: ChoiceSite| sites.iter().find(|info| info.site == site).map_or("", |info| info.phrase.as_str());
        // The choice point's own line (the mockups word each realm's their own way), else
        // "Choose <A> or <B>".
        let subtitle_key = mission.choice_text.clone().unwrap_or_else(|| "world_map.choice.subtitle".to_string());
        Some(NextMissionChoice {
            mission_index: step,
            title: text::format(UiTextCategory::Menu, "world_map.choice.title", &[("mission", name)]),
            subtitle: text::format(UiTextCategory::Menu, &subtitle_key, &[("a", site_name(ChoiceSite::A)), ("b", site_name(ChoiceSite::B))]),
            subtitle_key,
            sites,
            // Open once this campaign's mission is finished, or with no game being played; never
            // over another game (a custom game, another campaign's mission).
            open: match &self.active {
                Some(active) => active.completed,
                None => !self.other_game,
            },
        })
    }
}
