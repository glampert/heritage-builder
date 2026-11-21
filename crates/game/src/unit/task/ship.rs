use std::any::Any;

use common::coords::Cell;
use engine::{
    log,
    ui::{DrawDebugUi, UiSystem},
};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{UnitTask, UnitTaskContext, UnitTaskState, UnitTaskTransition};
use crate::{
    building::BuildingKindAndId,
    config::GameConfigs,
    pathfind::NodeKind as PathNodeKind,
    sim::{
        SimContextMut,
        resources::{RESOURCE_KIND_COUNT, ResourceKind, per_kind_map},
    },
    trade::{
        TradeNotice,
        ships::{self, BerthSearch},
    },
    unit::{Unit, UnitPostDespawnCb, navigation::UnitNavGoal},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskShip
// ----------------------------------------------

// A trade ship (docs/plans/trade.md, §5): a river or sea partner's junk that sails in from its water
// entry with the goods the city's import limits want, moors at the nearest working dock whose berth
// its water reaches and no ship holds (waiting off one while every dock is busy), and stays while the
// dock's dockers carry its goods to the stores and the city's exports back to it
// (crate::trade::ships). It sails home once the dock has no errand left for it or its stay is over,
// with its unsold goods, unpaid, and posts its message. Its cargo is kept here, as a caravan's is.

// The ground a ship sails: open water only (its path search keeps it out of canals, which are water
// too: crate::trade::ships::find_water_path).
pub const SHIP_NODE_KINDS: PathNodeKind = PathNodeKind::Water;

// How often a ship waiting for a berth looks again, in seconds.
const BERTH_RETRY_SECS: f32 = 2.0;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskShipState {
    // Looking for a dock with a free berth.
    #[default]
    FindingBerth,

    // Sailing to a dock's berth.
    Sailing,

    // Every dock its water reaches is busy: waiting off the nearest.
    WaitingOff,

    // At the berth, its claim on it sent to the dock (a deferred building update).
    Mooring,

    // Moored: the dock's dockers come and go.
    Moored,

    // Sailing home to its entry.
    Leaving,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UnitTaskShip {
    // The partner's city id.
    pub partner: String,
    // Where it came in, and leaves: open water at the map's edge.
    pub entry: Cell,

    // The partner's goods it still carries, and what it sold the city and bought from it so far.
    #[serde(with = "per_kind_map")]
    pub cargo: [u32; RESOURCE_KIND_COUNT],
    #[serde(with = "per_kind_map")]
    pub sold: [u32; RESOURCE_KIND_COUNT],
    #[serde(with = "per_kind_map")]
    pub bought: [u32; RESOURCE_KIND_COUNT],
    // The gold the city paid it, and the gold it paid the city.
    pub paid: u32,
    pub earned: u32,
    // How many goods it carries, all told.
    pub capacity: u32,

    // The dock it's bound for or moored at, and its berth (the water cell beside the dock it stops on).
    pub dock: Option<BuildingKindAndId>,
    pub berth: Option<Cell>,

    // Seconds moored, and seconds spent waiting for a berth (or for its claim to land).
    pub moored_secs: f32,
    pub waiting_secs: f32,

    pub state: UnitTaskShipState,
    // Whether it found no dock to reach, and turned back.
    pub turned_back: bool,
    // Whether its message was posted (as it sets out for home).
    pub reported: bool,
}

impl UnitTaskShip {
    // A ship from `partner` that comes in at `entry` carrying `cargo`.
    pub fn new(partner: &str, entry: Cell, cargo: [u32; RESOURCE_KIND_COUNT], capacity: u32) -> Self {
        Self {
            partner: partner.to_string(),
            entry,
            cargo,
            sold: [0; RESOURCE_KIND_COUNT],
            bought: [0; RESOURCE_KIND_COUNT],
            paid: 0,
            earned: 0,
            capacity,
            dock: None,
            berth: None,
            moored_secs: 0.0,
            waiting_secs: 0.0,
            state: UnitTaskShipState::default(),
            turned_back: false,
            reported: false,
        }
    }

    // What it carries, all told: its own goods and what it bought.
    pub fn load(&self) -> u32 {
        self.cargo.iter().sum::<u32>() + self.bought.iter().sum::<u32>()
    }

    // How many more goods it can take on.
    pub fn room(&self) -> u32 {
        self.capacity.saturating_sub(self.load())
    }

    #[inline]
    pub fn carries(&self, good: ResourceKind) -> u32 {
        self.cargo[good.index()]
    }

    // Moored at `dock` now.
    pub fn is_moored_at(&self, dock: BuildingKindAndId) -> bool {
        self.state == UnitTaskShipState::Moored && self.dock == Some(dock)
    }

    // The longest it stays moored, and waits for a berth, in seconds.
    fn stay_secs() -> f32 {
        let sim = &GameConfigs::get().sim;
        sim.trade.ship_stay_months.max(1) as f32 * sim.seconds_per_month
    }

    fn set_sail(&mut self, ctx: &mut UnitTaskContext, path: &crate::pathfind::Path) {
        let start = ctx.unit.cell();
        ctx.unit.move_to_goal(path, UnitNavGoal::tile(start, path));
    }

    fn turn_back(&mut self) -> UnitTaskTransition<UnitTaskShipState> {
        self.turned_back = true;
        UnitTaskTransition::Goto(UnitTaskShipState::Leaving)
    }

    fn update_finding_berth(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        let me = ctx.unit.id();
        match ships::find_berth(ctx.sim_context, ctx.unit.cell(), me) {
            BerthSearch::Free { dock, berth, path } => {
                self.dock = Some(dock);
                self.berth = Some(berth);
                if path.len() <= 1 {
                    return UnitTaskTransition::Goto(UnitTaskShipState::Mooring);
                }
                self.set_sail(ctx, &path);
                UnitTaskTransition::Goto(UnitTaskShipState::Sailing)
            }
            BerthSearch::Busy { path } => {
                self.dock = None;
                self.berth = None;
                if path.len() > 1 {
                    self.set_sail(ctx, &path);
                }
                UnitTaskTransition::Goto(UnitTaskShipState::WaitingOff)
            }
            BerthSearch::None => {
                log::info!(log::channel!("trade"), "A ship from '{}' finds no dock to reach and turns back.", self.partner);
                self.turn_back()
            }
        }
    }

    fn update_sailing(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        if ctx.unit.goal().is_none() {
            // Its way was blocked (something built on the water): look again.
            return UnitTaskTransition::Goto(UnitTaskShipState::FindingBerth);
        }
        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }
        ctx.unit.follow_path(None);
        UnitTaskTransition::Goto(UnitTaskShipState::Mooring)
    }

    fn update_waiting_off(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        if ctx.unit.goal().is_some() && !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay; // Still sailing up to wait.
        }
        if ctx.unit.goal().is_some() {
            ctx.unit.follow_path(None);
        }
        let before = self.waiting_secs;
        self.waiting_secs += ctx.sim_context.delta_time_secs();
        if self.waiting_secs >= Self::stay_secs() {
            log::info!(log::channel!("trade"), "A ship from '{}' waited too long for a berth and turns back.", self.partner);
            return self.turn_back();
        }
        // Look again every few seconds.
        if (before / BERTH_RETRY_SECS).floor() != (self.waiting_secs / BERTH_RETRY_SECS).floor() {
            return UnitTaskTransition::Goto(UnitTaskShipState::FindingBerth);
        }
        UnitTaskTransition::Stay
    }

    fn update_mooring(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        let Some(dock) = self.dock else {
            return UnitTaskTransition::Goto(UnitTaskShipState::FindingBerth);
        };
        let me = ctx.unit.id();
        match ships::berth_holder(ctx.sim_context, dock) {
            Some(Some(holder)) if holder == me => {
                self.moored_secs = 0.0;
                log::info!(
                    log::channel!("trade"),
                    "A ship from '{}' moors at the dock at {}.",
                    self.partner,
                    ctx.unit.cell()
                );
                UnitTaskTransition::Goto(UnitTaskShipState::Moored)
            }
            Some(None) => {
                // Claim the berth; the dock takes the claim after the step (first come, first served).
                ships::claim_berth(ctx.sim_cmds, dock, me);
                UnitTaskTransition::Stay
            }
            // Another ship holds it, or the dock is gone: look again.
            _ => UnitTaskTransition::Goto(UnitTaskShipState::FindingBerth),
        }
    }

    fn update_moored(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        let Some(dock) = self.dock else {
            return UnitTaskTransition::Goto(UnitTaskShipState::Leaving);
        };
        let me = ctx.unit.id();
        // The dock is gone, or no longer counts it as moored: home.
        if ships::berth_holder(ctx.sim_context, dock) != Some(Some(me)) {
            return UnitTaskTransition::Goto(UnitTaskShipState::Leaving);
        }
        self.moored_secs += ctx.sim_context.delta_time_secs();
        let stay = Self::stay_secs();
        let dockers_out = ships::dockers_out(ctx.sim_context, dock);
        // It leaves once no docker is out for it and there's nothing left to do, or its stay is over.
        // A docker still out keeps it a while longer (half a stay more at most), so the goods it
        // carries aren't stranded.
        let done = dockers_out == 0 && !ships::has_errand(ctx.sim_context, dock, self);
        let over = self.moored_secs >= stay && (dockers_out == 0 || self.moored_secs >= stay * 1.5);
        if done || over {
            ships::release_berth(ctx.sim_cmds, dock, me);
            return UnitTaskTransition::Goto(UnitTaskShipState::Leaving);
        }
        UnitTaskTransition::Stay
    }

    fn update_leaving(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskShipState> {
        if !self.reported {
            self.reported = true;
            crate::trade::notify(TradeNotice::ship(self));
        }
        if ctx.unit.goal().is_none() {
            if ctx.unit.cell() == self.entry {
                return UnitTaskTransition::Despawn(UnitPostDespawnCb::none());
            }
            let start = ctx.unit.cell();
            let entry = self.entry;
            match ships::find_water_path(ctx.sim_context, start, |cell| cell == entry) {
                Some(path) => {
                    self.set_sail(ctx, &path);
                    return UnitTaskTransition::Stay;
                }
                None => {
                    log::info!(
                        log::channel!("trade"),
                        "A ship from '{}' has no way out; it leaves where it is.",
                        self.partner
                    );
                    return UnitTaskTransition::Despawn(UnitPostDespawnCb::none());
                }
            }
        }
        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }
        ctx.unit.follow_path(None);
        UnitTaskTransition::Despawn(UnitPostDespawnCb::none())
    }
}

