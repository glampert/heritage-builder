#![allow(dead_code)]

mod app;
mod debug;
mod game;
mod imgui_ui;
mod log;
mod pathfind;
mod render;
mod tile;
mod utils;
mod save;

use imgui_ui::*;
use render::*;
use utils::*;
use app::*;
use app::input::*;
use debug::*;
use save::*;
use tile::{
    camera::{self, *},
    rendering::{self, *},
    selection::*,
    sets::*
};
use game::{
    sim::{self, *},
    world::*,
    system::*,
    cheats,
    building::config::BuildingConfigs,
    unit::config::UnitConfigs
};

// ----------------------------------------------
// WIP
// ----------------------------------------------

use tile::TileMap;
use serde::{
    Serialize,
    Deserialize
};

struct GameLoop<'tile_sets, 'config> {
    engine: GameEngine,
    assets: GameAssets,
    session: GameSession<'tile_sets, 'config>,
}

struct GameEngine {
    // TODO app, ui, render, etc
}

struct GameAssets {
    tile_sets: TileSets,
    unit_configs: UnitConfigs,
    building_configs: BuildingConfigs,
}

impl GameAssets {
    fn new(render_sys: &mut impl RenderSystem) -> Self {
        Self {
            tile_sets: TileSets::load(render_sys.texture_cache_mut()),
            unit_configs: UnitConfigs::load(),
            building_configs: BuildingConfigs::load(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct GameSession<'tile_sets, 'config> {
    tile_map: Box<TileMap<'tile_sets>>,
    world: World<'config>,
    sim: Simulation<'config>,
    systems: GameSystems,
    camera: Camera,

    // NOTE: These are not actually serialized. We only need to invoke post_load() on them.
    #[serde(skip)] tile_selection: TileSelection,
    #[serde(skip)] debug_menus: DebugMenusSystem,
}

impl<'tile_sets, 'config> GameSession<'tile_sets, 'config> {
    fn new<'assets>(render_sys: &mut impl RenderSystem, assets: &'assets GameAssets) -> Self
        where 'assets: 'tile_sets,
              'assets: 'config
    {
        let mut world = World::new(&assets.building_configs, &assets.unit_configs);

        // Test map with debug preset tiles:
        let mut tile_map = Box::new(debug::utils::create_test_tile_map_preset(
            &mut world,
            &assets.tile_sets,
            0));

        // 64x64 empty map (dirt tiles):
        /*
        let mut tile_map = Box::new(TileMap::with_terrain_tile(
            Size::new(64, 64),
            &assets.tile_sets,
            TERRAIN_GROUND_CATEGORY,
            hash::StrHashPair::from_str("dirt")
        ));
        */

        let sim = Simulation::new(
            &tile_map,
            &assets.building_configs,
            &assets.unit_configs);

        let mut systems = GameSystems::new();
        systems.register(settlers::SettlersSpawnSystem::new());

        let camera = Camera::new(
            render_sys.viewport().size(),
            tile_map.size_in_cells(),
            CameraZoom::MIN,
            CameraOffset::Center);

        let tile_selection = TileSelection::new();

        let debug_menus = DebugMenusSystem::new(
            &mut tile_map,
            render_sys.texture_cache_mut());

        Self {
            tile_map,
            world,
            sim,
            systems,
            camera,
            tile_selection,
            debug_menus,
        }
    }
}

// ----------------------------------------------
// Save/Load for GameSession
// ----------------------------------------------

impl Save for GameSession<'_, '_> {
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

impl<'tile_sets, 'config> Load<'tile_sets, 'config> for GameSession<'tile_sets, 'config> {
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

    fn post_load(&mut self, context: &PostLoadContext<'tile_sets, 'config>) {
        self.tile_map.post_load(context);
        self.world.post_load(context);
        self.sim.post_load(context);
        self.systems.post_load(context);
        self.camera.post_load(context);
        self.tile_selection.post_load(context);
        self.debug_menus.post_load(context);
    }
}

impl<'tile_sets, 'config> GameSession<'tile_sets, 'config> {
    fn save_game(&mut self) -> bool {
        let save_file_path = save_file_path();
        let mut state = save::backends::new_json_save_state(true);

        if !can_write_save_file(save_file_path) {
            log::error!(log::channel!("save_game"), "Saved game file path '{save_file_path}' is not accessible!");
            return false;
        }

        self.pre_save();

        if let Err(err) = self.save(&mut state) {
            log::error!(log::channel!("save_game"), "Failed to saved game: {err}");
            return false;
        }

        if let Err(err) = state.write_file(save_file_path) {
            log::error!(log::channel!("save_game"), "Failed to write saved game file '{save_file_path}': {err}");
            return false;
        }

        self.post_save();

        true
    }

    fn load_game<'assets>(&mut self, assets: &'assets GameAssets) -> bool
        where 'assets: 'tile_sets,
              'assets: 'config
    {
        let save_file_path = save_file_path();
        let mut state = save::backends::new_json_save_state(false);

        if let Err(err) = state.read_file(save_file_path) {
            log::error!(log::channel!("save_game"), "Failed to read saved game file '{save_file_path:?}': {err}");
            return false;
        }

        // Load into a temporary instance so that if we fail we'll avoid modifying any state.
        let session: GameSession = match state.load_new_instance() {
            Ok(session) => session,
            Err(err) => {
                log::error!(log::channel!("save_game"), "Failed to load saved game from '{save_file_path:?}': {err}");
                return false;  
            }
        };

        self.pre_load();

        *self = session;

        self.post_load(&PostLoadContext::new(
            &self.tile_map,
            &assets.tile_sets,
            &assets.unit_configs,
            &assets.building_configs
        ));

        true
    }
}

fn save_file_path() -> &'static str {
    "save_game.json"
}

fn can_write_save_file(save_file_path: &str) -> bool {
    // Attempt to write a dummy file to probe if the path is writable. 
    std::fs::write(save_file_path, save_file_path).is_ok()
}

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let log_viewer = log_viewer::LogViewerWindow::new(false, 32);

    let cwd = std::env::current_dir().unwrap();
    log::info!("The current directory is \"{}\".", cwd.display());

    let mut app = ApplicationBuilder::new()
        .window_title("CitySim")
        .window_size(Size::new(1024, 768))
        .fullscreen(false)
        .confine_cursor_to_window(Camera::confine_cursor_to_window())
        .build();

    let input_sys = app.create_input_system();

    let mut render_sys = RenderSystemBuilder::new()
        .viewport_size(app.window_size())
        .clear_color(rendering::MAP_BACKGROUND_COLOR)
        .build();

    let mut ui_sys = UiSystem::new(&app);

    cheats::initialize();
    debug::set_show_popup_messages(true);

    // TODO Box these! Too large to be on the stack.
    let assets = GameAssets::new(&mut render_sys);
    let mut session = GameSession::new(&mut render_sys, &assets);

    let mut tile_map_renderer = TileMapRenderer::new(
        rendering::DEFAULT_GRID_COLOR,
        1.0);

    let mut render_sys_stats = RenderStats::default();
    let mut frame_clock = FrameClock::new();

    let mut test_save_game_timer = UpdateTimer::new(10.0);
    session.save_game();
    session.load_game(&assets);

    while !app.should_quit() {
        frame_clock.begin_frame();

        let cursor_screen_pos = input_sys.cursor_pos();
        let delta_time_secs = frame_clock.delta_time();

        if test_save_game_timer.tick(delta_time_secs).should_update() {
            session.save_game();
            session.load_game(&assets);
            log::info!("Game Saved/Reloaded.");
        }

        for event in app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_sys.set_viewport_size(window_size);
                    session.camera.set_viewport_size(window_size);
                }
                ApplicationEvent::KeyInput(key, action, modifiers) => {
                    if ui_sys.on_key_input(key, action, modifiers).is_handled() {
                        continue;
                    }

                    if session.debug_menus.on_key_input(&mut DebugMenusOnInputArgs {
                                                            tile_map: &mut session.tile_map,
                                                            tile_selection: &mut session.tile_selection,
                                                            world: &mut session.world,
                                                            transform: *session.camera.transform(),
                                                            cursor_screen_pos,
                                                        },
                                                        key, action).is_handled() {
                        continue;
                    }
                }
                ApplicationEvent::CharInput(c) => {
                    if ui_sys.on_char_input(c).is_handled() {
                        continue;
                    }
                }
                ApplicationEvent::Scroll(amount) => {
                    if ui_sys.on_scroll(amount).is_handled() {
                        continue;
                    }

                    if amount.y < 0.0 {
                        session.camera.request_zoom(camera::CameraZoom::In);
                    } else if amount.y > 0.0 {
                        session.camera.request_zoom(camera::CameraZoom::Out);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if ui_sys.on_mouse_click(button, action, modifiers).is_handled() {
                        continue;
                    }

                    if session.debug_menus.on_mouse_click(&mut DebugMenusOnInputArgs {
                                                              tile_map: &mut session.tile_map,
                                                              tile_selection: &mut session.tile_selection,
                                                              world: &mut session.world,
                                                              transform: *session.camera.transform(),
                                                              cursor_screen_pos,
                                                          },
                                                          button, action, modifiers).is_handled() {
                        continue;
                    }
                }
            }
        }

