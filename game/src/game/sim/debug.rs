use rand::SeedableRng;

use crate::{
    imgui_ui::UiSystem,
    pathfind::{Graph, Search},
    tile::{TileMap, sets::TileSets},
    utils::{
        Size,
        Seconds,
        coords::WorldToScreenTransform
    },
    game::{
        constants::SIM_DEFAULT_RANDOM_SEED,
        unit::task::UnitTaskManager,
        system::GameSystems,
        world::World,
    }
};

use super::{
    Query,
    RandomGenerator
};

// ----------------------------------------------
// DebugUiMode
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DebugUiMode {
    Overview,
    Detailed,
}

// ----------------------------------------------
// DebugContext
// ----------------------------------------------

pub struct DebugContext<'config, 'ui, 'world, 'tile_map, 'tile_sets> {
    pub ui_sys: &'ui UiSystem,
    pub world: &'world mut World<'config>,
    pub systems: &'world mut GameSystems,
    pub tile_map: &'tile_map mut TileMap<'tile_sets>,
    pub tile_sets: &'tile_sets TileSets,
    pub transform: WorldToScreenTransform,
    pub delta_time_secs: Seconds,
}

// ----------------------------------------------
// DebugQueryBuilder
// ----------------------------------------------

// Dummy Query for unit tests/debug.
pub struct DebugQueryBuilder {
    rng: RandomGenerator,
    graph: Graph,
    search: Search,
    task_manager: UnitTaskManager,
}

impl DebugQueryBuilder {
    pub fn new(map_size_in_cells: Size) -> Self {
        Self {
            rng: RandomGenerator::seed_from_u64(SIM_DEFAULT_RANDOM_SEED),
            graph: Graph::with_empty_grid(map_size_in_cells),
            search: Search::with_grid_size(map_size_in_cells),
            task_manager: UnitTaskManager::new(1),
        }
    }

    pub fn new_query<'config, 'tile_sets>(&mut self,
                                          world: &mut World<'config>,
                                          tile_map: &mut TileMap<'tile_sets>,
                                          tile_sets: &'tile_sets TileSets) -> Query<'config, 'tile_sets> {
        Query::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            &mut self.task_manager,
            world,
            tile_map,
            tile_sets,
            world.building_configs(),
            world.unit_configs(),
            0.0)
    }
}
