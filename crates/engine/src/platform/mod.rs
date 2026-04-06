use strum::Display;
use crate::{log, file_sys::paths};

// ----------------------------------------------
// Platform backend implementations
// ----------------------------------------------

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::{run_environment, set_main_thread, is_main_thread, initialize_crash_report};

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
pub use web::{run_environment, set_main_thread, is_main_thread, initialize_crash_report};

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

    // Only log panics when running from a bundle or web browser.
    // Otherwise the default behavior is fine.
    let set_panic_hook = run_environment.is_app_bundle() || run_environment.is_web_browser();

    // Early initialization:
    log::redirect_to_file(log_to_file);
    initialize_crash_report(set_panic_hook);

    log::info!(log::channel!("engine"), "--- Platform Initialization ---");
    log::info!(log::channel!("engine"), "Running in {build_profile} profile.");
    log::info!(log::channel!("engine"), "{run_environment} environment.");
    log::info!(log::channel!("engine"), "Redirect log to file: {log_to_file}.");
    log::info!(log::channel!("engine"), "Set panic hook: {set_panic_hook}.");
    log::info!(log::channel!("engine"), "Base path: {}", paths::base_path());
    log::info!(log::channel!("engine"), "Assets path: {}", paths::assets_path());
}
