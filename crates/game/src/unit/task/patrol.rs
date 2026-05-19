use std::any::Any;
use rand::{Rng, seq::SliceRandom};
use serde::{Deserialize, Serialize};

use common::{
    callback::Callback,
    time::{CountdownTimer, Seconds},
};
use engine::{log, ui::UiSystem};

use super::{
    TaskContext,
    TaskState,
    Transition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    invoke_completion_callback_deferred,
    with_task,
};
use crate::{
    debug,
    pathfind::{
        Node,
        NodeKind as PathNodeKind,
        Path,
        PathFilter,
        PathHistory,
        RandomDirectionalBias,
        SearchResult,
    },
    sim::{SimCmdQueue, SimContext},
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
    // Walking out to randomized waypoints, visiting buildings along the way.
    #[default]
    Patrolling,

    // Walking back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    DeliveringToOrigin,

    // Terminal state.
    Done,
}

// Max unique buildings recorded in `UnitTaskRandomizedPatrol::visited_buildings`.
// Patrols rarely walk past more than a handful of target buildings in one run;
// 32 leaves headroom without inflating the task struct too much.
pub const MAX_PATROL_VISITED_BUILDINGS: usize = 32;

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

    #[serde(default)]
    pub state: UnitTaskPatrolState,

    // Set by the deferred completion callback; consumed by `DeliveringToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,

    // Unique buildings this patrol has queued visits to during its current run.
    // Capped at MAX_PATROL_VISITED_BUILDINGS -- further matches are silently
    // ignored. Visible to tests/debug UI; not used by the task logic itself.
    #[serde(skip)]
    pub visited_buildings: Option<Vec<BuildingKindAndId>>,
}

impl UnitTaskRandomizedPatrol {
    fn try_find_goal(&mut self, unit: &mut Unit, sim_context: &SimContext) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        let bias = RandomDirectionalBias::new(sim_context.rng_mut(), self.path_bias_min, self.path_bias_max);
        let mut filter = UnitPatrolWaypointFilter::new(sim_context.rng_mut(), &self.path_record);

