// Game crate — game logic, entities, simulation, tile map, camera, path finding.

// NOTE: Allow for the whole crate.
#![allow(dead_code)]

// Core game modules:
pub mod config;
pub mod constants;
pub mod cheats;
pub mod undo_redo;
pub mod ui_context;
pub mod save_context;
pub mod session;
pub mod building;
pub mod menu;
pub mod prop;
pub mod sim;
pub mod system;
pub mod unit;
pub mod world;

// Game systems:
pub mod tile;
pub mod camera;
pub mod pathfind;
pub mod debug;

// Re-export GameLoop and key types at the crate root.
mod game_loop;
pub use game_loop::{GameLoop, GameLoopStats};

// This must stay here — env!("CARGO_PKG_VERSION") resolves to the game crate's version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
