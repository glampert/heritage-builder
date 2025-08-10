use std::any::Any;
use slab::Slab;
use smallvec::SmallVec;
use strum_macros::Display;
use enum_dispatch::enum_dispatch;

use crate::{
    debug::{self},
    imgui_ui::UiSystem,
    tile::TileMapLayerKind,
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
            world::{GenerationalIndex, BuildingId},
            resources::{
                self,
                ShoppingList,
                ResourceKind
            }
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
    Unit,
    navigation::UnitNavGoal
};

// ----------------------------------------------
// Helper types
// ----------------------------------------------

pub type UnitTaskId = GenerationalIndex;
pub type UnitTaskCompletionCallback = fn(&mut Building, &mut Unit, &Query);

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
    fn as_any(&self) -> &dyn Any;

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
    fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
    }
}

// ----------------------------------------------
// UnitTaskArchetype
// ----------------------------------------------

#[enum_dispatch]
#[derive(Display)]
#[allow(clippy::enum_variant_names)]
pub enum UnitTaskArchetype {
    UnitTaskDespawn,
    UnitTaskPatrol,
    UnitTaskDeliverToStorage,
    UnitTaskFetchFromStorage,
}

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

pub struct UnitTaskDespawn;

impl UnitTask for UnitTaskDespawn {
    fn as_any(&self) -> &dyn Any { self }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        let current_task = unit.current_task()
            .expect("Unit should have a despawn task!");

        debug_assert!(query.task_manager().is_task::<UnitTaskDespawn>(current_task),
                      "Unit should have a despawn task!");

        debug_assert!(unit.inventory_is_empty(),
                      "Unit inventory should be empty before despawning!");

        UnitTaskState::TerminateAndDespawn
    }
}

// ----------------------------------------------
// UnitTaskPatrol
// ----------------------------------------------

pub struct UnitTaskPatrol;

// - Unit walks up to a certain distance away from the origin building.
// - Once max distance reached, start walking back to origin.
// - Visit any buildings it is interested on along the way.
impl UnitTask for UnitTaskPatrol {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, _unit: &mut Unit, _query: &Query) {
        // TODO
    }

    fn terminate(&mut self, _task_pool: &mut UnitTaskPool) {
        // TODO
    }

    fn update(&mut self, _unit: &mut Unit, _query: &Query) -> UnitTaskState {
        // TODO
        UnitTaskState::Completed
    }

    fn completed(&mut self, _unit: &mut Unit, _query: &Query) -> UnitTaskResult {
        // TODO
        UnitTaskResult::Completed { next_task: UnitTaskForwarded(None) }
    }

    fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
        // TODO
    }
}

// ----------------------------------------------
// UnitTaskDeliverToStorage
// ----------------------------------------------

// Deliver goods to a storage building.
// Producer -> Storage | Storage -> Storage | Producer -> Producer (fallback)
pub struct UnitTaskDeliverToStorage {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Resources to deliver:
    pub storage_buildings_accepted: BuildingKind,
    pub resource_kind_to_deliver: ResourceKind,
    pub resource_count: u32,

    // Called on the origin building once resources are delivered.
    // `|origin_building, runner_unit, query|`
    pub completion_callback: Option<UnitTaskCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Optional fallback if we are not able to deliver to a Storage.
    // E.g.: Deliver directly to a Producer building instead.
    pub allow_producer_fallback: bool,
}

impl UnitTaskDeliverToStorage {
    fn try_find_goal(&self, unit: &mut Unit, query: &Query) {
        let origin_kind = self.origin_building.kind;
        let origin_base_cell = unit.cell();

        // Prefer delivering to a storage building.
        let mut path_find_result = find_delivery_candidate(query,
                                                           origin_kind,
                                                           origin_base_cell,
                                                           self.storage_buildings_accepted,
                                                           self.resource_kind_to_deliver);

        if path_find_result.not_found() && self.allow_producer_fallback {
            // Find any producer that can take our resources as fallback.
            path_find_result = find_delivery_candidate(query,
                                                       origin_kind,
                                                       origin_base_cell,
                                                       BuildingKind::producers(),
                                                       self.resource_kind_to_deliver);
        }

        if let PathFindResult::Success { path, goal } = path_find_result {
            unit.go_to_building(path, goal);
        }
        // Else no path or Storage/Producer building found. Try again later.
    }
}

