use strum::Display;

use crate::{file_sys::paths, log};

// ----------------------------------------------
// Platform backend implementations
// ----------------------------------------------

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::{
    initialize_crash_report,
    set_crash_report_frame,
    add_panic_listener,
    PanicReport,
    set_main_thread,
    is_main_thread,
    run_environment,
    local_date_time,
    DebugBacktrace,
};

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
pub use web::{
    initialize_crash_report,
    set_crash_report_frame,
    set_main_thread,
    is_main_thread,
    run_environment,
    local_date_time,
    DebugBacktrace,
};

// ----------------------------------------------
// Build Profile / App Bundle Detection
// ----------------------------------------------

#[derive(Copy, Clone, Display, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Copy, Clone, Display, PartialEq, Eq)]
pub enum RunEnvironment {
    Standalone,
    MacOSAppBundle,
    WebBrowser,
}

impl RunEnvironment {
    pub fn is_app_bundle(self) -> bool {
        self == Self::MacOSAppBundle
    }

    pub fn is_web_browser(self) -> bool {
        self == Self::WebBrowser
    }
}

// ----------------------------------------------
// Local Date / Time
// ----------------------------------------------

// Wall-clock date and time in the user's time zone, from local_date_time().
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u32, // 1-12
    pub day: u32,   // 1-31
    pub hour: u32,  // 0-23
    pub minute: u32,
    pub second: u32,
}

pub fn build_profile() -> BuildProfile {
    if cfg!(debug_assertions) {
        BuildProfile::Debug
    } else {
        BuildProfile::Release
    }
}

// ----------------------------------------------
// Platform Initialization
// ----------------------------------------------

pub fn initialize() {
    set_main_thread();

    let build_profile = build_profile();
    let run_environment = run_environment();

    // Redirect log to file on bundled runs.
    let log_to_file = run_environment.is_app_bundle();

    // Log panics when running from a bundle or web browser, where nobody watches a terminal.
    // Otherwise the default output is fine. Desktop runs also write a crash file either way.
    let log_panics = run_environment.is_app_bundle() || run_environment.is_web_browser();

    // Early initialization:
    log::redirect_to_file(log_to_file);
    initialize_crash_report(log_panics);

    log::info!(log::channel!("engine"), "--- Platform Initialization ---");
    log::info!(log::channel!("engine"), "Running in {build_profile} profile.");
    log::info!(log::channel!("engine"), "{run_environment} environment.");
    log::info!(log::channel!("engine"), "Redirect log to file: {log_to_file}.");
    log::info!(log::channel!("engine"), "Log panics: {log_panics}.");
    log::info!(log::channel!("engine"), "Base path: {}", paths::base_path());
    log::info!(log::channel!("engine"), "Assets path: {}", paths::assets_path());
}
