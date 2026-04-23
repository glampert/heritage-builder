use std::any::Any;
use rand::{Rng, seq::SliceRandom};
use serde::{Deserialize, Serialize};

use common::{
    callback::Callback,
    time::{CountdownTimer, Seconds},
};
use engine::{log, ui::UiSystem};

use super::{
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    UnitTaskResult,
    UnitTaskState,
    common::invoke_completion_callback_deferred,
};
use crate::{
    debug::{self},
    pathfind::{
        Node,
        NodeKind as PathNodeKind,
        Path,
        PathFilter,
        PathHistory,
        RandomDirectionalBias,
        SearchResult,
    },
    sim::{SimCmds, SimCmdQueue, SimContext},
    tile::TileMapLayerKind,
    unit::{Unit, navigation::{self, UnitDirection, UnitNavGoal}},
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitPatrolPathRecord
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
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
        let new_direction = path_direction(path);

        if navigation::same_axis(new_direction, prev_direction) {
            self.repeated_axis_count += 1;
        } else {
            self.repeated_axis_count = 0;
        }

        self.current_length = path.len() as u32;
        self.current_direction = new_direction;
    }

    pub fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        ui.text(format!("Previous Path Hashes    : {}", self.history));
        ui.text(format!("Current Path Length     : {}", self.current_length));
        ui.text(format!("Current Path Direction  : {}", self.current_direction));
        ui.text(format!("Path History Same Axis  : {}", self.repeated_axis_count));
    }
}

fn path_direction(path: &Path) -> UnitDirection {
    let start = path.first().unwrap();
    let goal = path.last().unwrap();
    navigation::direction_between(start.cell, goal.cell)
}

// ----------------------------------------------
// UnitPatrolWaypointFilter
// ----------------------------------------------

const PATROL_MIN_PREFERRED_PATH_LEN: i32 = 4;
const PATROL_MAX_REPEATED_DIR_AXIS: i32 = 2;

struct UnitPatrolWaypointFilter<'task, R: Rng> {
    rng: &'task mut R,
    path_record: &'task UnitPatrolPathRecord,
    preferred_fallback_path_index: Option<usize>,
}

impl<'task, R: Rng> UnitPatrolWaypointFilter<'task, R> {
    fn new(rng: &'task mut R, path_record: &'task UnitPatrolPathRecord) -> Self {
        Self { rng, path_record, preferred_fallback_path_index: None }
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
        let new_direction = path_direction(path);

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
        if navigation::same_axis(new_direction, prev_direction)
            && self.path_record.repeated_axis_count >= PATROL_MAX_REPEATED_DIR_AXIS
        {
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

pub type UnitTaskPatrolCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskPatrolState {
    #[default]
    Running,
    PendingCompletionCallback,
    Completed,
}

// - Unit walks up to a certain distance away from the origin.
// - Once max distance is reached, start walking back to origin.
// - Visit any buildings it is interested on along the way.
#[derive(Serialize, Deserialize)]
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
    // `|context, origin_building, patrol_unit|`.
    pub completion_callback: Callback<UnitTaskPatrolCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Optional idle timeout between goals.
    pub idle_countdown: Option<(CountdownTimer, Seconds)>,

    // Current internal completion state. Should start as Running.
    // Deserialize uses Default if missing to retain backwards compatibility with older save files.
    #[serde(default)]
    pub internal_state: UnitTaskPatrolState,
}

impl UnitTaskRandomizedPatrol {
    fn try_find_goal(&mut self, unit: &mut Unit, context: &SimContext) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        let bias = RandomDirectionalBias::new(context.rng_mut(), self.path_bias_min, self.path_bias_max);
        let mut filter = UnitPatrolWaypointFilter::new(context.rng_mut(), &self.path_record);

        match context.find_waypoints(&bias, &mut filter, traversable_node_kinds, start, self.max_distance) {
            SearchResult::PathFound(path) => {
                unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                self.path_record.update(path); // Path taken.
                self.reset_idle_countdown();
            }
            SearchResult::PathNotFound => {
                // Didn't find a possible path. Retry next update.
            }
        }
    }

    fn try_return_to_origin(&mut self, unit: &mut Unit, context: &SimContext) -> bool {
        if context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::error!(log::channel!("task"), "Origin building is no longer valid! TaskPatrol will abort.");
            return false;
        }

        let start = unit.cell();
        let goal = self.origin_building_tile.road_link;
        let traversable_node_kinds = unit.traversable_node_kinds();

        // Try up to two paths, one of them should be a different path from the one we
        // came from. If there's no second path we only have one way of getting
        // here, so fallback to the same path.
        const MAX_PATHS: usize = 2;
        let mut filter = UnitPatrolReturnPathFilter::new(&self.path_record);

        match context.find_paths(&mut filter, MAX_PATHS, traversable_node_kinds, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                unit.move_to_goal(path, goal);
                self.reset_idle_countdown();
                true
            }
            SearchResult::PathNotFound => {
                log::error!(
                    log::channel!("task"),
                    "Origin building is no longer reachable! (no road access?) TaskPatrol will abort."
                );
                false
            }
        }
    }

    fn is_returning_to_origin(&self, unit_goal: &UnitNavGoal) -> bool {
        if unit_goal.is_building() {
            unit_goal.building_destination() == (self.origin_building.kind, self.origin_building_tile.base_cell)
        } else {
            false
        }
    }

    fn reset_idle_countdown(&mut self) {
        if let Some((idle_countdown, countdown)) = &mut self.idle_countdown {
            idle_countdown.reset(*countdown);
        }
    }
}

