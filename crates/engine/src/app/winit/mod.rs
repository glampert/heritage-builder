use common::{Size, Vec2, mem::RcMut};
use enum_dispatch::enum_dispatch;
use smallvec::SmallVec;
use winit::{
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    monitor::VideoModeHandle,
    window::{Fullscreen, Window},
};

use super::{
    ApplicationApi,
    ApplicationBackend,
    ApplicationContentScale,
    ApplicationEvent,
    ApplicationEventList,
    ApplicationInitParams,
    ApplicationWindowMode,
    input::{InputSystem, InputSystemBackendImpl},
};
use crate::{log, render::RenderApi};

pub(crate) mod input;
pub use input::WinitInputSystemBackend;

pub(crate) mod wgpu;

#[cfg(feature = "desktop")]
mod opengl;

// ----------------------------------------------
// WinitWindowManagerImpl
// ----------------------------------------------

#[enum_dispatch]
enum WinitWindowManagerImpl {
    Wgpu(wgpu::WinitWindowManager),

    #[cfg(feature = "desktop")]
    OpenGl(opengl::WinitWindowManager),
}

// ----------------------------------------------
// WinitWindowManager
// ----------------------------------------------

#[enum_dispatch(WinitWindowManagerImpl)]
trait WinitWindowManager: Sized {
    fn window(&self) -> &Window;
    fn app_context(&self) -> Option<&dyn std::any::Any>;

    fn present(&mut self);
    fn resize_framebuffer(&mut self, new_size: Size);

    fn poll_events<F>(&mut self, handler: F)
    where
        F: FnMut(&ActiveEventLoop, WindowEvent);

    fn set_cursor_position(&mut self, pos: Vec2);
}

// ----------------------------------------------
// WinitApplicationBackend
// ----------------------------------------------

pub struct WinitApplicationBackend {
    should_quit: bool,
    phys_window_size: winit::dpi::PhysicalSize<u32>, // Last known physical window size.
    window_manager: RcMut<WinitWindowManagerImpl>,
    input_state: RcMut<input::WinitInputState>,
    content_scale: ApplicationContentScale,
    resizable_window: bool,
    confine_cursor: bool,
}

impl WinitApplicationBackend {
    pub fn new(params: &ApplicationInitParams) -> Self {
        assert!(params.app_api == ApplicationApi::Winit);

        log::info!(log::channel!("app"), "--- App Backend: Winit ---");

        let window_manager = Self::new_window_manager(params);
        let phys_window_size = window_manager.window().inner_size();

        Self {
            should_quit: false,
            phys_window_size,
            window_manager,
            input_state: input::WinitInputState::new(),
            content_scale: params.content_scale,
            resizable_window: params.resizable_window,
            confine_cursor: params.confine_cursor,
        }
    }

    fn new_window_manager(params: &ApplicationInitParams) -> RcMut<WinitWindowManagerImpl> {
        RcMut::new(match params.render_api {
            RenderApi::Wgpu => WinitWindowManagerImpl::from(wgpu::WinitWindowManager::new(params)),

            #[cfg(feature = "desktop")]
            RenderApi::OpenGl => WinitWindowManagerImpl::from(opengl::WinitWindowManager::new(params)),
        })
    }

    fn confine_cursor_to_window(&mut self) {
        debug_assert!(self.confine_cursor);

        let window_size = self.window_size().to_vec2();
        let cursor_pos = self.input_state.cursor_pos();

        let mut new_x = cursor_pos.x;
        let mut new_y = cursor_pos.y;
        let mut changed = false;

        if cursor_pos.x < 0.0 {
            new_x = 0.0;
            changed = true;
        } else if cursor_pos.x > window_size.x {
            new_x = window_size.x;
            changed = true;
        }

        if cursor_pos.y < 0.0 {
            new_y = 0.0;
            changed = true;
        } else if cursor_pos.y > window_size.y {
            new_y = window_size.y;
            changed = true;
        }

        if changed {
            self.set_cursor_position(Vec2::new(new_x, new_y));
        }
    }

    fn set_cursor_position(&mut self, pos: Vec2) {
        debug_assert!(self.confine_cursor);
        self.window_manager.set_cursor_position(pos);
        self.input_state.set_cursor_pos(pos);
    }

