use super::*;
use crate::log;

// ----------------------------------------------
// WebRunner
// ----------------------------------------------

pub struct WebRunner;

impl Runner for WebRunner {
    fn new() -> Self {
        Self
    }

    fn run<GameLoop: RunLoop + 'static>(&self) {
        log::info!(log::channel!("game"), "WASM entry point started.");

        // TODO: Hand control to the browser event loop.
        // The Web/WASM runner handles async GPU init, asset loading,
        // config loading, engine creation, and then starts the GameLoop.
    }
}
