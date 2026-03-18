use std::any::Any;
use std::time::Duration;

use winit::{
    application::ApplicationHandler,
    event::{MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    platform::pump_events::EventLoopExtPumpEvents,
    window::WindowId,
};

use super::{
    input::InputSystem,
    Application, ApplicationFactory,
    ApplicationEvent, ApplicationEventList,
    ApplicationWindowMode, ApplicationContentScale,
};
use crate::{
    log,
    utils::{Size, Vec2, mem::RcMut},
};

mod window;
use window::WinitWindowManager;

pub mod input;
use input::{
    WinitInputState, WinitInputSystem,
    winit_physical_key_to_input_key,
    winit_modifiers_to_input_modifiers,
    winit_mouse_button_to_mouse_button,
    winit_element_state_to_input_action,
};

// ----------------------------------------------
// WinitApplication
// ----------------------------------------------

pub struct WinitApplication {
    should_quit: bool,
    event_loop: EventLoop<()>,
    window_manager: WinitWindowManager,
    input_state: RcMut<WinitInputState>,
    input_system: WinitInputSystem,
    resizable: bool,
}

impl ApplicationFactory for WinitApplication {
    fn new(window_title: &str,
           window_size: Size,
           window_mode: ApplicationWindowMode,
           resizable: bool,
           confine_cursor: bool,
           content_scale: ApplicationContentScale) -> Self
    {
        let mut event_loop = EventLoop::new()
            .expect("Failed to create winit event loop!");

        // Create the window and GL context during the first pump (triggers `resumed()`).
        let mut init = WinitInitHandler::new(
            window_title,
            window_size,
            window_mode,
            resizable,
            confine_cursor,
            content_scale
        );

        // Pump once to trigger `resumed()`, which creates the window + GL context.
        event_loop.pump_app_events(Some(Duration::ZERO), &mut init);

        let window_manager = init.result
            .expect("WinitApplication: window initialization failed — resumed() was not triggered");

        log::info!(log::channel!("app"), "WinitApplication initialized.");

        let input_state = RcMut::new(WinitInputState::new());
        let input_system = WinitInputSystem::new(input_state.clone().into_not_mut());

        Self {
            should_quit: false,
            event_loop,
            window_manager,
            input_state,
            input_system,
            resizable,
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
        let mut events = ApplicationEventList::new();

        {
            let window_id = self.window_manager.window.id();

            // Split borrows: event_loop is polled, window_manager / input_state
            // are borrowed by the handler. These are separate fields so the
            // borrow checker allows it.
            let WinitApplication {
                event_loop,
                should_quit,
                window_manager,
                input_state,
                resizable,
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

            let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut pump);
        }

        // Clamp cursor to window bounds.
        if let Some(clamped) = self.window_manager.try_confine_cursor(self.input_state.cursor_pos) {
            self.input_state.cursor_pos = clamped;
        }

        events
    }

    #[inline]
    fn present(&mut self) {
        use glutin::surface::GlSurface;
        self.window_manager.gl_surface
            .swap_buffers(&self.window_manager.gl_context)
            .expect("Failed to swap GL buffers!");
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
}

// ----------------------------------------------
// WinitEventPump — pumps a single frame of events
// ----------------------------------------------

struct WinitEventPump<'a> {
    events: &'a mut ApplicationEventList,
    should_quit: &'a mut bool,
    window_manager: &'a mut WinitWindowManager,
    input_state: &'a mut WinitInputState,
    window_id: WindowId,
    resizable: bool,
}

impl ApplicationHandler for WinitEventPump<'_> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Window is already created; nothing to do during normal polling.
    }

    fn window_event(&mut self,
                    event_loop: &ActiveEventLoop,
                    window_id: WindowId,
                    event: WindowEvent)
    {
        if window_id != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                *self.should_quit = true;
                self.events.push(ApplicationEvent::Quit);
                event_loop.exit();
            }
            WindowEvent::Resized(phys_size) if self.resizable => {
                self.window_manager.resize_surface(phys_size.width, phys_size.height);

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

                // Emit CharInput for printable characters on key press/repeat.
                // `text` is set by winit for keys that produce a character.
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
                    MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x, y)
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        // Convert pixel delta to approximate line counts.
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
                // When confinement is enabled: the title bar is outside the content
                // view, so winit stops sending CursorMoved once the cursor enters it
                // and clamping never triggers. Warp back to the last known in-bounds
                // position the moment CursorLeft fires.
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
            _ => {} // Unhandled event.
        }
    }
}

// ----------------------------------------------
// WinitInitHandler — creates window + GL context in resumed()
// ----------------------------------------------

struct WinitInitHandler<'a> {
    window_title: &'a str,
    window_size: Size,
    window_mode: ApplicationWindowMode,
    resizable: bool,
    confine_cursor: bool,
    content_scale: ApplicationContentScale,
    result: Option<WinitWindowManager>,
}

impl<'a> WinitInitHandler<'a> {
    fn new(window_title: &'a str,
           window_size: Size,
           window_mode: ApplicationWindowMode,
           resizable: bool,
           confine_cursor: bool,
           content_scale: ApplicationContentScale) -> Self
    {
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

impl ApplicationHandler for WinitInitHandler<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return; // Already initialized (e.g. app resumed after suspend on mobile).
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

    fn window_event(&mut self,
                    _event_loop: &ActiveEventLoop,
                    _window_id: WindowId,
                    _event: WindowEvent) {}
}
