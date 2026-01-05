use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use config::{GameConfigs, LoadMapSetting};
use system::{GameSystems, GameSystem, GameSystemImpl, settlers, ambient_effects};
use building::config::BuildingConfigs;
use unit::config::UnitConfigs;
use prop::config::PropConfigs;
use sim::Simulation;
use world::World;
use menu::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    home::HomeMenus,
    hud::InGameHudMenus,
};

use crate::{
    log,
    singleton_late_init,
    ui::{UiInputEvent, UiWidgetContext},
    render::TextureCache,
    save::{self, *},
    app::{
        input::{InputAction, InputKey, InputModifiers, MouseButton},
        ApplicationEvent,
    },
    engine::{
        self,
        config::EngineConfigs,
        time::{Seconds, UpdateTimer},
        Engine,
    },
    tile::{
        camera::*,
        rendering::TileMapRenderFlags,
        selection::TileSelection,
        sets::{TileDef, TileSets},
        TileKind, TileFlags, TileMap, TileMapLayerKind,
    },
    debug::{self, log_viewer::LogViewerWindow, DevEditorMenus},
    utils::{crash_report, platform::{self, paths}, coords::CellRange, file_sys, hash, Size, Vec2},
};

pub mod undo_redo;
pub mod building;
pub mod menu;
pub mod prop;
pub mod cheats;
pub mod config;
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
    menus: Option<Box<dyn GameMenusSystem>>,
}

impl GameSession {
    fn new(load_map_setting: &LoadMapSetting, viewport_size: Size, engine: &dyn Engine, home_menu: bool) -> Self {
        if !viewport_size.is_valid() {
            panic!("Invalid game viewport size!");
        }

        let mut world = World::new();
        let tile_map = Self::new_tile_map(&mut world, load_map_setting);
        let sim = Simulation::new(&tile_map);

        let mut systems = GameSystems::new();
        systems.register(settlers::SettlersSpawnSystem::default());
        systems.register(ambient_effects::AmbientEffectsSystem::default());

        let configs = GameConfigs::get();
        let camera = Camera::new(viewport_size,
                                 tile_map.size_in_cells(),
                                 configs.camera.zoom,
                                 configs.camera.offset);

        let mut session = Self {
            tile_map,
            world,
            sim,
            systems,
            camera,
            tile_selection: TileSelection::default(),
            menus: None,
        };

        session.menus = Some(session.new_game_menus_from_config(engine, home_menu));

        if let LoadMapSetting::SaveGame { save_file_path } = load_map_setting {
            session.load_save_game(&make_save_game_file_path(save_file_path));
        }

        if configs.sim.start_paused {
            session.sim.pause();
        } else {
            session.sim.resume();
        }

        session
    }

    fn new_game_menus(&mut self, engine: &dyn Engine, menu_mode: GameMenusMode) -> Box<dyn GameMenusSystem> {
        let mut context =
            UiWidgetContext::new(&mut self.sim, &self.world, &mut self.tile_map, engine);

        match menu_mode {
            GameMenusMode::DevEditor => {
                log::info!(log::channel!("session"), "Loading DevEditorMenus ...");
                Box::new(DevEditorMenus::new(&mut context))
            }
            GameMenusMode::InGameHud => {
                log::info!(log::channel!("session"), "Loading InGameHudMenus ...");
                Box::new(InGameHudMenus::new(&mut context))
            }
            GameMenusMode::Home => {
                log::info!(log::channel!("session"), "Loading HomeMenus ...");
                Box::new(HomeMenus::new(&mut context))
            }
        }
    }

    fn new_game_menus_from_config(&mut self, engine: &dyn Engine, home_menu: bool) -> Box<dyn GameMenusSystem> {
        let configs = GameConfigs::get();
        let menu_mode = {
            if configs.debug.skip_home_menu || !home_menu {
                if configs.debug.start_in_dev_editor_mode {
                    GameMenusMode::DevEditor
                } else {
                    GameMenusMode::InGameHud
                }
            } else {
                GameMenusMode::Home
            }
        };
        self.new_game_menus(engine, menu_mode)
    }

    fn toggle_menus_mode(&mut self, engine: &dyn Engine) {
        if let Some(mode) = self.current_menus_mode() {
            match mode {
                GameMenusMode::DevEditor => {
                    self.menus = Some(self.new_game_menus(engine, GameMenusMode::InGameHud));
                }
                GameMenusMode::InGameHud => {
                    self.menus = Some(self.new_game_menus(engine, GameMenusMode::DevEditor));
                }
                GameMenusMode::Home => {} // Cannot toggle out of home menu.
            }
        }
    }

