use config::GameConfigs;
use menu::GameMenusMode;
use sim::Simulation;
use system::GameSystems;
use prop::config::PropConfigs;
use unit::config::UnitConfigs;
use building::config::BuildingConfigs;
use session::{GameSessionCmdQueue, GameSession};

use crate::{
    log,
    save,
    platform,
    camera::*,
    engine::Engine,
    ui::UiInputEvent,
    render::TextureCache,
    debug::{self, log_viewer::LogViewerWindow},
    file_sys::{self, paths::{self, PathRef}},
    app::{
        input::{InputAction, InputKey, InputModifiers, MouseButton},
        ApplicationEvent,
    },
    tile::{
        rendering::TileMapRenderFlags,
        sets::{TileDef, TileSets},
    },
    utils::{
        Size, Vec2,
        coords::CellRange,
        mem::singleton_late_init,
        time::{Seconds, Milliseconds, UpdateTimer, PerfTimer},
    },
};

#[cfg(feature = "desktop")]
use crate::{
    engine::{self, config::EngineConfigs},
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
pub mod session;
pub mod unit;
pub mod world;

// ----------------------------------------------
// GameLoopStats
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub struct GameLoopStats {
    pub fps: f32,
    pub total_frame_time_ms: Milliseconds,

    pub sim_frame_time_ms: Milliseconds,
    pub anim_frame_time_ms: Milliseconds,
    pub sound_frame_time_ms: Milliseconds,
    pub draw_world_frame_time_ms: Milliseconds,

    pub ui_begin_frame_time_ms: Milliseconds,
    pub ui_end_frame_time_ms: Milliseconds,

    pub engine_begin_frame_time_ms: Milliseconds,
    pub engine_end_frame_time_ms: Milliseconds,
    pub present_frame_time_ms: Milliseconds,
}

// ----------------------------------------------
// GameLoop
// ----------------------------------------------

pub struct GameLoop {
    engine: Box<dyn Engine>,

    session: Box<GameSession>,
    session_cmd_queue: GameSessionCmdQueue,

    autosave_timer: UpdateTimer,
    enable_autosave: bool,

    stats: GameLoopStats,
}

impl GameLoop {
    // ----------------------
    // Public API:
    // ----------------------

    // Desktop entry point: creates the engine internally, loads configs, and starts the game.
    #[cfg(feature = "desktop")]
    pub fn start() -> &'static mut Self {
        let build_profile = platform::build_profile();
        let run_environment = platform::run_environment();
        let is_app_bundle = run_environment == platform::RunEnvironment::MacOSAppBundle;

        // Early initialization:
        log::redirect_to_file(is_app_bundle);
        LogViewerWindow::initialize();
        file_sys::paths::set_working_directory(file_sys::paths::base_path());

        // Only log panics when running from a bundle. Otherwise the default behavior is fine.
        platform::initialize_crash_report(is_app_bundle);

        log::info!(log::channel!("game"), "--- Game Initialization ---");

        log::info!(log::channel!("game"), "Base path: {}", paths::base_path());
        log::info!(log::channel!("game"), "Assets path: {}", paths::assets_path());

        log::info!(log::channel!("game"), "Running in {build_profile} profile.");
        log::info!(log::channel!("game"), "{run_environment} environment.");
        log::info!(log::channel!("game"), "Redirect log to file: {is_app_bundle}.");

        log::info!(log::channel!("game"), "Loading Game Configs ...");
        let configs = GameConfigs::load();

        // Boot the engine and load assets:
        let mut engine = Self::init_engine(&configs.engine);
        Self::load_assets(engine.texture_cache_mut(), configs);

        Self::finish_start(engine, configs)
    }

    // WASM entry point: accepts a pre-created engine (async wgpu init happened externally).
    #[cfg(feature = "web")]
    pub fn start_with_engine(mut engine: Box<dyn Engine>, configs: &'static GameConfigs) {
        LogViewerWindow::initialize();
        platform::initialize_crash_report(true);

        log::info!(log::channel!("game"), "--- Game Initialization (WASM) ---");
        log::info!(log::channel!("game"), "Base path: {}", paths::base_path());
        log::info!(log::channel!("game"), "Assets path: {}", paths::assets_path());

        // Load assets using the pre-created engine.
        Self::load_assets(engine.texture_cache_mut(), configs);

        Self::finish_start(engine, configs);
    }

    pub fn shutdown() {
        {
            let game_loop = Self::get_mut();
            session::destroy(&mut game_loop.session, &mut *game_loop.engine);
        }
        Self::terminate();
    }

    pub fn reset_session(&mut self, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.session_cmd_queue.push_reset_session(reset_map_with_tile_def, new_map_size);
    }

    pub fn quit_to_main_menu(&mut self) {
        self.session_cmd_queue.push_quit_to_main_menu();
    }

    pub fn load_preset_map(&mut self, preset_number: usize) {
        self.session_cmd_queue.push_load_preset_map(preset_number);
    }

    pub fn load_save_game(&mut self, save_file_name: PathRef) {
        self.session_cmd_queue.push_load_save_game(save_file_name);
    }

    pub fn save_game(&mut self, save_file_name: PathRef) {
        self.session_cmd_queue.push_save_game(save_file_name);
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
        self.engine.is_running()
    }

    #[inline]
    pub fn is_in_home_menu(&self) -> bool {
        self.session.current_menus_mode() == Some(GameMenusMode::Home)
    }

    #[inline]
    pub fn is_in_game(&self) -> bool {
        !self.is_in_home_menu()
    }

    #[inline]
    pub fn engine(&self) -> &dyn Engine {
        &*self.engine
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut dyn Engine {
        &mut *self.engine
    }

    #[inline]
    pub fn camera(&self) -> &Camera {
        self.session.camera()
    }

    #[inline]
    pub fn camera_mut(&mut self) -> &mut Camera {
        self.session.camera_mut()
    }

    #[inline]
    pub fn systems(&self) -> &GameSystems {
        self.session.systems()
    }

    #[inline]
    pub fn systems_mut(&mut self) -> &mut GameSystems {
        self.session.systems_mut()
    }

    #[inline]
    pub fn sim(&self) -> &Simulation {
        self.session.sim()
    }

    #[inline]
    pub fn sim_mut(&mut self) -> &mut Simulation {
        self.session.sim_mut()
    }

    #[inline]
    pub fn stats(&self) -> &GameLoopStats {
        &self.stats
    }

    #[inline]
    pub fn quit_game(&mut self) {
        self.engine_mut().app_mut().request_quit();
    }

    pub fn update(&mut self) {
        let frame_timer = PerfTimer::begin();

        let (delta_time_secs, cursor_screen_pos, begin_frame_time_ms) = self.engine.begin_frame();
        self.stats.fps = if delta_time_secs > 0.0 { 1.0 / delta_time_secs } else { 0.0 };

        self.update_autosave();
        self.session_cmd_queue.execute(&mut self.session, &mut *self.engine);

        // Input Events:
        for event in self.engine.app_events().clone() {
            self.handle_app_event(event);
        }

        // Game Logic:
        let visible_range = self.update_simulation(cursor_screen_pos, delta_time_secs);

        // Rendering:
        let render_flags = self.menus_begin_frame();
        self.draw_tile_map(delta_time_secs, visible_range, render_flags);
        self.menus_end_frame(visible_range);

        // Sound System Update:
        self.update_sound_system();

        let (end_frame_time_ms, present_frame_time_ms) = self.engine.end_frame();

        self.stats.engine_begin_frame_time_ms = begin_frame_time_ms;
        self.stats.engine_end_frame_time_ms   = end_frame_time_ms;
        self.stats.present_frame_time_ms      = present_frame_time_ms;
        self.stats.total_frame_time_ms        = frame_timer.end();
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn finish_start(mut engine: Box<dyn Engine>, configs: &'static GameConfigs) -> &'static mut Self {
        // Global initialization:
        cheats::initialize();
        undo_redo::initialize();
        Simulation::register_callbacks();
        debug::set_show_popup_messages(configs.debug.show_popups);

        let session = Box::new(session::create(&mut *engine, None));

        let game_loop = Self {
            engine,
            session,
            session_cmd_queue: GameSessionCmdQueue::new(),
            autosave_timer: UpdateTimer::new(configs.save.autosave_frequency_secs),
            enable_autosave: configs.save.enable_autosave,
            stats: GameLoopStats::default(),
        };

        Self::initialize(game_loop); // Set global instance.
        Self::get_mut() // Return it.
    }

    #[cfg(feature = "desktop")]
    fn init_engine(configs: &EngineConfigs) -> Box<dyn Engine> {
        let init_engine_timer = PerfTimer::begin();

        log::info!(log::channel!("game"), "--- Init Engine: GLFW + OpenGL ---");
        let engine = Box::new(engine::backend::GlfwOpenGlEngine::new(configs));

        // EXPERIMENTAL / WIP:

        //log::info!(log::channel!("game"), "--- Init Engine: Winit + OpenGL ---");
        //let engine = Box::new(engine::backend::WinitOpenGlEngine::new(configs));

        //log::info!(log::channel!("game"), "--- Init Engine: Winit + Wgpu ---");
        //let engine = Box::new(engine::backend::WinitWgpuEngine::new(configs));

        let init_engine_time_ms = init_engine_timer.end();
        log::info!(log::channel!("game"), "--- Init Engine took: {:.1}ms ---", init_engine_time_ms);

        engine
    }

    fn load_assets(tex_cache: &mut dyn TextureCache, configs: &GameConfigs) {
        log::info!(log::channel!("game"), "--- Loading Game Assets ---");
        file_sys::paths::set_working_directory(file_sys::paths::base_path());

        BuildingConfigs::load();
        log::info!(log::channel!("game"), "BuildingConfigs loaded.");

        UnitConfigs::load();
        log::info!(log::channel!("game"), "UnitConfigs loaded.");

        PropConfigs::load();
        log::info!(log::channel!("game"), "PropConfigs loaded.");

        TileSets::load(tex_cache, configs.engine.use_packed_texture_atlas, configs.debug.skip_loading_tile_sets);
        log::info!(log::channel!("game"), "TileSets loaded.");
    }

    // ----------------------
    // Update & Rendering:
    // ----------------------

    fn update_simulation(&mut self, cursor_screen_pos: Vec2, delta_time_secs: Seconds) -> CellRange {
        if !self.is_in_game() {
            return CellRange::default(); // No simulation to update while at the home menus.
        }

        let visible_range = self.update_camera(cursor_screen_pos, delta_time_secs);

        let sim_update_timer = PerfTimer::begin();
        self.session.update_simulation(&mut *self.engine, delta_time_secs);
        self.stats.sim_frame_time_ms = sim_update_timer.end();

        let anim_update_timer = PerfTimer::begin();
        self.session.update_anims(visible_range, delta_time_secs);
        self.stats.anim_frame_time_ms = anim_update_timer.end();

        visible_range
    }

    fn update_camera(&mut self, cursor_screen_pos: Vec2, delta_time_secs: Seconds) -> CellRange {
        let viewport_size = self.engine.app().window_size();
        let is_any_ui_item_hovered = self.engine.ui_system().ui().is_any_item_hovered();

        let camera = self.session.camera_mut();
        camera.set_viewport_size(viewport_size);        
        camera.update_zooming(delta_time_secs);

        // Map scrolling, if cursor not hovering a menu item.
        if !is_any_ui_item_hovered {
            camera.update_scrolling(cursor_screen_pos, delta_time_secs);
        }

        camera.visible_cells_range()
    }

    fn update_sound_system(&mut self) {
        let sound_update_timer = PerfTimer::begin();
        let listener_position = self.camera().iso_world_position();
        self.engine.sound_system_mut().update(listener_position);
        self.stats.sound_frame_time_ms = sound_update_timer.end();
    }

    fn update_autosave(&mut self) {
        if !self.enable_autosave || !self.is_in_game() {
            return; // Don't autosave while in the home/main menu.
        }

        let delta_time_secs = self.engine.frame_clock().delta_time();

        if self.autosave_timer.tick(delta_time_secs).should_update() {
            self.save_game(save::storage::AUTOSAVE_FILE_NAME);
        }
    }

    fn draw_tile_map(&mut self,
                     delta_time_secs: Seconds,
                     visible_range: CellRange,
                     flags: TileMapRenderFlags) {
        if !self.is_in_game() {
            return; // We don't have a tile map to render while at the home menus.
        }

        let draw_world_timer = PerfTimer::begin();
        self.session.draw_tile_map(&mut *self.engine, delta_time_secs, visible_range, flags);
        self.stats.draw_world_frame_time_ms = draw_world_timer.end();
    }

    fn handle_app_event(&mut self, event: ApplicationEvent) {
        match event {
            ApplicationEvent::WindowResize { window_size, framebuffer_size } => {
                self.camera_mut().set_viewport_size(window_size);
                log::info!(log::channel!("game"), "Window Resized: {window_size}");
                log::info!(log::channel!("game"), "Framebuffer Resized: {framebuffer_size}");
            }
            ApplicationEvent::KeyInput(key, action, modifiers) => {
                let mut input_event = if self.is_in_game() {
                    self.camera_mut().on_key_input(key, action, modifiers)
                } else {
                    UiInputEvent::NotHandled
                };

                // [CTRL]+[/]: Toggle between DevEditor menu / HUD menu.
                if input_event.not_handled()
                    && action == InputAction::Press
                    && key == InputKey::Slash
                    && modifiers.intersects(InputModifiers::Control)
                {
                    self.session_cmd_queue.push_toggle_menus_mode();
                    input_event = UiInputEvent::Handled;
                }

                if input_event.not_handled() {
                    self.menus_on_key_input(key, action, modifiers);
                }
            }
            ApplicationEvent::Scroll(amount) => {
                // If we're not hovering over an ImGui menu...
                let input_event = if self.is_in_game() && !self.engine().ui_system().is_handling_mouse_input() {
                    self.camera_mut().on_mouse_scroll(amount)
                } else {
                    UiInputEvent::NotHandled
                };

                if input_event.not_handled() {
                    self.menus_on_scroll(amount);
                }
            }
            ApplicationEvent::MouseButton(button, action, modifiers) => {
                self.menus_on_mouse_button(button, action, modifiers);
            }
            _ => {}
        }
    }

    // ----------------------
    // In-Game UI / Debug UI:
    // ----------------------

    fn menus_begin_frame(&mut self) -> TileMapRenderFlags {
        let ui_begin_timer = PerfTimer::begin();
        let flags = self.session.menus_begin_frame(&mut *self.engine);
        self.stats.ui_begin_frame_time_ms = ui_begin_timer.end();
        flags
    }

    fn menus_end_frame(&mut self, visible_range: CellRange) {
        let ui_end_timer = PerfTimer::begin();
        self.session.menus_end_frame(&mut *self.engine, visible_range);
        self.stats.ui_end_frame_time_ms = ui_end_timer.end();
    }

    fn menus_on_key_input(&mut self,
                          key: InputKey,
                          action: InputAction,
                          modifiers: InputModifiers)
                          -> UiInputEvent {
        self.session.menus_on_key_input(&mut *self.engine, key, action, modifiers)
    }

    fn menus_on_mouse_button(&mut self,
                             button: MouseButton,
                             action: InputAction,
                             modifiers: InputModifiers)
                             -> UiInputEvent {
        self.session.menus_on_mouse_button(&mut *self.engine, button, action, modifiers)
    }

    fn menus_on_scroll(&mut self, amount: Vec2) -> UiInputEvent {
        self.session.menus_on_scroll(&mut *self.engine, amount)
    }
}

// ----------------------------------------------
// GameLoop Global Singleton
// ----------------------------------------------

singleton_late_init! { GAME_LOOP_SINGLETON, GameLoop }
