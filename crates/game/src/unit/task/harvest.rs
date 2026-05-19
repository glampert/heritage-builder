use std::any::Any;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use common::{
    callback::Callback,
    mem::SingleThreadStatic,
    time::{CountdownTimer, Seconds},
};
use engine::{log, ui::UiSystem};

use super::{
    UnitTaskContext,
    UnitTaskState,
    UnitTaskTransition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    invoke_completion_callback_deferred,
    with_task,
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
    sim::{SimCmdQueue, SimContext, resources::ResourceKind},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskHarvestWood
// ----------------------------------------------

pub type UnitTaskHarvestCompletionCallback = fn(&SimContext, &mut Building, &mut Unit);

// How long it takes for a unit to complete a harvest once it arrives at a tree.
static WOOD_HARVEST_TIME_INTERVAL: SingleThreadStatic<Seconds> = SingleThreadStatic::new(20.0);

// Take a random range between 1 and this for the amount of wood harvested each time.
static WOOD_HARVEST_MAX_AMOUNT: SingleThreadStatic<u32> = SingleThreadStatic::new(5);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskHarvestState {
    // Looking for a harvestable tree.
    #[default]
    Searching,

    // Walking to the chosen tree.
    MovingToTree,

    // At the tree, ticking the harvest timer.
    Harvesting,

    // Waiting for the deferred harvest mutation to resolve.
    PendingHarvest,

    // Walking the harvested wood back to the origin building.
    ReturningToOrigin,

    // At the origin, waiting for the deferred completion callback.
    DeliveringToOrigin,

    // Terminal state.
    Done,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskHarvestWood {
    // Origin building info:
    pub origin_building: BuildingKindAndId,
    pub origin_building_tile: BuildingTileInfo,

    // Optional completion callback.
    // `|context, origin_building, harvester_unit|`
    pub completion_callback: Callback<UnitTaskHarvestCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // Internal - set to defaults.
    pub harvest_timer: CountdownTimer,
    pub harvest_target: PropId,

    #[serde(default)]
    pub state: UnitTaskHarvestState,

    // Set by the deferred harvest mutation; consumed by `PendingHarvest`.
    #[serde(skip)]
    pub harvest_done: bool,

    // Set by the deferred completion callback; consumed by `DeliveringToOrigin`.
    #[serde(skip)]
    pub completion_callback_done: bool,
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
        debug_assert_eq!(goal, *path.last().unwrap());

        if let Some(tree) = self.context.find_prop_for_cell(goal.cell) {
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
    pub fn set_harvest_time_interval(secs: Seconds) {
        WOOD_HARVEST_TIME_INTERVAL.set(secs);
    }

    pub fn set_max_harvest_amount(amount: u32) {
        WOOD_HARVEST_MAX_AMOUNT.set(amount);
    }

    // Finds a harvestable tree, calls dibs on it (deferred) and routes the unit
    // to a traversable cell next to it. Returns false if none was found.
    fn try_find_goal(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;
        let start = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        let bias = pathfind::Unbiased::new();
        let mut path_filter = FindHarvestableTreeFilter::new(sim_context);

        // Find a harvestable tree node:
        let result = sim_context.find_paths_to_node(
            &bias,
            &mut path_filter,
            traversable_node_kinds | PathNodeKind::HarvestableTree,
            start,
            PathNodeKind::HarvestableTree,
        );

        let SearchResult::PathFound(path_to_tree) = result else {
            return false;
        };

        // Last node should be our tree prop.
        let tree_cell = path_to_tree.last().unwrap().cell;

        let Some(tree) = sim_context.find_prop_for_cell(tree_cell) else {
            return false;
        };

        // Filter should only accept paths to trees that are not being harvested by another unit.
        debug_assert!(!tree.is_being_harvested());

        let tree_id = tree.id();
        let tree_range = tree.cell_range();
        let unit_id = ctx.unit.id();

        // Tree tile itself is not walkable, so find a nearby traversable neighbor we can path to.
        let mut dest_neighbor = None;
        pathfind::for_each_surrounding_cell(tree_range, |neighbor_cell| {
            let node_kind = sim_context.graph().node_kind(Node::new(neighbor_cell)).unwrap();
            if node_kind.intersects(traversable_node_kinds)
                && sim_context.find_path(traversable_node_kinds, start, neighbor_cell).found()
            {
                dest_neighbor = Some(neighbor_cell);
                return false; // done
            }
            true // continue to next neighbor
        });

        let Some(neighbor_cell) = dest_neighbor else {
            return false;
        };

        let SearchResult::PathFound(dest_path) =
            sim_context.find_path(traversable_node_kinds, start, neighbor_cell)
        else {
            return false;
        };

        // Call dibs on this tree (deferred).
        ctx.sim_cmds.defer_prop_update(tree_id, move |_ctx, tree| {
            tree.set_harvester_unit(unit_id);
        });

        self.harvest_target = tree_id;
        self.harvest_timer.reset(*WOOD_HARVEST_TIME_INTERVAL);
        ctx.unit.move_to_goal(dest_path, UnitNavGoal::tile(start, dest_path));
        true
    }

    fn try_return_to_origin(&mut self, ctx: &mut UnitTaskContext) -> bool {
        let sim_context = ctx.sim_context;

        if sim_context.find_building(self.origin_building.kind, self.origin_building.id).is_none() {
            log::warning!(log::channel!("task"), "Origin building is no longer valid! TaskHarvestWood will abort.");
            return false;
        }

        let start = ctx.unit.cell();
        let goal_cell = self.origin_building_tile.road_link;
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();

        match sim_context.find_path(traversable_node_kinds, start, goal_cell) {
            SearchResult::PathFound(path) => {
                let goal = UnitNavGoal::building(
                    self.origin_building.kind,
                    self.origin_building_tile.base_cell,
                    self.origin_building.kind,
                    self.origin_building_tile,
                );
                ctx.unit.move_to_goal(path, goal);
                true
            }
            SearchResult::PathNotFound => {
                log::warning!(log::channel!("task"), "Origin building is no longer reachable! TaskHarvestWood will abort.");
                false
            }
        }
    }

    // Schedules the deferred harvest mutation: the timer has elapsed,
    // so the tree is harvested and the wood handed to the unit.
    fn schedule_harvest(&mut self, ctx: &mut UnitTaskContext) {
        let harvest_amount = ctx.sim_context.random_range(1..*WOOD_HARVEST_MAX_AMOUNT);
        let unit_id = ctx.unit.id();
        let harvest_target = self.harvest_target;

        ctx.sim_cmds.defer_prop_update(harvest_target, move |context, tree| {
            let harvested_resource = tree.harvest(context, harvest_amount);
            debug_assert!(harvested_resource.kind == ResourceKind::Wood, "Expected to have ResourceKind::Wood");
            tree.set_harvester_unit(UnitId::invalid());

            let unit = context.find_unit_mut(unit_id).expect("Expected harvester unit to still be valid!");
            unit.receive_resources(harvested_resource.kind, harvested_resource.count);
            unit.follow_path(None);

            with_task::<UnitTaskHarvestWood>(unit, context, |task, _unit| {
                task.harvest_target = PropId::invalid();
                task.harvest_done = true;
            });
        });
    }

    // Schedules the deferred completion callback on the origin building.
    fn schedule_completion_callback(&mut self, ctx: &mut UnitTaskContext) -> bool {
        invoke_completion_callback_deferred(
            ctx.unit,
            ctx.sim_cmds,
            ctx.sim_context,
            self.origin_building.kind,
            self.origin_building.id,
            self.completion_callback.get(),
            |context, _building, unit| {
                with_task::<UnitTaskHarvestWood>(unit, context, |task, _unit| {
                    task.completion_callback_done = true;
                });
            },
        )
    }

    fn update_searching(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if self.try_find_goal(ctx) {
            UnitTaskTransition::Goto(UnitTaskHarvestState::MovingToTree)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_moving_to_tree(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if ctx.unit.goal().is_none() {
            // Lost the path; find another tree.
            self.harvest_target = PropId::invalid();
            return UnitTaskTransition::Goto(UnitTaskHarvestState::Searching);
        }

        if ctx.unit.has_reached_goal() {
            UnitTaskTransition::Goto(UnitTaskHarvestState::Harvesting)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_harvesting(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        // Is the tree still valid and still ours to harvest?
        let unit_id = ctx.unit.id();
        let tree_ok = ctx.sim_context
            .find_prop(self.harvest_target)
            .is_some_and(|tree| tree.harvester_unit() == unit_id);

        if !tree_ok {
            // Tree was removed or claimed by someone else; find another.
            ctx.unit.follow_path(None);
            self.harvest_target = PropId::invalid();
            return UnitTaskTransition::Goto(UnitTaskHarvestState::Searching);
        }

        // Once enough time has elapsed, harvest the tree.
        if self.harvest_timer.tick(ctx.sim_context.delta_time_secs()) {
            self.schedule_harvest(ctx);
            UnitTaskTransition::Goto(UnitTaskHarvestState::PendingHarvest)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_pending_harvest(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if self.harvest_done {
            UnitTaskTransition::Goto(UnitTaskHarvestState::ReturningToOrigin)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_returning(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if ctx.unit.goal().is_none() {
            if self.try_return_to_origin(ctx) {
                return UnitTaskTransition::Stay;
            }

            // Origin building is gone or unreachable; drop the wood and finish.
            log::warning!(log::channel!("task"), "Aborting TaskHarvestWood. Unable to return to origin building...");
            ctx.unit.clear_inventory();

            return UnitTaskTransition::Goto(UnitTaskHarvestState::Done);
        }

        if !ctx.unit.has_reached_goal() {
            return UnitTaskTransition::Stay;
        }

        // Reached the origin building with the harvested wood.
        debug_assert!(!ctx.unit.inventory_is_empty());
        debug_assert!(ctx.unit.peek_inventory().unwrap().kind == ResourceKind::Wood);
        ctx.unit.follow_path(None);

        if self.completion_callback.is_valid() && self.schedule_completion_callback(ctx) {
            UnitTaskTransition::Goto(UnitTaskHarvestState::DeliveringToOrigin)
        } else {
            // No callback, or origin building no longer exists.
            UnitTaskTransition::Goto(UnitTaskHarvestState::Done)
        }
    }

    fn update_delivering(&mut self, _ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if self.completion_callback_done {
            UnitTaskTransition::Goto(UnitTaskHarvestState::Done)
        } else {
            UnitTaskTransition::Stay
        }
    }

    fn update_done(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskTransition<UnitTaskHarvestState> {
        if !ctx.unit.inventory_is_empty() {
            log::warning!(log::channel!("task"), "TaskHarvestWood: Failed to unload all resources.");
            ctx.unit.clear_inventory();
        }
        UnitTaskTransition::Done
    }
}

impl UnitTaskState for UnitTaskHarvestState {
    type Task = UnitTaskHarvestWood;

    fn update(self, task: &mut UnitTaskHarvestWood, ctx: &mut UnitTaskContext) -> UnitTaskTransition<Self> {
        match self {
            Self::Searching          => task.update_searching(ctx),
            Self::MovingToTree       => task.update_moving_to_tree(ctx),
            Self::Harvesting         => task.update_harvesting(ctx),
            Self::PendingHarvest     => task.update_pending_harvest(ctx),
            Self::ReturningToOrigin  => task.update_returning(ctx),
            Self::DeliveringToOrigin => task.update_delivering(ctx),
            Self::Done               => task.update_done(ctx),
        }
    }
}

impl UnitTask for UnitTaskHarvestWood {
    type State = UnitTaskHarvestState;

    fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        debug_assert!(!self.harvest_target.is_valid());

        // Harvesters can go off-road.
        let current_node_kinds = ctx.unit.traversable_node_kinds();
        ctx.unit.set_traversable_node_kinds(
            current_node_kinds
            | PathNodeKind::EmptyLand
            | PathNodeKind::Road
            | PathNodeKind::VacantLot
            | PathNodeKind::SettlersSpawnPoint
        );
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

    fn draw_debug_ui(&mut self, _unit: &mut Unit, _sim_context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let building_kind = self.origin_building.kind;
        let building_cell = self.origin_building_tile.base_cell;
        let building_name = debug::tile_name_at(building_cell, TileMapLayerKind::Objects);

        ui.text(format!("Origin Building         : {}, '{}', {}", building_kind, building_name, building_cell));
        ui.text(format!("State                   : {:?}", self.state));
        ui.separator();
        ui.text(format!("Harvest Target          : {}", self.harvest_target));
        ui.text(format!("Harvest Countdown Timer : {:.2}", self.harvest_timer.remaining_secs()));
        ui.separator();
        ui.text(format!("Has Completion Callback : {}", self.completion_callback.is_valid()));
        ui.text(format!("Has Completion Task     : {}", self.completion_task.is_some()));
    }
}
