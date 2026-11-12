use engine::ui::text::{self, UiTextCategory};
use game::{
    campaign::{
        ActiveMission,
        config::{CampaignConfigs, ChoiceSite},
    },
    menu::skin,
    profile::Profile,
    realm::Realm,
    world_map::{ConfigWorldMapProvider, WorldMapConfigs, WorldMapProvider, config::WorldMapDef},
};

mod test_utils;

// ----------------------------------------------
// Integration tests for the world map data (docs/plans/realm-ui.md, Phase 2: Data):
// the shipped campaigns.json and assets/configs/game/world/*.json.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("World map", &[
        test_utils::test_fn!(test_world_maps_load_and_resolve),
        test_utils::test_fn!(test_text_keys_exist),
        test_utils::test_fn!(test_hidden_campaigns_stay_out_of_the_menus),
        test_utils::test_fn!(test_choice_steps),
        test_utils::test_fn!(test_player_cities_and_routes),
        test_utils::test_fn!(test_trade_panels),
        test_utils::test_fn!(test_next_mission_choice),
        test_utils::test_fn!(test_validation_finds_errors),
        test_utils::test_fn!(test_base_art_maps_the_world_files),
    ]);
}

const TEST_CAMPAIGN: &str = "test_choices";

fn configs() -> (&'static CampaignConfigs, &'static WorldMapConfigs) {
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::load();
    }
    if !WorldMapConfigs::is_initialized() {
        WorldMapConfigs::load();
    }
    (CampaignConfigs::get(), WorldMapConfigs::get())
}

fn index_of(id: &str) -> usize {
    configs().0.find_by_id(id).unwrap_or_else(|| panic!("campaigns.json should have the campaign '{id}'"))
}

fn provider<'a>(profile: &'a Profile, campaign: &str, active: Option<ActiveMission>) -> ConfigWorldMapProvider<'a> {
    let (campaigns, maps) = configs();
    ConfigWorldMapProvider::new(campaigns, maps, index_of(campaign), profile, active).expect("the campaign should have a world map")
}

fn city_ids(provider: &impl WorldMapProvider) -> Vec<String> {
    provider.cities().into_iter().map(|city| city.id).collect()
}

fn route_ids(provider: &impl WorldMapProvider) -> Vec<String> {
    provider.routes().into_iter().map(|route| route.id).collect()
}

// Every campaign has its world map, the campaigns' missions and choices check out, and every
// reference in the files (route cities, trade routes, owners) resolves.
fn test_world_maps_load_and_resolve() {
    let (campaigns, maps) = configs();
    assert_eq!(campaigns.validate(), Vec::<String>::new());
    assert_eq!(maps.validate(campaigns), Vec::<String>::new());

    for (index, campaign) in campaigns.campaigns.iter().enumerate() {
        let map = maps.for_campaign(campaigns, index).unwrap_or_else(|| panic!("campaign '{}' should have a world map", campaign.id));
        assert_eq!(map.extent, [1000.0, 672.0], "the generator's map space");
        // The realm's own cities, and a capital among them.
        let own: Vec<_> = map.cities.iter().filter(|city| city.owner == campaign.realm.to_string()).collect();
        assert!(!own.is_empty(), "campaign '{}' shows its realm's cities", campaign.id);
        // Every mission founds a city on the map.
        for (mission_index, mission) in campaign.missions.iter().enumerate() {
            let city = mission.city.as_ref().unwrap_or_else(|| panic!("campaign '{}', mission {mission_index} should found a city", campaign.id));
            assert!(map.contains(city.position));
        }
    }
    let three_files: Vec<&str> = maps.maps.keys().map(String::as_str).collect();
    assert_eq!(three_files, ["dragon_legacy", "temple_mountain", "ten_thousand_temples"]);
}

