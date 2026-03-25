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
mod runner;
mod save;
mod sound;
mod tile;
mod camera;
mod utils;

fn main() {
    runner::run::<game::GameLoop>();
}
