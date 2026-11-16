// Festivals (docs/plans/faith.md, §4, decisions Q8 to Q12). Each realm's festival year is its
// festivals, each once a year in its month, for its patron or every patron. The player plans one at
// a scale (one plan at a time); it's held at the start of its month if a festival square is staffed
// and the treasury, the storage yards and the granaries hold its costs, which are taken then. A
// festival can also be held at once during its own month, while it hasn't been held that year: the
// planner's "Hold now". Holding one raises its patron's favour (every patron's for an "every
// patron" festival), logs it for M9's Culture and Legitimacy, dresses the squares for the month
// (TileFlags::Festive) and posts a message; one that can't be held posts what was short.

use common::mem::SingleThreadStatic;
use engine::{
    log,
    ui::text::{self, UiTextCategory},
};
use strum::Display;

use super::config::{self, FaithConfigs, FestivalDef, FestivalScaleDef, RealmFaithDef};
use crate::{
    building::{BuildingKind, BuildingKindAndId},
    config::GameConfigs,
    sim::{
        GameCalendar,
        SimContext,
        SimContextMut,
        calendar::CalendarDate,
        faith::{CityFaith, FaithNotice, FaithNoticeKind, FestivalPlan, FestivalRecord},
        finance::{Finance, LedgerCategory},
        resources::StockItem,
    },
    tile::{TileFlags, TileKind},
    world::{
        World,
        city_stock::{CityStock, FESTIVAL_STOCK_BUILDINGS},
    },
};

// ----------------------------------------------
// Terms
// ----------------------------------------------

// What a festival costs and gives at a scale: its middle scale's figures (the file's) times the
// scale's `cost` and `effect`, the costs rounded to whole gold and goods.
#[derive(Clone, PartialEq, Debug)]
pub struct FestivalTerms {
    pub gold: u32,
    pub goods: Vec<StockItem>,
    pub favour: f32,
    pub culture: f32,
    pub legitimacy: f32,
}

pub fn terms(festival: &FestivalDef, scale: &FestivalScaleDef) -> FestivalTerms {
    let cost = |count: u32| (count as f32 * scale.cost).round().max(0.0) as u32;
    FestivalTerms {
        gold: cost(festival.gold),
        goods: festival
            .goods()
            .into_iter()
            .map(|(kind, count)| StockItem { kind, count: cost(count) })
            .filter(|item| item.count != 0)
            .collect(),
        favour: festival.favour * scale.effect,
        culture: festival.culture * scale.effect,
        legitimacy: festival.legitimacy * scale.effect,
    }
}

// ----------------------------------------------
// The festival year
// ----------------------------------------------

// Where a festival stands this year.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display)]
#[strum(serialize_all = "snake_case")]
pub enum FestivalState {
    Planned, // The festival planned: held at the start of its next month.
    Held,    // Held this year.
    Now,     // Its month is this one and it isn't held yet: it can be held at once.
    ToCome,  // Its month is later this year.
    Missed,  // Its month passed this year without it: next year's.
}

pub fn state(faith: &CityFaith, festival: &FestivalDef, date: CalendarDate) -> FestivalState {
    if faith.plan().is_some_and(|plan| plan.festival == festival.id) {
        FestivalState::Planned
    } else if faith.was_held_in(&festival.id, date.year) {
        FestivalState::Held
    } else if festival.month == date.month {
        FestivalState::Now
    } else if festival.month > date.month {
        FestivalState::ToCome
    } else {
        FestivalState::Missed
    }
}

// The festival the planner shows first: the one planned, else this month's if it can still be held,
// else the next to come (next year's first after the year's last).
pub fn next_festival<'a>(def: &'a RealmFaithDef, faith: &CityFaith, date: CalendarDate) -> Option<&'a FestivalDef> {
    if let Some(plan) = faith.plan()
        && let Some(festival) = def.festival(&plan.festival)
    {
        return Some(festival);
    }
    let ahead = |festival: &FestivalDef| {
        let months = (festival.month + 12 - date.month) % 12;
        if months == 0 && faith.was_held_in(&festival.id, date.year) { 12 } else { months }
    };
    def.festivals.iter().min_by_key(|festival| ahead(festival))
}