    fn current_menus_mode(&self) -> Option<GameMenusMode> {
        self.menus.as_ref().map(|menus| menus.mode())
    }

    fn new_tile_map(world: &mut World, load_map_setting: &LoadMapSetting) -> Box<TileMap> {
        let tile_map = {
            match load_map_setting {
                LoadMapSetting::None => {
                    TileMap::default() // Empty dummy map.
                }
                LoadMapSetting::EmptyMap { size_in_cells,
                                           terrain_tile_category,
                                           terrain_tile_name, } => {
                    log::info!(log::channel!("session"),
                               "Creating empty Tile Map. Size: {size_in_cells}, Fill: {terrain_tile_name}");

                    if !size_in_cells.is_valid() {
                        panic!("LoadMapSetting::EmptyMap: Invalid Tile Map dimensions! Width & height must not be zero.");
                    }

                    let mut tile_map =
                        TileMap::with_terrain_tile(*size_in_cells,
                                                   hash::fnv1a_from_str(terrain_tile_category),
                                                   hash::fnv1a_from_str(terrain_tile_name));

                    tile_map.for_each_tile_mut(TileMapLayerKind::Terrain, TileKind::Terrain, |terrain| {
                        if terrain.has_flags(TileFlags::RandomizePlacement) {
                            terrain.set_random_variation_index(&mut rand::rng());
                        }
                    });

                    tile_map
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

    fn load_preset_map(preset_number: usize, viewport_size: Size, engine: &dyn Engine) -> Self {
        // Override GameConfigs.load_map_setting
        let load_map_setting = LoadMapSetting::Preset { preset_number };
        Self::new(&load_map_setting, viewport_size, engine, false)
    }

    fn reset(&mut self, engine: &dyn Engine, reset_map: bool, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>, home_menu: bool) {
        undo_redo::clear();
        self.tile_selection = TileSelection::default();
        self.menus = Some(self.new_game_menus_from_config(engine, home_menu));
        self.sim.reset_world(&mut self.world, &mut self.systems, &mut self.tile_map);

        if reset_map && (self.tile_map.size_in_cells().is_valid() || new_map_size.is_some()) {
            self.tile_map.reset(reset_map_with_tile_def, new_map_size);

            if reset_map_with_tile_def.is_some() {
                // Randomize terrain tiles.
                self.tile_map.for_each_tile_mut(TileMapLayerKind::Terrain,
                                                TileKind::Terrain,
                                                |terrain| {
                                                    if terrain.has_flags(TileFlags::RandomizePlacement) {
                                                        terrain.set_random_variation_index(&mut rand::rng());
                                                    }
                                                });
            }

            log::info!(log::channel!("session"),
                       "Map size: {}x{}.",
                       self.tile_map.size_in_cells().width,
                       self.tile_map.size_in_cells().height);

            self.camera.set_map_size_in_cells(self.tile_map.size_in_cells());
        }

        self.sim.reset_search_graph(&self.tile_map);
        self.camera.center();
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

        if let Some(menus) = &mut self.menus {
            menus.pre_save();
        }
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

        if let Some(menus) = &mut self.menus {
            menus.post_save();
        }
    }
}

impl Load for GameSession {
    fn pre_load(&mut self, context: &PreLoadContext) {
        self.tile_map.pre_load(context);
        self.world.pre_load(context);
        self.sim.pre_load(context);
        self.systems.pre_load(context);
        self.camera.pre_load(context);
        self.tile_selection.pre_load(context);

        if let Some(menus) = &mut self.menus {
            menus.pre_load(context);
        }
    }

    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        undo_redo::clear();

        self.tile_map.post_load(context);
        self.world.post_load(context);
        self.sim.post_load(context);
        self.systems.post_load(context);
        self.camera.post_load(context);
        self.tile_selection.post_load(context);

        let mut menus = self.new_game_menus_from_config(context.engine(), false);
        menus.post_load(context);
        self.menus = Some(menus);
    }
}

// ----------------------------------------------
// Save Game
// ----------------------------------------------

pub const AUTOSAVE_FILE_NAME: &str = "autosave.json";
pub const DEFAULT_SAVE_FILE_NAME: &str = "save_game.json";

fn save_games_dir() -> PathBuf {
    paths::base_path("saves")
}

fn make_save_game_file_path(save_file_name: &str) -> String {
    Path::new(&save_games_dir()).join(save_file_name)
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
                log::error!(log::channel!("session"),
                            "Failed to write save game file '{save_file_path}': {err}");
                return false;
            }

            true
        }

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = std::fs::create_dir_all(save_games_dir());

        if !can_write_save_file(save_file_path) {
            log::error!(log::channel!("session"),
                        "Save game file path '{save_file_path}' is not accessible!");
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
            log::error!(log::channel!("session"),
                        "Failed to read save game file '{save_file_path}': {err}");
            return false;
        }

        // Load into a temporary instance so that if we fail we'll avoid modifying any
        // state.
        let session: GameSession = match state.load_new_instance() {
            Ok(session) => session,
            Err(err) => {
                log::error!(log::channel!("session"),
                            "Failed to load save game from '{save_file_path}': {err}");
                return false;
            }
        };

        let engine = GameLoop::get().engine();

        self.pre_load(&PreLoadContext::new(engine));
        *self = session;
        self.post_load(&PostLoadContext::new(engine, &self.tile_map, self.sim.rng()));

        if GameConfigs::get().sim.start_paused {
            self.sim.pause();
        } else {
            self.sim.resume();
        }

        true
    }
}

