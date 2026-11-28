// Game crate — game logic, entities, simulation, tile map, camera, path finding.

// NOTE: Allow for the whole crate.
#![allow(dead_code)]

// Core game modules:
pub mod automation;
pub mod building;
pub mod byte_runs;
pub mod camera;
pub mod campaign;
pub mod cheats;
pub mod city;
pub mod config;
pub mod constants;
pub mod custom_game;
pub mod debug;
pub mod defence;
pub mod faith;
pub mod farms;
pub mod floods;
pub mod inspect;
pub mod labour;
pub mod launch;
pub mod menu;
pub mod messages;
pub mod pathfind;
pub mod profile;
pub mod prop;
pub mod ratings;
pub mod realm;
pub mod save_context;
pub mod save_header;
pub mod session;
pub mod sim;
pub mod story;
pub mod system;
pub mod tile;
pub mod trade;
pub mod ui_context;
pub mod undo_redo;
pub mod unit;
pub mod world;
pub mod world_map;

// Re-export GameLoop and key types at the crate root.
mod game_loop;
pub use game_loop::{GameLoop, GameLoopStats};

// This must stay here — env!("CARGO_PKG_VERSION") resolves to the game crate's version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
