use super::*;

// ----------------------------------------------
// WebRunner
// ----------------------------------------------

pub struct WebRunner;

impl Runner for WebRunner {
    fn new() -> Self {
        Self
    }

    fn run<T: RunLoop + 'static>(&self) {
        log::info!(log::channel!("game"), "WASM entry point started.");

        // Hand control to the browser event loop.
        // The WASM runner handles async GPU init, asset loading, config loading,
        // engine creation, and then starts the GameLoop.
        crate::app::winit::wgpu::wasm_runner::run_wasm_event_loop();
    }
}