// ----------------------------------------------
// GameSessionCmd
// ----------------------------------------------

// Deferred session commands that must be processed at a safe point in the
// GameLoop update. These are kept in a queue and consumed every iteration of
// the loop.
enum GameSessionCmd {
    Reset { reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size> },
    LoadPreset { preset_number: usize },
    LoadSaveGame { save_file_path: String },
    SaveGame { save_file_path: String },
    QuitToMainMenu,
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
        let build_profile = platform::build_profile();
        let run_environment = platform::run_environment();
        let is_app_bundle = run_environment == platform::RunEnvironment::MacOSAppBundle;

        // Early initialization:
        log::redirect_to_file(is_app_bundle);
        LogViewerWindow::early_init();
        paths::set_default_working_dir();

        // Only log panics when running from a bundle. Otherwise the default behavior is fine.
        crash_report::initialize(is_app_bundle);

        log::info!(log::channel!("game"), "--- Game Initialization ---");

        log::info!(log::channel!("game"), "Base dir: {:?}", paths::base_dir());
        log::info!(log::channel!("game"), "Assets dir: {:?}", paths::assets_dir());

        log::info!(log::channel!("game"), "Running in {build_profile} profile.");
        log::info!(log::channel!("game"), "{run_environment} environment.");
        log::info!(log::channel!("game"), "Redirect log to file: {is_app_bundle}.");

        log::info!(log::channel!("game"), "Loading Game Configs ...");
        let configs = GameConfigs::load();

        // Boot the engine and load assets:
        let engine = Self::init_engine(&configs.engine);
        Self::load_assets(engine.texture_cache(), configs);

        // Global initialization:
        cheats::initialize();
        undo_redo::initialize();
        Simulation::register_callbacks();
        debug::set_show_popup_messages(configs.debug.show_popups);
        debug::init_dev_editor_menus(configs, engine.texture_cache());
        CameraGlobalSettings::get_mut().set_from_game_configs(configs);

        let instance = Self {
            engine,
            session: None,
            session_cmd_queue: VecDeque::new(),
            autosave_timer: UpdateTimer::new(configs.save.autosave_frequency_secs),
            enable_autosave: configs.save.enable_autosave
        };

