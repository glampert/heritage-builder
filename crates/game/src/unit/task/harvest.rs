use std::any::Any;
use rand::seq::SliceRandom;
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
    debug,
    pathfind::{
        self,
        Node,
        NodeKind as PathNodeKind,
        Path,
        PathFilter,
        SearchResult,
    },
    tile::TileMapLayerKind,
    prop::PropId,
    unit::{Unit, UnitId, navigation::UnitNavGoal},
    building::{Building, BuildingKindAndId, BuildingTileInfo},
    sim::{SimCmds, SimContext, resources::ResourceKind},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskHarvestWood
// ----------------------------------------------

pub type UnitTaskHarvestCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

// How long it takes for a unit to complete a harvest once it arrives at a tree.
const WOOD_HARVEST_TIME_INTERVAL: Seconds = 20.0;

// Take a random range between 1 and this for the amount of wood harvested each time.
const WOOD_HARVEST_MAX_AMOUNT: u32 = 5;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskHarvestState {
    #[default]
    Running,
    PendingCompletionCallback,
    Completed,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskHarvestWood {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Optional completion callback.
    // |context, origin_building, harvester_unit|
    pub completion_callback: Callback<UnitTaskHarvestCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Internal - set to defaults.
    pub harvest_timer: CountdownTimer,
    pub harvest_target: PropId,
    pub is_returning_to_origin: bool,

    // Current internal completion state. Should start as Running.
    // Deserialize uses Default if missing to retain backwards compatibility with older save files.
    #[serde(default)]
    pub internal_state: UnitTaskHarvestState,
}

struct FindHarvestableTreeFilter<'task> {
    context: &'task SimContext,
}

impl<'task> FindHarvestableTreeFilter<'task> {
    fn new(context: &'task SimContext) -> Self {
        Self { context }
    }
}

impl PathFilter for FindHarvestableTreeFilter<'_> {
    fn accepts(&mut self, _index: usize, path: &Path, goal: Node) -> bool {
        // Last node should be our tree prop.
        debug_assert!(goal == *path.last().unwrap());

        if let Some(tree) = self.context.world().find_prop_for_cell(goal.cell, self.context.tile_map()) {
            if !tree.is_being_harvested() && tree.harvestable_amount() != 0 {
                return true; // Accept path.
            }
        }

        false // Refuse path.
    }

    fn shuffle(&mut self, nodes: &mut [Node]) {
        nodes.shuffle(self.context.rng_mut());
    }
}

impl UnitTaskHarvestWood {
    fn try_find_goal(&mut self, unit: &mut Unit, context: &SimContext) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();

        let bias = pathfind::Unbiased::new();
        let mut path_filter = FindHarvestableTreeFilter::new(context);

        // Find a harvestable tree node:
        let result = context.find_paths_to_node(
            &bias,
            &mut path_filter,
            traversable_node_kinds | PathNodeKind::HarvestableTree,
            start,
            PathNodeKind::HarvestableTree,
        );

        if let SearchResult::PathFound(path_to_harvestable_tree) = result {
            // Last node should be our tree prop.
            let tree_cell = path_to_harvestable_tree.last().unwrap().cell;

            if let Some(tree) = context.world_mut().find_prop_for_cell_mut(tree_cell, context.tile_map_mut()) {
                // Filter should only accept paths to trees that are not being harvested by another unit.
                debug_assert!(!tree.is_being_harvested());

                // Tree tile itself is not walkable, so find a nearby traversable neighbor we can path to.
                pathfind::for_each_surrounding_cell(tree.cell_range(), |neighbor_cell| {
                    let node_kind = context.graph().node_kind(Node::new(neighbor_cell)).unwrap();

                    // Find a nearby node we can traverse/reach:
                    if node_kind.intersects(traversable_node_kinds) {
                        // Trace new path to reachable empty node:
                        if let SearchResult::PathFound(dest_path) =
                            context.find_path(traversable_node_kinds, start, neighbor_cell)
                        {
                            // Call dibs on this tree.
                            tree.set_harvester_unit(unit.id());
                            self.harvest_target = tree.id();

                            // Time it takes to harvest a tree.
                            self.harvest_timer.reset(WOOD_HARVEST_TIME_INTERVAL);

                            unit.move_to_goal(dest_path, UnitNavGoal::tile(start, dest_path));
                            return false; // done
                        }
                    }
                    true // continue to next neighbor
                });
            }
        }
    }

    fn try_return_to_origin(&mut self, unit: &mut Unit, context: &SimContext) -> bool {
        if context.world().find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::warning!(log::channel!("task"), "Origin building is no longer valid! TaskHarvestWood will abort.");
            return false;
        }

        let start = unit.cell();
        let goal = self.origin_building_tile.road_link;
        let traversable_node_kinds = unit.traversable_node_kinds();

        match context.find_path(traversable_node_kinds, start, goal) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                unit.move_to_goal(path, goal);
                self.is_returning_to_origin = true;
                true
            }
            SearchResult::PathNotFound => {
                log::warning!(log::channel!("task"), "Origin building is no longer reachable! TaskHarvestWood will abort.");
                false
            }
        }
    }
}

impl UnitTask for UnitTaskHarvestWood {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn initialize(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) {
        debug_assert!(!self.harvest_target.is_valid());
        debug_assert!(!self.is_returning_to_origin);

