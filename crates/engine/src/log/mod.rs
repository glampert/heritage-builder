use std::{
    cell::Cell,
    collections::VecDeque,
    fmt,
    hash::{Hash, Hasher},
    sync::{
        Mutex,
        PoisonError,
        RwLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use common::{
    Color,
    hash::{self, StringHash},
};
use serde::{Deserialize, Serialize};
use strum::Display;

use crate::file_sys::paths::{self, FixedPath};

#[cfg(feature = "desktop")]
mod desktop;

#[cfg(feature = "web")]
mod web;

// ----------------------------------------------
// Log Levels
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Debug, Display, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    Silent,
    Verbose,
    Info,
    Warning,
    Error,
}

impl Level {
    #[inline]
    pub fn is_enabled(self) -> bool {
        (self as u32) >= MIN_LOG_LEVEL.load(Ordering::Relaxed)
    }

    pub fn color(self) -> Color {
        match self {
            Self::Silent  => Color::white(),
            Self::Verbose => Color::gray(),
            Self::Info    => Color::green(),
            Self::Warning => Color::yellow(),
            Self::Error   => Color::red(),
        }
    }

    fn tty_color(self) -> (&'static str, &'static str) {
        match self {
            Self::Silent  => ("", ""),
            Self::Verbose => ("\x1b[90m", "\x1b[0m"), // gray
            Self::Info    => ("\x1b[32m", "\x1b[0m"), // green
            Self::Warning => ("\x1b[33m", "\x1b[0m"), // yellow
            Self::Error   => ("\x1b[31m", "\x1b[0m"), // red
        }
    }
}

// ----------------------------------------------
// Log Channel
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Channel {
    pub name: &'static str,
    pub hash: StringHash,
}

impl Channel {
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self { name, hash: hash::fnv1a_from_str(name) }
    }
}

impl Hash for Channel {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

#[macro_export]
macro_rules! log_channel {
    ($name:literal) => {
        $crate::log::Channel::new(concat!(" [", $name, "]"))
    };
}

// ----------------------------------------------
// Log Records & Listeners
// ----------------------------------------------

#[derive(Clone)]
pub struct Record {
    pub seq: u64, // Increases by one per record, starting at 1. Unique within the process.
    pub level: Level,
    pub channel: Option<Channel>,
    pub location: Location,
    pub message: String,
}

type ListenerFn = Box<dyn Fn(&Record) + Send + Sync>;

static LISTENERS: RwLock<Vec<ListenerFn>> = RwLock::new(Vec::new());

// Registers a function that receives every record at or above the current log level,
// on the thread that logged it. Records logged from inside a listener are printed but
// not passed to listeners again, so a listener that logs can't recurse or deadlock.
pub fn add_listener<F>(listener_fn: F)
where
    F: Fn(&Record) + Send + Sync + 'static,
{
    LISTENERS.write().unwrap_or_else(PoisonError::into_inner).push(Box::new(listener_fn));
}

// Copies of the most recent records with a sequence number greater than `after_seq`,
// oldest first. Pass 0 to get everything still in the buffer.
pub fn recent_records(after_seq: u64) -> Vec<Record> {
    let recent = RECENT_RECORDS.lock().unwrap_or_else(PoisonError::into_inner);
    recent.records.iter().filter(|record| record.seq > after_seq).cloned().collect()
}

// Number of records kept for recent_records().
pub const RECENT_RECORDS_CAPACITY: usize = 256;

struct RecentRecords {
    next_seq: u64,
    records: VecDeque<Record>,
}

impl RecentRecords {
    const fn new() -> Self {
        Self { next_seq: 1, records: VecDeque::new() }
    }