impl UnitTask for UnitTaskDeliverToStorage {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(self.resource_kind_to_deliver.bits().count_ones() == 1);
        debug_assert!(self.resource_count != 0);

        // Give the unit the resources we want to deliver:
        let received_count = unit.receive_resources(self.resource_kind_to_deliver, self.resource_count);
        debug_assert!(received_count == self.resource_count);

        self.try_find_goal(unit, query);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        // If we have goals we're already moving somewhere,
        // otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            self.try_find_goal(unit, query);
        }

        if unit.has_reached_goal() {
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskResult {
        visit_destination(unit, query);
        unit.follow_path(None);

        // If we've delivered our goods, we're done. Otherwise we were not able
        // to offload everything, so we'll retry with another building later.
        if unit.inventory_is_empty() {
            invoke_completion_callback(unit,
                                       query,
                                       self.origin_building.kind,
                                       self.origin_building.id,
                                       self.completion_callback);

            UnitTaskResult::Completed {
                next_task: forward_task(&mut self.completion_task)
            }
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let building_kind = self.origin_building.kind;
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
        ui.text(format!("Allow Producer Fallback    : {}", self.allow_producer_fallback));
    }
}

// ----------------------------------------------
// UnitTaskFetchFromStorage
// ----------------------------------------------

// Fetch goods from a storage building.
// Storage -> Producer | Storage -> Storage
pub struct UnitTaskFetchFromStorage {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Resources to fetch:
    pub storage_buildings_accepted: BuildingKind,
    pub resources_to_fetch: ShoppingList, // Will fetch at most *one* of these. This is a list of desired options.

    // Called on the origin building once resources are delivered.
    // `|origin_building, runner_unit, query|`
    pub completion_callback: Option<UnitTaskCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,
}

impl UnitTaskFetchFromStorage {
    fn try_find_goal(&self, unit: &mut Unit, query: &Query) {
        for resource_to_fetch in &self.resources_to_fetch {
            debug_assert!(resource_to_fetch.kind.bits().count_ones() == 1);

            let path_find_result = find_storage_fetch_candidate(query,
                                                                self.origin_building.kind,
                                                                unit.cell(),
                                                                self.storage_buildings_accepted,
                                                                resource_to_fetch.kind);

            if let PathFindResult::Success { path, goal } = path_find_result {
                unit.go_to_building(path, goal);
                break;
            }
            // Else no path or Storage building found. Try again.
        }
    }

    fn try_return_to_origin(&self, unit: &mut Unit, query: &Query) -> bool {
        if query.world().find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            eprintln!("Origin building is no longer valid! TaskFetchFromStorage will abort.");
            return false;
        }

        let start = unit.cell();
        let goal  = self.origin_building_tile.road_link;

        match query.find_path(PathNodeKind::Road, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal {
                    origin_kind: self.origin_building.kind,
                    origin_base_cell: self.origin_building_tile.base_cell,
                    destination_kind: self.origin_building.kind,
                    destination_base_cell: self.origin_building_tile.base_cell,
                    destination_road_link: self.origin_building_tile.road_link,
                };
                unit.go_to_building(path, goal);
                true
            },
            SearchResult::PathNotFound => {
                eprintln!("Origin building is no longer reachable! (no road access?) TaskFetchFromStorage will abort.");
                false
            },
        }
    }

    fn is_returning_to_origin(&self, unit_goal: &UnitNavGoal) -> bool {
        unit_goal.destination_base_cell == self.origin_building_tile.base_cell &&
        unit_goal.destination_kind == self.origin_building.kind
    }
}

