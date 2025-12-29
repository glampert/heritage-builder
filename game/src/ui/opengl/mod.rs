use std::any::Any;

use imgui_opengl_renderer::Renderer as ImGuiOpenGlRenderer;

use super::{UiRenderer, UiRendererFactory};
use crate::{
    app::{self, Application},
    utils,
};

// ----------------------------------------------
// UiRendererOpenGl
// ----------------------------------------------

pub struct UiRendererOpenGl {
    backend: ImGuiOpenGlRenderer,
}

impl UiRendererFactory for UiRendererOpenGl {
    fn new(ctx: &mut imgui::Context, app: &impl Application) -> Self {
        let glfw_app = app.as_any()
                          .downcast_ref::<app::backend::GlfwApplication>()
                          .expect("ImGui OpenGL backend assumes GLFW App!");

        // On MacOS this generates a lot of TTY spam about missing
        // OpenGL functions that we don't need or care about. This
        // is a hack to stop the TTY spamming but still keep a record
        // of the errors if ever required for inspection.
        let backend = utils::platform::macos_redirect_stderr(|| {
            // Set up the OpenGL renderer:
            ImGuiOpenGlRenderer::new(ctx, |func_name| glfw_app.load_gl_func(func_name))
        },
        "stderr_gl_load_imgui.log");

        Self { backend }
    }
}

impl UiRenderer for UiRendererOpenGl {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn render(&self, ctx: &mut imgui::Context) {
        self.backend.render(ctx);
    }
}