// The middle scale (Grand), the one the file's figures are for, else the first.
pub fn default_scale(def: &RealmFaithDef) -> Option<&FestivalScaleDef> {
    def.scales.iter().find(|scale| scale.cost == 1.0).or(def.scales.first())
}

// ----------------------------------------------
// Venues and costs
// ----------------------------------------------

// The festival squares that can hold a festival now: finished, staffed and not burning.
pub fn venues(world: &World) -> Vec<BuildingKindAndId> {
    let mut venues = Vec::new();
    world.for_each_building(BuildingKind::FestivalSquare, |building| {
        if building.is_operational() {
            venues.push(building.kind_and_id());
        }
        true
    });
    venues
}

// What keeps a festival from being held: no venue, or gold and goods short.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Shortage {
    pub venue: bool,           // No festival square with its workers.
    pub gold: u32,             // Gold the treasury lacks (a treasury in debt pays nothing).
    pub goods: Vec<StockItem>, // Goods the storage yards and granaries lack, by how many.
}

impl Shortage {
    pub fn is_empty(&self) -> bool {
        !self.venue && self.gold == 0 && self.goods.is_empty()
    }

    // "no festival square with its workers; short of 120 gold, 20 Rice".
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.venue {
            parts.push(text::get(UiTextCategory::Messages, "faith.festival.no_venue").to_string());
        }
        let mut short = Vec::new();
        if self.gold != 0 {
            short.push(text::format(UiTextCategory::Messages, "faith.festival.gold", &[("gold", &self.gold.to_string())]));
        }
        for item in &self.goods {
            short.push(format!("{} {}", item.count, item.kind.display_name()));
        }
        if !short.is_empty() {
            parts.push(text::format(UiTextCategory::Messages, "faith.festival.short", &[("what", &short.join(", "))]));
        }
        parts.join("; ")
    }
}

// What the city holds of a festival's costs: Ok, or what's short.
pub fn check(world: &World, finance: &Finance, terms: &FestivalTerms) -> Result<(), Shortage> {
    let shortage = Shortage {
        venue: venues(world).is_empty(),
        gold: (i64::from(terms.gold) - finance.balance().max(0)).clamp(0, i64::from(u32::MAX)) as u32,
        goods: terms
            .goods
            .iter()
            .filter_map(|item| {
                let held = CityStock::available_in(world, FESTIVAL_STOCK_BUILDINGS, item.kind);
                (held < item.count).then(|| StockItem { kind: item.kind, count: item.count - held })
            })
            .collect(),
    };
    if shortage.is_empty() { Ok(()) } else { Err(shortage) }
}

// How many of a good the storage yards and granaries hold (the planner's costs).
pub fn held(world: &World, kind: crate::sim::resources::ResourceKind) -> u32 {
    CityStock::available_in(world, FESTIVAL_STOCK_BUILDINGS, kind)
}

// ----------------------------------------------
// Holding
// ----------------------------------------------

// Why a festival wasn't held.
#[derive(Clone, PartialEq, Debug)]
pub enum NotHeld {
    Unknown(String), // No such festival or scale in the game's realm.
    Disabled,        // Festivals are off for the mission.
    AlreadyHeld,     // Held this year already.
    Short(Shortage),
}

impl NotHeld {
    pub fn describe(&self) -> String {
        match self {
            Self::Unknown(error) => error.clone(),
            Self::Disabled => "Festivals are off for this mission.".to_string(),
            Self::AlreadyHeld => "It was held this year already.".to_string(),
            Self::Short(shortage) => shortage.describe(),
        }
    }
}