        GameLoop::initialize(instance); // Set global instance.
        GameLoop::get_mut() // Return it.
    }

    pub fn create_session(&mut self) {
        debug_assert!(self.session.is_none());

        let config = GameConfigs::get();
        let home_menu = !config.debug.skip_home_menu;

        let viewport_size = self.engine.viewport().size();
        let new_session = GameSession::new(&config.save.load_map_setting, viewport_size, self.engine(), home_menu);

        self.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "--- Game Session created ---");
    }

    pub fn terminate_session(&mut self) {
        if let Some(session) = &mut self.session {
            session.reset(self.engine.as_ref(), false, None, None, false);
        }
        self.session = None;
        log::info!(log::channel!("game"), "--- Game Session destroyed ---");
    }

    pub fn reset_session(&mut self, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.session_cmd_queue.push_back(GameSessionCmd::Reset { reset_map_with_tile_def, new_map_size });
    }

    pub fn quit_to_main_menu(&mut self) {
        self.session_cmd_queue.push_back(GameSessionCmd::QuitToMainMenu);
    }

    pub fn load_preset_map(&mut self, preset_tile_map_number: usize) {
        self.session_cmd_queue
            .push_back(GameSessionCmd::LoadPreset { preset_number: preset_tile_map_number });
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
        file_sys::collect_files(&save_games_dir(),
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
    pub fn is_running(&self) -> bool {
        self.session.is_some() && self.engine.is_running()
    }

    #[inline]
    pub fn is_on_home_menus(&self) -> bool {
        if let Some(session) = &self.session
            && session.current_menus_mode() == Some(GameMenusMode::Home)
        {
            return true;
        }
        false
    }

    #[inline]
    pub fn is_in_game(&self) -> bool {
        self.session.is_some() && !self.is_on_home_menus()
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
    pub fn camera(&self) -> &Camera {
        &self.session.as_ref().unwrap().camera
    }

    #[inline]
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.session.as_mut().unwrap().camera
    }

    #[inline]
    fn session(&self) -> &GameSession {
        self.session.as_ref().unwrap()
    }

    #[inline]
    fn session_mut(&mut self) -> &mut GameSession {
        self.session.as_mut().unwrap()
    }

    pub fn request_quit(&mut self) {
        self.engine_mut().app().request_quit();
    }

    pub fn update(&mut self) {
        self.update_autosave();
        self.process_session_commands();

        let (delta_time_secs, cursor_screen_pos) = self.engine.begin_frame();

        // Input Events:
        for event in self.engine.app_events().clone() {
            self.handle_event(event, cursor_screen_pos, delta_time_secs);
        }

        let viewport_size = self.engine.app().window_size();
        self.camera_mut().set_viewport_size(viewport_size);

        // Game Logic:
        let visible_range = self.update_simulation(cursor_screen_pos, delta_time_secs);

        // Rendering:
        let render_flags = self.menus_begin_frame(cursor_screen_pos, delta_time_secs);

        self.draw_tile_map(delta_time_secs, visible_range, render_flags);

        self.menus_end_frame(visible_range, cursor_screen_pos, delta_time_secs);

        let listener_position = self.camera().iso_world_position();
        self.engine.sound_system().update(listener_position);

        self.engine.end_frame();
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn init_engine(configs: &EngineConfigs) -> Box<dyn Engine> {
        log::info!(log::channel!("game"), "Init Engine: GLFW + OpenGL");
        Box::new(engine::backend::GlfwOpenGlEngine::new(configs))
    }

    fn load_assets(tex_cache: &mut dyn TextureCache, configs: &GameConfigs) {
        log::info!(log::channel!("game"), "--- Loading Game Assets ---");
        paths::set_default_working_dir();

        BuildingConfigs::load();
        log::info!(log::channel!("game"), "BuildingConfigs loaded.");

        UnitConfigs::load();
        log::info!(log::channel!("game"), "UnitConfigs loaded.");

        PropConfigs::load();
        log::info!(log::channel!("game"), "PropConfigs loaded.");

        TileSets::load(tex_cache, configs.engine.use_packed_texture_atlas, configs.debug.skip_loading_tile_sets);
        log::info!(log::channel!("game"), "TileSets loaded.");
    }

    // Create system if it doesn't already exist in the session.
    fn create_system<System>(&mut self, system: System)
        where System: GameSystem + 'static,
              GameSystemImpl: From<System>
    {
        if let Some(session) = &mut self.session {
            if !session.systems.has(system.type_id()) {
                session.systems.register(system);
            }
        }
    }

    fn handle_event(&mut self, event: ApplicationEvent, cursor_screen_pos: Vec2, delta_time_secs: Seconds) {
        match event {
            ApplicationEvent::WindowResize(window_size) => {
                self.camera_mut().set_viewport_size(window_size);
            }
            ApplicationEvent::KeyInput(key, action, modifiers) => {
                let mut propagate = true;
                let camera_settings = CameraGlobalSettings::get();

                // [CTRL]+[-] / [CTRL]+[=]: Zoom in/out by a fixed step.
                if !camera_settings.disable_key_shortcut_zoom
                    && action == InputAction::Press
                    && modifiers.intersects(InputModifiers::Control)
                {
                    let camera = self.camera_mut();

                    if key == InputKey::Minus {
                        let step = camera_settings.fixed_step_zoom_amount;
                        camera.set_zoom(camera.current_zoom() - step);
                        propagate = false;
                    } else if key == InputKey::Equal {
                        let step = camera_settings.fixed_step_zoom_amount;
                        camera.set_zoom(camera.current_zoom() + step);
                        propagate = false;
                    }
                }

                // [CTRL]+[/]: Toggle between DevEditor menu / HUD menu.
                if action == InputAction::Press
                    && key == InputKey::Slash
                    && modifiers.intersects(InputModifiers::Control)
                {
                    self.session.as_mut().unwrap().toggle_menus_mode(self.engine.as_ref());
                }

                if propagate {
                    self.menus_on_key_input(key, action, modifiers, cursor_screen_pos, delta_time_secs);
                }
            }
            ApplicationEvent::Scroll(amount) => {
                let mut propagate = true;
                let camera_settings = CameraGlobalSettings::get();

                // If we're not hovering over an ImGui menu...
                if !camera_settings.disable_mouse_scroll_zoom
                    && !self.engine().ui_system().is_handling_mouse_input()
                {
                    let camera = self.camera_mut();

                    if camera_settings.disable_smooth_mouse_scroll_zoom {
                        // Fixed step zoom.
                        if amount.y < 0.0 {
                            camera.set_zoom(camera.current_zoom() + camera_settings.fixed_step_zoom_amount);
                            propagate = false;
                        } else if amount.y > 0.0 {
                            camera.set_zoom(camera.current_zoom() - camera_settings.fixed_step_zoom_amount);
                            propagate = false;
                        }
                    } else {
                        // Smooth interpolated zoom.
                        if amount.y < 0.0 {
                            camera.request_zoom(CameraZoom::In);
                            propagate = false;
                        } else if amount.y > 0.0 {
                            camera.request_zoom(CameraZoom::Out);
                            propagate = false;
                        }
                    }
                }

                if propagate {
                    self.menus_on_scroll(amount, cursor_screen_pos, delta_time_secs);
                }
            }
            ApplicationEvent::MouseButton(button, action, modifiers) => {
                self.menus_on_mouse_button(button, action, modifiers, cursor_screen_pos, delta_time_secs);
            }
            _ => {}
        }
    }

    fn update_simulation(&mut self,
                         cursor_screen_pos: Vec2,
                         delta_time_secs: Seconds)
                         -> CellRange {
        let ui_hovered = self.engine.ui_system().ui().is_any_item_hovered();
        let session = self.session_mut();

        session.camera.update_zooming(delta_time_secs);

        // Map scrolling:
        session.camera.update_scrolling(ui_hovered, cursor_screen_pos, delta_time_secs);

        session.sim.update(&mut session.world,
                           &mut session.systems,
                           &mut session.tile_map,
                           delta_time_secs);

        let visible_range = session.camera.visible_cells_range();

        if !session.sim.is_paused() {
            let scaled_delta_time_secs = delta_time_secs * session.sim.speed();
            session.tile_map.update_anims(visible_range, scaled_delta_time_secs);
        }

        visible_range
    }

    fn draw_tile_map(&mut self,
                     delta_time_secs: Seconds,
                     visible_range: CellRange,
                     flags: TileMapRenderFlags) {
        let tex_cache = self.engine.texture_cache();
        let input_sys = self.engine.input_system();
        let ui_sys = self.engine.ui_system();
        let session = self.session.as_mut().unwrap();

        let dev_editor_menus_mode =
            session.current_menus_mode() == Some(GameMenusMode::DevEditor);

        let enable_minimap_debug_controls =
            GameConfigs::get().debug.enable_minimap_debug_controls && dev_editor_menus_mode;

        session.tile_map.minimap_mut().update(&mut session.camera,
                                              tex_cache,
                                              input_sys,
                                              ui_sys,
                                              delta_time_secs);

        self.engine.draw_tile_map(&mut session.tile_map,
                                  &session.tile_selection,
                                  &session.camera,
                                  visible_range,
                                  flags);

        session.tile_map.minimap_mut().draw_debug_ui(&mut session.camera,
                                                     self.engine.ui_system(),
                                                     enable_minimap_debug_controls);
    }

    fn update_autosave(&mut self) {
        if !self.enable_autosave || !self.is_in_game() {
            return; // Don't autosave while in the home/main menu.
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
                GameSessionCmd::Reset { reset_map_with_tile_def, new_map_size } => {
                    self.session_cmd_reset(reset_map_with_tile_def, new_map_size);
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
                GameSessionCmd::QuitToMainMenu => {
                    self.session_cmd_quit_to_main_menu();
                }
            }
        }
    }

    fn session_cmd_reset(&mut self, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.session.as_mut().unwrap().reset(self.engine.as_ref(), true, reset_map_with_tile_def, new_map_size, false);
        log::info!(log::channel!("game"), "Game Session reset.");
    }

    fn session_cmd_load_preset(&mut self, preset_number: usize) {
        self.terminate_session();

        let viewport_size = self.engine.viewport().size();
        let new_session = GameSession::load_preset_map(preset_number, viewport_size, self.engine());

        self.session = Some(Box::new(new_session));
        log::info!(log::channel!("game"), "--- Game Session created ---");
    }

    fn session_cmd_load_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session_mut().load_save_game(&save_file_path);
    }

    fn session_cmd_save_game(&mut self, save_file_path: String) {
        debug_assert!(!save_file_path.is_empty());
        self.session_mut().save_game(&save_file_path);
    }

    fn session_cmd_quit_to_main_menu(&mut self) {
        self.terminate_session();
        self.create_session();
    }

    // ----------------------
    // In-Game UI / Debug UI:
    // ----------------------

    fn menus_begin_frame(&mut self,
                         cursor_screen_pos: Vec2,
                         delta_time_secs: Seconds)
                         -> TileMapRenderFlags {
        let session = self.session.as_mut().unwrap();
        if let Some(menus) = &mut session.menus {
            menus.begin_frame(&mut GameMenusContext {
                engine: self.engine.as_mut(),
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                sim: &mut session.sim,
                world: &mut session.world,
                systems: &mut session.systems,
                camera: &mut session.camera,
                cursor_screen_pos,
                delta_time_secs
            });
            menus.selected_render_flags()
        } else {
            TileMapRenderFlags::DrawTerrainAndObjects
        }
    }

    fn menus_end_frame(&mut self,
                       visible_range: CellRange,
                       cursor_screen_pos: Vec2,
                       delta_time_secs: Seconds) {
        let session = self.session.as_mut().unwrap();
        if let Some(menus) = &mut session.menus {
            menus.end_frame(&mut GameMenusContext {
                engine: self.engine.as_mut(),
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                sim: &mut session.sim,
                world: &mut session.world,
                systems: &mut session.systems,
                camera: &mut session.camera,
                cursor_screen_pos,
                delta_time_secs
            },
            visible_range);
        }
    }

    fn menus_on_key_input(&mut self,
                          key: InputKey,
                          action: InputAction,
                          modifiers: InputModifiers,
                          cursor_screen_pos: Vec2,
                          delta_time_secs: Seconds)
                          -> UiInputEvent {
        let session = self.session.as_mut().unwrap();
        if let Some(menus) = &mut session.menus {
            menus.handle_input(&mut GameMenusContext {
                engine: self.engine.as_mut(),
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                sim: &mut session.sim,
                world: &mut session.world,
                systems: &mut session.systems,
                camera: &mut session.camera,
                cursor_screen_pos,
                delta_time_secs
            },
            GameMenusInputArgs::Key { key, action, modifiers })
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn menus_on_mouse_button(&mut self,
                             button: MouseButton,
                             action: InputAction,
                             modifiers: InputModifiers,
                             cursor_screen_pos: Vec2,
                             delta_time_secs: Seconds)
                             -> UiInputEvent {
        let session = self.session.as_mut().unwrap();
        if let Some(menus) = &mut session.menus {
            menus.handle_input(&mut GameMenusContext {
                engine: self.engine.as_mut(),
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                sim: &mut session.sim,
                world: &mut session.world,
                systems: &mut session.systems,
                camera: &mut session.camera,
                cursor_screen_pos,
                delta_time_secs
            },
            GameMenusInputArgs::Mouse { button, action, modifiers })
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn menus_on_scroll(&mut self,
                       amount: Vec2,
                       cursor_screen_pos: Vec2,
                       delta_time_secs: Seconds)
                       -> UiInputEvent {
        let session = self.session.as_mut().unwrap();
        if let Some(menus) = &mut session.menus {
            menus.handle_input(&mut GameMenusContext {
                engine: self.engine.as_mut(),
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                sim: &mut session.sim,
                world: &mut session.world,
                systems: &mut session.systems,
                camera: &mut session.camera,
                cursor_screen_pos,
                delta_time_secs
            },
            GameMenusInputArgs::Scroll { amount })
        } else {
            UiInputEvent::NotHandled
        }
    }
}

// ----------------------------------------------
// GameLoop Global Singleton
// ----------------------------------------------

singleton_late_init! { GAME_LOOP_SINGLETON, GameLoop }
