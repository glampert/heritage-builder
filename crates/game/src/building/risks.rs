// Fire, collapse and disease risk per building (docs/plans/city-risks.md).
//
// Each risk builds up from 0 to MAX_RISK at its building config's rate, in points per calendar
// month, and a patrol walker's visit resets it: a fire watcher's the fire risk, a builder's the
// collapse risk, a medic's the disease risk. A well within reach halves the fire risk's rate.
//
// A building whose collapse risk reaches the top collapses; one whose fire risk does catches
// fire. A burning building stops working and raises its neighbours' fire risk, and burns down
// after `burn_secs` unless a fire watcher puts the fire out. Building::update drives this, and
// building::destruction removes what burns down or collapses and leaves rubble.
//
// Disease is a house's (§6): its rate grows with the house's level and how full it is, faster
// without water or food and slower with an apothecary in reach. A disease risk at the top sets
// nothing off; instead the house rolls for an outbreak now and then, at a chance that grows with
// the risk. A sick house spreads disease risk to the houses around it and loses residents to
// death after a grace period, until a medic's visit cures it or no one is left.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, EnumString};

use common::{
    coords::Cell,
    hash,
    mem::SingleThreadStatic,
    time::{Seconds, UpdateTimer},
};
use engine::log;
use proc_macros::DrawDebugUi;

use super::{BuildingKind, BuildingKindAndId};
use crate::{
    config::{GameConfigs, RiskConfigs},
    sim::SimContext,
    unit::{Unit, task::UnitTaskRespond},
    world::World,
};

// A risk's ceiling: a building at MAX_RISK is due to catch fire or collapse.
pub const MAX_RISK: f32 = 100.0;

// Where the info panel's "Low", "Medium" and "High" begin, and what risks.state counts as high.
pub const RISK_MEDIUM: f32 = 40.0;
pub const RISK_HIGH: f32 = 75.0;

// What the info panel calls a risk: "Low" under RISK_MEDIUM, "Medium" under RISK_HIGH, "High" from
// there.
pub fn risk_level_name(value: f32) -> &'static str {
    if value >= RISK_HIGH {
        "High"
    } else if value >= RISK_MEDIUM {
        "Medium"
    } else {
        "Low"
    }
}

// ----------------------------------------------
// RiskKind
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumCount, EnumIter, EnumString, Serialize, Deserialize)]
pub enum RiskKind {
    Fire,
    Collapse,
    Disease,
}

// ----------------------------------------------
// RiskRates
// ----------------------------------------------

// How fast a building's risks build up, in points per calendar month: the `risks` block of its
// building config. Left out, they are 0 and the building never burns, collapses or falls sick.
// Only house levels give a disease rate.
#[derive(Copy, Clone, Default, PartialEq, DrawDebugUi, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiskRates {
    pub fire: f32,
    pub collapse: f32,
    pub disease: f32,
}

impl RiskRates {
    pub const ZERO: Self = Self { fire: 0.0, collapse: 0.0, disease: 0.0 };

