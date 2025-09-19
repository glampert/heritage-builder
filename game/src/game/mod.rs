use std::path::{Path, PathBuf};
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

use crate::{
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
    utils::{mem, Size, Vec2, coords::CellRange, hash, file_sys},
    engine::time::{Seconds, UpdateTimer},
};

use {
    building::config::BuildingConfigs,
    config::{GameConfigs, LoadMapSetting},
    sim::Simulation,
    system::{GameSystems, settlers},
    unit::config::UnitConfigs,
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
// GameAssets
// ----------------------------------------------

struct GameAssets {
    tile_sets: TileSets,
    unit_configs: &'static UnitConfigs,
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

    // NOTE: These are not actually serialized on save games.
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
        load_map_setting: &LoadMapSetting,
        viewport_size: Size,
    ) -> Self {
        if !viewport_size.is_valid() {
            panic!("Invalid game viewport size!");
        }

        let game_configs = GameConfigs::get();

        let mut opt_save_file_to_load: Option<&String> = None;
        if let LoadMapSetting::SaveGame { save_file_path } = load_map_setting {
            opt_save_file_to_load = Some(save_file_path);
        }

        let mut world = World::new(&assets.building_configs, assets.unit_configs);
        let mut tile_map = Self::create_tile_map(&mut world, assets, load_map_setting);

        let sim = Simulation::new(
            &tile_map,
            game_configs,
            &assets.building_configs,
            assets.unit_configs,
        );

        let mut systems = GameSystems::new();
        systems.register(settlers::SettlersSpawnSystem::new(
            game_configs.sim.settlers_spawn_frequency_secs,
            game_configs.sim.population_per_settler_unit,
        ));

        let camera = Camera::new(
            viewport_size,
            tile_map.size_in_cells(),
            game_configs.camera.zoom,
            game_configs.camera.offset,
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
            session.load_save_game(save_file_path, assets);
        }

        session
    }

    fn create_tile_map(
        world: &mut World,
        assets: &'game GameAssets,
        load_map_setting: &LoadMapSetting,
    ) -> Box<TileMap<'game>> {
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
                        &assets.tile_sets,
                        hash::fnv1a_from_str(terrain_tile_category),
                        hash::fnv1a_from_str(terrain_tile_name),
                    )
                }
                LoadMapSetting::Preset { preset_number } => {
                    debug::utils::create_preset_tile_map(
                        world,
                        &assets.tile_sets,
                        *preset_number)
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

    fn load_preset_map(
        preset_number: usize,
        tex_cache: &mut dyn TextureCache,
        assets: &'game GameAssets,
        viewport_size: Size,
    ) -> Self {
        // Override GameConfigs.load_map_setting
        let load_map_setting = LoadMapSetting::Preset { preset_number };
        Self::new(tex_cache, assets, &load_map_setting, viewport_size)
    }

    fn reset(&mut self, tile_sets: &'game TileSets, reset_map: bool, reset_map_with_tile_def: Option<&'game TileDef>) {
        self.tile_selection = TileSelection::default();

        self.sim.reset(&mut self.world, &mut self.systems, &mut self.tile_map, tile_sets);

        if reset_map && self.tile_map.size_in_cells().is_valid() {
            self.tile_map.reset(reset_map_with_tile_def);
        }
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

impl<'game> GameSession<'game> {
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

    fn load_save_game(&mut self, save_file_path: &str, assets: &'game GameAssets) -> bool {
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

        self.post_load(&PostLoadContext::new(
            &self.tile_map,
            &assets.tile_sets,
            assets.unit_configs,
            &assets.building_configs,
        ));

        true
    }
}

// ----------------------------------------------
// GameSessionCmd
// ----------------------------------------------

// Deferred session commands that must be processed at a safe point in the GameLoop update.
// These are kept in a queue and consumed every iteration of the loop.
enum GameSessionCmd<'game> {
    Reset { reset_map_with_tile_def: Option<&'game TileDef> },
    LoadPreset { preset_number: usize },
    LoadSaveGame { save_file_path: String },
    SaveGame { save_file_path: String },
}

// ----------------------------------------------
// GameLoop
// ----------------------------------------------

pub struct GameLoop<'game> {
    engine: Box<dyn Engine>,
    assets: Box<GameAssets>,

    session: Option<Box<GameSession<'game>>>,
    session_cmd_queue: VecDeque<GameSessionCmd<'game>>,

    autosave_timer: UpdateTimer,
    enable_autosave: bool,
}

impl<'game> GameLoop<'game> {
    // ----------------------
    // Public API:
    // ----------------------

    pub fn new() -> Self {
        let game_configs = GameConfigs::load();

        // Boot the engine and load assets:
        let mut engine = Self::init_engine(&game_configs.engine);
        let assets = Self::load_assets(engine.texture_cache_mut());

        // Global initialization:
        cheats::initialize();
        Simulation::register_callbacks();
        debug::set_show_popup_messages(game_configs.debug.show_popups);

        Self {
            engine,
            assets,
            session: None,
            session_cmd_queue: VecDeque::new(),
            autosave_timer: UpdateTimer::new(game_configs.save.autosave_frequency_secs),
            enable_autosave: game_configs.save.enable_autosave,
        }
    }

    pub fn create_session(&mut self) {
        let game_loop = self.mut_ref();
        debug_assert!(game_loop.session.is_none());

        let viewport_size = game_loop.engine.viewport().size();
        let tex_cache = game_loop.engine.texture_cache_mut();

        let new_session = GameSession::new(
            tex_cache,
            &game_loop.assets,
            &GameConfigs::get().save.load_map_setting,
            viewport_size
        );

        game_loop.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "Game Session created.");
    }

    pub fn terminate_session(&mut self) {
        if self.session.is_some() {
            self.session().reset(&self.assets.tile_sets, false, None);
        }
        self.session = None;
        log::info!(log::channel!("game"), "Game Session destroyed.");
    }

    pub fn reset_session(&mut self, reset_map_with_tile_def: Option<&'game TileDef>) {
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
        file_sys::collect_files(&SAVE_GAMES_DIR_PATH, file_sys::CollectFlags::FilenamesOnly)
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
    pub fn is_running(&self) -> bool {
        self.session.is_some() && self.engine.is_running()
    }

    pub fn update(&mut self) {
        self.update_autosave();
        self.process_session_commands();

        let engine = self.engine();
        let (delta_time_secs, cursor_screen_pos) = engine.begin_frame();

        // Input Events:
        for event in engine.app_events() {
            self.handle_event(*event, cursor_screen_pos);
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

    // ----------------------
    // Internal:
    // ----------------------

    #[inline]
    fn mut_ref(&self) -> &mut GameLoop<'game> {
        mem::mut_ref_cast(self)
    }

    #[inline]
    fn session(&self) -> &mut GameSession<'game> {
        let session_box = self.session.as_ref().unwrap();
        mem::mut_ref_cast(session_box.as_ref())
    }

    #[inline]
    fn engine(&self) -> &mut dyn Engine {
        let engine_box = &self.engine;
        mem::mut_ref_cast(engine_box.as_ref())
    }

    fn load_assets(tex_cache: &mut dyn TextureCache) -> Box<GameAssets> {
        log::info!(log::channel!("game"), "Loading Game Assets ...");
        Box::new(GameAssets::new(tex_cache))
    }

    fn init_engine(engine_configs: &EngineConfigs) -> Box<dyn Engine> {
        log::info!(log::channel!("game"), "Init Engine: GLFW + OpenGL");
        Box::new(engine::backend::GlfwOpenGlEngine::new(engine_configs))
    }

    fn handle_event(&self, event: ApplicationEvent, cursor_screen_pos: Vec2) {
        match event {
            ApplicationEvent::WindowResize(window_size) => {
                self.session().camera.set_viewport_size(window_size);
            }
            ApplicationEvent::KeyInput(key, action, _modifiers) => {
                self.debug_menus_key_input(key, action, cursor_screen_pos);
            }
            ApplicationEvent::Scroll(amount) => {
                let session = self.session();
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

    fn update_autosave(&mut self) {
        if !self.enable_autosave {
            return;
        }

        let delta_time_secs = self.engine().frame_clock().delta_time();

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

    fn session_cmd_reset(&mut self, reset_map_with_tile_def: Option<&'game TileDef>) {
        self.session().reset(&self.assets.tile_sets, true, reset_map_with_tile_def);
        log::info!(log::channel!("game"), "Game Session reset.");
    }

    fn session_cmd_load_preset(&mut self, preset_number: usize) {
        let game_loop = self.mut_ref();
        game_loop.terminate_session();

        let viewport_size = game_loop.engine.viewport().size();
        let tex_cache = game_loop.engine.texture_cache_mut();

        let new_session = GameSession::load_preset_map(
            preset_number,
            tex_cache,
            &game_loop.assets,
            viewport_size
        );

        game_loop.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "Game Session created.");
    }

    fn session_cmd_load_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session().load_save_game(&save_file_path, &self.assets);
    }

    fn session_cmd_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session().save_game(&save_file_path);
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
        self.session().debug_menus.end_frame(&mut args, self.engine(), self.mut_ref());
    }

    fn debug_menus_key_input(
        &self,
        key: InputKey,
        action: InputAction,
        cursor_screen_pos: Vec2,
    ) -> UiInputEvent {
        let mut args = self.new_debug_menus_input_args(cursor_screen_pos);
        self.session().debug_menus.on_key_input(&mut args, key, action)
    }

    fn debug_menus_mouse_click(
        &self,
        button: MouseButton,
        action: InputAction,
        modifiers: InputModifiers,
        cursor_screen_pos: Vec2,
    ) -> UiInputEvent {
        let mut args = self.new_debug_menus_input_args(cursor_screen_pos);
        self.session().debug_menus.on_mouse_click(&mut args, button, action, modifiers)
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
        DebugMenusFrameArgs {
            tile_map: &mut session.tile_map,
            tile_sets: &self.assets.tile_sets,
            tile_selection: &mut session.tile_selection,
            sim: &mut session.sim,
            world: &mut session.world,
            systems: &mut session.systems,
            ui_sys: self.engine().ui_system(),
            camera: &mut session.camera,
            visible_range,
            cursor_screen_pos,
            delta_time_secs,
        }
    }
}
