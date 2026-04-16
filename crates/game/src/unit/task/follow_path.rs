use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::ui::UiSystem;

use super::{
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    UnitTaskResult,
    UnitTaskState,
};
use crate::{
    pathfind::Path,
    sim::{SimCmds, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
};

// ----------------------------------------------
// UnitTaskFollowPath
// ----------------------------------------------

pub type UnitTaskFollowPathCompletionCallback = fn(&mut Unit, &SimContext);

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
}

impl UnitTask for UnitTaskFollowPath {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn initialize(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, _context: &SimContext) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(!self.path.is_empty());

        unit.move_to_goal(&self.path, UnitNavGoal::tile(unit.cell(), &self.path));
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, _context: &SimContext) -> UnitTaskState {
        if unit.has_reached_goal() || (unit.path_is_blocked() && self.terminate_if_stuck) {
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        if !unit.path_is_blocked() {
            unit.goal().expect("Expected unit to have an active goal!");
            debug_assert!(unit.cell() == self.path.last().unwrap().cell, "Unit has not reached its goal yet!");
        }

        if let Some(completion_callback) = self.completion_callback.try_get() {
            completion_callback(unit, context);
        }

        unit.follow_path(None);

        UnitTaskResult::completed_with(&mut self.completion_task)
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let start = self.path.first().unwrap().cell;
        let end   = self.path.last().unwrap().cell;

        ui.text(format!("Path Start/End          : {start},{end}"));
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));
    }
}
