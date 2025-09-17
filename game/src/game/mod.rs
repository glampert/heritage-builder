use serde::{Deserialize, Serialize};

use crate::{
    app::ApplicationEvent,
    debug::{self, DebugMenusFrameArgs, DebugMenusInputArgs, DebugMenusSystem},
    engine::{self, Engine, EngineConfigs},
    log,
    render::TextureCache,
    save::{self, *},
    tile::{
        TileMap, camera::*, rendering::TileMapRenderFlags, selection::TileSelection, sets::TileSets,
    },
    utils::{self, Seconds, Size, Vec2, coords::CellRange, hash},
};

use building::config::BuildingConfigs;
use sim::Simulation;
use system::{GameSystems, settlers};
use unit::config::UnitConfigs;
use world::World;

pub mod building;
pub mod cheats;
pub mod constants;
pub mod sim;
pub mod system;
pub mod unit;
pub mod world;

// ----------------------------------------------
// GameConfigs
// ----------------------------------------------

// TODO: Deserialize with serde. Load from json config file.
pub struct GameConfigs {
    // Low-level Engine configs:
    pub engine: EngineConfigs,

    // Tile Map:
    pub load_map_setting: Option<LoadMapSetting>,

    // Camera:
    pub camera_zoom: f32,
    pub camera_offset: CameraOffset,

    // Simulation:
    pub sim_random_seed: u64,
    pub sim_update_frequency_secs: Seconds,

    // Workers/Population:
    pub workers_search_radius: i32,
    pub workers_update_frequency_secs: Seconds,

    // Game Systems:
    pub settlers_spawn_frequency_secs: Seconds,
    pub population_per_settler_unit: u32,

    // Debug:
    pub show_debug_popups: bool,
}

pub enum LoadMapSetting {
    EmptyMap { size_in_cells: Size, terrain_tile_category: String, terrain_tile_name: String },
    Preset { preset_number: usize },
    SaveGame { save_file_path: String },
}

impl Default for GameConfigs {
    fn default() -> Self {
        Self {
            // Engine:
            engine: EngineConfigs::default(),

            // Tile Map:
            load_map_setting: None,

            // Camera:
            camera_zoom: CameraZoom::MIN,
            camera_offset: CameraOffset::Center,

            // Simulation:
            sim_random_seed: 0xCAFE1CAFE2CAFE3A,
            sim_update_frequency_secs: 0.5,

            // Workers/Population:
            workers_search_radius: 20,
            workers_update_frequency_secs: 20.0,

            // Game Systems:
            settlers_spawn_frequency_secs: 20.0,
            population_per_settler_unit: 1,

            // Debug:
            show_debug_popups: true,
        }
    }
}

// ----------------------------------------------
// GameAssets
// ----------------------------------------------

struct GameAssets {
    tile_sets: TileSets,
    unit_configs: UnitConfigs,
    building_configs: BuildingConfigs,
}

impl GameAssets {
    fn new(tex_cache: &mut dyn TextureCache) -> Self {
        Self {
            tile_sets: TileSets::load(tex_cache),
            unit_configs: UnitConfigs::load(),
            building_configs: BuildingConfigs::load(),
        }
    }
}

// ----------------------------------------------
// GameSession
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct GameSession<'game> {
    tile_map: Box<TileMap<'game>>,
    world: World<'game>,
    sim: Simulation<'game>,
    systems: GameSystems,
    camera: Camera,

    // NOTE: These are not actually serialized.
    // We only need to invoke post_load() on them.
    #[serde(skip)]
    tile_selection: TileSelection,

    #[serde(skip)]
    debug_menus: DebugMenusSystem,
}

impl<'game> GameSession<'game> {
    fn new(
        tex_cache: &mut dyn TextureCache,
        assets: &'game GameAssets,
        configs: &GameConfigs,
        viewport_size: Size,
    ) -> Self {
        if !viewport_size.is_valid() {
            panic!("Invalid game viewport size!");
        }

        let mut opt_save_file_to_load: Option<&String> = None;
        if let Some(LoadMapSetting::SaveGame { save_file_path }) = &configs.load_map_setting {
            opt_save_file_to_load = Some(save_file_path);
        }

        let mut world = World::new(&assets.building_configs, &assets.unit_configs);
        let mut tile_map = Self::create_tile_map(&mut world, assets, configs);

        let sim = Simulation::new(
            &tile_map,
            configs.sim_random_seed,
            configs.sim_update_frequency_secs,
            configs.workers_search_radius,
            configs.workers_update_frequency_secs,
            &assets.building_configs,
            &assets.unit_configs,
        );

        let mut systems = GameSystems::new();
        systems.register(settlers::SettlersSpawnSystem::new(
            configs.settlers_spawn_frequency_secs,
            configs.population_per_settler_unit,
        ));

        let camera = Camera::new(
            viewport_size,
            tile_map.size_in_cells(),
            configs.camera_zoom,
            configs.camera_offset,
        );

        let debug_menus = DebugMenusSystem::new(&mut tile_map, tex_cache);

        let mut session = Self {
            tile_map,
            world,
            sim,
            systems,
            camera,
            tile_selection: TileSelection::default(),
            debug_menus,
        };

        if let Some(save_file_path) = opt_save_file_to_load {
            session.load_game(save_file_path, assets);
        }

        session
    }

