// Court requests (docs/plans/trade.md, §6): a mission's schedule asks for goods (the court, from
// the player's capital, or another city of the world map), each to be sent within so many months.
//
// The schedule is kept in the trade state from the mission's start (CityTrade::start). Each month's
// start (on_new_month, from trade::on_new_month) fails the requests whose months ran out and opens
// the scheduled ones due. The player sends a request from the City book's Goods page (Send now)
// once the storage yards and granaries hold its amount: the goods are taken at once, as a festival
// takes its own (send). The page's button queues the send (request_send), run at the start of the
// next frame (run_requests). Each opened, met or failed request posts a TradeNotice::Request. The
// log counts toward MissionGoal::RequestsFulfilled, and its requests' Legitimacy feeds the ratings'
// `requests` driver (docs/plans/ratings.md, §3); their relations effects wait for game Phase 2. A
// failed court request is asked again, for more (ask_again).

use common::mem::SingleThreadStatic;
use engine::{
    log,
    ui::text::{self, UiTextCategory},
};

use super::{TradeNotice, notify, place_name, world_map};
use crate::{
    config::GameConfigs,
    sim::{
        SimContext,
        SimContextMut,
        resources::{ResourceKind, StockItem},
        trade::{CityTrade, TradeRequest},
    },
    unit::task::TRADE_STORES,
    world::{World, city_stock::CityStock},
    world_map::{
        config::{CityKind, WorldMapDef},
        provider::CourtRequest,
    },
};

// ----------------------------------------------
// Who asks
// ----------------------------------------------

// Whether `city` is the court: the player's capital on the trade state's world map.
pub fn is_court(trade: &CityTrade, city: &str) -> bool {
    world_map(trade.world_map()).is_some_and(|map| is_court_on(map, city))
}

// Whether `city` is the player's capital on `map`.
pub fn is_court_on(map: &WorldMapDef, city: &str) -> bool {
    map.city(city).is_some_and(|city| city.kind == CityKind::Capital && city.player)
}

// The court's city on a world map: the player's capital. None for a map without one.
pub fn court_city(trade: &CityTrade) -> Option<String> {
    let map = world_map(trade.world_map())?;
    map.cities.iter().find(|city| city.kind == CityKind::Capital && city.player).map(|city| city.id.clone())
}

// A request as the player reads it: "Pottery for the court", "Pottery for Vijara".
pub fn title(trade: &CityTrade, request: &TradeRequest) -> String {
    let good = request.good.display_name();
    if is_court(trade, &request.from) {
        let key = if request.again > 0 { "book.requests.for_court_again" } else { "book.requests.for_court" };
        text::format(UiTextCategory::City, key, &[("good", &good)])
    } else {
        let city = place_name(trade.world_map(), &request.from);
        text::format(UiTextCategory::City, "book.requests.for_city", &[("good", &good), ("city", &city)])
    }
}

// The ordinal month the game is in.
pub fn current_month(context: &SimContext) -> u32 {
    let sim = &GameConfigs::get().sim;
    context.calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month()
}

// What the city holds of a good for a request: its storage yards and granaries, staffed or not.
pub fn held(context: &SimContext, good: ResourceKind) -> u32 {
    CityStock::available_in(context.world(), TRADE_STORES, good)
}

// ----------------------------------------------
// Each month
// ----------------------------------------------

// A new month, `now` (from trade::on_new_month, whether the game trades or not): the requests whose
// months ran out fail, then the scheduled ones due open, each with its notice. A failed court
// request is asked again (ask_again).
pub fn on_new_month(context: &SimContextMut, now: u32) {
    let (failed, opened) = context.trade_mut().on_new_month(now);
    for record in failed {
        log::info!(log::channel!("trade"), "The request '{}' failed.", record.request.id);
        let court = is_court(context.trade(), &record.request.from);
        if court {
            ask_again(context, &record.request, now);
        }
        notify(TradeNotice::Request { event: RequestEvent::Failed, request: record.request, court });
    }
    for request in opened {
        log::info!(log::channel!("trade"), "The request '{}' opened.", request.id);
        let court = is_court(context.trade(), &request.from);
        notify(TradeNotice::Request { event: RequestEvent::Opened, request, court });
    }
}

// After a failed court request, the court asks for more (GDD 5.14; docs/plans/ratings.md, Q4): the
// same good, RatingRules::ask_again_scale times the amount, ask_again_months later, with as many
// months to send it. A request asked again is a request like any other, so failing it is asked
// again too, for more again.
pub fn ask_again(context: &SimContextMut, failed: &TradeRequest, now: u32) {
    let rules = &GameConfigs::get().sim.ratings;
    let again = failed.asked_again(now, rules.ask_again_months, rules.ask_again_scale);
    let (id, opens) = (again.id.clone(), again.opened);
    if context.trade_mut().schedule(again) {
        log::info!(log::channel!("trade"), "The court will ask again: '{id}', in month {opens}.");
    } else {
        log::warning!(log::channel!("trade"), "The court can't ask again: the request '{id}' is known already.");
    }
}

// What happened to a request, for its notice.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RequestEvent {
    Opened,
    Met,
    Failed,
}

