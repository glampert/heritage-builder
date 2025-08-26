#![allow(clippy::enum_variant_names)]

use std::any::Any;
use slab::Slab;
use rand::{seq::SliceRandom, Rng};
use strum_macros::Display;
use enum_dispatch::enum_dispatch;

use crate::{
    debug::{self},
    imgui_ui::UiSystem,
    tile::{Tile, TileKind, TileMapLayerKind},
    pathfind::{
        Node,
        Path,
        PathHistory,
        PathFilter,
        SearchResult,
        NodeKind as PathNodeKind,
        RandomDirectionalBias
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
    navigation::{self, UnitNavGoal, UnitDirection}
};

// ----------------------------------------------
// Helper types
// ----------------------------------------------

pub type UnitTaskId = GenerationalIndex;

#[derive(Display, PartialEq, Eq)]
pub enum UnitTaskState {
    Uninitialized,
    Running,
    Completed,
    TerminateAndDespawn { post_despawn_callback: Option<fn(&Query, Cell, Option<UnitNavGoal>)> },
}

#[derive(Display)]
pub enum UnitTaskResult {
    Running,
    Retry,
    Completed { next_task: UnitTaskForwarded }, // Optional next task to run.
    TerminateAndDespawn { post_despawn_callback: Option<fn(&Query, Cell, Option<UnitNavGoal>)> },
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
    fn draw_debug_ui(&mut self, _unit: &mut Unit, _query: &Query, _ui_sys: &UiSystem) {
    }
}

// ----------------------------------------------
// UnitTaskArchetype
// ----------------------------------------------

#[enum_dispatch]
#[derive(Display)]
pub enum UnitTaskArchetype {
    UnitTaskDespawn,
    UnitTaskDespawnWithCallback,
    UnitTaskRandomizedPatrol,
    UnitTaskDeliverToStorage,
    UnitTaskFetchFromStorage,
    UnitTaskFindVacantHouseLot,
}

// ----------------------------------------------
// UnitTaskDespawn
// ----------------------------------------------

pub struct UnitTaskDespawn;

impl UnitTask for UnitTaskDespawn {
    fn as_any(&self) -> &dyn Any { self }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        check_unit_despawn_state::<UnitTaskDespawn>(unit, query);
        UnitTaskState::TerminateAndDespawn { post_despawn_callback: None }
    }
}

// ----------------------------------------------
// UnitTaskDespawnWithCallback
// ----------------------------------------------

pub struct UnitTaskDespawnWithCallback {
    // Callback invoked *after* the unit has despawned.
    // |query, unit_prev_cell, unit_prev_goal|
    pub callback: Option<fn(&Query, Cell, Option<UnitNavGoal>)>,
}

impl UnitTask for UnitTaskDespawnWithCallback {
    fn as_any(&self) -> &dyn Any { self }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        check_unit_despawn_state::<UnitTaskDespawnWithCallback>(unit, query);
        UnitTaskState::TerminateAndDespawn { post_despawn_callback: self.callback }
    }
}

fn check_unit_despawn_state<Task>(unit: &Unit, query: &Query)
    where Task: UnitTask + 'static
{
    let current_task = unit.current_task()
        .expect("Unit should have a despawn task!");

    debug_assert!(query.task_manager().is_task::<Task>(current_task),
                  "Unit should have a despawn task!");

    debug_assert!(unit.inventory_is_empty(),
                  "Unit inventory should be empty before despawning!");
}

// ----------------------------------------------
// UnitPatrolPathRecord
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct UnitPatrolPathRecord {
    history: PathHistory,
    current_length: u32,
    current_direction: UnitDirection,
    repeated_axis_count: i32,
}

impl UnitPatrolPathRecord {
    fn update(&mut self, path: &Path) {
        self.history.add_path(path);

        let prev_direction = self.current_direction;
        let new_direction  = path_direction(path);

        if navigation::same_axis(new_direction, prev_direction) {
            self.repeated_axis_count += 1;
        } else {
            self.repeated_axis_count = 0;
        }

        self.current_length = path.len() as u32;
        self.current_direction = new_direction;
    }

