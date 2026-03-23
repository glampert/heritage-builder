#![allow(unused_imports)]

use strum::Display;

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

pub fn build_profile() -> BuildProfile {
    if cfg!(debug_assertions) {
        BuildProfile::Debug
    } else {
        BuildProfile::Release
    }
}
