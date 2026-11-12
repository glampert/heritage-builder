// World map commands (docs/plans/realm-ui.md, Phase 2): what the world map screen shows of a
// campaign, read through its WorldMapProvider.

use serde::Deserialize;
use serde_json::{Value, json};

use engine::ui::widgets::UiMapViewState;

use crate::{
    menu::dialog::world_map as world_map_screen,
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    campaign::config::CampaignConfigs,
    profile,
    world_map::{
        self, WorldMapProvider,
        provider::{CityPanel, MapCity, MapRoute, NextMissionChoice},
    },
};

pub(super) const COMMANDS: &[CommandDef] = &[
CommandDef {
    name: "world_map.state",
    help: "What the world map shows of a campaign: its map (extent, compass, choice box), its cities (kind, \
           owner, position, label side, player_owned, the mission that founds it, current, and the side panel's \
           trade data for a trade partner), the routes shown (a route shows once every city it joins is on the \
           map), the region labels, uncharted areas and threats (their text, with its keys), and the next \
           mission's choice (its sites and the missions they start). Player cities: those the world map file \
           gives the player (its capital, monuments), those of the missions the profile has finished and the \
           mission being played's. screen: the world map screen while it's open (screen_open; null otherwise): the \
           campaign it shows (the UI realm's), the city selected and the one under the cursor, and the map's \
           view (zoom: 1 is the fit, the whole map; centre: the map point in the middle, in map units; fit). \
           selected: the screen's selected city, when the screen shows this campaign.",
    params: &[ParamDef {
        name: "campaign",
        kind: "string",
        required: false,
        help: "A campaign id (campaigns.json), hidden ones included. Default: the open screen's campaign, else the \
               mission being played's campaign, else the custom game's realm's, else the latest the player can play.",
    }],
    run: cmd_state,
},
CommandDef {
    name: "world_map.set",
    help: "Sets the open world map screen: the city selected (its side panel), and the map's view (zoom from 1, \
           the fit, to its maximum; centre, the map point in the middle, in map units), kept in its limits. \
           The screen shows them from the next frame. Fails while the screen isn't open.",
    params: &[
        ParamDef { name: "city", kind: "string", required: false, help: "A city id on the screen's map (world_map.state's cities)." },
        ParamDef { name: "zoom", kind: "number", required: false, help: "1 shows the whole map; more is closer." },
        ParamDef { name: "centre", kind: "point", required: false, help: "[x, y] in map units (the world file's)." },
    ],
    run: cmd_set,
},
];

fn cmd_set(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        city: Option<String>,
        zoom: Option<f32>,
        centre: Option<[f32; 2]>,
    }
    let params: Params = parse_params(params)?;
    let Some(screen) = world_map_screen::screen_info() else {
        return Err(CommandError::failed("The world map screen isn't open (dialog.open name=WorldMap, in game)."));
    };
    if let Some(zoom) = params.zoom
        && !(zoom.is_finite() && zoom > 0.0)
    {
        return Err(CommandError::invalid_params(format!("zoom must be a positive number, not {zoom}.")));
    }
    if let Some(city) = &params.city
        && !world_map_screen::select_city(city)
    {
        return Err(CommandError::not_found(format!("The world map has no city '{city}'.")));
    }
    if params.zoom.is_some() || params.centre.is_some() {
        let current = screen.view;
        let centre = params.centre.map_or(current.centre, |[x, y]| common::Vec2::new(x, y));
        world_map_screen::set_view(UiMapViewState { zoom: params.zoom.unwrap_or(current.zoom), centre });
    }
    done(json!({ "city": params.city, "zoom": params.zoom, "centre": params.centre }))
}

// The open screen as world_map.state reports it.
fn screen_json(screen: &world_map_screen::WorldMapScreenInfo) -> Value {
    let configs = CampaignConfigs::get();
    json!({
        "campaign": screen.campaign.and_then(|index| configs.campaign(index)).map(|campaign| campaign.id.clone()),
        "selected": screen.selected,
        "hovered": screen.hovered,
        "view": json!({
            "zoom": screen.view.zoom,
            "centre": [screen.view.centre.x, screen.view.centre.y],
            "fit": screen.view.zoom <= 1.0,
        }),
    })
}

