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
pub struct DebugQueryBuilder<'config, 'tile_sets, 'tile_map> {
    rng: RandomGenerator,
    graph: Graph,
    search: Search,
    task_manager: UnitTaskManager,
    world: mem::RawPtr<World<'config>>,
    tile_map: &'tile_map mut TileMap<'tile_sets>,
    tile_sets: &'tile_sets TileSets,
}

impl<'config, 'tile_sets, 'tile_map> DebugQueryBuilder<'config, 'tile_sets, 'tile_map> {
    pub fn new(world: &mut World<'config>,
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

    pub fn new_query(&mut self) -> Query<'config, 'tile_sets> {
        let game_configs = GameConfigs::get();
        let building_configs = self.world.building_configs();
        let unit_configs = self.world.unit_configs();
        Query::new(&mut self.rng,
                   &mut self.graph,
                   &mut self.search,
                   &mut self.task_manager,
                   &mut self.world,
                   self.tile_map,
                   self.tile_sets,
                   game_configs,
                   building_configs,
                   unit_configs,
                   0.0)
    }
}
