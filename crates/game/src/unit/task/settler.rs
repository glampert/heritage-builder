use std::any::Any;
use serde::{Deserialize, Serialize};

use common::callback::Callback;
use engine::{log, ui::UiSystem};

use super::{
    UnitTask,
    UnitTaskId,
    UnitTaskPool,
    UnitTaskResult,
    UnitTaskState,
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
    sim::{SimCmds, SimContext},
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
    #[default]
    Idle,
    MovingToGoal(UnitTaskSettlerGoal),
    PendingBuildingVisit,
    BuildingVisited { settler_accepted: bool },
    Completed,
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

    // Current internal settler task state. Should start as Idle.
    // Deserialize uses Default if missing to retain backwards compatibility with older save files.
    #[serde(default)]
    pub internal_state: UnitTaskSettlerState,
}

impl UnitTaskSettler {
    fn try_find_goal(&mut self, unit: &mut Unit, context: &SimContext) {
        let start = unit.cell();
        let traversable_node_kinds = unit.traversable_node_kinds();
        let bias = RandomDirectionalBias::new(context.rng_mut(), 0.1, 0.5);

        // First try to find an empty lot we can settle:
        {
            let result =
                context.find_path_to_node(&bias, traversable_node_kinds, start, PathNodeKind::VacantLot);

            if let SearchResult::PathFound(path) = result {
                unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                self.internal_state = UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::VacantLot);
                return;
            }
        }

        // Alternatively try to find a house with room that can take this settler.
        if self.fallback_to_houses_with_room {
            let result = context.find_nearest_buildings(
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
                unit.move_to_goal(
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
                self.internal_state = UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::House);
                return;
            }
        }

        // If we can't find any viable destination, move back to the settler spawn point and abort.
        if self.return_to_spawn_point_if_failed {
            let result =
                context.find_path_to_node(&bias, traversable_node_kinds, start, PathNodeKind::SettlersSpawnPoint);

            if let SearchResult::PathFound(path) = result {
                unit.move_to_goal(path, UnitNavGoal::tile(start, path));
                self.internal_state = UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::SpawnPointExit);
            }
        }
    }

    fn notify_completion(&mut self, unit: &mut Unit, tile: &Tile, context: &SimContext) {
        if self.completion_callback.is_valid() {
            let callback = self.completion_callback.get();
            callback(context, unit, tile, self.population_to_add);
        }

        debug_assert_ne!(self.internal_state, UnitTaskSettlerState::Completed);
        self.internal_state = UnitTaskSettlerState::Completed;
    }
}

