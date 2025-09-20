use rand::SeedableRng;
use super::{Query, RandomGenerator};
use crate::{
    game::{
        system::GameSystems,
        unit::task::UnitTaskManager,
        world::World,
        GameConfigs,
    },
    imgui_ui::UiSystem,
    pathfind::{Graph, Search},
    tile::TileMap,
    engine::time::Seconds,
    utils::{mem, coords::WorldToScreenTransform, Size},
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

pub struct DebugContext<'game> {
    pub ui_sys: &'game UiSystem,
    pub world: &'game mut World,
    pub systems: &'game mut GameSystems,
    pub tile_map: &'game mut TileMap,
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
    world: mem::RawPtr<World>,
    tile_map: mem::RawPtr<TileMap>,
}

impl DebugQueryBuilder {
    pub fn new(world: &mut World, tile_map: &mut TileMap, map_size_in_cells: Size) -> Self {
        Self {
            rng: RandomGenerator::seed_from_u64(GameConfigs::get().sim.random_seed),
            graph: Graph::with_empty_grid(map_size_in_cells),
            search: Search::with_grid_size(map_size_in_cells),
            task_manager: UnitTaskManager::new(1),
            world: mem::RawPtr::from_ref(world),
            tile_map: mem::RawPtr::from_ref(tile_map),
        }
    }

    pub fn new_query(&mut self) -> Query {
        Query::new(&mut self.rng,
                   &mut self.graph,
                   &mut self.search,
                   &mut self.task_manager,
                   &mut self.world,
                   &mut self.tile_map,
                   0.0)
    }
}
