#![allow(dead_code)]

mod app;
mod debug;
mod engine;
mod game;
mod imgui_ui;
mod log;
mod pathfind;
mod render;
mod save;
mod tile;
mod utils;

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    use game::core::GameLoop;

    let mut game_loop = GameLoop::new();

    game_loop.create_session();

    while game_loop.is_running() {
        game_loop.update();
    }

    game_loop.terminate_session();
}
