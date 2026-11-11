use std::any::Any;
use serde::{Deserialize, Serialize};

use common::{callback::Callback, coords::Cell};
use engine::{log, ui::{DrawDebugUi, UiSystem}};
use proc_macros::DrawDebugUi;

use super::{
    UnitTaskOriginBuildingDebug,
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    invoke_completion_callback_deferred,
    with_task,
};
use crate::{
    building::{Building, BuildingKindAndId, BuildingTileInfo, config::BuildingConfigs, risks::RiskKind},
    pathfind::{Node, NodeKind as PathNodeKind, Path, SearchResult},
    sim::{SimCmdQueue, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
    world::object::GameObject,
};

// ----------------------------------------------
// Emergency
// ----------------------------------------------

// What a responder is sent to: a fire, by a fire watch, or an outbreak, by a hospital
// (docs/plans/city-risks.md, §3 and §6).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Emergency {
    #[default]
    Fire,
    Outbreak,
}

impl Emergency {
    // The risk the responder's visit resets (and the fire or outbreak it ends).
    #[inline]
    pub fn risk_kind(self) -> RiskKind {
        match self {
            Self::Fire => RiskKind::Fire,
            Self::Outbreak => RiskKind::Disease,
        }
    }

    // Whether `building` has this emergency now: it burns, or it's sick.
    #[inline]
    pub fn is_at(self, building: &Building) -> bool {
        match self {
            Self::Fire => building.is_burning(),
            Self::Outbreak => building.has_outbreak(),
        }
    }

    // For logs: "fire" or "outbreak", and who answers it.
    #[inline]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Outbreak => "outbreak",
        }
    }

    #[inline]
    pub fn responder_name(self) -> &'static str {
        match self {
            Self::Fire => "fire watcher",
            Self::Outbreak => "medic",
        }
    }
}

// ----------------------------------------------
// UnitTaskRespond
// ----------------------------------------------

// A walker sent straight to an emergency (docs/plans/city-risks.md): a fire watcher from a fire
// watch to a fire (§3), a medic from a hospital to an outbreak (§6). He walks over any land to
// the building, road or not, makes his visit (which puts the fire out, or cures the sickness),
// and goes on to the nearest other such emergency within his building's radius of where he
// stands, until none is left; then he goes home. Building::visited_by knows him by this task
// (building::risks::risk_reset_by).

pub type UnitTaskRespondCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

// The ground a responder crosses on a call: any land, on the road or off it, but not water, and
// not through buildings, rubble, rocks or trees.
pub const RESPONDER_NODE_KINDS: PathNodeKind = PathNodeKind::from_bits_retain(
    PathNodeKind::EmptyLand.bits()
        | PathNodeKind::Road.bits()
        | PathNodeKind::VacantLot.bits()
        | PathNodeKind::SettlersSpawnPoint.bits(),
);

