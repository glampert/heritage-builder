use std::any::Any;

use common::coords::Cell;
use engine::{
    log,
    ui::{DrawDebugUi, UiSystem},
};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{UnitTask, UnitTaskContext, UnitTaskState, UnitTaskTransition, visit_destination_deferred, with_task};
use crate::{
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo, BuildingVisitResult},
    pathfind::{NodeKind as PathNodeKind, SearchResult},
    sim::{
        SimContext,
        SimContextMut,
        resources::{RESOURCE_KIND_COUNT, ResourceKind, per_kind_map},
    },
    trade::{self, Holdings, TradeNotice},
    unit::{Unit, UnitPostDespawnCb, navigation::UnitNavGoal},
    world::city_stock::FESTIVAL_STOCK_BUILDINGS,
};

// ----------------------------------------------
// UnitTaskTrade
// ----------------------------------------------

// A caravan (docs/plans/trade.md, §4): a partner's trader who walks in from the map's edge with the
// goods the city's import limits want, and goes from store to store (storage yards and granaries,
// the nearest where it has something to do first). At each it sells what the store takes and the
// city still wants, then buys what the city exports above its limit, the gold moving as each good
// changes hands (crate::trade::trade_at_store). It leaves once it's sold everything and is full,
// when no store in reach is left to trade with, or after its last stop, and walks back to where it
// came in. Its cargo is kept here, not in the unit's one-good inventory, and isn't the city's.

// The ground a caravan walks: land and roads, as settlers do.
pub const TRADER_NODE_KINDS: PathNodeKind = PathNodeKind::from_bits_retain(
    PathNodeKind::EmptyLand.bits() | PathNodeKind::Road.bits() | PathNodeKind::SettlersSpawnPoint.bits(),
);

