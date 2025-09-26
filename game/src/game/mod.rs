use std::path::{Path, PathBuf};
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

use crate::{
    singleton_late_init,
    imgui_ui::UiInputEvent,
    app::{ApplicationEvent, input::{InputAction, InputKey, InputModifiers, MouseButton}},
    debug::{self, DebugMenusFrameArgs, DebugMenusInputArgs, DebugMenusSystem},
    engine::{self, Engine, config::EngineConfigs},
    log,
    render::TextureCache,
    save::{self, *},
    tile::{
        TileMap, camera::*, rendering::TileMapRenderFlags, selection::TileSelection, sets::{TileSets, TileDef},
    },
    utils::{Size, Vec2, coords::CellRange, hash, file_sys},
    engine::time::{Seconds, UpdateTimer},
};

use {
    config::{GameConfigs, LoadMapSetting},
    building::config::BuildingConfigs,
    unit::config::UnitConfigs,
    sim::Simulation,
    system::{GameSystems, settlers},
    world::World,
};

pub mod building;
pub mod config;
pub mod cheats;
pub mod constants;
pub mod sim;
pub mod system;
pub mod unit;
pub mod world;

// ----------------------------------------------
// GameSession
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct GameSession {
    tile_map: Box<TileMap>,
    world: World,
    sim: Simulation,
    systems: GameSystems,
    camera: Camera,

    // NOTE: These are not actually serialized on save games.
    // We only need to invoke post_load() on them.
    #[serde(skip)]
    tile_selection: TileSelection,

    #[serde(skip)]
    debug_menus: DebugMenusSystem,
}

impl GameSession {
    fn new(tex_cache: &mut dyn TextureCache, load_map_setting: &LoadMapSetting, viewport_size: Size) -> Self {
        if !viewport_size.is_valid() {
            panic!("Invalid game viewport size!");
        }

        let mut world = World::new();
        let mut tile_map = Self::create_tile_map(&mut world, load_map_setting);
        let sim = Simulation::new(&tile_map);

        let mut systems = GameSystems::new();
        systems.register(settlers::SettlersSpawnSystem::new());

        let configs = GameConfigs::get();
        let camera = Camera::new(
            viewport_size,
            tile_map.size_in_cells(),
            configs.camera.zoom,
            configs.camera.offset,
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

        if let LoadMapSetting::SaveGame { save_file_path } = load_map_setting {
            session.load_save_game(save_file_path);
        }

        session
    }

    fn create_tile_map(world: &mut World, load_map_setting: &LoadMapSetting) -> Box<TileMap> {
        let tile_map = {
            match load_map_setting {
                LoadMapSetting::None => {
                    TileMap::default() // Empty dummy map.
                }
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
                        panic!("LoadMapSetting::EmptyMap: Invalid Tile Map dimensions! Width & height must not be zero.");
                    }

                    TileMap::with_terrain_tile(
                        *size_in_cells,
                        hash::fnv1a_from_str(terrain_tile_category),
                        hash::fnv1a_from_str(terrain_tile_name),
                    )
                }
                LoadMapSetting::Preset { preset_number } => {
                    debug::utils::create_preset_tile_map(world, *preset_number)
                }
                LoadMapSetting::SaveGame { save_file_path } => {
                    if save_file_path.is_empty() {
                        panic!("LoadMapSetting::SaveGame: No save file path provided!");
                    }
                    // Loading a save requires loading a full GameSession, so we'll just create
                    // a dummy map here. The actual loading will be handled by the caller.
                    TileMap::default()
                }
            }
        };
        Box::new(tile_map)
    }

    fn load_preset_map(preset_number: usize, tex_cache: &mut dyn TextureCache, viewport_size: Size) -> Self {
        // Override GameConfigs.load_map_setting
        let load_map_setting = LoadMapSetting::Preset { preset_number };
        Self::new(tex_cache, &load_map_setting, viewport_size)
    }

    fn reset(&mut self, reset_map: bool, reset_map_with_tile_def: Option<&'static TileDef>) {
        self.tile_selection = TileSelection::default();

        self.sim.reset(&mut self.world, &mut self.systems, &mut self.tile_map);

        if reset_map && self.tile_map.size_in_cells().is_valid() {
            self.tile_map.reset(reset_map_with_tile_def);
        }
    }
}

// ----------------------------------------------
// Save/Load for GameSession
// ----------------------------------------------

impl Save for GameSession {
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

impl Load for GameSession {
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

