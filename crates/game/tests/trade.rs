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
    trade::{self, NotSent, OpenRouteError, PartnerTerms, TradeNotice},
    unit::{config::UnitConfigKey, task::UnitTaskTrade},
    world_map::{
        WorldMapConfigs,
        config::{RouteKind, TradeGoodDef, WorldMapDef, trade_goods_problems},
    },
};

mod test_utils;
use test_utils::{TestEnvironment, find_building, find_building_mut, find_unit_by_config, spawn_building, tick, tick_until};

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
    trade::send_caravan(&context, city)
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
    assert_eq!(send(&mut map, "vijara"), Err(NotSent::ByWater), "ships come with docks");
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
    assert_eq!(trade::import_price(1.0, clans, &wood), 7);
    assert_eq!(trade::import_price(scale, clans, &wood), 6, "7 / 1.15");
    assert_eq!(trade::export_price(scale, clans, &rice), 7, "6 × 1.15");

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
