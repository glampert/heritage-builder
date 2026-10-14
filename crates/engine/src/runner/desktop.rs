use common::time::PerfTimer;

use super::*;
use crate::{
    app::{Application, ApplicationApi, ApplicationInitParams},
    cli,
    engine::Engine,
    file_sys::paths,
    log,
    platform,
    render::{RenderApi, RenderSystem, RenderSystemInitParams},
};

// ----------------------------------------------
// DesktopRunner
// ----------------------------------------------

pub struct DesktopRunner;

impl Runner for DesktopRunner {
    fn new() -> Self {
        Self
    }

    fn run<GameLoop: RunLoop + 'static>(&self) {
        let (engine, configs) = Self::start::<GameLoop>();
        let game = GameLoop::start(engine, configs);

        while game.is_running() {
            game.update();
        }

        Engine::get_mut().finish_run();
        let exit_code = Engine::get().exit_code();

        GameLoop::shutdown();
        Engine::shutdown();

        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    }
}

impl DesktopRunner {
    fn start<GameLoop: RunLoop + 'static>() -> (&'static mut Engine, &'static GameLoop::Configs) {
        let start_engine_timer = PerfTimer::begin();

        // Early initialization:
        GameLoop::on_early_init();
        platform::initialize();

        // After platform::initialize(), which decides where log output goes.
        cli::initialize(GameLoop::cli_options());

        paths::set_working_directory(paths::base_path());

        log::info!(log::channel!("engine"), "--- Engine Initialization ---");

        let configs = GameLoop::Configs::load();
        let engine_configs = configs.engine();
        log::info!(log::channel!("engine"), "Configs Loaded.");

        log::set_level(engine_configs.log_level);

        let app_api = engine_configs.app_api;
        let mut render_api = engine_configs.render_api;

        if app_api == ApplicationApi::Glfw && render_api != RenderApi::OpenGl {
            log::warning!(log::channel!("engine"), "Glfw is only compatible OpenGl. Setting render backend to OpenGl.");
            render_api = RenderApi::OpenGl;
        }

        // Initialize Application:
        let app = Application::new(ApplicationInitParams {
            app_api,
            render_api,
            window_title: &engine_configs.window_title,
            window_size: cli::window_size().unwrap_or(engine_configs.window_size),
            window_mode: engine_configs.window_mode,
            content_scale: engine_configs.content_scale,
            resizable_window: engine_configs.resizable_window,
            confine_cursor: engine_configs.confine_cursor_to_window,
        });
        log::info!(log::channel!("engine"), "Application initialized.");

        // Initialize Render System:
        let render_system = RenderSystem::new(RenderSystemInitParams {
            render_api,
            clear_color: engine_configs.window_background_color,
            texture_settings: engine_configs.texture_settings,
            viewport_size: app.window_size(),
            framebuffer_size: app.framebuffer_size(),
            app_context: app.app_context(),
            ..Default::default()
        });
        log::info!(log::channel!("engine"), "RenderSystem initialized.");

        let engine = Engine::start(engine_configs, app, render_system);

        let start_engine_time_ms = start_engine_timer.end();
        log::info!(log::channel!("engine"), "Engine running. Startup took: {:.1}ms", start_engine_time_ms);

        (engine, configs)
    }
}
