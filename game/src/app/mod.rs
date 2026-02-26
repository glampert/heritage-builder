use std::any::Any;
use smallvec::SmallVec;
use strum_macros::Display;
use serde::{Deserialize, Serialize};

use crate::utils::{Size, Vec2};

use input::{InputAction, InputKey, InputModifiers, InputSystem, MouseButton};
pub mod input;

// Internal implementation.
mod glfw;
pub mod backend {
    use super::*;
    pub type GlfwApplication = glfw::GlfwApplication;
    pub type GlfwInputSystem = glfw::input::GlfwInputSystem;
}

// ----------------------------------------------
// Application / ApplicationFactory
// ----------------------------------------------

pub trait Application: Any {
    fn as_any(&self) -> &dyn Any;

    fn should_quit(&self) -> bool;
    fn request_quit(&mut self);

    fn poll_events(&mut self) -> ApplicationEventList;
    fn present(&mut self);

    fn window_size(&self) -> Size;
    fn framebuffer_size(&self) -> Size;
    fn content_scale(&self) -> Vec2;

    fn input_system(&self) -> &dyn InputSystem;
}

pub trait ApplicationFactory: Sized {
    fn new(window_title: &str,
           window_size: Size,
           window_mode: ApplicationWindowMode,
           resizable_window: bool,
           confine_cursor: bool) -> Self;
}

// ----------------------------------------------
// ApplicationEvent
// ----------------------------------------------

#[derive(Copy, Clone, Debug)]
pub enum ApplicationEvent {
    Quit,
    WindowResize(Size),
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
    pub fn is_fullscreen(self) -> bool {
        self == Self::FullScreen ||
        self == Self::ExclusiveFullScreen
    }
}

// ----------------------------------------------
// ApplicationBuilder
// ----------------------------------------------

pub struct ApplicationBuilder<'a> {
    window_title: &'a str,
    window_size: Size,
    window_mode: ApplicationWindowMode,
    resizable_window: bool,
    confine_cursor: bool,
}

impl<'a> ApplicationBuilder<'a> {
    pub fn new() -> Self {
        Self {
            window_title: "",
            window_size: Size::new(1024, 768),
            window_mode: ApplicationWindowMode::Windowed,
            resizable_window: false,
            confine_cursor: false,
        }
    }

    pub fn window_title(&mut self, title: &'a str) -> &mut Self {
        self.window_title = title;
        self
    }

    pub fn window_size(&mut self, size: Size) -> &mut Self {
        self.window_size = size;
        self
    }

    pub fn window_mode(&mut self, mode: ApplicationWindowMode) -> &mut Self {
        self.window_mode = mode;
        self
    }

    pub fn resizable_window(&mut self, resizable: bool) -> &mut Self {
        self.resizable_window = resizable;
        self
    }

    pub fn confine_cursor_to_window(&mut self, confine: bool) -> &mut Self {
        self.confine_cursor = confine;
        self
    }

    pub fn build<AppBackendImpl>(&self) -> Box<AppBackendImpl>
        where AppBackendImpl: Application + ApplicationFactory + 'static
    {
        Box::new(AppBackendImpl::new(self.window_title,
                                     self.window_size,
                                     self.window_mode,
                                     self.resizable_window,
                                     self.confine_cursor))
    }
}