impl UnitTaskState for UnitTaskShipState {
    type Task = UnitTaskShip;

    fn update(self, task: &mut UnitTaskShip, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::FindingBerth => task.update_finding_berth(ctx),
            Self::Sailing => task.update_sailing(ctx),
            Self::WaitingOff => task.update_waiting_off(ctx),
            Self::Mooring => task.update_mooring(ctx),
            Self::Moored => task.update_moored(ctx),
            Self::Leaving => task.update_leaving(ctx),
        }
    }
}

impl UnitTask for UnitTaskShip {
    type State = UnitTaskShipState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        debug_assert!(ctx.unit.goal().is_none());
        ctx.unit.set_traversable_node_kinds(SHIP_NODE_KINDS);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _sim_context: &SimContextMut, ui_sys: &UiSystem) {
        #[derive(DrawDebugUi)]
        struct View {
            partner: String,
            entry: Cell,
            load: u32,
            room: u32,
            paid: u32,
            earned: u32,
            dock: String,
            moored_secs: f32,
            waiting_secs: f32,
            #[debug_ui(debug)]
            state: UnitTaskShipState,
        }
        View {
            partner: self.partner.clone(),
            entry: self.entry,
            load: self.load(),
            room: self.room(),
            paid: self.paid,
            earned: self.earned,
            dock: self.dock.map_or_else(|| "-".to_string(), |dock| format!("{} {}", dock.kind, dock.id)),
            moored_secs: self.moored_secs,
            waiting_secs: self.waiting_secs,
            state: self.state,
        }
        .draw_debug_ui(ui_sys);
        for good in ResourceKind::all().iter() {
            let (carries, sold, bought) = (self.cargo[good.index()], self.sold[good.index()], self.bought[good.index()]);
            if carries + sold + bought != 0 {
                engine::ui::text(
                    ui_sys,
                    common::format_small!("{}: carries {carries}, sold {sold}, bought {bought}", good.name()),
                );
            }
        }
    }
}