    pub fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Previous Path Hashes    : {}", self.history));
        ui.text(format!("Current Path Length     : {}", self.current_length));
        ui.text(format!("Current Path Direction  : {}", self.current_direction));
        ui.text(format!("Path History Same Axis  : {}", self.repeated_axis_count));
    }
}

fn path_direction(path: &Path) -> UnitDirection {
    let start = path.first().unwrap();
    let goal  = path.last().unwrap();
    navigation::direction_between(start.cell, goal.cell)
}

// ----------------------------------------------
// UnitPatrolWaypointFilter
// ----------------------------------------------

const PATROL_MIN_PREFERRED_PATH_LEN: i32 = 4;
const PATROL_MAX_REPEATED_DIR_AXIS:  i32 = 2;

struct UnitPatrolWaypointFilter<'task, R: Rng> {
    rng: &'task mut R,
    path_record: &'task UnitPatrolPathRecord,
    preferred_fallback_path_index: Option<usize>,
}

impl<'task, R: Rng> UnitPatrolWaypointFilter<'task, R> {
    fn new(rng: &'task mut R, path_record: &'task UnitPatrolPathRecord) -> Self {
        Self {
            rng,
            path_record,
            preferred_fallback_path_index: None
        }
    }
}

impl<R: Rng> PathFilter for UnitPatrolWaypointFilter<'_, R> {
    const TAKE_FALLBACK_PATH: bool = true;

    fn accepts(&mut self, index: usize, path: &Path, _goal: Node) -> bool {
        if self.path_record.history.has_path(path) {
            // We've taken this path recently, reject it.
            return false; 
        }

        let prev_direction = self.path_record.current_direction;
        let new_direction  = path_direction(path);

        // Try picking a different direction.
        if new_direction == prev_direction {
            return false;
        }

        // Remember a "good enough" fallback path. Not taken recently and different
        // direction, but might still be in the same axis. If we can't find a better
        // alternative we'll take this one as fallback.
        if self.preferred_fallback_path_index.is_none() {
            self.preferred_fallback_path_index = Some(index);
        }

        // If we've taken the same axis (N/S/E/W) multiple times in a row,
        // try finding a new path on an opposite axis.
        if navigation::same_axis(new_direction, prev_direction) &&
            self.path_record.repeated_axis_count >= PATROL_MAX_REPEATED_DIR_AXIS {
            return false;
        }

        // Avoid very short paths, use the fallback path instead.
        if path.len() <= (PATROL_MIN_PREFERRED_PATH_LEN as usize) {
            return false;
        }

        true // Accept path.
    }

    #[inline]
    fn shuffle(&mut self, nodes: &mut [Node]) {
        nodes.shuffle(self.rng);
    }

    #[inline]
    fn choose_fallback(&mut self, nodes: &[Node]) -> Option<Node> {
        if let Some(index) = self.preferred_fallback_path_index {
            return Some(nodes[index]);
        }

        // Secondary fallback if we never found a preferred fallback path.
        if !nodes.is_empty() {
            return Some(nodes[0]);
        }

        None
    }
}

// ----------------------------------------------
// UnitPatrolReturnPathFilter
// ----------------------------------------------

struct UnitPatrolReturnPathFilter<'task> {
    path_record: &'task UnitPatrolPathRecord,
}

impl<'task> UnitPatrolReturnPathFilter<'task> {
    fn new(path_record: &'task UnitPatrolPathRecord) -> Self {
        Self { path_record }
    }
}

impl PathFilter for UnitPatrolReturnPathFilter<'_> {
    const TAKE_FALLBACK_PATH: bool = true;

    #[inline]
    fn accepts(&mut self, _index: usize, path: &Path, _goal: Node) -> bool {
        let path_hash = PathHistory::hash_path_reverse(path);

        if self.path_record.history.is_last_path_hash(path_hash) {
            // This is the same as the last path we've taken to get here,
            // try a different path to return home.
            return false; 
        }

        true // Accept path.
    }
}

