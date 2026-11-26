use common::{coords::Cell, time::Seconds};
use game::{
    building::{
        BuildingKind,
        BuildingKindAndId,
        construction::{self, ConstructionMode},
        risks::{self, MAX_RISK, RiskKind, RiskNoticeKind},
    },
    cheats,
    config::{FinanceConfigs, GameConfigs},
    floods::{self, FloodNotice},
    menu::{PlaceOrClearResult, TilePlacement},
    messages::{self, Message, MessageKind, MessagePriority, MessageTarget},
    ratings::failure::{self, FailureNotice},
    realm::Realm,
    sim::finance::DebtNoticeKind,
    trade::{self, TradeNotice},
};
use serde_json::json;
use strum::IntoEnumIterator;

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the game's messages
// (docs/plans/messages.md, M11 Phase 0).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Messages", &[
        test_utils::test_fn!(test_the_log_keeps_the_latest_messages),
        test_utils::test_fn!(test_each_kind_has_a_default_priority),
        test_utils::test_fn!(test_a_message_reports_its_place),
        test_utils::test_fn!(test_a_completed_building_is_its_place),
        test_utils::test_fn!(test_a_fire_is_at_its_building_and_its_ruins_at_their_cell),
        test_utils::test_fn!(test_debt_notices_say_what_they_tell),
        test_utils::test_fn!(test_the_rivers_notices_have_their_places),
        test_utils::test_fn!(test_trade_notices_go_to_the_partner_or_the_goods_page),
        test_utils::test_fn!(test_failure_warnings_go_to_their_pages),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

const TICK_SECS: Seconds = 0.5;

fn road(env: &mut TestEnvironment) {
    let cells: Vec<Cell> = (1..31).map(|x| Cell::new(x, 10)).collect();
    test_utils::place_road(env, &cells);
}

fn target_of(message: &Message) -> Option<MessageTarget> {
    message.target.clone()
}

// ----------------------------------------------
// The log
// ----------------------------------------------

// The log keeps the latest CAPACITY messages, newest first, each with its id, its month and its
// priority; closing it hides it until a newer one comes; a session change empties it.
fn test_the_log_keeps_the_latest_messages() {
    messages::clear();
    let first = messages::post(Message::new(MessageKind::Construction, "one"), 5);
    let second =
        messages::post(Message::new(MessageKind::Warning, "two").with_target(MessageTarget::Cell(Cell::new(3, 4))), 6);
    assert!(second > first, "ids count up");
    let latest = messages::latest(10);
    assert_eq!(latest.iter().map(|message| message.text.as_str()).collect::<Vec<_>>(), ["two", "one"]);
    assert_eq!((latest[0].id, latest[0].month, latest[0].priority), (second, 6, MessagePriority::Warning));
    assert_eq!(messages::newest_id(), second);
    assert_eq!(messages::find(first).map(|message| message.text), Some("one".to_string()));
    assert!(target_of(&latest[0]) == Some(MessageTarget::Cell(Cell::new(3, 4))));

    messages::dismiss();
    assert!(messages::is_dismissed());
    messages::post(Message::new(MessageKind::Trade, "three"), 6);
    assert!(!messages::is_dismissed(), "a newer message shows the log again");

    for k in 0..messages::CAPACITY + 4 {
        messages::post(Message::new(MessageKind::Harvest, format!("harvest {k}")), 7);
    }
    assert_eq!(messages::latest(usize::MAX).len(), messages::CAPACITY);
    assert!(messages::find(first).is_none(), "the oldest went");

    messages::clear();
    assert_eq!(messages::newest_id(), 0);
    assert!(messages::latest(10).is_empty());
}

// A message's priority is its kind's unless it says otherwise: a building done or a caravan is
// news, a harvest or the river a notice, a warning a warning.
fn test_each_kind_has_a_default_priority() {
    for kind in MessageKind::iter() {
        let expected = match kind {
            MessageKind::Construction | MessageKind::Trade => MessagePriority::Info,
            MessageKind::Warning => MessagePriority::Warning,
            _ => MessagePriority::Notice,
        };
        assert_eq!(Message::new(kind, "").priority, expected, "{kind:?}");
    }
    let message = Message::new(MessageKind::Request, "").with_priority(MessagePriority::Event);
    assert_eq!(message.priority, MessagePriority::Event);
}

