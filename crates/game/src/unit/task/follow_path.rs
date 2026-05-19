use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::ui::UiSystem;

use super::{
    TaskContext,
    TaskState,
    Transition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
};
use crate::{
    pathfind::Path,
    sim::SimContext,
    unit::{Unit, navigation::UnitNavGoal},
};

// ----------------------------------------------
// UnitTaskFollowPath
// ----------------------------------------------

pub type UnitTaskFollowPathCompletionCallback = fn(&mut Unit, &SimContext);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskFollowPathState {
    // Following the path; completes once the goal is reached (or the unit gets
    // stuck, if `terminate_if_stuck` is set).
    #[default]
    Following,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskFollowPath {
    // Follow this path from start to finish, once.
    pub path: Path,

    // Optional task completion callback.
    pub completion_callback: Callback<UnitTaskFollowPathCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // If the unit gets stuck, terminate the task and run the completion callback/task.
    #[serde(default)]
    pub terminate_if_stuck: bool,

    #[serde(default)]
    pub state: UnitTaskFollowPathState,
}

impl TaskState for UnitTaskFollowPathState {
    type Task = UnitTaskFollowPath;

    fn update(self, task: &mut UnitTaskFollowPath, ctx: &mut TaskContext) -> Transition<Self> {
        let reached_goal = ctx.unit.has_reached_goal();
        let stuck = ctx.unit.path_is_blocked() && task.terminate_if_stuck;

        if !reached_goal && !stuck {
            return Transition::Stay;
        }

        if !ctx.unit.path_is_blocked() {
            ctx.unit.goal().expect("Expected unit to have an active goal!");
            debug_assert!(
                ctx.unit.cell() == task.path.last().unwrap().cell,
                "Unit has not reached its goal yet!"
            );
        }

        if let Some(completion_callback) = task.completion_callback.try_get() {
            completion_callback(ctx.unit, ctx.sim_context);
        }

        ctx.unit.follow_path(None);

        Transition::Done
    }
}

impl UnitTask for UnitTaskFollowPath {
    type State = UnitTaskFollowPathState;

    fn initialize(&mut self, ctx: &mut TaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert!(!self.path.is_empty());

        let start = ctx.unit.cell();
        ctx.unit.move_to_goal(&self.path, UnitNavGoal::tile(start, &self.path));
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

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _sim_context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let start = self.path.first().unwrap().cell;
        let end   = self.path.last().unwrap().cell;

        ui.text(format!("Path Start/End          : {start},{end}"));
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));
    }
}
