use serde::{Deserialize, Serialize};
use strum::Display;

use common::{callback::Callback, coords::Cell};
use engine::log;

use super::despawn::UnitTaskPostDespawnCallback;
use crate::{
    pathfind::{NodeKind as PathNodeKind, Path},
    world::object::{GameObject, GenerationalIndex},
    sim::{SimCmds, SimContext, resources::ResourceKind},
    unit::{Unit, navigation::UnitNavGoal},
    building::{Building, BuildingId, BuildingKind, BuildingVisitResult},
};

// ----------------------------------------------
// Helper types
// ----------------------------------------------

pub type UnitTaskId = GenerationalIndex;

#[derive(Display, Serialize, Deserialize)]
pub enum UnitTaskState {
    Uninitialized,
    Running,
    Completed,
    TerminateAndDespawn {
        post_despawn_callback: Callback<UnitTaskPostDespawnCallback>,
        callback_extra_args: UnitTaskArgs,
    },
}

#[derive(Display)]
pub enum UnitTaskResult {
    Running,
    Retry,
    Completed {
        next_task: UnitTaskForwarded, // Optional next task to run.
    },
    TerminateAndDespawn {
        post_despawn_callback: Callback<UnitTaskPostDespawnCallback>,
        callback_extra_args: UnitTaskArgs,
    },
}

impl UnitTaskResult {
    #[inline]
    pub(super) fn completed_with(completion_task: &mut Option<UnitTaskId>) -> Self {
        Self::Completed { next_task: UnitTaskForwarded(completion_task.take()) }
    }
}

pub struct UnitTaskForwarded(pub(super) Option<UnitTaskId>);

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum UnitTaskArg {
    None,
    Bool(bool),
    I32(i32),
    U32(u32),
    F32(f32),
}

impl UnitTaskArg {
    pub fn as_bool(self) -> bool {
        match self {
            Self::Bool(value) => value,
            _ => panic!("UnitTaskArg does not hold bool!"),
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::I32(value) => value,
            _ => panic!("UnitTaskArg does not hold i32!"),
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::U32(value) => value,
            _ => panic!("UnitTaskArg does not hold u32!"),
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            Self::F32(value) => value,
            _ => panic!("UnitTaskArg does not hold f32!"),
        }
    }
}

pub(super) const MAX_TASK_EXTRA_ARGS: usize = 1;

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct UnitTaskArgs {
    pub(super) args: Option<[UnitTaskArg; MAX_TASK_EXTRA_ARGS]>,
}

impl UnitTaskArgs {
    pub fn empty() -> Self {
        Self { args: None }
    }

    pub fn new(args: &[UnitTaskArg]) -> Self {
        let len = args.len();
        debug_assert!(len > 0 && len <= MAX_TASK_EXTRA_ARGS);

        let mut arr = [UnitTaskArg::None; MAX_TASK_EXTRA_ARGS];
        arr[..len].copy_from_slice(&args[..len]);

        Self { args: Some(arr) }
    }
}

// ----------------------------------------------
// Task helpers:
// ----------------------------------------------

pub(super) fn visit_destination_deferred<F>(
    unit: &mut Unit,
    cmds: &mut SimCmds,
    context: &SimContext,
    on_post_visit: F,
) -> bool
where
    F: Fn(&SimContext, &mut Building, &mut Unit, BuildingVisitResult) + 'static
{
    let unit_goal = unit.goal().expect("Expected unit to have an active goal!");
    let (destination_kind, destination_cell) = unit_goal.building_destination();

    debug_assert!(destination_kind.is_single_building());
    debug_assert!(destination_cell.is_valid());

    // Visit destination building (deferred):
    if let Some(destination_building) = context.world().find_building_for_cell(destination_cell, context.tile_map()) {
        // NOTE: No need to check for generation match here. If the destination building
        // is still the same kind of building we where looking for, it doesn't matter if
        // it was destroyed and recreated since we started the task.
        if destination_building.kind() == destination_kind {
            cmds.visit_building_with_completion(destination_building.kind_and_id(), unit.id(), on_post_visit);
            return true;
        }
    }

    false
}

// The post-callback is used to notify the owning task that the deferred
// callback has been executed so it can end.
//
// NOTE: If the origin building is no longer valid, neither callback will run,
// so the caller must be able to handle this case as well (the task will simply
// not see its post-callback fire and should recover on its own).
pub(super) fn invoke_completion_callback_deferred<F>(
    unit: &mut Unit,
    cmds: &mut SimCmds,
    context: &SimContext,
    origin_building_kind: BuildingKind,
    origin_building_id: BuildingId,
    callback: F,
    post_callback: F,
) -> bool
where
    F: Fn(&SimContext, &mut Building, &mut Unit) + 'static
{
    // Deferred execution (non-mutable building access).
    if let Some(origin_building) = context.world().find_building(origin_building_kind, origin_building_id) {
        // NOTE: Only invoke the completion callback if the original base cell still
        // contains the exact same building that initiated this task. We don't
        // want to accidentally invoke the callback on a different building,
        // even if the type of building there is the same.
        debug_assert!(origin_building.kind() == origin_building_kind);
        debug_assert!(origin_building.id()   == origin_building_id);

        cmds.defer_task_step_with_completion(origin_building.kind_and_id(), unit.id(), callback, post_callback);
        return true;
    }

    false
}