// ----------------------------------------------
// UnitTaskRandomizedPatrol
// ----------------------------------------------

// - Unit walks up to a certain distance away from the origin.
// - Once max distance is reached, start walking back to origin.
// - Visit any buildings it is interested on along the way.
pub struct UnitTaskRandomizedPatrol {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Max distance from origin to move to.
    pub max_distance: i32,
    pub path_bias_min: f32,
    pub path_bias_max: f32,
    pub path_record: UnitPatrolPathRecord,

    // If this is not None, will invoke Building::visited_by() on each
    // building of these kinds that we may come across while patrolling.
    pub buildings_to_visit: Option<BuildingKind>,

    // Called on the origin building once the unit has completed its patrol and returned.
    // `|origin_building, patrol_unit, query| -> bool`: Returns if the task should complete or retry.
    pub completion_callback: Option<fn(&mut Building, &mut Unit, &Query) -> bool>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,
}

impl UnitTaskRandomizedPatrol {
    fn try_find_goal(&mut self, unit: &mut Unit, query: &Query) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        let bias = RandomDirectionalBias::new(query.rng(), self.path_bias_min, self.path_bias_max);
        let mut filter = UnitPatrolWaypointFilter::new(query.rng(), &self.path_record);

        match query.find_waypoints(&bias, &mut filter, traversable_node_kinds, start, self.max_distance) {
            SearchResult::PathFound(path) => {
                unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                self.path_record.update(path); // Path taken.
            },
            SearchResult::PathNotFound => {
                // Didn't find a possible path. Retry next update.
            },
        }
    }

    fn try_return_to_origin(&self, unit: &mut Unit, query: &Query) -> bool {
        if query.world().find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            eprintln!("Origin building is no longer valid! TaskPatrol will abort.");
            return false;
        }

        let start = unit.cell();
        let goal  = self.origin_building_tile.road_link;
        let traversable_node_kinds = unit.traversable_node_kinds();

        // Try up to two paths, one of them should be a different path from the one we came from.
        // If there's no second path we only have one way of getting here, so fallback to the same path.
        const MAX_PATHS: usize = 2;
        let mut filter = UnitPatrolReturnPathFilter::new(&self.path_record);

        match query.find_paths(&mut filter, MAX_PATHS, traversable_node_kinds, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile
                );
                unit.move_to_goal(path, goal);
                true
            },
            SearchResult::PathNotFound => {
                eprintln!("Origin building is no longer reachable! (no road access?) TaskPatrol will abort.");
                false
            },
        }
    }

    fn is_returning_to_origin(&self, unit_goal: &UnitNavGoal) -> bool {
        if unit_goal.is_building() {
            unit_goal.building_destination() == (self.origin_building.kind, self.origin_building_tile.base_cell)
        } else {
            false
        }
    }
}

