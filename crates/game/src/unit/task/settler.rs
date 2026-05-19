use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::ui::UiSystem;

use super::{
    TaskContext,
    TaskState,
    Transition,
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    with_task,
};
use crate::{
    pathfind::{
        NodeKind as PathNodeKind,
        RandomDirectionalBias,
        SearchResult,
    },
    tile::{Tile, TileFlags, TileKind, TileMapLayerKind},
    unit::{Unit, navigation::UnitNavGoal},
    building::{BuildingKind, BuildingTileInfo, BuildingVisitResult},
    sim::{SimCmdQueue, SimContext},
    world::object::GameObject,
};

// ----------------------------------------------
// UnitTaskSettler
// ----------------------------------------------

pub type UnitTaskSettlerCompletionCallback = fn(&SimContext, &mut Unit, &Tile, u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskSettlerGoal {
    VacantLot,
    House,
    SpawnPointExit,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTaskSettlerState {
    // Looking for a vacant lot, a house with room, or the spawn point exit.
    #[default]
    Searching,

    // Walking to the chosen destination.
    MovingTo(UnitTaskSettlerGoal),

    // At a house, waiting for the deferred visit to resolve.
    VisitingHouse,

    // Settled (or left via the spawn point); terminal state.
    Done,
}

#[derive(Serialize, Deserialize)]
pub struct UnitTaskSettler {
    // Optional completion callback. Invoke with the empty house lot building we've visited.
    // |context, unit, dest_tile, population_to_add|
    pub completion_callback: Callback<UnitTaskSettlerCompletionCallback>,

    // Optional completion task to run after this task.
    pub completion_task: Option<UnitTaskId>,

    // If true and we can't find an empty lot, try to find any house with room that will take the settler.
    pub fallback_to_houses_with_room: bool,

    // If we can't find either an empty lot or a house, find a way back to the spawn point and leave.
    pub return_to_spawn_point_if_failed: bool,

    // Amount to add once settled into a new lot or house.
    pub population_to_add: u32,

    #[serde(default)]
    pub state: UnitTaskSettlerState,

    // Outcome of the deferred house visit, written by the visit callback and
    // consumed by `VisitingHouse`: `Some(true)` accepted, `Some(false)` refused.
    #[serde(skip)]
    pub visit_outcome: Option<bool>,
}

impl UnitTaskSettler {
    fn try_find_goal(&mut self, ctx: &mut TaskContext) -> Option<UnitTaskSettlerGoal> {
        let sim_context = ctx.sim_context;
        let start = ctx.unit.cell();
        let traversable_node_kinds = ctx.unit.traversable_node_kinds();
        let bias = RandomDirectionalBias::new(sim_context.rng_mut(), 0.1, 0.5);

        // First try to find an empty lot we can settle:
        {
            let result =
                sim_context.find_path_to_node(&bias, traversable_node_kinds, start, PathNodeKind::VacantLot);

            if let SearchResult::PathFound(path) = result {
                ctx.unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                return Some(UnitTaskSettlerGoal::VacantLot);
            }
        }

        // Alternatively try to find a house with room that can take this settler.
        if self.fallback_to_houses_with_room {
            let result = sim_context.find_nearest_buildings(
                start,
                BuildingKind::House,
                traversable_node_kinds,
                None,
                |building, _path| {
                    if let Some(population) = building.population() {
                        if !population.is_max() {
                            return false; // Accept this building and end the search.
                        }
                    }
                    true // Continue search.
                },
            );

            if let Some((building, path)) = result {
                ctx.unit.move_to_goal(
                    path,
                    UnitNavGoal::building(
                        BuildingKind::empty(), // Unused.
                        start,
                        building.kind(),
                        BuildingTileInfo {
                            // NOTE: Always use path goal cell; house may not be connected to a road, so we use any available access tile.
                            road_link: path.last().unwrap().cell,
                            base_cell: building.base_cell(),
                        },
                    ),
                );
                return Some(UnitTaskSettlerGoal::House);
            }
        }

        // If we can't find any viable destination, move back to the settler spawn point and abort.
        if self.return_to_spawn_point_if_failed {
            let result =
                sim_context.find_path_to_node(&bias, traversable_node_kinds, start, PathNodeKind::SettlersSpawnPoint);

            if let SearchResult::PathFound(path) = result {
                ctx.unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                return Some(UnitTaskSettlerGoal::SpawnPointExit);
            }
        }

        None
    }

    // Schedules the deferred house visit. Returns false if the house is no longer valid.
    fn try_visit_house(&mut self, ctx: &mut TaskContext) -> bool {
        let unit_goal = ctx.unit.goal().expect("Expected unit to have an active goal!");
        let (destination_kind, destination_cell) = unit_goal.building_destination();

        debug_assert!(destination_kind == BuildingKind::House);
        debug_assert!(destination_cell.is_valid());

        if let Some(house) = ctx.sim_context.find_building_for_cell(destination_cell)
            && house.kind() == destination_kind
        {
            let house_id = house.kind_and_id();
            let unit_id = ctx.unit.id();

            ctx.sim_cmds.visit_building_with_completion(house_id, unit_id, |context, _building, unit, result| {
                let accepted = result == BuildingVisitResult::Accepted;
                with_task::<UnitTaskSettler>(unit, context, |task, _unit| {
                    task.visit_outcome = Some(accepted);
                });
            });
            true
        } else {
            false
        }
    }

    fn notify_completion(&self, unit: &mut Unit, sim_context: &SimContext, tile: &Tile) {
        if self.completion_callback.is_valid() {
            let callback = self.completion_callback.get();
            callback(sim_context, unit, tile, self.population_to_add);
        }
    }

    fn update_searching(&mut self, ctx: &mut TaskContext) -> Transition<UnitTaskSettlerState> {
        match self.try_find_goal(ctx) {
            Some(goal) => Transition::Goto(UnitTaskSettlerState::MovingTo(goal)),
            None => Transition::Stay,
        }
    }

    fn update_moving(&mut self, goal: UnitTaskSettlerGoal, ctx: &mut TaskContext) -> Transition<UnitTaskSettlerState> {
        if ctx.unit.goal().is_none() {
            // Lost the path to the destination; search again.
            return Transition::Goto(UnitTaskSettlerState::Searching);
        }

        if !ctx.unit.has_reached_goal() {
            return Transition::Stay;
        }

        match goal {
            UnitTaskSettlerGoal::VacantLot | UnitTaskSettlerGoal::SpawnPointExit => {
                self.finish_at_tile(ctx)
            }
            UnitTaskSettlerGoal::House => {
                if self.try_visit_house(ctx) {
                    Transition::Goto(UnitTaskSettlerState::VisitingHouse)
                } else {
                    // House no longer valid; search for another destination.
                    ctx.unit.follow_path(None);
                    Transition::Goto(UnitTaskSettlerState::Searching)
                }
            }
        }
    }

    fn finish_at_tile(&mut self, ctx: &mut TaskContext) -> Transition<UnitTaskSettlerState> {
        let sim_context = ctx.sim_context;

        let destination_cell = ctx.unit.goal()
            .expect("Expected unit to have an active goal!")
            .tile_destination();
        debug_assert!(destination_cell.is_valid());

        if let Some(tile) = sim_context.try_tile_from_layer(destination_cell, TileMapLayerKind::Terrain) {
            if tile.path_kind().is_vacant_lot() || tile.has_flags(TileFlags::SettlersSpawnPoint) {
                self.notify_completion(ctx.unit, sim_context, tile);
                return Transition::Goto(UnitTaskSettlerState::Done);
            }
        }

        // Destination tile is no longer a vacant lot / spawn point; search again.
        ctx.unit.follow_path(None);
        Transition::Goto(UnitTaskSettlerState::Searching)
    }

    fn update_visiting_house(&mut self, ctx: &mut TaskContext) -> Transition<UnitTaskSettlerState> {
        match self.visit_outcome.take() {
            None => Transition::Stay, // Deferred visit not resolved yet.
            Some(true) => {
                // House accepted the settler.
                let sim_context = ctx.sim_context;
                let destination_cell = ctx.unit.goal()
                    .expect("Expected unit to have an active goal!")
                    .building_destination()
                    .1;

                if let Some(house_tile) = sim_context.find_tile(destination_cell, TileKind::Building) {
                    self.notify_completion(ctx.unit, sim_context, house_tile);
                }
                Transition::Goto(UnitTaskSettlerState::Done)
            }
            Some(false) => {
                // House refused the settler; search for another destination.
                ctx.unit.follow_path(None);
                Transition::Goto(UnitTaskSettlerState::Searching)
            }
        }
    }
}

impl TaskState for UnitTaskSettlerState {
    type Task = UnitTaskSettler;

    fn update(self, task: &mut UnitTaskSettler, ctx: &mut TaskContext) -> Transition<Self> {
        match self {
            Self::Searching      => task.update_searching(ctx),
            Self::MovingTo(goal) => task.update_moving(goal, ctx),
            Self::VisitingHouse  => task.update_visiting_house(ctx),
            Self::Done           => Transition::Done,
        }
    }
}

impl UnitTask for UnitTaskSettler {
    type State = UnitTaskSettlerState;

    fn initialize(&mut self, ctx: &mut TaskContext) {
        debug_assert_ne!(self.population_to_add, 0);

        // Settlers can go off-road.
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

    fn draw_debug_ui(&mut self, unit: &mut Unit, _sim_context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        ui.text(format!("Population To Add               : {}", self.population_to_add));
        ui.text(format!("State                           : {:?}", self.state));
        ui.separator();
        ui.text(format!("Fallback To Houses With Room    : {}", self.fallback_to_houses_with_room));
        ui.text(format!("Return To Spawn Point If Failed : {}", self.return_to_spawn_point_if_failed));
        ui.text(format!("Traversable Node Kinds          : {}", unit.traversable_node_kinds()));
    }
}
