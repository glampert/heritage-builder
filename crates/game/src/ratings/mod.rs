// The four ratings (docs/plans/ratings.md): Prosperity, Culture, Security and Legitimacy (the
// Mandate, Divine Favour or Merit), each the sum of its drivers, recomputed at each month's start
// (decision Q1). A realm's file says which drivers feed which rating (config); this reads them from
// the city. Events (festivals, requests, floods, disasters) count in full in their month and fade to
// nothing over RatingRules::fade_months (decision Q2). The state is sim::ratings::CityRatings.

pub mod config;
pub mod distinctions;
pub mod failure;

use common::{coords::CellRange, mem::SingleThreadStatic};
pub use config::{DriverDef, DriverKind, NO_FESTIVAL_DRIVER, RatingConfigs, RealmRatingsDef, driver_name, rating_name};
use engine::log;
use strum::IntoEnumIterator;

pub use crate::sim::ratings::{
    CityRatings,
    DriverValue,
    FailureReason,
    FailureSwitches,
    MAX_RATING,
    RATING_COUNT,
    Rating,
    RatingKind,
};
use crate::{
    building::{BuildingKind, HouseLevel, config::BuildingConfigs, footprints_within_reach, risks::RISK_MEDIUM},
    config::{GameConfigs, RatingRules},
    sim::{SimContext, SimContextMut, calendar::CalendarDate, finance::LedgerCategory},
    world::World,
};

// ----------------------------------------------
// Recompute
// ----------------------------------------------

// A new month's start (Simulation::update, last in the month, so the month's festival and the
// requests opened or failed count): the ratings recomputed, then whether the mission is lost
// (failure::check_failure), which opens the failure screen, and the distinctions broken this month
// spoiled (distinctions::check_spoiled).
pub fn on_new_month(context: &SimContextMut, _date: CalendarDate) {
    recompute(context);
    let terms = failure::mission_terms(context);
    if let Some(reason) = failure::check_failure(context, terms.as_ref()) {
        log::info!(log::channel!("ratings"), "The mission is lost: {reason}.");
        crate::campaign::request_failure_screen();
    }
    if let Some(mission_distinctions) = crate::campaign::active_distinctions() {
        distinctions::check_spoiled(context, mission_distinctions, terms.as_ref());
    }
}

// Every rating read again from the city and its realm's file, and the month it was read in kept.
// A realm without a file rates 0 everywhere.
pub fn recompute(context: &SimContextMut) {
    let rules = &GameConfigs::get().sim.ratings;
    let realm = context.realm();
    let configs = RatingConfigs::get_or_load();
    let facts = CityFacts::read(context);
    for kind in RatingKind::iter() {
        let rating = match configs.for_realm(realm) {
            Some(def) => rate(context, &facts, def.drivers(kind), rules),
            None => Rating::default(),
        };
        context.ratings_mut().set(kind, rating);
    }
    context.ratings_mut().set_rated_month(facts.now);
    let ratings = context.ratings();
    log::verbose!(
        log::channel!("ratings"),
        "Rated: prosperity {}, culture {}, security {}, legitimacy {}.",
        ratings.value(RatingKind::Prosperity),
        ratings.value(RatingKind::Culture),
        ratings.value(RatingKind::Security),
        ratings.value(RatingKind::Legitimacy)
    );
}

// A rating from its drivers: each read, rounded and held to its cap and floor; those the game
// doesn't offer (floods in a game without them) left out.
pub fn rate(context: &SimContext, facts: &CityFacts, drivers: &[DriverDef], rules: &RatingRules) -> Rating {
    let values = drivers
        .iter()
        .filter_map(|driver| {
            let (id, value) = read_driver(context, facts, driver, rules)?;
            let value = if value.is_finite() { value.round() as i32 } else { 0 };
            Some(DriverValue { id, value: value.clamp(driver.min.min(0), driver.max.max(0)) })
        })
        .collect();
    Rating::from_drivers(values)
}

// A recompute asked for while the simulation's parts are borrowed (the World Debug window's Ratings
// tab): Simulation::update runs it at the start of the next frame, paused or not.
static RECOMPUTE_REQUESTED: SingleThreadStatic<bool> = SingleThreadStatic::new(false);