impl UnitTask for UnitTaskRandomizedPatrol {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.max_distance > PATROL_MIN_PREFERRED_PATH_LEN);
        debug_assert!(self.path_bias_min <= self.path_bias_max);

        self.try_find_goal(unit, query);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        // If we have a goal we're already moving somewhere,
        // otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            self.try_find_goal(unit, query);
        }

        if let Some(buildings_to_visit) = self.buildings_to_visit {
            let current_node = Node::new(unit.cell());
            let graph = query.graph();

            if let Some(node_kind) = graph.node_kind(current_node) {
                if node_kind.intersects(PathNodeKind::BuildingRoadLink) {
                    let world = query.world();
                    let tile_map = query.tile_map();
                    let neighbors = graph.neighbors(current_node, PathNodeKind::Building);

                    for neighbor in neighbors {
                        if let Some(building) = world.find_building_for_cell_mut(neighbor.cell, tile_map) {
                            if building.is(buildings_to_visit) {
                                building.visited_by(unit, query);
                            }
                        }
                    }
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
        let unit_goal = unit.goal().expect("Expected unit to have an active goal!");
        let mut task_completed = false;

        if self.is_returning_to_origin(unit_goal) {
            task_completed = invoke_completion_callback(unit,
                                                        query,
                                                        self.origin_building.kind,
                                                        self.origin_building.id,
                                                        self.completion_callback)
                                                        .unwrap_or(true);
            unit.follow_path(None);
        } else {
            // Reached end of path, reroute bach to origin.
            unit.follow_path(None);

            if !self.try_return_to_origin(unit, query) {
                eprintln!("Aborting TaskPatrol. Unable to return to origin building...");
                task_completed = true;
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

    fn draw_debug_ui(&mut self, unit: &mut Unit, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building         : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("Max Distance            : {}", self.max_distance));
        ui.text(format!("Min Path Bias           : {}", self.path_bias_min));
        ui.text(format!("Max Path Bias           : {}", self.path_bias_max));

        self.path_record.draw_debug_ui(ui_sys);

        ui.text(format!("Buildings To Visit      : {}", self.buildings_to_visit.unwrap_or(BuildingKind::empty())));
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_some()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));

        ui.separator();

        if ui.button("Return to Origin") {
            unit.follow_path(None);
            self.try_return_to_origin(unit, query);
        }

        if ui.button("Find New Goal") {
            unit.follow_path(None);
            self.try_find_goal(unit, query);
        }
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
    pub completion_callback: Option<fn(&mut Building, &mut Unit, &Query)>,

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
        let traversable_node_kinds = unit.traversable_node_kinds();

        // Prefer delivering to a storage building.
        let mut path_find_result = find_delivery_candidate(query,
                                                           origin_kind,
                                                           origin_base_cell,
                                                           traversable_node_kinds,
                                                           self.storage_buildings_accepted,
                                                           self.resource_kind_to_deliver);

        if path_find_result.not_found() && self.allow_producer_fallback {
            // Find any producer that can take our resources as fallback.
            path_find_result = find_delivery_candidate(query,
                                                       origin_kind,
                                                       origin_base_cell,
                                                       traversable_node_kinds,
                                                       BuildingKind::producers(),
                                                       self.resource_kind_to_deliver);
        }

        if let PathFindResult::Success { path, goal } = path_find_result {
            unit.move_to_goal(path, goal);
        }
        // Else no path or Storage/Producer building found. Try again later.
    }
}

impl UnitTask for UnitTaskDeliverToStorage {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.inventory_is_empty());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(!self.storage_buildings_accepted.is_empty());
        debug_assert!(self.resource_kind_to_deliver.is_single_resource());
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
        // If we have a goal we're already moving somewhere,
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

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _query: &Query, ui_sys: &UiSystem) {
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

    // Called on the origin building once the unit has returned with resources.
    // `|origin_building, runner_unit, query|`
    pub completion_callback: Option<fn(&mut Building, &mut Unit, &Query)>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Debug...
    pub storage_buildings_visited: u32,
    pub returning_to_origin: bool,
}

impl UnitTaskFetchFromStorage {
    fn try_find_goal(&self, unit: &mut Unit, query: &Query) {
        for resource_to_fetch in self.resources_to_fetch.iter() {
            debug_assert!(resource_to_fetch.kind.is_single_resource());

            let path_find_result = find_storage_fetch_candidate(query,
                                                                self.origin_building.kind,
                                                                unit.cell(),
                                                                unit.traversable_node_kinds(),
                                                                self.storage_buildings_accepted,
                                                                resource_to_fetch.kind);

            if let PathFindResult::Success { path, goal } = path_find_result {
                unit.move_to_goal(path, goal);
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
        let traversable_node_kinds = unit.traversable_node_kinds();

        match query.find_path(traversable_node_kinds, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile
                );
                unit.move_to_goal(path, goal);
                true
            },
            SearchResult::PathNotFound => {
                eprintln!("Origin building is no longer reachable! (no road access?) TaskFetchFromStorage will abort.");
                false
            },
        }
    }

