use std::{sync::OnceLock, thread::ThreadId};

use super::*;

// ----------------------------------------------
// Crash Report System
// ----------------------------------------------

mod crash_report;
pub use crash_report::DebugBacktrace;

pub fn initialize_crash_report(set_panic_hook: bool) {
    crash_report::initialize(set_panic_hook);
}

// ----------------------------------------------
// Run Env / App Bundle Detection
// ----------------------------------------------

pub fn run_environment() -> RunEnvironment {
    #[cfg(target_os = "macos")]
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

    RunEnvironment::Standalone
}

// ----------------------------------------------
// Main Thread Detection
// ----------------------------------------------

static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();

pub fn set_main_thread() {
    MAIN_THREAD_ID.set(std::thread::current().id()).expect("MAIN_THREAD_ID already initialized!");
}

pub fn is_main_thread() -> bool {
    MAIN_THREAD_ID.get().is_some_and(|id| *id == std::thread::current().id())
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