// The stores a caravan trades at.
pub const TRADE_STORES: BuildingKind = FESTIVAL_STOCK_BUILDINGS;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskTradeState {
    // Looking for the next store to trade at.
    #[default]
    FindingStore,

    // Walking to a store.
    MovingToStore,

    // At a store, waiting for the deferred visit that trades.
    AtStore,

    // Walking back to where it came in.
    Leaving,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UnitTaskTrade {
    // The partner's city id.
    pub partner: String,
    // Where it came in, and leaves.
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

    // How many goods it carries, all told, and how many more stores it trades at.
    pub capacity: u32,
    pub stops_left: u32,

    // The store it's walking to, and those it traded at already.
    pub target: Option<BuildingKindAndId>,
    pub visited: Vec<BuildingKindAndId>,

    pub state: UnitTaskTradeState,
    // Set by the deferred visit, consumed by `AtStore`.
    pub visit_done: bool,
    // Whether its message was posted (when it leaves the last store).
    pub reported: bool,
    // Envoys bringing gifts (faith's Envoys): they unload free and buy nothing.
    pub gift: bool,
}

impl UnitTaskTrade {
    // A caravan from `partner` that comes in at `entry` carrying `cargo`.
    pub fn new(partner: &str, entry: Cell, cargo: [u32; RESOURCE_KIND_COUNT], capacity: u32, stops: u32) -> Self {
        Self {
            partner: partner.to_string(),
            entry,
            cargo,
            sold: [0; RESOURCE_KIND_COUNT],
            bought: [0; RESOURCE_KIND_COUNT],
            paid: 0,
            earned: 0,
            capacity,
            stops_left: stops,
            target: None,
            visited: Vec::new(),
            state: UnitTaskTradeState::default(),
            visit_done: false,
            reported: false,
            gift: false,
        }
    }

    // Envoys bringing their cargo as gifts.
    pub fn as_gift(mut self) -> Self {
        self.gift = true;
        self
    }

    // What it carries, all told: its own goods and what it bought.
    pub fn load(&self) -> u32 {
        self.cargo.iter().sum::<u32>() + self.bought.iter().sum::<u32>()
    }

    // How many more goods it can buy.
    pub fn room(&self) -> u32 {
        self.capacity.saturating_sub(self.load())
    }

    #[inline]
    pub fn carries(&self, good: ResourceKind) -> u32 {
        self.cargo[good.index()]
    }

    // Whether `building`, a store, is worth a visit: one it hasn't traded at yet where it can sell or
    // buy something now.
    fn has_business_at(&self, context: &SimContext, building: &Building, holdings: &Holdings) -> bool {
        building.is(TRADE_STORES)
            && !self.visited.contains(&building.kind_and_id())
            && trade::has_business_at(context, self, building, holdings)
    }

    fn update_finding_store(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskTradeState> {
        if self.stops_left == 0 {
            return UnitTaskTransition::Goto(UnitTaskTradeState::Leaving);
        }
        let sim_context = ctx.sim_context;
        let holdings = trade::holdings(sim_context);
        let start = ctx.unit.cell();
        let result = sim_context.find_nearest_buildings(
            start,
            TRADE_STORES,
            ctx.unit.traversable_node_kinds(),
            None,
            |building, _path| !self.has_business_at(sim_context, building, &holdings),
        );
        let Some((building, path)) = result else {
            return UnitTaskTransition::Goto(UnitTaskTradeState::Leaving);
        };
        self.target = Some(building.kind_and_id());
        // It stands on whichever cell next to the store the path ends at.
        let tile = BuildingTileInfo { road_link: path.last().unwrap().cell, base_cell: building.base_cell() };
        ctx.unit.move_to_goal(path, UnitNavGoal::building(BuildingKind::empty(), start, building.kind(), tile));
        UnitTaskTransition::Goto(UnitTaskTradeState::MovingToStore)
    }

    fn update_moving_to_store(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskTradeState> {
        if ctx.unit.goal().is_none() {
            // Lost its way (the path was blocked): look again.
            return UnitTaskTransition::Goto(UnitTaskTradeState::FindingStore);
        }
        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }
        self.visit_done = false;
        let visiting =
            visit_destination_deferred(ctx.unit, ctx.sim_cmds, ctx.sim_context, |context, building, unit, result| {
                if result == BuildingVisitResult::Accepted {
                    trade::trade_at_store(context, building, unit);
                }
                with_task::<UnitTaskTrade>(unit, context, |task, _unit| task.visit_done = true);
            });
        if visiting {
            UnitTaskTransition::Goto(UnitTaskTradeState::AtStore)
        } else {
            // The store is gone.
            ctx.unit.follow_path(None);
            self.stops_left = self.stops_left.saturating_sub(1);
            UnitTaskTransition::Goto(UnitTaskTradeState::FindingStore)
        }
    }

    fn update_at_store(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskTradeState> {
        if !self.visit_done {
            return UnitTaskTransition::Stay;
        }
        self.visit_done = false;
        ctx.unit.follow_path(None);
        if let Some(target) = self.target.take() {
            self.visited.push(target);
        }
        self.stops_left = self.stops_left.saturating_sub(1);
        if self.is_done_trading(ctx.sim_context) {
            UnitTaskTransition::Goto(UnitTaskTradeState::Leaving)
        } else {
            UnitTaskTransition::Goto(UnitTaskTradeState::FindingStore)
        }
    }

    // Sold everything and full (envoys: given everything), or its partner no longer trades here.
    fn is_done_trading(&self, context: &SimContext) -> bool {
        let sold_out = self.cargo.iter().all(|&count| count == 0);
        (sold_out && (self.room() == 0 || self.gift)) || !trade::is_trading_with(context, &self.partner)
    }

    fn update_leaving(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskTradeState> {
        if !self.reported {
            self.reported = true;
            trade::notify(TradeNotice::caravan(self));
        }
        if ctx.unit.goal().is_none() {
            if ctx.unit.cell() == self.entry {
                return UnitTaskTransition::Despawn(UnitPostDespawnCb::none());
            }
            let start = ctx.unit.cell();
            match ctx.sim_context.find_path(ctx.unit.traversable_node_kinds(), start, self.entry) {
                SearchResult::PathFound(path) => {
                    ctx.unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                    return UnitTaskTransition::Stay;
                }
                SearchResult::PathNotFound => {
                    log::info!(
                        log::channel!("trade"),
                        "A caravan from '{}' has no way out; it leaves where it stands.",
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

impl UnitTaskState for UnitTaskTradeState {
    type Task = UnitTaskTrade;

    fn update(self, task: &mut UnitTaskTrade, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::FindingStore => task.update_finding_store(ctx),
            Self::MovingToStore => task.update_moving_to_store(ctx),
            Self::AtStore => task.update_at_store(ctx),
            Self::Leaving => task.update_leaving(ctx),
        }
    }
}

impl UnitTask for UnitTaskTrade {
    type State = UnitTaskTradeState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        debug_assert!(ctx.unit.goal().is_none());
        ctx.unit.set_traversable_node_kinds(TRADER_NODE_KINDS);
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
            stops_left: u32,
            stores_visited: usize,
            #[debug_ui(debug)]
            state: UnitTaskTradeState,
        }
        View {
            partner: self.partner.clone(),
            entry: self.entry,
            load: self.load(),
            room: self.room(),
            paid: self.paid,
            earned: self.earned,
            stops_left: self.stops_left,
            stores_visited: self.visited.len(),
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