    fn is_returning_to_origin(&self, unit_goal: &UnitNavGoal) -> bool {
        if unit_goal.is_building() {
            unit_goal.building_destination() == (self.origin_building.kind, self.origin_building_tile.base_cell)
        } else {
            false
        }
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
        debug_assert!(self.storage_buildings_visited == 0 && !self.returning_to_origin);

        self.try_find_goal(unit, query);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
        // If we have a goal we're already moving somewhere,
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
            debug_assert!(self.returning_to_origin);

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
            debug_assert!(unit.inventory_is_empty());
            visit_destination(unit, query);
            unit.follow_path(None);

            self.storage_buildings_visited += 1;

            // If we've collected resources from the visited destination
            // we are done and can return to our origin building.
            if let Some(item) = unit.peek_inventory() {
                debug_assert!(item.count != 0, "{item}");
                debug_assert!(self.resources_to_fetch.iter().any(|entry| entry.kind == item.kind), "Expected to have item kind {}", item.kind);

                if !self.try_return_to_origin(unit, query) {
                    // If we couldn't find a path back to the origin, maybe because the origin building
                    // was destroyed, we'll have to abort the task. Any resources collected will be lost.
                    eprintln!("Aborting TaskFetchFromStorage. Unable to return to origin building...");

                    // TODO: We can recover from this and ship the resources back to storage.
                    todo!("Switch to a UnitTaskDeliverToStorage and return the resources");
                } else {
                    self.returning_to_origin = true;
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

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building            : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.separator();
        ui.text(format!("Storage Buildings Accepted : {}", self.storage_buildings_accepted));
        ui.text(format!("Resources To Fetch         : {}", self.resources_to_fetch));
        ui.separator();
        ui.text(format!("Has Completion Callback    : {}", self.completion_callback.is_some()));
        ui.text(format!("Has Completion Task        : {}", self.completion_task.is_some()));
    }
}

// ----------------------------------------------
// UnitTaskFindVacantHouseLot
// ----------------------------------------------

pub struct UnitTaskFindVacantHouseLot {
    // Optional completion callback. Invoke with the empty house lot building we've visited.
    // |unit, vacant_lot, query|
    pub completion_callback: Option<fn(&mut Unit, &Tile, &Query)>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // If true and we can't find an empty lot, try to find any house with room that will take the settler.
    pub fallback_to_houses_with_room: bool,
}

impl UnitTaskFindVacantHouseLot {
    fn try_find_goal(&self, unit: &mut Unit, query: &Query) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();
        let bias = RandomDirectionalBias::new(query.rng(), 0.1, 0.5);

        // First try to find an empty lot we can settle:
        let result = query.find_path_to_node(
            &bias,
            traversable_node_kinds,
            start,
            PathNodeKind::VacantLot);

        if let SearchResult::PathFound(path) = result {
            unit.move_to_goal(path, UnitNavGoal::tile(start, path));
            return;
        }

        // Alternatively try to find a house with room that can take this settler.
        if self.fallback_to_houses_with_room {
            let result = query.find_nearest_buildings(
                start,
                BuildingKind::House,
                traversable_node_kinds,
                None,
                |building, _path| {
                    if let Some(population) = building.population() {
                        if !population.is_maxed() && building.is_linked_to_road(query) {
                            return false; // Accept this building and end the search.
                        }
                    }
                    true // Continue search.
                });

            if let Some((building, path)) = result {
                unit.move_to_goal(
                    path,
                    UnitNavGoal::building(
                        BuildingKind::empty(), // Unused.
                        start,
                        building.kind(),
                        building.tile_info(query))
                );
            }
        }
    }
}

impl UnitTask for UnitTaskFindVacantHouseLot {
    fn as_any(&self) -> &dyn Any { self }

