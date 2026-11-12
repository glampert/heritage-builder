use rand::SeedableRng;
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};

use common::{
    Size,
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
    launch,
    save_context::*,
    debug::DebugUiMode,
    pathfind::Search,
    tile::{Tile, TileKind, TileMap},
};

pub mod commands;
pub use commands::{SimCmds, SimCmdQueue};

pub mod context;
pub use context::{SimContext, SimContextMut};

pub mod resources;

pub mod finance;
pub use finance::Finance;

pub mod risks;
pub use risks::CityRisks;

pub mod calendar;
pub use calendar::{CalendarEvents, CalendarSettings, GameCalendar};

// ----------------------------------------------
// WorldStep
// ----------------------------------------------

// A fixed step of the world and game systems update (see `Simulation::advance_clocks`).
pub struct WorldStep {
    pub delta_time_secs: Seconds,
    pub calendar_events: CalendarEvents,
}

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
    paused_update_timer: UpdateTimer,

    #[serde(skip)]
    cmds: RcMut<SimCmds>,
    task_manager: UnitTaskManager,
    finance: Finance,
    calendar: GameCalendar,
    risks: CityRisks,

    // Path finding:
    #[serde(skip)]
    search: Search,

    // Sim speed:
    speed: f32,
    is_paused: bool,

    // Simulated seconds since this game started or was loaded: frame time scaled by the speed,
    // not counting pauses. Runtime only (automation waits on it), so it isn't saved.
    #[serde(skip)]
    elapsed_secs: f64,
}

impl Simulation {
    pub fn new(map_size_in_cells: Size, configs: &GameConfigs) -> Self {
        Self {
            rng: RcMut::new(RandomGenerator::seed_from_u64(launch::get().random_seed(configs))),
            update_timer: UpdateTimer::new(configs.sim.update_frequency_secs),
            paused_update_timer: UpdateTimer::new(configs.sim.paused_update_frequency_secs),
            cmds: RcMut::new(commands::DeferredSimCmds::new(SIM_CMDS_CAPACITY)),
            task_manager: UnitTaskManager::new(UNIT_TASK_POOL_CAPACITY),
            finance: Finance::new(configs.sim.starting_gold_units, &configs.sim.finance),
            calendar: GameCalendar::new(configs.sim.calendar.clone()),
            risks: CityRisks::new(&configs.sim.risks),
            search: Search::with_grid_size(map_size_in_cells),
            speed: configs.sim.default_speed.clamp(Self::MIN_SIM_SPEED, Self::MAX_SIM_SPEED),
            is_paused: false,
            elapsed_secs: 0.0,
        }
    }

