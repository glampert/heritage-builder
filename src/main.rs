#![allow(dead_code)]

mod utils;
mod app;
mod render;
mod ui;

use std::time::{self};
use utils::*;
use app::*;
use app::input::*;
use ui::system::*;
use render::system::*;
use render::texture::*;
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

    pub fn draw(&mut self, ui_sys: &UiSystem, _delta_time: time::Duration, transform: &WorldToScreenTransform, selection_rect: Rect2D) {
        let ui = ui_sys.builder();

        ui.window("CitySim")
            .size([250.0, 180.0], imgui::Condition::FirstUseEver)
            .position([10.0, 10.0], imgui::Condition::FirstUseEver)
            .build(|| {
                /*
                ui.text("CitySim!");
                ui.text("This...is...imgui-rs!");
                ui.text_colored([0.0, 1.0, 1.0, 1.0], format!("dt: {}", delta_time.as_secs_f64()));
                ui.separator();

                ui.radio_button("Radio button", &mut self.radio_btn_state, true);
                ui.checkbox("Checkbox", &mut self.checkbox_state);
                ui.button("Button");
                ui.input_int("Input int", &mut self.input_int_state).build();
                ui.input_text("Input text", &mut self.input_text_state).build();
                */

                ui.text(format!(
                    "Cursor Position: ({:.1},{:.1})",
                    ui.io().mouse_pos[0], ui.io().mouse_pos[1]
                ));

                let iso_top_left = utils::screen_to_iso_point(selection_rect.mins, transform);
                let iso_bottom_right = utils::screen_to_iso_point(selection_rect.maxs, transform);

                ui.text(format!(
                    "Sel Rect: min({},{}) max({},{})",
                    selection_rect.mins.x, selection_rect.mins.y,
                    selection_rect.maxs.x, selection_rect.maxs.y
                ));

                ui.text(format!(
                    "Sel Rect Iso: min({},{}) max({},{})",
                    iso_top_left.x, iso_top_left.y,
                    iso_bottom_right.x, iso_bottom_right.y
                ));
            });
    }
}

// ----------------------------------------------
// TileSelection
// ----------------------------------------------

pub struct TileSelection {
    rect: Rect2D,
    cursor_drag_start: Point2D,
    left_mouse_button_held: bool,
}

impl TileSelection {
    pub fn new() -> Self {
        Self {
            rect: Rect2D::zero(),
            cursor_drag_start: Point2D::zero(),
            left_mouse_button_held: false,
        }
    }

    pub fn on_mouse_click(&mut self, button: MouseButton, action: InputAction, cursor_pos: Point2D) {
        if action == InputAction::Press && button == MouseButton::Left {
            self.cursor_drag_start = cursor_pos;
            self.left_mouse_button_held = true;
        }
        else if action == InputAction::Release && button == MouseButton::Left {
            self.cursor_drag_start = Point2D::zero();
            self.left_mouse_button_held = false;
        }
    }

    pub fn update_rect(&mut self, cursor_pos: Point2D) -> Rect2D {
        if self.left_mouse_button_held {
            self.rect = Rect2D::with_points(self.cursor_drag_start, cursor_pos);   
        } else {
            self.rect = Rect2D::zero();
        }
        self.rect
    }

    pub fn draw_rect(&self, render_sys: &mut RenderSystem) {
        if self.left_mouse_button_held && self.rect.is_valid() {
            render_sys.draw_wireframe_rect_with_thickness(self.rect, Color::blue(), 1.5);
        }
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

    let input_sys = app::input::new_input_system(&app);

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
                    tile_selection.on_mouse_click(button, action, cursor_pos);
                }
            }
            println!("ApplicationEvent::{:?}", event);
        }

        let selection_rect = tile_selection.update_rect(cursor_pos);
        if selection_rect.is_valid() {
            tile_map.update_range_selection(&selection_rect, &transform);
        } else {
            tile_map.update_selection(cursor_pos, &transform);
        }

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

            tile_map.clear_range_selection();

            tile_selection.draw_rect(&mut render_sys);

            render_sys.draw_screen_origin_debug_marker();

            test_ui.draw(&ui_sys, frame_clock.delta_time(), &transform, selection_rect);

            ui_sys.draw_debug_cursor_overlay();
        }
        render_sys.end_frame(&tex_cache);
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }
}