    #[inline]
    pub fn get(&self, kind: RiskKind) -> f32 {
        match kind {
            RiskKind::Fire => self.fire,
            RiskKind::Collapse => self.collapse,
            RiskKind::Disease => self.disease,
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.fire <= 0.0 && self.collapse <= 0.0 && self.disease <= 0.0
    }

    #[inline]
    pub fn scaled(&self, factor: f32) -> Self {
        Self { fire: self.fire * factor, collapse: self.collapse * factor, disease: self.disease * factor }
    }
}

// ----------------------------------------------
// BurningState
// ----------------------------------------------

// A building on fire: the simulated seconds until it burns down.
#[derive(Clone, Serialize, Deserialize)]
pub struct BurningState {
    pub secs_left: Seconds,
}

// ----------------------------------------------
// OutbreakState
// ----------------------------------------------

// A sick house: how long it has been sick, when its residents next die, the share of them that
// die then, and how many have died so far (for the notice when it ends).
#[derive(Clone, Serialize, Deserialize)]
pub struct OutbreakState {
    pub secs: Seconds,
    pub next_death_secs: Seconds,
    pub fraction: f32,
    pub deaths: u32,
}

// ----------------------------------------------
// BuildingRisks
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct BuildingRisks {
    fire: f32,
    collapse: f32,
    disease: f32,

    // Whether a small or large well reaches the building, checked on `water_check_timer`. A house
    // also checks then for an apothecary in reach and for food in its stock.
    near_water: bool,
    near_apothecary: bool,
    has_food: bool,
    water_check_timer: UpdateTimer,

    // When a house next rolls for an outbreak.
    disease_check_timer: UpdateTimer,

    // Some while the building burns.
    burning: Option<BurningState>,

    // Some while the house is sick.
    outbreak: Option<OutbreakState>,

    // Set when the building's destruction is queued (building::destruction), which runs at the
    // end of the same step: nothing else happens to it, and it can't be queued twice. A save
    // never sees it set, so it isn't saved.
    #[serde(skip)]
    doomed: bool,

    // This building's share of its config's rates (1 - rate_jitter to 1 + rate_jitter), so a row
    // of identical buildings built together doesn't burn all at once. It comes from the
    // building's cell and kind, not the RNG: recomputed on spawn and load, never saved.
    #[serde(skip, default = "default_jitter")]
    jitter: f32,
}

#[inline]
const fn default_jitter() -> f32 {
    1.0
}

impl Default for BuildingRisks {
    fn default() -> Self {
        Self {
            fire: 0.0,
            collapse: 0.0,
            disease: 0.0,
            near_water: false,
            near_apothecary: false,
            has_food: false,
            water_check_timer: UpdateTimer::default(),
            disease_check_timer: UpdateTimer::default(),
            burning: None,
            outbreak: None,
            doomed: false,
            jitter: default_jitter(),
        }
    }
}

impl BuildingRisks {
    pub fn new(base_cell: Cell, kind: BuildingKind, configs: &RiskConfigs) -> Self {
        let mut risks = Self {
            water_check_timer: UpdateTimer::new(configs.water_check_secs),
            disease_check_timer: UpdateTimer::new(configs.disease_check_secs),
            jitter: jitter(base_cell, kind, configs.rate_jitter),
            ..Self::default()
        };
        // A new building finds out about the wells around it on its first step.
        risks.water_check_timer.force_update();
        risks
    }

    pub fn post_load(&mut self, base_cell: Cell, kind: BuildingKind, configs: &RiskConfigs) {
        self.water_check_timer.post_load(configs.water_check_secs);
        self.disease_check_timer.post_load(configs.disease_check_secs);
        self.refresh_jitter(base_cell, kind, configs);
    }

    // After the building's base cell moves (a house that expands, a debug teleport), so its
    // jitter is the one a load will compute.
    pub fn refresh_jitter(&mut self, base_cell: Cell, kind: BuildingKind, configs: &RiskConfigs) {
        self.jitter = jitter(base_cell, kind, configs.rate_jitter);
    }

    #[inline]
    pub fn get(&self, kind: RiskKind) -> f32 {
        match kind {
            RiskKind::Fire => self.fire,
            RiskKind::Collapse => self.collapse,
            RiskKind::Disease => self.disease,
        }
    }

    // Sets a risk, clamped to 0..=MAX_RISK (automation, debug tools, later milestones' events).
    pub fn set(&mut self, kind: RiskKind, value: f32) {
        let value = if value.is_finite() { value.clamp(0.0, MAX_RISK) } else { 0.0 };
        match kind {
            RiskKind::Fire => self.fire = value,
            RiskKind::Collapse => self.collapse = value,
            RiskKind::Disease => self.disease = value,
        }
    }

    #[inline]
    pub fn add(&mut self, kind: RiskKind, amount: f32) {
        self.set(kind, self.get(kind) + amount);
    }

    // A patrol walker's visit: a fire watcher's resets the fire risk, a builder's the collapse
    // risk, a medic's the disease risk.
    #[inline]
    pub fn reset(&mut self, kind: RiskKind) {
        self.set(kind, 0.0);
    }

    // The highest of the risks.
    #[inline]
    pub fn highest(&self) -> f32 {
        self.fire.max(self.collapse).max(self.disease)
    }