impl UnitTask for UnitTaskFetchFromStorage {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.inventory_is_empty());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(!self.resources_to_fetch.is_empty());

        self.try_find_goal(unit, query);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        // If we have goals we're already moving somewhere,
        // otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            self.try_find_goal(unit, query);
        }

        if unit.has_reached_goal() {
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskResult {
        let unit_goal = unit.goal().expect("Expected unit to have an active goal!");
        let mut task_completed = false;

        if self.is_returning_to_origin(unit_goal) {
            // We've reached our origin building with the resources we were supposed to fetch.
            // Invoke the completion callback and end the task.
            debug_assert!(!unit.inventory_is_empty());
            debug_assert!(self.resources_to_fetch.iter().any(|entry| entry.kind == unit.peek_inventory().unwrap().kind));
            invoke_completion_callback(unit,
                                       query,
                                       self.origin_building.kind,
                                       self.origin_building.id,
                                       self.completion_callback);
            task_completed = true;
            unit.follow_path(None);
        } else {
            // We've reached a destination to visit and attempt to fetch some resources.
            // We may fail and try again with another building or start returning to the origin.
            visit_destination(unit, query);
            unit.follow_path(None);

            // If we've collected resources from the visited destination
            // we are done and can return to our origin building.
            if let Some(item) = unit.peek_inventory() {
                debug_assert!(item.count != 0);
                debug_assert!(self.resources_to_fetch.iter().any(|entry| entry.kind == item.kind));

                if !self.try_return_to_origin(unit, query) {
                    // If we couldn't find a path back to the origin, maybe because the origin building
                    // was destroyed, we'll have to abort the task. Any resources collected will be lost.
                    eprintln!("Aborting TaskFetchFromStorage. Unable to return to origin building...");

                    // TODO: We can recover from this and ship the resources back to storage.
                    todo!("Switch to a UnitTaskDeliverToStorage and return the resources");
                }
            }
        }

        if task_completed {
            UnitTaskResult::Completed {
                next_task: forward_task(&mut self.completion_task)
            }
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building            : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.separator();
        ui.text(format!("Storage Buildings Accepted : {}", self.storage_buildings_accepted));
        ui.text(format!("Resources To Fetch         : {}", resources::shopping_list_debug_string(&self.resources_to_fetch)));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_some()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
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

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
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

        self.archetype.draw_debug_ui(ui_sys);
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
                weak_ref.mut_ref_cast().archetype.terminate(self);
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
            eprintln!("Leaked Task[{index}]: {}, {}, {}", task.archetype, task.id, task.state);
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
        task.archetype.as_any().is::<Task>()
    }

    #[inline]
    pub fn try_get_task<Task>(&self, task_id: UnitTaskId) -> Option<&Task>
        where
            Task: UnitTask + 'static
    {
        let task = self.task_pool.try_get(task_id)?;
        task.archetype.as_any().downcast_ref::<Task>()
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

    pub fn draw_tasks_debug_ui(&self, unit: &Unit, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get(current_task_id) {
                task.draw_debug_ui(ui_sys);
            } else if cfg!(debug_assertions) {
                panic!("Unit '{}' current TaskId is invalid: {}", unit.name(), current_task_id);
            }
        } else {
            ui.text("<no task>");
        }
    }
}

// ----------------------------------------------
// Task helpers:
// ----------------------------------------------

fn visit_destination(unit: &mut Unit, query: &Query) {
    let unit_goal = unit.goal().expect("Expected unit to have an active goal!");

    let destination_cell = unit_goal.destination_base_cell;
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
}

fn invoke_completion_callback(unit: &mut Unit,
                              query: &Query,
                              origin_building_kind: BuildingKind,
                              origin_building_id: BuildingId,
                              completion_callback: Option<UnitTaskCompletionCallback>) {
    if let Some(on_completion) = completion_callback {
        if let Some(origin_building) = query.world().find_building_mut(origin_building_kind, origin_building_id) {
            // NOTE: Only invoke the completion callback if the original base cell still contains the
            // exact same building that initiated this task. We don't want to accidentally invoke the
            // callback on a different building, even if the type of building there is the same.
            debug_assert!(origin_building.kind() == origin_building_kind);
            debug_assert!(origin_building.id()   == origin_building_id);
            on_completion(origin_building, unit, query);
        }
    }
}

// ----------------------------------------------
// Path finding helpers:
// ----------------------------------------------

enum PathFindResult<'search> {
    Success {
        path: &'search Path,
        goal: UnitNavGoal,
    },
    NotFound,
}

impl PathFindResult<'_> {
    fn not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
}

