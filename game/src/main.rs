// NOTE: Allow these for the whole project.
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]

mod log;
mod app;
mod debug;
mod engine;
mod file_sys;
mod game;
mod ui;
mod pathfind;
mod platform;
mod render;
mod save;
mod sound;
mod tile;
mod utils;

// ----------------------------------------------
// Desktop main()
// ----------------------------------------------

#[cfg(feature = "desktop")]
fn main() {
    platform::set_main_thread();

    let game_loop = game::GameLoop::start();

    while game_loop.is_running() {
        game_loop.update();
    }

    game::GameLoop::shutdown();
}

// ----------------------------------------------
// Web/WASM entry point
// ----------------------------------------------

#[cfg(feature = "web")]
fn main() {
    platform::set_main_thread();

    // Early init: paths, logging.
    file_sys::paths::set_working_directory(file_sys::paths::base_path());
    log::info!(log::channel!("game"), "WASM entry point started.");

    // Hand control to the browser event loop.
    // The WASM runner will handle async asset loading, config loading,
    // wgpu init, then start the GameLoop.
    app::winit::wgpu::wasm_runner::run_wasm_event_loop();
}
