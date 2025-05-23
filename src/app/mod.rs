pub mod input;

// Internal implementation.
mod glfw;
use glfw::GlfwApplication;
pub use glfw::load_gl_func;

use smallvec::SmallVec;

use crate::{
    utils::{Size2D, Vec2}
};

use input::{
    InputAction, InputKey, InputModifiers, InputSystem, MouseButton
};

// ----------------------------------------------
// Application
// ----------------------------------------------

pub trait Application {
    fn should_quit(&self) -> bool;
    fn request_quit(&mut self);

    fn poll_events(&mut self) -> ApplicationEventList;
    fn present(&mut self);

    fn window_size(&self) -> Size2D;
    fn framebuffer_size(&self) -> Size2D;
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
    WindowResize(Size2D),
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
    window_size: Size2D,
    fullscreen: bool,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        ApplicationBuilder {
            title: String::default(),
            window_size: Size2D::new(1024, 768),
            fullscreen: false,
        }
    }

    pub fn window_title(&mut self, title: &str) -> &mut Self {
        self.title = title.to_string();
        self
    }

    pub fn window_size(&mut self, size: Size2D) -> &mut Self {
        self.window_size = size;
        self
    }

    pub fn fullscreen(&mut self, fullscreen: bool) -> &mut Self {
        self.fullscreen = fullscreen;
        self
    }

    pub fn build<'a>(&self) -> impl Application + use<'a> {
        GlfwApplication::new(
            self.title.clone(),
            self.window_size,
            self.fullscreen)
    }
}