// Holds `festival` at `scale` now, on `date`: takes its gold and goods, raises its patrons'
// favour, logs it, dresses the festival squares for the month and posts its message. A plan for
// it is done with.
pub fn hold(context: &SimContextMut, festival_id: &str, scale_id: &str, date: CalendarDate) -> Result<(), NotHeld> {
    let realm = context.faith().realm();
    let def = FaithConfigs::get_or_load()
        .for_realm(realm)
        .ok_or_else(|| NotHeld::Unknown(format!("{realm} has no festivals.")))?;
    let (festival, scale) = find(def, festival_id, scale_id).map_err(NotHeld::Unknown)?;
    if !context.faith().is_festivals_enabled() {
        return Err(NotHeld::Disabled);
    }
    if context.faith().was_held_in(&festival.id, date.year) {
        return Err(NotHeld::AlreadyHeld);
    }
    let terms = terms(festival, scale);
    check(context.world(), context.finance(), &terms).map_err(NotHeld::Short)?;

    context.finance_mut().spend(LedgerCategory::Festivals, terms.gold);
    CityStock::withdraw_from(context.world_mut(), FESTIVAL_STOCK_BUILDINGS, &terms.goods);

    let faith = context.faith_mut();
    if festival.is_for_every_patron() {
        for patron in &def.patrons {
            faith.add_favour(&patron.id, terms.favour);
        }
    } else {
        faith.add_favour(&festival.patron, terms.favour);
    }
    faith.record_festival(FestivalRecord {
        festival: festival.id.clone(),
        scale: scale.id.clone(),
        ordinal_month: date.ordinal_month(),
        culture: terms.culture,
        legitimacy: terms.legitimacy,
    });
    if faith.plan().is_some_and(|plan| plan.festival == festival.id) {
        faith.set_plan(None);
    }

    // The squares dressed for it, until the month ends.
    for venue in venues(context.world()) {
        if let Some(building) = context.world().find_building(venue.kind, venue.id) {
            context.tile_map_mut().set_tile_flags(building.base_cell(), TileKind::Building, TileFlags::Festive, true);
        }
    }

    let text = text::format(UiTextCategory::Messages, "faith.festival.held", &[
        ("festival", &config::festival_name(realm, &festival.id)),
        ("scale", &config::scale_name(realm, &scale.id).to_lowercase()),
        ("patrons", &patrons_text(realm, festival)),
        ("favour", &format!("{:.0}", terms.favour)),
    ]);
    log::info!(log::channel!("faith"), "Festival held: {} ({}) on {}", festival.id, scale.id, date.ordinal_month());
    context.faith_mut().push_notice(FaithNotice {
        kind: FaithNoticeKind::FestivalHeld,
        patron: festival.patron.clone(),
        id: festival.id.clone(),
        text,
    });
    Ok(())
}

// Posts that `festival` couldn't be held, and why.
fn post_not_held(context: &SimContextMut, festival: &FestivalDef, why: &NotHeld) {
    let realm = context.faith().realm();
    let text = text::format(UiTextCategory::Messages, "faith.festival.not_held", &[
        ("festival", &config::festival_name(realm, &festival.id)),
        ("reason", &why.describe()),
    ]);
    log::info!(log::channel!("faith"), "Festival not held: {} ({})", festival.id, why.describe());
    context.faith_mut().push_notice(FaithNotice {
        kind: FaithNoticeKind::FestivalNotHeld,
        patron: festival.patron.clone(),
        id: festival.id.clone(),
        text,
    });
}

// "The Water Lord", or "Every patron".
pub fn patrons_text(realm: crate::realm::Realm, festival: &FestivalDef) -> String {
    if festival.is_for_every_patron() {
        text::get(UiTextCategory::Messages, "faith.festival.every_patron").to_string()
    } else {
        config::patron_name(realm, &festival.patron)
    }
}

// ----------------------------------------------
// Planning
// ----------------------------------------------

// What planning a festival did.
#[derive(Clone, PartialEq, Debug)]
pub enum PlanOutcome {
    Planned,          // Held at the start of its month.
    Held,             // In its month: held at once.
    NotHeld(NotHeld), // In its month, but short: not held, and not planned (a message says why).
}

// Plans `festival` at `scale` for its next month, in place of any other plan. In its own month,
// while it hasn't been held this year, it's held at once instead, or not (the outcome and a message
// say what was short). Errors: no such festival or scale, or festivals are off for the mission.
pub fn plan(context: &SimContextMut, festival_id: &str, scale_id: &str) -> Result<PlanOutcome, String> {
    let realm = context.faith().realm();
    let def = FaithConfigs::get_or_load().for_realm(realm).ok_or_else(|| format!("{realm} has no festivals."))?;
    let (festival, scale) = find(def, festival_id, scale_id)?;
    if !context.faith().is_festivals_enabled() {
        return Err(NotHeld::Disabled.describe());
    }
    let date = today(context);
    if state(context.faith(), festival, date) == FestivalState::Now {
        return Ok(match hold(context, &festival.id, &scale.id, date) {
            Ok(()) => PlanOutcome::Held,
            Err(why) => {
                post_not_held(context, festival, &why);
                PlanOutcome::NotHeld(why)
            }
        });
    }
    context.faith_mut().set_plan(Some(FestivalPlan { festival: festival.id.clone(), scale: scale.id.clone() }));
    log::info!(log::channel!("faith"), "Festival planned: {} ({})", festival.id, scale.id);
    Ok(PlanOutcome::Planned)
}