// ----------------------------------------------
// Sending
// ----------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SendError {
    NotUnderWay,                      // No request of that id is under way.
    Short { held: u32, amount: u32 }, // The stores hold less than it asks for.
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUnderWay => write!(f, "no such request is under way"),
            Self::Short { held, amount } => write!(f, "the stores hold {held} of the {amount} it asks for"),
        }
    }
}

// Whether the request `id` can be sent now: it's under way and the stores hold its amount.
pub fn can_send(context: &SimContext, id: &str) -> Result<(), SendError> {
    let request = context.trade().request(id).ok_or(SendError::NotUnderWay)?;
    let held = held(context, request.good);
    if held < request.amount {
        return Err(SendError::Short { held, amount: request.amount });
    }
    Ok(())
}

// Sends the request `id` (Q8): its goods are taken from the yards and granaries at once, and it's
// met and logged, with its notice.
pub fn send(context: &SimContextMut, id: &str) -> Result<(), SendError> {
    can_send(context, id)?;
    let request = context.trade().request(id).cloned().ok_or(SendError::NotUnderWay)?;
    CityStock::withdraw_from(context.world_mut(), TRADE_STORES, &[StockItem { kind: request.good, count: request.amount }]);
    let now = current_month(context);
    let record = context.trade_mut().meet_request(id, now).ok_or(SendError::NotUnderWay)?;
    let court = is_court(context.trade(), &record.request.from);
    notify(TradeNotice::Request { event: RequestEvent::Met, request: record.request, court });
    Ok(())
}

// Sends asked for by the UI (the Goods page's Send now), run at the start of the next frame
// (Simulation::update), since the page draws while the world is read.
static SENDS: SingleThreadStatic<Vec<String>> = SingleThreadStatic::new(Vec::new());

pub fn request_send(id: &str) {
    SENDS.as_mut().push(id.to_string());
}

#[inline]
pub fn has_requests() -> bool {
    !SENDS.as_ref().is_empty()
}

pub fn run_requests(context: &SimContextMut) {
    for id in std::mem::take(SENDS.as_mut()) {
        if let Err(error) = send(context, &id) {
            log::warning!(log::channel!("trade"), "The request '{id}' wasn't sent: {error}.");
        }
    }
}

// ----------------------------------------------
// What the player sees
// ----------------------------------------------

// A request under way as the Goods page's card and the Overview's warning show it.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct RequestRow {
    pub id: String,
    pub title: String, // "Pottery for the court".
    pub good: ResourceKind,
    pub amount: u32,
    pub held: u32,
    pub months_left: u32,
    pub court: bool,
    pub from: String, // The asker's name ("Vijara"); the court's city's for the court.
}

impl RequestRow {
    #[inline]
    pub fn can_send(&self) -> bool {
        self.held >= self.amount
    }
}

// The requests under way in month `now`, soonest due first, with what the city holds for each.
pub fn rows(world: &World, trade: &CityTrade, now: u32) -> Vec<RequestRow> {
    let mut rows: Vec<RequestRow> = trade
        .requests()
        .iter()
        .map(|request| RequestRow {
            id: request.id.clone(),
            title: title(trade, request),
            good: request.good,
            amount: request.amount,
            held: CityStock::available_in(world, TRADE_STORES, request.good),
            months_left: request.months_left(now),
            court: is_court(trade, &request.from),
            from: place_name(trade.world_map(), &request.from),
        })
        .collect();
    rows.sort_by_key(|row| row.months_left);
    rows
}

// The world map's request box for `city` in a game whose trade is on that map: its request under way
// that's due soonest, if any.
pub fn panel_request(trade: &CityTrade, city: &str, now: u32) -> Option<CourtRequest> {
    let request =
        trade.requests().iter().filter(|request| request.from == city).min_by_key(|request| request.months_left(now))?;
    Some(CourtRequest {
        goods: text::format(UiTextCategory::Menu, "world_map.request.goods", &[
            ("good", &request.good.display_name()),
            ("amount", &request.amount.to_string()),
        ]),
        within: text::format_count(
            UiTextCategory::Menu,
            "world_map.request.within",
            i64::from(request.months_left(now)),
            &[],
        ),
    })
}

// A request's notice in the message log: its kind, sound and text. `city` names its asker.
pub(crate) fn notice_text(event: RequestEvent, request: &TradeRequest, court: bool, city: &str) -> String {
    let goods = super::goods_text(&[(request.good, request.amount)]);
    let months = text::format_count(
        UiTextCategory::Messages,
        "request.months",
        i64::from(request.due.saturating_sub(request.opened)),
        &[],
    );
    let again = request.again > 0;
    let key = match (event, court) {
        (RequestEvent::Opened, true) if again => "request.opened_court_again",
        (RequestEvent::Opened, true) => "request.opened_court",
        (RequestEvent::Opened, false) => "request.opened",
        (RequestEvent::Met, true) => "request.met_court",
        (RequestEvent::Met, false) => "request.met",
        (RequestEvent::Failed, true) => "request.failed_court",
        (RequestEvent::Failed, false) => "request.failed",
    };
    text::format(UiTextCategory::Messages, key, &[("city", city), ("goods", &goods), ("months", &months)])
}
