use game::{
    campaign::config::{CampaignConfigs, MissionPartnerDef, MissionTradeDef},
    config::{GameConfigs, TradeRules},
    sim::{
        finance::LedgerCategory,
        resources::ResourceKind,
        trade::{CityTrade, TradeLimit},
    },
    trade::{self, PartnerTerms},
    world_map::{
        WorldMapConfigs,
        config::{RouteKind, TradeGoodDef, WorldMapDef, trade_goods_problems},
    },
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for trade (docs/plans/trade.md): Phase 0's data and state.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Trade", &[
        test_utils::test_fn!(test_a_new_game_trades_with_no_one),
        test_utils::test_fn!(test_the_traded_goods_are_checked),
        test_utils::test_fn!(test_campaign_one_has_its_land_partners),
        test_utils::test_fn!(test_a_mission_block_starts_trade),
        test_utils::test_fn!(test_a_block_without_partners_trades_with_no_one),
        test_utils::test_fn!(test_a_custom_game_trades_with_the_land_partners),
        test_utils::test_fn!(test_routes_limits_and_the_new_year),
        test_utils::test_fn!(test_the_mission_block_is_checked),
        test_utils::test_fn!(test_the_state_round_trips_a_save),
        test_utils::test_fn!(test_a_load_drops_partners_without_terms),
        test_utils::test_fn!(test_the_ledger_has_the_trade_lines),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn world_maps() -> &'static WorldMapConfigs {
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::load();
    }
    if !WorldMapConfigs::is_initialized() {
        WorldMapConfigs::load();
    }
    WorldMapConfigs::get()
}

fn dragon_legacy() -> &'static WorldMapDef {
    world_maps().maps.get("dragon_legacy").expect("Campaign I's world map")
}

fn rules() -> &'static TradeRules {
    &GameConfigs::get().sim.trade
}

fn partner(city: &str, open: bool) -> MissionPartnerDef {
    MissionPartnerDef { city: city.into(), open, entry: None, open_cost: None, every_months: None, sells: None, buys: None }
}

fn good(name: &str, price: u32, per_year: Option<u32>) -> TradeGoodDef {
    TradeGoodDef { good: name.into(), price, per_year }
}

fn block(partners: Vec<MissionPartnerDef>) -> MissionTradeDef {
    MissionTradeDef { enabled: None, partners }
}

fn terms_of(city: &str) -> PartnerTerms {
    trade::terms_on(dragon_legacy(), &partner(city, false)).unwrap_or_else(|| panic!("terms for '{city}'"))
}

