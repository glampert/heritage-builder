use objc2::rc::Retained;
use objc2_foundation::MainThreadMarker;
use objc2_app_kit::{
    NSApp,
    NSWindow,
    NSApplication,
    NSApplicationPresentationOptions,
};

use crate::log;

// ----------------------------------------------
// NSApplication presentation helpers
// ----------------------------------------------

fn with_app<F>(f: F)
    where F: FnOnce(&Retained<NSApplication>)
{
    // Ensure we're on the main thread (required by AppKit).
    let mtm = MainThreadMarker::new().expect("Must be called from the main thread!");
    let app = NSApp(mtm);
    f(&app);
}

pub fn enable_kiosk_mode() {
    with_app(|app| {
        let options =
            NSApplicationPresentationOptions::NSApplicationPresentationHideDock |
            NSApplicationPresentationOptions::NSApplicationPresentationHideMenuBar;

        app.setPresentationOptions(options);
    });
}

pub fn disable_kiosk_mode() {
    with_app(|app| {
        let options =
            NSApplicationPresentationOptions::NSApplicationPresentationDefault;

        app.setPresentationOptions(options);
    });
}

pub fn enable_auto_hide_dock_and_menu_bar() {
    with_app(|app| {
        let options =
            NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock |
            NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar;

        app.setPresentationOptions(options);
    });
}

// ----------------------------------------------
// NSWindow helpers
// ----------------------------------------------

// Toggles native macOS fullscreen on the given NSWindow.
//
// # Safety
// `ns_window_ptr` must be a valid pointer to an `NSWindow` instance
// (e.g. obtained via `glfwGetCocoaWindow`).
pub fn toggle_native_fullscreen(ns_window_ptr: *mut std::ffi::c_void) {
    let _mtm = MainThreadMarker::new().expect("Must be called from the main thread!");
    assert!(!ns_window_ptr.is_null(), "NSWindow pointer is null!");

    let ns_window = unsafe { &*(ns_window_ptr as *mut NSWindow) };
    ns_window.toggleFullScreen(None);
}

// ----------------------------------------------
// Cursor warping (CoreGraphics)
// ----------------------------------------------

// Warps the OS cursor to the given position in CG global coordinates
// (top-left origin, Y-down, logical points).
pub fn warp_cursor(x: f64, y: f64) {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint { x: f64, y: f64 }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> i32;
    }

    unsafe { CGWarpMouseCursorPosition(CGPoint { x, y }); }
}

// ----------------------------------------------
// Stderr redirect
// ----------------------------------------------

// Redirects stderr to a log file for the duration of `f`, then restores it.
// Used to suppress TTY spam from the OpenGL loader.
pub fn redirect_stderr<F, R>(f: F, filename: &str) -> R
    where F: FnOnce() -> R
{
    use std::os::unix::io::AsRawFd;
    use libc::{close, dup, dup2, STDERR_FILENO};
    use crate::file_sys;

    let logs_path = log::logs_path();
    let _ = file_sys::create_path(&logs_path);

    unsafe {
        let saved_fd = dup(STDERR_FILENO);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(logs_path.join(filename))
            .expect("Failed to open stderr log file!");
        dup2(file.as_raw_fd(), STDERR_FILENO);
        let result = f();
        dup2(saved_fd, STDERR_FILENO);
        close(saved_fd);
        result
    }
}