    // Assigns the next sequence number, keeps a copy (evicting the oldest when full)
    // and returns the new record.
    fn push(&mut self, level: Level, channel: Option<Channel>, location: &Location, message: String) -> Record {
        let record = Record { seq: self.next_seq, level, channel, location: *location, message };
        self.next_seq += 1;

        if self.records.len() == RECENT_RECORDS_CAPACITY {
            self.records.pop_front();
        }
        self.records.push_back(record.clone());

        record
    }
}

// Sequence numbers are assigned under this lock, so the buffer is always in sequence order,
// even when several threads log at once.
static RECENT_RECORDS: Mutex<RecentRecords> = Mutex::new(RecentRecords::new());

thread_local! {
    static IS_DISPATCHING: Cell<bool> = const { Cell::new(false) };
}

fn dispatch_record(level: Level, channel: Option<Channel>, location: &Location, message: String) {
    let record = RECENT_RECORDS.lock().unwrap_or_else(PoisonError::into_inner).push(level, channel, location, message);

    // Skip listeners for records logged by a listener (see add_listener).
    if IS_DISPATCHING.with(|dispatching| dispatching.replace(true)) {
        return;
    }

    // Resets the flag even if a listener panics.
    struct ResetDispatching;
    impl Drop for ResetDispatching {
        fn drop(&mut self) {
            IS_DISPATCHING.with(|dispatching| dispatching.set(false));
        }
    }
    let _reset = ResetDispatching;

    for listener in LISTENERS.read().unwrap_or_else(PoisonError::into_inner).iter() {
        listener(&record);
    }
}

// ----------------------------------------------
// Global Configs
// ----------------------------------------------

static MIN_LOG_LEVEL:       AtomicU32  = AtomicU32::new(Level::Verbose as u32);
static REDIRECT_TO_FILE:    AtomicBool = AtomicBool::new(false);
static ENABLE_SRC_LOCATION: AtomicBool = AtomicBool::new(false);
static ENABLE_TTY_COLORS:   AtomicBool = AtomicBool::new(true);

pub fn set_level(level: Level) {
    MIN_LOG_LEVEL.store(level as u32, Ordering::Relaxed);
}

pub fn redirect_to_file(redirect: bool) {
    REDIRECT_TO_FILE.store(redirect, Ordering::Relaxed);
}

pub fn enable_source_location(enable: bool) {
    ENABLE_SRC_LOCATION.store(enable, Ordering::Relaxed);
}

pub fn enable_tty_colors(enable: bool) {
    ENABLE_TTY_COLORS.store(enable, Ordering::Relaxed);
}

pub fn logs_path() -> FixedPath {
    paths::base_path().join("logs")
}

// ----------------------------------------------
// Internal Implementation
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct Location {
    pub file: &'static str,
    pub line: u32,
    pub module: &'static str,
}

pub fn print_internal(level: Level, channel: Option<Channel>, location: &Location, args: fmt::Arguments) {
    if !level.is_enabled() {
        return;
    }

    // Format once; the output backends and the record share the string.
    let message = args.to_string();

    #[cfg(feature = "desktop")]
    desktop::output_log(level, channel, location, format_args!("{message}"));

    #[cfg(feature = "web")]
    web::output_log(level, channel, location, format_args!("{message}"));

    dispatch_record(level, channel, location, message);
}

// Shared helper used by all logging macros.
#[macro_export]
macro_rules! log_print_message {
    ($level:expr, $chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        if $level.is_enabled() {
            $crate::log::print_internal(
                $level,
                $chan,
                &$crate::log::Location { file: file!(), line: line!(), module: module_path!() },
                format_args!($fmt $(, $($arg)+)?)
            );
        }
    };
}

// ----------------------------------------------
// Public API
// ----------------------------------------------

// Verbose
#[macro_export]
macro_rules! log_verbose {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Verbose, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Verbose, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Info
#[macro_export]
macro_rules! log_info {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Info, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Info, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Warning
#[macro_export]
macro_rules! log_warning {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Warning, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Warning, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Error
#[macro_export]
macro_rules! log_error {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Error, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log::print_message!($crate::log::Level::Error, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Re-export #[macro_export] macros into this module for scoped usage: log::info!(), log::warning!(), etc.
#[allow(unused_imports)]
pub use crate::{
    log_channel       as channel,
    log_verbose       as verbose,
    log_info          as info,
    log_warning       as warning,
    log_error         as error,
    log_print_message as print_message,
};

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    // Tests run in parallel and share the global listener list and record buffer,
    // so every test only looks at records carrying its own marker string.

    #[test]
    fn test_multiple_listeners_receive_records() {
        const MARKER: &str = "log-test-two-listeners";

        let received_a = Arc::new(Mutex::new(Vec::new()));
        let received_b = Arc::new(Mutex::new(Vec::new()));

        let sink_a = received_a.clone();
        add_listener(move |record| {
            if record.message.contains(MARKER) {
                sink_a.lock().unwrap().push((record.seq, record.message.clone()));
            }
        });

        let sink_b = received_b.clone();
        add_listener(move |record| {
            if record.message.contains(MARKER) {
                sink_b.lock().unwrap().push((record.seq, record.message.clone()));
            }
        });

        crate::log::info!("{MARKER} first");
        crate::log::warning!(crate::log::channel!("test"), "{MARKER} second");

        let received_a = received_a.lock().unwrap();
        let received_b = received_b.lock().unwrap();

        assert_eq!(received_a.len(), 2);
        assert_eq!(*received_a, *received_b);
        assert_eq!(received_a[0].1, format!("{MARKER} first"));
        assert_eq!(received_a[1].1, format!("{MARKER} second"));
        assert!(received_a[0].0 < received_a[1].0);
    }

    #[test]
    fn test_recent_records() {
        const MARKER: &str = "log-test-recent-records";

        let before = recent_records(0).last().map_or(0, |record| record.seq);

        for i in 0..3 {
            crate::log::info!("{MARKER} {i}");
        }

        let mine: Vec<Record> =
            recent_records(before).into_iter().filter(|record| record.message.contains(MARKER)).collect();

        assert_eq!(mine.len(), 3);
        assert!(mine.windows(2).all(|pair| pair[0].seq < pair[1].seq));
        assert_eq!(mine[2].message, format!("{MARKER} 2"));
        assert_eq!(mine[2].level, Level::Info);
    }

    #[test]
    fn test_recent_records_capacity() {
        // Uses a local buffer; flooding the global one could evict other tests' records.
        let mut recent = RecentRecords::new();
        let location = Location { file: file!(), line: line!(), module: module_path!() };

        let total = RECENT_RECORDS_CAPACITY + 10;
        for i in 0..total {
            let record = recent.push(Level::Info, None, &location, format!("record {i}"));
            assert_eq!(record.seq, i as u64 + 1);
        }

        // Only the newest records are kept, in sequence order.
        assert_eq!(recent.records.len(), RECENT_RECORDS_CAPACITY);
        assert_eq!(recent.records.front().unwrap().seq, (total - RECENT_RECORDS_CAPACITY) as u64 + 1);
        assert_eq!(recent.records.back().unwrap().seq, total as u64);
        assert_eq!(recent.records.back().unwrap().message, format!("record {}", total - 1));
    }

    #[test]
    fn test_listener_that_logs_does_not_recurse() {
        const MARKER: &str = "log-test-reentrant";
        const NESTED: &str = "log-test-reentrant-nested";

        let nested_seen = Arc::new(Mutex::new(0));

        let seen = nested_seen.clone();
        add_listener(move |record| {
            if record.message.contains(NESTED) {
                *seen.lock().unwrap() += 1;
            } else if record.message.contains(MARKER) {
                // Logging from a listener must neither deadlock nor reach listeners again.
                crate::log::info!("{NESTED}");
            }
        });

        crate::log::info!("{MARKER}");

        assert_eq!(*nested_seen.lock().unwrap(), 0);

        // The nested record still lands in the recent records buffer.
        assert!(recent_records(0).iter().any(|record| record.message == NESTED));
    }
}
