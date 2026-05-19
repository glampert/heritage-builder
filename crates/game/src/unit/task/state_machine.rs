use std::any::Any;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use common::callback::Callback;
use engine::ui::UiSystem;

use super::{
    UnitTaskArgs,
    UnitTaskId,
    UnitTaskPool,
    despawn::UnitTaskPostDespawnCallback,
};
use crate::{
    sim::{SimCmds, SimContext},
    unit::Unit,
};

// ----------------------------------------------
// UnitTaskContext
// ----------------------------------------------

// Bundle of everything a task touches during one tick.
pub struct UnitTaskContext<'a> {
    pub unit: &'a mut Unit,
    pub sim_cmds: &'a mut SimCmds,
    pub sim_context: &'a SimContext,
}

// ----------------------------------------------
// UnitPostDespawnCb
// ----------------------------------------------

// A despawn callback paired with its extra arguments.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct UnitPostDespawnCb {
    pub callback: Callback<UnitTaskPostDespawnCallback>,
    pub args: UnitTaskArgs,
}

impl UnitPostDespawnCb {
    #[inline]
    pub fn none() -> Self {
        Self { callback: Callback::default(), args: UnitTaskArgs::empty() }
    }
}

// ----------------------------------------------
// UnitTaskTransition
// ----------------------------------------------

// What a state handler returns to the FSM driver.
pub enum UnitTaskTransition<S> {
    // Stay in the current state; run it again next tick.
    Stay,

    // Change state: fires `on_exit(old)` then `on_enter(new)`.
    Goto(S),

    // Task finished; chains to the completion task if the task has one.
    Done,

    // Task finished; despawn the unit.
    Despawn(UnitPostDespawnCb),
}

// ----------------------------------------------
// UnitTaskState
// ----------------------------------------------

// Implemented by each task's own state enum. The `update` impl is the single
// dispatch point - one match arm per state, each delegating to a handler.
pub trait UnitTaskState: Copy + Default + Serialize + DeserializeOwned + 'static {
    type Task: UnitTask<State = Self>;

    // Run the active state for one tick.
    fn update(self, task: &mut Self::Task, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self>;

    // Optional hook run when this state becomes active (via `Goto`).
    fn on_enter(self, _task: &mut Self::Task, _ctx: &mut UnitTaskContext) {}

    // Optional hook run when leaving this state (via `Goto`).
    fn on_exit(self, _task: &mut Self::Task, _ctx: &mut UnitTaskContext) {}
}

// ----------------------------------------------
// UnitTask
// ----------------------------------------------

// Implemented by each concrete task struct. The task owns its `State` field;
// the FSM driver (the blanket `UnitTaskFsm` impl) advances it.
pub trait UnitTask: Sized + 'static {
    type State: UnitTaskState<Task = Self>;

    // One-time setup, run once before the first state update.
    fn initialize(&mut self, _ctx: &mut UnitTaskContext) {}

    // Cleans up any other task handles this task owns, before it is freed.
    fn terminate(&mut self, _pool: &mut UnitTaskPool) {}

    // Type-erased reference to the task as Any.
    fn as_any(&self) -> &dyn Any;

    // Mutable access to the task's current state field.
    fn state(&mut self) -> &mut Self::State;

    // Optional task to run after this one; taken when the task reaches `Done`.
    fn completion_task(&mut self) -> Option<UnitTaskId> { None }

    // Optional post-deserialization fixups (e.g. callback pointers).
    fn post_load(&mut self) {}

    // Optional ImGui debug panel.
    fn draw_debug_ui(&mut self, _unit: &mut Unit, _sim_context: &SimContext, _ui: &UiSystem) {}
}

// ----------------------------------------------
// UnitTaskFlow
// ----------------------------------------------

// The type-erased result of running a task for one tick, consumed by the
// task executor in `UnitTaskManager`.
pub enum UnitTaskFlow {
    Running,
    Completed { next_task: Option<UnitTaskId> },
    Despawn(UnitPostDespawnCb),
}

// ----------------------------------------------
// UnitTaskFsm - Finite State Machine Executor
// ----------------------------------------------

// Type-erased driver trait, implemented for every concrete `UnitTask` by the
// blanket impl below (which drives the FSM). `UnitTaskArchetype` forwards to it
// per variant.
pub trait UnitTaskFsm {
    fn initialize(&mut self, ctx: &mut UnitTaskContext);
    fn terminate(&mut self, pool: &mut UnitTaskPool);
    fn run(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskFlow;
    fn post_load(&mut self);
    fn draw_debug_ui(&mut self, unit: &mut Unit, sim_context: &SimContext, ui: &UiSystem);
    fn as_any(&self) -> &dyn Any;
}

// The whole task executor: read the current state, run it, apply the
// transition (firing exit/enter hooks), report the outcome.
impl<T: UnitTask> UnitTaskFsm for T {
    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        UnitTask::initialize(self, ctx);
    }

    fn terminate(&mut self, pool: &mut UnitTaskPool) {
        UnitTask::terminate(self, pool);
    }

    fn run(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskFlow {
        let state = *self.state();
        match state.update(self, ctx) {
            UnitTaskTransition::Stay => UnitTaskFlow::Running,
            UnitTaskTransition::Goto(next) => {
                state.on_exit(self, ctx);
                *self.state() = next;
                next.on_enter(self, ctx);
                UnitTaskFlow::Running
            }
            UnitTaskTransition::Done => UnitTaskFlow::Completed { next_task: self.completion_task() },
            UnitTaskTransition::Despawn(action) => UnitTaskFlow::Despawn(action),
        }
    }

    fn post_load(&mut self) {
        UnitTask::post_load(self);
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, sim_context: &SimContext, ui: &UiSystem) {
        UnitTask::draw_debug_ui(self, unit, sim_context, ui);
    }

    fn as_any(&self) -> &dyn Any {
        UnitTask::as_any(self)
    }
}
