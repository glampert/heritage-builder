use common::{Size, coords::Cell};
use game::{
    building::BuildingKindAndId,
    campaign::config::{CampaignConfigs, MissionPartnerDef, MissionTradeDef},
    cheats,
    config::{GameConfigs, TradeRules},
    sim::{
        calendar::CalendarDate,
        finance::LedgerCategory,
        resources::ResourceKind,
        trade::{CityTrade, TradeLimit},
    },
    trade::{self, GoodTerms, NotSent, OpenRouteError, PartnerTerms, PriceTrend, TradeNotice, TradeSide, ships},
    unit::{
        config::UnitConfigKey,
        task::{UnitTaskShip, UnitTaskShipState, UnitTaskTrade},
    },
    world_map::{
        WorldMapConfigs,
        config::{RouteKind, TradeGoodDef, WorldMapDef, trade_goods_problems},
    },
};

mod test_utils;
use test_utils::{TestEnvironment, find_building, find_building_mut, find_unit_by_config, spawn_building, tick, tick_until};

// ----------------------------------------------
// Integration tests for trade (docs/plans/trade.md): the data and state (Phase 0), caravans (Phase 1),
// docks and ships (Phase 2) and prices (Phase 3).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Trade", &[
        test_utils::test_fn!(test_a_new_game_trades_with_no_one),
        test_utils::test_fn!(test_the_traded_goods_are_checked),
        test_utils::test_fn!(test_campaign_one_has_its_land_partners),
        test_utils::test_fn!(test_a_mission_block_starts_trade),
        test_utils::test_fn!(test_a_block_without_partners_trades_with_no_one),
        test_utils::test_fn!(test_a_custom_game_trades_with_the_realms_partners),
        test_utils::test_fn!(test_routes_limits_and_the_new_year),
        test_utils::test_fn!(test_the_mission_block_is_checked),
        test_utils::test_fn!(test_the_state_round_trips_a_save),
        test_utils::test_fn!(test_a_load_drops_partners_without_terms),
        test_utils::test_fn!(test_the_ledger_has_the_trade_lines),
        // Caravans (Phase 1):
        test_utils::test_fn!(test_a_caravan_sells_and_buys_store_by_store),
        test_utils::test_fn!(test_a_caravan_brings_only_what_the_limit_wants),
        test_utils::test_fn!(test_the_yearly_amounts_cap_the_trade),
        test_utils::test_fn!(test_the_treasury_pays_only_what_it_can),
        test_utils::test_fn!(test_a_caravan_without_a_store_leaves),
        test_utils::test_fn!(test_no_caravan_without_something_to_trade),
        test_utils::test_fn!(test_the_monthly_schedule),
        test_utils::test_fn!(test_opening_a_route_costs_gold),
        test_utils::test_fn!(test_easy_trade_moves_the_prices),
        test_utils::test_fn!(test_envoys_bring_gifts),
        test_utils::test_fn!(test_the_caravans_panel),
        // Docks and ships (Phase 2):
        test_utils::test_fn!(test_a_dock_stands_at_the_shore),
        test_utils::test_fn!(test_a_ship_moors_and_its_dockers_trade),
        test_utils::test_fn!(test_river_and_sea_routes_need_a_dock),
        test_utils::test_fn!(test_a_ship_without_a_dock_in_reach_turns_back),
        test_utils::test_fn!(test_ships_keep_out_of_canals),
        test_utils::test_fn!(test_a_busy_dock_keeps_a_ship_waiting),
        test_utils::test_fn!(test_the_monthly_schedule_sends_ships),
        // Prices (Phase 3):
        test_utils::test_fn!(test_trade_moves_prices_and_they_drift_back),
        test_utils::test_fn!(test_relations_scale_the_prices),
        test_utils::test_fn!(test_the_world_maps_panel_shows_the_prices_now),
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

// The clans by the hill track, open from Chợ Đèo; the delta's Đê Sơn by a closed road; Vijara by
// sea, trading metal for pottery and rice (its other goods are shown only).
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
    assert_eq!(vijara.route_kind, RouteKind::Sea);
    assert_eq!(goods(&vijara.sells), [ResourceKind::Metal], "salt and aloeswood are shown only");
    assert_eq!(goods(&vijara.buys), [ResourceKind::Pottery, ResourceKind::Rice], "silk is shown only");

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