        match sim_context.find_waypoints(&bias, &mut filter, traversable_node_kinds, start, self.max_distance) {
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

    fn try_return_to_origin(&mut self, unit: &mut Unit, sim_context: &SimContext) -> bool {
        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::error!(log::channel!("task"), "Origin building is no longer valid! TaskPatrol will abort.");
            return false;
        }

        let start = unit.cell();
        let goal_cell = self.origin_building_tile.road_link;
        let traversable_node_kinds = unit.traversable_node_kinds();

        // Try up to two paths, one of them should be a different path from the one we
        // came from. If there's no second path we only have one way of getting
        // here, so fallback to the same path.
        const MAX_PATHS: usize = 2;
        let mut filter = UnitPatrolReturnPathFilter::new(&self.path_record);

        match sim_context.find_paths(&mut filter, MAX_PATHS, traversable_node_kinds, start, goal_cell) {
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

    fn reset_idle_countdown(&mut self) {
        if let Some((idle_countdown, countdown)) = &mut self.idle_countdown {
            idle_countdown.reset(*countdown);
        }
    }

    // Ticks the optional idle countdown. Returns true when the unit is free to
    // move on (countdown elapsed, or there is no countdown); false while idling.
    fn tick_idle(&mut self, ctx: &mut TaskContext) -> bool {
        let Some((idle_countdown, _)) = &mut self.idle_countdown else {
            return true;
        };

        if idle_countdown.tick(ctx.sim_context.delta_time_secs()) {
            true
        } else {
            // NOTE: idle() changes the anim in the underlying Tile instance,
            // so it must be deferred to post-update.
            let unit_id = ctx.unit.id();
            ctx.sim_cmds.defer_unit_update(unit_id, |context, unit| {
                unit.idle(context);
            });
            false
        }
    }

    // Queues deferred visits to any target buildings adjacent to the unit's current cell.
    fn visit_buildings_along_way(&mut self, ctx: &mut TaskContext) {
        let Some(buildings_to_visit) = self.buildings_to_visit else {
            return;
        };

        let sim_context = ctx.sim_context;
        let current_node = Node::new(ctx.unit.cell());
        let unit_id = ctx.unit.id();

        let graph = sim_context.graph();
        let Some(node_kind) = graph.node_kind(current_node) else {
            return;
        };

        if !node_kind.intersects(PathNodeKind::BuildingRoadLink) {
            return;
        }

        let neighbors = graph.neighbors(current_node, PathNodeKind::Building);
        for neighbor in neighbors {
            let Some(building) = sim_context.find_building_for_cell(neighbor.cell) else {
                continue;
            };

            if !building.is(buildings_to_visit) {
                continue;
            }

            let kind_and_id = building.kind_and_id();
            ctx.sim_cmds.visit_building(kind_and_id, unit_id);

            // Track unique buildings the patrol has queued visits to.
            // The Vec is capped; once full, additional matches are dropped.
            if let Some(visited_buildings) = &mut self.visited_buildings {
                if visited_buildings.len() < MAX_PATROL_VISITED_BUILDINGS
                    && !visited_buildings.contains(&kind_and_id)
                {
                    visited_buildings.push(kind_and_id);
                }
            }
        }
    }

    // Schedules the deferred completion callback on the origin building.
    fn schedule_completion_callback(&mut self, ctx: &mut TaskContext) -> bool {
        invoke_completion_callback_deferred(
            ctx.unit,
            ctx.sim_cmds,
            ctx.sim_context,
            self.origin_building.kind,
            self.origin_building.id,
            self.completion_callback.get(),
            |context, _building, unit| {
                with_task::<UnitTaskRandomizedPatrol>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_patrolling(&mut self, ctx: &mut TaskContext) -> Transition<UnitTaskPatrolState> {
        if ctx.unit.goal().is_none() {
            // Wait out the idle countdown, then find the next waypoint.
            if self.tick_idle(ctx) {
                self.try_find_goal(ctx.unit, ctx.sim_context);
            }
            return Transition::Stay;
        }

        self.visit_buildings_along_way(ctx);

        if !ctx.unit.has_reached_goal() {
            return Transition::Stay;
        }

        // Reached the waypoint. Idle here until the countdown elapses, then head home.
        if !self.tick_idle(ctx) {
            return Transition::Stay;
        }

        ctx.unit.follow_path(None);

        if self.try_return_to_origin(ctx.unit, ctx.sim_context) {
            Transition::Goto(UnitTaskPatrolState::ReturningToOrigin)
        } else {
            // Can't get back to origin; abort.
            Transition::Goto(UnitTaskPatrolState::Done)
        }
    }

    fn update_returning(&mut self, ctx: &mut TaskContext) -> Transition<UnitTaskPatrolState> {
        if ctx.unit.goal().is_none() {
            // No path home yet; try to (re)route.
            if !self.try_return_to_origin(ctx.unit, ctx.sim_context) {
                return Transition::Goto(UnitTaskPatrolState::Done);
            }
            return Transition::Stay;
        }

        self.visit_buildings_along_way(ctx);

        if !ctx.unit.has_reached_goal() {
            return Transition::Stay;
        }

        // Reached origin. Idle out the countdown, then run the completion callback.
        if !self.tick_idle(ctx) {
            return Transition::Stay;
        }

        ctx.unit.follow_path(None);

        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            Transition::Goto(UnitTaskPatrolState::DeliveringToOrigin)
        } else {
            // No completion callback, or origin building no longer exists.
            Transition::Goto(UnitTaskPatrolState::Done)
        }
    }

    fn update_delivering(&mut self, _ctx: &mut TaskContext) -> Transition<UnitTaskPatrolState> {
        if self.completion_callback_done {
            Transition::Goto(UnitTaskPatrolState::Done)
        } else {
            Transition::Stay
        }
    }
}

impl TaskState for UnitTaskPatrolState {
    type Task = UnitTaskRandomizedPatrol;

    fn update(self, task: &mut UnitTaskRandomizedPatrol, ctx: &mut TaskContext) -> Transition<Self> {
        match self {
            Self::Patrolling         => task.update_patrolling(ctx),
            Self::ReturningToOrigin  => task.update_returning(ctx),
            Self::DeliveringToOrigin => task.update_delivering(ctx),
            Self::Done               => Transition::Done,
        }
    }
}

impl UnitTask for UnitTaskRandomizedPatrol {
    type State = UnitTaskPatrolState;

    fn initialize(&mut self, ctx: &mut TaskContext) {
        // Sanity check:
        debug_assert!(ctx.unit.goal().is_none());
        debug_assert_eq!(ctx.unit.cell(), self.origin_building_tile.road_link); // We start at the nearest building road link.
        debug_assert!(self.origin_building.is_valid());
        debug_assert!(self.origin_building_tile.is_valid());
        debug_assert!(self.max_distance > PATROL_MIN_PREFERRED_PATH_LEN);
        debug_assert!(self.path_bias_min <= self.path_bias_max);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn state(&mut self) -> &mut Self::State {
        &mut self.state
    }

    fn completion_task(&mut self) -> Option<UnitTaskId> {
        self.completion_task.take()
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, sim_context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building         : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("State                   : {:?}", self.state));
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
            self.try_return_to_origin(unit, sim_context);
        }

        if ui.button("Find New Goal") {
            unit.follow_path(None);
            self.try_find_goal(unit, sim_context);
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
