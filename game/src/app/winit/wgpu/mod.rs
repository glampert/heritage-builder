use std::{any::Any, sync::Arc};

#[cfg(feature = "web")]
pub mod wasm_runner;

#[cfg(feature = "desktop")]
use winit::platform::pump_events::EventLoopExtPumpEvents;

use winit::{
    application::ApplicationHandler,
    event::{MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

use super::{
    input::{
        WinitInputState, WinitInputSystem,
        winit_physical_key_to_input_key,
        winit_modifiers_to_input_modifiers,
        winit_mouse_button_to_mouse_button,
        winit_element_state_to_input_action,
    },
};
use crate::{
    log,
    utils::{Size, Vec2, mem::RcMut},
    app::{
        input::InputSystem,
        Application, ApplicationFactory,
        ApplicationEvent, ApplicationEventList,
        ApplicationWindowMode, ApplicationContentScale,
    },
};

pub mod window;
use window::WinitWindowManager;

// ----------------------------------------------
// WinitApplication
// ----------------------------------------------

pub struct WinitApplication {
    should_quit: bool,
    window_manager: WinitWindowManager,
    input_state: RcMut<WinitInputState>,
    input_system: WinitInputSystem,
    resizable: bool,

    #[cfg(feature = "desktop")]
    event_loop: EventLoop<()>,

    // On WASM, events are accumulated by the browser event loop handler
    // and drained here by poll_events().
    #[cfg(feature = "web")]
    pending_events: std::cell::RefCell<ApplicationEventList>,
}

impl WinitApplication {
    // Expose the Arc<Window> for the wgpu RenderSystem to create a surface.
    pub fn window_arc(&self) -> Arc<winit::window::Window> {
        self.window_manager.window_arc()
    }

    // Create a WinitApplication from a pre-created window manager (used on web/WASM).
    #[cfg(feature = "web")]
    pub fn from_window_manager(window_manager: WinitWindowManager, resizable: bool) -> Self {
        let input_state = RcMut::new(WinitInputState::new());
        let input_system = WinitInputSystem::new(input_state.clone().into_not_mut());

        log::info!(log::channel!("app"), "WinitApplication initialized (WASM).");

        Self {
            should_quit: false,
            window_manager,
            input_state,
            input_system,
            resizable,
            pending_events: std::cell::RefCell::new(ApplicationEventList::new()),
        }
    }

    // Push an event from the WASM event loop handler.
    #[cfg(feature = "web")]
    pub fn push_event(&self, event: ApplicationEvent) {
        self.pending_events.borrow_mut().push(event);
    }

    // Access the input state for updating from the WASM event loop handler.
    #[cfg(feature = "web")]
    pub fn input_state_mut(&self) -> &mut WinitInputState {
        &mut self.input_state
    }
}

#[cfg(feature = "desktop")]
impl ApplicationFactory for WinitApplication {
    fn new(
        window_title: &str,
        window_size: Size,
        window_mode: ApplicationWindowMode,
        resizable: bool,
        confine_cursor: bool,
        content_scale: ApplicationContentScale,
    ) -> Self {
        let mut event_loop = EventLoop::new()
            .expect("Failed to create winit event loop!");

        let mut init = WinitInitHandler::new(
            window_title,
            window_size,
            window_mode,
            resizable,
            confine_cursor,
            content_scale,
        );

        event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut init);

        let window_manager = init.result
            .expect("WinitApplication: window init failed — resumed() was not triggered");

        log::info!(log::channel!("app"), "WinitApplication initialized.");

        let input_state = RcMut::new(WinitInputState::new());
        let input_system = WinitInputSystem::new(input_state.clone().into_not_mut());

        Self {
            should_quit: false,
            window_manager,
            input_state,
            input_system,
            resizable,
            event_loop,
        }
    }
}

impl Application for WinitApplication {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn request_quit(&mut self) {
        self.should_quit = true;
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        #[cfg(feature = "desktop")]
        {
            let mut events = ApplicationEventList::new();

            {
                let window_id = self.window_manager.window.id();

                let WinitApplication {
                    should_quit,
                    window_manager,
                    input_state,
                    resizable,
                    event_loop,
                    ..
                } = self;

                let mut pump = WinitEventPump {
                    events: &mut events,
                    should_quit,
                    window_manager,
                    input_state: &mut *input_state,
                    window_id,
                    resizable: *resizable,
                };

                let _ = event_loop.pump_app_events(Some(std::time::Duration::ZERO), &mut pump);
            }

            if let Some(clamped) = self.window_manager.try_confine_cursor(self.input_state.cursor_pos) {
                self.input_state.cursor_pos = clamped;
            }

            events
        }

        #[cfg(feature = "web")]
        {
            // On WASM, events were accumulated by the browser event loop handler.
            // Drain them here.
            let mut events = self.pending_events.borrow_mut();
            let drained = events.clone();
            events.clear();
            drained
        }
    }

    #[inline]
    fn present(&mut self) {
        // No-op for wgpu: surface presentation is handled by the RenderSystem.
    }

    #[inline]
    fn window_size(&self) -> Size {
        self.window_manager.window_size()
    }

    #[inline]
    fn framebuffer_size(&self) -> Size {
        self.window_manager.framebuffer_size()
    }

    #[inline]
    fn content_scale(&self) -> Vec2 {
        self.window_manager.content_scale()
    }

    #[inline]
    fn input_system(&self) -> &dyn InputSystem {
        &self.input_system
    }

    fn app_context(&self) -> Option<&dyn Any> {
        Some(&self.window_manager.window)
    }
}

// ----------------------------------------------
// WinitEventPump (desktop only — WASM uses the
// browser event loop via wasm_runner)
// ----------------------------------------------

#[cfg(feature = "desktop")]
struct WinitEventPump<'a> {
    events: &'a mut ApplicationEventList,
    should_quit: &'a mut bool,
    window_manager: &'a mut WinitWindowManager,
    input_state: &'a mut WinitInputState,
    window_id: WindowId,
    resizable: bool,
}

