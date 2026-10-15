use std::{
    fmt::Write as _,
    panic,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use backtrace::Backtrace;

use crate::{file_sys, log, platform};

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
// Panic reports
// ----------------------------------------------

pub struct PanicReport<'a> {
    pub message: &'a str,
    pub location: &'a str,
    pub frame: u64,                    // Frame the panic happened on.
    pub crash_file: Option<&'a Path>,  // logs/crash-<timestamp>.txt, if it could be written.
}

type PanicListenerFn = Box<dyn Fn(&PanicReport) + Send + Sync>;

static PANIC_LISTENERS: Mutex<Vec<PanicListenerFn>> = Mutex::new(Vec::new());
static CURRENT_FRAME: AtomicU64 = AtomicU64::new(0);

// Called by the panic hook, on the panicking thread, after the crash file is written. Listeners
// must not panic, and shouldn't block for long.
pub fn add_panic_listener<F>(listener_fn: F)
where
    F: Fn(&PanicReport) + Send + Sync + 'static,
{
    PANIC_LISTENERS.lock().unwrap_or_else(PoisonError::into_inner).push(Box::new(listener_fn));
}

// The engine updates this every frame so crash reports can name the frame.
#[inline]
pub fn set_current_frame(frame: u64) {
    CURRENT_FRAME.store(frame, Ordering::Relaxed);
}

// ----------------------------------------------
// initialize()
// ----------------------------------------------

// Every panic writes logs/crash-<timestamp>.txt (message, backtrace, frame, recent log records)
// and notifies the panic listeners. With `log_panics`, the panic and its backtrace also go to the
// log, for runs whose log goes to a file; otherwise Rust's default hook prints the panic.
pub fn initialize(log_panics: bool) {
    log::info!(log::channel!("crash_report"), "Setting panic hook ...");

    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Unknown panic");

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "<unknown>".to_string());

        let frame = CURRENT_FRAME.load(Ordering::Relaxed);
        let backtrace = DebugBacktrace::capture().to_string(0, 0);

        let crash_file = write_crash_file(message, &location, frame, &backtrace);

        // A listener that panics would land back here; try_lock keeps that from deadlocking.
        if let Ok(listeners) = PANIC_LISTENERS.try_lock() {
            let report = PanicReport { message, location: &location, frame, crash_file: crash_file.as_deref() };
            for listener in listeners.iter() {
                listener(&report);
            }
        }

        if log_panics {
            log::error!("==========================");
            log::error!("         PANIC'D          ");
            log::error!("==========================");
            log::error!("");
            log::error!("Message  : {message}");
            log::error!("Location : {location}");
            log::error!("");
            log::error!("==========================");
            log::error!("         BACKTRACE        ");
            log::error!("==========================");
            log::error!("\n{backtrace}\n");
        } else {
            default_hook(panic_info);
        }
    }));
}

// Writes the crash report file. Must not panic: it runs inside the panic hook.
fn write_crash_file(message: &str, location: &str, frame: u64, backtrace: &str) -> Option<PathBuf> {
    let stamp = platform::local_date_time()
        .map(|now| {
            format!(
                "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
                now.year, now.month, now.day, now.hour, now.minute, now.second
            )
        })
        .unwrap_or_else(|| "unknown-time".to_string());

    let logs_path = log::logs_path();
    let path = (1..100)
        .map(|n| {
            let file_name = if n == 1 { format!("crash-{stamp}.txt") } else { format!("crash-{stamp}-{n}.txt") };
            logs_path.as_path().join(file_name)
        })
        .find(|path| !path.exists())?;

    let mut report = String::new();
    let _ = writeln!(report, "Message  : {message}");
    let _ = writeln!(report, "Location : {location}");
    let _ = writeln!(report, "Frame    : {frame}");
    let _ = writeln!(report, "Thread   : {}", std::thread::current().name().unwrap_or("<unnamed>"));
    let _ = writeln!(report, "\nBacktrace:\n{backtrace}");

    // The logger's locks can be held by the code that panicked, so this never waits for them.
    match log::try_recent_records(0) {
        Some(records) => {
            let _ = writeln!(report, "Recent log records (oldest first):");
            for record in &records {
                let channel = record.channel.map(|channel| channel.name).unwrap_or_default();
                let _ = writeln!(report, "  #{} [{:?}]{} {}", record.seq, record.level, channel, record.message);
            }
        }
        None => {
            let _ = writeln!(report, "Recent log records: unavailable (the logger was busy).");
        }
    }

    file_sys::create_path(&path).ok()?;
    file_sys::write_file(&path, report).ok()?;
    Some(path)
}
