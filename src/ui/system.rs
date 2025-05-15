use std::ptr::null;
use std::time::{self};
use imgui::Context as ImGuiContext;
use imgui_opengl_renderer::Renderer as ImGuiRenderer;
use crate::utils::{self, Vec2};
use crate::app::{self, Application};
use crate::app::input::{InputAction, InputKey, InputSystem, MouseButton};

// ----------------------------------------------
// UiSystem
// ----------------------------------------------

pub struct UiSystem {
    context: UiContext,
    builder_ptr: *const imgui::Ui,
}

impl UiSystem {
    pub fn new(app: &impl Application) -> Self {
        Self {
            context: UiContext::new(app),
            builder_ptr: null::<imgui::Ui>(),
        }
    }

    pub fn begin_frame(&mut self, app: &impl Application, input_sys: &impl InputSystem, delta_time: time::Duration) {
        let ui_builder = self.context.begin_frame(app, input_sys, delta_time);
        self.builder_ptr = ui_builder as *const imgui::Ui;
    }

    pub fn end_frame(&mut self) {
        self.builder_ptr = null::<imgui::Ui>();
        self.context.end_frame();
    }

    pub fn on_key_input(&mut self, key: InputKey, action: InputAction) {
        self.context.on_key_input(key, action);
    }

    pub fn on_char_input(&mut self, c: char) {
        self.context.on_char_input(c);
    }

    pub fn on_scroll(&mut self, amount: Vec2) {
        self.context.on_scroll(amount);
    }

    pub fn fonts(&self) -> &FontIds {
        self.context.fonts()
    }

    pub fn context(&self) -> &UiContext {
        &self.context
    }

    pub fn builder(&self) -> &imgui::Ui {
        debug_assert!(self.builder_ptr.is_null() == false);
        unsafe{ &*self.builder_ptr }
    }

    // Show a small debug overlay under the cursor with its current position.
    pub fn draw_debug_cursor_overlay(&self) {
        let ui = self.builder();
        let cursor_pos = ui.io().mouse_pos;

        // Make the window background transparent and remove decorations.
        let window_flags =
            imgui::WindowFlags::NO_DECORATION |
            imgui::WindowFlags::NO_MOVE |
            imgui::WindowFlags::NO_SAVED_SETTINGS |
            imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
            imgui::WindowFlags::NO_NAV |
            imgui::WindowFlags::NO_MOUSE_INPUTS;

        // Draw a tiny window near the cursor
        ui.window("Cursor Debug")
            .position([cursor_pos[0] + 10.0, cursor_pos[1] + 10.0], imgui::Condition::Always)
            .flags(window_flags)
            .always_auto_resize(true)
            .bg_alpha(0.6) // Semi-transparent
            .build(|| {
                ui.text(format!("({},{})", cursor_pos[0], cursor_pos[1]));
            });
    }
}

// ----------------------------------------------
// FontIds
// ----------------------------------------------

pub struct FontIds {
    pub normal: imgui::FontId,
    pub small: imgui::FontId,
}

// ----------------------------------------------
// UiContext
// ----------------------------------------------

pub struct UiContext {
    imgui_ctx: ImGuiContext,
    imgui_renderer: ImGuiRenderer,
    fonts_ids: FontIds,
    frame_started: bool,
}

impl UiContext {
    pub fn new(app: &impl Application) -> Self {
        let mut imgui_ctx = ImGuiContext::create();

        // 'None' disables automatic "imgui.ini" saving.
        imgui_ctx.set_ini_filename(None);
    
        // Load default fonts:
        const NORMAL_FONT_SIZE: f32 = 13.0;
        let font_normal = imgui_ctx.fonts().add_font(
            &[imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    size_pixels: NORMAL_FONT_SIZE,
                    ..imgui::FontConfig::default()
                }),
            }]);

        const SMALL_FONT_SIZE: f32 = 10.0;
        let font_small = imgui_ctx.fonts().add_font(
            &[imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    size_pixels: SMALL_FONT_SIZE,
                    ..imgui::FontConfig::default()
                }),
            }]);

        // On MacOS this generates a lot of TTY spam about missing
        // OpenGL functions that we don't need or care about. This
        // is a hack to stop the TTY spamming but still keep a record
        // of the errors if ever required for inspection.
        let imgui_renderer = utils::macos_redirect_stderr(|| {
            // Set up the OpenGL renderer:
            let imgui_renderer = ImGuiRenderer::new(&mut imgui_ctx,
                |func_name| {
                    app::load_gl_func(app, func_name)
                });
            imgui_renderer
        }, "stderr_gl_load_imgui.log");

        Self {
            imgui_ctx: imgui_ctx,
            imgui_renderer: imgui_renderer,
            fonts_ids: FontIds { normal: font_normal, small: font_small },
            frame_started: false,
        }
    }

    pub fn begin_frame(&mut self, app: &impl Application, input_sys: &impl InputSystem, delta_time: time::Duration) -> &imgui::Ui {
        debug_assert!(self.frame_started == false);
    
        let io = self.imgui_ctx.io_mut();
        io.update_delta_time(delta_time);

        let fb_size = app.framebuffer_size();
        let content_scale = app.content_scale();

        io.display_size = [(fb_size.width as f32) / content_scale.x, (fb_size.height as f32) / content_scale.y];
        io.display_framebuffer_scale = [content_scale.x, content_scale.y];

        // Send mouse/keyboard input to ImGui. The rest is handled by application events.
        self.update_input(input_sys);

        // Start new ImGui frame. Use the returned `ui` object to build the UI windows.
        let ui = self.imgui_ctx.new_frame();
        self.frame_started = true;

        ui
    }

    pub fn fonts(&self) -> &FontIds {
        &self.fonts_ids
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

    fn update_input(&mut self, input_sys: &impl InputSystem) {
        let io = self.imgui_ctx.io_mut();

        let cursor = input_sys.cursor_pos();
        io.mouse_pos = [cursor.x as f32, cursor.y as f32];
    
        io.mouse_down[0] = input_sys.mouse_button_state(MouseButton::Left)   == InputAction::Press;
        io.mouse_down[1] = input_sys.mouse_button_state(MouseButton::Right)  == InputAction::Press;
        io.mouse_down[2] = input_sys.mouse_button_state(MouseButton::Middle) == InputAction::Press;
    
        io.key_shift = input_sys.key_state(InputKey::LeftShift)  == InputAction::Press
                    || input_sys.key_state(InputKey::RightShift) == InputAction::Press;

        io.key_ctrl = input_sys.key_state(InputKey::LeftControl)  == InputAction::Press
                   || input_sys.key_state(InputKey::RightControl) == InputAction::Press;

        io.key_alt = input_sys.key_state(InputKey::LeftAlt)  == InputAction::Press
                  || input_sys.key_state(InputKey::RightAlt) == InputAction::Press;

        io.key_super = input_sys.key_state(InputKey::LeftSuper)  == InputAction::Press
                    || input_sys.key_state(InputKey::RightSuper) == InputAction::Press;
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