pub fn request_recompute() {
    *RECOMPUTE_REQUESTED.as_mut() = true;
}

pub fn take_recompute_request() -> bool {
    std::mem::take(RECOMPUTE_REQUESTED.as_mut())
}

// ----------------------------------------------
// CityFacts
// ----------------------------------------------

// A finished house with people.
#[derive(Clone)]
pub struct HouseFact {
    pub footprint: CellRange,
    pub residents: u32,
    pub level: usize, // HouseLevel as an index, 0 up.
    pub hungry: bool,
}

// What several drivers read, gathered once a recompute.
#[derive(Clone, Default)]
pub struct CityFacts {
    pub now: u32, // The ordinal month.
    pub population: u32,
    pub houses: Vec<HouseFact>,
    pub employed: u32,
    pub unemployed: u32,
}

impl CityFacts {
    pub fn read(context: &SimContext) -> Self {
        let world = context.world();
        let mut houses = Vec::new();
        world.for_each_building(BuildingKind::House, |building| {
            let residents = building.population_count();
            if building.is_under_construction() || residents == 0 {
                return true;
            }
            let house = building.as_house();
            houses.push(HouseFact {
                footprint: building.cell_range(),
                residents,
                level: house.level() as usize,
                hungry: !house.has_food(),
            });
            true
        });
        let stats = &world.stats().population;
        Self {
            now: context.calendar().ordinal_month(),
            population: houses.iter().map(|house| house.residents).sum(),
            houses,
            employed: stats.employed,
            unemployed: stats.unemployed,
        }
    }

    #[inline]
    pub fn workforce(&self) -> u32 {
        self.employed + self.unemployed
    }

    // The residents of the houses `world`'s working buildings of `kinds` reach, along the grid,
    // within each one's effect radius, as faith's coverage counts them.
    pub fn residents_reached(&self, world: &World, kinds: BuildingKind) -> u32 {
        let configs = BuildingConfigs::get();
        let mut reach: Vec<(CellRange, i32)> = Vec::new();
        world.for_each_building(kinds, |building| {
            if building.is_operational() {
                reach.push((building.cell_range(), configs.find_service_config(building.kind()).effect_radius));
            }
            true
        });
        if reach.is_empty() {
            return 0;
        }
        self.houses
            .iter()
            .filter(|house| {
                reach.iter().any(|(footprint, radius)| footprints_within_reach(*footprint, house.footprint, *radius))
            })
            .map(|house| house.residents)
            .sum()
    }

    // `residents` over the population, 0 for an empty city.
    #[inline]
    pub fn share_of_people(&self, residents: u32) -> f32 {
        if self.population == 0 { 0.0 } else { residents as f32 / self.population as f32 }
    }
}

// ----------------------------------------------
// Drivers
// ----------------------------------------------

// What's left of an event `age` months old: all of it in its month, nothing after `months`.
#[inline]
pub fn fade(age: u32, months: u32) -> f32 {
    if months == 0 || age >= months { 0.0 } else { 1.0 - age as f32 / months as f32 }
}

// The part of the range from `min` (at 0) through 0 to `max` (at 1) that `share` stands for: a
// share above 0 scales the cap, below 0 the floor.
#[inline]
fn scaled(share: f32, driver: &DriverDef) -> f32 {
    if share >= 0.0 { driver.max as f32 * share.min(1.0) } else { -(driver.min as f32) * share.max(-1.0) }
}