    fn initialize(&mut self, unit: &mut Unit, query: &Query) {
        self.try_find_goal(unit, query);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, query: &Query) -> UnitTaskState {
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
        let tile_map = query.tile_map();

        if unit_goal.is_tile() {
            // Moving to a vacant lot:
            let destination_cell = unit_goal.tile_destination();
            debug_assert!(destination_cell.is_valid());

            if let Some(vacant_lot) = tile_map.try_tile_from_layer(destination_cell, TileMapLayerKind::Terrain) {
                if vacant_lot.tile_def().path_kind.intersects(PathNodeKind::VacantLot) {
                    // Notify completion:
                    if let Some(on_completion) = self.completion_callback {
                        on_completion(unit, vacant_lot, query);
                    }

                    return UnitTaskResult::Completed {
                        next_task: forward_task(&mut self.completion_task)
                    };
                }
            }
        } else if unit_goal.is_building() {
            // Moving to a house with room to take a new settler:
            debug_assert!(self.fallback_to_houses_with_room);

            let (destination_kind, destination_cell) = unit_goal.building_destination();

            debug_assert!(destination_kind == BuildingKind::House);
            debug_assert!(destination_cell.is_valid());

            if let Some(house_tile) = tile_map.find_tile(
                destination_cell,
                TileMapLayerKind::Objects,
                TileKind::Building)
            {
                let world = query.world();
                let mut task_completed = false;

                // Visit destination building:
                if let Some(house_building) = world.find_building_for_cell_mut(destination_cell, tile_map) {
                    if house_building.kind() == destination_kind {
                        let prev_population = house_building.population_count();
                        house_building.visited_by(unit, query);
                        let curr_population = house_building.population_count();

                        // House accepted the setter. Task finished.
                        if curr_population > prev_population {
                            task_completed = true;
                        }
                    }
                }

                if task_completed {
                    // Notify completion:
                    if let Some(on_completion) = self.completion_callback {
                        on_completion(unit, house_tile, query);
                    }

                    return UnitTaskResult::Completed {
                        next_task: forward_task(&mut self.completion_task)
                    };
                }
            }
        }

        // Failed; retry.
        unit.follow_path(None);
        UnitTaskResult::Retry
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
            UnitTaskState::TerminateAndDespawn { post_despawn_callback } => {
                UnitTaskResult::TerminateAndDespawn { post_despawn_callback }
            },
            UnitTaskState::Uninitialized => {
                panic!("Invalid task state: Uninitialized");
            }
        }
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

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

        self.archetype.draw_debug_ui(unit, query, ui_sys);
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
    pub fn new(pool_capacity: usize) -> Self {
        Self {
            task_pool: UnitTaskPool::new(pool_capacity),
        }
    }

    #[inline]
    pub fn new_task<Task>(&mut self, task: Task) -> Option<UnitTaskId>
        where Task: UnitTask,
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
        where Task: UnitTask + 'static
    {
        let task = match self.task_pool.try_get(task_id) {
            Some(task) => task,
            None => return false,
        };
        task.archetype.as_any().is::<Task>()
    }

