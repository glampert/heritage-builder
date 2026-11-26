use common::{coords::Cell, time::Seconds};
use game::{
    building::{
        BuildingKind,
        BuildingKindAndId,
        construction::{self, ConstructionMode},
        risks::{self, MAX_RISK, RiskKind, RiskNotice, RiskNoticeKind},
    },
    cheats,
    config::{FinanceConfigs, GameConfigs},
    floods::{self, FloodNotice},
    menu::{PlaceOrClearResult, TilePlacement},
    messages::{self, EventLine, EventPopup, Message, MessageKind, MessagePriority, MessageTarget, popups},
    ratings::failure::{self, FailureNotice},
    realm::Realm,
    sim::{
        faith::{FaithNotice, FaithNoticeKind},
        finance::{DebtNotice, DebtNoticeKind},
        resources::ResourceKind,
        trade::TradeRequest,
    },
    trade::{self, TradeNotice, requests::RequestEvent},
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
        // The event popups (Phase 1):
        test_utils::test_fn!(test_an_events_popup_waits_in_the_queue),
        test_utils::test_fn!(test_the_floods_popup),
        test_utils::test_fn!(test_a_requests_popups),
        test_utils::test_fn!(test_a_blessings_and_a_misfortunes_popups),
        test_utils::test_fn!(test_an_outbreaks_popup),
        test_utils::test_fn!(test_the_last_warnings_popups),
        test_utils::test_fn!(test_the_debts_popups),
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
            "popup": null,
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
    let flood = place(FloodNotice::Flood { cells: 10, buildings_damaged: 1, fields_flooded: 0, breaches: 0, at });
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

// ----------------------------------------------
// The event popups
// ----------------------------------------------

// Every text a popup shows, to check none is a missing key ("event.…").
fn assert_worded(popup: &EventPopup) {
    let texts = [&popup.kicker, &popup.title, &popup.text].into_iter().chain(popup.lines.iter().map(|line| &line.text));
    for text in texts {
        assert!(!text.is_empty() && !text.contains("event.") && !text.contains('{'), "{text:?} in {popup:?}");
    }
    for icon in std::iter::once(&popup.icon).chain(popup.lines.iter().map(|line| &line.icon)) {
        assert!(popups::LINE_ICONS.contains(&icon.as_str()), "{icon} in {popup:?}");
    }
}

fn line_texts(popup: &EventPopup) -> Vec<&str> {
    popup.lines.iter().map(|line| line.text.as_str()).collect()
}

fn test_popup() -> EventPopup {
    EventPopup {
        kicker: "Flood".into(),
        title: "The river floods".into(),
        text: String::new(),
        icon: "water".into(),
        lines: (0..5).map(|k| EventLine { icon: "warn".into(), text: format!("line {k}") }).collect(),
    }
}

// A message with a popup is an Event, its lines at most MAX_EVENT_LINES; queued, it waits until
// taken, oldest first; a session change (clear) drops the queue.
fn test_an_events_popup_waits_in_the_queue() {
    messages::clear();
    let plain = Message::new(MessageKind::Warning, "plain").with_popup(None);
    assert_eq!(plain.priority, MessagePriority::Warning, "no popup, no change");
    let event = Message::new(MessageKind::Warning, "flood").with_popup(Some(test_popup()));
    assert_eq!(event.priority, MessagePriority::Event);
    assert_eq!(event.popup.as_ref().unwrap().lines.len(), messages::MAX_EVENT_LINES);
    assert_eq!(event.to_json()["popup"]["kicker"], json!("Flood"));

    let plain = messages::post(plain, 3);
    let first = messages::post(event.clone(), 3);
    let second = messages::post(event, 4);
    messages::queue_popup(plain);
    assert!(messages::queued_popups().is_empty(), "a message without a popup isn't queued");
    messages::queue_popup(first);
    messages::queue_popup(second);
    assert_eq!(messages::queued_popups(), [first, second]);
    assert!(messages::has_popup());
    assert_eq!(messages::take_popup().map(|message| message.id), Some(first));
    assert_eq!(messages::queued_popups(), [second]);
    messages::clear();
    assert!(!messages::has_popup() && messages::take_popup().is_none());
}

// The flood's popup: the land it covered, whether the dykes held or broke, the fields under water
// for how long, the buildings damaged, the silt; the river's other notices have none.
fn test_the_floods_popup() {
    let flood = |fields, buildings, breaches| FloodNotice::Flood {
        cells: 140,
        buildings_damaged: buildings,
        fields_flooded: fields,
        breaches,
        at: None,
    };
    let popup = popups::flood(&flood(6, 3, 0), "the Cinnabar River", 2, true).expect("a popup");
    assert_worded(&popup);
    assert_eq!((popup.kicker.as_str(), popup.title.as_str()), ("Flood", "The Cinnabar River floods"));
    assert_eq!(popup.text, "The water rose over 140 cells of land. The dykes held.");
    assert_eq!(line_texts(&popup), [
        "6 rice fields are under water for 2 months.",
        "3 buildings were damaged.",
        "The silt will make the flooded land richer.",
    ]);
    assert_eq!(popup.icon, "water");

    let broken = popups::flood(&flood(0, 0, 2), "the river", 1, true).unwrap();
    assert_worded(&broken);
    assert_eq!(broken.text, "The water rose over 140 cells of land. It broke through the dykes in 2 places.");
    assert_eq!(line_texts(&broken)[..2], ["The water stays for 1 month.", "No building was damaged."]);
    let no_dykes = popups::flood(&flood(1, 1, 0), "the river", 2, false).unwrap();
    assert_eq!(no_dykes.text, "The water rose over 140 cells of land.");

    assert!(popups::flood(&FloodNotice::RiverRising, "the river", 2, true).is_none());
    assert!(popups::flood(&FloodNotice::Receded { fields: 2 }, "the river", 2, true).is_none());
    assert!(popups::flood(&FloodNotice::Breached { breaches: 1, at: None }, "the river", 2, true).is_none());
}

fn request(amount: u32, legitimacy: i32) -> TradeRequest {
    TradeRequest {
        id: "court_bricks".into(),
        from: "thang_long".into(),
        good: ResourceKind::Bricks,
        amount,
        opened: 20,
        due: 26,
        legitimacy,
        relation: 0,
        again: 0,
    }
}

// A request opened: what's asked and by when, Legitimacy when it's met and when it's late; failed:
// the Legitimacy lost and, the court's, asked again for more. One met has no popup.
fn test_a_requests_popups() {
    let rules = &GameConfigs::get().sim.ratings;
    let notice = |event, court| TradeNotice::Request { event, request: request(300, 8), court };

    let opened = popups::request(&notice(RequestEvent::Opened, true), "Thăng Long", Realm::Kambara).unwrap();
    assert_worded(&opened);
    assert_eq!((opened.kicker.as_str(), opened.title.as_str()), ("Request from the court", "The court asks for bricks"));
    let lost = (8.0 * rules.request_failed_scale).round() as i32;
    assert_eq!(line_texts(&opened), [
        "Send 300 bricks within 6 months.".to_string(),
        "When it is met: Divine favour +8.".to_string(),
        format!("If it is late: Divine favour −{lost}."),
    ]);

    let city = popups::request(&notice(RequestEvent::Opened, false), "Vijara", Realm::DaiLong).unwrap();
    assert_worded(&city);
    assert_eq!((city.kicker.as_str(), city.title.as_str()), ("Request from Vijara", "Vijara asks for bricks"));
    let none = TradeNotice::Request { event: RequestEvent::Opened, request: request(10, 0), court: false };
    assert_eq!(popups::request(&none, "Vijara", Realm::DaiLong).unwrap().lines.len(), 1, "no Legitimacy at stake");

    let failed = popups::request(&notice(RequestEvent::Failed, true), "Thăng Long", Realm::DaiLong).unwrap();
    assert_worded(&failed);
    assert_eq!(failed.title, "The court's request went unmet");
    let again = (300.0 * rules.ask_again_scale).ceil() as u32;
    assert_eq!(line_texts(&failed), [
        format!("The Mandate −{lost}."),
        format!("The court will ask again, for {again} bricks, in {} months.", rules.ask_again_months),
    ]);
    assert!(popups::request(&notice(RequestEvent::Met, true), "Thăng Long", Realm::DaiLong).is_none());
    let caravan = TradeNotice::Gifts { partner: "vijara".into(), goods: vec![] };
    assert!(popups::request(&caravan, "Vijara", Realm::DaiLong).is_none());
}

// A blessing or a misfortune: its name, its message and what it does; a festival has none.
fn test_a_blessings_and_a_misfortunes_popups() {
    let notice =
        |kind, id: &str| FaithNotice { kind, patron: "water_lord".into(), id: id.into(), text: "It happened.".into() };
    let blessing = popups::faith(&notice(FaithNoticeKind::Blessing, "rich_harvest"), Realm::DaiLong).unwrap();
    assert_worded(&blessing);
    assert_eq!(blessing.kicker, "Blessing");
    assert_eq!(blessing.text, "It happened.");
    assert_eq!(line_texts(&blessing), ["Production rises by 50% for 3 months."]);
    let misfortune = popups::faith(&notice(FaithNoticeKind::Misfortune, "blight"), Realm::DaiLong).unwrap();
    assert_worded(&misfortune);
    assert_eq!(
        (misfortune.kicker.as_str(), line_texts(&misfortune)),
        ("Misfortune", vec!["Production falls by 50% for 3 months."])
    );
    let gentle = popups::faith(&notice(FaithNoticeKind::Blessing, "gentle_flood"), Realm::DaiLong).unwrap();
    assert_eq!(line_texts(&gentle), ["The next flood will be gentler."]);
    assert!(popups::faith(&notice(FaithNoticeKind::FestivalHeld, "tet"), Realm::DaiLong).is_none());
}

// An outbreak starting: when its people start to die, that it spreads, what cures it; a fire has none.
fn test_an_outbreaks_popup() {
    let notice = |kind| RiskNotice {
        kind,
        text: "Sickness has broken out in a house.".into(),
        target: MessageTarget::Cell(Cell::new(1, 1)),
    };
    let popup = popups::outbreak(&notice(RiskNoticeKind::Outbreak)).unwrap();
    assert_worded(&popup);
    assert_eq!((popup.kicker.as_str(), popup.title.as_str()), ("Sickness", "Sickness breaks out"));
    assert_eq!(popup.lines.len(), 3);
    assert!(popups::outbreak(&notice(RiskNoticeKind::Fire)).is_none());
    assert!(popups::outbreak(&notice(RiskNoticeKind::OutbreakOver)).is_none());
}

// Legitimacy's clock starting and its last month, and a time limit's last 3 months and last month,
// open popups; the warnings between are log lines.
fn test_the_last_warnings_popups() {
    let context = popups::FailureContext {
        drivers: vec![("Taxes".to_string(), -6), ("Festivals".to_string(), 4)],
        unmet: vec![popups::UnmetGoal { goal: "Culture".into(), achieved: "34".into(), target: "60".into() }],
    };
    let legitimacy = |months_left, started| FailureNotice::LegitimacyLow { months_left, started };
    let started = popups::failure(&legitimacy(12, true), "It fell.", Realm::DaiLong, &context).unwrap();
    assert_worded(&started);
    assert_eq!(started.title, "The Mandate has fallen too low");
    assert_eq!(line_texts(&started), ["Taxes: −6", "Festivals: +4"]);
    let last = popups::failure(&legitimacy(1, false), "Last.", Realm::Kambara, &context).unwrap();
    assert_eq!(last.title, "One month left to raise Divine favour");
    assert!(popups::failure(&legitimacy(6, false), "Six.", Realm::DaiLong, &context).is_none());

    let time = |months_left| FailureNotice::TimeLeft { months_left };
    let three = popups::failure(&time(3), "3 months left.", Realm::DaiLong, &context).unwrap();
    assert_worded(&three);
    assert_eq!((three.title.as_str(), line_texts(&three)), ("3 months left", vec!["Culture: 34 of 60"]));
    assert_eq!(popups::failure(&time(1), "1 month.", Realm::DaiLong, &context).unwrap().title, "1 month left");
    assert!(popups::failure(&time(6), "6 months.", Realm::DaiLong, &context).is_none());
    assert!(popups::failure(&time(12), "12 months.", Realm::DaiLong, &context).is_none());
}

// The debt's warnings 3 months and 1 month before it's overdue open popups while the game can be
// lost to it; the others are log lines.
fn test_the_debts_popups() {
    let notice = |kind| DebtNotice { kind, text: "In debt.".into() };
    let three = popups::debt(&notice(DebtNoticeKind::Warning { months_left: 3 }), -120, true).unwrap();
    assert_worded(&three);
    assert_eq!(three.title, "The debt is due in 3 months");
    assert_eq!(line_texts(&three), [
        "The treasury holds -120 gold.",
        "If it's still in debt in 3 months, the mission is lost."
    ]);
    assert!(popups::debt(&notice(DebtNoticeKind::Warning { months_left: 1 }), -5, true).is_some());
    assert!(popups::debt(&notice(DebtNoticeKind::Warning { months_left: 2 }), -5, true).is_none());
    assert!(popups::debt(&notice(DebtNoticeKind::Warning { months_left: 3 }), -5, false).is_none(), "can't be lost");
    assert!(popups::debt(&notice(DebtNoticeKind::InDebt), -5, true).is_none());
    assert!(popups::debt(&notice(DebtNoticeKind::OutOfDebt), 5, true).is_none());
}
