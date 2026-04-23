use std::{fmt::Write as _, panic, path::Path};
use backtrace::Backtrace;
use crate::log;

// ----------------------------------------------
// DebugBacktrace
// ----------------------------------------------

pub struct DebugBacktrace {
    backtrace: Backtrace,
}

impl DebugBacktrace {
    pub fn capture() -> Self {
        Self { backtrace: Backtrace::new() }
    }

    // `backtrace` orders frames most-recent first (frame 0 = deepest call,
    // last frame = `main` / thread entry). `skip_top` drops the N newest
    // frames from the front; `skip_bottom` drops the N oldest from the back.
    pub fn to_string(&self, skip_top: usize, skip_bottom: usize) -> String {
        let mut out = String::new();

        let frames = self.backtrace.frames();
        let start = skip_top.min(frames.len());
        let end = frames.len().saturating_sub(skip_bottom).max(start);

        for (i, frame) in frames[start..end].iter().enumerate() {
            // Each frame can contain multiple symbols (e.g., inlined functions).
            for symbol in frame.symbols() {
                let name = symbol.name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "<unknown>".into());

                let file = symbol.filename()
                    .and_then(Path::file_name)
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<unknown>".into());

                let line = symbol.lineno().unwrap_or(0);

                writeln!(out, "  #{:<2} {} @ [{}:{}]", start + i, name, file, line).unwrap();
            }
        }

        out
    }
}

// ----------------------------------------------
// initialize()
// ----------------------------------------------

pub fn initialize(set_panic_hook: bool) {
    if !set_panic_hook {
        return;
    }

    log::info!(log::channel!("crash_report"), "Setting debug panic hook ...");

    panic::set_hook(Box::new(move |panic_info| {
        let error_message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Unknown panic");

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "<unknown>".to_string());

        log::error!("==========================");
        log::error!("         PANIC'D          ");
        log::error!("==========================");
        log::error!("");
        log::error!("Message  : {error_message}");
        log::error!("Location : {location}");
        log::error!("");
        log::error!("==========================");
        log::error!("         BACKTRACE        ");
        log::error!("==========================");

        let backtrace = DebugBacktrace::capture();
        log::error!("\n{}\n", backtrace.to_string(0, 0));
    }));
}
