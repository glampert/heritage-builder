use std::{panic, path::Path, fmt::Write as _};
use backtrace::Backtrace;
use crate::log;

pub fn initialize(set_panic_hook: bool) {
    if !set_panic_hook {
        return;
    }

    log::info!(log::channel!("crash_report"), "Setting debug panic hook ...");

    panic::set_hook(Box::new(move |panic_info| {
        let error_message = panic_info.payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Unknown panic");

        let location = panic_info.location()
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

        let backtrace = Backtrace::new();
        let mut out = String::new();

        for (i, frame) in backtrace.frames().iter().enumerate() {
            // Each frame can contain multiple symbols (e.g., inlined functions).
            for symbol in frame.symbols() {
                let name = symbol.name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "<unknown>".into());

                let file = symbol.filename()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".into());

                let line = symbol.lineno().unwrap_or(0);

                writeln!(out, "  #{:<2} {} @ [{:?}:{}]",
                         i, name, Path::new(&file).file_name().unwrap(), line).unwrap();
            }
        }

        log::error!("\n{out}\n");
    }));
}
