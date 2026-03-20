use std::{fmt, sync::atomic::Ordering};
use super::{Level, Channel, Location, ENABLE_SRC_LOCATION};

// ----------------------------------------------
// Web Console Log Output
// ----------------------------------------------

pub fn output_log(level: Level,
                  channel: Option<Channel>,
                  location: &Location,
                  args: fmt::Arguments)
{
    let chan_str = channel.map(|chan| chan.name).unwrap_or_default();

    let msg = if ENABLE_SRC_LOCATION.load(Ordering::Relaxed) {
        format!("[{:?}]{} {}:{} {} - {}",
                level, chan_str, location.file, location.line, location.module, args)
    } else {
        format!("[{:?}]{} {}", level, chan_str, args)
    };

    let js_msg = wasm_bindgen::JsValue::from_str(&msg);
    match level {
        Level::Silent | Level::Verbose => web_sys::console::debug_1(&js_msg),
        Level::Info  => web_sys::console::info_1(&js_msg),
        Level::Warn  => web_sys::console::warn_1(&js_msg),
        Level::Error => web_sys::console::error_1(&js_msg),
    }
}
