use slab::Slab;
use serde::{Deserialize, Serialize};

use common::{Color, mem::{self, RawPtr}};
use engine::{log, ui::UiSystem};

use super::{
    UnitTask,
    UnitTaskArchetype,
    UnitTaskArg,
    UnitTaskId,
    UnitTaskResult,
    UnitTaskState,
};
use crate::{
    constants::*,
    unit::Unit,
    sim::{SimCmds, SimContext},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskInstance
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct UnitTaskInstance {
    id: UnitTaskId,
    state: UnitTaskState,
    archetype: UnitTaskArchetype,
}

impl UnitTaskInstance {
    fn new(id: UnitTaskId, archetype: UnitTaskArchetype) -> Self {
        debug_assert!(id.is_valid());
        Self { id, state: UnitTaskState::Uninitialized, archetype }
    }

    fn update(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        debug_assert!(matches!(self.state, UnitTaskState::Uninitialized | UnitTaskState::Running));

        // First update?
        if matches!(self.state, UnitTaskState::Uninitialized) {
            self.archetype.initialize(unit, cmds, context);
            self.state = UnitTaskState::Running;
        }

        self.state = self.archetype.update(unit, cmds, context);

        match self.state {
            UnitTaskState::Running => UnitTaskResult::Running,
            UnitTaskState::Completed => {
                // Completed may ask for a retry, in which case we revert back to Running.
                match self.archetype.completed(unit, cmds, context) {
                    UnitTaskResult::Retry => {
                        self.state = UnitTaskState::Running;
                        UnitTaskResult::Running
                    }
                    completed @ UnitTaskResult::Completed { .. } => completed,
                    invalid => {
                        panic!("Invalid task completion result: {}", invalid);
                    }
                }
            }
            UnitTaskState::TerminateAndDespawn { post_despawn_callback, callback_extra_args } => {
                UnitTaskResult::TerminateAndDespawn { post_despawn_callback, callback_extra_args }
            }
            UnitTaskState::Uninitialized => {
                panic!("Invalid task state: Uninitialized");
            }
        }
    }

    fn post_load(&mut self) {
        self.archetype.post_load();
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let status_color = match self.state {
            UnitTaskState::Uninitialized => Color::yellow(),
            UnitTaskState::Running => Color::green(),
            UnitTaskState::Completed => Color::magenta(),
            UnitTaskState::TerminateAndDespawn { .. } => Color::red(),
        };

        let archetype_text = format!("Task   : {}", self.archetype);
        let status_text    = format!("Status : {}", self.state);

        ui.text(archetype_text);
        ui.text_colored(status_color.to_array(), status_text);

        ui.separator();

        self.archetype.draw_debug_ui(unit, context, ui_sys);
    }
}

// ----------------------------------------------
// UnitTaskPool
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct UnitTaskPool {
    tasks: Slab<UnitTaskInstance>,
    generation: u32,
}

impl UnitTaskPool {
    fn new(capacity: usize) -> Self {
        Self { tasks: Slab::with_capacity(capacity), generation: INITIAL_GENERATION }
    }

    fn allocate(&mut self, archetype: UnitTaskArchetype) -> UnitTaskId {
        let generation = self.generation;
        self.generation += 1;

        let id = UnitTaskId::new(generation, self.tasks.vacant_key());
        let index = self.tasks.insert(UnitTaskInstance::new(id, archetype));

        debug_assert!(id == self.tasks[index].id);
        id
    }

    pub(super) fn free(&mut self, task_id: UnitTaskId) {
        if !task_id.is_valid() {
            return;
        }

        let index = task_id.index();

        // Handle freeing an invalid handle gracefully.
        // This will also avoid any invalid frees thanks to the generation check.
        match self.tasks.get(index) {
            Some(task) => {
                if task.id != task_id {
                    return; // Slot reused, not same item.
                }

                // HACK: Borrow checker bypass so we can pass self to terminate()...
                let task_ptr = RawPtr::from_ref(task);
                task_ptr.mut_ref_cast().archetype.terminate(self);
            }
            None => return, // Already free.
        }

        if self.tasks.try_remove(index).is_none() {
            panic!("Failed to free task slot [{index}]!");
        }
    }

    fn try_get(&self, task_id: UnitTaskId) -> Option<&UnitTaskInstance> {
        if !task_id.is_valid() {
            return None;
        }

        self.tasks.get(task_id.index()).filter(|task| task.id == task_id)
    }

    fn try_get_mut(&mut self, task_id: UnitTaskId) -> Option<&mut UnitTaskInstance> {
        if !task_id.is_valid() {
            return None;
        }

        self.tasks.get_mut(task_id.index()).filter(|task| task.id == task_id)
    }

    fn pre_load(&mut self) {
        self.tasks.clear();
        self.generation = RESERVED_GENERATION;
    }

    fn post_load(&mut self) {
        debug_assert!(self.generation != RESERVED_GENERATION);

        for (index, task) in &mut self.tasks {
            debug_assert!(task.id.is_valid());
            debug_assert!(task.id.index() == index);
            debug_assert!(task.id.generation() != RESERVED_GENERATION);
            debug_assert!(task.id.generation() < self.generation);

            task.post_load();
        }
    }

    fn debug_leak_check(&self) {
        if self.tasks.is_empty() {
            return;
        }

        log::error!("-----------------------");
        log::error!("    TASK POOL LEAKS    ");
        log::error!("-----------------------");

        for (index, task) in &self.tasks {
            log::error!("Leaked Task[{index}]: {}, {}, {}", task.archetype, task.id, task.state);
        }

        if cfg!(debug_assertions) {
            panic!("UnitTaskPool dropped with {} remaining tasks (generation: {}).", self.tasks.len(), self.generation);
        } else {
            log::error!(
                "UnitTaskPool dropped with {} remaining tasks (generation: {}).",
                self.tasks.len(),
                self.generation
            );
        }
    }
}

// Detect any leaked task instances.
impl Drop for UnitTaskPool {
    fn drop(&mut self) {
        self.debug_leak_check();
    }
}

// ----------------------------------------------
// UnitTaskManager
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct UnitTaskManager {
    task_pool: UnitTaskPool,
}

impl UnitTaskManager {
    pub fn new(pool_capacity: usize) -> Self {
        Self { task_pool: UnitTaskPool::new(pool_capacity) }
    }

    #[inline]
    pub fn new_task<Task>(&mut self, task: Task) -> Option<UnitTaskId>
    where
        Task: UnitTask,
        UnitTaskArchetype: From<Task>,
    {
        Some(self.task_pool.allocate(UnitTaskArchetype::from(task)))
    }

    #[inline]
    pub fn free_task(&mut self, task_id: UnitTaskId) {
        self.task_pool.free(task_id);
    }

    #[inline]
    pub fn is_task<Task>(&self, task_id: UnitTaskId) -> bool
    where
        Task: UnitTask + 'static,
    {
        let task = match self.task_pool.try_get(task_id) {
            Some(task) => task,
            None => return false,
        };
        task.archetype.as_any().is::<Task>()
    }

    #[inline]
    pub fn try_get_task<Task>(&self, task_id: UnitTaskId) -> Option<&Task>
    where
        Task: UnitTask + 'static,
    {
        let task = self.task_pool.try_get(task_id)?;
        task.archetype.as_any().downcast_ref::<Task>()
    }

    #[inline]
    pub fn try_get_task_mut<Task>(&mut self, task_id: UnitTaskId) -> Option<&mut Task>
    where
        Task: UnitTask + 'static,
    {
        let task = self.task_pool.try_get_mut(task_id)?;
        // NOTE: Reuse the non-mutable as_any() interface for convenience.
        mem::mut_ref_cast(task.archetype.as_any()).downcast_mut::<Task>()
    }

    #[inline]
    pub fn try_get_task_archetype_and_state(&self, task_id: UnitTaskId) -> Option<(&UnitTaskArchetype, &UnitTaskState)> {
        let task = self.task_pool.try_get(task_id)?;
        Some((&task.archetype, &task.state))
    }

    pub fn run_unit_tasks(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) {
        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get_mut(current_task_id) {
                match task.update(unit, cmds, context) {
                    UnitTaskResult::Running => {
                        // Stay on current task and run it again next update.
                    }
                    UnitTaskResult::Completed { next_task } => {
                        unit.assign_task(self, next_task.0);
                    }
                    UnitTaskResult::TerminateAndDespawn { post_despawn_callback, callback_extra_args } => {
                        let unit_prev_cell = unit.cell();
                        let unit_prev_goal = unit.goal().cloned();

                        unit.assign_task(self, None);

                        // Push deferred despawn command. Completes after world update.
                        cmds.despawn_unit_with_id(unit.id());

                        if post_despawn_callback.is_valid() {
                            let callback = post_despawn_callback.get();

                            let args: &[UnitTaskArg] = callback_extra_args.args
                                .as_ref()
                                .map(|arr| &arr[..])
                                .unwrap_or(&[]);

                            callback(cmds, context, unit_prev_cell, unit_prev_goal, args);
                        }
                    }
                    invalid => {
                        panic!("Invalid task completion result: {}", invalid);
                    }
                }
            } else if cfg!(debug_assertions) {
                panic!("Unit '{}' current TaskId is invalid: {}", unit.name(), current_task_id);
            }
        }
    }

    pub fn pre_load(&mut self) {
        self.task_pool.pre_load();
    }

    pub fn post_load(&mut self) {
        self.task_pool.post_load();
    }

    pub fn draw_tasks_debug_ui(&mut self, unit: &mut Unit, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if !ui.collapsing_header("Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get_mut(current_task_id) {
                task.draw_debug_ui(unit, context, ui_sys);
            } else if cfg!(debug_assertions) {
                panic!("Unit '{}' current TaskId is invalid: {}", unit.name(), current_task_id);
            }
        } else {
            ui.text("<no task>");
        }
    }
}
