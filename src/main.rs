#![allow(dead_code)]

mod utils;
mod app;
mod render;
mod ui;

use std::time::{Duration, Instant};
use utils::{Color, Point2D, Size2D, Rect2D, RectTexCoords};
use app::{Application, ApplicationBuilder, ApplicationEvent};
use render::RenderBackend;
use render::{TextureCache, TextureHandle};
use render::tile_sets::TileSets;
use render::tile_map::{TileMap, TileMapRenderer, TileMapRenderFlags};
use ui::backend::UiBackend;

// ----------------------------------------------
// TestUiState
// ----------------------------------------------

pub struct TestUiState {
    radio_btn_state: bool,
    checkbox_state: bool,
    input_int_state: i32,
    input_text_state: String,
}

impl TestUiState {
    pub fn new() -> Self {
        Self {
            radio_btn_state: false,
            checkbox_state: false,
            input_int_state: 0,
            input_text_state: String::new(),
        }
    }

    pub fn draw(&mut self, ui: &mut imgui::Ui) {
        ui.window("Hello world")
            .size([200.0, 400.0], imgui::Condition::FirstUseEver)
            .position([10.0, 10.0], imgui::Condition::FirstUseEver)
            .build(|| {
                ui.text("Hello world!");
                ui.text("This...is...imgui-rs!");
                ui.separator();

                ui.radio_button("Radio button", &mut self.radio_btn_state, true);
                ui.checkbox("Checkbox", &mut self.checkbox_state);
                ui.button("Button");
                ui.input_int("Input int", &mut self.input_int_state).build();
                ui.input_text("Input text", &mut self.input_text_state).build();

                ui.text(format!(
                    "Mouse Position: ({:.1},{:.1})",
                    ui.io().mouse_pos[0], ui.io().mouse_pos[1]
                ));
            });
    }
}

// ----------------------------------------------
// FrameTime
// ----------------------------------------------

pub struct FrameTime {
    last_frame_time: Instant,
    delta_time: Duration,
}

impl FrameTime {
    pub fn new() -> Self {
        Self {
            last_frame_time: Instant::now(),
            delta_time: Duration::new(0, 0),
        }
    }

    #[inline]
    pub fn begin(&self) {}

    #[inline]
    pub fn end(&mut self) {
        let time_now = std::time::Instant::now();
        self.delta_time = time_now - self.last_frame_time;
        self.last_frame_time = time_now;
    }

    #[inline]
    pub fn delta_time(&self) -> Duration {
        self.delta_time
    }
}

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let cwd = std::env::current_dir().unwrap();
    println!("The current directory is \"{}\".", cwd.display());

    let mut app_builder = ApplicationBuilder::new();

    let mut app = app_builder
        .window_title("CitySim")
        .window_size(Size2D { width: 1024, height: 768 })
        .fullscreen(false)
        .build();

    let mut render_backend = RenderBackend::new(app.window_size());
    let mut ui_backend = UiBackend::new(app.as_mut());
    let mut tex_cache = TextureCache::new(128);

    let mut tile_sets = TileSets::new_with_test_tiles(&mut tex_cache);
    let tile_map = TileMap::new_test_map(&mut tile_sets);

    let mut tile_map_renderer = TileMapRenderer::new();
    tile_map_renderer
        .set_draw_scaling(2)
        .set_draw_offset(Point2D { x: 448, y: 128 })
        .set_grid_line_thickness(3.0)
        .set_tile_spacing(4);

    let mut test_ui = TestUiState::new();

    let mut frame_time = FrameTime::new();

    while !app.should_quit() {
        frame_time.begin();

        let events = app.poll_events();
        for event in events {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_backend.set_window_size(window_size);
                }
                ApplicationEvent::KeyInput(key, action, _modifiers) => {
                    ui_backend.on_key_input(key, action);
                }
                ApplicationEvent::CharInput(c) => {
                    ui_backend.on_char_input(c);
                }
                ApplicationEvent::Scroll(amount) => {
                    ui_backend.on_scroll(amount);
                }
            }
            println!("ApplicationEvent::{:?}", event);
        }

        let ui = ui_backend.begin_frame(app.as_ref(), frame_time.delta_time());

        render_backend.begin_frame();

        tile_map_renderer.draw_map(
            &mut render_backend,
            &tile_map,
            TileMapRenderFlags::DrawTerrain |
            TileMapRenderFlags::DrawBuildings |
            TileMapRenderFlags::DrawUnits |
            TileMapRenderFlags::DrawGrid);

        // Screen origin marker.
        render_backend.draw_textured_colored_rect(
            Rect2D::with_xy_and_size(0, 0, Size2D { width: 20, height: 20 }), 
            &RectTexCoords::default(), TextureHandle::invalid(), Color::white());

        test_ui.draw(ui);

        render_backend.end_frame(&tex_cache);

        ui_backend.end_frame();

        app.present();

        frame_time.end();
    }
}
