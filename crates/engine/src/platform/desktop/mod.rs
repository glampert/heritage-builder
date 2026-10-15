use std::{sync::OnceLock, thread::ThreadId};

use super::*;

// ----------------------------------------------
// Crash Report System
// ----------------------------------------------

mod crash_report;
pub use crash_report::{DebugBacktrace, PanicReport, add_panic_listener};

pub fn initialize_crash_report(log_panics: bool) {
    crash_report::initialize(log_panics);
}

#[inline]
pub fn set_crash_report_frame(frame: u64) {
    crash_report::set_current_frame(frame);
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
// Local Date / Time
// ----------------------------------------------

// None if the C library can't convert the current time.
pub fn local_date_time() -> Option<LocalDateTime> {
    // SAFETY: localtime_r is the thread-safe variant and only writes to `tm`.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return None;
        }
        tm
    };

    Some(LocalDateTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
    })
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
    fn local_date_time_is_plausible() {
        let now = local_date_time().unwrap();
        assert!(now.year >= 2024);
        assert!((1..=12).contains(&now.month) && (1..=31).contains(&now.day));
        assert!(now.hour < 24 && now.minute < 60 && now.second < 61); // 60 is a leap second.
    }

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