        // Harvesters can go off-road.
        let current_node_kinds = unit.traversable_node_kinds();
        unit.set_traversable_node_kinds(
            current_node_kinds
                | PathNodeKind::EmptyLand
                | PathNodeKind::Road
                | PathNodeKind::VacantLot
                | PathNodeKind::SettlersSpawnPoint,
        );

        self.try_find_goal(unit, context);
    }

    fn terminate(&mut self, task_pool: &mut UnitTaskPool) {
        if let Some(task_id) = self.completion_task {
            task_pool.free(task_id);
        }
    }

    fn update(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) -> UnitTaskState {
        match self.internal_state {
            UnitTaskHarvestState::PendingCompletionCallback => {
                // Wait for the deferred completion callback to be executed.
                return UnitTaskState::Running;
            }
            UnitTaskHarvestState::Completed => {
                // Deferred completion callback has run; end the task.
                return UnitTaskState::Completed;
            }
            UnitTaskHarvestState::Running => {}
        }

        // If we have a goal we're already moving somewhere,
        // otherwise we may need to pathfind again.
        if unit.goal().is_none() {
            if self.is_returning_to_origin {
                if !self.try_return_to_origin(unit, context) {
                    // Not possible to recover if the origin building is gone.
                    log::warning!(log::channel!("task"), "Aborting TaskHarvestWood. Unable to return to origin building...");
                    unit.clear_inventory();
                    return UnitTaskState::Completed;
                }
            } else {
                self.try_find_goal(unit, context);
            }
        }

        if unit.has_reached_goal() { UnitTaskState::Completed } else { UnitTaskState::Running }
    }

    fn completed(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        // If the deferred completion callback has already run, finalize the task.
        if self.internal_state == UnitTaskHarvestState::Completed {
            if !unit.inventory_is_empty() {
                log::warning!(log::channel!("task"), "TaskHarvestWood: Failed to unload all resources.");
                unit.clear_inventory();
            }
            return UnitTaskResult::completed_with(&mut self.completion_task);
        }

        let mut task_completed = false;

        if self.is_returning_to_origin {
            debug_assert!(
                unit.goal().is_some_and(|goal| {
                    goal.is_building()
                        && goal.building_destination() == (self.origin_building.kind, self.origin_building_tile.base_cell)
                }),
                "Unit goal is not its origin building!"
            );

            // We've reached our origin building with the resources we were supposed to
            // harvest. Invoke the completion callback and end the task.
            debug_assert!(!unit.inventory_is_empty());
            debug_assert!(unit.peek_inventory().unwrap().kind == ResourceKind::Wood);

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
                            .expect("Expected unit to be running UnitTaskHarvestWood!");

                        debug_assert_eq!(task.internal_state, UnitTaskHarvestState::PendingCompletionCallback);
                        task.internal_state = UnitTaskHarvestState::Completed;
                    },
                );

                if scheduled {
                    // Wait for deferred callback to complete before ending the task.
                    self.internal_state = UnitTaskHarvestState::PendingCompletionCallback;
                    unit.follow_path(None);
                    return UnitTaskResult::Retry;
                }
            }

            if !unit.inventory_is_empty() {
                log::warning!(log::channel!("task"), "TaskHarvestWood: Failed to unload all resources.");
                unit.clear_inventory();
            }

            task_completed = true;
            unit.follow_path(None);
        } else {
            debug_assert!(unit.inventory_is_empty());

            fn reroute(unit: &mut Unit, harvest_target: &mut PropId) {
                unit.follow_path(None);
                *harvest_target = PropId::invalid();
            }

            // Reached the tree we wanted to harvest.
            if let Some(tree) = context.world_mut().find_prop_mut(self.harvest_target) {
                if tree.harvester_unit() == unit.id() {
                    // Once enough time has elapsed, give it the harvested wood.
                    if self.harvest_timer.tick(context.delta_time_secs()) {
                        // Finished.
                        let harvest_amount = context.random_range(1..WOOD_HARVEST_MAX_AMOUNT);
                        let harvested_resource = tree.harvest(context, harvest_amount);

                        debug_assert!(harvested_resource.kind == ResourceKind::Wood, "Expected to have ResourceKind::Wood");
                        unit.receive_resources(harvested_resource.kind, harvested_resource.count);

                        tree.set_harvester_unit(UnitId::invalid());
                        self.harvest_target = PropId::invalid();

                        // If we couldn't find a path back to the origin, maybe because the origin
                        // building was destroyed, we'll have to abort the task. Any
                        // resources harvested will be lost.
                        if !self.try_return_to_origin(unit, context) {
                            // Not possible to recover if the origin building is gone.
                            log::warning!(
                                log::channel!("task"),
                                "Aborting TaskHarvestWood. Unable to return to origin building..."
                            );
                            unit.clear_inventory();
                            task_completed = true;
                        }
                    }
                } else {
                    reroute(unit, &mut self.harvest_target);
                }
            } else {
                reroute(unit, &mut self.harvest_target);
            }
        }

        if task_completed {
            UnitTaskResult::completed_with(&mut self.completion_task)
        } else {
            UnitTaskResult::Retry
        }
    }

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building         : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("Internal State          : {:?}", self.internal_state));
        ui.text(format!("Is Returning To Origin  : {}", self.is_returning_to_origin));
        ui.separator();
        ui.text(format!("Harvest Target          : {}", self.harvest_target));
        ui.text(format!("Harvest Countdown Timer : {:.2}", self.harvest_timer.remaining_secs()));
        ui.separator();
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));
    }
}