// Every key the world maps and the mission cities and choices (their sites, the box's line) name is
// in en.json.
fn test_text_keys_exist() {
    let (campaigns, maps) = configs();
    let mut keys: Vec<(UiTextCategory, String)> = maps.maps.values().flat_map(|map| map.text_keys()).collect();
    for campaign in &campaigns.campaigns {
        for mission in &campaign.missions {
            keys.extend(mission.city.iter().map(|city| (UiTextCategory::Realm, city.name.clone())));
            keys.extend(mission.choices.iter().map(|choice| (UiTextCategory::Realm, choice.name.clone())));
            keys.extend(mission.choices.iter().filter_map(|choice| choice.phrase.clone().map(|key| (UiTextCategory::Realm, key))));
            keys.extend(mission.choice_text.iter().map(|key| (UiTextCategory::Menu, key.clone())));
        }
    }
    let missing: Vec<String> = keys.iter().filter(|(category, key)| text::peek(*category, key, None).1.is_none()).map(|(category, key)| format!("{category} '{key}'")).collect();
    assert!(missing.is_empty(), "en.json lacks: {missing:?}");
}

// The hidden test campaign: after the others, out of the numbering, the playable list, the home
// menu's realm and Custom Game's unlocks; reached only by hand (profile.set).
fn test_hidden_campaigns_stay_out_of_the_menus() {
    let (configs, _) = configs();
    let hidden = index_of(TEST_CAMPAIGN);
    assert!(configs.campaigns[hidden].hidden);
    let first_hidden = configs.campaigns.iter().position(|campaign| campaign.hidden).unwrap();
    assert!(configs.campaigns[first_hidden..].iter().all(|campaign| campaign.hidden), "hidden campaigns go last");
    // One per realm, each on its realm's map, so every realm's choice box can be shown.
    for (id, realm) in [(TEST_CAMPAIGN, Realm::DaiLong), ("test_choices_kambara", Realm::Kambara), ("test_choices_tampadesa", Realm::Tampadesa)] {
        let campaign = &configs.campaigns[index_of(id)];
        assert!(campaign.hidden && campaign.realm == realm, "{id}");
        assert!(campaign.missions.iter().any(|mission| !mission.choices.is_empty()), "{id} offers a choice");
    }

    // Campaigns I to III keep their numbers.
    assert_eq!(configs.campaign_number(index_of("dragon_legacy")), Some(1));
    assert_eq!(configs.campaign_number(index_of("temple_mountain")), Some(2));
    assert_eq!(configs.campaign_number(index_of("ten_thousand_temples")), Some(3));
    assert_eq!(configs.campaign_number(hidden), None);
    assert!(configs.visible().all(|(index, _)| index != hidden));

    let mut profile = Profile::new();
    assert!(!profile.is_playable(configs, hidden));
    profile.mark_reached(TEST_CAMPAIGN);
    assert!(profile.is_playable(configs, hidden), "reached by hand, it can be played");
    assert!(!profile.playable_campaigns(configs).contains(&hidden), "but no menu lists it");
    assert_eq!(profile.latest_reached_realm(configs), None, "and it's never the home menu's realm");

    // Playing it unlocks no realm for Custom Game; finishing it reaches nothing.
    profile.custom_realms.clear();
    profile.mission_started(configs, hidden, 0);
    assert!(profile.custom_realms.is_empty());
    let last = configs.campaigns[hidden].missions.len() - 1;
    profile.mission_finished(configs, hidden, last);
    assert!(profile.record(TEST_CAMPAIGN).is_some_and(|record| record.finished));
    assert_eq!(profile.campaigns.iter().filter(|record| record.reached).count(), 1);

    // Finishing Campaign III, the last visible one, doesn't reach it.
    assert_eq!(configs.next_visible_after(index_of("ten_thousand_temples")), None);
    assert_eq!(configs.next_visible_after(index_of("dragon_legacy")), Some(index_of("temple_mountain")));
    assert_eq!(configs.next_visible_after(hidden), None);
}

