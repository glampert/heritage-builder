use rand::SeedableRng;
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};

use common::{
    mem::RcMut,
    coords::CellRange,
    time::{Seconds, UpdateTimer},
};
use engine::{Engine, save::*};

use super::{
    constants::*,
    world::World,
    system::GameSystems,
    config::GameConfigs,
    ui_context::GameUiContext,
    unit::task::UnitTaskManager,
};
use crate::{
    save_context::*,
    debug::DebugUiMode,
    pathfind::{Graph, Search},
    tile::{Tile, TileKind, TileMap},
};

pub mod commands;
pub use commands::SimCmds;

pub mod context;
pub use context::SimContext;

pub mod resources;
pub use resources::GlobalTreasury;

// ----------------------------------------------
// RandomGenerator
// ----------------------------------------------

pub type RandomGenerator = Pcg64;

// ----------------------------------------------
// Simulation
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Simulation {
    rng: RcMut<RandomGenerator>,

    update_timer: UpdateTimer,
    #[serde(default)] // Preserve save game backwards compatibility.
    paused_update_timer: UpdateTimer,

    task_manager: UnitTaskManager,
    treasury: GlobalTreasury,
    #[serde(skip)]
    cmds: SimCmds,

    // Path finding:
    graph: Graph,
    #[serde(skip)]
    search: Search,

    // Sim speed:
    speed: f32,
    is_paused: bool,
}

impl Simulation {
    pub fn new(tile_map: &TileMap, configs: &GameConfigs) -> Self {
        Self {
            rng: RcMut::new(RandomGenerator::seed_from_u64(configs.sim.random_seed)),
            update_timer: UpdateTimer::new(configs.sim.update_frequency_secs),
            paused_update_timer: UpdateTimer::new(configs.sim.paused_update_frequency_secs),
            task_manager: UnitTaskManager::new(UNIT_TASK_POOL_CAPACITY),
            treasury: GlobalTreasury::new(configs.sim.starting_gold_units),
            cmds: SimCmds::new(),
            graph: Graph::from_tile_map(tile_map),
            search: Search::with_grid_size(tile_map.size_in_cells()),
            speed: Self::MIN_SIM_SPEED,
            is_paused: false,
        }
    }

    #[inline]
    pub fn new_sim_context(&mut self, world: &mut World, tile_map: &mut TileMap, delta_time_secs: Seconds) -> SimContext {
        SimContext::new(
            &mut self.rng,
            &mut self.graph,
            &mut self.search,
            &mut self.task_manager,
            world,
            tile_map,
            &mut self.treasury,
            delta_time_secs,
        )
    }

    #[inline]
    pub fn treasury(&self) -> &GlobalTreasury {
        &self.treasury
    }

    #[inline]
    pub fn treasury_mut(&mut self) -> &mut GlobalTreasury {
        &mut self.treasury
    }

    #[inline]
    pub fn task_manager(&self) -> &UnitTaskManager {
        &self.task_manager
    }

    #[inline]
    pub fn task_manager_mut(&mut self) -> &mut UnitTaskManager {
        &mut self.task_manager
    }

    #[inline]
    pub fn rng(&self) -> &RcMut<RandomGenerator> {
        &self.rng
    }

    #[inline]
    pub fn rng_mut(&mut self) -> &mut RandomGenerator {
        &mut self.rng
    }

    pub fn update(
        &mut self,
        engine: &mut Engine,
        world: &mut World,
        systems: &mut GameSystems,
        tile_map: &mut TileMap,
        delta_time_secs: Seconds,
    ) {
        // Rebuild the search graph once every frame so any
        // add/remove tile changes will be reflected on the graph.
        //
        // FIXME (Perf): Should only rebuild the graph if the map was changed.
        // This can get quite expensive for large maps. Maybe could update
        // the graph on-the-spot when a change happens instead. Would avoid
        // this full map update pass altogether.
        self.graph.rebuild_from_tile_map(tile_map, true);

        // Paused simulation update:
        if self.is_paused {
            if self.paused_update_timer.tick(delta_time_secs).should_update() {
                let context = self.new_sim_context(world, tile_map, delta_time_secs);
                systems.paused_update(engine, &context);
            }
            return; // Early out.
        }

        let scaled_delta_time_secs = delta_time_secs * self.speed;

        // Units movement needs to be smooth, so it updates every frame.
        {
            let context = self.new_sim_context(world, tile_map, scaled_delta_time_secs);
            world.update_unit_navigation(&context);
        }

        // Fixed step world & systems update.
        {
            const LOCK_WORLD_AND_MAP_DURING_UPDATE: bool = true;

            let world_update_delta_time_secs = self.update_timer.time_since_last_secs() * self.speed;

            if self.update_timer.tick(scaled_delta_time_secs).should_update() {
                debug_assert!(self.cmds.is_empty());

                if cfg!(debug_assertions) && LOCK_WORLD_AND_MAP_DURING_UPDATE {
                    tile_map.lock();
                    world.lock();
                }

                let context = self.new_sim_context(world, tile_map, world_update_delta_time_secs);
                world.update(&mut self.cmds, &context);
                systems.update(engine, &mut self.cmds, &context);

                if cfg!(debug_assertions) && LOCK_WORLD_AND_MAP_DURING_UPDATE {
                    world.unlock();
                    tile_map.unlock();
                }

                // Any world or tile map mutation would have been deferred until now.
                self.cmds.execute(&context);
            }
        }
    }