// What messages.state reports of a message and its place.
fn test_a_message_reports_its_place() {
    let mut env = TestEnvironment::new();
    road(&mut env);
    let yard = test_utils::spawn_building(&mut env, Cell::new(20, 11), "storage_yard");
    let message = Message::new(MessageKind::Warning, "Fire!")
        .with_priority(MessagePriority::Warning)
        .with_target(MessageTarget::Building(yard, Cell::new(20, 11)));
    assert_eq!(
        message.to_json(),
        json!({
            "id": 0,
            "kind": "warning",
            "priority": "warning",
            "text": "Fire!",
            "target": { "kind": "building", "building": "StorageYard", "cell": [20, 11] },
            "month": 0,
        })
    );
    let page = MessageTarget::CityPage("goods").to_json();
    assert_eq!(page, json!({ "kind": "page", "page": "goods" }));
    let map = MessageTarget::WorldMap(Some("vijara".into())).to_json();
    assert_eq!(map, json!({ "kind": "world_map", "city": "vijara" }));
    assert_eq!(MessageTarget::Cell(Cell::new(1, 2)).to_json(), json!({ "kind": "cell", "cell": [1, 2] }));
}

// ----------------------------------------------
// The notices' places
// ----------------------------------------------

// A building the player had built tells where it stands once it's done.
fn test_a_completed_building_is_its_place() {
    cheats::get_mut().instant_construction = false;
    let mut env = TestEnvironment::new();
    let _ = construction::take_completed();
    let cell = Cell::new(6, 6);
    {
        let tile_def = test_utils::find_building_def("small_well");
        let context = env.new_sim_context(0.0);
        let placed = TilePlacement::place(&context, cell, tile_def, true, true, ConstructionMode::Site);
        assert!(!matches!(placed, PlaceOrClearResult::Failed { .. }), "the well's site");
    }
    let is_done = |env: &TestEnvironment| {
        env.world.find_building_for_cell(cell, &env.tile_map).is_some_and(|well| !well.is_under_construction())
    };
    test_utils::tick_until(&mut env, 2_000, TICK_SECS, is_done);
    cheats::get_mut().instant_construction = true;

    let well = env.world.find_building_for_cell(cell, &env.tile_map).expect("the well").kind_and_id();
    let completed = construction::take_completed();
    assert_eq!(completed.len(), 1);
    assert!(completed[0].building == well);
    assert_eq!(completed[0].cell, cell);
}

// A fire's notices are at the burning building; a building burnt down is at the cell it stood on.
fn test_a_fire_is_at_its_building_and_its_ruins_at_their_cell() {
    cheats::get_mut().no_city_risks = false;
    let mut env = TestEnvironment::new();
    let _ = risks::take_notices();
    road(&mut env);
    let cell = Cell::new(20, 11);
    let yard: BuildingKindAndId = test_utils::spawn_building(&mut env, cell, "storage_yard");
    let base_cell = test_utils::find_building(&env, yard).base_cell();
    test_utils::find_building_mut(&mut env, yard).risks_mut().set(RiskKind::Fire, MAX_RISK);
    test_utils::tick(&mut env, TICK_SECS);

    let notices = risks::take_notices();
    assert_eq!(notices.iter().map(|notice| notice.kind).collect::<Vec<_>>(), [RiskNoticeKind::Fire]);
    assert!(notices[0].target == MessageTarget::Building(yard, base_cell));

    let burn_secs = GameConfigs::get().sim.risks.burn_secs;
    test_utils::tick_n(&mut env, (burn_secs / TICK_SECS).ceil() as usize + 2, TICK_SECS);
    cheats::get_mut().no_city_risks = true;
    assert!(env.world.find_building(yard.kind, yard.id).is_none(), "burnt down");
    let notices = risks::take_notices();
    let ruin = notices.iter().find(|notice| notice.kind == RiskNoticeKind::BurntDown).expect("its notice");
    assert!(ruin.target == MessageTarget::Cell(base_cell), "where it stood");
    assert!(yard.kind == BuildingKind::StorageYard);
}

