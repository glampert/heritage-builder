use std::any::Any;
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
pub mod backend {
    use super::*;
    pub type GlfwApplication = glfw::GlfwApplication;
    pub type GlfwInputSystem = glfw::GlfwInputSystem;
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
    fn new(title: &str,
           window_size: Size,
           fullscreen: bool,
           confine_cursor: bool) -> Self;
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

pub type ApplicationEventList = SmallVec::<[ApplicationEvent; 16]>;

// ----------------------------------------------
// ApplicationBuilder
// ----------------------------------------------

pub struct ApplicationBuilder<'a> {
    title: &'a str,
    window_size: Size,
    fullscreen: bool,
    confine_cursor: bool,
}

impl<'a> ApplicationBuilder<'a> {
    pub fn new() -> Self {
        Self {
            title: "",
            window_size: Size::new(1024, 768),
            fullscreen: false,
            confine_cursor: false,
        }
    }

    pub fn window_title(&mut self, title: &'a str) -> &mut Self {
        self.title = title;
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

    pub fn build<AppBackendImpl>(&self) -> Box<AppBackendImpl>
        where AppBackendImpl: Application + ApplicationFactory + 'static
    {
        Box::new(AppBackendImpl::new(
            self.title,
            self.window_size,
            self.fullscreen,
            self.confine_cursor))
    }
}