    pub fn reset_world(
        &mut self,
        engine: &mut Engine,
        world: &mut World,
        systems: &mut GameSystems,
        tile_map: &mut TileMap,
    ) {
        let context = self.new_sim_context(world, tile_map, 0.0);
        world.reset(&context);
        systems.reset(engine);
        self.cmds.reset();
    }

    pub fn reset_search_graph(&mut self, tile_map: &TileMap) {
        self.graph  = Graph::from_tile_map(tile_map);
        self.search = Search::with_graph(&self.graph);
    }

    // ----------------------
    // Sim speed:
    // ----------------------

    const MIN_SIM_SPEED: f32 = 1.0;
    const MAX_SIM_SPEED: f32 = 10.0;

    #[inline]
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    #[inline]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    #[inline]
    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    #[inline]
    pub fn resume(&mut self) {
        self.is_paused = false;
    }

    #[inline]
    pub fn speedup(&mut self) {
        self.speed = (self.speed + 1.0).min(Self::MAX_SIM_SPEED);
    }

    #[inline]
    pub fn slowdown(&mut self) {
        self.speed = (self.speed - 1.0).max(Self::MIN_SIM_SPEED);
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        World::register_callbacks();
        GameSystems::register_callbacks();
    }

    // ----------------------
    // Debug:
    // ----------------------

    // World:
    pub fn draw_world_debug_ui(&mut self, context: &mut GameUiContext) {
        context.world.draw_debug_ui(&mut self.treasury, context.ui_sys);
    }

    // Game Systems:
    pub fn draw_game_systems_debug_ui(
        &mut self,
        context: &mut GameUiContext,
        engine: &mut Engine,
        systems: &mut GameSystems,
    ) {
        debug_assert!(self.cmds.is_empty());

        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        systems.draw_debug_ui(engine, &mut self.cmds, &sim_context, context.ui_sys);

        self.cmds.execute(&sim_context);
    }

    // Generic GameObjects:
    pub fn draw_game_object_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        if tile.is(TileKind::Building) {
            self.draw_building_debug_ui(context, tile, mode);
        } else if tile.is(TileKind::Unit) {
            self.draw_unit_debug_ui(context, tile, mode);
        } else if tile.is(TileKind::Prop) {
            self.draw_prop_debug_ui(context, tile, mode);
        }
    }

    pub fn draw_game_object_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        self.draw_building_debug_popups(context, visible_range);
        self.draw_unit_debug_popups(context, visible_range);
        self.draw_prop_debug_popups(context, visible_range);
    }

    // Buildings:
    fn draw_building_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_building_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_building_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_building_debug_ui(&sim_context, context.ui_sys, tile, mode);
    }

    // Units:
    fn draw_unit_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_unit_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_unit_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_unit_debug_ui(&sim_context, context.ui_sys, tile, mode);
    }

    // Props:
    fn draw_prop_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_prop_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_prop_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        let sim_context = self.new_sim_context(context.world, context.tile_map, context.delta_time_secs);
        context.world.draw_prop_debug_ui(&sim_context, context.ui_sys, tile, mode);
    }
}

// ----------------------------------------------
// Save/Load for Simulation
// ----------------------------------------------

impl Save for Simulation {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load for Simulation {
    fn pre_load(&mut self, _context: &mut PreLoadContext) {
        self.task_manager.pre_load();
        self.cmds.pre_load();
    }

    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &mut PostLoadContext) {
        self.search = Search::with_graph(&self.graph);
        self.update_timer.post_load(context.configs().sim.update_frequency_secs);
        self.paused_update_timer.post_load(context.configs().sim.paused_update_frequency_secs);
        self.task_manager.post_load();
        self.cmds.post_load();
    }
}
