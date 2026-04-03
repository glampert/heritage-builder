use std::{collections::VecDeque, path::PathBuf};
use serde::{Deserialize, Serialize};

use super::{
    undo_redo,
    world::World,
    sim::Simulation,
    system::GameSystems,
    config::{GameConfigs, LoadMapSetting},
    menu::{
        GameMenusMode,
        GameMenusSystem,
        GameMenusInputArgs,
        home::HomeMenus,
        in_game::InGameMenus,
    }
};

use crate::{
    log,
    camera::*,
    save::{self, *},
    engine::Engine,
    file_sys::paths::PathRef,
    debug::{self, DevEditorMenus},
    ui::UiInputEvent,
    app::input::{InputAction, InputKey, InputModifiers, MouseButton},
    utils::{Vec2, Size, hash, coords::CellRange, mem::RcMut, time::Seconds},
    tile::{
        sets::TileDef,
        selection::TileSelection,
        rendering::{TileMapRenderFlags, TileMapRenderStats, TileMapRenderer},
        TileKind, TileFlags, TileMap, TileMapLayerKind,
    },
};

// ----------------------------------------------
// GameSessionCmd
// ----------------------------------------------

// Deferred session commands that must be processed at a safe point in the
// GameLoop update. These are kept in a queue and consumed every iteration
// of the game loop.
enum GameSessionCmd {
    QuitToMainMenu,
    ToggleMenusMode,
    Reset { reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size> },
    LoadPreset { preset_number: usize },
    LoadSaveGame { save_file: PathBuf },
    SaveGame { save_file: PathBuf },
}

// ----------------------------------------------
// GameSessionCmdQueue
// ----------------------------------------------

pub struct GameSessionCmdQueue {
    queue: VecDeque<GameSessionCmd>,
}

impl GameSessionCmdQueue {
    pub fn new() -> Self {
        Self { queue: VecDeque::with_capacity(8) }
    }

    pub fn push_quit_to_main_menu(&mut self) {
        self.queue.push_back(GameSessionCmd::QuitToMainMenu);
    }

    pub fn push_toggle_menus_mode(&mut self) {
        self.queue.push_back(GameSessionCmd::ToggleMenusMode);
    }

    pub fn push_reset_session(&mut self, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.queue.push_back(GameSessionCmd::Reset {
            reset_map_with_tile_def,
            new_map_size,
        });
    }

    pub fn push_load_preset_map(&mut self, preset_number: usize) {
        self.queue.push_back(GameSessionCmd::LoadPreset {
            preset_number,
        });
    }

    pub fn push_load_save_game(&mut self, save_file_name: PathRef) {
        if save_file_name.is_empty() {
            log::error!(log::channel!("session"), "Load game: Empty file name!");
            return;
        }

        self.queue.push_back(GameSessionCmd::LoadSaveGame {
            save_file: save_file_name.to_path_buf(),
        });
    }

    pub fn push_save_game(&mut self, save_file_name: PathRef) {
        if save_file_name.is_empty() {
            log::error!(log::channel!("session"), "Save game: Empty file name!");
            return;
        }

        self.queue.push_back(GameSessionCmd::SaveGame {
            save_file: save_file_name.to_path_buf(),
        });
    }

    pub fn execute(&mut self, session: &mut GameSession, engine: &mut Engine, configs: &'static GameConfigs) {
        while let Some(cmd) = self.queue.pop_front() {
            match cmd {
                GameSessionCmd::QuitToMainMenu => {
                    self.cmd_quit_to_main_menu(session, engine, configs);
                }
                GameSessionCmd::ToggleMenusMode => {
                    self.cmd_toggle_menus_mode(session, engine);
                }
                GameSessionCmd::Reset { reset_map_with_tile_def, new_map_size } => {
                    self.cmd_reset_session(session, engine, configs, reset_map_with_tile_def, new_map_size);
                }
                GameSessionCmd::LoadPreset { preset_number } => {
                    self.cmd_load_preset(session, engine, configs, preset_number);
                }
                GameSessionCmd::LoadSaveGame { save_file } => {
                    self.cmd_load_save_game(session, engine, configs, PathRef::from_path(&save_file));
                }
                GameSessionCmd::SaveGame { save_file } => {
                    self.cmd_save_game(session, PathRef::from_path(&save_file));
                }
            }
        }
    }

