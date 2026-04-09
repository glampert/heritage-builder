use objc2::{sel, rc::Retained};
use objc2_app_kit::{NSApp, NSApplication, NSApplicationPresentationOptions, NSMenu, NSWindow};
use objc2_foundation::MainThreadMarker;

use crate::{file_sys, log};

// ----------------------------------------------
// NSApplication presentation helpers
// ----------------------------------------------

fn with_app<F>(f: F)
where
    F: FnOnce(&Retained<NSApplication>),
{
    // Ensure we're on the main thread (required by AppKit).
    let mtm = MainThreadMarker::new().expect("Must be called from the main thread!");
    let app = NSApp(mtm);
    f(&app);
}

pub fn enable_kiosk_mode() {
    with_app(|app| {
        let options = NSApplicationPresentationOptions::NSApplicationPresentationHideDock
            | NSApplicationPresentationOptions::NSApplicationPresentationHideMenuBar;

        app.setPresentationOptions(options);
    });
}

pub fn disable_kiosk_mode() {
    with_app(|app| {
        let options = NSApplicationPresentationOptions::NSApplicationPresentationDefault;

        app.setPresentationOptions(options);
    });
}

pub fn enable_auto_hide_dock_and_menu_bar() {
    with_app(|app| {
        let options = NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock
            | NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar;

        app.setPresentationOptions(options);
    });
}

// Rewires the default "Quit" menu item (CMD+Q) installed by Winit so that it
// sends `performClose:` to the key window instead of `terminate:` on the app.
//
// Winit's default MacOS menu wires CMD+Q to `NSApp.terminate:`, which calls
// `applicationWillTerminate:` and then hard-exits the process from AppKit —
// the winit event loop never observes the quit and our cleanup code does not
// run. `performClose:` instead triggers `windowShouldClose:` on the key
// window, which Winit translates into `WindowEvent::CloseRequested`, giving
// us a chance to emit `ApplicationEvent::Quit` and shut down cleanly.
//
// Must be called after the Winit event loop has initialized the default menu
// (i.e. after the first pump_app_events that creates the window).
pub fn rewire_quit_menu_item_to_close() {
    with_app(|app| {
        let Some(main_menu) = (unsafe { app.mainMenu() }) else {
            log::warning!(log::channel!("app"), "rewire_quit_menu_item_to_close: no main menu installed.");
            return;
        };

        if !patch_quit_item(&main_menu) {
            log::warning!(log::channel!("app"), "rewire_quit_menu_item_to_close: Quit menu item not found.");
        }
    });
}

// Walks the menu tree looking for an item whose action is `terminate:` and
// rewrites it to `performClose:` with a nil target (so AppKit routes it via
// the responder chain to the key window). Returns true if patched.
fn patch_quit_item(menu: &NSMenu) -> bool {
    let count = unsafe { menu.numberOfItems() };
    for i in 0..count {
        let Some(item) = (unsafe { menu.itemAtIndex(i) }) else { continue };

        if let Some(submenu) = unsafe { item.submenu() } {
            if patch_quit_item(&submenu) {
                return true;
            }
        }

        if unsafe { item.action() } == Some(sel!(terminate:)) {
            unsafe {
                item.setAction(Some(sel!(performClose:)));
                item.setTarget(None);
            }
            return true;
        }
    }
    false
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
pub fn set_cursor_position(x: f64, y: f64) {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        unsafe fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> i32;
    }

    unsafe {
        CGWarpMouseCursorPosition(CGPoint { x, y });
    }
}

// ----------------------------------------------
// Stderr redirect
// ----------------------------------------------

// Redirects stderr to a log file for the duration of `f`, then restores it.
// Used to suppress TTY spam from the OpenGL loader.
pub fn redirect_stderr<F, R>(f: F, filename: &str) -> R
where
    F: FnOnce() -> R,
{
    use std::os::unix::io::AsRawFd;

    use libc::{STDERR_FILENO, close, dup, dup2};

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