// The nearest building with `emergency` within `radius` cells of `start`, other than `except`,
// that a responder can walk up to, and the way to the cell beside it where he'd stand.
pub fn find_nearest_emergency<'ctx>(
    context: &'ctx SimContext,
    emergency: Emergency,
    start: Cell,
    radius: i32,
    except: Option<BuildingKindAndId>,
) -> Option<(&'ctx Building, &'ctx Path)> {
    let mut found = None;

    let result = context.find_path_next_to(RESPONDER_NODE_KINDS, start, Some(radius), |cell| {
        if !context.graph().node_kind(Node::new(cell)).is_some_and(PathNodeKind::is_building) {
            return false;
        }
        let target = context
            .find_building_for_cell(cell)
            .filter(|building| emergency.is_at(building) && Some(building.kind_and_id()) != except);
        if target.is_some() {
            found = target; // The search stops here.
        }
        target.is_some()
    });

    match result {
        SearchResult::PathFound(path) => found.map(|building| (building, path)),
        SearchResult::PathNotFound => None,
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskRespondState {
    // Walking to a cell next to the building.
    #[default]
    MovingToTarget,

    // Walking back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    ReportingToOrigin,

    // Terminal state.
    Done,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskRespond {
    // What he answers: fires or outbreaks.
    pub emergency: Emergency,

    // Origin building info: the fire watch or the hospital.
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // The building the walker is on his way to. He stands on whichever cell next to it he
    // reaches first, so its road link (if it has one) doesn't matter.
    pub target: BuildingKindAndId,
    pub target_tile: BuildingTileInfo,

    // Called on the origin building once the walker is back.
    // `|context, origin_building, responder|`
    pub completion_callback: Callback<UnitTaskRespondCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    pub state: UnitTaskRespondState,

    // Set by the deferred completion callback; consumed by `ReportingToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,
}

impl UnitTaskRespond {
    // The emergency is still where it was when the walker set out: the building burns, or is sick.
    fn target_needs_him(&self, sim_context: &SimContext) -> bool {
        sim_context
            .find_building(self.target.kind, self.target.id)
            .is_some_and(|building| self.emergency.is_at(building))
    }

    // How far from where he stands a responder looks for the next emergency: his building's reach.
    fn search_radius(&self) -> i32 {
        BuildingConfigs::get().find_service_config(self.origin_building.kind).effect_radius
    }

    // Heads for `target`, along `path`, which ends next to it.
    fn walk_to(&mut self, ctx: &mut UnitTaskContext, target: &Building, path: &Path) {
        self.target = target.kind_and_id();
        self.target_tile = target.tile_info();

        // The goal is where the path ends, the cell he'll stand on, in place of a road link.
        let stand_on = BuildingTileInfo { road_link: path.last().unwrap().cell, base_cell: self.target_tile.base_cell };
        let goal =
            UnitNavGoal::building(self.origin_building.kind, self.origin_building_tile.base_cell, self.target.kind, stand_on);
        ctx.unit.move_to_goal(path, goal);
    }

    // Sets out for the target, or finds another way after the path was blocked.
    fn try_route_to_target(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;
        let Some(target) = sim_context.find_building(self.target.kind, self.target.id) else {
            return false;
        };
        let footprint = target.cell_range();

        match sim_context.find_path_next_to(RESPONDER_NODE_KINDS, ctx.unit.cell(), None, |cell| footprint.contains(cell)) {
            SearchResult::PathFound(path) => {
                self.walk_to(ctx, target, path);
                true
            }
            SearchResult::PathNotFound => {
                log::info!(
                    log::channel!("task"),
                    "TaskRespond: no way to the {} at {}.",
                    self.emergency.name(),
                    self.target_tile.base_cell
                );
                false
            }
        }
    }

    // Sets out for the nearest emergency of his kind within reach of where he stands but
    // `except`, the one he's just seen to (its visit hasn't run yet, so it still needs him).
    fn try_route_to_next_target(&mut self, ctx: &mut UnitTaskContext, except: Option<BuildingKindAndId>) -> bool {
        let sim_context = ctx.sim_context;
        match find_nearest_emergency(sim_context, self.emergency, ctx.unit.cell(), self.search_radius(), except) {
            Some((target, path)) => {
                log::info!(
                    log::channel!("risks"),
                    "A {} goes on to the {} at {}.",
                    self.emergency.responder_name(),
                    self.emergency.name(),
                    target.base_cell()
                );
                self.walk_to(ctx, target, path);
                true
            }
            None => false,
        }
    }

    // On to the next emergency, or home if there's none.
    fn next_target_or_home(
        &mut self,
        ctx: &mut UnitTaskContext,
        except: Option<BuildingKindAndId>,
    ) -> UnitTaskTransition<UnitTaskRespondState> {
        if self.try_route_to_next_target(ctx, except) {
            UnitTaskTransition::Stay
        } else {
            UnitTaskTransition::Goto(UnitTaskRespondState::ReturningToOrigin)
        }
    }

    // Queues the visit that puts the fire out or cures the sickness (Building::visited_by). It
    // runs with this step's commands, before a burning down queued in the same step.
    fn make_visit(&mut self, ctx: &mut UnitTaskContext) {
        if self.target_needs_him(ctx.sim_context) {
            ctx.sim_cmds.visit_building(self.target, ctx.unit.id());
        }
    }

    fn try_return_to_origin(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;

        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::info!(log::channel!("task"), "TaskRespond: his building is gone; the walker leaves.");
            return false;
        }

        let start = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        match sim_context.find_path(traversable_node_kinds, start, self.origin_building_tile.road_link) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                ctx.unit.move_to_goal(path, goal);
                true
            }
            SearchResult::PathNotFound => {
                log::info!(log::channel!("task"), "TaskRespond: no way back to his building; the walker leaves.");
                false
            }
        }
    }

    fn schedule_completion_callback(&mut self, ctx: &mut UnitTaskContext) -> bool {
        invoke_completion_callback_deferred(
            ctx.unit,
            ctx.sim_cmds,
            ctx.sim_context,
            self.origin_building.kind,
            self.origin_building.id,
            self.completion_callback.get(),
            |context, _building, unit| {
                with_task::<UnitTaskRespond>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_moving_to_target(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskRespondState> {
        if ctx.unit.goal().is_none() {
            // Set out, or find another way after the path was blocked. A fire that's out or an
            // outbreak that's over (or one that can't be reached) needs no one, but there may be
            // another.
            if self.target_needs_him(ctx.sim_context) && self.try_route_to_target(ctx) {
                return UnitTaskTransition::Stay;
            }
            return self.next_target_or_home(ctx, Some(self.target));
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // A path that's just the cell he stands on (he's next to it already) gets here too.
        ctx.unit.follow_path(None);
        self.make_visit(ctx);
        self.next_target_or_home(ctx, Some(self.target))
    }

    fn update_returning(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskRespondState> {
        if ctx.unit.goal().is_none() {
            if ctx.unit.cell() == self.origin_building_tile.road_link {
                return self.report_to_origin(ctx);
            }
            if self.try_return_to_origin(ctx) {
                return UnitTaskTransition::Stay;
            }
            return UnitTaskTransition::Goto(UnitTaskRespondState::Done);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        ctx.unit.follow_path(None);
        self.report_to_origin(ctx)
    }

    fn report_to_origin(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskRespondState> {
        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            UnitTaskTransition::Goto(UnitTaskRespondState::ReportingToOrigin)
        } else {
            // No completion callback, or his building is gone.
            UnitTaskTransition::Goto(UnitTaskRespondState::Done)
        }
    }

    fn update_reporting(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskRespondState> {
        if self.completion_callback_done {
            UnitTaskTransition::Goto(UnitTaskRespondState::Done)
        } else {
            UnitTaskTransition::Stay
        }
    }
}

impl UnitTaskState for UnitTaskRespondState {
    type Task = UnitTaskRespond;

    fn update(self, task: &mut UnitTaskRespond, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::MovingToTarget    => task.update_moving_to_target(ctx),
            Self::ReturningToOrigin => task.update_returning(ctx),
            Self::ReportingToOrigin => task.update_reporting(ctx),
            Self::Done              => UnitTaskTransition::Done,
        }
    }
}

impl UnitTask for UnitTaskRespond {
    type State = UnitTaskRespondState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert_eq!(ctx.unit.cell(), self.origin_building_tile.road_link); // We start at his building's road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.target.is_valid());

        // On a call, responders go off-road, there and back.
        let current_node_kinds = ctx.unit.traversable_node_kinds();
        ctx.unit.set_traversable_node_kinds(current_node_kinds | RESPONDER_NODE_KINDS);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }

    fn completion_task(&mut self) -> Option<UnitTaskId> {
        self.completion_task.take()
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, sim_context: &SimContext, ui_sys: &UiSystem) {
        #[derive(DrawDebugUi)]
        struct View {
            #[debug_ui(debug)]
            emergency: Emergency,
            origin_building: UnitTaskOriginBuildingDebug,
            #[debug_ui(label = "Target")]
            target: UnitTaskOriginBuildingDebug,
            target_needs_him: bool,
            #[debug_ui(debug)]
            state: UnitTaskRespondState,
            has_completion_callback: bool,
            has_completion_task: bool,
        }
        View {
            emergency: self.emergency,
            origin_building: UnitTaskOriginBuildingDebug::new(self.origin_building, self.origin_building_tile),
            target: UnitTaskOriginBuildingDebug::new(self.target, self.target_tile),
            target_needs_him: self.target_needs_him(sim_context),
            state: self.state,
            has_completion_callback: self.completion_callback.is_valid(),
            has_completion_task: self.completion_task.is_some(),
        }
        .draw_debug_ui(ui_sys);
    }
}