    // ----------------------
    // Command Impls:
    // ----------------------

    fn cmd_quit_to_main_menu(&mut self,
                             session: &mut GameSession,
                             engine: &mut Engine,
                             configs: &'static GameConfigs)
    {
        destroy(session, engine, configs);
        *session = create(engine, configs, None);
    }

    fn cmd_toggle_menus_mode(&mut self,
                             session: &mut GameSession,
                             engine: &mut Engine)
    {
        session.toggle_menus_mode(engine);
    }

    fn cmd_reset_session(&mut self,
                         session: &mut GameSession,
                         engine: &mut Engine,
                         configs: &'static GameConfigs,
                         reset_map_with_tile_def: Option<&'static TileDef>,
                         new_map_size: Option<Size>)
    {
        let reset_map = true;
        let home_menu = false;
        session.reset(engine, configs, reset_map, reset_map_with_tile_def, new_map_size, home_menu);
        log::info!(log::channel!("session"), "--- Game Session Reset ---");
    }

    fn cmd_load_preset(&mut self,
                       session: &mut GameSession,
                       engine: &mut Engine,
                       configs: &'static GameConfigs,
                       preset_number: usize)
    {
        destroy(session, engine, configs);
        *session = create(engine, configs, Some(preset_number));
    }

    fn cmd_load_save_game(&mut self,
                          session: &mut GameSession,
                          engine: &mut Engine,
                          configs: &'static GameConfigs,
                          save_file: PathRef)
    {
        debug_assert!(!save_file.is_empty());
        session.load_save_game(engine, configs, save_file);
    }

    fn cmd_save_game(&mut self,
                     session: &mut GameSession,
                     save_file: PathRef)
    {
        debug_assert!(!save_file.is_empty());
        session.save_game(save_file);
    }
}

// ----------------------------------------------
// Session creation helpers
// ----------------------------------------------

pub fn create(engine: &mut Engine, configs: &'static GameConfigs, preset_number: Option<usize>) -> GameSession {
    let session = {
        if let Some(preset_number) = preset_number {
            GameSession::create_with_preset_map(engine, configs, preset_number)
        } else {
            let load_map_setting = &configs.save.load_map_setting;
            let home_menu = !configs.debug.skip_home_menu;

            GameSession::create_with_settings(engine, configs, load_map_setting, home_menu)
        }
    };

    log::info!(log::channel!("session"), "--- Game Session Created ---");
    session
}

pub fn destroy(session: &mut GameSession, engine: &mut Engine, configs: &'static GameConfigs) {
    session.reset(engine, configs, false, None, None, false);
    log::info!(log::channel!("session"), "--- Game Session Destroyed ---");
}

// ----------------------------------------------
// Macro: make_ui_widget_context
// ----------------------------------------------

// Helper macro to create a new GameUiContext from GameSession member variables.
macro_rules! make_ui_widget_context {
    ($session:ident, $engine:ident) => {
        super::ui_context::GameUiContext::new(
            &mut $session.sim,
            &mut $session.world,
            &mut $session.tile_map,
            &mut $session.tile_selection,
            &mut $session.camera,
            $engine
        )
    }
}

// ----------------------------------------------
// GameSession
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct GameSession {
    tile_map: RcMut<TileMap>,
    world: World,
    sim: Simulation,
    systems: GameSystems,
    camera: Camera,

    // NOTE: The following members are not serialized on save games.
    // We only need to invoke pre_load/post_load on them.

    #[serde(skip)]
    tile_selection: TileSelection,

    #[serde(skip)]
    tile_map_renderer: TileMapRenderer,

    #[serde(skip)]
    menus: Option<Box<dyn GameMenusSystem>>,
}

impl GameSession {
    // ----------------------
    // Public Accessors:
    // ----------------------