// A custom map without a trade block trades with its realm's partners, by land, river or sea, closed.
fn test_a_custom_game_trades_with_the_realms_partners() {
    world_maps();
    assert_eq!(trade::realm_world_map(game::realm::Realm::DaiLong).as_deref(), Some("dragon_legacy"));
    let all = trade::map_partners(dragon_legacy());
    let cities: Vec<&str> = all.partners.iter().map(|partner| partner.city.as_str()).collect();
    assert_eq!(cities, ["de_son", "vijara", "clan_camps"], "the Gate of the North has no terms");
    assert!(all.partners.iter().all(|partner| !partner.open));
    assert!(all.is_enabled());

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

// ----------------------------------------------
// Caravans (Phase 1)
// ----------------------------------------------

const ENTRY: Cell = Cell::new(24, 36);
const STEP_SECS: f32 = 0.25;

// Grass with a storage yard and a granary, trading with the clans (open) and the delta (closed),
// each coming in at ENTRY, in month 25.
struct TradeMap {
    env: TestEnvironment,
    yard: BuildingKindAndId,
    granary: BuildingKindAndId,
}

fn trade_map() -> TradeMap {
    world_maps();
    let mut env = TestEnvironment::with_terrain(Size::new(48, 48), test_utils::find_terrain_def("grass"));
    let yard = spawn_building(&mut env, Cell::new(20, 22), "storage_yard");
    let granary = spawn_building(&mut env, Cell::new(28, 22), "granary");
    let mut clans = partner("clan_camps", true);
    clans.entry = Some([ENTRY.x, ENTRY.y]);
    let mut delta = partner("de_son", false);
    delta.entry = Some([ENTRY.x, ENTRY.y]);
    let vijara = partner("vijara", true);
    trade::start(env.sim.trade_mut(), "dragon_legacy", &block(vec![clans, delta, vijara]), 25, rules());
    trade::take_notices();
    TradeMap { env, yard, granary }
}

fn stock(map: &TradeMap, store: BuildingKindAndId, good: ResourceKind) -> u32 {
    find_building(&map.env, store).available_resources(good)
}

fn put(map: &mut TradeMap, store: BuildingKindAndId, good: ResourceKind, count: u32) {
    assert_eq!(find_building_mut(&mut map.env, store).receive_resources(good, count), count);
}

fn set_limit(map: &mut TradeMap, good: ResourceKind, limit: TradeLimit) {
    assert!(map.env.sim.trade_mut().set_limit("clan_camps", good, limit));
}

fn send(map: &mut TradeMap, city: &str) -> Result<(), NotSent> {
    let context = map.env.new_sim_context(0.0);
    trade::send(&context, city)
}

fn caravan(map: &TradeMap) -> Option<UnitTaskTrade> {
    let unit = find_unit_by_config(&map.env, UnitConfigKey::Trader)?;
    let unit = test_utils::find_unit(&map.env, unit);
    unit.current_task_as::<UnitTaskTrade>(map.env.sim.task_manager()).cloned()
}

// Ticks until the caravan has come and gone; returns the notice it left.
fn until_it_leaves(map: &mut TradeMap) -> TradeNotice {
    tick(&mut map.env, STEP_SECS); // It spawns with the step's commands.
    assert!(caravan(map).is_some(), "the caravan came in");
    tick_until(&mut map.env, 20_000, STEP_SECS, |env| find_unit_by_config(env, UnitConfigKey::Trader).is_none());
    let notices = trade::take_notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    notices.into_iter().next().unwrap()
}

// Real gold for the test's length: the harness ignores costs.
struct PaysForItself;

impl PaysForItself {
    fn new() -> Self {
        cheats::get_mut().ignore_tile_cost = false;
        Self
    }
}

impl Drop for PaysForItself {
    fn drop(&mut self) {
        cheats::get_mut().ignore_tile_cost = true;
    }
}

fn test_a_caravan_sells_and_buys_store_by_store() {
    let _gold = PaysForItself::new();
    let mut map = trade_map();
    let store = map.granary;
    put(&mut map, store, ResourceKind::Rice, 30);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(20));
    set_limit(&mut map, ResourceKind::Rice, TradeLimit::Export(10));
    map.env.sim.finance_mut().set_balance_unrecorded(1000);
    let balance = map.env.sim.finance().balance();

    send(&mut map, "clan_camps").unwrap();
    let notice = until_it_leaves(&mut map);

    // It sold the yard 20 wood at 7 and bought the granary's 20 rice above the limit at 6.
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 20);
    assert_eq!(stock(&map, map.granary, ResourceKind::Rice), 10);
    let clans = map.env.sim.trade().partner("clan_camps").unwrap();
    assert_eq!(clans.sold[ResourceKind::Wood.index()], 20);
    assert_eq!(clans.bought[ResourceKind::Rice.index()], 20);
    let finance = map.env.sim.finance();
    assert_eq!(finance.this_year().total(LedgerCategory::Imports), 140);
    assert_eq!(finance.this_year().total(LedgerCategory::Exports), 120);
    assert_eq!(finance.balance(), balance - 140 + 120);
    assert_eq!(notice, TradeNotice::Caravan {
        partner: "clan_camps".into(),
        sold: vec![(ResourceKind::Wood, 20)],
        bought: vec![(ResourceKind::Rice, 20)],
        paid: 140,
        earned: 120,
    });
    let (_, _, text) = trade::notice_message(&notice, "the Clan camps");
    assert_eq!(text, "A caravan from the Clan camps brought 20 wood and took 20 rice: −20 gold.");
}

// The city's holdings count toward an import limit, and a full caravan buys no more than its room.
fn test_a_caravan_brings_only_what_the_limit_wants() {
    let mut map = trade_map();
    let store = map.yard;
    put(&mut map, store, ResourceKind::Wood, 15);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(20));
    send(&mut map, "clan_camps").unwrap();
    tick(&mut map.env, STEP_SECS);
    assert_eq!(caravan(&map).unwrap().carries(ResourceKind::Wood), 5, "the yard holds 15 of the 20 wanted");
    until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 20);

    // Exports up to its capacity.
    let store = map.granary;
    put(&mut map, store, ResourceKind::Rice, 60);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Off);
    set_limit(&mut map, ResourceKind::Rice, TradeLimit::Export(0));
    send(&mut map, "clan_camps").unwrap();
    until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.granary, ResourceKind::Rice), 60 - rules().caravan_capacity);
}

fn test_the_yearly_amounts_cap_the_trade() {
    let mut map = trade_map();
    let per_year = trade::terms(map.env.sim.trade(), "clan_camps").unwrap().sells(ResourceKind::Metal).unwrap().per_year;
    map.env.sim.trade_mut().partner_mut("clan_camps").unwrap().sold[ResourceKind::Metal.index()] = per_year - 3;
    set_limit(&mut map, ResourceKind::Metal, TradeLimit::Import(30));
    send(&mut map, "clan_camps").unwrap();
    tick(&mut map.env, STEP_SECS);
    assert_eq!(caravan(&map).unwrap().carries(ResourceKind::Metal), 3, "3 left this year");
    until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.yard, ResourceKind::Metal), 3);

    // The amounts are spent: nothing to trade until the new year.
    assert_eq!(send(&mut map, "clan_camps"), Err(NotSent::NothingToTrade));
    map.env.sim.trade_mut().new_year();
    assert!(send(&mut map, "clan_camps").is_ok());
    until_it_leaves(&mut map);
}

