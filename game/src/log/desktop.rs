use std::{
    fmt, io,
    sync::{LazyLock, RwLock, atomic::Ordering},
};

use crate::utils::{
    file_sys,
    paths::{self, FixedPath},
};
use super::{Level, Channel, Location, REDIRECT_TO_FILE, ENABLE_SRC_LOCATION, ENABLE_TTY_COLORS};

// ----------------------------------------------
// Log File Path
// ----------------------------------------------

const LOG_FILENAME: &str = "runtime.log";

pub fn logs_path() -> FixedPath {
    paths::base_path().join("logs")
}

// ----------------------------------------------
// Log Output Selection
// ----------------------------------------------

enum LogOutput {
    Stdout(io::Stdout),
    File(std::fs::File),
}

impl io::Write for LogOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.write(buf),
            Self::File(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(stdout) => stdout.flush(),
            Self::File(file) => file.flush(),
        }
    }
}

fn init_log_output() -> LogOutput {
    if REDIRECT_TO_FILE.load(Ordering::Relaxed) {
        let logs_path = logs_path();
        file_sys::create_path(&logs_path);

        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(logs_path.join(LOG_FILENAME))
        {
            super::enable_tty_colors(false);
            LogOutput::File(file)
        } else {
            // Fallback to TTY
            LogOutput::Stdout(io::stdout())
        }
    } else {
        // TTY
        LogOutput::Stdout(io::stdout())
    }
}

static LOG_OUTPUT: LazyLock<RwLock<LogOutput>> = LazyLock::new(|| {
    RwLock::new(init_log_output())
});

// ----------------------------------------------
// Desktop Log Output
// ----------------------------------------------

pub fn output_log(level: Level,
                  channel: Option<Channel>,
                  location: &Location,
                  args: fmt::Arguments)
{
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
        writeln!(output,
                 "{}[{:?}]{}{} {}:{} {} - {}",
                 color_start,
                 level,
                 chan_str,
                 color_end,
                 location.file,
                 location.line,
                 location.module,
                 args)
                 .unwrap();
    } else {
        writeln!(output,
                 "{}[{:?}]{}{} {}",
                 color_start,
                 level,
                 chan_str,
                 color_end,
                 args)
                 .unwrap();
    }
}