// A driver's id as its breakdown shows it and its value before rounding, or None when the game
// doesn't offer it.
fn read_driver(context: &SimContext, facts: &CityFacts, driver: &DriverDef, rules: &RatingRules) -> Option<(String, f32)> {
    let world = context.world();
    let max = driver.max as f32;
    let value = match &driver.driver {
        DriverKind::HousingLevels {} => {
            let top = (HouseLevel::count() - 1) as f32;
            let levels: u32 = facts.houses.iter().map(|house| house.level as u32 * house.residents).sum();
            max * facts.share_of_people(levels) / top
        }
        DriverKind::Treasury { per_resident } => {
            let balance = context.finance().balance() as f32;
            let mark = facts.population.max(1) as f32 * per_resident;
            scaled(balance / mark, driver)
        }
        DriverKind::Employment { free_share } => {
            let workforce = facts.workforce();
            if workforce == 0 {
                0.0
            } else {
                let employed = facts.employed as f32 / workforce as f32;
                max * (employed / (1.0 - free_share)).min(1.0)
            }
        }
        DriverKind::Wages { standard } => {
            if facts.employed == 0 {
                0.0
            } else {
                let standard = standard.unwrap_or(GameConfigs::get().sim.finance.default_wage).max(1) as f32;
                let ratio = context.finance().wage() as f32 / standard;
                let half = max / 2.0;
                if ratio <= 1.0 {
                    driver.min as f32 + (half - driver.min as f32) * ratio
                } else {
                    half + half * (ratio - 1.0).min(1.0)
                }
            }
        }
        DriverKind::TradeIncome { per_resident } => {
            if !context.trade().is_enabled() {
                return None;
            }
            let finance = context.finance();
            let income =
                finance.this_year().total(LedgerCategory::Exports).max(finance.last_year().total(LedgerCategory::Exports));
            let mark = facts.population.max(1) as f32 * per_resident;
            max * (income.max(0) as f32 / mark).min(1.0)
        }
        DriverKind::FaithCoverage {} => {
            let reached = facts.residents_reached(world, BuildingKind::Shrine | BuildingKind::Temple);
            max * facts.share_of_people(reached)
        }
        DriverKind::ServiceCoverage { service } => max * facts.share_of_people(facts.residents_reached(world, *service)),
        DriverKind::FestivalCulture { none_penalty } => {
            let faith = context.faith();
            let culture: f32 = faith
                .log()
                .iter()
                .map(|record| record.culture * fade(facts.now.saturating_sub(record.ordinal_month), rules.fade_months))
                .sum();
            let held_this_year =
                faith.log().iter().any(|record| facts.now.saturating_sub(record.ordinal_month) < MONTHS_A_YEAR);
            if *none_penalty > 0 && !held_this_year && faith.is_festivals_enabled() && facts.population > 0 {
                return Some((NO_FESTIVAL_DRIVER.to_string(), -(*none_penalty as f32)));
            }
            culture
        }
        DriverKind::FestivalLegitimacy {} => context
            .faith()
            .log()
            .iter()
            .map(|record| record.legitimacy * fade(facts.now.saturating_sub(record.ordinal_month), rules.fade_months))
            .sum(),
        DriverKind::RiskSafety {} => {
            let mut buildings = 0u32;
            let mut safe = 0u32;
            world.for_each_building(BuildingKind::all(), |building| {
                if building.has_risks() {
                    buildings += 1;
                    if building.risks().highest() < RISK_MEDIUM {
                        safe += 1;
                    }
                }
                true
            });
            if buildings == 0 {
                0.0
            } else if !context.risks().is_active() {
                max
            } else {
                max * safe as f32 / buildings as f32
            }
        }
        DriverKind::CalmStreets { high_taxes } => {
            if facts.population == 0 {
                0.0
            } else {
                let workforce = facts.workforce();
                let unemployed = if workforce == 0 { 0.0 } else { facts.unemployed as f32 / workforce as f32 };
                let hungry: u32 = facts.houses.iter().filter(|house| house.hungry).map(|house| house.residents).sum();
                let hungry = facts.share_of_people(hungry);
                let taxes = match context.finance().tax_level() {
                    crate::sim::finance::TaxLevel::High => *high_taxes,
                    crate::sim::finance::TaxLevel::Harsh => *high_taxes * 2,
                    _ => 0,
                };
                max * (1.0 - unemployed) * (1.0 - hungry) - taxes as f32
            }
        }
        DriverKind::Disasters { per_building, per_house, per_death } => {
            let lost: f32 = context
                .risks()
                .disasters()
                .iter()
                .map(|month| {
                    let others = month.buildings_lost().saturating_sub(month.houses_lost) as f32;
                    let cost =
                        others * per_building + month.houses_lost as f32 * per_house + month.deaths as f32 * per_death;
                    cost * fade(facts.now.saturating_sub(month.ordinal_month), rules.fade_months)
                })
                .sum();
            -lost
        }
        DriverKind::Requests {} => {
            let trade = context.trade();
            if trade.scheduled().is_empty() && trade.requests().is_empty() && trade.request_log().is_empty() {
                return None;
            }
            trade
                .request_log()
                .iter()
                .map(|record| {
                    let legitimacy = record.request.legitimacy as f32;
                    let value = if record.met { legitimacy } else { -legitimacy * rules.request_failed_scale };
                    value * fade(facts.now.saturating_sub(record.closed), rules.request_fade_months)
                })
                .sum()
        }
        DriverKind::FairTaxes { by_level } => by_level[context.finance().tax_level().index()] as f32,
        DriverKind::Floods { held, per_breach, per_building_damaged } => {
            let floods = context.floods();
            if !floods.is_enabled() {
                return None;
            }
            floods
                .log()
                .iter()
                .map(|record| {
                    let answered = if record.breaches == 0 { *held } else { 0.0 };
                    let cost = record.breaches as f32 * per_breach + record.buildings_damaged as f32 * per_building_damaged;
                    (answered - cost) * fade(facts.now.saturating_sub(record.ordinal_month), rules.fade_months)
                })
                .sum()
        }
        DriverKind::Favour {} => {
            let patrons = context.faith().patrons();
            if patrons.is_empty() {
                return None;
            }
            let mean = patrons.values().map(|patron| patron.favour).sum::<f32>() / patrons.len() as f32;
            max * (mean / 100.0).clamp(0.0, 1.0)
        }
    };
    Some((driver.id.clone(), value))
}

