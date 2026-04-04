use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use strum::Display;

use common::{Color, hash::{self, StringHash}};
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
// Log Listener
// ----------------------------------------------

pub struct Record {
    pub level: Level,
    pub channel: Option<Channel>,
    pub location: Location,
    pub message: String,
}

// One global listener, set once.
static LISTENER: OnceLock<Box<dyn Fn(Record) + Send + Sync>> = OnceLock::new();

pub fn set_listener<F>(listener_fn: F)
    where F: Fn(Record) + Send + Sync + 'static
{
    LISTENER.set(Box::new(listener_fn))
            .unwrap_or_else(|_| panic!("Log listener can only be set once!"));
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

pub fn print_internal(level: Level,
                      channel: Option<Channel>,
                      location: &Location,
                      args: fmt::Arguments) {
    if !level.is_enabled() {
        return;
    }

    #[cfg(feature = "desktop")]
    desktop::output_log(level, channel, location, args);

    #[cfg(feature = "web")]
    web::output_log(level, channel, location, args);

    if let Some(listener) = LISTENER.get() {
        listener(Record { level, channel, location: *location, message: args.to_string() });
    }
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