    fn create_tile_map(
        world: &mut World,
        assets: &'game GameAssets,
        configs: &GameConfigs
    ) -> Box<TileMap<'game>> {
        let tile_map = {
            if let Some(settings) = &configs.load_map_setting{
                match settings {
                    LoadMapSetting::EmptyMap {
                        size_in_cells,
                        terrain_tile_category,
                        terrain_tile_name,
                    } => {
                        log::info!(
                            log::channel!("session"),
                            "Creating empty Tile Map. Size: {size_in_cells}, Fill: {terrain_tile_name}"
                        );

                        if !size_in_cells.is_valid() {
                            panic!("Invalid Tile Map dimensions! Width & height must not be zero.");
                        }

                        TileMap::with_terrain_tile(
                            *size_in_cells,
                            &assets.tile_sets,
                            hash::fnv1a_from_str(terrain_tile_category),
                            hash::fnv1a_from_str(terrain_tile_name),
                        )
                    }
                    LoadMapSetting::Preset { preset_number } => {
                        debug::utils::create_preset_tile_map(
                            world,
                            &assets.tile_sets,
                            configs,
                            *preset_number,
                        )
                    }
                    LoadMapSetting::SaveGame { save_file_path } => {
                        if save_file_path.is_empty() {
                            panic!("No save file path provided!");
                        }

                        // Loading a save requires loading a full GameSession, so we'll just create
                        // a dummy map here. The actual loading will be handled by the caller.
                        TileMap::default()
                    }
                }
            } else {
                TileMap::default() // Empty dummy map.
            }
        };

        Box::new(tile_map)
    }

    fn terminate(&mut self, tile_sets: &'game TileSets) {
        self.tile_selection = TileSelection::default();
        self.sim.reset(&mut self.world, &mut self.systems, &mut self.tile_map, tile_sets);
    }
}

// ----------------------------------------------
// Save/Load for GameSession
// ----------------------------------------------

impl Save for GameSession<'_> {
    fn pre_save(&mut self) {
        self.tile_map.pre_save();
        self.world.pre_save();
        self.sim.pre_save();
        self.systems.pre_save();
        self.camera.pre_save();
        self.tile_selection.pre_save();
        self.debug_menus.pre_save();
    }

    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }

    fn post_save(&mut self) {
        self.tile_map.post_save();
        self.world.post_save();
        self.sim.post_save();
        self.systems.post_save();
        self.camera.post_save();
        self.tile_selection.post_save();
        self.debug_menus.post_save();
    }
}

impl<'game> Load<'game, 'game> for GameSession<'game> {
    fn pre_load(&mut self) {
        self.tile_map.pre_load();
        self.world.pre_load();
        self.sim.pre_load();
        self.systems.pre_load();
        self.camera.pre_load();
        self.tile_selection.pre_load();
        self.debug_menus.pre_load();
    }

    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext<'game, 'game>) {
        self.tile_map.post_load(context);
        self.world.post_load(context);
        self.sim.post_load(context);
        self.systems.post_load(context);
        self.camera.post_load(context);
        self.tile_selection.post_load(context);
        self.debug_menus.post_load(context);
    }
}

// ----------------------------------------------
// Save Game
// ----------------------------------------------

const AUTOSAVE_FILE_NAME: &str = "autosave.json";
const DEFAULT_SAVE_FILE_NAME: &str = "save_game.json";

impl<'game> GameSession<'game> {
    fn save_game(&mut self, save_file_path: &str) -> bool {
        log::info!(log::channel!("session"), "Save game {save_file_path} ...");

        fn can_write_save_file(save_file_path: &str) -> bool {
            // Attempt to write a dummy file to probe if the path is writable.
            std::fs::write(save_file_path, save_file_path).is_ok()
        }

        fn do_save(state: &mut SaveStateImpl, sesion: &GameSession, save_file_path: &str) -> bool {
            if let Err(err) = sesion.save(state) {
                log::error!(log::channel!("session"), "Failed to saved game: {err}");
                return false;
            }

            if let Err(err) = state.write_file(save_file_path) {
                log::error!(
                    log::channel!("session"),
                    "Failed to write saved game file '{save_file_path}': {err}"
                );
                return false;
            }

            true
        }

        if !can_write_save_file(save_file_path) {
            log::error!(
                log::channel!("session"),
                "Saved game file path '{save_file_path}' is not accessible!"
            );
            return false;
        }

        let mut state = save::backend::new_json_save_state(true);

        self.pre_save();

        let result = do_save(&mut state, self, save_file_path);

        self.post_save();

        result
    }

