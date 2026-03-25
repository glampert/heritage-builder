use super::*;
use crate::{
    log,
    platform,
    file_sys::paths,
    render::RenderSystemBuilder,
    engine::{Engine, backend::*},
    app::{Application, ApplicationBuilder},
    debug::log_viewer::LogViewer,
    utils::{time::PerfTimer, mem::RcMut},
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
        platform::set_main_thread();
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
        let render_system: RcMut<RenderSystemBackendImpl> = RcMut::new({
            let mut builder = RenderSystemBuilder::new();
            builder
                .viewport_size(app.window_size())
                .framebuffer_size(app.framebuffer_size())
                .clear_color(configs.engine.window_background_color)
                .texture_settings(configs.engine.texture_settings);
            if let Some(ctx) = app.app_context() {
                builder.app_context(ctx);
            }
            builder.build()
        });
        log::info!(log::channel!("engine"), "RenderSystem initialized.");

        let engine = Engine::start(&configs.engine, app, render_system);

        let start_engine_time_ms = start_engine_timer.end();
        log::info!(log::channel!("engine"), "Engine running. Startup took: {:.1}ms", start_engine_time_ms);

        (engine, configs)
    }
}
