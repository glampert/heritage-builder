use std::any::Any;
use serde::{Deserialize, Serialize};

use common::{callback::Callback, coords::Cell};

use super::{UnitTask, UnitTaskArg, UnitTaskArgs, UnitTaskState};
use crate::{
    sim::{SimCmds, SimContext},
    unit::{Unit, navigation::UnitNavGoal},
};

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct UnitTaskDespawn;

impl UnitTask for UnitTaskDespawn {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        check_unit_despawn_state::<UnitTaskDespawn>(unit, context);
        UnitTaskState::TerminateAndDespawn {
            post_despawn_callback: Callback::default(),
            callback_extra_args: UnitTaskArgs::empty(),
        }
    }
}

// ----------------------------------------------
// UnitTaskDespawnWithCallback
// ----------------------------------------------

pub type UnitTaskPostDespawnCallback = fn(&mut SimCmds, &SimContext, Cell, Option<UnitNavGoal>, &[UnitTaskArg]);

#[derive(Serialize, Deserialize)]
pub struct UnitTaskDespawnWithCallback {
    // Callback invoked *after* the unit has despawned.
    // |cmds, context, unit_prev_cell, unit_prev_goal, extra_args|
    pub post_despawn_callback: Callback<UnitTaskPostDespawnCallback>,

    // Extra arguments for the callback.
    pub callback_extra_args: UnitTaskArgs,
}

impl UnitTask for UnitTaskDespawnWithCallback {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.post_despawn_callback.post_load();
    }

    fn update(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        check_unit_despawn_state::<UnitTaskDespawnWithCallback>(unit, context);
        UnitTaskState::TerminateAndDespawn {
            post_despawn_callback: self.post_despawn_callback,
            callback_extra_args: self.callback_extra_args,
        }
    }
}

fn check_unit_despawn_state<Task>(unit: &Unit, context: &SimContext)
where
    Task: UnitTask + 'static,
{
    let current_task = unit.current_task().expect("Unit should have a despawn task!");
    debug_assert!(context.task_manager().is_task::<Task>(current_task), "Unit should have a despawn task!");

    debug_assert!(
        unit.inventory_is_empty(),
        "Unit inventory should be empty before despawning! Contains {}",
        unit.peek_inventory().unwrap()
    );
}