#[cfg(feature = "desktop")]
impl ApplicationHandler for WinitEventPump<'_> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if window_id != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                *self.should_quit = true;
                self.events.push(ApplicationEvent::Quit);
                event_loop.exit();
            }
            WindowEvent::Resized(_phys_size) if self.resizable => {
                // No GL surface resize needed; the wgpu RenderSystem
                // reconfigures its surface in set_framebuffer_size().
                let window_size = self.window_manager.window_size();
                let framebuffer_size = self.window_manager.framebuffer_size();
                self.events.push(ApplicationEvent::WindowResize {
                    window_size,
                    framebuffer_size,
                });
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let key = winit_physical_key_to_input_key(key_event.physical_key);
                let action = winit_element_state_to_input_action(key_event.state, key_event.repeat);
                let modifiers = self.input_state.modifiers;

                self.input_state.set_key(key, key_event.state.is_pressed());
                self.events.push(ApplicationEvent::KeyInput(key, action, modifiers));

                if let Some(text) = key_event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        self.events.push(ApplicationEvent::CharInput(c));
                    }
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.input_state.modifiers = winit_modifiers_to_input_modifiers(new_modifiers.state());
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(x, y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        Vec2::new(pos.x as f32 / 20.0, pos.y as f32 / 20.0)
                    }
                };
                self.events.push(ApplicationEvent::Scroll(scroll));
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if let Some(mb) = winit_mouse_button_to_mouse_button(button) {
                    let action = winit_element_state_to_input_action(state, false);
                    let modifiers = self.input_state.modifiers;
                    self.input_state.set_mouse_button(mb, state.is_pressed());
                    self.events.push(ApplicationEvent::MouseButton(mb, action, modifiers));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let pos = self.input_state.cursor_pos;
                self.window_manager.warp_cursor_to_pos(pos);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window_manager.content_scale();
                self.input_state.cursor_pos = Vec2::new(
                    position.x as f32 / scale.x,
                    position.y as f32 / scale.y,
                );
            }
            _ => {}
        }
    }
}

// ----------------------------------------------
// WinitInitHandler (desktop only — on WASM the
// window is created inside the wasm_runner)
// ----------------------------------------------

#[cfg(feature = "desktop")]
struct WinitInitHandler<'a> {
    window_title: &'a str,
    window_size: Size,
    window_mode: ApplicationWindowMode,
    resizable: bool,
    confine_cursor: bool,
    content_scale: ApplicationContentScale,
    result: Option<WinitWindowManager>,
}

#[cfg(feature = "desktop")]
impl<'a> WinitInitHandler<'a> {
    fn new(
        window_title: &'a str,
        window_size: Size,
        window_mode: ApplicationWindowMode,
        resizable: bool,
        confine_cursor: bool,
        content_scale: ApplicationContentScale,
    ) -> Self {
        Self {
            window_title,
            window_size,
            window_mode,
            resizable,
            confine_cursor,
            content_scale,
            result: None,
        }
    }
}

#[cfg(feature = "desktop")]
impl ApplicationHandler for WinitInitHandler<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return;
        }

        let manager = WinitWindowManager::create(
            event_loop,
            self.window_title,
            self.window_size,
            self.window_mode,
            self.resizable,
            self.confine_cursor,
            self.content_scale,
        );

        self.result = Some(manager);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {}
}