    #[inline]
    pub fn is_near_water(&self) -> bool {
        self.near_water
    }

    #[inline]
    pub fn is_near_apothecary(&self) -> bool {
        self.near_apothecary
    }

    #[inline]
    pub fn has_food(&self) -> bool {
        self.has_food
    }

    #[inline]
    pub fn set_disease_needs(&mut self, near_apothecary: bool, has_food: bool) {
        self.near_apothecary = near_apothecary;
        self.has_food = has_food;
    }

    #[inline]
    pub fn is_burning(&self) -> bool {
        self.burning.is_some()
    }

    #[inline]
    pub fn burning(&self) -> Option<&BurningState> {
        self.burning.as_ref()
    }

    // Sets the building on fire for `burn_secs`, its fire risk at the top.
    pub fn start_burning(&mut self, burn_secs: Seconds) {
        self.burning = Some(BurningState { secs_left: burn_secs.max(0.0) });
        self.fire = MAX_RISK;
    }

    // Puts the fire out: the building is as it was, with no fire risk (decision Q9).
    pub fn put_out(&mut self) {
        self.burning = None;
        self.fire = 0.0;
        self.doomed = false;
    }

    // Burns for `delta_time_secs`: true once the building has burnt down.
    pub fn advance_burning(&mut self, delta_time_secs: Seconds) -> bool {
        match &mut self.burning {
            Some(burning) => {
                burning.secs_left = (burning.secs_left - delta_time_secs).max(0.0);
                burning.secs_left <= 0.0
            }
            None => false,
        }
    }

    #[inline]
    pub fn has_outbreak(&self) -> bool {
        self.outbreak.is_some()
    }

    #[inline]
    pub fn outbreak(&self) -> Option<&OutbreakState> {
        self.outbreak.as_ref()
    }

    #[inline]
    pub fn outbreak_mut(&mut self) -> Option<&mut OutbreakState> {
        self.outbreak.as_mut()
    }

    // Makes the house sick, its disease risk at the top. No one dies for `death_grace_secs`.
    pub fn start_outbreak(&mut self, configs: &RiskConfigs) {
        self.outbreak = Some(OutbreakState {
            secs: 0.0,
            next_death_secs: configs.death_grace_secs.max(0.0),
            fraction: configs.death_fraction.max(0.0),
            deaths: 0,
        });
        self.disease = MAX_RISK;
    }

    // Ends the outbreak with the disease risk at `disease`, returning how many died of it.
    pub fn end_outbreak(&mut self, disease: f32) -> u32 {
        let deaths = self.outbreak.take().map_or(0, |outbreak| outbreak.deaths);
        self.set(RiskKind::Disease, disease);
        deaths
    }

    // Ticks the outbreak roll's timer: true when the house should roll again.
    #[inline]
    pub fn disease_check_due(&mut self, delta_time_secs: Seconds) -> bool {
        self.disease_check_timer.tick(delta_time_secs).should_update()
    }

    #[inline]
    pub fn is_doomed(&self) -> bool {
        self.doomed
    }

    #[inline]
    pub fn set_doomed(&mut self) {
        self.doomed = true;
    }

    #[inline]
    pub fn jitter(&self) -> f32 {
        self.jitter
    }

    // The rates this building's risks build up at now, in points per month: its config's, times
    // its jitter, with the fire rate scaled down near water and the disease rate by its needs.
    // How full a house is scales its disease rate too; Building::risk_rates applies that.
    pub fn effective_rates(&self, config_rates: &RiskRates, configs: &RiskConfigs) -> RiskRates {
        let mut rates = config_rates.scaled(self.jitter);
        if self.near_water {
            rates.fire *= configs.near_water_fire_scale;
        }
        rates.disease *= self.disease_needs_factor(configs);
        rates
    }

    // What a house's needs make of its disease rate: 1, plus `missing_water_scale` without a well
    // in reach and `missing_food_scale` without food, times `apothecary_scale` with an apothecary.
    pub fn disease_needs_factor(&self, configs: &RiskConfigs) -> f32 {
        let mut factor = 1.0;
        if !self.near_water {
            factor += configs.missing_water_scale;
        }
        if !self.has_food {
            factor += configs.missing_food_scale;
        }
        if self.near_apothecary {
            factor *= configs.apothecary_scale;
        }
        factor.max(0.0)
    }