    fn load_game(&mut self, save_file_path: &str, assets: &'game GameAssets) -> bool {
        log::info!(log::channel!("session"), "Loading save game {save_file_path} ...");

        let mut state = save::backend::new_json_save_state(false);

        if let Err(err) = state.read_file(save_file_path) {
            log::error!(
                log::channel!("session"),
                "Failed to read saved game file '{save_file_path:?}': {err}"
            );
            return false;
        }

        // Load into a temporary instance so that if we fail we'll avoid modifying any state.
        let session: GameSession = match state.load_new_instance() {
            Ok(session) => session,
            Err(err) => {
                log::error!(
                    log::channel!("session"),
                    "Failed to load saved game from '{save_file_path:?}': {err}"
                );
                return false;
            }
        };

        self.pre_load();

        *self = session;

        self.post_load(&PostLoadContext::new(
            &self.tile_map,
            &assets.tile_sets,
            &assets.unit_configs,
            &assets.building_configs,
        ));

        true
    }
}

// ----------------------------------------------
// GameLoop
// ----------------------------------------------

pub struct GameLoop<'game> {
    configs: Box<GameConfigs>,
    engine: Box<dyn Engine>,
    assets: Box<GameAssets>,
    session: Option<Box<GameSession<'game>>>,
}

impl<'game> GameLoop<'game> {
    // ----------------------
    // Public API:
    // ----------------------

    pub fn new() -> Self {
        let configs = Self::load_configs();

        // Boot the engine and load assets:
        let mut engine = Self::init_engine(&configs.engine);
        let assets = Self::load_assets(engine.texture_cache_mut());

        // Global initialization:
        cheats::initialize();
        debug::set_show_popup_messages(configs.show_debug_popups);

        Self { configs, engine, assets, session: None }
    }

    pub fn create_session(&mut self) {
        debug_assert!(self.session.is_none());

        let viewport_size = self.engine.viewport().size();
        let tex_cache = self.engine.texture_cache_mut();

        let new_session = GameSession::new(tex_cache, &self.assets, &self.configs, viewport_size);

        let game_loop = utils::mut_ref_cast(self);
        game_loop.session = Some(Box::new(new_session));

        log::info!(log::channel!("game"), "Game Session created.");
    }

    pub fn terminate_session(&mut self) {
        if self.session.is_some() {
            self.session().terminate(&self.assets.tile_sets);
            self.session = None;
            log::info!(log::channel!("game"), "Game Session destroyed.");
        }
    }

    pub fn update(&mut self) {
        let engine = self.engine();
        let (delta_time_secs, cursor_screen_pos) = engine.begin_frame();

        // Input Events:
        for event in engine.poll_events() {
            self.handle_event(event, cursor_screen_pos);
        }

        // Game Logic:
        let update_map_scrolling = !engine.ui_system().is_handling_mouse_input();
        let visible_range =
            self.update_simulation(update_map_scrolling, cursor_screen_pos, delta_time_secs);

        // Rendering:
        let render_flags =
            self.debug_menus_begin_frame(visible_range, cursor_screen_pos, delta_time_secs);
        self.draw_tile_map(visible_range, render_flags);
        self.debug_menus_end_frame(visible_range, cursor_screen_pos, delta_time_secs);

        engine.end_frame();
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.session.is_some() && self.engine.is_running()
    }

    // ----------------------
    // Internal:
    // ----------------------