    fn handle_window_event(&mut self, events: &mut ApplicationEventList, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.request_quit();
                events.push(ApplicationEvent::Quit);
                event_loop.exit();
            }
            WindowEvent::Resized(new_phys_size) => {
                if self.resizable_window && self.phys_window_size != new_phys_size {
                    let prev_size = Size::new(self.phys_window_size.width as i32, self.phys_window_size.height as i32);
                    let new_size = Size::new(new_phys_size.width as i32, new_phys_size.height as i32);

                    log::info!(log::channel!("app"), "WindowEvent::Resized: Prev: {prev_size}, New: {new_size}");

                    self.phys_window_size = new_phys_size;
                    self.window_manager.resize_framebuffer(new_size);

                    let window_size = self.window_size();
                    let framebuffer_size = self.framebuffer_size();

                    events.push(ApplicationEvent::WindowResize { window_size, framebuffer_size });
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.input_state.set_modifiers(input::winit_modifiers_to_input_modifiers(new_modifiers.state()));
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let key = input::winit_physical_key_to_input_key(key_event.physical_key);
                let action = input::winit_element_state_to_input_action(key_event.state, key_event.repeat);
                let modifiers = self.input_state.modifiers();

                self.input_state.set_key(key, key_event.state.is_pressed());
                events.push(ApplicationEvent::KeyInput(key, action, modifiers));

                // Emit CharInput for printable characters on key press/repeat.
                // `text` is set by winit for keys that produce a character.
                if let Some(text) = key_event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        events.push(ApplicationEvent::CharInput(c));
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if let Some(mb) = input::winit_mouse_button_to_mouse_button(button) {
                    let action = input::winit_element_state_to_input_action(state, false);
                    let modifiers = self.input_state.modifiers();

                    self.input_state.set_mouse_button(mb, state.is_pressed());
                    events.push(ApplicationEvent::MouseButton(mb, action, modifiers));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = input::winit_mouse_scroll_delta_to_vec2(delta);
                events.push(ApplicationEvent::Scroll(scroll));
            }
            WindowEvent::CursorLeft { .. } => {
                if self.confine_cursor {
                    // When confinement is enabled: the title bar is outside the content
                    // view, so winit stops sending CursorMoved once the cursor enters it
                    // and clamping never triggers. Warp back to the last known in-bounds
                    // position the moment CursorLeft fires.
                    let last_in_window_pos = self.input_state.cursor_pos();
                    self.set_cursor_position(last_in_window_pos);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.content_scale();
                self.input_state.set_cursor_pos(Vec2::new(position.x as f32 / scale.x, position.y as f32 / scale.y));
            }
            _ => {} // Unhandled event.
        }
    }
}

// ----------------------------------------------
// ApplicationBackend (Winit)
// ----------------------------------------------

impl ApplicationBackend for WinitApplicationBackend {
    fn new_input_system(&mut self) -> InputSystem {
        let input_system = WinitInputSystemBackend::new(self.input_state.clone());

        InputSystem::new(InputSystemBackendImpl::Winit(input_system))
    }

    #[inline]
    fn app_context(&self) -> Option<&dyn std::any::Any> {
        self.window_manager.app_context()
    }

    #[inline]
    fn should_quit(&self) -> bool {
        self.should_quit
    }

    #[inline]
    fn request_quit(&mut self) {
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        let mut events = ApplicationEventList::new();

        // Need a local borrow because closure requires exclusive access to self.
        let mut window_mgr = self.window_manager.clone();

        window_mgr.poll_events(|event_loop, event| {
            self.handle_window_event(&mut events, event_loop, event);
        });

        // Clamp cursor to window bounds.
        if self.confine_cursor {
            self.confine_cursor_to_window();
        }

        events
    }

    #[inline]
    fn present(&mut self) {
        self.window_manager.present();
    }

    #[inline]
    fn window_size(&self) -> Size {
        let window = self.window_manager.window();
        match self.content_scale {
            ApplicationContentScale::System => {
                let logical = window.inner_size().to_logical::<f64>(window.scale_factor());
                Size::new(logical.width as i32, logical.height as i32)
            }
            ApplicationContentScale::Custom(scale) => {
                let phys = window.inner_size();
                Size::new((phys.width as f32 / scale) as i32, (phys.height as f32 / scale) as i32)
            }
        }
    }

    #[inline]
    fn framebuffer_size(&self) -> Size {
        let phys = self.window_manager.window().inner_size();
        Size::new(phys.width as i32, phys.height as i32)
    }

    #[inline]
    fn content_scale(&self) -> Vec2 {
        match self.content_scale {
            ApplicationContentScale::System => {
                let scale = self.window_manager.window().scale_factor() as f32;
                Vec2::new(scale, scale)
            }
            ApplicationContentScale::Custom(scale) => Vec2::new(scale, scale),
        }
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn select_fullscreen(event_loop: &ActiveEventLoop, window_mode: ApplicationWindowMode) -> Option<Fullscreen> {
    match window_mode {
        ApplicationWindowMode::FullScreen => {
            // Borderless fullscreen on the primary monitor.
            Some(Fullscreen::Borderless(event_loop.primary_monitor()))
        }
        ApplicationWindowMode::ExclusiveFullScreen => {
            // Attempt to select the best video mode on the primary monitor.
            let monitor = event_loop.primary_monitor()?;
            let video_mode = select_best_video_mode(monitor.video_modes())?;
            Some(Fullscreen::Exclusive(video_mode))
        }
        ApplicationWindowMode::Windowed => None,
    }
}

// Selects the best exclusive fullscreen video mode:
//  - Prefer highest pixel area;
//  - Prefer 60 Hz if available at that resolution;
//  - Otherwise prefer highest refresh rate.
fn select_best_video_mode<I>(modes: I) -> Option<VideoModeHandle>
where
    I: Iterator<Item = VideoModeHandle>,
{
    let all_modes: SmallVec<[VideoModeHandle; 16]> = modes.collect();
    if all_modes.is_empty() {
        return None;
    }

    let max_area = all_modes.iter().map(|m| m.size().width * m.size().height).max()?;

    let mut best: SmallVec<[&VideoModeHandle; 16]> =
        all_modes.iter().filter(|m| m.size().width * m.size().height == max_area).collect();

    if let Some(mode_60hz) = best.iter().find(|m| m.refresh_rate_millihertz() == 60_000) {
        return Some((*mode_60hz).clone());
    }

    best.sort_by_key(|m| m.refresh_rate_millihertz());
    best.last().map(|m| (*m).clone())
}