// Debt's notices say what they tell, so the message's kind and sound don't hang on their text.
fn test_debt_notices_say_what_they_tell() {
    let mut env = TestEnvironment::new();
    let configs = FinanceConfigs { debt_limit_months: 4, ..FinanceConfigs::default() };
    let finance = env.sim.finance_mut();
    finance.set_wage(12);
    finance.set_balance_unrecorded(5);
    let _ = finance.take_notices();
    for _ in 0..4 {
        finance.on_month_end(10, 0, &configs);
    }
    finance.add_unrecorded(1_000);
    finance.on_month_end(10, 0, &configs);

    let notices = finance.take_notices();
    let kinds: Vec<DebtNoticeKind> = notices.iter().map(|notice| notice.kind).collect();
    assert_eq!(kinds, [
        DebtNoticeKind::InDebt,
        DebtNoticeKind::Warning { months_left: 2 },
        DebtNoticeKind::Warning { months_left: 1 },
        DebtNoticeKind::Overdue,
        DebtNoticeKind::OutOfDebt,
    ]);
    assert_eq!(notices.iter().filter(|notice| notice.is_good_news()).count(), 1);
    assert_eq!(notices[4].text, "The treasury is out of debt.");
}

// The river rising is at the river, the flood at a building it damaged (or a cell under water), a
// breach at the dyke and the silt at a field; the water going down has no place.
fn test_the_rivers_notices_have_their_places() {
    let sources = [Cell::new(2, 3), Cell::new(9, 9)];
    let place = |notice: FloodNotice| floods::notice_message(&notice, "the river", &sources).0;

    let rising = place(FloodNotice::RiverRising);
    assert_eq!(rising.kind, MessageKind::Season);
    assert!(rising.target == Some(MessageTarget::Cell(Cell::new(2, 3))), "the river's first source");

    let at = Some(Cell::new(7, 8));
    let flood = place(FloodNotice::Flood { cells: 10, buildings_damaged: 1, fields_flooded: 0, at });
    assert_eq!((flood.kind, flood.priority), (MessageKind::Warning, MessagePriority::Warning));
    assert!(flood.target == Some(MessageTarget::Cell(Cell::new(7, 8))));
    assert!(place(FloodNotice::Breached { breaches: 1, at }).target == Some(MessageTarget::Cell(Cell::new(7, 8))));
    assert!(place(FloodNotice::Sluiced { fields: 2, at }).target == Some(MessageTarget::Cell(Cell::new(7, 8))));
    assert!(place(FloodNotice::Receded { fields: 3 }).target.is_none());
    assert!(floods::notice_message(&FloodNotice::RiverRising, "the river", &[]).0.target.is_none(), "no river");
}

// A caravan's, a ship's and envoys' messages go to their partner on the world map.
fn test_trade_notices_go_to_the_partner_or_the_goods_page() {
    let caravan = TradeNotice::Caravan { partner: "clan_camps".into(), sold: vec![], bought: vec![], paid: 0, earned: 0 };
    let (message, _) = trade::notice_message(&caravan, "the Clan camps");
    assert_eq!((message.kind, message.priority), (MessageKind::Trade, MessagePriority::Info));
    assert!(message.target == Some(MessageTarget::WorldMap(Some("clan_camps".into()))));

    let gifts = TradeNotice::Gifts { partner: "vijara".into(), goods: vec![] };
    assert!(trade::notice_message(&gifts, "Vijara").0.target == Some(MessageTarget::WorldMap(Some("vijara".into()))));
}

// Legitimacy's warnings go to the Ratings page, the time limit's to the Overview, with the goals.
fn test_failure_warnings_go_to_their_pages() {
    let legitimacy = FailureNotice::LegitimacyLow { months_left: 3, started: false };
    let (message, _) = failure::notice_message(&legitimacy, Realm::DaiLong);
    assert_eq!((message.kind, message.priority), (MessageKind::Warning, MessagePriority::Warning));
    assert!(message.target == Some(MessageTarget::CityPage("ratings")));
    let time = FailureNotice::TimeLeft { months_left: 6 };
    assert!(failure::notice_message(&time, Realm::DaiLong).0.target == Some(MessageTarget::CityPage("overview")));
}