const MONTHS_A_YEAR: u32 = crate::sim::calendar::MONTHS_PER_YEAR;

// ----------------------------------------------
// What the screens show
// ----------------------------------------------

// A rating as the HUD, the City book and the debrief show it, in a realm's words.
#[derive(Clone, PartialEq, Debug)]
pub struct RatingView {
    pub kind: RatingKind,
    pub name: String,
    pub value: u32,
    // The mission's goal for it, if it has one.
    pub goal: Option<u32>,
    // Each driver's name and value, in its breakdown's order.
    pub drivers: Vec<(String, i32)>,
}

impl RatingView {
    pub fn is_goal_met(&self) -> bool {
        self.goal.is_some_and(|goal| self.value >= goal)
    }

    // Its largest drivers: up to `count` raising it, then up to `count` lowering it, each the
    // largest first.
    pub fn largest_drivers(&self, count: usize) -> Vec<(String, i32)> {
        let mut up: Vec<_> = self.drivers.iter().filter(|(_, value)| *value > 0).cloned().collect();
        let mut down: Vec<_> = self.drivers.iter().filter(|(_, value)| *value < 0).cloned().collect();
        up.sort_by_key(|(_, value)| -value);
        down.sort_by_key(|(_, value)| *value);
        up.into_iter().take(count).chain(down.into_iter().take(count)).collect()
    }
}

// The four ratings in `realm`'s words, with the mission's goals; None while the city isn't rated
// yet.
pub fn views(ratings: &CityRatings, realm: crate::realm::Realm) -> Option<Vec<RatingView>> {
    if !ratings.is_rated() {
        return None;
    }
    let views = RatingKind::iter()
        .map(|kind| {
            let rating = ratings.get(kind);
            RatingView {
                kind,
                name: rating_name(realm, kind),
                value: rating.value,
                goal: mission_goal(kind),
                drivers: rating.drivers.iter().map(|driver| (driver_name(realm, &driver.id), driver.value)).collect(),
            }
        })
        .collect();
    Some(views)
}

// The four ratings' values, in RatingKind's order.
pub fn values(ratings: &CityRatings) -> [u32; RATING_COUNT] {
    let mut values = [0; RATING_COUNT];
    for (value, kind) in values.iter_mut().zip(RatingKind::iter()) {
        *value = ratings.value(kind);
    }
    values
}

// The game's goal for a rating (MissionGoal::Rating), the highest if it sets two: a mission's, or a
// custom game's goal set's; None in a game without one.
pub fn mission_goal(kind: RatingKind) -> Option<u32> {
    crate::campaign::game_requirements()?.rating_goal(kind)
}