fn test_the_treasury_pays_only_what_it_can() {
    let _gold = PaysForItself::new();
    let mut map = trade_map();
    map.env.sim.finance_mut().set_balance_unrecorded(30);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(20));
    send(&mut map, "clan_camps").unwrap();
    let notice = until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 4, "30 gold buys 4 wood at 7");
    assert_eq!(map.env.sim.finance().balance(), 2);
    let TradeNotice::Caravan { sold, paid, .. } = notice else { panic!("{notice:?}") };
    assert_eq!((sold, paid), (vec![(ResourceKind::Wood, 4)], 28));

    // In debt, it buys nothing.
    map.env.sim.finance_mut().set_balance_unrecorded(-10);
    send(&mut map, "clan_camps").unwrap();
    let TradeNotice::Caravan { sold, .. } = until_it_leaves(&mut map) else { panic!("a caravan's notice") };
    assert!(sold.is_empty());
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 4);
}

fn test_a_caravan_without_a_store_leaves() {
    let mut map = trade_map();
    test_utils::despawn_building(&mut map.env, map.yard);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(20));
    send(&mut map, "clan_camps").unwrap();
    let notice = until_it_leaves(&mut map);
    let TradeNotice::Caravan { sold, bought, .. } = &notice else { panic!("{notice:?}") };
    assert!(sold.is_empty() && bought.is_empty(), "the granary takes no wood");
    let (_, _, text) = trade::notice_message(&notice, "the Clan camps");
    assert_eq!(text, "A caravan from the Clan camps found nothing to trade and left.");
    assert!(map.env.sim.trade().partner("clan_camps").unwrap().sold.iter().all(|&count| count == 0));
}

fn test_no_caravan_without_something_to_trade() {
    let mut map = trade_map();
    assert_eq!(send(&mut map, "clan_camps"), Err(NotSent::NothingToTrade), "no limits set");
    set_limit(&mut map, ResourceKind::Rice, TradeLimit::Export(10));
    assert_eq!(send(&mut map, "clan_camps"), Err(NotSent::NothingToTrade), "no rice above 10");
    let store = map.granary;
    put(&mut map, store, ResourceKind::Rice, 11);
    assert!(send(&mut map, "clan_camps").is_ok());
    until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.granary, ResourceKind::Rice), 10);

    assert_eq!(send(&mut map, "de_son"), Err(NotSent::NotTrading), "its route is closed");
    assert_eq!(send(&mut map, "vijara"), Err(NotSent::NoDock), "ships come only to a dock");
    map.env.sim.trade_mut().set_enabled(false);
    assert_eq!(send(&mut map, "clan_camps"), Err(NotSent::NotTrading));
}

fn new_month(map: &mut TradeMap, year: u32, month: u32) {
    let context = map.env.new_sim_context(0.0);
    trade::on_new_month(&context, CalendarDate { day: 1, month, year });
}

fn test_the_monthly_schedule() {
    let mut map = trade_map();
    let every = trade::terms(map.env.sim.trade(), "clan_camps").unwrap().every_months;
    let first = map.env.sim.trade().partner("clan_camps").unwrap().next_arrival.unwrap();
    let next = |map: &TradeMap| map.env.sim.trade().partner("clan_camps").unwrap().next_arrival.unwrap();

    // Due, but nothing to trade: it tries again next month.
    new_month(&mut map, first / 12, first % 12 + 1);
    tick(&mut map.env, STEP_SECS);
    assert!(caravan(&map).is_none());
    assert_eq!(next(&map), first + 1);

    // Not due yet: nothing.
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(10));
    new_month(&mut map, first / 12, first % 12 + 1);
    tick(&mut map.env, STEP_SECS);
    assert!(caravan(&map).is_none());

    // Due with something to trade: it sets out, the next one `every` months later.
    let due = first + 1;
    new_month(&mut map, due / 12, due % 12 + 1);
    assert_eq!(next(&map), due + every);
    until_it_leaves(&mut map);
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 10);

    // A closed route and a partner by water send nothing.
    assert!(map.env.sim.trade().partner("de_son").unwrap().next_arrival.is_none());
    let vijara_due = map.env.sim.trade().partner("vijara").unwrap().next_arrival.unwrap();
    new_month(&mut map, vijara_due / 12, vijara_due % 12 + 1);
    tick(&mut map.env, STEP_SECS);
    assert!(caravan(&map).is_none());

    // A new year starts the partners' amounts again.
    assert!(map.env.sim.trade().partner("clan_camps").unwrap().sold[ResourceKind::Wood.index()] > 0);
    new_month(&mut map, 9, 1);
    assert_eq!(map.env.sim.trade().partner("clan_camps").unwrap().sold[ResourceKind::Wood.index()], 0);
}

fn test_opening_a_route_costs_gold() {
    let _gold = PaysForItself::new();
    let mut map = trade_map();
    let cost = trade::terms(map.env.sim.trade(), "de_son").unwrap().open_cost;
    let open = |map: &mut TradeMap, city: &str| {
        let context = map.env.new_sim_context(0.0);
        trade::open_route(&context, city)
    };

    map.env.sim.finance_mut().set_balance_unrecorded(i64::from(cost) - 1);
    assert_eq!(open(&mut map, "de_son"), Err(OpenRouteError::CantAfford(cost)));
    map.env.sim.finance_mut().set_balance_unrecorded(1000);
    assert_eq!(open(&mut map, "de_son"), Ok(cost));
    assert_eq!(map.env.sim.finance().balance(), 1000 - i64::from(cost));
    assert_eq!(map.env.sim.finance().this_year().total(LedgerCategory::Routes), i64::from(cost));
    let delta = map.env.sim.trade().partner("de_son").unwrap();
    assert!(delta.open && delta.next_arrival.is_some());

    assert_eq!(open(&mut map, "de_son"), Err(OpenRouteError::AlreadyOpen));
    assert_eq!(open(&mut map, "nowhere"), Err(OpenRouteError::NotAPartner));
    map.env.sim.trade_mut().close_route("vijara");
    assert_eq!(open(&mut map, "vijara"), Err(OpenRouteError::NeedsDock));
    map.env.sim.trade_mut().set_enabled(false);
    assert_eq!(open(&mut map, "clan_camps"), Err(OpenRouteError::TradeOff));
}

