// Engine crate — platform, rendering, UI, sound, and app runner infrastructure.

// NOTE: Allow for the whole crate.
#![allow(dead_code)]

pub mod log;
pub mod app;
pub mod config;
pub mod file_sys;
pub mod ui;
pub mod platform;
pub mod render;
pub mod runner;
pub mod save;
pub mod sound;

// Re-export Engine and key types at the crate root.
mod engine;
pub use engine::{Engine, EngineSystemsMutRefs};
