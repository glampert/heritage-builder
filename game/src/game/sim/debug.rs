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
    tile::{sets::TileSets, TileMap},
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

pub struct DebugContext<'game, 'tile_sets> {
    pub ui_sys: &'game UiSystem,
    pub world: &'game mut World,
    pub systems: &'game mut GameSystems,
    pub tile_map: &'game mut TileMap<'tile_sets>,
    pub tile_sets: &'tile_sets TileSets,
    pub transform: WorldToScreenTransform,
    pub delta_time_secs: Seconds,
}

// ----------------------------------------------
// DebugQueryBuilder
// ----------------------------------------------

// Dummy Query for unit tests/debug.
pub struct DebugQueryBuilder<'tile_sets, 'tile_map> {
    rng: RandomGenerator,
    graph: Graph,
    search: Search,
    task_manager: UnitTaskManager,
    world: mem::RawPtr<World>,
    tile_map: &'tile_map mut TileMap<'tile_sets>,
    tile_sets: &'tile_sets TileSets,
}

impl<'tile_sets, 'tile_map> DebugQueryBuilder<'tile_sets, 'tile_map> {
    pub fn new(world: &mut World,
               tile_map: &'tile_map mut TileMap<'tile_sets>,
               tile_sets: &'tile_sets TileSets,
               map_size_in_cells: Size) -> Self {
        Self {
            rng: RandomGenerator::seed_from_u64(GameConfigs::get().sim.random_seed),
            graph: Graph::with_empty_grid(map_size_in_cells),
            search: Search::with_grid_size(map_size_in_cells),
            task_manager: UnitTaskManager::new(1),
            world: mem::RawPtr::from_ref(world),
            tile_map,
            tile_sets,
        }
    }

    pub fn new_query(&mut self) -> Query<'tile_sets> {
        Query::new(&mut self.rng,
                   &mut self.graph,
                   &mut self.search,
                   &mut self.task_manager,
                   &mut self.world,
                   self.tile_map,
                   self.tile_sets,
                   0.0)
    }
}
