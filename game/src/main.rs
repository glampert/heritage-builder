#![allow(dead_code)]

mod log;
mod app;
mod debug;
mod engine;
mod game;
mod imgui_ui;
mod pathfind;
mod render;
mod save;
mod sound;
mod tile;
mod utils;

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let game_loop = game::GameLoop::new();

    game_loop.create_session();

    while game_loop.is_running() {
        game_loop.update();
    }

    game_loop.terminate_session();
}
