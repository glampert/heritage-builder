// Engine crate — platform, rendering, UI, sound, and app runner infrastructure.

// NOTE: Allow for the whole crate.
#![allow(dead_code)]

// Allow referring to this crate as `engine` from within itself, so that
// proc-macro-generated code (e.g. `#[derive(DrawDebugUi)]`) can use the same
// fully-qualified `::engine::...` paths in both this crate and the game crate.
extern crate self as engine;

pub mod app;
pub mod config;
pub mod debug;
pub mod file_sys;
pub mod log;
pub mod platform;
pub mod render;
pub mod runner;
pub mod save;
pub mod sound;
pub mod ui;

// Re-export Engine and key types at the crate root.
// NOTE: The module is `engine_impl` (file `engine_impl.rs`) rather than `engine`
// to avoid clashing with the `extern crate self as engine;` self-alias above.
mod engine_impl;
pub use engine_impl::{Engine, EngineSystemsMutRefs};