impl UnitTask for UnitTaskSettler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn post_load(&mut self) {
        self.completion_callback.post_load();
    }

    fn initialize(&mut self, unit: &mut Unit, _cmds: &mut SimCmds, context: &SimContext) {
        debug_assert!(self.population_to_add != 0);
        debug_assert_eq!(self.internal_state, UnitTaskSettlerState::Idle);

        // Settlers can go off-road.
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
            UnitTaskSettlerState::Idle | UnitTaskSettlerState::MovingToGoal(_) => {
                if unit.goal().is_none() {
                    self.try_find_goal(unit, context);
                }

                if unit.has_reached_goal() {
                    UnitTaskState::Completed
                } else {
                    UnitTaskState::Running
                }
            }
            UnitTaskSettlerState::PendingBuildingVisit => {
                // Wait for building visited callback to be invoked.
                UnitTaskState::Running
            }
            UnitTaskSettlerState::BuildingVisited { settler_accepted } => {
                if settler_accepted {
                    // House accepted the setter. Task finished.
                    UnitTaskState::Completed
                } else {
                    // Else we have to try another house.
                    unit.follow_path(None);
                    self.internal_state = UnitTaskSettlerState::Idle;
                    UnitTaskState::Running
                }
            }
            UnitTaskSettlerState::Completed => {
                // Shouldn't ever be reached. We won't update the task if state == Completed.
                panic!("Unexpected UnitTaskSettlerState: Completed");
            }
        }
    }

    fn completed(&mut self, unit: &mut Unit, cmds: &mut SimCmds, context: &SimContext) -> UnitTaskResult {
        let unit_goal = unit.goal().expect("Expected unit to have an active goal!");

        if let UnitTaskSettlerState::BuildingVisited { settler_accepted } = self.internal_state {
            // Completed a deferred Building::visited_by command.
            // If the house accepted the settler we finish the task.
            if settler_accepted {
                let (destination_kind, destination_cell) = unit_goal.building_destination();
                debug_assert!(destination_kind == BuildingKind::House);
                debug_assert!(destination_cell.is_valid());

                if let Some(house_tile) = context.find_tile(destination_cell, TileKind::Building) {
                    self.notify_completion(unit, house_tile, context);
                    return UnitTaskResult::completed_with(&mut self.completion_task);
                }
            }
        } else if unit_goal.is_tile() {
            // Moving to a vacant lot or back to spawn point:
            if !matches!(self.internal_state,
                UnitTaskSettlerState::Idle | // NOTE: Allow Idle for backwards compatibility with old saves that don't have internal_state.
                UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::VacantLot) |
                UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::SpawnPointExit)
            ) {
                log::error!(
                    log::channel!("task"),
                    "Expected UnitTaskSettlerState to be MovingToGoal(VacantLot | SpawnPointExit), found: {:?}",
                    self.internal_state,
                );
            }

            let destination_cell = unit_goal.tile_destination();
            debug_assert!(destination_cell.is_valid());

            if let Some(tile) = context.tile_map().try_tile_from_layer(destination_cell, TileMapLayerKind::Terrain) {
                if tile.path_kind().is_vacant_lot() || tile.has_flags(TileFlags::SettlersSpawnPoint) {
                    // Notify completion:
                    self.notify_completion(unit, tile, context);
                    return UnitTaskResult::completed_with(&mut self.completion_task);
                }
            }
        } else if unit_goal.is_building() {
            // Moving to a house with room to take a new settler:
            debug_assert!(self.fallback_to_houses_with_room);

            if !matches!(self.internal_state,
                UnitTaskSettlerState::Idle | // NOTE: Allow Idle for backwards compatibility with old saves that don't have internal_state.
                UnitTaskSettlerState::MovingToGoal(UnitTaskSettlerGoal::House)
            ) {
                log::error!(
                    log::channel!("task"),
                    "Expected UnitTaskSettlerState to be MovingToGoal(House), found: {:?}",
                    self.internal_state,
                );
            }

            let (destination_kind, destination_cell) = unit_goal.building_destination();
            debug_assert!(destination_kind == BuildingKind::House);
            debug_assert!(destination_cell.is_valid());

            // Visit destination building:
            if let Some(house_building) = context.world().find_building_for_cell(destination_cell, context.tile_map())
                && house_building.kind() == destination_kind
            {
                cmds.visit_building_with_completion(house_building.kind_and_id(), unit.id(),
                    |context, _building, unit, result| {
                        // House accepted the setter. Task will complete.
                        let settler_accepted = result == BuildingVisitResult::Accepted;

                        let task = unit.current_task_as_mut::<Self>(context.task_manager_mut())
                            .expect("Expected unit to be running UnitTaskSettler!");

                        debug_assert_eq!(task.internal_state, UnitTaskSettlerState::PendingBuildingVisit);
                        task.internal_state = UnitTaskSettlerState::BuildingVisited { settler_accepted };
                    });

                // Waiting to complete a building visit or not reached a valid goal yet; Retry.
                self.internal_state = UnitTaskSettlerState::PendingBuildingVisit;
                return UnitTaskResult::Retry;
            }
        }

        // Failed; Retry.
        unit.follow_path(None);
        self.internal_state = UnitTaskSettlerState::Idle;

        UnitTaskResult::Retry
    }

    fn draw_debug_ui(&mut self, unit: &mut Unit, _context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        ui.text(format!("Population To Add               : {}", self.population_to_add));
        ui.text(format!("Internal State                  : {:?}", self.internal_state));
        ui.separator();
        ui.text(format!("Fallback To Houses With Room    : {}", self.fallback_to_houses_with_room));
        ui.text(format!("Return To Spawn Point If Failed : {}", self.return_to_spawn_point_if_failed));
        ui.text(format!("Traversable Node Kinds          : {}", unit.traversable_node_kinds()));
    }
}