// Tampadesa's River Spirit's easy trade: partners pay more for exports and ask less for imports.
fn test_easy_trade_moves_the_prices() {
    let _gold = PaysForItself::new();
    let mut map = trade_map();
    map.env.sim.faith_mut().start(game::realm::Realm::Tampadesa, &GameConfigs::get().sim.faith);
    assert_eq!(map.env.sim.faith().trade_scale(), 1.0);
    map.env.sim.faith_mut().add_effect("river_spirit", "easy_trade", 6);
    let scale = map.env.sim.faith().trade_scale();
    assert!((scale - 1.15).abs() < 1e-5, "{scale}");

    let terms = trade::terms(map.env.sim.trade(), "clan_camps").unwrap();
    let clans = map.env.sim.trade().partner("clan_camps").unwrap();
    let wood = *terms.sells(ResourceKind::Wood).unwrap();
    let rice = *terms.buys(ResourceKind::Rice).unwrap();
    assert_eq!(trade::import_price(1.0, &terms, clans, &wood), 7);
    assert_eq!(trade::import_price(scale, &terms, clans, &wood), 6, "7 / 1.15");
    assert_eq!(trade::export_price(scale, &terms, clans, &rice), 7, "6 × 1.15");

    // A caravan trades at them.
    map.env.sim.finance_mut().set_balance_unrecorded(1000);
    let store = map.granary;
    put(&mut map, store, ResourceKind::Rice, 20);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(10));
    set_limit(&mut map, ResourceKind::Rice, TradeLimit::Export(10));
    send(&mut map, "clan_camps").unwrap();
    let TradeNotice::Caravan { paid, earned, .. } = until_it_leaves(&mut map) else { panic!("a caravan's notice") };
    assert_eq!((paid, earned), (60, 70));
}

// Kambara's Sky Deva's envoys: a caravan from an open land route unloads its goods free.
fn test_envoys_bring_gifts() {
    let _gold = PaysForItself::new();
    let mut map = trade_map();
    map.env.sim.finance_mut().set_balance_unrecorded(100);
    {
        let context = map.env.new_sim_context(0.0);
        assert_eq!(trade::envoys_partner(&context).as_deref(), Some("clan_camps"), "open, by land");
        assert!(trade::send_envoys(&context, 11));
    }
    let carried = caravan(&map).unwrap();
    assert!(carried.gift);
    assert_eq!((carried.carries(ResourceKind::Wood), carried.carries(ResourceKind::Metal)), (6, 5));
    let notice = until_it_leaves(&mut map);
    assert_eq!(notice, TradeNotice::Gifts {
        partner: "clan_camps".into(),
        goods: vec![(ResourceKind::Wood, 6), (ResourceKind::Metal, 5)],
    });
    assert_eq!(stock(&map, map.yard, ResourceKind::Wood), 6);
    assert_eq!(stock(&map, map.yard, ResourceKind::Metal), 5);
    assert_eq!(map.env.sim.finance().balance(), 100, "free");
    assert!(
        map.env.sim.trade().partner("clan_camps").unwrap().sold.iter().all(|&count| count == 0),
        "outside the year's amounts"
    );
    let (_, _, text) = trade::notice_message(&notice, "the Clan camps");
    assert_eq!(text, "Envoys from the Clan camps brought gifts: 6 wood, 5 metal.");

    // No envoys without an open route by land.
    map.env.sim.trade_mut().close_route("clan_camps");
    let context = map.env.new_sim_context(0.0);
    assert!(trade::envoys_partner(&context).is_none());
    assert!(!trade::send_envoys(&context, 11));
}

