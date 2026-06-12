use slab::Slab;
use serde::{Deserialize, Serialize};

use common::mem;
use engine::log;

use super::{
    UnitTaskContext,
    UnitTaskFlow,
    UnitTask,
    UnitTaskArchetype,
    UnitTaskId,
};
use crate::{
    constants::*,
    unit::Unit,
    sim::{SimCmds, SimCmdQueue, SimContext},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskInstance
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct UnitTaskInstance {
    id: UnitTaskId,

    // False until `initialize()` has run.
    started: bool,

    archetype: UnitTaskArchetype,
}

impl UnitTaskInstance {
    fn new(id: UnitTaskId, archetype: UnitTaskArchetype) -> Self {
        debug_assert!(id.is_valid());
        Self { id, started: false, archetype }
    }

    fn run(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskFlow {
        let mut ctx = UnitTaskContext { unit, sim_cmds: cmds, sim_context: context };

        // First run? Perform one-time initialization before the first state update.
        if !self.started {
            self.archetype.initialize(&mut ctx);
            self.started = true;
        }

        self.archetype.run(&mut ctx)
    }

    fn post_load(&mut self) {
        self.archetype.post_load();
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

impl Default for UnitTaskPool {
    fn default() -> Self {
        Self { tasks: Slab::default(), generation: INITIAL_GENERATION }
    }
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

        debug_assert_eq!(id, self.tasks[index].id);
        id
    }

    pub(super) fn free(&mut self, task_id: UnitTaskId) {
        if !task_id.is_valid() {
            return;
        }

        let index = task_id.index();

        // Handle freeing an invalid handle gracefully (already free or slot reused).
        // The generation check on `task.id` prevents double-frees of stale handles.
        if !matches!(self.tasks.get(index), Some(task) if task.id == task_id) {
            return;
        }

        // Remove before terminate: `terminate()` may recursively call `self.free()`
        // on a completion task, so the entry we are freeing must already be out of
        // the slab to avoid aliasing through `&mut self`.
        let mut task = self.tasks
            .try_remove(index)
            .expect("Task slot was just verified to be occupied!");

        task.archetype.terminate(self);
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
            log::error!("Leaked Task[{index}]: {}, {}", task.archetype, task.id);
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

#[derive(Default, Serialize, Deserialize)]
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
    pub fn try_get_task_archetype_and_started(&self, task_id: UnitTaskId) -> Option<(&UnitTaskArchetype, bool)> {
        let task = self.task_pool.try_get(task_id)?;
        Some((&task.archetype, task.started))
    }

    // Mutable counterpart of `try_get_task_archetype_and_started`, used by the
    // debug UI (see `crate::debug::unit`) to render the task's own panel.
    #[inline]
    pub(crate) fn try_get_task_archetype_and_started_mut(
        &mut self,
        task_id: UnitTaskId,
    ) -> Option<(&mut UnitTaskArchetype, bool)> {
        let task = self.task_pool.try_get_mut(task_id)?;
        Some((&mut task.archetype, task.started))
    }

    pub fn run_unit_tasks(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) {
        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get_mut(current_task_id) {
                match task.run(unit, cmds, context) {
                    UnitTaskFlow::Running => {
                        // Stay on current task and run it again next update.
                    }
                    UnitTaskFlow::Completed { next_task } => {
                        unit.assign_task(self, next_task);
                    }
                    UnitTaskFlow::Despawn(post_despawn) => {
                        let unit_prev_cell = unit.cell();
                        let unit_prev_goal = unit.goal().cloned();

                        unit.assign_task(self, None);

                        // Push deferred despawn command. Completes after world update.
                        cmds.despawn_unit_with_id(unit.id());

                        if post_despawn.callback.is_valid() {
                            let callback = post_despawn.callback.get();
                            callback(cmds, context, unit_prev_cell, unit_prev_goal, post_despawn.args.as_slice());
                        }
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
}