    #[inline]
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    #[inline]
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    #[inline]
    pub fn systems(&self) -> &GameSystems {
        &self.systems
    }

    #[inline]
    pub fn systems_mut(&mut self) -> &mut GameSystems {
        &mut self.systems
    }

    #[inline]
    pub fn sim(&self) -> &Simulation {
        &self.sim
    }

    #[inline]
    pub fn sim_mut(&mut self) -> &mut Simulation {
        &mut self.sim
    }

    #[inline]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[inline]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    #[inline]
    pub fn tile_map(&self) -> &TileMap {
        &self.tile_map
    }

    #[inline]
    pub fn tile_map_mut(&mut self) -> &mut TileMap {
        &mut self.tile_map
    }

    #[inline]
    pub fn current_menus_mode(&self) -> Option<GameMenusMode> {
        self.menus.as_ref().map(|menus| menus.mode())
    }

    #[inline]
    pub fn tile_map_render_stats(&self) -> &TileMapRenderStats {
        self.tile_map_renderer.stats()
    }

    #[inline]
    pub fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.tile_map_renderer.set_grid_line_thickness(thickness);
    }

    #[inline]
    pub fn grid_line_thickness(&self) -> f32 {
        self.tile_map_renderer.grid_line_thickness()
    }

    // ----------------------
    // Update & Rendering:
    // ----------------------

    pub fn update_simulation(&mut self, engine: &mut Engine, delta_time_secs: Seconds) {
        self.sim.update(engine,
                        &mut self.world,
                        &mut self.systems,
                        &mut self.tile_map,
                        delta_time_secs);
    }