        session.sim.update(&mut session.world, &mut session.systems, &mut session.tile_map, &assets.tile_sets, delta_time_secs);

        session.camera.update_zooming(delta_time_secs);

        // Map scrolling:
        if !ui_sys.is_handling_mouse_input() {
            session.camera.update_scrolling(cursor_screen_pos, delta_time_secs);
        }

        let visible_range = session.camera.visible_cells_range();

        session.tile_map.update_anims(visible_range, delta_time_secs);

        ui_sys.begin_frame(&app, &input_sys, delta_time_secs);
        render_sys.begin_frame();

        let selected_render_flags =
            session.debug_menus.begin_frame(&mut DebugMenusBeginFrameArgs {
                ui_sys: &ui_sys,
                sim: &mut session.sim,
                world: &mut session.world,
                tile_map: &mut session.tile_map,
                tile_selection: &mut session.tile_selection,
                tile_sets: &assets.tile_sets,
                transform: *session.camera.transform(),
                cursor_screen_pos,
                delta_time_secs,
            });

        let tile_render_stats =
            tile_map_renderer.draw_map(
                &mut render_sys,
                &ui_sys,
                &session.tile_map,
                session.camera.transform(),
                visible_range,
                selected_render_flags);

        session.tile_selection.draw(&mut render_sys);

        session.debug_menus.end_frame(&mut DebugMenusEndFrameArgs {
            context: sim::debug::DebugContext {
                ui_sys: &ui_sys,
                world: &mut session.world,
                systems: &mut session.systems,
                tile_map: &mut session.tile_map,
                tile_sets: &assets.tile_sets,
                transform: *session.camera.transform(),
                delta_time_secs,
            },
            sim: &mut session.sim,
            log_viewer: &log_viewer,
            camera: &mut session.camera,
            render_sys: &mut render_sys,
            render_sys_stats: &render_sys_stats,
            tile_map_renderer: &mut tile_map_renderer,
            tile_render_stats: &tile_render_stats,
            tile_selection: &session.tile_selection,
            visible_range,
            cursor_screen_pos,
        });

        render_sys_stats = render_sys.end_frame();
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }

    session.sim.reset(&mut session.world, &mut session.systems, &mut session.tile_map, &assets.tile_sets);
}
