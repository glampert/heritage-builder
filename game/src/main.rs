// NOTE: Allow these for the whole project.
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]

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
    use game::GameLoop;

    let game_loop = GameLoop::new();

    game_loop.create_session();

    while game_loop.is_running() {
        game_loop.update();
    }

    game_loop.terminate_session();

    GameLoop::terminate();
}
