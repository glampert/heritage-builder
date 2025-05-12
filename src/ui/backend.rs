use std::time::Duration;
use imgui::Context as ImGuiContext;
use imgui_opengl_renderer::Renderer as ImGuiRenderer;
use crate::utils::Vec2;
use crate::app::{self, Application, InputAction, InputKey, MouseButton};

pub struct UiBackend {
    imgui_ctx: ImGuiContext,
    imgui_renderer: ImGuiRenderer,
    frame_started: bool,
}

impl UiBackend {
    pub fn new(app: &mut impl Application) -> Self {
        let mut imgui_ctx = ImGuiContext::create();

        // 'None' disables automatic "imgui.ini" saving.
        imgui_ctx.set_ini_filename(None);
    
        // Load default font:
        const DEFAULT_FONT_SIZE: f32 = 13.0;
        imgui_ctx.fonts().add_font(
            &[imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    size_pixels: DEFAULT_FONT_SIZE,
                    ..imgui::FontConfig::default()
                }),
            }]);

        // Set up the OpenGL renderer:
        let imgui_renderer = ImGuiRenderer::new(&mut imgui_ctx,
            |func_name| {
                app::load_gl_func(app, func_name)
            });

        Self {
            imgui_ctx: imgui_ctx,
            imgui_renderer: imgui_renderer,
            frame_started: false,
        }
    }

    pub fn begin_frame(&mut self, app: &impl Application, delta_time: Duration) -> &mut imgui::Ui {
        debug_assert!(self.frame_started == false);
    
        let io = self.imgui_ctx.io_mut();
        io.update_delta_time(delta_time);

        let fb_size = app.framebuffer_size();
        let content_scale = app.content_scale();

        io.display_size = [(fb_size.width as f32) / content_scale.x, (fb_size.height as f32) / content_scale.y];
        io.display_framebuffer_scale = [content_scale.x, content_scale.y];

        // Send mouse/keyboard input to ImGui. The rest is handled by application events.
        self.update_input(app);

        // Start new ImGui frame. Use the returned `ui` object to build the UI windows.
        let ui = self.imgui_ctx.new_frame();
        self.frame_started = true;

        ui
    }

    pub fn end_frame(&mut self) {
        debug_assert!(self.frame_started == true);

        let draw_data = self.imgui_ctx.render();

        if draw_data.total_idx_count != 0 && draw_data.total_vtx_count != 0 {
            self.imgui_renderer.render(&mut self.imgui_ctx);
        }

        self.frame_started = false;
    }

    pub fn on_key_input(&mut self, key: InputKey, action: InputAction) {
        let io = self.imgui_ctx.io_mut();
        let pressed = action != InputAction::Release;
        if let Some(imgui_key) = Self::app_input_key_to_imgui_key(key) {
            io.add_key_event(imgui_key, pressed);
        }
    }

    pub fn on_char_input(&mut self, c: char) {
        let io = self.imgui_ctx.io_mut();
        io.add_input_character(c);
    }

    pub fn on_scroll(&mut self, amount: Vec2) {
        let io = self.imgui_ctx.io_mut();
        io.mouse_wheel_h += amount.x;
        io.mouse_wheel += amount.y;
    }

    fn update_input(&mut self, app: &impl Application) {
        let io = self.imgui_ctx.io_mut();

        let cursor = app.cursor_pos();
        io.mouse_pos = [cursor.x as f32, cursor.y as f32];
    
        io.mouse_down[0] = app.button_state(MouseButton::Left)   == InputAction::Press;
        io.mouse_down[1] = app.button_state(MouseButton::Right)  == InputAction::Press;
        io.mouse_down[2] = app.button_state(MouseButton::Middle) == InputAction::Press;
    
        io.key_shift = app.key_state(InputKey::LeftShift)  == InputAction::Press
                    || app.key_state(InputKey::RightShift) == InputAction::Press;

        io.key_ctrl = app.key_state(InputKey::LeftControl)  == InputAction::Press
                   || app.key_state(InputKey::RightControl) == InputAction::Press;

        io.key_alt = app.key_state(InputKey::LeftAlt)  == InputAction::Press
                  || app.key_state(InputKey::RightAlt) == InputAction::Press;

        io.key_super = app.key_state(InputKey::LeftSuper)  == InputAction::Press
                    || app.key_state(InputKey::RightSuper) == InputAction::Press;
    }

    // Converts our InputKey to the corresponding ImGui key, if available.
    fn app_input_key_to_imgui_key(input_key: InputKey) -> Option<imgui::Key> {
        Some(match input_key {
            InputKey::Tab       => imgui::Key::Tab,
            InputKey::Left      => imgui::Key::LeftArrow,
            InputKey::Right     => imgui::Key::RightArrow,
            InputKey::Up        => imgui::Key::UpArrow,
            InputKey::Down      => imgui::Key::DownArrow,
            InputKey::PageUp    => imgui::Key::PageUp,
            InputKey::PageDown  => imgui::Key::PageDown,
            InputKey::Home      => imgui::Key::Home,
            InputKey::End       => imgui::Key::End,
            InputKey::Insert    => imgui::Key::Insert,
            InputKey::Delete    => imgui::Key::Delete,
            InputKey::Backspace => imgui::Key::Backspace,
            InputKey::Space     => imgui::Key::Space,
            InputKey::Enter     => imgui::Key::Enter,
            InputKey::Escape    => imgui::Key::Escape,
    
            // We only need to map these for the internal ImGui key combos (CTRL+C, CTRV+V, etc).
            InputKey::A => imgui::Key::A,
            InputKey::C => imgui::Key::C,
            InputKey::V => imgui::Key::V,
            InputKey::X => imgui::Key::X,
            InputKey::Y => imgui::Key::Y,
            InputKey::Z => imgui::Key::Z,
    
            InputKey::LeftShift   | InputKey::RightShift   => imgui::Key::ModShift,
            InputKey::LeftControl | InputKey::RightControl => imgui::Key::ModCtrl,
            InputKey::LeftAlt     | InputKey::RightAlt     => imgui::Key::ModAlt,
            InputKey::LeftSuper   | InputKey::RightSuper   => imgui::Key::ModSuper,
    
            _ => return None, // Key not used by ImGui.
        })
    }
}
