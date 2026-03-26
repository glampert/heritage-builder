use crate::{
    engine::Engine,
    game::config::GameConfigs,
};

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
type RunnerImpl = desktop::DesktopRunner;

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
type RunnerImpl = web::WebRunner;

// ----------------------------------------------
// RunLoop
// ----------------------------------------------

// Base trait implemented by the GameLoop.
pub trait RunLoop: Sized {
    fn start(engine: &'static mut Engine, configs: &'static GameConfigs) -> &'static mut impl RunLoop;
    fn shutdown();

    fn update(&mut self);
    fn is_running(&self) -> bool;
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
