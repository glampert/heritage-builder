// NOTE: Allow these for the whole project.
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]

mod log;
mod app;
mod debug;
mod engine;
mod game;
mod ui;
mod pathfind;
mod render;
mod save;
mod sound;
mod tile;
mod utils;

use game::GameLoop;
use utils::platform;

// ----------------------------------------------
// Desktop main()
// ----------------------------------------------

#[cfg(feature = "desktop")]
fn main() {
    platform::set_main_thread();

    let game_loop = GameLoop::start();

    while game_loop.is_running() {
        game_loop.update();
    }

    GameLoop::shutdown();
}

// ----------------------------------------------
// WASM entry point
// ----------------------------------------------

#[cfg(feature = "web")]
fn main() {
    platform::set_main_thread();

    // Early init: paths, logging, configs.
    utils::paths::set_default_working_directory();
    log::info!(log::channel!("game"), "WASM entry point started.");

    log::info!(log::channel!("game"), "Loading Game Configs ...");
    let configs = game::config::GameConfigs::load();

    // Hand control to the browser event loop.
    // The WASM runner will handle async wgpu init, then start the GameLoop.
    app::winit::wgpu::wasm_runner::run_wasm_event_loop(configs);
}
