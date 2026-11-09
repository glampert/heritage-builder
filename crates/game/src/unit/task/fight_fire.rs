use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
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
    building::{Building, BuildingKindAndId, BuildingTileInfo},
    pathfind::SearchResult,
    sim::{SimCmdQueue, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskFightFire
// ----------------------------------------------

// A fire watcher a fire watch sends straight to a fire (docs/plans/city-risks.md, §3): it walks
// the road to the burning building, puts the fire out, and goes home. Building::visited_by knows
// it by this task (building::risks::risk_reset_by).

pub type UnitTaskFightFireCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskFightFireState {
    // Walking to the burning building's road link.
    #[default]
    MovingToFire,

    // Walking back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    ReportingToOrigin,

    // Terminal state.
    Done,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskFightFire {
    // Origin building info: the fire watch.
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // The burning building, and its road link, where the walker stands to fight the fire.
    pub fire: BuildingKindAndId,
    pub fire_tile: BuildingTileInfo,

    // Called on the origin building once the walker is back.
    // `|context, origin_building, fire_watcher|`
    pub completion_callback: Callback<UnitTaskFightFireCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    pub state: UnitTaskFightFireState,

    // Set by the deferred completion callback; consumed by `ReportingToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,
}

impl UnitTaskFightFire {
    // The fire is still burning where it was when the walker set out.
    fn fire_is_burning(&self, sim_context: &SimContext) -> bool {
        sim_context.find_building(self.fire.kind, self.fire.id).is_some_and(Building::is_burning)
    }

    fn try_route_to_fire(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let start = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        match ctx.sim_context.find_path(traversable_node_kinds, start, self.fire_tile.road_link) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.fire.kind,
                    self.fire_tile,
                );
                ctx.unit.move_to_goal(path, goal);
                true
            }
            SearchResult::PathNotFound => {
                log::info!(log::channel!("task"), "TaskFightFire: no road to the fire at {}; going home.", self.fire_tile.base_cell);
                false
            }
        }
    }

    // Queues the visit that puts the fire out (Building::visited_by). It runs with this step's
    // commands, before a burning down queued in the same step.
    fn fight_fire(&mut self, ctx: &mut UnitTaskContext) {
        if self.fire_is_burning(ctx.sim_context) {
            ctx.sim_cmds.visit_building(self.fire, ctx.unit.id());
        }
    }

    fn try_return_to_origin(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;

        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::info!(log::channel!("task"), "TaskFightFire: the fire watch is gone; the walker leaves.");
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
                log::info!(log::channel!("task"), "TaskFightFire: no road back to the fire watch; the walker leaves.");
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
                with_task::<UnitTaskFightFire>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_moving_to_fire(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if ctx.unit.goal().is_none() {
            // Next to the fire already: the fire watch and the fire share a road link.
            if ctx.unit.cell() == self.fire_tile.road_link {
                self.fight_fire(ctx);
                return UnitTaskTransition::Goto(UnitTaskFightFireState::ReturningToOrigin);
            }
            // Set out, or find another way after the road changed. A fire that's out needs no one.
            if self.fire_is_burning(ctx.sim_context) && self.try_route_to_fire(ctx) {
                return UnitTaskTransition::Stay;
            }
            return UnitTaskTransition::Goto(UnitTaskFightFireState::ReturningToOrigin);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        ctx.unit.follow_path(None);
        self.fight_fire(ctx);
        UnitTaskTransition::Goto(UnitTaskFightFireState::ReturningToOrigin)
    }

    fn update_returning(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if ctx.unit.goal().is_none() {
            if ctx.unit.cell() == self.origin_building_tile.road_link {
                return self.report_to_origin(ctx);
            }
            if self.try_return_to_origin(ctx) {
                return UnitTaskTransition::Stay;
            }
            return UnitTaskTransition::Goto(UnitTaskFightFireState::Done);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        ctx.unit.follow_path(None);
        self.report_to_origin(ctx)
    }

    fn report_to_origin(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            UnitTaskTransition::Goto(UnitTaskFightFireState::ReportingToOrigin)
        } else {
            // No completion callback, or the fire watch is gone.
            UnitTaskTransition::Goto(UnitTaskFightFireState::Done)
        }
    }

    fn update_reporting(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskFightFireState> {
        if self.completion_callback_done {
            UnitTaskTransition::Goto(UnitTaskFightFireState::Done)
        } else {
            UnitTaskTransition::Stay
        }
    }
}

impl UnitTaskState for UnitTaskFightFireState {
    type Task = UnitTaskFightFire;

    fn update(self, task: &mut UnitTaskFightFire, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::MovingToFire      => task.update_moving_to_fire(ctx),
            Self::ReturningToOrigin => task.update_returning(ctx),
            Self::ReportingToOrigin => task.update_reporting(ctx),
            Self::Done              => UnitTaskTransition::Done,
        }
    }
}

impl UnitTask for UnitTaskFightFire {
    type State = UnitTaskFightFireState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert_eq!(ctx.unit.cell(), self.origin_building_tile.road_link); // We start at the fire watch's road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.fire.is_valid());
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
            origin_building: UnitTaskOriginBuildingDebug,
            #[debug_ui(label = "Fire")]
            fire: UnitTaskOriginBuildingDebug,
            fire_is_burning: bool,
            #[debug_ui(debug)]
            state: UnitTaskFightFireState,
            has_completion_callback: bool,
            has_completion_task: bool,
        }
        View {
            origin_building: UnitTaskOriginBuildingDebug::new(self.origin_building, self.origin_building_tile),
            fire: UnitTaskOriginBuildingDebug::new(self.fire, self.fire_tile),
            fire_is_burning: self.fire_is_burning(sim_context),
            state: self.state,
            has_completion_callback: self.completion_callback.is_valid(),
            has_completion_task: self.completion_task.is_some(),
        }
        .draw_debug_ui(ui_sys);
    }
}