// The inspector's walker panel for a caravan: its partner, what it carries and what it bought.
fn test_the_caravans_panel() {
    let mut map = trade_map();
    let store = map.granary;
    put(&mut map, store, ResourceKind::Rice, 20);
    set_limit(&mut map, ResourceKind::Wood, TradeLimit::Import(8));
    set_limit(&mut map, ResourceKind::Rice, TradeLimit::Export(10));
    send(&mut map, "clan_camps").unwrap();
    tick(&mut map.env, STEP_SECS);
    let unit = find_unit_by_config(&map.env, UnitConfigKey::Trader).unwrap();
    let report = |map: &mut TradeMap| {
        let cell = test_utils::find_unit(&map.env, unit).cell();
        let context = map.env.new_sim_context(0.0);
        let target = game::inspect::InspectTarget::Unit { id: unit, cell };
        game::inspect::inspect_target(&context, &target, game::realm::Realm::DaiLong).unwrap()
    };
    let rows = |report: &game::inspect::InspectReport| {
        report
            .sections
            .iter()
            .flat_map(|section| match section {
                game::inspect::Section::Values { rows, .. } => {
                    rows.iter().map(|row| (row.label.clone(), row.value.clone())).collect()
                }
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
    };

    let setting_out = report(&mut map);
    assert_eq!(setting_out.subtitle, "Caravan · from Clan camps");
    assert_eq!(setting_out.status.as_ref().unwrap().word, "Trading");
    assert!(rows(&setting_out).contains(&("Carrying".to_string(), "8 wood".to_string())), "{:?}", rows(&setting_out));

    // Once it's bought the rice and is on its way out.
    tick_until(&mut map.env, 20_000, STEP_SECS, |env| {
        let unit = test_utils::find_unit(env, unit);
        unit.current_task_as::<UnitTaskTrade>(env.sim.task_manager())
            .is_some_and(|caravan| caravan.state == game::unit::task::UnitTaskTradeState::Leaving)
    });
    let leaving = report(&mut map);
    assert_eq!(leaving.status.as_ref().unwrap().word, "Leaving");
    let leaving_rows = rows(&leaving);
    assert!(leaving_rows.contains(&("Bought".to_string(), "10 rice".to_string())), "{leaving_rows:?}");
    assert!(leaving_rows.contains(&("Sold".to_string(), "8 wood".to_string())), "{leaving_rows:?}");
    until_it_leaves_now(&mut map);
}

// Ticks until the caravan already in the city has gone, taking its notice.
fn until_it_leaves_now(map: &mut TradeMap) {
    tick_until(&mut map.env, 20_000, STEP_SECS, |env| find_unit_by_config(env, UnitConfigKey::Trader).is_none());
    trade::take_notices();
}

// ----------------------------------------------
// Docks and ships (Phase 2)
// ----------------------------------------------

// The sea: open water from x 14 to 32 and y 30 to 34, its north shore at y 29.
const SEA_X: std::ops::RangeInclusive<i32> = 14..=32;
const SEA_Y: std::ops::RangeInclusive<i32> = 30..=34;
const SEA_ENTRY: Cell = Cell::new(31, 33);
const DOCK_CELL: Cell = Cell::new(24, 30);

// The trade map with the sea, roads from the stores to the shore, and Vijara's route open, its ships
// coming in at SEA_ENTRY. A dock at DOCK_CELL with `dock`.
struct PortMap {
    map: TradeMap,
    dock: Option<BuildingKindAndId>,
}

fn lay_water(env: &mut test_utils::TestEnvironment, xs: std::ops::RangeInclusive<i32>, ys: std::ops::RangeInclusive<i32>) {
    let water = test_utils::find_water_def();
    for y in ys {
        for x in xs.clone() {
            let cell = Cell::new(x, y);
            assert!(env.tile_map.is_cell_within_playable_area(cell), "{cell} is off the playable area");
            test_utils::place_terrain_by_def(env, cell, water);
        }
    }
}

fn port_map(dock: bool) -> PortMap {
    let mut map = trade_map();
    lay_water(&mut map.env, SEA_X, SEA_Y);
    // A road along the stores' fronts and down to the dock.
    let mut road: Vec<Cell> = (19..=31).map(|x| Cell::new(x, 25)).collect();
    road.extend((26..=29).map(|y| Cell::new(24, y)));
    test_utils::place_road(&mut map.env, &road);
    let mut vijara = partner("vijara", true);
    vijara.entry = Some([SEA_ENTRY.x, SEA_ENTRY.y]);
    let mut partners: Vec<MissionPartnerDef> =
        map.env.sim.trade().partners().iter().map(|partner| partner.def.clone()).collect();
    partners.retain(|def| def.city != "vijara");
    partners.push(vijara);
    trade::start(map.env.sim.trade_mut(), "dragon_legacy", &block(partners), 25, rules());
    let dock = dock.then(|| spawn_building(&mut map.env, DOCK_CELL, "dock"));
    tick(&mut map.env, STEP_SECS); // Road links.
    PortMap { map, dock }
}

fn set_vijara_limit(port: &mut PortMap, good: ResourceKind, limit: TradeLimit) {
    assert!(port.map.env.sim.trade_mut().set_limit("vijara", good, limit));
}

fn ship(port: &PortMap) -> Option<UnitTaskShip> {
    let unit = find_unit_by_config(&port.map.env, UnitConfigKey::TradeShip)?;
    let unit = test_utils::find_unit(&port.map.env, unit);
    unit.current_task_as::<UnitTaskShip>(port.map.env.sim.task_manager()).cloned()
}

fn ships_in(env: &test_utils::TestEnvironment) -> Vec<UnitTaskShip> {
    let mut ships = Vec::new();
    env.world.for_each_unit(|unit| {
        if let Some(task) = unit.current_task_as::<UnitTaskShip>(env.sim.task_manager()) {
            ships.push(task.clone());
        }
        true
    });
    ships
}

// Ticks until every ship has come and gone; returns the notices they left.
fn until_the_ships_leave(port: &mut PortMap) -> Vec<TradeNotice> {
    tick_until(&mut port.map.env, 40_000, STEP_SECS, |env| find_unit_by_config(env, UnitConfigKey::TradeShip).is_none());
    trade::take_notices()
}

fn test_a_dock_stands_at_the_shore() {
    let mut port = port_map(true);
    let dock = find_building(&port.map.env, port.dock.unwrap());
    assert!(dock.is(game::building::BuildingKind::Dock));
    assert!(game::tile::water::is_port_or_wharf(test_utils::find_building_def("dock")));
    assert!(game::tile::water::is_port_or_wharf(test_utils::find_building_def("fishing_wharf")));
    assert!(!game::tile::water::is_port_or_wharf(test_utils::find_building_def("storage_yard")));
    let berth = ships::berth_of(dock).expect("a dock keeps a berth");
    assert!(berth.ship.is_none());
    assert_eq!(berth.dockers.len(), rules().dockers as usize);
    assert!(dock.road_link().is_some(), "linked to the road down to the shore");
    let context = port.map.env.new_sim_context(0.0);
    assert!(ships::has_dock(&context));
    assert_eq!(ships::ship_entry(&context, "vijara"), Some(SEA_ENTRY));
}

fn test_a_ship_moors_and_its_dockers_trade() {
    let _gold = PaysForItself::new();
    let mut port = port_map(true);
    let (yard, granary) = (port.map.yard, port.map.granary);
    put(&mut port.map, granary, ResourceKind::Rice, 26);
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(16));
    set_vijara_limit(&mut port, ResourceKind::Rice, TradeLimit::Export(10));
    port.map.env.sim.finance_mut().set_balance_unrecorded(1000);
    let balance = port.map.env.sim.finance().balance();

    send(&mut port.map, "vijara").unwrap();
    assert_eq!(ship(&port).unwrap().carries(ResourceKind::Metal), 16, "it brings what the limit wants");

    // It sails to the dock and moors; the dockers come and go.
    tick_until(&mut port.map.env, 20_000, STEP_SECS, |env| {
        find_unit_by_config(env, UnitConfigKey::TradeShip).is_some_and(|unit| {
            test_utils::find_unit(env, unit)
                .current_task_as::<UnitTaskShip>(env.sim.task_manager())
                .is_some_and(|task| task.state == UnitTaskShipState::Moored)
        })
    });
    let moored = ship(&port).unwrap();
    assert!(moored.dock == port.dock, "moored at the dock");
    assert!(ships::berth_of(find_building(&port.map.env, port.dock.unwrap())).unwrap().ship.is_some());

    let notices = until_the_ships_leave(&mut port);
    assert_eq!(stock(&port.map, yard, ResourceKind::Metal), 16, "unloaded to the yard");
    assert_eq!(stock(&port.map, granary, ResourceKind::Rice), 10, "loaded from the granary down to the limit");
    let vijara = port.map.env.sim.trade().partner("vijara").unwrap();
    assert_eq!(vijara.sold[ResourceKind::Metal.index()], 16);
    assert_eq!(vijara.bought[ResourceKind::Rice.index()], 16);
    // Paid load by load, at Vijara's prices then (friendly, relations 4: 3% better for the city), each
    // load moving the next one's: metal 24 × 0.97 = 23, then × 1.016 = 24; rice 18 × 1.03 = 19,
    // then × 0.984 = 18.
    let finance = port.map.env.sim.finance();
    assert_eq!(finance.this_year().total(LedgerCategory::Imports), 8 * 23 + 8 * 24);
    assert_eq!(finance.this_year().total(LedgerCategory::Exports), 8 * 19 + 8 * 18);
    assert_eq!(finance.balance(), balance - 376 + 296);
    assert_eq!(notices, [TradeNotice::Ship {
        partner: "vijara".into(),
        sold: vec![(ResourceKind::Metal, 16)],
        bought: vec![(ResourceKind::Rice, 16)],
        paid: 376,
        earned: 296,
        turned_back: false,
    }]);
    let (_, _, text) = trade::notice_message(&notices[0], "Vijara");
    assert_eq!(text, "A ship from Vijara brought 16 metal and took 16 rice: −80 gold.");

    // The berth is free again, and no docker is out.
    let berth = ships::berth_of(find_building(&port.map.env, port.dock.unwrap())).unwrap();
    assert!(berth.ship.is_none());
    assert_eq!(berth.dockers_out(), 0);
    assert!(find_unit_by_config(&port.map.env, UnitConfigKey::Runner).is_none(), "the dockers are home");
}

fn test_river_and_sea_routes_need_a_dock() {
    let _gold = PaysForItself::new();
    let mut port = port_map(false);
    port.map.env.sim.trade_mut().close_route("vijara");
    port.map.env.sim.finance_mut().set_balance_unrecorded(1000);
    let open = |port: &mut PortMap| {
        let context = port.map.env.new_sim_context(0.0);
        trade::open_route(&context, "vijara")
    };
    assert_eq!(open(&mut port), Err(OpenRouteError::NeedsDock));
    {
        let trade = port.map.env.sim.trade();
        assert_eq!(trade::route_detail(trade, "vijara", false).as_deref(), Some("Closed · needs a dock"));
        assert_eq!(trade::route_detail(trade, "vijara", true).as_deref(), Some("Closed · opens for free"));
    }

    // With a dock, it opens, and its ships come.
    port.dock = Some(spawn_building(&mut port.map.env, DOCK_CELL, "dock"));
    tick(&mut port.map.env, STEP_SECS);
    assert_eq!(open(&mut port), Ok(0));
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(8));
    assert_eq!(send(&mut port.map, "vijara"), Ok(()));
    until_the_ships_leave(&mut port);
    assert_eq!(stock(&port.map, port.map.yard, ResourceKind::Metal), 8);

    // Without one, no ship sets out.
    test_utils::despawn_building(&mut port.map.env, port.dock.unwrap());
    assert_eq!(send(&mut port.map, "vijara"), Err(NotSent::NoDock));
}