    #[inline]
    pub fn try_get_task<Task>(&self, task_id: UnitTaskId) -> Option<&Task>
        where Task: UnitTask + 'static
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
                    UnitTaskResult::TerminateAndDespawn { post_despawn_callback } => {
                        let unit_prev_cell = unit.cell();
                        let unit_prev_goal = unit.goal().cloned();

                        unit.assign_task(self, None);
                        query.despawn_unit(unit);

                        if let Some(callback) = post_despawn_callback {
                            callback(query, unit_prev_cell, unit_prev_goal);
                        }
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

    pub fn draw_tasks_debug_ui(&mut self, unit: &mut Unit, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(current_task_id) = unit.current_task() {
            if let Some(task) = self.task_pool.try_get_mut(current_task_id) {
                task.draw_debug_ui(unit, query, ui_sys);
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
    let (destination_kind, destination_cell) = unit_goal.building_destination();

    debug_assert!(destination_kind.is_single_building());
    debug_assert!(destination_cell.is_valid());

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

fn invoke_completion_callback<F, R>(unit: &mut Unit,
                                    query: &Query,
                                    origin_building_kind: BuildingKind,
                                    origin_building_id: BuildingId,
                                    completion_callback: Option<F>) -> Option<R>
    where F: FnOnce(&mut Building, &mut Unit, &Query) -> R
{
    if let Some(on_completion) = completion_callback {
        if let Some(origin_building) = query.world().find_building_mut(origin_building_kind, origin_building_id) {
            // NOTE: Only invoke the completion callback if the original base cell still contains the
            // exact same building that initiated this task. We don't want to accidentally invoke the
            // callback on a different building, even if the type of building there is the same.
            debug_assert!(origin_building.kind() == origin_building_kind);
            debug_assert!(origin_building.id()   == origin_building_id);
            return Some(on_completion(origin_building, unit, query));
        }
    }
    None
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

    fn from_query_result<'search>(query: &'search Query,
                                  origin_kind: BuildingKind,
                                  origin_base_cell: Cell,
                                  result: Option<(&Building, &'search Path)>) -> PathFindResult<'search> {
        match result {
            Some((destination_building, path)) => {
                debug_assert!(!path.is_empty());

                let destination_road_link = destination_building.road_link(query)
                    .expect("Dest building should have a road link tile!");

                if destination_road_link != path.last().unwrap().cell {
                    eprintln!("Dest building road link does not match path goal!: {} != {}, path length = {}",
                              destination_road_link, path.last().unwrap().cell, path.len());
                    return PathFindResult::NotFound;
                }

                PathFindResult::Success {
                    path,
                    goal: UnitNavGoal::building(
                        origin_kind,
                        origin_base_cell,
                        destination_building.kind(),
                        destination_building.tile_info(query))
                }
            },
            None => PathFindResult::NotFound,
        }
    }
}

fn find_delivery_candidate<'search>(query: &'search Query,
                                    origin_kind: BuildingKind,
                                    origin_base_cell: Cell,
                                    traversable_node_kinds: PathNodeKind,
                                    building_kinds_accepted: BuildingKind,
                                    resource_kind_to_deliver: ResourceKind) -> PathFindResult<'search> {

    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!building_kinds_accepted.is_empty());
    debug_assert!(resource_kind_to_deliver.is_single_resource()); // Only one resource kind at a time.
    debug_assert!(traversable_node_kinds == PathNodeKind::Road, "Traversable Nodes={traversable_node_kinds}");

    // Try to find a building that can accept our delivery:
    let result = query.find_nearest_buildings(
        origin_base_cell,
        building_kinds_accepted,
        traversable_node_kinds,
        None,
        |building, _path| {
            if building.receivable_resources(resource_kind_to_deliver) != 0 &&
               building.is_linked_to_road(query) {
                return false; // Accept this building and end the search.
            }
            // Else we couldn't find a free slot in this building or it
            // is not connected to a road. Try again with another one.
            true
        });

    PathFindResult::from_query_result(query, origin_kind, origin_base_cell, result)
}

fn find_storage_fetch_candidate<'search>(query: &'search Query,
                                         origin_kind: BuildingKind,
                                         origin_base_cell: Cell,
                                         traversable_node_kinds: PathNodeKind,
                                         storage_buildings_accepted: BuildingKind,
                                         resource_kind_to_fetch: ResourceKind) -> PathFindResult<'search> {

    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!storage_buildings_accepted.is_empty());
    debug_assert!(resource_kind_to_fetch.is_single_resource()); // Only one resource kind at a time.
    debug_assert!(traversable_node_kinds == PathNodeKind::Road, "Traversable Nodes={traversable_node_kinds}");

    // Try to find a storage building that has the resource we want:
    let result = query.find_nearest_buildings(
        origin_base_cell,
        storage_buildings_accepted,
        traversable_node_kinds,
        None,
        |building, _path| {
            if building.available_resources(resource_kind_to_fetch) != 0 &&
               building.is_linked_to_road(query) {
                return false; // Accept this building and end the search.
            }
            // Else we couldn't find the resource we're looking for in this building
            // or it is not connected to a road. Try again with another one.
            true
        });

    PathFindResult::from_query_result(query, origin_kind, origin_base_cell, result)
}
