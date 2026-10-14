use super::*;
mod libc;

// ----------------------------------------------
// Crash Report System
// ----------------------------------------------

mod crash_report;

pub struct DebugBacktrace; // No-op.
impl DebugBacktrace {
    pub fn capture() -> Self { Self }
    pub fn to_string(&self, _skip_top: usize, _skip_bottom: usize) -> String { String::new() }
}

pub fn initialize_crash_report(set_panic_hook: bool) {
    crash_report::initialize(set_panic_hook);
}

// Crash reports don't name the frame on web.
#[inline]
pub fn set_crash_report_frame(_frame: u64) {}

// ----------------------------------------------
// Run Env / App Bundle Detection
// ----------------------------------------------

pub fn run_environment() -> RunEnvironment {
    RunEnvironment::WebBrowser
}

// ----------------------------------------------
// Local Date / Time
// ----------------------------------------------

pub fn local_date_time() -> Option<LocalDateTime> {
    let now = js_sys::Date::new_0();
    Some(LocalDateTime {
        year: now.get_full_year() as i32,
        month: now.get_month() + 1, // JS months start at 0.
        day: now.get_date(),
        hour: now.get_hours(),
        minute: now.get_minutes(),
        second: now.get_seconds(),
    })
}

// ----------------------------------------------
// Main Thread Detection
// ----------------------------------------------

pub fn set_main_thread() {
    // No concept of "main thread" in Web/WASM - single threaded.
}

pub fn is_main_thread() -> bool {
    // No user threads in Web/WASM - always "main thread".
    true
}

// ----------------------------------------------
// File downloads
// ----------------------------------------------

// Hands `bytes` to the browser as a file download named `file_name`, the web build's way of
// saving a file (e.g. a screenshot).
pub fn download_file(file_name: &str, bytes: &[u8], mime_type: &str) -> Result<(), String> {
    use wasm_bindgen::{JsCast, closure::Closure};

    let window = web_sys::window().ok_or("No browser window.")?;
    let document = window.document().ok_or("No document.")?;

    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|err| format!("Can't make a blob: {err:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|err| format!("Can't make a blob URL: {err:?}"))?;

    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(|err| format!("Can't make a link: {err:?}"))?
        .dyn_into()
        .map_err(|_| "Not a link element.".to_string())?;

    anchor.set_href(&url);
    anchor.set_download(file_name);
    anchor.click();

    // Free the blob once the browser has had time to start the download.
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000);

    Ok(())
}
