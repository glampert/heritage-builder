use std::any::Any;
use serde::{Deserialize, Serialize};

use common::coords::Cell;

use super::{
    UnitPostDespawnCb,
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskArg,
};
use crate::{
    sim::{SimCmds, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
};

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskDespawnState {
    #[default]
    Despawning,
}

#[derive(Default, Serialize, Deserialize)]
pub struct UnitTaskDespawn {
    pub state: UnitTaskDespawnState,
}

impl UnitTaskState for UnitTaskDespawnState {
    type Task = UnitTaskDespawn;

    fn update(self, _task: &mut UnitTaskDespawn, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        debug_assert_unit_ready_to_despawn(ctx.unit);
        UnitTaskTransition::Despawn(UnitPostDespawnCb::none())
    }
}

impl UnitTask for UnitTaskDespawn {
    type State = UnitTaskDespawnState;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }
}

// ----------------------------------------------
// UnitTaskDespawnWithCallback
// ----------------------------------------------

// Callback invoked *after* the unit has despawned.
// |cmds, context, unit_prev_cell, unit_prev_goal, extra_args|
pub type UnitTaskPostDespawnCallback = fn(&mut SimCmds, &SimContext, Cell, Option<UnitNavGoal>, &[UnitTaskArg]);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskDespawnWithCallbackState {
    #[default]
    Despawning,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskDespawnWithCallback {
    // Callback + extra args invoked once the unit has despawned.
    pub post_despawn: UnitPostDespawnCb,

    pub state: UnitTaskDespawnWithCallbackState,
}

impl UnitTaskState for UnitTaskDespawnWithCallbackState {
    type Task = UnitTaskDespawnWithCallback;

    fn update(self, task: &mut UnitTaskDespawnWithCallback, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        debug_assert_unit_ready_to_despawn(ctx.unit);
        UnitTaskTransition::Despawn(task.post_despawn)
    }
}

impl UnitTask for UnitTaskDespawnWithCallback {
    type State = UnitTaskDespawnWithCallbackState;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }

    fn post_load(&mut self) {
        self.post_despawn.callback.post_load();
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn debug_assert_unit_ready_to_despawn(unit: &Unit) {
    debug_assert!(
        unit.inventory_is_empty(),
        "Unit inventory should be empty before despawning! Contains {}",
        unit.peek_inventory().unwrap()
    );
}
