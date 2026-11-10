// Fire and collapse risk per building (docs/plans/city-risks.md).
//
// Each risk builds up from 0 to MAX_RISK at its building config's rate, in points per calendar
// month, and a patrol walker's visit resets it: a fire watcher's the fire risk, a builder's the
// collapse risk. A well within reach halves the fire risk's rate.
//
// A building whose collapse risk reaches the top collapses; one whose fire risk does catches
// fire. A burning building stops working and raises its neighbours' fire risk, and burns down
// after `burn_secs` unless a fire watcher puts the fire out. Building::update drives this, and
// building::destruction removes what burns down or collapses and leaves rubble.

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

use super::BuildingKind;
use crate::{
    config::{GameConfigs, RiskConfigs},
    sim::SimContext,
    unit::{Unit, task::UnitTaskFightFire},
};

// A risk's ceiling: a building at MAX_RISK is due to catch fire or collapse.
pub const MAX_RISK: f32 = 100.0;

// Where the info panel's "Low", "Medium" and "High" begin (Phase 3), and what risks.state
// counts as high.
pub const RISK_MEDIUM: f32 = 40.0;
pub const RISK_HIGH: f32 = 75.0;

// ----------------------------------------------
// RiskKind
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumCount, EnumIter, EnumString, Serialize, Deserialize)]
pub enum RiskKind {
    Fire,
    Collapse,
}

// ----------------------------------------------
// RiskRates
// ----------------------------------------------

// How fast a building's risks build up, in points per calendar month: the `risks` block of its
// building config. Left out, both are 0 and the building never burns or collapses.
#[derive(Copy, Clone, Default, PartialEq, DrawDebugUi, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiskRates {
    pub fire: f32,
    pub collapse: f32,
}

impl RiskRates {
    pub const ZERO: Self = Self { fire: 0.0, collapse: 0.0 };

    #[inline]
    pub fn get(&self, kind: RiskKind) -> f32 {
        match kind {
            RiskKind::Fire => self.fire,
            RiskKind::Collapse => self.collapse,
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.fire <= 0.0 && self.collapse <= 0.0
    }

    #[inline]
    pub fn scaled(&self, factor: f32) -> Self {
        Self { fire: self.fire * factor, collapse: self.collapse * factor }
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
// BuildingRisks
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct BuildingRisks {
    fire: f32,
    collapse: f32,

    // Whether a small or large well reaches the building, checked on `water_check_timer`.
    near_water: bool,
    water_check_timer: UpdateTimer,

    // Some while the building burns.
    burning: Option<BurningState>,

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
            near_water: false,
            water_check_timer: UpdateTimer::default(),
            burning: None,
            doomed: false,
            jitter: default_jitter(),
        }
    }
}

impl BuildingRisks {
    pub fn new(base_cell: Cell, kind: BuildingKind, configs: &RiskConfigs) -> Self {
        let mut risks = Self {
            water_check_timer: UpdateTimer::new(configs.water_check_secs),
            jitter: jitter(base_cell, kind, configs.rate_jitter),
            ..Self::default()
        };
        // A new building finds out about the wells around it on its first step.
        risks.water_check_timer.force_update();
        risks
    }

    pub fn post_load(&mut self, base_cell: Cell, kind: BuildingKind, configs: &RiskConfigs) {
        self.water_check_timer.post_load(configs.water_check_secs);
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
        }
    }

    // Sets a risk, clamped to 0..=MAX_RISK (automation, debug tools, later milestones' events).
    pub fn set(&mut self, kind: RiskKind, value: f32) {
        let value = if value.is_finite() { value.clamp(0.0, MAX_RISK) } else { 0.0 };
        match kind {
            RiskKind::Fire => self.fire = value,
            RiskKind::Collapse => self.collapse = value,
        }
    }

    #[inline]
    pub fn add(&mut self, kind: RiskKind, amount: f32) {
        self.set(kind, self.get(kind) + amount);
    }

    // A patrol walker's visit: a fire watcher's resets the fire risk, a builder's the collapse risk.
    #[inline]
    pub fn reset(&mut self, kind: RiskKind) {
        self.set(kind, 0.0);
    }

    // The higher of the two risks.
    #[inline]
    pub fn highest(&self) -> f32 {
        self.fire.max(self.collapse)
    }

    #[inline]
    pub fn is_near_water(&self) -> bool {
        self.near_water
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
    // its jitter, with the fire rate scaled down near water.
    pub fn effective_rates(&self, config_rates: &RiskRates, configs: &RiskConfigs) -> RiskRates {
        let mut rates = config_rates.scaled(self.jitter);
        if self.near_water {
            rates.fire *= configs.near_water_fire_scale;
        }
        rates
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

    // Builds up both risks over `delta_time_secs` of simulated time.
    pub fn build_up(&mut self, config_rates: &RiskRates, configs: &RiskConfigs, delta_time_secs: Seconds) {
        let seconds_per_month = GameConfigs::get().sim.seconds_per_month;
        if seconds_per_month <= 0.0 {
            return;
        }
        let rates = self.effective_rates(config_rates, configs);
        let months = delta_time_secs / seconds_per_month;
        self.add(RiskKind::Fire, rates.fire * months);
        self.add(RiskKind::Collapse, rates.collapse * months);
    }
}

// The risk a walker's visit resets, if any: a fire watcher's (a patrol from a fire watch, or one
// sent to a fire) the fire risk, a builder's (from a builders' guild) the collapse risk. Walkers
// are told apart by their task and the building their patrol came from, as tax collectors are,
// so a builder can wear a peasant's sprites.
pub fn risk_reset_by(unit: &Unit, context: &SimContext) -> Option<RiskKind> {
    if unit.is_running_task::<UnitTaskFightFire>(context.task_manager()) {
        return Some(RiskKind::Fire);
    }
    let origin = unit.patrol_task_building_kind(context)?;
    if origin == BuildingKind::FireWatch {
        Some(RiskKind::Fire)
    } else if origin == BuildingKind::BuildersGuild {
        Some(RiskKind::Collapse)
    } else {
        None
    }
}

// ----------------------------------------------
// Notices
// ----------------------------------------------

// Fires, fires put out, buildings burnt down and collapses since the GameLoop last took them;
// it shows each as a toast (GameLoop::notify_risks). The simulation has no Engine, hence the
// queue, as for construction and finance notices.
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