    fn post_load(&mut self, context: &PostLoadContext) {
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

pub const AUTOSAVE_FILE_NAME: &str = "autosave.json";
pub const DEFAULT_SAVE_FILE_NAME: &str = "save_game.json";
pub const SAVE_GAMES_DIR_PATH: &str = "saves";

fn make_save_game_file_path(save_file_name: &str) -> String {
    Path::new(SAVE_GAMES_DIR_PATH)
        .join(save_file_name)
        .with_extension("json")
        .to_string_lossy()
        .into()
}

impl GameSession {
    fn save_game(&mut self, save_file_path: &str) -> bool {
        log::info!(log::channel!("session"), "Saving game '{save_file_path}' ...");

        fn can_write_save_file(save_file_path: &str) -> bool {
            // Attempt to write a dummy file to probe if the path is writable.
            std::fs::write(save_file_path, save_file_path).is_ok()
        }

        fn do_save(state: &mut SaveStateImpl, sesion: &GameSession, save_file_path: &str) -> bool {
            if let Err(err) = sesion.save(state) {
                log::error!(log::channel!("session"), "Failed to save game: {err}");
                return false;
            }

            if let Err(err) = state.write_file(save_file_path) {
                log::error!(
                    log::channel!("session"),
                    "Failed to write save game file '{save_file_path}': {err}"
                );
                return false;
            }

            true
        }

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = std::fs::create_dir_all(SAVE_GAMES_DIR_PATH);

        if !can_write_save_file(save_file_path) {
            log::error!(
                log::channel!("session"),
                "Save game file path '{save_file_path}' is not accessible!"
            );
            return false;
        }

        let mut state = save::backend::new_json_save_state(true);

        self.pre_save();
        let result = do_save(&mut state, self, save_file_path);
        self.post_save();

        result
    }

    fn load_save_game(&mut self, save_file_path: &str) -> bool {
        log::info!(log::channel!("session"), "Loading save game '{save_file_path}' ...");

        let mut state = save::backend::new_json_save_state(false);

        if let Err(err) = state.read_file(save_file_path) {
            log::error!(
                log::channel!("session"),
                "Failed to read save game file '{save_file_path}': {err}"
            );
            return false;
        }

        // Load into a temporary instance so that if we fail we'll avoid modifying any state.
        let session: GameSession = match state.load_new_instance() {
            Ok(session) => session,
            Err(err) => {
                log::error!(
                    log::channel!("session"),
                    "Failed to load save game from '{save_file_path}': {err}"
                );
                return false;
            }
        };

        self.pre_load();
        *self = session;
        self.post_load(&PostLoadContext::new(&self.tile_map));

        true
    }
}

// ----------------------------------------------
// GameSessionCmd
// ----------------------------------------------

// Deferred session commands that must be processed at a safe point in the GameLoop update.
// These are kept in a queue and consumed every iteration of the loop.
enum GameSessionCmd {
    Reset { reset_map_with_tile_def: Option<&'static TileDef> },
    LoadPreset { preset_number: usize },
    LoadSaveGame { save_file_path: String },
    SaveGame { save_file_path: String },
}

// ----------------------------------------------
// GameLoop
// ----------------------------------------------

pub struct GameLoop {
    engine: Box<dyn Engine>,

    session: Option<Box<GameSession>>,
    session_cmd_queue: VecDeque<GameSessionCmd>,

    autosave_timer: UpdateTimer,
    enable_autosave: bool,
}

impl GameLoop {
    // ----------------------
    // Public API:
    // ----------------------

    pub fn new() -> &'static mut Self {
        let configs = GameConfigs::load();

        // Boot the engine and load assets:
        let mut engine = Self::init_engine(&configs.engine);
        Self::load_assets(engine.texture_cache_mut());

        // Global initialization:
        cheats::initialize();
        Simulation::register_callbacks();
        debug::set_show_popup_messages(configs.debug.show_popups);

        let instance = Self {
            engine,
            session: None,
            session_cmd_queue: VecDeque::new(),
            autosave_timer: UpdateTimer::new(configs.save.autosave_frequency_secs),
            enable_autosave: configs.save.enable_autosave,
        };

        GameLoop::initialize(instance); // Set global instance.
        GameLoop::get_mut()
    }

    pub fn create_session(&mut self) {
        debug_assert!(self.session.is_none());

        let viewport_size = self.engine.viewport().size();
        let tex_cache = self.engine.texture_cache_mut();

        let new_session = GameSession::new(
            tex_cache,
            &GameConfigs::get().save.load_map_setting,
            viewport_size
        );

        self.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "Game Session created.");
    }

    pub fn terminate_session(&mut self) {
        if let Some(session) = &mut self.session {
            session.reset(false, None);
        }
        self.session = None;
        log::info!(log::channel!("game"), "Game Session destroyed.");
    }

    pub fn reset_session(&mut self, reset_map_with_tile_def: Option<&'static TileDef>) {
        self.session_cmd_queue.push_back(GameSessionCmd::Reset {
            reset_map_with_tile_def
        });
    }

    pub fn load_preset_map(&mut self, preset_tile_map_number: usize) {
        self.session_cmd_queue.push_back(GameSessionCmd::LoadPreset {
            preset_number: preset_tile_map_number
        });
    }

    pub fn load_save_game(&mut self, save_file_name: &str) {
        if save_file_name.is_empty() {
            log::error!(log::channel!("game"), "Load game: Empty file name!");
            return;
        }

        self.session_cmd_queue.push_back(GameSessionCmd::LoadSaveGame {
            save_file_path: make_save_game_file_path(save_file_name)
        });
    }

    pub fn save_game(&mut self, save_file_name: &str) {
        if save_file_name.is_empty() {
            log::error!(log::channel!("game"), "Save game: Empty file name!");
            return;
        }

        self.session_cmd_queue.push_back(GameSessionCmd::SaveGame {
            save_file_path: make_save_game_file_path(save_file_name)
        });
    }

    #[inline]
    pub fn save_files_list(&self) -> Vec<PathBuf> {
        file_sys::collect_files(
            &SAVE_GAMES_DIR_PATH,
            file_sys::CollectFlags::FilenamesOnly,
            Some("json"))
    }

    #[inline]
    pub fn is_autosave_enabled(&self) -> bool {
        self.enable_autosave
    }

    #[inline]
    pub fn enable_autosave(&mut self, enable: bool) {
        self.enable_autosave = enable;
    }

    #[inline]
    pub fn engine(&self) -> &dyn Engine {
        self.engine.as_ref()
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut dyn Engine {
        self.engine.as_mut()
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.session.is_some() && self.engine.is_running()
    }

    pub fn update(&mut self) {
        self.update_autosave();
        self.process_session_commands();

        let (delta_time_secs, cursor_screen_pos) = self.engine.begin_frame();

        // Input Events:
        for event in self.engine.app_events().clone() {
            self.handle_event(event, cursor_screen_pos);
        }

        // Game Logic:
        let visible_range = self.update_simulation(cursor_screen_pos, delta_time_secs);

        // Rendering:
        let render_flags = self.debug_menus_begin_frame(visible_range, cursor_screen_pos, delta_time_secs);

        self.draw_tile_map(visible_range, render_flags);

        self.debug_menus_end_frame(visible_range, cursor_screen_pos, delta_time_secs);

        self.engine.end_frame();
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn init_engine(configs: &EngineConfigs) -> Box<dyn Engine> {
        log::info!(log::channel!("game"), "Init Engine: GLFW + OpenGL");
        Box::new(engine::backend::GlfwOpenGlEngine::new(configs))
    }

    fn load_assets(tex_cache: &mut dyn TextureCache) {
        log::info!(log::channel!("game"), "Loading Game Assets ...");

        BuildingConfigs::load();
        log::info!(log::channel!("game"), "BuildingConfigs loaded.");

        UnitConfigs::load();
        log::info!(log::channel!("game"), "UnitConfigs loaded.");

        TileSets::load(tex_cache);
        log::info!(log::channel!("game"), "TileSets loaded.");
    }

    fn handle_event(&mut self, event: ApplicationEvent, cursor_screen_pos: Vec2) {
        match event {
            ApplicationEvent::WindowResize(window_size) => {
                self.session.as_mut().unwrap().camera.set_viewport_size(window_size);
            }
            ApplicationEvent::KeyInput(key, action, _modifiers) => {
                self.debug_menus_key_input(key, action, cursor_screen_pos);
            }
            ApplicationEvent::Scroll(amount) => {
                let session = self.session.as_mut().unwrap();
                if amount.y < 0.0 {
                    session.camera.request_zoom(CameraZoom::In);
                } else if amount.y > 0.0 {
                    session.camera.request_zoom(CameraZoom::Out);
                }
            }
            ApplicationEvent::MouseButton(button, action, modifiers) => {
                self.debug_menus_mouse_click(button, action, modifiers, cursor_screen_pos);
            }
            _ => {}
        }
    }

    fn update_simulation(&mut self, cursor_screen_pos: Vec2, delta_time_secs: Seconds) -> CellRange {
        let session = self.session.as_mut().unwrap();

        session.camera.update_zooming(delta_time_secs);

        // Map scrolling:
        let ui_sys = self.engine.ui_system();
        session.camera.update_scrolling(ui_sys, cursor_screen_pos, delta_time_secs);

        session.sim.update(
            &mut session.world,
            &mut session.systems,
            &mut session.tile_map,
            delta_time_secs,
        );

        let visible_range = session.camera.visible_cells_range();
        session.tile_map.update_anims(visible_range, delta_time_secs);

        visible_range
    }

    fn draw_tile_map(&mut self, visible_range: CellRange, flags: TileMapRenderFlags) {
        let session = self.session.as_ref().unwrap();
        self.engine.draw_tile_map(
            &session.tile_map,
            &session.tile_selection,
            session.camera.transform(),
            visible_range,
            flags);
    }

    fn update_autosave(&mut self) {
        if !self.enable_autosave {
            return;
        }

        let delta_time_secs = self.engine.frame_clock().delta_time();

        if self.autosave_timer.tick(delta_time_secs).should_update() {
            self.save_game(AUTOSAVE_FILE_NAME);
        }
    }

    // ----------------------
    // Session Commands:
    // ----------------------

    fn process_session_commands(&mut self) {
        while let Some(cmd) = self.session_cmd_queue.pop_front() {
            match cmd {
                GameSessionCmd::Reset { reset_map_with_tile_def } => {
                    self.session_cmd_reset(reset_map_with_tile_def);
                }
                GameSessionCmd::LoadPreset { preset_number } => {
                    self.session_cmd_load_preset(preset_number);
                }
                GameSessionCmd::LoadSaveGame { save_file_path } => {
                    self.session_cmd_load_save_game(save_file_path);
                }
                GameSessionCmd::SaveGame { save_file_path } => {
                    self.session_cmd_save_game(save_file_path);
                }
            }
        }
    }

    fn session_cmd_reset(&mut self, reset_map_with_tile_def: Option<&'static TileDef>) {
        self.session.as_mut().unwrap().reset(true, reset_map_with_tile_def);
        log::info!(log::channel!("game"), "Game Session reset.");
    }

    fn session_cmd_load_preset(&mut self, preset_number: usize) {
        self.terminate_session();

        let viewport_size = self.engine.viewport().size();
        let tex_cache = self.engine.texture_cache_mut();

        let new_session = GameSession::load_preset_map(
            preset_number,
            tex_cache,
            viewport_size
        );

        self.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "Game Session created.");
    }

    fn session_cmd_load_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session.as_mut().unwrap().load_save_game(&save_file_path);
    }

    fn session_cmd_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session.as_mut().unwrap().save_game(&save_file_path);
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    fn debug_menus_begin_frame(
        &mut self,
        visible_range: CellRange,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) -> TileMapRenderFlags {
        let session = self.session.as_mut().unwrap();
        session.debug_menus.begin_frame(&mut DebugMenusFrameArgs {
            tile_map: &mut session.tile_map,
            tile_selection: &mut session.tile_selection,
            sim: &mut session.sim,
            world: &mut session.world,
            systems: &mut session.systems,
            ui_sys: self.engine.ui_system(),
            camera: &mut session.camera,
            visible_range,
            cursor_screen_pos,
            delta_time_secs,
        })
    }

    fn debug_menus_end_frame(
        &mut self,
        visible_range: CellRange,
        cursor_screen_pos: Vec2,
        delta_time_secs: Seconds,
    ) {
        let session = self.session.as_mut().unwrap();
        session.debug_menus.end_frame(&mut DebugMenusFrameArgs {
            tile_map: &mut session.tile_map,
            tile_selection: &mut session.tile_selection,
            sim: &mut session.sim,
            world: &mut session.world,
            systems: &mut session.systems,
            ui_sys: self.engine.ui_system(),
            camera: &mut session.camera,
            visible_range,
            cursor_screen_pos,
            delta_time_secs,
        });
    }

    fn debug_menus_key_input(
        &mut self,
        key: InputKey,
        action: InputAction,
        cursor_screen_pos: Vec2,
    ) -> UiInputEvent {
        let session = self.session.as_mut().unwrap();
        session.debug_menus.on_key_input(&mut DebugMenusInputArgs {
            tile_map: &mut session.tile_map,
            tile_selection: &mut session.tile_selection,
            world: &mut session.world,
            transform: session.camera.transform(),
            cursor_screen_pos,
        }, key, action)
    }

    fn debug_menus_mouse_click(
        &mut self,
        button: MouseButton,
        action: InputAction,
        modifiers: InputModifiers,
        cursor_screen_pos: Vec2,
    ) -> UiInputEvent {
        let session = self.session.as_mut().unwrap();
        session.debug_menus.on_mouse_click(&mut DebugMenusInputArgs {
            tile_map: &mut session.tile_map,
            tile_selection: &mut session.tile_selection,
            world: &mut session.world,
            transform: session.camera.transform(),
            cursor_screen_pos,
        }, button, action, modifiers)
    }
}

// ----------------------------------------------
// GameLoop Global Singleton
// ----------------------------------------------

singleton_late_init! { GAME_LOOP_SINGLETON, GameLoop }
