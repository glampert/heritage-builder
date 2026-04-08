use common::{
    Size,
    Vec2,
    coords::CellRange,
    time::{Milliseconds, PerfTimer, Seconds, UpdateTimer},
};
use engine::{
    log,
    save,
    Engine,
    ui::UiInputEvent,
    runner::RunLoop,
    file_sys::paths::PathRef,
    app::{
        ApplicationEvent,
        input::{InputAction, InputKey, InputModifiers, MouseButton},
    },
};

use crate::{
    cheats,
    debug,
    undo_redo,
    menu::GameMenusMode,
    config::GameConfigs,
    unit::config::UnitConfigs,
    building::config::BuildingConfigs,
    prop::config::PropConfigs,
    session::{self, GameSession, GameSessionCmdQueue},
    sim::Simulation,
    system::GameSystems,
    tile::{
        rendering::{TileMapRenderFlags, TileMapRenderStats},
        sets::{TileDef, TileSets},
    },
};

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
    engine: &'static mut Engine,
    configs: &'static GameConfigs,

    session: Box<GameSession>,
    session_cmd_queue: GameSessionCmdQueue,

    autosave_timer: UpdateTimer,
    enable_autosave: bool,

    stats: GameLoopStats,
}

impl RunLoop for GameLoop {
    type Configs = GameConfigs;

    fn on_early_init() {
        debug::log_viewer::LogViewer::initialize();
    }

    fn start(engine: &'static mut Engine, configs: &'static Self::Configs) -> &'static mut Self {
        log::info!(log::channel!("game"), "--- GameLoop Initialization ---");

        // Load configs / tile sets:
        Self::load_assets(engine, configs);

        // Global initialization:
        cheats::initialize();
        undo_redo::initialize();
        Simulation::register_callbacks();
        debug::set_show_popup_messages(configs.debug.show_popups);

        // Create Session and GameLoop:
        let session = session::create(engine, configs, None);
        let game_loop = Self {
            engine,
            configs,
            session: Box::new(session),
            session_cmd_queue: GameSessionCmdQueue::new(),
            autosave_timer: UpdateTimer::new(configs.save.autosave_frequency_secs),
            enable_autosave: configs.save.enable_autosave,
            stats: GameLoopStats::default(),
        };

        // Set global instance:
        Self::initialize(game_loop);
        Self::get_mut()
    }

    fn shutdown() {
        // Terminate game session:
        {
            let this = Self::get_mut();
            session::destroy(&mut this.session, this.engine, this.configs);
        }

        // Terminate singleton instances.
        Self::terminate();
        Self::unload_assets();
    }

    // RunLoop::get_mut impl.
    fn get_mut() -> &'static mut Self {
        Self::get_mut() // Delegates to singleton_late_init! generated method.
    }

    fn update(&mut self) {
        let frame_timer = PerfTimer::begin();

        let (delta_time_secs, cursor_screen_pos, begin_frame_time_ms) = self.engine.begin_frame();
        self.stats.fps = if delta_time_secs > 0.0 { 1.0 / delta_time_secs } else { 0.0 };

        self.update_autosave();
        self.session_cmd_queue.execute(&mut self.session, self.engine, self.configs);

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

    #[inline]
    fn is_running(&self) -> bool {
        self.engine.is_running()
    }
}

impl GameLoop {
    // ----------------------
    // Public GameLoop API:
    // ----------------------

    #[inline]
    pub fn quit_game(&mut self) {
        self.engine.app_mut().request_quit();
    }

    #[inline]
    pub fn quit_to_main_menu(&mut self) {
        self.session_cmd_queue.push_quit_to_main_menu();
    }

    #[inline]
    pub fn reset_session(&mut self, reset_map_with_tile_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.session_cmd_queue.push_reset_session(reset_map_with_tile_def, new_map_size);
    }

    #[inline]
    pub fn load_preset_map(&mut self, preset_number: usize) {
        self.session_cmd_queue.push_load_preset_map(preset_number);
    }

    #[inline]
    pub fn load_save_game(&mut self, save_file_name: PathRef) {
        self.session_cmd_queue.push_load_save_game(save_file_name);
    }

