#![allow(dead_code)]

mod utils;
mod app;
mod render;
mod ui;

use std::time::{self};
use utils::*;
use app::*;
use app::input::*;
use ui::*;
use render::*;
use render::tile_sets::*;
use render::tile_map::*;

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

    pub fn draw(&mut self, ui_sys: &UiSystem, _delta_time: time::Duration) {
        let ui = ui_sys.builder();

        ui.window("CitySim")
            .size([250.0, 180.0], imgui::Condition::FirstUseEver)
            .position([10.0, 10.0], imgui::Condition::FirstUseEver)
            .build(|| {
                /*
                ui.text("CitySim!");
                ui.text_colored([0.0, 1.0, 1.0, 1.0], format!("dt: {}", delta_time.as_secs_f64()));
                ui.separator();

                ui.radio_button("Radio button", &mut self.radio_btn_state, true);
                ui.checkbox("Checkbox", &mut self.checkbox_state);
                ui.button("Button");
                ui.input_int("Input int", &mut self.input_int_state).build();
                ui.input_text("Input text", &mut self.input_text_state).build();
                */
            });
    }
}

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let cwd = std::env::current_dir().unwrap();
    println!("The current directory is \"{}\".", cwd.display());

    let mut app = ApplicationBuilder::new()
        .window_title("CitySim")
        .window_size(Size2D::new(1024, 768))
        .fullscreen(false)
        .build();

    let input_sys = app.create_input_system();

    let mut render_sys = RenderSystem::new(app.window_size());
    let mut ui_sys = UiSystem::new(&app);
    let mut tex_cache = TextureCache::new(128);

    let mut tile_sets = TileSets::new_with_test_tiles(&mut tex_cache);
    let mut tile_map = TileMap::new_test_map(&mut tile_sets);

    let mut tile_map_renderer = TileMapRenderer::new();
    tile_map_renderer
        .set_draw_scaling(2)
        .set_draw_offset(Point2D::new(448, 600))
        .set_grid_line_thickness(3.0)
        .set_tile_spacing(4);

    let mut tile_selection = TileSelection::new();
    let mut test_ui = TestUiState::new();

    let mut frame_clock = FrameClock::new();

    while !app.should_quit() {
        frame_clock.begin_frame();

        let transform = tile_map_renderer.world_to_screen_transform();
        let cursor_pos = input_sys.cursor_pos();
        let events = app.poll_events();

        for event in events {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_sys.set_window_size(window_size);
                }
                ApplicationEvent::KeyInput(key, action, _modifiers) => {
                    ui_sys.on_key_input(key, action);
                }
                ApplicationEvent::CharInput(c) => {
                    ui_sys.on_char_input(c);
                }
                ApplicationEvent::Scroll(amount) => {
                    ui_sys.on_scroll(amount);
                }
                ApplicationEvent::MouseButton(button, action, _modifiers) => {
                    let range_selecting = tile_selection.on_mouse_click(button, action, cursor_pos);
                    if !range_selecting {
                        tile_map.clear_selection(&mut tile_selection);
                    }
                }
            }
            println!("ApplicationEvent::{:?}", event);
        }

        tile_selection.update(cursor_pos);
        tile_map.update_selection(&mut tile_selection, &transform);

        ui_sys.begin_frame(&app, &input_sys, frame_clock.delta_time());
        render_sys.begin_frame();
        {
            tile_map_renderer.draw_map(
                &mut render_sys,
                &ui_sys,
                &tile_map,
                //TileMapRenderFlags::DrawTerrainTileDebugInfo |
                //TileMapRenderFlags::DrawBuildingsTileDebugInfo |
                //TileMapRenderFlags::DrawUnitsTileDebugInfo |
                //TileMapRenderFlags::DrawTileDebugBounds |
                TileMapRenderFlags::DrawTerrain |
                TileMapRenderFlags::DrawBuildings |
                TileMapRenderFlags::DrawUnits |
                TileMapRenderFlags::DrawGrid);

            tile_selection.draw(&mut render_sys);

            test_ui.draw(&ui_sys, frame_clock.delta_time());

            ui_sys.draw_debug(&mut render_sys);
        }
        render_sys.end_frame(&tex_cache);
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }
}