fn test_a_ship_without_a_dock_in_reach_turns_back() {
    let mut port = port_map(false);
    // A pond up the land, cut off from the sea, with the dock on it: no ship sets out.
    lay_water(&mut port.map.env, 30..=33, 18..=20);
    let pond_dock = spawn_building(&mut port.map.env, Cell::new(31, 19), "dock");
    tick(&mut port.map.env, STEP_SECS);
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(8));
    assert_eq!(send(&mut port.map, "vijara"), Err(NotSent::NoDock), "no dock its water reaches");

    // One sets out for the dock on the sea, which is gone before it arrives: it turns back.
    port.dock = Some(spawn_building(&mut port.map.env, DOCK_CELL, "dock"));
    tick(&mut port.map.env, STEP_SECS);
    assert_eq!(send(&mut port.map, "vijara"), Ok(()));
    tick(&mut port.map.env, STEP_SECS);
    test_utils::despawn_building(&mut port.map.env, port.dock.unwrap());
    let notices = until_the_ships_leave(&mut port);
    assert_eq!(notices.len(), 1);
    let TradeNotice::Ship { turned_back, sold, .. } = &notices[0] else { panic!("{notices:?}") };
    assert!(*turned_back && sold.is_empty());
    let (_, _, text) = trade::notice_message(&notices[0], "Vijara");
    assert_eq!(text, "A ship from Vijara found no dock to moor at and turned back.");
    assert_eq!(stock(&port.map, port.map.yard, ResourceKind::Metal), 0);
    assert!(ships::berth_of(find_building(&port.map.env, pond_dock)).unwrap().ship.is_none());
}