fn cmd_state(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        campaign: Option<String>,
    }
    let params: Params = parse_params(params)?;
    let configs = CampaignConfigs::get();

    let campaign_index = match &params.campaign {
        Some(id) => configs.find_by_id(id).ok_or_else(|| {
            let ids: Vec<&str> = configs.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
            CommandError::not_found(format!("No campaign has the id '{id}'. Campaigns: {}.", ids.join(", ")))
        })?,
        None => world_map_screen::screen_info().and_then(|screen| screen.campaign).unwrap_or_else(world_map::default_campaign),
    };
    let campaign = configs.campaign(campaign_index).ok_or_else(|| CommandError::not_found("There are no campaigns."))?;
    let provider = world_map::provider(campaign_index).ok_or_else(|| {
        CommandError::not_found(format!("Campaign '{}' has no world map ({}.json).", campaign.id, campaign.world_map_name()))
    })?;
    let map = provider.map();
    let screen = world_map_screen::screen_info();

    let cities: Vec<Value> = provider.cities().iter().map(|city| city_json(&provider, city)).collect();
    let routes: Vec<Value> = provider.routes().iter().map(route_json).collect();
    let labels: Vec<Value> = map
        .labels
        .iter()
        .map(|label| {
            json!({
                "text": realm_text(&label.text),
                "key": label.text,
                "position": label.position,
                "style": label.style.to_string(),
                "own": label.own,
                "size": label.size,
                "letter_spacing": label.letter_spacing,
                "angle": label.angle,
                "align": label.align.to_string(),
            })
        })
        .collect();

    done(json!({
        "campaign": campaign.id,
        "hidden": campaign.hidden,
        "realm": campaign.realm.to_string(),
        "world_map": campaign.world_map_name(),
        "playable": profile::get().is_playable(configs, campaign_index),
        "extent": map.extent,
        "compass": map.compass,
        "choice_box": map.choice_box,
        "cities": cities,
        "routes": routes,
        "labels": labels,
        "uncharted": map
            .uncharted
            .iter()
            .map(|area| {
                json!({
                    "title": realm_text(&area.title), "title_key": area.title,
                    "subtitle": realm_text(&area.subtitle), "subtitle_key": area.subtitle,
                    "outline": area.outline, "label": area.label,
                })
            })
            .collect::<Vec<_>>(),
        "threats": map
            .threats
            .iter()
            .map(|threat| {
                json!({
                    "title": realm_text(&threat.title), "title_key": threat.title,
                    "subtitle": realm_text(&threat.subtitle), "subtitle_key": threat.subtitle,
                    "position": threat.position, "arrow": threat.arrow,
                })
            })
            .collect::<Vec<_>>(),
        "next_mission": provider.next_mission_choice().map(|choice| choice_json(&choice)),
        "selected": screen.as_ref().filter(|screen| screen.campaign == Some(campaign_index)).and_then(|screen| screen.selected.clone()),
        "screen_open": screen.is_some(),
        "screen": screen.as_ref().map(screen_json),
    }))
}

// A Realm string as the player reads it.
fn realm_text(key: &str) -> &str {
    engine::ui::text::get(engine::ui::text::UiTextCategory::Realm, key)
}

fn city_json(provider: &impl WorldMapProvider, city: &MapCity) -> Value {
    json!({
        "id": city.id,
        "name": city.name(),
        "kind": city.kind.to_string(),
        "owner": city.owner,
        "role": city.role.map(|role| role.to_string()),
        "position": city.position,
        "label": city.label.to_string(),
        "player_owned": city.player_owned,
        "founded_by_mission": city.founded_by_mission,
        "current": city.current,
        "panel": city.has_trade.then(|| provider.city_panel(&city.id)).flatten().map(|panel| panel_json(&panel)),
    })
}

fn panel_json(panel: &CityPanel) -> Value {
    let offers = |offers: &[world_map::provider::TradeOffer]| {
        offers.iter().map(|offer| json!({"good": offer.good, "price": offer.price})).collect::<Vec<_>>()
    };
    json!({
        "name": panel.name,
        "subtitle": panel.subtitle,
        "role": panel.role,
        "relation": panel.relation.as_ref().map(|relation| json!({"level": relation.level, "max": relation.max, "word": relation.word})),
        "route_line": panel.route_line,
        "route_detail": panel.route_detail,
        "sells": offers(&panel.sells),
        "buys": offers(&panel.buys),
        "request": panel.request.as_ref().map(|request| json!({"goods": request.goods, "within": request.within})),
    })
}

fn route_json(route: &MapRoute) -> Value {
    json!({
        "id": route.id,
        "kind": route.kind.to_string(),
        "state": route.state.to_string(),
        "points": route.points,
        "cities": route.cities,
    })
}

fn choice_json(choice: &NextMissionChoice) -> Value {
    json!({
        "mission": choice.mission_index,
        "title": choice.title,
        "subtitle": choice.subtitle,
        "sites": choice.sites.iter().map(|site| json!({
            "site": site.site.to_string(),
            "name": site.name,
            "position": site.position,
            "label": site.label.to_string(),
            "mission": site.mission_index,
        })).collect::<Vec<_>>(),
    })
}
