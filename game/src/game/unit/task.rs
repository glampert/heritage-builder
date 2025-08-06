use slab::Slab;
use smallvec::SmallVec;
use std::any::{Any, TypeId};
use strum_macros::Display;
use enum_dispatch::enum_dispatch;

use crate::{
    debug::{self},
    imgui_ui::UiSystem,
    tile::map::TileMapLayerKind,
    pathfind::{
        Path,
        SearchResult,
        NodeKind as PathNodeKind
    },
    utils::{
        Color,
        UnsafeWeakRef,
        coords::Cell
    },
    game::{
        sim::{
            Query,
            resources::ResourceKind,
            world::GenerationalIndex
        },
        building::{
            Building,
            BuildingKind,
            BuildingKindAndId,
            BuildingTileInfo
        }
    }
};

use super::{
    Unit
};

// ----------------------------------------------
// Helper types
// ----------------------------------------------

pub type UnitTaskId = GenerationalIndex;
pub type UnitTaskCompletionCallback = fn(&mut Building, &mut Unit);

#[derive(Display, PartialEq, Eq)]
pub enum UnitTaskState {
    Uninitialized,
    Running,
    Completed,
    TerminateAndDespawn,
}

#[derive(Display)]
pub enum UnitTaskResult {
    Running,
    Retry,
    Completed { next_task: UnitTaskForwarded }, // Optional next task to run.
    TerminateAndDespawn,
}

pub struct UnitTaskForwarded(Option<UnitTaskId>);

#[inline]
fn forward_task(task: &mut Option<UnitTaskId>) -> UnitTaskForwarded {
    let mut forwarded = None;
    std::mem::swap(&mut forwarded, task);
    UnitTaskForwarded(forwarded)
}

// ----------------------------------------------
// UnitTask
// ----------------------------------------------

#[enum_dispatch(UnitTaskArchetype)]
pub trait UnitTask: Any {
    // Performs one time initialization before the task is first run.
    fn initialize(&mut self, _unit: &mut Unit, _query: &Query) {
    }

    // Cleans up any other task handles this task may have.
    // Called just before the task instance is freed.
    fn terminate(&mut self, _task_pool: &mut UnitTaskPool) {
    }

    // Returns the next state to move to.
    fn update(&mut self, _unit: &mut Unit, _query: &Query) -> UnitTaskState {
        UnitTaskState::Completed
    }

    // Logic to execute once the task is marked as completed.
    // Returns the next task to run when completed or `None` if the task chain is over.
    fn completed(&mut self, _unit: &mut Unit, _query: &Query) -> UnitTaskResult {
        UnitTaskResult::Completed { next_task: UnitTaskForwarded(None) }
    }

    // Task ImGui debug. Optional override.
    fn draw_debug_ui(&self, _query: &Query, _ui_sys: &UiSystem) {
    }

    // TypeId for RTTI.
    fn get_type(&self) -> TypeId {
        self.type_id()
    }
}

// ----------------------------------------------
// UnitTaskArchetype
// ----------------------------------------------

#[enum_dispatch]
#[derive(Display)]
pub enum UnitTaskArchetype {
    UnitTaskDespawn,
    UnitTaskDeliverToStorage,
}

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

pub struct UnitTaskDespawn;

impl UnitTask for UnitTaskDespawn {
    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        let current_task = unit.current_task()
            .expect("Unit should have a despawn task!");

        debug_assert!(query.task_manager().is_task::<UnitTaskDespawn>(current_task),
                      "Unit should have a despawn task!");

        debug_assert!(unit.is_inventory_empty(),
                      "Unit inventory should be empty before despawning!");

        UnitTaskState::TerminateAndDespawn
    }
}

// ----------------------------------------------
// UnitTaskDeliverToStorage
// ----------------------------------------------

// Deliver goods to a storage building.
// Producer -> Storage | Storage -> Storage
pub struct UnitTaskDeliverToStorage {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Resources to deliver:
    pub storage_buildings_accepted: BuildingKind,
    pub resource_kind_to_deliver: ResourceKind,
    pub resource_count: u32,

    // Called on the origin building once resources are delivered.
    // `|unit, origin_building|`
    pub completion_callback: Option<UnitTaskCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Optional fallback task to run if this task is unable to complete.
    // E.g.: Deliver directly to a Producer building instead.
    pub fallback_task: Option<UnitTaskId>,
}

