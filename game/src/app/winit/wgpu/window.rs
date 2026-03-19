use std::sync::Arc;
use smallvec::SmallVec;

use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Fullscreen, Window, WindowAttributes},
};

use crate::{
    log,
    utils::{Size, Vec2},
    app::{ApplicationWindowMode, ApplicationContentScale},
};

// ----------------------------------------------
// WinitWindowManager
// ----------------------------------------------

// Window manager for the winit + wgpu backend.
// Unlike the GL variant, this does NOT create a GL context.
// The wgpu RenderSystem creates its own surface/device from the window.
pub struct WinitWindowManager {
    pub window: Arc<Window>,
    pub resizable: bool,

    window_mode: ApplicationWindowMode,
    confine_cursor: bool,
    content_scale: ApplicationContentScale,
}

impl WinitWindowManager {
    pub fn create(
        event_loop: &ActiveEventLoop,
        window_title: &str,
        window_size: Size,
        window_mode: ApplicationWindowMode,
        resizable: bool,
        confine_cursor: bool,
        content_scale: ApplicationContentScale,
    ) -> Self {
        debug_assert!(window_size.is_valid());

        let needs_resizable = resizable || window_mode.is_fullscreen();
        let fullscreen = select_fullscreen(event_loop, window_mode);

        #[allow(unused_mut)]
        let mut window_attributes = WindowAttributes::default()
            .with_title(window_title)
            .with_inner_size(LogicalSize::new(window_size.width as f64, window_size.height as f64))
            .with_resizable(needs_resizable)
            .with_fullscreen(fullscreen);

        // On WASM, attach to an HTML canvas element.
        #[cfg(feature = "web")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            use wasm_bindgen::JsCast;

            let canvas = web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.get_element_by_id("game-canvas"))
                .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                .expect("Failed to find <canvas id='game-canvas'> element!");

            window_attributes = window_attributes.with_canvas(Some(canvas));
        }

        let window = Arc::new(
            event_loop.create_window(window_attributes)
                .expect("Failed to create winit window!")
        );

        log::info!(log::channel!("app"), "WinitWindowManager initialized.");
        log::info!(log::channel!("app"), "Window Size: {window_size}");

        Self {
            window,
            resizable,
            window_mode,
            confine_cursor,
            content_scale,
        }
    }

    pub fn window_arc(&self) -> Arc<Window> {
        self.window.clone()
    }

    pub fn try_confine_cursor(&self, cursor_pos: Vec2) -> Option<Vec2> {
        // No cursor confinement on WASM.
        #[cfg(feature = "web")]
        { let _ = cursor_pos; return None; }

        if !self.confine_cursor {
            return None;
        }

        let size = self.window_size();

        let mut new_x = cursor_pos.x;
        let mut new_y = cursor_pos.y;
        let mut changed = false;

        if cursor_pos.x < 0.0 {
            new_x = 0.0;
            changed = true;
        } else if cursor_pos.x > size.width as f32 {
            new_x = size.width as f32;
            changed = true;
        }

        if cursor_pos.y < 0.0 {
            new_y = 0.0;
            changed = true;
        } else if cursor_pos.y > size.height as f32 {
            new_y = size.height as f32;
            changed = true;
        }

        if changed {
            set_cursor_position_native(&self.window, new_x as f64, new_y as f64);
            Some(Vec2::new(new_x, new_y))
        } else {
            None
        }
    }

    pub fn warp_cursor_to_pos(&self, pos: Vec2) {
        #[cfg(feature = "desktop")]
        if self.confine_cursor {
            set_cursor_position_native(&self.window, pos.x as f64, pos.y as f64);
        }

        #[cfg(feature = "web")]
        let _ = pos;
    }

    #[inline]
    pub fn window_size(&self) -> Size {
        match self.content_scale {
            ApplicationContentScale::System => {
                let logical = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
                Size::new(logical.width as i32, logical.height as i32)
            }
            ApplicationContentScale::Custom(scale) => {
                let phys = self.window.inner_size();
                Size::new((phys.width as f32 / scale) as i32, (phys.height as f32 / scale) as i32)
            }
        }
    }

    #[inline]
    pub fn framebuffer_size(&self) -> Size {
        let phys = self.window.inner_size();
        Size::new(phys.width as i32, phys.height as i32)
    }

    #[inline]
    pub fn content_scale(&self) -> Vec2 {
        match self.content_scale {
            ApplicationContentScale::System => {
                let scale = self.window.scale_factor() as f32;
                Vec2::new(scale, scale)
            }
            ApplicationContentScale::Custom(scale) => {
                Vec2::new(scale, scale)
            }
        }
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn select_fullscreen(
    event_loop: &ActiveEventLoop,
    window_mode: ApplicationWindowMode,
) -> Option<Fullscreen> {
    match window_mode {
        ApplicationWindowMode::FullScreen => {
            Some(Fullscreen::Borderless(event_loop.primary_monitor()))
        }
        ApplicationWindowMode::ExclusiveFullScreen => {
            let monitor = event_loop.primary_monitor()?;
            let video_mode = select_best_video_mode(monitor.video_modes())?;
            Some(Fullscreen::Exclusive(video_mode))
        }
        ApplicationWindowMode::Windowed => None,
    }
}

fn select_best_video_mode<I>(modes: I) -> Option<winit::monitor::VideoModeHandle>
where
    I: Iterator<Item = winit::monitor::VideoModeHandle>,
{
    let all: SmallVec<[winit::monitor::VideoModeHandle; 16]> = modes.collect();
    if all.is_empty() {
        return None;
    }

    let max_area = all.iter()
        .map(|m| m.size().width * m.size().height)
        .max()?;

    let mut best: SmallVec<[&winit::monitor::VideoModeHandle; 16]> = all.iter()
        .filter(|m| m.size().width * m.size().height == max_area)
        .collect();

    if let Some(mode_60hz) = best.iter().find(|m| m.refresh_rate_millihertz() == 60_000) {
        return Some((*mode_60hz).clone());
    }

    best.sort_by_key(|m| m.refresh_rate_millihertz());
    best.last().map(|m| (*m).clone())
}

// ----------------------------------------------
// Cursor positioning
// ----------------------------------------------

#[cfg(target_os = "macos")]
fn set_cursor_position_native(window: &Window, x: f64, y: f64) {
    use crate::app::platform;
    let Ok(inner_pos) = window.inner_position() else { return };
    let scale = window.scale_factor();
    platform::warp_cursor(
        (inner_pos.x as f64 / scale) + x,
        (inner_pos.y as f64 / scale) + y,
    );
}

#[cfg(not(target_os = "macos"))]
fn set_cursor_position_native(window: &Window, x: f64, y: f64) {
    let _ = window.set_cursor_position(winit::dpi::LogicalPosition::new(x, y));
}