// The test campaign: a mission, then a choice point whose site B starts a variant.
fn test_choice_steps() {
    let (configs, _) = configs();
    let campaign = &configs.campaigns[index_of(TEST_CAMPAIGN)];
    assert_eq!(campaign.steps(), [0, 1]);
    assert!(campaign.is_variant(2));
    assert_eq!(campaign.step_of(2), 1);
    assert_eq!(campaign.choice_targets(1), [1, 2]);
    assert_eq!(campaign.mission_count(), 2);
    assert_eq!(campaign.mission_number(2), 2);
    assert!(campaign.is_last_step(1) && campaign.is_last_step(2) && !campaign.is_last_step(0));
    assert_eq!(campaign.next_step_after(0), Some(1));
    assert_eq!(campaign.next_step_after(2), None);

    let index = index_of(TEST_CAMPAIGN);
    let mut profile = Profile::new();
    assert_eq!(profile.next_mission(configs, index), Some(0));
    profile.mission_finished(configs, index, 0);
    assert_eq!(profile.next_mission(configs, index), Some(1), "the choice point is next");
    profile.mission_finished(configs, index, 2);
    assert!(profile.record(TEST_CAMPAIGN).is_some_and(|record| record.finished), "the variant finishes the last step");
    assert_eq!(profile.next_mission(configs, index), Some(0), "all done: from the start again");

    // Campaign I has no choices: its steps are its missions.
    let first = &configs.campaigns[index_of("dragon_legacy")];
    assert_eq!(first.steps(), (0..first.missions.len()).collect::<Vec<_>>());
}

// The player's cities: the map's own (its capital), the finished missions' and the one being played; routes
// show once their cities do.
fn test_player_cities_and_routes() {
    let (configs, _) = configs();
    let campaign = index_of("dragon_legacy");
    let mut profile = Profile::new();

    let fresh = provider(&profile, "dragon_legacy", None);
    let player: Vec<String> = fresh.player_cities().into_iter().map(|city| city.id).collect();
    assert_eq!(player, ["hoa_thanh"], "missions 4 and 5's placeholder cities aren't the player's");
    assert!(fresh.cities().iter().any(|city| city.id == "de_son" && !city.player_owned && city.owner == "dailong"));
    assert!(!city_ids(&fresh).contains(&"phu_sa".to_string()));
    assert!(fresh.cities().iter().any(|city| city.id == "vijara" && !city.player_owned && city.owner == "sea_kingdom"));
    let routes = route_ids(&fresh);
    assert!(routes.contains(&"hoa_thanh_north".to_string()));
    assert!(!routes.contains(&"ben_gom_vijara".to_string()), "Bến Gốm isn't founded yet");
    assert!(!routes.contains(&"phu_sa_de_son".to_string()));

    // Mission 1 finished, mission 2 being played.
    profile.mission_finished(configs, campaign, 0);
    let active = ActiveMission { campaign_id: campaign, mission_index: 1, completed: false };
    let playing = provider(&profile, "dragon_legacy", Some(active.clone()));
    let cities = playing.cities();
    let phu_sa = cities.iter().find(|city| city.id == "phu_sa").expect("Phù Sa is founded");
    assert!(phu_sa.player_owned && phu_sa.founded_by_mission == Some(0) && !phu_sa.current);
    assert_eq!(phu_sa.name(), "Phù Sa");
    let ben_gom = cities.iter().find(|city| city.id == "ben_gom").expect("Bến Gốm is being founded");
    assert!(ben_gom.current && ben_gom.position == [836.0, 170.0]);
    assert!(!city_ids(&playing).contains(&"than_khe".to_string()));
    let routes = route_ids(&playing);
    assert!(routes.contains(&"ben_gom_vijara".to_string()) && routes.contains(&"phu_sa_de_son".to_string()));
    let sea = playing.routes().into_iter().find(|route| route.id == "ben_gom_vijara").unwrap();
    assert_eq!(sea.points.len(), 6);

    // Another campaign's mission isn't this map's.
    let elsewhere = provider(&profile, "temple_mountain", Some(active));
    assert!(elsewhere.cities().iter().all(|city| !city.current));
    let own: Vec<String> = elsewhere.player_cities().into_iter().map(|city| city.id).collect();
    assert_eq!(own, ["yasonagara", "suryagiri"]);
    assert_eq!(configs.campaigns[elsewhere.campaign_index()].realm, Realm::Kambara);
}

