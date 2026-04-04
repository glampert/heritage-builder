use super::*;
mod libc;

// ----------------------------------------------
// Crash Report System
// ----------------------------------------------

mod crash_report;

pub fn initialize_crash_report(set_panic_hook: bool) {
    crash_report::initialize(set_panic_hook);
}

// ----------------------------------------------
// Run Env / App Bundle Detection
// ----------------------------------------------

pub fn run_environment() -> RunEnvironment {
    RunEnvironment::WebBrowser
}

// ----------------------------------------------
// Main Thread Detection
// ----------------------------------------------

pub fn set_main_thread() {
    // No concept of "main thread" in Web/WASM - single threaded.
}

pub fn is_main_thread() -> bool {
    // No user threads in Web/WASM - always "main thread".
    true
}
