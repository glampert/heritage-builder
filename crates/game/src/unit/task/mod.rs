use std::any::Any;
use strum::Display;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use engine::ui::UiSystem;

use crate::{
    sim::{SimCmds, SimContext},
    unit::Unit,
};

mod common;
mod despawn;
mod deliver;
mod fetch;
mod follow_path;
mod harvest;
mod manager;
mod patrol;
mod settler;

pub use common::*;
pub use despawn::*;
pub use deliver::*;
pub use fetch::*;
pub use follow_path::*;
pub use harvest::*;
pub use manager::*;
pub use patrol::*;
pub use settler::*;

// ----------------------------------------------
// UnitTask
// ----------------------------------------------

#[enum_dispatch(UnitTaskArchetype)]
pub trait UnitTask: Any {
    fn as_any(&self) -> &dyn Any;

    // Optional post-deserialization pointer fixups for the task callbacks.
    fn post_load(&mut self) {}

    // Performs one time initialization before the task is first run.
    fn initialize(&mut self, _unit: &mut Unit, _cmds: &mut SimCmds, _context: &SimContext) {}

    // Cleans up any other task handles this task may have.
    // Called just before the task instance is freed.
    fn terminate(&mut self, _task_pool: &mut UnitTaskPool) {}

    // Returns the next state to move to.
    fn update(&mut self, _unit: &mut Unit, _cmds: &mut SimCmds, _context: &SimContext) -> UnitTaskState {
        UnitTaskState::Completed
    }

    // Logic to execute once the task is marked as completed.
    // Returns the next task to run when completed or `None` if the task chain is over.
    fn completed(&mut self, _unit: &mut Unit, _cmds: &mut SimCmds, _context: &SimContext) -> UnitTaskResult {
        UnitTaskResult::Completed { next_task: UnitTaskForwarded(None) }
    }

    // Task ImGui debug. Optional override.
    fn draw_debug_ui(&mut self, _unit: &mut Unit, _context: &SimContext, _ui_sys: &UiSystem) {}
}

// ----------------------------------------------
// UnitTaskArchetype
// ----------------------------------------------

#[enum_dispatch]
#[derive(Display, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum UnitTaskArchetype {
    UnitTaskDespawn,
    UnitTaskDespawnWithCallback,
    UnitTaskRandomizedPatrol,
    UnitTaskDeliverToStorage,
    UnitTaskFetchFromStorage,
    UnitTaskSettler,
    UnitTaskHarvestWood,
    UnitTaskFollowPath,
}