// Drops the plan. False if there was none.
pub fn cancel(faith: &mut CityFaith) -> bool {
    let planned = faith.plan().is_some();
    faith.set_plan(None);
    planned
}

// A new month (faith::events::on_new_month, after the favour's drift): last month's festival is
// over, and the festival planned for this month is held, or not, and the plan is done with.
pub fn on_new_month(context: &SimContextMut, date: CalendarDate) {
    undress_squares(context);

    let Some(plan) = context.faith().plan().cloned() else {
        return;
    };
    let realm = context.faith().realm();
    let Some(festival) = FaithConfigs::get_or_load().for_realm(realm).and_then(|def| def.festival(&plan.festival)) else {
        context.faith_mut().set_plan(None); // Its realm's file no longer lists it.
        return;
    };
    if festival.month != date.month {
        return;
    }
    if let Err(why) = hold(context, &plan.festival, &plan.scale, date) {
        context.faith_mut().set_plan(None);
        if why != NotHeld::Disabled {
            post_not_held(context, festival, &why);
        }
    }
}

// The festival squares lose last month's festival dress.
fn undress_squares(context: &SimContextMut) {
    let mut cells = Vec::new();
    context.world().for_each_building(BuildingKind::FestivalSquare, |building| {
        cells.push(building.base_cell());
        true
    });
    for cell in cells {
        if context.tile_map().find_tile(cell, TileKind::Building).is_some_and(|tile| tile.has_flags(TileFlags::Festive)) {
            context.tile_map_mut().set_tile_flags(cell, TileKind::Building, TileFlags::Festive, false);
        }
    }
}

fn find<'a>(
    def: &'a RealmFaithDef,
    festival_id: &str,
    scale_id: &str,
) -> Result<(&'a FestivalDef, &'a FestivalScaleDef), String> {
    let festival = def.festival(festival_id).ok_or_else(|| {
        let ids: Vec<&str> = def.festivals.iter().map(|festival| festival.id.as_str()).collect();
        format!("No festival '{festival_id}'. Festivals: {}.", ids.join(", "))
    })?;
    let scale = def.scale(scale_id).ok_or_else(|| {
        let ids: Vec<&str> = def.scales.iter().map(|scale| scale.id.as_str()).collect();
        format!("No scale '{scale_id}'. Scales: {}.", ids.join(", "))
    })?;
    Ok((festival, scale))
}

// The calendar's date.
pub fn today(context: &SimContext) -> CalendarDate {
    date_of(context.calendar())
}

pub fn date_of(calendar: &GameCalendar) -> CalendarDate {
    let configs = &GameConfigs::get().sim;
    calendar.date(configs.days_per_month, configs.seconds_per_month)
}

// ----------------------------------------------
// Requests
// ----------------------------------------------

// What the City book's planner asked for while it was drawn: Simulation::update runs it at the
// start of the next frame, when nothing holds the world (as faith::events::request does).
#[derive(Clone)]
pub enum Request {
    Plan { festival: String, scale: String },
    Cancel,
}

static REQUESTS: SingleThreadStatic<Vec<Request>> = SingleThreadStatic::new(Vec::new());

pub fn request(request: Request) {
    REQUESTS.as_mut().push(request);
}

#[inline]
pub fn has_requests() -> bool {
    !REQUESTS.as_ref().is_empty()
}

pub fn run_requests(context: &SimContextMut) {
    for request in std::mem::take(REQUESTS.as_mut()) {
        match request {
            Request::Plan { festival, scale } => {
                if let Err(error) = plan(context, &festival, &scale) {
                    log::warning!(log::channel!("faith"), "{error}");
                }
            }
            Request::Cancel => {
                cancel(context.faith_mut());
            }
        }
    }
}