impl UnitTask for UnitTaskRandomizedPatrol {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn initialize(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) {
        // Sanity check:
        debug_assert!(unit.goal().is_none());
        debug_assert!(unit.cell() == self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.max_distance > PATROL_MIN_PREFERRED_PATH_LEN);
        debug_assert!(self.path_bias_min <= self.path_bias_max);

        if self.idle_countdown.is_none() {
            self.try_find_goal(unit, context);
        }
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        match self.internal_state {
            UnitTaskPatrolState::PendingCompletionCallback => {
                // Wait for the deferred completion callback to be executed.
                return UnitTaskState::Running;
            }
            UnitTaskPatrolState::Completed => {
                // Deferred completion callback has run; end the task.
                return UnitTaskState::Completed;
            }
            UnitTaskPatrolState::Running => {}
        }

        // If we have a goal we're already moving somewhere,
        // otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            // If we have an idle countdown, only transition to the next goal when it has elapsed.
            if let Some((idle_countdown, _)) = &mut self.idle_countdown {
                if idle_countdown.tick(context.delta_time_secs()) {
                    self.try_find_goal(unit, context);
                } else {
                    cmds.defer_unit_update(unit.id(), |context, unit| {
                        // NOTE: idle() changes the anim in the underlying
                        // Tile instance, so must be deferred to post update.
                        unit.idle(context);
                    });
                }
            } else {
                // Move to goal immediately.
                self.try_find_goal(unit, context);
            }
        }

        if let Some(buildings_to_visit) = self.buildings_to_visit {
            let current_node = Node::new(unit.cell());
            let graph = context.graph();

            if let Some(node_kind) = graph.node_kind(current_node) {
                if node_kind.intersects(PathNodeKind::BuildingRoadLink) {
                    let neighbors = graph.neighbors(current_node, PathNodeKind::Building);
                    for neighbor in neighbors {
                        if let Some(building) = context.find_building_for_cell(neighbor.cell) {
                            if building.is(buildings_to_visit) {
                                cmds.visit_building(building.kind_and_id(), unit.id());
                            }
                        }
                    }
                }
            }
        }

        if unit.has_reached_goal() {
            if let Some((idle_countdown, _)) = &mut self.idle_countdown {
                if idle_countdown.tick(context.delta_time_secs()) {
                    return UnitTaskState::Completed;
                } else {
                    cmds.defer_unit_update(unit.id(), |context, unit| {
                        // NOTE: idle() changes the anim in the underlying
                        // Tile instance, so must be deferred to post update.
                        unit.idle(context);
                    });
                    return UnitTaskState::Running;
                }
            }
            UnitTaskState::Completed
        } else {
            UnitTaskState::Running
        }
    }

    fn completed(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        // If the deferred completion callback has already run, finalize the task.
        if self.internal_state == UnitTaskPatrolState::Completed {
            return UnitTaskResult::completed_with(&mut self.completion_task);
        }

        let unit_goal = unit.goal().expect("Expected unit to have an active goal!");
        let mut task_completed = false;

        if self.is_returning_to_origin(unit_goal) {
            if self.completion_callback.is_valid() {
                let scheduled = invoke_completion_callback_deferred(
                    unit,
                    cmds,
                    context,
                    self.origin_building.kind,
                    self.origin_building.id,
                    self.completion_callback.get(),
                    |context, _building, unit| {
                        let task = unit.current_task_as_mut::<Self>(context.task_manager_mut())
                            .expect("Expected unit to be running UnitTaskRandomizedPatrol!");

                        debug_assert_eq!(task.internal_state, UnitTaskPatrolState::PendingCompletionCallback);
                        task.internal_state = UnitTaskPatrolState::Completed;
                    },
                );

                if scheduled {
                    // Wait for deferred callback to complete before ending the task.
                    self.internal_state = UnitTaskPatrolState::PendingCompletionCallback;
                    unit.follow_path(None);
                    return UnitTaskResult::Retry;
                }

                // Origin building no longer exists; end the task without invoking the callback.
                task_completed = true;
                unit.follow_path(None);
            }
        } else {
            // Reached end of path, reroute bach to origin.
            unit.follow_path(None);

            if !self.try_return_to_origin(unit, context) {
                log::error!(log::channel!("task"), "Aborting TaskPatrol. Unable to return to origin building...");
                task_completed = true;
            }
        }

        if task_completed {
            UnitTaskResult::completed_with(&mut self.completion_task)
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building         : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("Internal State          : {:?}", self.internal_state));
        ui.text(format!("Max Distance            : {}", self.max_distance));
        ui.text(format!("Min Path Bias           : {}", self.path_bias_min));
        ui.text(format!("Max Path Bias           : {}", self.path_bias_max));

        self.path_record.draw_debug_ui(ui_sys);

        ui.text(format!("Buildings To Visit      : {}", self.buildings_to_visit.unwrap_or(BuildingKind::empty())));
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));
        ui.text(format!("Idle Countdown Timer    : {:.2}", self.idle_countdown.as_ref().map_or(0.0, |(countdown, _)| countdown.remaining_secs())));

        ui.separator();

        if ui.button("Return to Origin") {
            unit.follow_path(None);
            self.try_return_to_origin(unit, context);
        }

        if ui.button("Find New Goal") {
            unit.follow_path(None);
            self.try_find_goal(unit, context);
        }

        if let Some((idle_countdown, countdown)) = &mut self.idle_countdown {
            ui.separator();

            if ui.button("Restart Idle Countdown") {
                idle_countdown.reset(*countdown);
            }

            if ui.button("Clear Idle Countdown") {
                idle_countdown.reset(0.0);
            }
        }
    }
}