    // A mutable context over this simulation, the world and the tile map, for edits made outside
    // the world step (the player's tools, automation, tests). It borrows all three for as long
    // as it lives.
    #[inline]
    pub fn new_sim_context<'game>(
        &'game mut self,
        delta_time_secs: Seconds,
        tile_map: &'game mut TileMap,
        world: &'game mut World,
    ) -> SimContextMut<'game> {
        // SAFETY: every part is borrowed for 'game.
        unsafe { SimContextMut::from_raw(self.context_parts(tile_map, world), delta_time_secs, false) }
    }

    #[inline]
    fn context_parts<'a>(&'a mut self, tile_map: &'a mut TileMap, world: &'a mut World) -> context::SimContextParts<'a> {
        context::SimContextParts {
            rng: &mut self.rng,
            search: &mut self.search,
            task_manager: &mut self.task_manager,
            world,
            tile_map,
            finance: &mut self.finance,
            calendar: &mut self.calendar,
            risks: &mut self.risks,
            cmds: &mut self.cmds,
        }
    }

    // Contexts over parts the caller goes on using while the context lives: the world updates its
    // game objects mutably while they read it through the context, and SimCmds run against the
    // simulation that owns them.
    //
    // SAFETY: the caller must drop the context before the simulation, the world or the tile map
    // is moved or dropped, and must not let it escape the call that uses it.
    #[inline]
    unsafe fn aliased_context<'a>(
        &mut self,
        delta_time_secs: Seconds,
        tile_map: &mut TileMap,
        world: &mut World,
    ) -> SimContext<'a> {
        unsafe { SimContext::from_raw(self.context_parts(tile_map, world), delta_time_secs, false) }
    }

    // SAFETY: as aliased_context.
    #[inline]
    unsafe fn aliased_context_mut<'a>(
        &mut self,
        delta_time_secs: Seconds,
        tile_map: &mut TileMap,
        world: &mut World,
        is_world_teardown: bool,
    ) -> SimContextMut<'a> {
        unsafe { SimContextMut::from_raw(self.context_parts(tile_map, world), delta_time_secs, is_world_teardown) }
    }

    #[inline]
    pub fn cmds(&self) -> &RcMut<SimCmds> {
        &self.cmds
    }

    #[inline]
    pub fn finance(&self) -> &Finance {
        &self.finance
    }

    #[inline]
    pub fn finance_mut(&mut self) -> &mut Finance {
        &mut self.finance
    }

    #[inline]
    pub fn risks(&self) -> &CityRisks {
        &self.risks
    }

    #[inline]
    pub fn risks_mut(&mut self) -> &mut CityRisks {
        &mut self.risks
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
    pub fn calendar(&self) -> &GameCalendar {
        &self.calendar
    }

    #[inline]
    pub fn calendar_mut(&mut self) -> &mut GameCalendar {
        &mut self.calendar
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
        // Collapses the developer inspector asked for last frame (building::destruction::request).
        if crate::building::destruction::has_requests() {
            let context = self.new_sim_context(0.0, tile_map, world);
            crate::building::destruction::run_requests(&context);
        }

        // Paused simulation update.
        if self.is_paused {
            if self.paused_update_timer.tick(delta_time_secs).should_update() {
                let context = self.new_sim_context(delta_time_secs, tile_map, world);
                systems.paused_update(engine, &context);
            }
            return; // Early out.
        }

        let scaled_delta_time_secs = delta_time_secs * self.speed;

        // Units movement needs to be smooth, so it updates every frame.
        self.update_unit_navigation(world, tile_map, scaled_delta_time_secs);

        // Fixed step world & game systems update.
        if let Some(step) = self.advance_clocks(scaled_delta_time_secs) {
            self.step_world(world, tile_map, step.delta_time_secs, |cmds, context| {
                systems.update(engine, cmds, context);
                systems.on_calendar_events(engine, cmds, context, &step.calendar_events);
            });

            // Month-end charges and the new year's accounts, once the world is unlocked.
            self.finance.on_calendar_events(&step.calendar_events, world, tile_map);
        }
    }

    // Moves the units along their paths. Runs every frame, for smooth movement.
    pub fn update_unit_navigation(&mut self, world: &mut World, tile_map: &mut TileMap, delta_time_secs: Seconds) {
        // SAFETY: the world moves its units through the context; it's dropped on return.
        let context = unsafe { self.aliased_context_mut(delta_time_secs, tile_map, world, false) };
        world.update_unit_navigation(&context);
    }

    // One world step: the world's update and `update_systems` on a read-only context, with the
    // world and tile map locked in debug builds, then the SimCmds they queued on a mutable one.
    // Whatever changes the world, the tile map or the simulation's state during the update goes
    // through SimCmds. The test harness steps the world through here too.
    pub fn step_world(
        &mut self,
        world: &mut World,
        tile_map: &mut TileMap,
        delta_time_secs: Seconds,
        update_systems: impl FnOnce(&mut SimCmds, &SimContext),
    ) {
        const LOCK_WORLD_AND_MAP_DURING_UPDATE: bool = cfg!(debug_assertions);

        debug_assert!(self.cmds.is_empty());

        if LOCK_WORLD_AND_MAP_DURING_UPDATE {
            tile_map.lock();
            world.lock();
        }

        // Update world and game systems. Cross entity mutation will be deferred into SimCmds.
        {
            // SAFETY: the world updates its game objects while they read it through the
            // context; it's dropped at the end of this block.
            let context = unsafe { self.aliased_context(delta_time_secs, tile_map, world) };
            world.update(&mut self.cmds, &context);
            update_systems(&mut self.cmds, &context);
        }

        if LOCK_WORLD_AND_MAP_DURING_UPDATE {
            world.unlock();
            tile_map.unlock();
        }

        // Any world or tile map mutation would have been deferred until now.
        {
            // SAFETY: the commands run against the simulation that owns them; the context is
            // dropped at the end of this block.
            let context = unsafe { self.aliased_context_mut(delta_time_secs, tile_map, world, false) };
            self.cmds.execute(&context);
        }

        debug_assert!(self.cmds.is_empty());
    }

    // Advances the simulation's clocks by one frame of simulated time (frame time times the
    // speed). Returns the world step when one is due: the simulated time since the last step,
    // this frame included, and the calendar events it crossed. The calendar advances by the same
    // steps, so it, the world and `elapsed_secs` all count the same time.
    pub fn advance_clocks(&mut self, scaled_delta_time_secs: Seconds) -> Option<WorldStep> {
        self.elapsed_secs += f64::from(scaled_delta_time_secs);

        // The timer accumulates simulated time already, and doesn't add the frame it fires on.
        let delta_time_secs = self.update_timer.time_since_last_secs() + scaled_delta_time_secs;

        if !self.update_timer.tick(scaled_delta_time_secs).should_update() {
            return None;
        }

        let calendar_events = self.calendar.advance(delta_time_secs, GameConfigs::get().sim.seconds_per_month);
        Some(WorldStep { delta_time_secs, calendar_events })
    }

    pub fn reset_world(
        &mut self,
        engine: &mut Engine,
        world: &mut World,
        systems: &mut GameSystems,
        tile_map: &mut TileMap,
    ) {
        {
            // SAFETY: the world despawns its game objects through the context; it's dropped at
            // the end of this block.
            let context = unsafe { self.aliased_context_mut(0.0, tile_map, world, true) };
            world.reset(&context);
        }
        systems.reset(engine);
        self.cmds.reset();
        self.elapsed_secs = 0.0;

        let calendar_settings = self.calendar.settings().clone();
        self.calendar.restart(calendar_settings);
    }

    pub fn reset_search_graph(&mut self, tile_map: &mut TileMap) {
        tile_map.reset_search_graph();
        self.search = Search::with_graph(tile_map.graph());
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

    #[inline]
    pub fn speed_limits(&self) -> (f32, f32) {
        (Self::MIN_SIM_SPEED, Self::MAX_SIM_SPEED)
    }

    // Clamped to speed_limits().
    #[inline]
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(Self::MIN_SIM_SPEED, Self::MAX_SIM_SPEED);
    }

    // Simulated seconds since this game started or was loaded (see `elapsed_secs`).
    #[inline]
    pub fn elapsed_secs(&self) -> f64 {
        self.elapsed_secs
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
        context.world.draw_debug_ui(&mut self.finance, &mut self.risks, context.camera, context.ui_sys);
    }

    // Game Systems:
    pub fn draw_game_systems_debug_ui(
        &mut self,
        context: &mut GameUiContext,
        engine: &mut Engine,
        systems: &mut GameSystems,
    ) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        let mut cmds = commands::ImmediateModeSimCmds::new(&sim_context);

        systems.draw_debug_ui(engine, &mut cmds, &sim_context, context.ui_sys);
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
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        context.world.draw_building_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_building_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        let mut cmds = commands::ImmediateModeSimCmds::new(&sim_context);

        context.world.draw_building_debug_ui(&mut cmds, &sim_context, context.ui_sys, tile, mode);
    }

    // Units:
    fn draw_unit_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        context.world.draw_unit_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_unit_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        let mut cmds = commands::ImmediateModeSimCmds::new(&sim_context);

        context.world.draw_unit_debug_ui(&mut cmds, &sim_context, context.ui_sys, tile, mode);
    }

    // Props:
    fn draw_prop_debug_popups(&mut self, context: &mut GameUiContext, visible_range: CellRange) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        context.world.draw_prop_debug_popups(&sim_context, context.ui_sys, context.camera.transform(), visible_range);
    }

    fn draw_prop_debug_ui(&mut self, context: &mut GameUiContext, tile: &Tile, mode: DebugUiMode) {
        // SAFETY: the world draws its game objects' debug UI, which edits them through the context;
        // it's dropped on return.
        let sim_context = unsafe { self.aliased_context_mut(context.delta_time_secs, context.tile_map, context.world, false) };
        let mut cmds = commands::ImmediateModeSimCmds::new(&sim_context);

        context.world.draw_prop_debug_ui(&mut cmds, &sim_context, context.ui_sys, tile, mode);
    }
}

// ----------------------------------------------
// Save/Load for Simulation
// ----------------------------------------------

impl Save for Simulation {
    fn pre_save(&mut self, _context: &mut PreSaveContext) {
        debug_assert!(self.cmds.is_empty(), "Shouldn't have any pending sim commands when saving!");
    }

    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        debug_assert!(self.cmds.is_empty(), "Shouldn't have any pending sim commands when saving!");
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
        self.search = Search::with_graph(context.tile_map().graph());
        self.update_timer.post_load(context.configs().sim.update_frequency_secs);
        self.paused_update_timer.post_load(context.configs().sim.paused_update_frequency_secs);
        self.task_manager.post_load();
        self.cmds.post_load();
    }
}
