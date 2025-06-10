use std::ptr::null;
use std::ffi::c_void;
use std::time::{self};

use imgui::{
    Context as ImGuiContext,
    StyleColor,
    Style
};

use imgui_opengl_renderer::Renderer as ImGuiRenderer;

pub use imgui::FontId as UiFontHandle;
pub use imgui::TextureId as UiTextureHandle;

use crate::{
    utils::{self, Vec2},
    render::{TextureCache, TextureHandle},
    app::{self, Application, input::{InputAction, InputKey, InputModifiers, InputSystem, MouseButton}}
};

// ----------------------------------------------
// UiInputEvent
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UiInputEvent {
    Handled,   // Input event was handled/consumed and should not propagate.
    NotHandled // Input event wasn't handled and should propagate to other widgets.
}

impl UiInputEvent {
    #[inline]
    pub fn is_handled(self) -> bool {
        self == UiInputEvent::Handled
    }

    #[inline]
    pub fn not_handled(self) -> bool {
        self == UiInputEvent::NotHandled
    }
}

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

    #[inline]
    pub fn begin_frame(&mut self, app: &impl Application, input_sys: &impl InputSystem, delta_time: time::Duration) {
        debug_assert!(self.builder_ptr.is_null() == true);
        let ui_builder = self.context.begin_frame(app, input_sys, delta_time);
        self.builder_ptr = ui_builder as *const imgui::Ui;
    }

    #[inline]
    pub fn end_frame(&mut self) {
        debug_assert!(self.builder_ptr.is_null() == false);
        self.builder_ptr = null::<imgui::Ui>();
        self.context.end_frame();
    }

    #[inline]
    pub fn on_key_input(&mut self, key: InputKey, action: InputAction, _: InputModifiers) -> UiInputEvent {
        self.context.on_key_input(key, action);

        if self.is_handling_key_input() {
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    #[inline]
    pub fn on_char_input(&mut self, c: char) -> UiInputEvent {
        self.context.on_char_input(c);

        if self.is_handling_key_input() {
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    #[inline]
    pub fn on_scroll(&mut self, amount: Vec2) -> UiInputEvent {
        self.context.on_scroll(amount);

        if self.is_handling_mouse_input() {
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    #[inline]
    pub fn on_mouse_click(&mut self, _: MouseButton, _: InputAction, _: InputModifiers) -> UiInputEvent {
        // Mouse events are polled from the InputSystem instead;
        // Just perform a quick check to see if mouse clicks are being consumed by ImGui.
        if self.is_handling_mouse_input() {
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    #[inline]
    pub fn is_handling_mouse_input(&self) -> bool {
        self.context.imgui_ctx.io().want_capture_mouse
    }

    #[inline]
    pub fn is_handling_key_input(&self) -> bool {
        self.context.imgui_ctx.io().want_capture_keyboard
    }

    #[inline]
    pub fn fonts(&self) -> &UiFonts {
        self.context.fonts()
    }

    #[inline]
    pub fn context(&self) -> &UiContext {
        &self.context
    }

    #[inline]
    pub fn builder(&self) -> &imgui::Ui {
        debug_assert!(self.builder_ptr.is_null() == false);
        unsafe{ &*self.builder_ptr }
    }

    #[inline]
    pub fn to_ui_texture(&self, tex_cache: &TextureCache, tex_handle: TextureHandle) -> UiTextureHandle {
        let native_handle = tex_cache.to_native_handle(tex_handle);
        debug_assert!(std::mem::size_of_val(&native_handle) <= std::mem::size_of::<usize>());
        UiTextureHandle::from(native_handle as *mut c_void)
    }
}

// ----------------------------------------------
// UiFonts
// ----------------------------------------------

pub struct UiFonts {
    pub normal: UiFontHandle,
    pub small: UiFontHandle,
}

// ----------------------------------------------
// UiContext
// ----------------------------------------------

pub struct UiContext {
    imgui_ctx: ImGuiContext,
    imgui_renderer: ImGuiRenderer,
    fonts_ids: UiFonts,
    frame_started: bool,
}

impl UiContext {
    pub fn new(app: &impl Application) -> Self {
        let mut imgui_ctx = ImGuiContext::create();

        // 'None' disables automatic "imgui.ini" saving.
        imgui_ctx.set_ini_filename(None);

        Self::apply_custom_theme(imgui_ctx.style_mut());

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
        let imgui_renderer = utils::platform::macos_redirect_stderr(|| {
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
            fonts_ids: UiFonts { normal: font_normal, small: font_small },
            frame_started: false,
        }
    }

    pub fn begin_frame(&mut self, app: &impl Application, input_sys: &impl InputSystem, delta_time: time::Duration) -> &imgui::Ui {
        debug_assert!(self.frame_started == false);
    
        let io = self.imgui_ctx.io_mut();
        io.update_delta_time(delta_time);

        let fb_size = app.framebuffer_size().to_vec2();
        let content_scale = app.content_scale();

        io.display_size = [fb_size.x / content_scale.x, fb_size.y / content_scale.y];
        io.display_framebuffer_scale = [content_scale.x, content_scale.y];

        // Send mouse/keyboard input to ImGui. The rest is handled by application events.
        self.update_input(input_sys);

        // Start new ImGui frame. Use the returned `ui` object to build the UI windows.
        let ui = self.imgui_ctx.new_frame();
        self.frame_started = true;

        ui
    }

    pub fn fonts(&self) -> &UiFonts {
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

        let cursor_pos = input_sys.cursor_pos();
        io.mouse_pos = [cursor_pos.x, cursor_pos.y];
    
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

    fn apply_custom_theme(style: &mut Style) {
        let colors = &mut style.colors;

        colors[StyleColor::Text as usize] = [0.92, 0.93, 0.94, 1.0].into();
        colors[StyleColor::TextDisabled as usize] = [0.50, 0.52, 0.54, 1.0].into();
        colors[StyleColor::WindowBg as usize] = [0.14, 0.14, 0.16, 0.9].into();
        colors[StyleColor::ChildBg as usize] = [0.16, 0.16, 0.18, 0.9].into();
        colors[StyleColor::PopupBg as usize] = [0.18, 0.18, 0.20, 0.9].into();
        colors[StyleColor::Border as usize] = [0.28, 0.29, 0.30, 0.60].into();
        colors[StyleColor::BorderShadow as usize] = [0.00, 0.00, 0.00, 0.00].into();
        colors[StyleColor::FrameBg as usize] = [0.20, 0.22, 0.24, 1.0].into();
        colors[StyleColor::FrameBgHovered as usize] = [0.22, 0.24, 0.26, 1.0].into();
        colors[StyleColor::FrameBgActive as usize] = [0.24, 0.26, 0.28, 1.0].into();
        colors[StyleColor::TitleBg as usize] = [0.14, 0.14, 0.16, 1.0].into();
        colors[StyleColor::TitleBgActive as usize] = [0.16, 0.16, 0.18, 1.0].into();
        colors[StyleColor::TitleBgCollapsed as usize] = [0.14, 0.14, 0.16, 0.9].into();
        colors[StyleColor::MenuBarBg as usize] = [0.20, 0.20, 0.22, 1.0].into();
        colors[StyleColor::ScrollbarBg as usize] = [0.16, 0.16, 0.18, 1.0].into();

        let theme_color = [0.58, 0.45, 0.35, 1.0]; // Soft light brown

        colors[StyleColor::ScrollbarGrab as usize] = theme_color.into();
        colors[StyleColor::ScrollbarGrabHovered as usize] = [0.63, 0.50, 0.38, 1.0].into();
        colors[StyleColor::ScrollbarGrabActive as usize] = [0.68, 0.54, 0.42, 1.0].into();

        colors[StyleColor::CheckMark as usize] = theme_color.into();
        colors[StyleColor::SliderGrab as usize] = theme_color.into();
        colors[StyleColor::SliderGrabActive as usize] = [0.65, 0.50, 0.40, 1.0].into();

        colors[StyleColor::Button as usize] = theme_color.into();
        colors[StyleColor::ButtonHovered as usize] = [0.65, 0.50, 0.40, 1.0].into();
        colors[StyleColor::ButtonActive as usize] = [0.70, 0.55, 0.45, 1.0].into();

        colors[StyleColor::Header as usize] = theme_color.into();
        colors[StyleColor::HeaderHovered as usize] = [0.65, 0.50, 0.40, 1.0].into();
        colors[StyleColor::HeaderActive as usize] = [0.70, 0.55, 0.45, 1.0].into();

        colors[StyleColor::Separator as usize] = [0.28, 0.29, 0.30, 1.0].into();
        colors[StyleColor::SeparatorHovered as usize] = theme_color.into();
        colors[StyleColor::SeparatorActive as usize] = theme_color.into();

        colors[StyleColor::ResizeGrip as usize] = theme_color.into();
        colors[StyleColor::ResizeGripHovered as usize] = [0.65, 0.50, 0.40, 1.0].into();
        colors[StyleColor::ResizeGripActive as usize] = [0.70, 0.55, 0.45, 1.0].into();

        colors[StyleColor::Tab as usize] = [0.20, 0.22, 0.24, 1.0].into();
        colors[StyleColor::TabHovered as usize] = [0.65, 0.50, 0.40, 1.0].into();
        colors[StyleColor::TabActive as usize] = theme_color.into();
        colors[StyleColor::TabUnfocused as usize] = [0.20, 0.22, 0.24, 1.0].into();
        colors[StyleColor::TabUnfocusedActive as usize] = theme_color.into();

        colors[StyleColor::PlotLines as usize] = theme_color.into();
        colors[StyleColor::PlotLinesHovered as usize] = theme_color.into();
        colors[StyleColor::PlotHistogram as usize] = theme_color.into();
        colors[StyleColor::PlotHistogramHovered as usize] = [0.65, 0.50, 0.40, 1.0].into();

        colors[StyleColor::TableHeaderBg as usize] = [0.20, 0.22, 0.24, 1.0].into();
        colors[StyleColor::TableBorderStrong as usize] = [0.28, 0.29, 0.30, 1.0].into();
        colors[StyleColor::TableBorderLight as usize] = [0.24, 0.25, 0.26, 1.0].into();
        colors[StyleColor::TableRowBg as usize] = [0.20, 0.22, 0.24, 1.0].into();
        colors[StyleColor::TableRowBgAlt as usize] = [0.22, 0.24, 0.26, 1.0].into();

        colors[StyleColor::TextSelectedBg as usize] = [0.58, 0.45, 0.35, 0.35].into();
        colors[StyleColor::DragDropTarget as usize] = [0.58, 0.45, 0.35, 0.90].into();
        colors[StyleColor::NavHighlight as usize] = theme_color.into();
        colors[StyleColor::NavWindowingHighlight as usize] = [1.0, 1.0, 1.0, 0.70].into();
        colors[StyleColor::NavWindowingDimBg as usize] = [0.80, 0.80, 0.80, 0.20].into();
        colors[StyleColor::ModalWindowDimBg as usize] = [0.80, 0.80, 0.80, 0.35].into();

        style.window_padding = [8.0, 8.0].into();
        style.frame_padding = [5.0, 2.0].into();
        style.cell_padding = [6.0, 6.0].into();
        style.item_spacing = [6.0, 6.0].into();
        style.item_inner_spacing = [6.0, 6.0].into();
        style.touch_extra_padding = [0.0, 0.0].into();
        style.indent_spacing = 25.0;
        style.scrollbar_size = 11.0;
        style.grab_min_size = 10.0;
        style.window_border_size = 1.0;
        style.child_border_size = 1.0;
        style.popup_border_size = 1.0;
        style.frame_border_size = 1.0;
        style.tab_border_size = 1.0;
        style.window_rounding = 7.0;
        style.child_rounding = 4.0;
        style.frame_rounding = 3.0;
        style.popup_rounding = 4.0;
        style.scrollbar_rounding = 9.0;
        style.grab_rounding = 3.0;
        style.log_slider_deadzone = 4.0;
        style.tab_rounding = 4.0;
    }
}
