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
// main()
// ----------------------------------------------

fn main() {
    platform::set_main_thread();

    let game_loop = GameLoop::start();

    while game_loop.is_running() {
        game_loop.update();
    }

    GameLoop::shutdown();
}