// The side panels' static trade data, as the mockups show it.
fn test_trade_panels() {
    let (configs, _) = configs();
    let mut profile = Profile::new();

    let panel = provider(&profile, "dragon_legacy", None).city_panel("vijara").expect("Vijara has a panel");
    assert_eq!(panel.name, "Vijara");
    assert_eq!(panel.subtitle, "The Sea Kingdom");
    assert_eq!(panel.role.as_deref(), Some("Trade partner"));
    let relation = panel.relation.as_ref().unwrap();
    assert_eq!((relation.level, relation.max, relation.word.as_str()), (4, 5, "Friendly"));
    assert_eq!(panel.route_line.as_deref(), Some("Sea route from —"), "until Bến Gốm is founded");
    assert_eq!(panel.route_detail.as_deref(), Some("Not yet open"), "its route isn't on the map yet");
    let sells: Vec<(&str, u32)> = panel.sells.iter().map(|offer| (offer.good.as_str(), offer.price)).collect();
    assert_eq!(sells, [("Salt", 24), ("Aloeswood", 60), ("Cotton", 32)]);
    let buys: Vec<(&str, u32)> = panel.buys.iter().map(|offer| (offer.good.as_str(), offer.price)).collect();
    assert_eq!(buys, [("Silk", 90), ("Ceramics", 45), ("Rice", 18)]);
    let request = panel.request.as_ref().unwrap();
    assert_eq!((request.goods.as_str(), request.within.as_str()), ("Ceramics × 30", "within 8 months"));

    profile.mission_finished(configs, index_of("dragon_legacy"), 1);
    let panel = provider(&profile, "dragon_legacy", None).city_panel("vijara").unwrap();
    assert_eq!(panel.route_line.as_deref(), Some("Sea route from Bến Gốm"));
    assert_eq!(panel.route_detail.as_deref(), Some("Open · junks every 3 months"));

    let hamsara = provider(&profile, "temple_mountain", None).city_panel("hamsara").unwrap();
    assert_eq!(hamsara.subtitle, "The Delta Ports");
    assert_eq!(hamsara.relation.as_ref().unwrap().word, "Allied");
    assert_eq!(hamsara.route_line.as_deref(), Some("Hill track from Pashchima"));
    assert_eq!(hamsara.route_detail.as_deref(), Some("Open · caravans every 4 months"));
    assert_eq!(hamsara.request.as_ref().unwrap().goods, "Palm sugar × 40");

    let yasonagara = provider(&profile, "ten_thousand_temples", None).city_panel("yasonagara").unwrap();
    assert_eq!(yasonagara.subtitle, "Kambara · the Temple Mountain");
    assert_eq!(yasonagara.route_line.as_deref(), Some("Hill road from Kyautthin"));
    assert_eq!(yasonagara.request.as_ref().unwrap().within, "within 10 months");

    // A city without trade data has a panel with its name and owner only; one not on the map, none.
    let gate = provider(&profile, "dragon_legacy", None).city_panel("gate_of_the_north").unwrap();
    assert_eq!(gate.subtitle, "The Northern Throne");
    assert!(gate.relation.is_none() && gate.sells.is_empty());
    assert!(provider(&Profile::new(), "dragon_legacy", None).city_panel("than_khe").is_none());
}