fn test_ships_keep_out_of_canals() {
    let mut port = port_map(false);
    // The pond again, joined to the sea only by a canal.
    lay_water(&mut port.map.env, 30..=33, 18..=20);
    let canal = game::tile::sets::TileSets::get()
        .find_tile_def_by_name(
            game::tile::TileMapLayerKind::Terrain,
            game::tile::sets::TERRAIN_WATER_CATEGORY.string,
            "canal",
        )
        .expect("the canal");
    for y in 21..=29 {
        test_utils::place_terrain_by_def(&mut port.map.env, Cell::new(33, y), canal);
    }
    port.dock = Some(spawn_building(&mut port.map.env, Cell::new(31, 19), "dock"));
    tick(&mut port.map.env, STEP_SECS);
    {
        let context = port.map.env.new_sim_context(0.0);
        assert!(
            ships::find_water_path(&context, SEA_ENTRY, |cell| cell == Cell::new(32, 20)).is_none(),
            "not through a canal"
        );
        assert!(ships::find_water_path(&context, SEA_ENTRY, |cell| cell == Cell::new(15, 31)).is_some(), "across the sea");
        assert!(!ships::reaches_working_dock(&context, SEA_ENTRY), "the dock is on the pond");
    }
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(8));
    assert_eq!(send(&mut port.map, "vijara"), Err(NotSent::NoDock));
}

fn test_a_busy_dock_keeps_a_ship_waiting() {
    let _gold = PaysForItself::new();
    let mut port = port_map(true);
    port.map.env.sim.finance_mut().set_balance_unrecorded(10_000);
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(40));
    send(&mut port.map, "vijara").unwrap();
    send(&mut port.map, "vijara").unwrap();
    tick_until(&mut port.map.env, 20_000, STEP_SECS, |env| {
        ships_in(env).iter().any(|ship| ship.state == UnitTaskShipState::Moored)
    });
    // One moors; the other waits off the dock until the berth is free.
    tick(&mut port.map.env, 2.0);
    let states: Vec<UnitTaskShipState> = ships_in(&port.map.env).iter().map(|ship| ship.state).collect();
    assert!(states.contains(&UnitTaskShipState::Moored), "{states:?}");
    assert!(
        states.iter().any(|state| matches!(state, UnitTaskShipState::WaitingOff | UnitTaskShipState::FindingBerth)),
        "{states:?}"
    );
    let notices = until_the_ships_leave(&mut port);
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert!(notices.iter().all(|notice| matches!(notice, TradeNotice::Ship { turned_back: false, .. })), "{notices:?}");
    assert_eq!(stock(&port.map, port.map.yard, ResourceKind::Metal), 40, "both unloaded, up to the limit");
}

fn test_the_monthly_schedule_sends_ships() {
    let mut port = port_map(true);
    set_vijara_limit(&mut port, ResourceKind::Metal, TradeLimit::Import(8));
    let due = port.map.env.sim.trade().partner("vijara").unwrap().next_arrival.unwrap();
    new_month(&mut port.map, due / 12, due % 12 + 1);
    assert!(ship(&port).is_some(), "a ship sets out");
    let every = trade::terms(port.map.env.sim.trade(), "vijara").unwrap().every_months;
    assert_eq!(port.map.env.sim.trade().partner("vijara").unwrap().next_arrival, Some(due + every));
    until_the_ships_leave(&mut port);

    // Without a working dock it tries again next month.
    test_utils::despawn_building(&mut port.map.env, port.dock.unwrap());
    let due = due + every;
    new_month(&mut port.map, due / 12, due % 12 + 1);
    assert!(ship(&port).is_none());
    assert_eq!(port.map.env.sim.trade().partner("vijara").unwrap().next_arrival, Some(due + 1));
}

// ----------------------------------------------
// Prices (Phase 3)
// ----------------------------------------------

fn offset(state: &CityTrade, city: &str, good: ResourceKind) -> f32 {
    state.partner(city).unwrap().price_offset(good)
}

fn assert_near(value: f32, expected: f32) {
    assert!((value - expected).abs() < 1e-5, "{value}, expected {expected}");
}