    #[inline]
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
    pub fn is_in_home_menu(&self) -> bool {
        self.session.current_menus_mode() == Some(GameMenusMode::Home)
    }

    #[inline]
    pub fn is_in_game(&self) -> bool {
        !self.is_in_home_menu()
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
    pub fn tile_map_render_stats(&self) -> &TileMapRenderStats {
        self.session.tile_map_render_stats()
    }

    #[inline]
    pub fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.session.set_grid_line_thickness(thickness);
    }

    #[inline]
    pub fn grid_line_thickness(&self) -> f32 {
        self.session.grid_line_thickness()
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn load_assets(engine: &mut Engine, configs: &GameConfigs) {
        let load_assets_timer = PerfTimer::begin();

        log::info!(log::channel!("game"), "Loading Game Assets ...");

        BuildingConfigs::load();
        log::info!(log::channel!("game"), "BuildingConfigs loaded.");

        UnitConfigs::load();
        log::info!(log::channel!("game"), "UnitConfigs loaded.");

        PropConfigs::load();
        log::info!(log::channel!("game"), "PropConfigs loaded.");

        let tex_cache = engine.texture_cache_mut();
        TileSets::load(tex_cache, configs.engine.use_packed_texture_atlas, configs.debug.skip_loading_tile_sets, false);
        log::info!(log::channel!("game"), "TileSets loaded.");

        let load_assets_time_ms = load_assets_timer.end();
        log::info!(log::channel!("game"), "Load Assets took: {:.1}ms", load_assets_time_ms);
    }

    fn unload_assets() {
        TileSets::terminate();
        PropConfigs::terminate();
        UnitConfigs::terminate();
        BuildingConfigs::terminate();
        GameConfigs::terminate();
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
        self.session.update_simulation(self.engine, delta_time_secs);
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
        let listener_position = self.session.camera().iso_world_position();
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

    fn draw_tile_map(&mut self, delta_time_secs: Seconds, visible_range: CellRange, flags: TileMapRenderFlags) {
        if !self.is_in_game() {
            return; // We don't have a tile map to render while at the home menus.
        }

        let draw_world_timer = PerfTimer::begin();
        self.session.draw_tile_map(self.engine, delta_time_secs, visible_range, flags);
        self.stats.draw_world_frame_time_ms = draw_world_timer.end();
    }

    fn handle_app_event(&mut self, event: ApplicationEvent) {
        match event {
            ApplicationEvent::WindowResize { window_size, framebuffer_size } => {
                self.session.camera_mut().set_viewport_size(window_size);
                log::info!(log::channel!("game"), "Resized Window: {window_size}, Framebuffer: {framebuffer_size}");
            }
            ApplicationEvent::KeyInput(key, action, modifiers) => {
                let mut input_event = if self.is_in_game() {
                    self.session.camera_mut().on_key_input(key, action, modifiers)
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
                let input_event = if self.is_in_game() && !self.engine.ui_system().is_handling_mouse_input() {
                    self.session.camera_mut().on_mouse_scroll(amount)
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
        let flags = self.session.menus_begin_frame(self.engine);
        self.stats.ui_begin_frame_time_ms = ui_begin_timer.end();
        flags
    }

    fn menus_end_frame(&mut self, visible_range: CellRange) {
        let ui_end_timer = PerfTimer::begin();
        self.session.menus_end_frame(self.engine, visible_range);
        self.stats.ui_end_frame_time_ms = ui_end_timer.end();
    }

    fn menus_on_scroll(&mut self, amount: Vec2) -> UiInputEvent {
        self.session.menus_on_scroll(self.engine, amount)
    }

    fn menus_on_key_input(&mut self, key: InputKey, action: InputAction, modifiers: InputModifiers) -> UiInputEvent {
        self.session.menus_on_key_input(self.engine, key, action, modifiers)
    }

    fn menus_on_mouse_button(
        &mut self,
        button: MouseButton,
        action: InputAction,
        modifiers: InputModifiers,
    ) -> UiInputEvent {
        self.session.menus_on_mouse_button(self.engine, button, action, modifiers)
    }
}

// ----------------------------------------------
// GameLoop Global Singleton
// ----------------------------------------------

common::singleton_late_init! { GAME_LOOP_SINGLETON, GameLoop }
