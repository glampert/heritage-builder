use std::{
    fmt,
    io::{self, IsTerminal},
    sync::{LazyLock, RwLock, atomic::Ordering},
};

use super::{Channel, ENABLE_SRC_LOCATION, ENABLE_TTY_COLORS, Level, Location, REDIRECT_TO_FILE, REDIRECT_TO_STDERR};
use crate::file_sys;

// ----------------------------------------------
// Log Output Selection
// ----------------------------------------------

const LOG_FILENAME: &str = "runtime.log";

enum LogOutput {
    Stdout(io::Stdout),
    Stderr(io::Stderr),
    File(std::fs::File),
}

impl io::Write for LogOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.write(buf),
            Self::Stderr(stderr) => stderr.write(buf),
            Self::File(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(stdout) => stdout.flush(),
            Self::Stderr(stderr) => stderr.flush(),
            Self::File(file) => file.flush(),
        }
    }
}

fn init_log_output() -> LogOutput {
    if REDIRECT_TO_FILE.load(Ordering::Relaxed) {
        let log_file_path = super::logs_path().join(LOG_FILENAME);
        let _ = file_sys::create_path(&log_file_path);

        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)
        {
            super::enable_tty_colors(false);
            LogOutput::File(file)
        } else {
            // Fallback to TTY
            LogOutput::Stdout(io::stdout())
        }
    } else if REDIRECT_TO_STDERR.load(Ordering::Relaxed) {
        // Color codes only help a terminal; they'd clutter a log captured to a file.
        if !io::stderr().is_terminal() {
            super::enable_tty_colors(false);
        }
        LogOutput::Stderr(io::stderr())
    } else {
        // TTY
        LogOutput::Stdout(io::stdout())
    }
}

static LOG_OUTPUT: LazyLock<RwLock<LogOutput>> = LazyLock::new(|| RwLock::new(init_log_output()));

// ----------------------------------------------
// Desktop Log Output
// ----------------------------------------------

pub fn output_log(level: Level, channel: Option<Channel>, location: &Location, args: fmt::Arguments) {
    use io::Write;
    let mut output = LOG_OUTPUT.write().unwrap();

    let chan_str = channel.map(|chan| chan.name).unwrap_or_default();

    let (color_start, color_end) = {
        if ENABLE_TTY_COLORS.load(Ordering::Relaxed) {
            level.tty_color()
        } else {
            ("", "")
        }
    };

    if ENABLE_SRC_LOCATION.load(Ordering::Relaxed) {
        writeln!(
            output,
            "{}[{:?}]{}{} {}:{} {} - {}",
            color_start, level, chan_str, color_end, location.file, location.line, location.module, args
        )
        .unwrap();
    } else {
        writeln!(output, "{}[{:?}]{}{} {}", color_start, level, chan_str, color_end, args).unwrap();
    }
}