// What a partner sells the city grows dearer and what it buys cheaper, 2% for every 10 traded, at
// most 40% either way; each month they drift a quarter of the way back.
fn test_trade_moves_prices_and_they_drift_back() {
    let mut state = started();
    let terms = terms_of("clan_camps");
    let (wood, rice, metal) = (ResourceKind::Wood, ResourceKind::Rice, ResourceKind::Metal);
    let clans = state.partner_mut("clan_camps").unwrap();
    clans.record_sold(wood, 50, rules());
    clans.record_bought(rice, 30, rules());
    assert_eq!((clans.sold[wood.index()], clans.bought[rice.index()]), (50, 30), "the year's amounts");
    assert_near(clans.price_offset(wood), 0.10);
    assert_near(clans.price_offset(rice), -0.06);

    clans.record_sold(wood, 1000, rules());
    clans.record_bought(rice, 1000, rules());
    assert_near(clans.price_offset(wood), 0.4);
    assert_near(clans.price_offset(rice), -0.4);

    // The clans (neutral, relations 3) at their base prices: wood 7 × 1.4 = 10, rice 6 × 0.6 = 4;
    // metal, untraded, 18.
    let clans = state.partner("clan_camps").unwrap();
    let price = |side, good: ResourceKind| {
        let terms_good = match side {
            TradeSide::Import => terms.sells(good),
            TradeSide::Export => terms.buys(good),
        };
        let terms_good = terms_good.unwrap();
        (trade::price_now(side, 1.0, &terms, clans, terms_good), trade::price_trend(side, 1.0, &terms, clans, terms_good))
    };
    assert_eq!(price(TradeSide::Import, wood), (10, PriceTrend::Up));
    assert_eq!(price(TradeSide::Export, rice), (4, PriceTrend::Down));
    assert_eq!(price(TradeSide::Import, metal), (18, PriceTrend::Steady));

    // A month: a quarter of the way back.
    state.drift_prices(rules());
    assert_near(offset(&state, "clan_camps", wood), 0.3);
    assert_near(offset(&state, "clan_camps", rice), -0.3);
    state.drift_prices(rules());
    assert_near(offset(&state, "clan_camps", wood), 0.225);

    // Back to the base once too small to move a price.
    for _ in 0..40 {
        state.drift_prices(rules());
    }
    assert_eq!(offset(&state, "clan_camps", wood), 0.0);
    assert_eq!(offset(&state, "clan_camps", rice), 0.0);

    // In a game, the month's start drifts them.
    let mut map = trade_map();
    map.env.sim.trade_mut().partner_mut("clan_camps").unwrap().record_sold(wood, 40, rules());
    assert_near(offset(map.env.sim.trade(), "clan_camps", wood), 0.08);
    new_month(&mut map, 3, 2);
    assert_near(offset(map.env.sim.trade(), "clan_camps", wood), 0.06);
}

// Relations move prices 3% a level from the middle (3): a friend asks less for its goods and pays
// more for the city's.
fn test_relations_scale_the_prices() {
    let state = started();
    let clans = state.partner("clan_camps").unwrap();
    let mut terms = terms_of("clan_camps");
    assert_eq!(terms.relation, 3);
    let hundred = GoodTerms { good: ResourceKind::Wood, price: 100, per_year: 10 };
    for (relation, import, export) in [(0, 109, 91), (2, 103, 97), (3, 100, 100), (4, 97, 103), (5, 94, 106)] {
        terms.relation = relation;
        assert_eq!(trade::import_price(1.0, &terms, clans, &hundred), import, "relations {relation}");
        assert_eq!(trade::export_price(1.0, &terms, clans, &hundred), export, "relations {relation}");
        assert_eq!(trade::price_trend(TradeSide::Import, 1.0, &terms, clans, &hundred), PriceTrend::Steady);
    }

    // Campaign I's: the delta is allied (5), Vijara friendly (4).
    let delta = terms_of("de_son");
    let bricks = *delta.sells(ResourceKind::Bricks).unwrap();
    assert_eq!((delta.relation, trade::import_price(1.0, &delta, clans, &bricks)), (5, 9), "10 × 0.94");
    let vijara = terms_of("vijara");
    let rice = *vijara.buys(ResourceKind::Rice).unwrap();
    assert_eq!((vijara.relation, trade::export_price(1.0, &vijara, clans, &rice)), (4, 19), "18 × 1.03");
}

// The world map's side panel shows a partner's goods at their prices now, with the way trade moved
// them; the goods the game doesn't trade show no price.
fn test_the_world_maps_panel_shows_the_prices_now() {
    let mut map = trade_map();
    let rows = |offers: &[game::world_map::provider::TradeOffer]| -> Vec<(String, Option<u32>, PriceTrend)> {
        offers.iter().map(|offer| (offer.good.clone(), offer.price, offer.trend)).collect()
    };
    let row = |good: &str, price: Option<u32>, trend| (good.to_string(), price, trend);

    let (sells, buys) = trade::panel_offers(map.env.sim.trade(), "vijara", 1.0).unwrap();
    assert_eq!(rows(&sells), [
        row("Salt", None, PriceTrend::Steady),
        row("Aloeswood", None, PriceTrend::Steady),
        row("Metal", Some(23), PriceTrend::Steady),
    ]);
    assert_eq!(rows(&buys), [
        row("Silk", None, PriceTrend::Steady),
        row("Pottery", Some(46), PriceTrend::Steady),
        row("Rice", Some(19), PriceTrend::Steady),
    ]);

    // Vijara sold the city 40 metal and bought 50 rice: metal 24 × 1.08 × 0.97 = 25, rice
    // 18 × 0.9 × 1.03 = 17.
    let vijara = map.env.sim.trade_mut().partner_mut("vijara").unwrap();
    vijara.record_sold(ResourceKind::Metal, 40, rules());
    vijara.record_bought(ResourceKind::Rice, 50, rules());
    let (sells, buys) = trade::panel_offers(map.env.sim.trade(), "vijara", 1.0).unwrap();
    assert_eq!(rows(&sells)[2], row("Metal", Some(25), PriceTrend::Up));
    assert_eq!(rows(&buys)[2], row("Rice", Some(17), PriceTrend::Down));

    // Easy trade improves both for the city, the trend still trade's alone.
    let (sells, buys) = trade::panel_offers(map.env.sim.trade(), "vijara", 1.15).unwrap();
    assert_eq!(rows(&sells)[2], row("Metal", Some(22), PriceTrend::Up), "25.14 / 1.15");
    assert_eq!(rows(&buys)[2], row("Rice", Some(19), PriceTrend::Down), "16.69 × 1.15");

    // A city the game doesn't trade with has none: the panel keeps the world file's.
    assert!(trade::panel_offers(map.env.sim.trade(), "cho_deo", 1.0).is_none());
}
