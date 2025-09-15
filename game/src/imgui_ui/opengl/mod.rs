use super::UiRenderer;
use crate::{
    app::{Application, ApplicationBackend},
    utils,
};

use imgui_opengl_renderer::Renderer as ImGuiOpenGlRenderer;

// ----------------------------------------------
// UiOpenGlRenderer
// ----------------------------------------------

pub struct UiOpenGlRenderer {
    backend: ImGuiOpenGlRenderer,
}

impl UiOpenGlRenderer {
    pub fn new(imgui_ctx: &mut imgui::Context, app: &dyn Application) -> Self {
        let glfw_app =
            app.as_any().downcast_ref::<ApplicationBackend>().expect("ImGui OpenGL backend assumes GLFW App!");

        // On MacOS this generates a lot of TTY spam about missing
        // OpenGL functions that we don't need or care about. This
        // is a hack to stop the TTY spamming but still keep a record
        // of the errors if ever required for inspection.
        let backend = utils::platform::macos_redirect_stderr(
            || {
                // Set up the OpenGL renderer:
                ImGuiOpenGlRenderer::new(imgui_ctx, |func_name| glfw_app.load_gl_func(func_name))
            },
            "stderr_gl_load_imgui.log",
        );

        Self { backend }
    }
}

impl UiRenderer for UiOpenGlRenderer {
    fn render(&self, imgui_ctx: &mut imgui::Context) {
        self.backend.render(imgui_ctx);
    }
}