// The next mission's choice: shown once the choice point is next, while playing the mission before
// it, and gone once one of its sites is finished.
fn test_next_mission_choice() {
    let (configs, _) = configs();
    let index = index_of(TEST_CAMPAIGN);
    let mut profile = Profile::new();
    profile.mark_reached(TEST_CAMPAIGN);
    assert!(provider(&profile, TEST_CAMPAIGN, None).next_mission_choice().is_none(), "mission 1 has no choice");

    let playing_first = ActiveMission { campaign_id: index, mission_index: 0, completed: false };
    let choice = provider(&profile, TEST_CAMPAIGN, Some(playing_first)).next_mission_choice().expect("the choice point follows");
    assert_eq!(choice.mission_index, 1);
    assert_eq!(choice.title, "Next: Test: The Southern Shore");
    assert_eq!(choice.subtitle, "Choose Hội Thủy or Vân Cảng");
    let sites: Vec<(ChoiceSite, &str, [f32; 2], usize)> = choice.sites.iter().map(|site| (site.site, site.name.as_str(), site.position, site.mission_index)).collect();
    assert_eq!(sites, [(ChoiceSite::A, "Hội Thủy", [756.0, 316.0], 1), (ChoiceSite::B, "Vân Cảng", [854.0, 294.0], 2)]);

    assert!(!choice.open, "closed until the mission being played is finished");
    let finished_first = ActiveMission { campaign_id: index, mission_index: 0, completed: true };
    assert!(provider(&profile, TEST_CAMPAIGN, Some(finished_first)).next_mission_choice().is_some_and(|choice| choice.open));

    profile.mission_finished(configs, index, 0);
    let choice = provider(&profile, TEST_CAMPAIGN, None).next_mission_choice().expect("the profile's next step");
    assert_eq!(choice.mission_index, 1);
    assert!(choice.open, "open with no game being played");
    // Over another game (a custom game, another campaign's mission) the choice stays closed:
    // starting its mission would end that game.
    let over_other = provider(&profile, TEST_CAMPAIGN, None).with_other_game(true).next_mission_choice().expect("still shown");
    assert!(!over_other.open);
    let other_campaign = ActiveMission { campaign_id: index_of("dragon_legacy"), mission_index: 0, completed: true };
    let over_other = provider(&profile, TEST_CAMPAIGN, Some(other_campaign)).with_other_game(true).next_mission_choice().expect("still shown");
    assert!(!over_other.open, "another campaign's finished mission doesn't open it");

    // Playing site B's mission: nothing comes after it.
    let playing_b = ActiveMission { campaign_id: index, mission_index: 2, completed: false };
    let provider_b = provider(&profile, TEST_CAMPAIGN, Some(playing_b));
    assert!(provider_b.next_mission_choice().is_none());
    assert!(provider_b.cities().iter().any(|city| city.id == "van_cang" && city.current));

    profile.mission_finished(configs, index, 2);
    assert!(provider(&profile, TEST_CAMPAIGN, None).next_mission_choice().is_none());

    // Replaying mission 1 once all is finished: the step straight after it is the choice point
    // again, as campaign::advance_to_next_mission would go to.
    let replaying_first = ActiveMission { campaign_id: index, mission_index: 0, completed: true };
    let choice = provider(&profile, TEST_CAMPAIGN, Some(replaying_first)).next_mission_choice().expect("the step after the one played");
    assert_eq!(choice.mission_index, 1);

    // Campaign I's missions have no choices yet.
    assert!(provider(&Profile::new(), "dragon_legacy", None).next_mission_choice().is_none());
}