impl UnitTask for UnitTaskDeliverToStorage {
    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        // Sanity check:
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(self.resource_kind_to_deliver.bits().count_ones() == 1);
        debug_assert!(self.resource_count != 0);

        // Give the unit the resources we want to deliver:
        let received_count = unit.receive_resources(self.resource_kind_to_deliver, self.resource_count);
        debug_assert!(received_count == self.resource_count);

        let maybe_path_info = find_path_to_storage(
            query,
            self.origin_building_tile.road_link, // We start at the nearest building road link.
            self.storage_buildings_accepted,
            self.resource_kind_to_deliver);

        match maybe_path_info {
            Some((destination_building_tile, path)) => {
                unit.go_to_building(path, &self.origin_building_tile, &destination_building_tile);
            },
            None => {} // No path or storage building found. Try again later.
        }
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
        if let Some(task_id) = self.fallback_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        // If we have goals we're already moving somewhere, otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            let maybe_path_info = find_path_to_storage(
                query,
                unit.cell(), // If we are retrying this task, take the unit's current cell as the starting point.
                self.storage_buildings_accepted,
                self.resource_kind_to_deliver);

            match maybe_path_info {
                Some((destination_building_tile, path)) => {
                    unit.go_to_building(path, &self.origin_building_tile, &destination_building_tile);
                },
                None => {
                    // Again we couldn't find a storage building to deliver to.
                    // If there's a fallback task, we'll switch to it now, if not
                    // we stay on this task and keep retrying indefinitely.
                    return UnitTaskState::Completed;
                }
            }
        }

        if unit.has_reached_goal() {
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskResult {
        let unit_goal = match unit.goal() {
            Some(goal) => goal,
            None => {
                // If we've reached completion without a goal it means we weren't able
                // to pathfind to a storage building, so we'll switch to the fallback
                // task if there's one. If there's no fallback we'll keep retrying indefinitely.
                if self.fallback_task.is_some() {
                    return UnitTaskResult::Completed { next_task: forward_task(&mut self.fallback_task) };
                } else {
                    return UnitTaskResult::Retry;
                }
            }
        };

        let destination_cell = unit_goal.destination_cell;
        let destination_kind = unit_goal.destination_kind;

        debug_assert!(destination_cell.is_valid());
        debug_assert!(destination_kind.bits().count_ones() == 1);

        let world = query.world();
        let tile_map = query.tile_map();

        // Visit destination building:
        if let Some(destination_building) = world.find_building_for_cell_mut(destination_cell, tile_map) {
            // NOTE: No need to check for generation match here. If the destination building
            // is still the same kind of building we where looking for, it doesn't matter if it
            // was destroyed and recreated since we started the task.
            if destination_building.kind() == destination_kind {
                destination_building.visited_by(unit, query);
            }
        }

        let task_completed = unit.is_inventory_empty();

        // Notify origin building of task completion:
        if task_completed {
            if let Some(on_completion) = self.completion_callback {
                if let Some(origin_building) = world.find_building_mut(self.origin_building.kind, self.origin_building.id) {
                    // NOTE: Only invoke the completion callback if the original base cell still contains the
                    // exact same building that initiated this task. We don't want to accidentally invoke the
                    // callback on a different building, even if the type of building there is the same.
                    debug_assert!(origin_building.kind() == self.origin_building.kind);
                    debug_assert!(origin_building.id()   == self.origin_building.id);
                    on_completion(origin_building, unit);
                }
            }
        }

        unit.follow_path(None);

        // If we've delivered our goods, we're done.
        // Otherwise we were not able to offload everything,
        // so we'll retry with another storage building later.
        if task_completed {
            UnitTaskResult::Completed { next_task: forward_task(&mut self.completion_task) }
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&self, _query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let building_kind = self.origin_building_tile.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building            : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.separator();
        ui.text(format!("Storage Buildings Accepted : {}", self.storage_buildings_accepted));
        ui.text(format!("Resource Kind To Deliver   : {}", self.resource_kind_to_deliver));
        ui.text(format!("Resource Count             : {}", self.resource_count));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_some()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
        ui.text(format!("Has Fallback Task          : {}", self.fallback_task.is_some()));
    }
}

// ----------------------------------------------
// UnitTaskInstance
// ----------------------------------------------

struct UnitTaskInstance {
    id: UnitTaskId,
    state: UnitTaskState,
    archetype: UnitTaskArchetype,
}

impl UnitTaskInstance {
    fn new(id: UnitTaskId, archetype: UnitTaskArchetype) -> Self {
        debug_assert!(id.is_valid());
        Self {
            id,
            state: UnitTaskState::Uninitialized,
            archetype
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskResult {
        debug_assert!(self.state == UnitTaskState::Uninitialized ||
                      self.state == UnitTaskState::Running);

        // First update?
        if self.state == UnitTaskState::Uninitialized {
            self.archetype.initialize(unit, query);
            self.state = UnitTaskState::Running;
        }

        self.state = self.archetype.update(unit, query);

        match self.state {
            UnitTaskState::Running => {
                UnitTaskResult::Running
            },
            UnitTaskState::Completed => {
                // Completed may ask for a retry, in which case we revert back to Running.
                match self.archetype.completed(unit, query) {
                    UnitTaskResult::Retry => {
                        self.state = UnitTaskState::Running;
                        UnitTaskResult::Running
                    },
                    completed @ UnitTaskResult::Completed { .. } => {
                        completed
                    },
                    invalid => {
                        panic!("Invalid task completion result: {}", invalid);
                    }
                }
            },
            UnitTaskState::TerminateAndDespawn => {
                UnitTaskResult::TerminateAndDespawn
            },
            UnitTaskState::Uninitialized => {
                panic!("Invalid task state: Uninitialized");
            }
        }
    }

    fn draw_debug_ui(&self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let status_color = match self.state {
            UnitTaskState::Uninitialized       => Color::yellow(),
            UnitTaskState::Running             => Color::green(),
            UnitTaskState::Completed           => Color::magenta(),
            UnitTaskState::TerminateAndDespawn => Color::red(),
        };

        let archetype_text = format!("Task   : {}", self.archetype);
        let status_text    = format!("Status : {}", self.state);

        ui.text(archetype_text);
        ui.text_colored(status_color.to_array(), status_text);

        ui.separator();

        self.archetype.draw_debug_ui(query, ui_sys);
    }
}

// ----------------------------------------------
// UnitTaskPool
// ----------------------------------------------

pub struct UnitTaskPool {
    tasks: Slab<UnitTaskInstance>,
    generation: u32,
}

impl UnitTaskPool {
    fn new(capacity: usize) -> Self {
        Self {
            tasks: Slab::with_capacity(capacity),
            generation: 0,
        }
    }

    fn allocate(&mut self, archetype: UnitTaskArchetype) -> UnitTaskId {
        let generation = self.generation;
        self.generation += 1;

        let id = UnitTaskId::new(generation, self.tasks.vacant_key());
        let index = self.tasks.insert(UnitTaskInstance::new(id, archetype));

        debug_assert!(id == self.tasks[index].id);
        id
    }

    fn free(&mut self, task_id: UnitTaskId) {
        if !task_id.is_valid() {
            return;
        }

        let index = task_id.index();

        // Handle feeing an invalid handle gracefully.
        // This will also avoid any invalid frees thanks to the generation check.
        match self.tasks.get(index) {
            Some(task) => {
                if task.id != task_id {
                    return; // Slot reused, not same item.
                }

                // Borrow checker hack so we can pass self to terminate()...
                let weak_ref = UnsafeWeakRef::new(task);
                let mut_task = weak_ref.mut_ref_cast();
                mut_task.archetype.terminate(self);
            },
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

        self.tasks.get(task_id.index())
            .filter(|task| task.id == task_id)
    }

    fn try_get_mut(&mut self, task_id: UnitTaskId) -> Option<&mut UnitTaskInstance> {
        if !task_id.is_valid() {
            return None;
        }

        self.tasks.get_mut(task_id.index())
            .filter(|task| task.id == task_id)
    }
}

// Detect any leaked task instances.
impl Drop for UnitTaskPool {
    fn drop(&mut self) {
        if self.tasks.is_empty() {
            return;
        }

        eprintln!("-----------------------");
        eprintln!("    TASK POOL LEAKS    ");
        eprintln!("-----------------------");

        for (index, task) in &self.tasks {
            eprintln!("Leaked Task[{index}]: {}", task.archetype);
        }

        if cfg!(debug_assertions) {
            panic!("UnitTaskAllocator dropped with {} remaining tasks (generation: {}).",
                   self.tasks.len(), self.generation);
        } else {
            eprintln!("UnitTaskAllocator dropped with {} remaining tasks (generation: {}).",
                      self.tasks.len(), self.generation);
        }
    }
}

// ----------------------------------------------
// UnitTaskManager
// ----------------------------------------------

pub struct UnitTaskManager {
    task_pool: UnitTaskPool,    
}

impl UnitTaskManager {
    pub fn new() -> Self {
        Self {
            task_pool: UnitTaskPool::new(64),
        }
    }

    #[inline]
    pub fn new_task<Task>(&mut self, task: Task) -> Option<UnitTaskId>
        where
            Task: UnitTask,
            UnitTaskArchetype: From<Task>
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
            Task: UnitTask + 'static
    {
        let task = match self.task_pool.try_get(task_id) {
            Some(task) => task,
            None => return false,
        };

        task.archetype.get_type() == TypeId::of::<Task>()
    }

    pub fn run_unit_tasks(&mut self, unit: &mut Unit, query: &Query) {
        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get_mut(current_task_id) {
                match task.update(unit, query) {
                    UnitTaskResult::Running => {
                        // Stay on current task and run it again next update.
                    },
                    UnitTaskResult::Completed { next_task } => {
                        unit.assign_task(self, next_task.0);
                    },
                    UnitTaskResult::TerminateAndDespawn => {
                        unit.assign_task(self, None);
                        query.despawn_unit(unit);
                    },
                    invalid => {
                        panic!("Invalid task completion result: {}", invalid);
                    }
                }
            } else if cfg!(debug_assertions) {
                panic!("Unit '{}' current TaskId is invalid: {}", unit.name(), current_task_id);
            }
        }
    }

    pub fn draw_tasks_debug_ui(&self, unit: &Unit, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get(current_task_id) {
                task.draw_debug_ui(query, ui_sys);
            } else if cfg!(debug_assertions) {
                panic!("Unit '{}' current TaskId is invalid: {}", unit.name(), current_task_id);
            }
        } else {
            ui.text("<no task>");
        }
    }
}

// ----------------------------------------------
// Path finding helpers:
// ----------------------------------------------

fn find_path_to_storage<'search>(query: &'search Query,
                                 origin: Cell,
                                 storage_buildings_accepted: BuildingKind,
                                 resource_kind_to_deliver: ResourceKind) -> Option<(BuildingTileInfo, &'search Path)> {

    debug_assert!(origin.is_valid());
    debug_assert!(!storage_buildings_accepted.is_empty());
    debug_assert!(resource_kind_to_deliver.bits().count_ones() == 1); // Only one resource kind at a time.

    struct StorageInfo {
        kind: BuildingKind,
        road_link: Cell,
        base_cell: Cell,
        distance: i32,
        slots_available: u32,
    }

    const MAX_CANDIDATES: usize = 4;
    let mut storage_candidates: SmallVec<[StorageInfo; MAX_CANDIDATES]> = SmallVec::new();

    // Try to find storage buildings that can accept our delivery.
    query.for_each_storage_building(storage_buildings_accepted, |storage| {
        let slots_available = storage.receivable_amount(resource_kind_to_deliver);
        if slots_available != 0 {
            if let Some(storage_road_link) = query.find_nearest_road_link(storage.cell_range()) {
                storage_candidates.push(StorageInfo {
                    kind: storage.kind(),
                    road_link: storage_road_link,
                    base_cell: storage.base_cell(),
                    distance: origin.manhattan_distance(storage_road_link),
                    slots_available,
                });
                if storage_candidates.len() == MAX_CANDIDATES {
                    // We've collected enough candidate storage buildings, stop the search.
                    return false;
                }
            }
        }
        // Else we couldn't find a single free slot in this storage, try again with another one.
        true
    });

    if storage_candidates.is_empty() {
        // Couldn't find any suitable storage building.
        return None;
    }

    // Sort by closest storage buildings first. Tie breaker is the number of slots available, highest first.
    storage_candidates.sort_by_key(|storage| {
        (storage.distance, std::cmp::Reverse(storage.slots_available))
    });

    // Find a road path to a storage building. Try our best candidates first.
    for storage in &storage_candidates {
        match query.find_path(PathNodeKind::Road, origin, storage.road_link) {
            SearchResult::PathFound(path) => {
                let destination = BuildingTileInfo {
                    kind: storage.kind,
                    road_link: storage.road_link,
                    base_cell: storage.base_cell,
                };
                return Some((destination, path));
            },
            SearchResult::PathNotFound => {
                // Building is not reachable (lacks road access?).
                // Try another candidate.
                continue;
            },
        }
    }

    None
}