    // Ticks the well check's timer: true when it's time to look for wells again.
    #[inline]
    pub fn water_check_due(&mut self, delta_time_secs: Seconds) -> bool {
        self.water_check_timer.tick(delta_time_secs).should_update()
    }

    #[inline]
    pub fn set_near_water(&mut self, near_water: bool) {
        self.near_water = near_water;
    }

    // Builds up the risks over `delta_time_secs` of simulated time at `rates` (effective_rates,
    // and for a house how full it is). A sick house's disease risk stays at the top.
    pub fn build_up(&mut self, rates: &RiskRates, delta_time_secs: Seconds) {
        let seconds_per_month = GameConfigs::get().sim.seconds_per_month;
        if seconds_per_month <= 0.0 {
            return;
        }
        let months = delta_time_secs / seconds_per_month;
        self.add(RiskKind::Fire, rates.fire * months);
        self.add(RiskKind::Collapse, rates.collapse * months);
        if !self.has_outbreak() {
            self.add(RiskKind::Disease, rates.disease * months);
        }
    }
}

// The chance a house with `disease` risk falls sick over one roll, `check_secs` apart: the
// monthly chance, `outbreak_chance_at_max` x (risk / 100) squared, spread over the month's rolls.
pub fn outbreak_chance(disease: f32, check_secs: Seconds, configs: &RiskConfigs) -> f32 {
    let seconds_per_month = GameConfigs::get().sim.seconds_per_month;
    if disease <= 0.0 || check_secs <= 0.0 || seconds_per_month <= 0.0 {
        return 0.0;
    }
    let share = (disease / MAX_RISK).clamp(0.0, 1.0);
    let monthly = (configs.outbreak_chance_at_max * share * share).clamp(0.0, 1.0);
    if monthly >= 1.0 {
        return 1.0;
    }
    1.0 - (1.0 - monthly).powf(check_secs / seconds_per_month)
}

// The risk a walker's visit resets, if any: a fire watcher's (a patrol from a fire watch, or one
// sent to a fire) the fire risk, a builder's (from a builders' guild) the collapse risk, a
// medic's (a patrol from a hospital, or one sent to an outbreak) the disease risk. Walkers are
// told apart by their task and the building their patrol came from, as tax collectors are, so a
// builder can wear a peasant's sprites.
pub fn risk_reset_by(unit: &Unit, context: &SimContext) -> Option<RiskKind> {
    if let Some(task) = unit.current_task_as::<UnitTaskRespond>(context.task_manager()) {
        return Some(task.emergency.risk_kind());
    }
    let origin = unit.patrol_task_building_kind(context)?;
    if origin == BuildingKind::FireWatch {
        Some(RiskKind::Fire)
    } else if origin == BuildingKind::BuildersGuild {
        Some(RiskKind::Collapse)
    } else if origin == BuildingKind::Hospital {
        Some(RiskKind::Disease)
    } else {
        None
    }
}

// ----------------------------------------------
// RiskSummary
// ----------------------------------------------

// One building's risks, as a RiskSummary lists them.
#[derive(Clone)]
pub struct RiskEntry {
    pub kind_and_id: BuildingKindAndId,
    pub name: &'static str,
    pub kind: BuildingKind,
    pub cell: Cell,
    pub fire: f32,
    pub collapse: f32,
    pub disease: f32,
    pub near_water: bool,
    pub population: u32,
    pub burning: Option<BurningState>,
    pub outbreak: Option<OutbreakState>,
}

impl RiskEntry {
    #[inline]
    pub fn highest(&self) -> f32 {
        self.fire.max(self.collapse).max(self.disease)
    }

