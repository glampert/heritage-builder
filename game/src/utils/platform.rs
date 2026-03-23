use strum::Display;

#[cfg(feature = "desktop")]
use std::{sync::OnceLock, thread::ThreadId};

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

pub fn run_environment() -> RunEnvironment {
    #[cfg(feature = "web")]
    {
        return RunEnvironment::WebBrowser;
    }

    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        // Example: /Applications/MyGame.app/Contents/MacOS/MyGame
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                if exe_dir
                    .parent()
                    .and_then(|contents| contents.parent())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
                    .is_some()
                {
                    return RunEnvironment::MacOSAppBundle;
                }
            }
        }
    }

    #[cfg(feature = "desktop")]
    RunEnvironment::Standalone
}

// ----------------------------------------------
// Main Thread Detection
// ----------------------------------------------

#[cfg(feature = "desktop")]
static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();

pub fn set_main_thread() {
    #[cfg(feature = "desktop")]
    MAIN_THREAD_ID.set(std::thread::current().id())
        .expect("MAIN_THREAD_ID already initialized!");
}

pub fn is_main_thread() -> bool {
    #[cfg(feature = "desktop")]
    {
        MAIN_THREAD_ID
            .get()
            .is_some_and(|id| *id == std::thread::current().id())
    }

    #[cfg(feature = "web")]
    true // Web/WASM is always single-threaded.
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_thread_check() {
        set_main_thread();
        assert!(is_main_thread()); // Main thread.

        let handle = std::thread::spawn(|| {
            assert!(!is_main_thread()); // Not main thread.
        });

        handle.join().unwrap();
    }
}
