use super::*;
use crate::{
    log,
    platform,
    file_sys::paths,
    debug::log_viewer::LogViewer,
    engine::{Engine, backend::*},
    utils::{time::PerfTimer, mem::RcMut},
    app::{Application, ApplicationBuilder},
    render::{RenderSystem, RenderSystemInitParams},
};

// ----------------------------------------------
// DesktopRunner
// ----------------------------------------------

pub struct DesktopRunner;

impl Runner for DesktopRunner {
    fn new() -> Self { Self }

    fn run<Game: RunLoop + 'static>(&self) {
        let (engine, configs) = Self::start_engine();
        let game = Game::start(engine, configs);

        while game.is_running() {
            game.update();
        }

        Game::shutdown();
        Engine::shutdown();
    }
}

impl DesktopRunner {
    fn start_engine() -> (&'static mut Engine, &'static GameConfigs) {
        let start_engine_timer = PerfTimer::begin();

        // Early initialization:
        LogViewer::initialize();
        platform::initialize();
        paths::set_working_directory(paths::base_path());

        log::info!(log::channel!("engine"), "--- Engine Initialization ---");

        let configs = GameConfigs::load();
        log::info!(log::channel!("engine"), "GameConfigs Loaded.");

        log::set_level(configs.engine.log_level);

        // Initialize Application:
        let app: RcMut<ApplicationBackendImpl> = RcMut::new(
            ApplicationBuilder::new()
                .window_title(&configs.engine.window_title)
                .window_size(configs.engine.window_size)
                .window_mode(configs.engine.window_mode)
                .resizable_window(configs.engine.resizable_window)
                .confine_cursor_to_window(configs.engine.confine_cursor_to_window)
                .content_scale(configs.engine.content_scale)
                .build()
        );
        log::info!(log::channel!("engine"), "Application initialized.");

        // Initialize Render System:
        let render_system = RenderSystem::new(
            &RenderSystemInitParams {
                render_api: configs.engine.render_api,
                clear_color: configs.engine.window_background_color,
                texture_settings: configs.engine.texture_settings,
                viewport_size: app.window_size(),
                framebuffer_size: app.framebuffer_size(),
                app_context: app.app_context(),
                ..Default::default()
            }
        );
        log::info!(log::channel!("engine"), "RenderSystem initialized.");

        let engine = Engine::start(&configs.engine, app, render_system);

        let start_engine_time_ms = start_engine_timer.end();
        log::info!(log::channel!("engine"), "Engine running. Startup took: {:.1}ms", start_engine_time_ms);

        (engine, configs)
    }
}