    fn sort_cell(&self) -> (i32, i32) {
        (self.cell.y, self.cell.x)
    }
}

// The city's risks at a glance, for risks.state and the World Debug window's Risks tab: how many
// buildings can build up risk and how many are at high risk, the highest of each risk, the
// riskiest buildings, the fires and the outbreaks. Sites and buildings whose configs give them
// no risks are left out (Building::has_risks).
#[derive(Default)]
pub struct RiskSummary {
    pub buildings: usize,
    high: [usize; RiskKind::COUNT],
    highest: [f32; RiskKind::COUNT],
    pub riskiest: Vec<RiskEntry>,  // Highest risk first, ties by cell; at most the count asked for.
    pub fires: Vec<RiskEntry>,     // By cell.
    pub outbreaks: Vec<RiskEntry>, // By cell.
}

impl RiskSummary {
    // Lists at most `max_riskiest` of the riskiest buildings. Lists are ordered by cell where
    // their values tie, so they read the same on every run.
    pub fn of(world: &World, max_riskiest: usize) -> Self {
        let mut summary = Self::default();
        world.for_each_building_of_any_kind(|building| {
            if !building.has_risks() {
                return true;
            }
            let risks = building.risks();
            let entry = RiskEntry {
                kind_and_id: building.kind_and_id(),
                name: building.name(),
                kind: building.kind(),
                cell: building.base_cell(),
                fire: risks.fire,
                collapse: risks.collapse,
                disease: risks.disease,
                near_water: risks.near_water,
                population: building.population_count(),
                burning: risks.burning.clone(),
                outbreak: risks.outbreak.clone(),
            };

            summary.buildings += 1;
            for kind in [RiskKind::Fire, RiskKind::Collapse, RiskKind::Disease] {
                let value = risks.get(kind);
                summary.high[kind as usize] += usize::from(value >= RISK_HIGH);
                summary.highest[kind as usize] = summary.highest[kind as usize].max(value);
            }
            if entry.burning.is_some() {
                summary.fires.push(entry.clone());
            }
            if entry.outbreak.is_some() {
                summary.outbreaks.push(entry.clone());
            }
            summary.riskiest.push(entry);
            true
        });

        summary.riskiest.sort_by(|a, b| b.highest().total_cmp(&a.highest()).then_with(|| a.sort_cell().cmp(&b.sort_cell())));
        summary.riskiest.truncate(max_riskiest);
        summary.fires.sort_by_key(RiskEntry::sort_cell);
        summary.outbreaks.sort_by_key(RiskEntry::sort_cell);
        summary
    }

    // How many buildings have `kind` risk at RISK_HIGH or more.
    #[inline]
    pub fn high(&self, kind: RiskKind) -> usize {
        self.high[kind as usize]
    }

    // The highest `kind` risk of any building.
    #[inline]
    pub fn highest(&self, kind: RiskKind) -> f32 {
        self.highest[kind as usize]
    }
}

// ----------------------------------------------
// Notices
// ----------------------------------------------

// Fires, fires put out, buildings burnt down, collapses and outbreaks since the GameLoop last
// took them; it shows each as a toast (GameLoop::notify_risks). The simulation has no Engine,
// hence the queue, as for construction notices.
static NOTICES: SingleThreadStatic<Vec<String>> = SingleThreadStatic::new(Vec::new());

// Logs `message` on the risks channel and queues it as a toast.
pub(crate) fn notify(message: String) {
    log::info!(log::channel!("risks"), "{message}");
    NOTICES.as_mut().push(message);
}

pub fn take_notices() -> Vec<String> {
    std::mem::take(NOTICES.as_mut())
}

// A factor in [1 - spread, 1 + spread] from the building's cell and kind: the same building in
// the same place always gets the same one, on every run and after every load.
fn jitter(base_cell: Cell, kind: BuildingKind, spread: f32) -> f32 {
    let spread = spread.clamp(0.0, 0.9);
    if spread == 0.0 {
        return 1.0;
    }
    let cell_bits = (base_cell.x as u32 as u64) | ((base_cell.y as u32 as u64) << 32);
    let key = cell_bits ^ u64::from(kind.bits()).wrapping_mul(hash::SPLITMIX64_GAMMA);
    1.0 - spread + 2.0 * spread * hash::hash_to_unit_f32(hash::splitmix64(key))
}
