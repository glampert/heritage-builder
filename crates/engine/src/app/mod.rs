use common::{Size, Vec2, mem::RcMut};
use enum_dispatch::enum_dispatch;
use input::{InputAction, InputKey, InputModifiers, InputSystem, MouseButton};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::Display;

use crate::render::RenderApi;

pub mod input;

// ----------------------------------------------
// Internal backend implementations
// ----------------------------------------------

mod platform;
pub(crate) mod winit;

#[cfg(feature = "desktop")]
mod glfw;

#[enum_dispatch]
enum ApplicationBackendImpl {
    Winit(winit::WinitApplicationBackend),

    #[cfg(feature = "desktop")]
    Glfw(glfw::GlfwApplicationBackend),
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum ApplicationApi {
    #[default]
    Winit,

    #[cfg(feature = "desktop")]
    Glfw,
}

// ----------------------------------------------
// ApplicationBackend
// ----------------------------------------------

#[enum_dispatch(ApplicationBackendImpl)]
trait ApplicationBackend: Sized {
    fn new_input_system(&mut self) -> InputSystem;
    fn app_context(&self) -> Option<&dyn std::any::Any>;

    fn should_quit(&self) -> bool;
    fn request_quit(&mut self);

    fn poll_events(&mut self) -> ApplicationEventList;
    fn present(&mut self);

    fn window_size(&self) -> Size;
    fn framebuffer_size(&self) -> Size;
    fn content_scale(&self) -> Vec2;
}

// ----------------------------------------------
// ApplicationInitParams
// ----------------------------------------------

pub struct ApplicationInitParams<'a> {
    pub app_api: ApplicationApi,
    pub render_api: RenderApi,
    pub window_title: &'a str,
    pub window_size: Size,
    pub window_mode: ApplicationWindowMode,
    pub content_scale: ApplicationContentScale,
    pub resizable_window: bool,
    pub confine_cursor: bool,

    // Optional pre-created Arc<winit::window::Window> for Web/WASM.
    #[cfg(feature = "web")]
    pub opt_window: Option<&'a dyn std::any::Any>,
}

impl Default for ApplicationInitParams<'_> {
    fn default() -> Self {
        Self {
            app_api: ApplicationApi::default(),
            render_api: RenderApi::default(),
            window_title: "Heritage Builder",
            window_size: Size::new(1024, 768),
            window_mode: ApplicationWindowMode::Windowed,
            content_scale: ApplicationContentScale::default(),
            resizable_window: false,
            confine_cursor: true,

            #[cfg(feature = "web")]
            opt_window: None,
        }
    }
}

// ----------------------------------------------
// Application
// ----------------------------------------------

pub struct Application {
    app_api: ApplicationApi,
    backend: ApplicationBackendImpl,
    input_system: InputSystem,
    pending_events: ApplicationEventList,
}

impl Application {
    pub fn new(params: ApplicationInitParams) -> RcMut<Self> {
        debug_assert!(params.window_size.is_valid());

        let mut backend = match params.app_api {
            ApplicationApi::Winit => ApplicationBackendImpl::from(winit::WinitApplicationBackend::new(&params)),

            #[cfg(feature = "desktop")]
            ApplicationApi::Glfw => ApplicationBackendImpl::from(glfw::GlfwApplicationBackend::new(&params)),
        };

        let input_system = backend.new_input_system();

        RcMut::new(Self { app_api: params.app_api, backend, input_system, pending_events: ApplicationEventList::new() })
    }

    // Push an event from an external source (e.g. WebRunner).
    #[inline]
    pub fn push_event(&mut self, event: ApplicationEvent) {
        self.pending_events.push(event);
    }

    #[inline]
    pub fn app_api(&self) -> ApplicationApi {
        self.app_api
    }

    // Optional context passed to the RenderSystem (e.g.: Arc<Window> for wgpu).
    #[inline]
    pub fn app_context(&self) -> Option<&dyn std::any::Any> {
        self.backend.app_context()
    }

    #[inline]
    pub fn should_quit(&self) -> bool {
        self.backend.should_quit()
    }

    #[inline]
    pub fn request_quit(&mut self) {
        self.backend.request_quit();
    }

    #[inline]
    pub fn poll_events(&mut self) -> ApplicationEventList {
        let mut events = std::mem::take(&mut self.pending_events);
        events.extend(self.backend.poll_events());
        events
    }

    #[inline]
    pub fn present(&mut self) {
        self.backend.present();
    }

    #[inline]
    pub fn window_size(&self) -> Size {
        self.backend.window_size()
    }

    #[inline]
    pub fn framebuffer_size(&self) -> Size {
        self.backend.framebuffer_size()
    }

    #[inline]
    pub fn content_scale(&self) -> Vec2 {
        self.backend.content_scale()
    }

    #[inline]
    pub fn input_system(&self) -> &InputSystem {
        &self.input_system
    }

    #[inline]
    pub(crate) fn input_system_mut(&mut self) -> &mut InputSystem {
        &mut self.input_system
    }
}

// ----------------------------------------------
// ApplicationEvent
// ----------------------------------------------

#[derive(Copy, Clone, Debug)]
pub enum ApplicationEvent {
    Quit,
    WindowResize { window_size: Size, framebuffer_size: Size },
    KeyInput(InputKey, InputAction, InputModifiers),
    CharInput(char),
    Scroll(Vec2),
    MouseButton(MouseButton, InputAction, InputModifiers),
}

pub type ApplicationEventList = SmallVec<[ApplicationEvent; 16]>;

// ----------------------------------------------
// ApplicationWindowMode
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationWindowMode {
    Windowed,

    // MacOS:
    //  - "Kiosk-style" fullscreen.
    //  - Similar behavior as if the user clicked the green fullscreen button in the window top bar.
    //  - We prevent the system menu and dock bar from showing when the cursor hovers the screen edges.
    //  - Desktop resolution and retina display DIP scale preserved. Cmd+Tab seamlessly switches between apps.
    //
    // Windows:
    //  - Same as ExclusiveFullScreen.
    FullScreen,

    // MacOS:
    //  - Exclusive fullscreen.
    //  - May change desktop resolution and retina display DIP settings.
    //  - Cmd+Tab still allows switching between apps, but desktop resolution may be disrupted.
    //
    // Windows:
    //  - Same as FullScreen. Changes display mode and acquires full window focus.
    ExclusiveFullScreen,
}

impl ApplicationWindowMode {
    pub fn is_windowed(self) -> bool {
        self == Self::Windowed
    }

    pub fn is_fullscreen(self) -> bool {
        self == Self::FullScreen || self == Self::ExclusiveFullScreen
    }

    pub fn is_exclusive_fullscreen(self) -> bool {
        self == Self::ExclusiveFullScreen
    }
}

// ----------------------------------------------
// ApplicationContentScale
// ----------------------------------------------

#[derive(Copy, Clone, Default, Display, Serialize, Deserialize)]
pub enum ApplicationContentScale {
    #[default]
    System,
    Custom(f32),
}
