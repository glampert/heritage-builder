use crate::{config::EngineConfigs, engine::Engine};

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
type RunnerImpl = desktop::DesktopRunner;

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
type RunnerImpl = web::WebRunner;

// ----------------------------------------------
// RunLoopConfigs
// ----------------------------------------------

// Trait for config types that contain engine configuration.
// The runner only needs access to EngineConfigs for initialization;
// the concrete config type (e.g. GameConfigs) is an associated type on RunLoop.
pub trait RunLoopConfigs: Sized + 'static {
    fn engine(&self) -> &EngineConfigs;

    // Load configs from disk/storage. Returns a &'static reference
    // (configs are stored as a global singleton).
    fn load() -> &'static Self;
    fn get() -> &'static Self;
}

// ----------------------------------------------
// RunLoop
// ----------------------------------------------

// Base trait implemented by the GameLoop.
pub trait RunLoop: Sized {
    type Configs: RunLoopConfigs;

    fn start(engine: &'static mut Engine, configs: &'static Self::Configs) -> &'static mut Self;
    fn shutdown();
    fn get_mut() -> &'static mut Self;

    fn update(&mut self);
    fn is_running(&self) -> bool;

    // Called by the runner before engine initialization.
    // Override to perform early setup (e.g., log viewer, debug tools).
    fn on_early_init() {}
}

// ----------------------------------------------
// Runner
// ----------------------------------------------

// Game loop runner — platform-specific entry points.
//
// Each platform implements `Runner::run()`:
//  - Desktop: synchronous loop — create engine, load configs, pump frames, shut down.
//  - Web: hand control to the browser event loop (WebRunner), which drives
//         async init and then pumps frames via requestAnimationFrame.
trait Runner: Sized {
    fn new() -> Self;
    fn run<GameLoop: RunLoop + 'static>(&self);
}

// Top-level entry point called from main().
pub fn run<GameLoop: RunLoop + 'static>() {
    let runner = RunnerImpl::new();
    runner.run::<GameLoop>();
}
