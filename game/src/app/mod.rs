use smallvec::SmallVec;

use crate::{
    utils::{Size, Vec2}
};

use input::{
    InputAction,
    InputKey,
    InputModifiers,
    InputSystem,
    MouseButton
};

pub mod input;

// Internal implementation.
mod glfw;
pub use glfw::load_gl_func;

// ----------------------------------------------
// Application
// ----------------------------------------------

pub trait Application {
    fn should_quit(&self) -> bool;
    fn request_quit(&mut self);

    fn poll_events(&mut self) -> ApplicationEventList;
    fn present(&mut self);

    fn window_size(&self) -> Size;
    fn framebuffer_size(&self) -> Size;
    fn content_scale(&self) -> Vec2;

    type InputSystemType: InputSystem;
    fn create_input_system(&self) -> Self::InputSystemType;
}

// ----------------------------------------------
// ApplicationEvent
// ----------------------------------------------

#[derive(Debug)]
pub enum ApplicationEvent {
    Quit,
    WindowResize(Size),
    KeyInput(InputKey, InputAction, InputModifiers),
    CharInput(char),
    Scroll(Vec2),
    MouseButton(MouseButton, InputAction, InputModifiers),
}

type ApplicationEventList = SmallVec::<[ApplicationEvent; 16]>;

// ----------------------------------------------
// ApplicationBuilder
// ----------------------------------------------

pub struct ApplicationBuilder {
    title: String,
    window_size: Size,
    fullscreen: bool,
    confine_cursor: bool,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        ApplicationBuilder {
            title: String::default(),
            window_size: Size::new(1024, 768),
            fullscreen: false,
            confine_cursor: false,
        }
    }

    pub fn window_title(&mut self, title: &str) -> &mut Self {
        self.title = title.to_string();
        self
    }

    pub fn window_size(&mut self, size: Size) -> &mut Self {
        self.window_size = size;
        self
    }

    pub fn fullscreen(&mut self, fullscreen: bool) -> &mut Self {
        self.fullscreen = fullscreen;
        self
    }

    pub fn confine_cursor_to_window(&mut self, confine: bool) -> &mut Self {
        self.confine_cursor = confine;
        self
    }

    pub fn build<'a>(&self) -> impl Application + use<'a> {
        glfw::GlfwApplication::new(
            self.title.clone(),
            self.window_size,
            self.fullscreen,
            self.confine_cursor)
    }
}