    #[inline]
    fn session(&self) -> &mut GameSession<'game> {
        let session_box = self.session.as_ref().unwrap();
        let session_ref = session_box.as_ref();
        utils::mut_ref_cast(session_ref)
    }

    #[inline]
    fn engine(&self) -> &mut dyn Engine {
        let engine_box = &self.engine;
        let engine_ref = engine_box.as_ref();
        utils::mut_ref_cast(engine_ref)
    }

    fn load_configs() -> Box<GameConfigs> {
        // TODO: Load configs from a json file using serde.
        // TODO: Could support commandline overrides for configs & game cheats.
        let configs = GameConfigs {
            // TEMP TEST CODE:
            load_map_setting: Some(LoadMapSetting::Preset { preset_number: 0 }),
            //load_map_setting: Some(LoadMapSetting::EmptyMap { size_in_cells: Size::new(64, 64), terrain_tile_category: "ground".into(), terrain_tile_name: "dirt".into() }),
            //load_map_setting: Some(LoadMapSetting::SaveGame { save_file_path: DEFAULT_SAVE_FILE_NAME.into() }),
            ..Default::default()
        };
        Box::new(configs)
    }

    fn load_assets(tex_cache: &mut dyn TextureCache) -> Box<GameAssets> {
        log::info!(log::channel!("game"), "Loading Game Assets ...");
        Box::new(GameAssets::new(tex_cache))
    }

    fn init_engine(configs: &EngineConfigs) -> Box<dyn Engine> {
        log::info!(log::channel!("game"), "Init Engine: GLFW + OpenGL");
        Box::new(engine::backend::GlfwOpenGlEngine::new(configs))
    }

    fn handle_event(&self, event: ApplicationEvent, cursor_screen_pos: Vec2) {
        let session = self.session();

        match event {
            ApplicationEvent::WindowResize(window_size) => {
                session.camera.set_viewport_size(window_size);
            }
            ApplicationEvent::KeyInput(key, action, _modifiers) => {
                let mut args = self.new_debug_menus_input_args(cursor_screen_pos);
                session.debug_menus.on_key_input(&mut args, key, action);
            }
            ApplicationEvent::Scroll(amount) => {
                if amount.y < 0.0 {
                    session.camera.request_zoom(CameraZoom::In);
                } else if amount.y > 0.0 {
                    session.camera.request_zoom(CameraZoom::Out);
                }
            }
            ApplicationEvent::MouseButton(button, action, modifiers) => {
                let mut args = self.new_debug_menus_input_args(cursor_screen_pos);
                session.debug_menus.on_mouse_click(&mut args, button, action, modifiers);
            }
            _ => {}
        }
    }

    fn update_simulation(
        &self,
        update_map_scrolling: bool,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) -> CellRange {
        let session = self.session();

        session.camera.update_zooming(delta_time_secs);

        // Map scrolling:
        if update_map_scrolling {
            session.camera.update_scrolling(cursor_screen_pos, delta_time_secs);
        }

        session.sim.update(
            &mut session.world,
            &mut session.systems,
            &mut session.tile_map,
            &self.assets.tile_sets,
            delta_time_secs,
        );

        let visible_range = session.camera.visible_cells_range();
        session.tile_map.update_anims(visible_range, delta_time_secs);

        visible_range
    }

    fn draw_tile_map(&self, visible_range: CellRange, flags: TileMapRenderFlags) {
        let session = self.session();
        self.engine().draw_tile_map(
            &session.tile_map,
            &session.tile_selection,
            session.camera.transform(),
            visible_range,
            flags,
        );
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    fn debug_menus_begin_frame(
        &self,
        visible_range: CellRange,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) -> TileMapRenderFlags {
        let mut args =
            self.new_debug_menus_frame_args(visible_range, cursor_screen_pos, delta_time_secs);
        self.session().debug_menus.begin_frame(&mut args)
    }

    fn debug_menus_end_frame(
        &self,
        visible_range: CellRange,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) {
        let mut args =
            self.new_debug_menus_frame_args(visible_range, cursor_screen_pos, delta_time_secs);
        self.session().debug_menus.end_frame(&mut args);
    }

    fn new_debug_menus_input_args(
        &'game self,
        cursor_screen_pos: Vec2,
    ) -> DebugMenusInputArgs<'game> {
        let session = self.session();
        DebugMenusInputArgs {
            tile_map: &mut session.tile_map,
            tile_selection: &mut session.tile_selection,
            world: &mut session.world,
            transform: session.camera.transform(),
            cursor_screen_pos,
        }
    }

    fn new_debug_menus_frame_args(
        &'game self,
        visible_range: CellRange,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) -> DebugMenusFrameArgs<'game> {
        let session = self.session();
        let engine = self.engine();
        DebugMenusFrameArgs {
            tile_map: &mut session.tile_map,
            tile_sets: &self.assets.tile_sets,
            tile_selection: &mut session.tile_selection,
            sim: &mut session.sim,
            world: &mut session.world,
            systems: &mut session.systems,
            ui_sys: engine.ui_system(),
            log_viewer: engine.log_viewer(),
            render_sys_stats: engine.render_stats(),
            tile_render_stats: engine.tile_map_render_stats(),
            transform: session.camera.transform(),
            camera: &mut session.camera,
            visible_range,
            cursor_screen_pos,
            delta_time_secs,
        }
    }
}