// A game trading with the clans (open) and the delta (closed) on Campaign I's map, in month 25.
fn started() -> CityTrade {
    world_maps();
    let mut state = CityTrade::new();
    trade::start(
        &mut state,
        "dragon_legacy",
        &block(vec![partner("clan_camps", true), partner("de_son", false)]),
        25,
        rules(),
    );
    state
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

fn test_a_new_game_trades_with_no_one() {
    let env = TestEnvironment::new();
    let trade = env.sim.trade();
    assert!(!trade.is_enabled());
    assert!(trade.partners().is_empty() && trade.requests().is_empty() && trade.request_log().is_empty());
    assert_eq!(trade.requests_met(), 0);
}

// A world file's goods: one the game trades by its ResourceKind name with a yearly amount, or a shown
// one by its key without.
fn test_the_traded_goods_are_checked() {
    assert!(trade_goods_problems(&[good("Wood", 7, Some(100)), good("trade_good.salt", 24, None)]).is_empty());
    assert_eq!(good("RoofTiles", 1, Some(1)).resource(), Some(ResourceKind::RoofTiles));
    assert_eq!(good("RoofTiles", 1, Some(1)).display_name(), "Roof Tiles");
    assert_eq!(good("trade_good.salt", 1, None).resource(), None);

    let problems = |goods: &[TradeGoodDef]| trade_goods_problems(goods);
    assert_eq!(problems(&[good("Wood", 7, None)]).len(), 1, "a traded good needs per_year");
    assert_eq!(problems(&[good("Wood", 7, Some(0))]).len(), 1, "of 1 or more");
    assert_eq!(problems(&[good("trade_good.salt", 7, Some(5))]).len(), 1, "a shown good takes none");
    assert_eq!(problems(&[good("Gold", 7, Some(5))]).len(), 1, "gold isn't a good");
    assert_eq!(problems(&[good("Timber", 7, Some(5))]).len(), 1, "neither a good nor a key");
    assert_eq!(problems(&[good("Wood", 0, Some(5))]).len(), 1, "a price of 0");
    assert_eq!(problems(&[good("Wood", 7, Some(5)), good("Wood", 8, Some(5))]).len(), 1, "given twice");

    // The shipped files have no errors (world_map.rs checks them all; this is the trade part).
    for (name, map) in &world_maps().maps {
        for city in &map.cities {
            if let Some(trade) = &city.trade {
                let errors: Vec<String> =
                    trade_goods_problems(&trade.sells).into_iter().chain(trade_goods_problems(&trade.buys)).collect();
                assert!(errors.is_empty(), "{name}, {}: {errors:?}", city.id);
            }
        }
    }
}

// The clans by the hill track, open from Chợ Đèo; the delta's Đê Sơn by a closed road; Vijara's
// goods are shown only.
fn test_campaign_one_has_its_land_partners() {
    let clans = terms_of("clan_camps");
    assert_eq!(clans.route, "cho_deo_clans");
    assert_eq!(clans.route_kind, RouteKind::Land);
    assert!(!clans.by_water());
    assert_eq!(clans.from, "cho_deo");
    assert_eq!(clans.open_cost, 0);
    let goods = |list: &[trade::GoodTerms]| list.iter().map(|terms| terms.good).collect::<Vec<_>>();
    assert_eq!(goods(&clans.sells), [ResourceKind::Wood, ResourceKind::Metal]);
    assert_eq!(goods(&clans.buys), [ResourceKind::Rice, ResourceKind::Pottery]);
    assert!(clans.sells(ResourceKind::Wood).is_some_and(|wood| wood.per_year > 0 && wood.price > 0));
    assert!(clans.buys(ResourceKind::Wood).is_none());

    let delta = terms_of("de_son");
    assert_eq!(delta.route, "cho_deo_de_son");
    assert_eq!(delta.route_kind, RouteKind::Land);
    assert!(delta.open_cost > 0, "opened for gold");
    assert_eq!(goods(&delta.sells), [ResourceKind::Bricks, ResourceKind::RoofTiles]);
    assert_eq!(goods(&delta.buys), [ResourceKind::Wood, ResourceKind::Metal]);
    assert_eq!(
        dragon_legacy().route("cho_deo_de_son").map(|route| route.cities.clone()),
        Some(vec!["cho_deo".to_string(), "de_son".to_string()])
    );

    let vijara = terms_of("vijara");
    assert!(vijara.by_water(), "by sea");
    assert!(vijara.sells.is_empty() && vijara.buys.is_empty(), "its goods are shown only, until the game has them");

    // A neighbour without terms isn't a partner.
    assert!(trade::terms_on(dragon_legacy(), &partner("gate_of_the_north", false)).is_none());
    assert!(trade::terms_on(dragon_legacy(), &partner("nowhere", false)).is_none());
}

fn test_a_mission_block_starts_trade() {
    world_maps();
    let mut overridden = partner("de_son", false);
    overridden.every_months = Some(6);
    overridden.open_cost = Some(50);
    overridden.sells = Some(vec![good("Bricks", 11, Some(30))]);
    overridden.entry = Some([3, 4]);
    let partners =
        vec![partner("clan_camps", true), overridden, partner("gate_of_the_north", true), partner("clan_camps", false)];
    let mut state = CityTrade::new();
    trade::start(&mut state, "dragon_legacy", &block(partners), 25, rules());

    assert!(state.is_enabled());
    assert_eq!(state.world_map(), "dragon_legacy");
    let cities: Vec<&str> = state.partners().iter().map(|partner| partner.city()).collect();
    assert_eq!(cities, ["clan_camps", "de_son"], "a city without terms and one named twice are left out");

    let clans = state.partner("clan_camps").unwrap();
    assert!(clans.open);
    assert_eq!(clans.next_arrival, Some(25 + rules().first_arrival_months));
    assert!(ResourceKind::all().iter().all(|good| clans.limit(good).is_off()), "the limits start off");
    let delta = state.partner("de_son").unwrap();
    assert!(!delta.open && delta.next_arrival.is_none());
    assert_eq!(delta.def.entry, Some([3, 4]));

    let terms = trade::terms(&state, "de_son").unwrap();
    assert_eq!(terms.every_months, 6);
    assert_eq!(terms.open_cost, 50);
    assert_eq!(terms.sells.len(), 1);
    assert_eq!((terms.sells[0].price, terms.sells[0].per_year), (11, 30));
    assert_eq!(terms.buys.len(), 2, "what it buys is the world map's");
    assert!(trade::terms(&state, "vijara").is_none(), "not a partner of this game");

    // Starting again replaces it all.
    trade::start(&mut state, "dragon_legacy", &block(vec![partner("de_son", true)]), 3, rules());
    assert_eq!(state.partners().len(), 1);
    assert!(state.partner("clan_camps").is_none());
}

fn test_a_block_without_partners_trades_with_no_one() {
    world_maps();
    let mut state = started();
    trade::start(&mut state, "dragon_legacy", &MissionTradeDef::default(), 0, rules());
    assert!(!state.is_enabled() && state.partners().is_empty());

    // Turned off with partners.
    let mut off = block(vec![partner("clan_camps", true)]);
    off.enabled = Some(false);
    trade::start(&mut state, "dragon_legacy", &off, 0, rules());
    assert!(!state.is_enabled());
    assert_eq!(state.partners().len(), 1);
    state.set_enabled(true);
    assert!(state.is_enabled());

    // A world map that isn't there leaves every partner out.
    trade::start(&mut state, "no_such_map", &block(vec![partner("clan_camps", true)]), 0, rules());
    assert!(!state.is_enabled() && state.partners().is_empty());
    state.set_enabled(true);
    assert!(!state.is_enabled(), "nothing to trade with");
}

// A custom map without a trade block trades with its realm's land partners, closed.
fn test_a_custom_game_trades_with_the_land_partners() {
    world_maps();
    assert_eq!(trade::realm_world_map(game::realm::Realm::DaiLong).as_deref(), Some("dragon_legacy"));
    let land = trade::land_partners(dragon_legacy());
    let cities: Vec<&str> = land.partners.iter().map(|partner| partner.city.as_str()).collect();
    assert_eq!(cities, ["de_son", "clan_camps"], "Vijara is by sea, the Gate of the North has no terms");
    assert!(land.partners.iter().all(|partner| !partner.open));
    assert!(land.is_enabled());

    // Every realm's world map is there.
    for realm in [game::realm::Realm::Kambara, game::realm::Realm::Tampadesa] {
        let name = trade::realm_world_map(realm).expect("a world map");
        assert!(trade::world_map(&name).is_some(), "{name}");
    }
}

fn test_routes_limits_and_the_new_year() {
    let mut state = started();
    assert!(!state.open_route("clan_camps", 30, rules()), "already open");
    assert!(state.open_route("de_son", 30, rules()));
    assert_eq!(state.partner("de_son").unwrap().next_arrival, Some(30 + rules().first_arrival_months));
    assert!(!state.open_route("vijara", 30, rules()), "not a partner");

    assert!(state.set_limit("clan_camps", ResourceKind::Wood, TradeLimit::Import(40)));
    assert!(state.set_limit("clan_camps", ResourceKind::Rice, TradeLimit::Export(100)));
    assert!(!state.set_limit("vijara", ResourceKind::Wood, TradeLimit::Import(1)));
    let clans = state.partner("clan_camps").unwrap();
    assert_eq!(clans.limit(ResourceKind::Wood), TradeLimit::Import(40));
    assert_eq!(clans.limit(ResourceKind::Rice), TradeLimit::Export(100));
    assert!(clans.limit(ResourceKind::Metal).is_off());

    assert!(state.close_route("clan_camps"));
    assert!(!state.close_route("clan_camps"));
    let clans = state.partner("clan_camps").unwrap();
    assert!(!clans.open && clans.next_arrival.is_none());
    assert_eq!(clans.limit(ResourceKind::Wood), TradeLimit::Import(40), "closing keeps the limits");

    let delta = state.partner_mut("de_son").unwrap();
    delta.sold[ResourceKind::Bricks.index()] = 20;
    delta.bought[ResourceKind::Wood.index()] = 12;
    state.new_year();
    let delta = state.partner("de_son").unwrap();
    assert_eq!(delta.sold[ResourceKind::Bricks.index()], 0);
    assert_eq!(delta.bought[ResourceKind::Wood.index()], 0);
}

fn test_the_mission_block_is_checked() {
    let map = dragon_legacy();
    assert!(map.trade_problems(&block(vec![partner("clan_camps", true), partner("de_son", false)])).is_empty());

    let mut bad_override = partner("de_son", false);
    bad_override.sells = Some(vec![good("trade_good.salt", 3, None)]);
    let mut never = partner("clan_camps", false);
    never.every_months = Some(0);
    let problems = map.trade_problems(&block(vec![
        partner("nowhere", false),
        partner("gate_of_the_north", false),
        bad_override,
        never,
        partner("de_son", true),
    ]));
    assert_eq!(problems.len(), 5, "{problems:#?}");

    // Every mission's block in campaigns.json passes (WorldMapConfigs::validate checks them at boot).
    let campaigns = CampaignConfigs::get();
    for campaign in &campaigns.campaigns {
        let Some(map) = world_maps().maps.get(campaign.world_map_name()) else {
            continue;
        };
        for (index, mission) in campaign.missions.iter().enumerate() {
            if let Some(block) = &mission.trade {
                let problems = map.trade_problems(block);
                assert!(problems.is_empty(), "{}, mission {index}: {problems:?}", campaign.id);
            }
        }
    }
}

fn test_the_state_round_trips_a_save() {
    let mut state = started();
    state.set_limit("clan_camps", ResourceKind::Wood, TradeLimit::Import(40));
    state.set_limit("clan_camps", ResourceKind::RoofTiles, TradeLimit::Export(8));
    let clans = state.partner_mut("clan_camps").unwrap();
    clans.price_offsets[ResourceKind::Wood.index()] = 0.12;
    clans.sold[ResourceKind::Wood.index()] = 16;

    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains(r#""Wood":{"Import":40}"#), "limits are saved by good name: {json}");
    assert!(json.contains(r#""RoofTiles":{"Export":8}"#), "{json}");
    assert!(!json.contains(r#""Metal""#), "limits left off aren't written: {json}");
    let back: CityTrade = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    let clans = back.partner("clan_camps").unwrap();
    assert_eq!(clans.limit(ResourceKind::Wood), TradeLimit::Import(40));
    assert_eq!(clans.price_offset(ResourceKind::Wood), 0.12);
    assert_eq!(clans.sold[ResourceKind::Wood.index()], 16);
    assert!(back.is_enabled());

    // What the v11 -> v12 migration writes reads as a game without trade.
    let migrated = r#"{"enabled": false, "world_map": "", "partners": [], "requests": [], "request_log": []}"#;
    let empty: CityTrade = serde_json::from_str(migrated).unwrap();
    assert!(!empty.is_enabled() && empty.partners().is_empty());
    assert_eq!(serde_json::to_value(&empty).unwrap(), serde_json::to_value(CityTrade::new()).unwrap());
}

fn test_a_load_drops_partners_without_terms() {
    let mut state = started();
    trade::sync_on_load(&mut state);
    assert_eq!(state.partners().len(), 2, "both still have terms");

    // A partner the world map no longer has terms for, as after an edit of its file.
    let json = serde_json::to_string(&state).unwrap().replace(r#""city":"de_son""#, r#""city":"gone_city""#);
    let mut edited: CityTrade = serde_json::from_str(&json).unwrap();
    assert!(edited.partner("gone_city").is_some());
    trade::sync_on_load(&mut edited);
    let cities: Vec<&str> = edited.partners().iter().map(|partner| partner.city()).collect();
    assert_eq!(cities, ["clan_camps"]);
    assert!(edited.is_enabled());

    let json = serde_json::to_string(&edited).unwrap().replace(r#""city":"clan_camps""#, r#""city":"gone_too""#);
    let mut none_left: CityTrade = serde_json::from_str(&json).unwrap();
    trade::sync_on_load(&mut none_left);
    assert!(none_left.partners().is_empty() && !none_left.is_enabled());
}

fn test_the_ledger_has_the_trade_lines() {
    assert!(LedgerCategory::Exports.is_income());
    assert!(!LedgerCategory::Imports.is_income() && !LedgerCategory::Routes.is_income());
    assert_eq!([LedgerCategory::Exports, LedgerCategory::Imports, LedgerCategory::Routes].map(LedgerCategory::name), [
        "Exports", "Imports", "Routes"
    ]);

    let mut env = TestEnvironment::new();
    let finance = env.sim.finance_mut();
    finance.earn(LedgerCategory::Exports, 120);
    finance.spend(LedgerCategory::Imports, 30);
    finance.spend(LedgerCategory::Routes, 400);
    let year = finance.this_year();
    assert_eq!(year.total(LedgerCategory::Exports), 120);
    assert_eq!(year.total(LedgerCategory::Imports), 30);
    assert_eq!(year.total(LedgerCategory::Routes), 400);
    let json = serde_json::to_string(env.sim.finance()).unwrap();
    assert!(json.contains(r#""Exports":120"#) && json.contains(r#""Routes":400"#), "saved by name: {json}");
}