// A broken file and broken choices are reported, each problem once.
fn test_validation_finds_errors() {
    let map: WorldMapDef = serde_json::from_str(
        r#"{
            "extent": [100, 100], "compass": [50, 50], "choice_box": [200, 10],
            "cities": [
                { "id": "a", "name": "city.a", "position": [10, 10], "kind": "city", "owner": "nobody", "player": true },
                { "id": "a", "name": "city.a", "position": [10, 150], "kind": "capital", "owner": "dailong",
                  "trade": { "relation": 9, "route": { "route": "nowhere", "from": "b", "line": "x", "carrier": "y", "every_months": 1 } } }
            ],
            "routes": [ { "id": "r", "kind": "sea", "state": "open", "points": [[0, 0]], "cities": ["a", "missing"] } ]
        }"#,
    )
    .expect("the test map should parse");
    let errors = map.validate(&[]);
    for expected in ["choice box", "given twice", "owner 'nobody'", "off the map", "relations 9", "route 'nowhere'", "trades from 'b'", "1 points", "joins 'missing'", "is the player's"] {
        assert!(errors.iter().any(|error| error.contains(expected)), "expected an error about {expected}: {errors:?}");
    }
    assert_eq!(errors.len(), 10, "{errors:?}");

    // Unknown fields are refused.
    assert!(serde_json::from_str::<WorldMapDef>(r#"{"extent": [1, 1], "compass": [0, 0], "choice_box": [0, 0], "cities": [], "town": 1}"#).is_err());

    // A campaign's choices: a choice point needs sites A and B naming missions after it.
    let mut configs: CampaignConfigs = serde_json::from_str(
        r#"{ "campaigns": [ { "id": "c", "name": "C", "missions": [
            { "name": "One", "map": { "Preset": { "preset_number": 2 } } },
            { "name": "Two", "map": { "Preset": { "preset_number": 2 } },
              "choices": [ { "site": "B", "name": "city.x", "position": [1, 1], "mission": 0 },
                           { "site": "A", "name": "city.y", "position": [1, 1], "mission": "nope" } ] }
        ] } ] }"#,
    )
    .expect("the test campaigns should parse");
    let errors = configs.validate();
    assert!(errors.iter().any(|error| error.contains("two sites")), "{errors:?}");
    assert!(errors.iter().any(|error| error.contains("comes before it")), "{errors:?}");
    assert!(errors.iter().any(|error| error.contains("names no mission")), "{errors:?}");
    assert!(errors.iter().any(|error| error.contains("no site starts the choice point itself")), "{errors:?}");
    configs.campaigns[0].hidden = true;
    assert!(configs.validate().iter().all(|error| !error.contains("hidden")), "a hidden campaign last is fine");

    // A mistyped mission field is refused, not ignored.
    let typo = r#"{ "campaigns": [ { "id": "c", "name": "C", "missions": [
        { "name": "One", "map": { "Preset": { "preset_number": 2 } }, "cty": { "id": "x" } } ] } ] }"#;
    assert!(serde_json::from_str::<CampaignConfigs>(typo).is_err());

    // A visible campaign after a hidden one would take its index in saves.
    let configs: CampaignConfigs = serde_json::from_str(
        r#"{ "campaigns": [ { "id": "test", "name": "Test", "hidden": true, "missions": [] },
                            { "id": "late", "name": "Late", "missions": [] } ] }"#,
    )
    .expect("the test campaigns should parse");
    let errors = configs.validate();
    assert!(errors.iter().any(|error| error.contains("'late' comes after a hidden campaign")), "{errors:?}");
}

// Each campaign's map is its realm's base art (the skin's `world_map`), which covers the world
// file's extent, and a city of the file lands on the base where the mockup generator draws it.
// The generator places a city at common.P(x, y) = (52 + 1.29 x, 132 + 1.29 y) screen points, the
// map viewport starts at (52, 132), and ui-art draws the base at 2 pixels per point: content.py's
// Hoa Thành (742, 222) is at (1914.36, 572.76) pixels, Yasonagara (548, 410) in Kambara's map at
// (1413.84, 1057.8), Pugara (236, 258) in Tampadesa's at (608.88, 665.64).
// crates/tools/ui-art/tests checks every city against the generator itself.
fn test_base_art_maps_the_world_files() {
    let (campaigns, maps) = configs();
    let files = skin::read_skin_files().unwrap_or_else(|err| panic!("{err}"));
    let file = |realm: Realm| files.iter().find(|(file_realm, _)| *file_realm == realm).unwrap().1.clone();
    let art = |realm: Realm| skin::merge_skin(realm, &file).unwrap_or_else(|err| panic!("{err}")).world_map;

    for (index, campaign) in campaigns.campaigns.iter().enumerate() {
        let map = maps.for_campaign(campaigns, index).unwrap();
        let art = art(campaign.realm);
        assert!(!art.base.is_empty() && !art.frame.is_empty() && !art.backdrop.is_empty(), "{}: the skin names the art", campaign.id);
        assert_eq!(art.extent, map.extent, "{}: the base covers the world file's extent", campaign.id);
    }

    for (campaign, city, expected) in [
        ("dragon_legacy", "hoa_thanh", [1914.36, 572.76]),
        ("temple_mountain", "yasonagara", [1413.84, 1057.8]),
        ("ten_thousand_temples", "pugara", [608.88, 665.64]),
    ] {
        let index = index_of(campaign);
        let position = maps.for_campaign(campaigns, index).unwrap().city(city).unwrap().position;
        let pixel = art(campaigns.campaigns[index].realm).base_pixel(position);
        assert!(
            (pixel.x - expected[0]).abs() < 0.5 && (pixel.y - expected[1]).abs() < 0.5,
            "{campaign}: {city} lands on {pixel:?}, the generator draws it at {expected:?}"
        );
    }
}
