use crate::log;

pub fn initialize(set_panic_hook: bool) {
    if !set_panic_hook {
        return;
    }

    log::info!(log::channel!("crash_report"), "Setting WASM panic hook ...");

    // Install console_error_panic_hook so that panics produce readable
    // stack traces in the browser console instead of "unreachable executed".
    console_error_panic_hook::set_once();
}