    pub fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: Seconds) {
        if !self.sim.is_paused() {
            let scaled_delta_time_secs = delta_time_secs * self.sim.speed();
            self.tile_map.update_anims(visible_range, scaled_delta_time_secs);
        }
    }

    pub fn draw_tile_map(&mut self,
                         engine: &mut Engine,
                         delta_time_secs: Seconds,
                         visible_range: CellRange,
                         flags: TileMapRenderFlags)
    {
        let systems = engine.systems_mut_refs();
        let tex_cache = systems.render_sys.texture_cache_mut();

        self.tile_map.minimap_mut().update(&mut self.camera,
                                           tex_cache,
                                           systems.input_sys,
                                           systems.ui_sys,
                                           delta_time_secs);

        if self.tile_map.size_in_cells().is_valid() {
            self.tile_map_renderer.draw_map(systems.render_sys,
                                            systems.debug_draw,
                                            systems.ui_sys,
                                            &self.tile_map,
                                            self.camera.transform(),
                                            visible_range,
                                            flags);

            self.tile_selection.draw(engine.render_system_mut());
        }
    }

    // ----------------------
    // Session Create/Reset:
    // ----------------------

    fn create_with_settings(engine: &mut Engine,
                            configs: &'static GameConfigs,
                            load_map_setting: &LoadMapSetting,
                            home_menu: bool) -> Self
    {
        let viewport_size = engine.viewport().integer_size();

        if !viewport_size.is_valid() {
            panic!("Invalid game viewport size!");
        }

        let mut world = World::new();
        let tile_map = Self::create_tile_map(&mut world, load_map_setting);

        let sim = Simulation::new(&tile_map, configs);
        let systems = GameSystems::register_all();

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
            tile_map_renderer: TileMapRenderer::new(configs.engine.grid_color, configs.engine.grid_line_thickness),
            menus: None,
        };

        session.menus = Some(session.create_game_menus_from_config(engine, configs, home_menu));

        if let LoadMapSetting::SaveGame { save_file } = load_map_setting {
            session.load_save_game(engine, configs, PathRef::from_path(save_file));
        }

        if configs.sim.start_paused {
            session.sim.pause();
        } else {
            session.sim.resume();
        }

        session
    }

    fn create_with_preset_map(engine: &mut Engine,
                              configs: &'static GameConfigs,
                              preset_number: usize) -> Self
    {
        // Override GameConfigs.load_map_setting
        Self::create_with_settings(engine,
                                   configs,
                                   &LoadMapSetting::Preset { preset_number },
                                   false)
    }

    fn reset(&mut self,
             engine: &mut Engine,
             configs: &'static GameConfigs,
             reset_map: bool,
             reset_map_with_tile_def: Option<&'static TileDef>,
             new_map_size: Option<Size>,
             home_menu: bool)
    {
        undo_redo::clear();

        self.tile_selection = TileSelection::default();
        self.menus = Some(self.create_game_menus_from_config(engine, configs, home_menu));
        self.sim.reset_world(engine, &mut self.world, &mut self.systems, &mut self.tile_map);

        if reset_map && (self.tile_map.size_in_cells().is_valid() || new_map_size.is_some()) {
            self.tile_map.reset(reset_map_with_tile_def, new_map_size);

            if reset_map_with_tile_def.is_some() {
                // Randomize terrain tiles.
                self.tile_map.for_each_tile_mut(
                    TileMapLayerKind::Terrain,
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

    // ----------------------
    // Game Menus Setup:
    // ----------------------

    fn toggle_menus_mode(&mut self, engine: &mut Engine) {
        if let Some(mode) = self.current_menus_mode() {
            match mode {
                GameMenusMode::DevEditor => {
                    self.menus = Some(self.create_game_menus(engine, GameMenusMode::InGame));
                }
                GameMenusMode::InGame => {
                    self.menus = Some(self.create_game_menus(engine, GameMenusMode::DevEditor));
                }
                GameMenusMode::Home => {} // Cannot toggle out of home menu.
            }
        }
    }

    fn create_game_menus(&mut self, engine: &mut Engine, menu_mode: GameMenusMode) -> Box<dyn GameMenusSystem> {
        let tile_map_rc = self.tile_map.clone();
        let mut context = make_ui_widget_context!(self, engine);

        match menu_mode {
            GameMenusMode::DevEditor => {
                log::info!(log::channel!("session"), "Loading DevEditorMenus ...");
                Box::new(DevEditorMenus::new(&mut context, tile_map_rc))
            }
            GameMenusMode::InGame => {
                log::info!(log::channel!("session"), "Loading InGameMenus ...");
                Box::new(InGameMenus::new(&mut context))
            }
            GameMenusMode::Home => {
                log::info!(log::channel!("session"), "Loading HomeMenus ...");
                Box::new(HomeMenus::new(&mut context))
            }
        }
    }

    fn create_game_menus_from_config(&mut self,
                                     engine: &mut Engine,
                                     configs: &'static GameConfigs,
                                     home_menu: bool) -> Box<dyn GameMenusSystem>
    {
        let menu_mode = {
            if configs.debug.skip_home_menu || !home_menu {
                if configs.debug.start_in_dev_editor_mode {
                    GameMenusMode::DevEditor
                } else {
                    GameMenusMode::InGame
                }
            } else {
                GameMenusMode::Home
            }
        };

        self.create_game_menus(engine, menu_mode)
    }

    // ----------------------
    // Tile Map Setup:
    // ----------------------

    fn create_tile_map(world: &mut World, load_map_setting: &LoadMapSetting) -> RcMut<TileMap> {
        RcMut::new(match load_map_setting {
            LoadMapSetting::None => {
                TileMap::default() // Empty dummy map.
            }
            LoadMapSetting::EmptyMap { size_in_cells,
                                       terrain_tile_category,
                                       terrain_tile_name } => {
                let map_size = if !size_in_cells.is_valid() {
                    log::error!(log::channel!("session"),
                                "LoadMapSetting::EmptyMap: Invalid Tile Map dimensions! Width & height must not be zero.");
                    Size::new(64, 64) // Default fallback.
                } else {
                    *size_in_cells
                };

                log::info!(log::channel!("session"),
                           "Creating empty Tile Map. Size: {map_size}, Fill: {terrain_tile_name}");

                let mut tile_map =
                    TileMap::with_terrain_tile(map_size,
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
            LoadMapSetting::SaveGame { save_file } => {
                if save_file.to_str().unwrap().is_empty() {
                    log::error!(log::channel!("session"), "LoadMapSetting::SaveGame: No save file path provided!");
                }

                // Loading a save requires loading a full GameSession, so we'll just create
                // a dummy map here. The actual loading will be handled by the caller.
                TileMap::default()
            }
        })
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
    fn pre_load(&mut self, context: &mut PreLoadContext) {
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

    fn post_load(&mut self, context: &mut PostLoadContext) {
        undo_redo::clear();

        self.tile_map.post_load(context);
        self.world.post_load(context);
        self.sim.post_load(context);
        self.systems.post_load(context);
        self.camera.post_load(context);
        self.tile_selection.post_load(context);

        let (configs, engine) = context.configs_and_engine();

        let mut menus = self.create_game_menus_from_config(engine, configs, false);
        menus.post_load(context);
        self.menus = Some(menus);

        if configs.sim.start_paused {
            self.sim.pause();
        } else {
            self.sim.resume();
        }
    }
}

// ----------------------------------------------
// Save/Load Game
// ----------------------------------------------

impl GameSession {
    fn save_game(&mut self, save_file: PathRef) -> bool {
        log::info!(log::channel!("session"), "Saving game '{save_file}' ...");

        if !save::storage::can_write_save_file(save_file) {
            log::error!(log::channel!("session"), "Save game file path '{save_file}' is not accessible!");
            return false; 
        }

        self.pre_save();
        let save_result = save::storage::write_save_file(save_file, self);
        self.post_save();

        if let Err(err) = save_result {
            log::error!(log::channel!("session"), "{err}");
            return false;
        }

        true
    }

    fn load_save_game(&mut self, engine: &mut Engine, configs: &'static GameConfigs, save_file: PathRef) -> bool {
        log::info!(log::channel!("session"), "Loading save game '{save_file}' ...");

        let session = match save::storage::load_save_file(save_file) {
            Ok(session) => session,
            Err(err) => {
                log::error!(log::channel!("session"), "{err}");
                return false;
            }
        };

        self.pre_load(&mut PreLoadContext::new(engine));
        *self = session;
        self.post_load(&mut PostLoadContext::new(
            engine,
            configs,
            self.sim.rng().clone(),
            self.tile_map.clone()
        ));

        true
    }
}

// ----------------------------------------------
// In-Game UI / Debug UI
// ----------------------------------------------

impl GameSession {
    pub fn menus_begin_frame(&mut self, engine: &mut Engine) -> TileMapRenderFlags {
        if let Some(menus) = &mut self.menus {
            menus.begin_frame(&mut make_ui_widget_context!(self, engine));
            menus.selected_render_flags()
        } else {
            TileMapRenderFlags::DrawTerrainAndObjects
        }
    }

    pub fn menus_end_frame(&mut self, engine: &mut Engine, visible_range: CellRange) {
        if let Some(menus) = &mut self.menus {
            menus.end_frame(&mut make_ui_widget_context!(self, engine), visible_range);
        }
    }

    pub fn menus_on_key_input(&mut self,
                              engine: &mut Engine,
                              key: InputKey,
                              action: InputAction,
                              modifiers: InputModifiers)
                              -> UiInputEvent
    {
        if let Some(menus) = &mut self.menus {
            menus.handle_input(
                &mut make_ui_widget_context!(self, engine),
                GameMenusInputArgs::Key { key, action, modifiers }
            )
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn menus_on_mouse_button(&mut self,
                                 engine: &mut Engine,
                                 button: MouseButton,
                                 action: InputAction,
                                 modifiers: InputModifiers)
                                 -> UiInputEvent
    {
        if let Some(menus) = &mut self.menus {
            menus.handle_input(
                &mut make_ui_widget_context!(self, engine),
                GameMenusInputArgs::Mouse { button, action, modifiers }
            )
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn menus_on_scroll(&mut self, engine: &mut Engine, amount: Vec2) -> UiInputEvent {
        if let Some(menus) = &mut self.menus {
            menus.handle_input(
                &mut make_ui_widget_context!(self, engine),
                GameMenusInputArgs::Scroll { amount }
            )
        } else {
            UiInputEvent::NotHandled
        }
    }
}