pub(super) fn invoke_completion_callback_immediate<F>(
    unit: &mut Unit,
    context: &SimContext,
    origin_building_kind: BuildingKind,
    origin_building_id: BuildingId,
    callback: F,
) -> bool
where
    F: Fn(&SimContext, &mut Building, &mut Unit) + 'static
{
    // Immediate execution (requires mutable building access).
    if let Some(origin_building) = context.world_mut().find_building_mut(origin_building_kind, origin_building_id) {
        // See comment above on invoke_completion_callback_deferred.
        debug_assert!(origin_building.kind() == origin_building_kind);
        debug_assert!(origin_building.id()   == origin_building_id);

        callback(context, origin_building, unit);
        return true;
    }

    false
}

// ----------------------------------------------
// Path finding helpers:
// ----------------------------------------------

pub(super) enum PathFindResult<'search> {
    Success { path: &'search Path, goal: UnitNavGoal },
    NotFound,
}

impl PathFindResult<'_> {
    pub(super) fn not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }

    fn from_query_result<'search>(
        context: &'search SimContext,
        origin_kind: BuildingKind,
        origin_base_cell: Cell,
        result: Option<(&Building, &'search Path)>,
    ) -> PathFindResult<'search> {
        match result {
            Some((destination_building, path)) => {
                debug_assert!(!path.is_empty());

                let destination_road_link =
                    destination_building.road_link(context).expect("Dest building should have a road link tile!");

                if destination_road_link != path.last().unwrap().cell {
                    log::error!(
                        log::channel!("task"),
                        "Dest building road link does not match path goal!: {} != {}, path length = {}",
                        destination_road_link,
                        path.last().unwrap().cell,
                        path.len()
                    );
                    return PathFindResult::NotFound;
                }

                PathFindResult::Success {
                    path,
                    goal: UnitNavGoal::building(
                        origin_kind,
                        origin_base_cell,
                        destination_building.kind(),
                        destination_building.tile_info(context),
                    ),
                }
            }
            None => PathFindResult::NotFound,
        }
    }
}

pub(super) fn find_delivery_candidate(
    context: &SimContext,
    origin_kind: BuildingKind,
    origin_base_cell: Cell,
    traversable_node_kinds: PathNodeKind,
    building_kinds_accepted: BuildingKind,
    resource_kind_to_deliver: ResourceKind,
) -> PathFindResult<'_> {
    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!building_kinds_accepted.is_empty());
    debug_assert!(resource_kind_to_deliver.is_single_resource()); // Only one resource kind at a time.
    debug_assert!(traversable_node_kinds == PathNodeKind::Road, "Traversable Nodes={traversable_node_kinds}");

    // Try to find a building that can accept our delivery:
    let result = context.find_nearest_buildings(
        origin_base_cell,
        building_kinds_accepted,
        traversable_node_kinds,
        None,
        |building, _path| {
            if building.receivable_resources(resource_kind_to_deliver) != 0 && building.is_linked_to_road(context) {
                return false; // Accept this building and end the search.
            }
            // Else we couldn't find a free slot in this building or it
            // is not connected to a road. Try again with another one.
            true
        },
    );

    PathFindResult::from_query_result(context, origin_kind, origin_base_cell, result)
}

pub(super) fn find_storage_fetch_candidate(
    context: &SimContext,
    origin_kind: BuildingKind,
    origin_base_cell: Cell,
    traversable_node_kinds: PathNodeKind,
    storage_buildings_accepted: BuildingKind,
    resource_kind_to_fetch: ResourceKind,
) -> PathFindResult<'_> {
    debug_assert!(origin_base_cell.is_valid());
    debug_assert!(!storage_buildings_accepted.is_empty());
    debug_assert!(resource_kind_to_fetch.is_single_resource()); // Only one resource kind at a time.
    debug_assert!(traversable_node_kinds == PathNodeKind::Road, "Traversable Nodes={traversable_node_kinds}");

    // Try to find a storage building that has the resource we want:
    let result = context.find_nearest_buildings(
        origin_base_cell,
        storage_buildings_accepted,
        traversable_node_kinds,
        None,
        |building, _path| {
            if building.available_resources(resource_kind_to_fetch) != 0 && building.is_linked_to_road(context) {
                return false; // Accept this building and end the search.
            }
            // Else we couldn't find the resource we're looking for
            // in this building or it is not connected to a road.
            // Try again with another one.
            true
        },
    );

    PathFindResult::from_query_result(context, origin_kind, origin_base_cell, result)
}