fn find_delivery_candidate<'search>(query: &'search Query,
                                    origin_kind: BuildingKind,
                                    origin_base_cell: Cell,
                                    building_kinds_accepted: BuildingKind,
                                    resource_kind_to_deliver: ResourceKind) -> PathFindResult<'search> {

    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!building_kinds_accepted.is_empty());
    debug_assert!(resource_kind_to_deliver.bits().count_ones() == 1); // Only one resource kind at a time.

    struct DeliveryCandidate {
        kind: BuildingKind,
        road_link: Cell,
        base_cell: Cell,
        distance: i32,
        receivable_resources: u32,
    }

    const MAX_CANDIDATES: usize = 4;
    let mut candidates: SmallVec<[DeliveryCandidate; MAX_CANDIDATES]> = SmallVec::new();

    // Try to find buildings that can accept our delivery.
    query.for_each_building(building_kinds_accepted, |building| {
        let receivable_resources = building.receivable_resources(resource_kind_to_deliver);
        if receivable_resources != 0 {
            if let Some(road_link) = query.find_nearest_road_link(building.cell_range()) {
                candidates.push(DeliveryCandidate {
                    kind: building.kind(),
                    road_link,
                    base_cell: building.base_cell(),
                    distance: origin_base_cell.manhattan_distance(road_link),
                    receivable_resources,
                });
                if candidates.len() == MAX_CANDIDATES {
                    // We've collected enough candidate buildings, stop the search now.
                    return false;
                }
            }
        }
        // Else we couldn't find a single free slot in this building, try again with another one.
        true
    });

    if candidates.is_empty() {
        // Couldn't find any suitable building.
        return PathFindResult::NotFound;
    }

    // Sort by closest buildings first. Tie breaker is the number of storage slots available, highest first.
    candidates.sort_by_key(|candidate| {
        (candidate.distance, std::cmp::Reverse(candidate.receivable_resources))
    });

    // Find a road path to the building. Try our best candidates first.
    for candidate in &candidates {
        match query.find_path(PathNodeKind::Road, origin_base_cell, candidate.road_link) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal {
                    origin_kind,
                    origin_base_cell,
                    destination_kind: candidate.kind,
                    destination_base_cell: candidate.base_cell,
                    destination_road_link: candidate.road_link,
                };
                return PathFindResult::Success { path, goal };
            },
            SearchResult::PathNotFound => {
                // Building is not reachable (lacks road access?).
                // Try another candidate.
                continue;
            },
        }
    }

    PathFindResult::NotFound
}

fn find_storage_fetch_candidate<'search>(query: &'search Query,
                                         origin_kind: BuildingKind,
                                         origin_base_cell: Cell,
                                         storage_buildings_accepted: BuildingKind,
                                         resource_kind_to_fetch: ResourceKind) -> PathFindResult<'search> {

    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!storage_buildings_accepted.is_empty());
    debug_assert!(resource_kind_to_fetch.bits().count_ones() == 1); // Only one resource kind at a time.

    struct StorageCandidate {
        kind: BuildingKind,
        road_link: Cell,
        base_cell: Cell,
        distance: i32,
        available_resources: u32,
    }

    const MAX_CANDIDATES: usize = 4;
    let mut candidates: SmallVec<[StorageCandidate; MAX_CANDIDATES]> = SmallVec::new();

    // Try to find storage buildings that can accept our delivery.
    query.for_each_building(storage_buildings_accepted, |building| {
        let available_resources = building.available_resources(resource_kind_to_fetch);
        if available_resources != 0 {
            if let Some(road_link) = query.find_nearest_road_link(building.cell_range()) {
                candidates.push(StorageCandidate {
                    kind: building.kind(),
                    road_link,
                    base_cell: building.base_cell(),
                    distance: origin_base_cell.manhattan_distance(road_link),
                    available_resources,
                });
                if candidates.len() == MAX_CANDIDATES {
                    // We've collected enough candidate buildings, stop the search now.
                    return false;
                }
            }
        }
        // Else we couldn't find the resource we're looking for in this building, try another one.
        true
    });

    if candidates.is_empty() {
        // Couldn't find any suitable building.
        return PathFindResult::NotFound;
    }

    // Sort by closest buildings first. Tie breaker is the number of resources available, highest first.
    candidates.sort_by_key(|candidate| {
        (candidate.distance, std::cmp::Reverse(candidate.available_resources))
    });

    // Find a road path to the building. Try our best candidates first.
    for candidate in &candidates {
        match query.find_path(PathNodeKind::Road, origin_base_cell, candidate.road_link) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal {
                    origin_kind,
                    origin_base_cell,
                    destination_kind: candidate.kind,
                    destination_base_cell: candidate.base_cell,
                    destination_road_link: candidate.road_link,
                };
                return PathFindResult::Success { path, goal };
            },
            SearchResult::PathNotFound => {
                // Building is not reachable (lacks road access?).
                // Try another candidate.
                continue;
            },
        }
    }

    PathFindResult::NotFound
}
