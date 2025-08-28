use std::fmt;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::hash::{Hash, Hasher};

use crate::utils::{Color, hash::{self, StringHash}};

// ----------------------------------------------
// Log Levels
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Silent,
    Verbose,
    Info,
    Warn,
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
            Self::Warn    => Color::yellow(),
            Self::Error   => Color::red(),
        }
    }

    fn tty_color(self) -> (&'static str, &'static str) {
        match self {
            Self::Silent  => ("", ""),
            Self::Verbose => ("\x1b[90m", "\x1b[0m"), // gray
            Self::Info    => ("\x1b[32m", "\x1b[0m"), // green
            Self::Warn    => ("\x1b[33m", "\x1b[0m"), // yellow
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
        Self {
            name,
            hash: hash::fnv1a_from_str(name),
        }
    }
}

impl Hash for Channel {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

#[macro_export]
macro_rules! channel {
    ($name:literal) => { $crate::log::Channel::new(concat!(" [", $name, "]")) };
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
    LISTENER.set(Box::new(listener_fn)).unwrap_or_else(|_| panic!("Log listener can only be set once!"));
}

// ----------------------------------------------
// Global Configs
// ----------------------------------------------

static MIN_LOG_LEVEL: AtomicU32 = AtomicU32::new(Level::Verbose as u32);
static ENABLE_SRC_LOCATION: AtomicBool = AtomicBool::new(false);
static ENABLE_TTY_COLORS: AtomicBool = AtomicBool::new(true);

pub fn set_level(level: Level) {
    MIN_LOG_LEVEL.store(level as u32, Ordering::Relaxed);
}

pub fn enable_source_location(enable: bool) {
    ENABLE_SRC_LOCATION.store(enable, Ordering::Relaxed);
}

pub fn enable_tty_colors(enable: bool) {
    ENABLE_TTY_COLORS.store(enable, Ordering::Relaxed);
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

    let chan_str = channel
        .as_ref()
        .map(|chan| chan.name)
        .unwrap_or_default();

    let (color_start, color_end) = {
        if ENABLE_TTY_COLORS.load(Ordering::Relaxed) {
            level.tty_color()
        } else {
            ("", "")
        }
    };

    let mut out = std::io::stdout();

    if ENABLE_SRC_LOCATION.load(Ordering::Relaxed) {
        writeln!(
            &mut out,
            "{}[{:?}]{}{} {}:{} {} - {}",
            color_start, level, chan_str, color_end,
            location.file, location.line, location.module, args
        ).unwrap();
    } else {
        writeln!(
            &mut out,
            "{}[{:?}]{}{} {}",
            color_start, level, chan_str, color_end, args
        ).unwrap();
    }

    if let Some(listener) = LISTENER.get() {
        listener(Record {
            level,
            channel,
            location: *location,
            message: args.to_string(),
        });
    }
}

// Shared helper used by all logging macros.
#[macro_export]
macro_rules! log_message {
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
macro_rules! verbose {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Verbose, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Verbose, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Info
#[macro_export]
macro_rules! info {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Info, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Info, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Warn
#[macro_export]
macro_rules! warn {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Warn, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Warn, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Error
#[macro_export]
macro_rules! error {
    ($fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Error, None, $fmt $(, $($arg)+)?)
    };
    ($chan:expr, $fmt:literal $(, $($arg:tt)+)?) => {
        $crate::log_message!($crate::log::Level::Error, Some($chan), $fmt $(, $($arg)+)?)
    };
}

// Re-export these here so usage is scoped, e.g., log::info!(), log::warn!(), etc.
#[allow(unused_imports)]
pub use crate::{channel, verbose, info, warn, error};
