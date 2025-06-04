#![allow(dead_code)]

mod imgui_ui;
mod render;
mod utils;
mod app;
mod tile;

use imgui_ui::*;
use render::*;
use utils::*;
use app::{*, input::*};

use tile::{
    rendering::{self, *},
    camera::{self, *},
    debug_utils::{self},
    debug_ui::*,
    map::*,
    selection::*,
    sets::*
};

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let cwd = std::env::current_dir().unwrap();
    println!("The current directory is \"{}\".", cwd.display());

    let mut app = ApplicationBuilder::new()
        .window_title("CitySim")
        .window_size(Size::new(1024, 768))
        .fullscreen(false)
        .confine_cursor_to_window(camera::CONFINE_CURSOR_TO_WINDOW)
        .build();

    let input_sys = app.create_input_system();

    let mut render_sys = RenderSystemBuilder::new()
        .viewport_size(app.window_size())
        .clear_color(rendering::MAP_BACKGROUND_COLOR)
        .build();

    let mut ui_sys = UiSystem::new(&app);

    let tile_sets = TileSets::load(render_sys.texture_cache_mut());

    //let mut tile_map = debug_utils::create_test_tile_map(&tile_sets);
    let mut tile_map = TileMap::new(Size::new(64, 64));

    let mut tile_selection = TileSelection::new();
    let mut tile_map_renderer = TileMapRenderer::new(rendering::DEFAULT_GRID_COLOR, 3.0);

    let mut camera = Camera::new(
        render_sys.viewport().size(),
        tile_map.size(),
        camera::MIN_ZOOM,
        camera::Offset::Center,
        camera::MIN_TILE_SPACING);

    let mut tile_inspector_menu = TileInspectorMenu::new();
    let mut tile_list_menu = TileListMenu::new(render_sys.texture_cache_mut(), true);
    let mut debug_settings_menu = DebugSettingsMenu::new(true);

    let mut frame_clock = FrameClock::new();

    while !app.should_quit() {
        frame_clock.begin_frame();

        let cursor_screen_pos = input_sys.cursor_pos();

        for event in app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_sys.set_viewport_size(window_size);
                    camera.set_viewport_size(window_size);
                }
                ApplicationEvent::KeyInput(key, action, modifiers) => {
                    if ui_sys.on_key_input(key, action, modifiers).is_handled() {
                        continue;
                    }

                    if key == InputKey::Escape {
                        tile_inspector_menu.close();
                        tile_list_menu.clear_selection();
                        tile_map.clear_selection(&mut tile_selection);
                    }
                }
                ApplicationEvent::CharInput(c) => {
                    if ui_sys.on_char_input(c).is_handled() {
                        continue;
                    }
                }
                ApplicationEvent::Scroll(amount) => {
                    if ui_sys.on_scroll(amount).is_handled() {
                        continue;
                    }

                    if amount.y < 0.0 {
                        camera.request_zoom(camera::Zoom::In);
                    } else if amount.y > 0.0 {
                        camera.request_zoom(camera::Zoom::Out);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if ui_sys.on_mouse_click(button, action, modifiers).is_handled() {
                        continue;
                    }

                    if tile_list_menu.has_selection() {
                        if tile_list_menu.on_mouse_click(button, action).not_handled() {
                            tile_list_menu.clear_selection();
                            tile_map.clear_selection(&mut tile_selection);
                        }
                    } else {
                        if tile_selection.on_mouse_click(button, action, cursor_screen_pos).not_handled() {
                            tile_list_menu.clear_selection();
                            tile_map.clear_selection(&mut tile_selection);
                        }

                        if let Some(selected_tile) = tile_map.topmost_selected_tile(&tile_selection) {
                            if tile_inspector_menu.on_mouse_click(button, action, selected_tile).is_handled() {
                                continue;
                            }
                        }
                    }
                }
            }
            println!("ApplicationEvent::{:?}", event);
        }

        camera.update_zooming(frame_clock.delta_time());

        // If we're not hovering over an ImGui menu...
        if !ui_sys.is_handling_mouse_input() {
            // Map scrolling:
            camera.update_scrolling(cursor_screen_pos, frame_clock.delta_time());

            // Tile hovering and selection:
            let placement_candidate = tile_list_menu.current_selection(&tile_sets);
            tile_map.update_selection(&mut tile_selection,
                                      cursor_screen_pos,
                                      camera.transform(),
                                      placement_candidate);
        }

        if tile_list_menu.can_place_tile() {
            let tile_to_place = tile_list_menu.current_selection(&tile_sets).unwrap();

            let did_place = tile_map.try_place_tile_at_cursor(
                cursor_screen_pos,
                camera.transform(),
                tile_to_place);

            if did_place && (tile_to_place.is_building() || tile_to_place.is_unit() || tile_to_place.is_empty()) {
                // Dop or remove building/unit and exit tile placement mode.
                tile_list_menu.clear_selection();
                tile_map.clear_selection(&mut tile_selection);
            }
        }

        let visible_range = camera.visible_cells_range();

        tile_map.update_anims(visible_range, frame_clock.delta_time());

        ui_sys.begin_frame(&app, &input_sys, frame_clock.delta_time());
        render_sys.begin_frame();

        let tile_render_stats = tile_map_renderer.draw_map(
            &mut render_sys,
            &ui_sys,
            &tile_map,
            camera.transform(),
            visible_range,
            debug_settings_menu.selected_render_flags());

        tile_selection.draw(&mut render_sys);

        tile_list_menu.draw(
            &mut render_sys,
            &ui_sys,
            &tile_sets,
            cursor_screen_pos,
            camera.transform(),
            tile_selection.has_valid_placement(),
            debug_settings_menu.show_selection_bounds());

        tile_inspector_menu.draw(&mut tile_map, &tile_sets, &ui_sys, camera.transform());
        debug_settings_menu.draw(&mut camera, &mut tile_map_renderer, &mut tile_map, &tile_sets, &ui_sys);

        if debug_settings_menu.show_cursor_pos() {
            debug_utils::draw_cursor_overlay(&ui_sys, camera.transform());
        }

        if debug_settings_menu.show_screen_origin() {
            debug_utils::draw_screen_origin_marker(&mut render_sys);
        }

        let render_sys_stats = render_sys.end_frame();

        if debug_settings_menu.show_render_stats() {
            debug_utils::draw_render_stats(&ui_sys, &render_sys_stats, &tile_render_stats);
        }

        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }
}
