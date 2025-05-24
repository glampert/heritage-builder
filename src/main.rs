#![allow(dead_code)]

mod ui;
mod render;
mod utils;
mod app;
mod tile;

use ui::*;
use render::*;
use utils::*;

use app::{
    *,
    input::*
};

use tile::{
    debug::{self},
    debug_ui::*,
    rendering::*,
    selection::*
};

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

    let tile_sets = debug::create_test_tile_sets(&mut tex_cache);

    let mut tile_map = debug::create_test_tile_map(&tile_sets);
    //let mut tile_map = TileMap::new(Size2D::new(8, 8));

    let mut tile_map_renderer = TileMapRenderer::new();

    let mut debug_settings_menu = DebugSettingsMenu::new();
    debug_settings_menu.apply_render_settings(&mut tile_map_renderer);

    let mut tile_selection = TileSelection::new();
    let mut tile_list_menu = TileListMenu::new(&mut tex_cache);
    let mut tile_inspector_menu = TileInspectorMenu::new();

    let mut frame_clock = FrameClock::new();

    while !app.should_quit() {
        frame_clock.begin_frame();

        let transform = tile_map_renderer.world_to_screen_transform();
        let cursor_pos = input_sys.cursor_pos();

        for event in app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_sys.set_window_size(window_size);
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
                        if tile_selection.on_mouse_click(button, action, cursor_pos).not_handled() {
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

        if !ui_sys.is_handling_mouse_input() { // If we're not hovering over an ImGui menu.
            tile_map.update_selection(&mut tile_selection,
                                      cursor_pos,
                                      &transform,
                                      tile_list_menu.current_selection());
        }

        if tile_list_menu.can_place_tile() {
            let current_sel = tile_list_menu.current_selection().unwrap();

            let did_place = tile_map.try_place_tile_at_cursor(
                cursor_pos,
                &transform,
                current_sel);

            if did_place && (current_sel.is_building() || current_sel.is_unit() || current_sel.is_empty()) {
                // Dop or remove building/unit and exit tile placement mode.
                tile_list_menu.clear_selection();
                tile_map.clear_selection(&mut tile_selection);
            }
        }

        ui_sys.begin_frame(&app, &input_sys, frame_clock.delta_time());
        render_sys.begin_frame();
        {
            tile_map_renderer.draw_map(
                &mut render_sys,
                &ui_sys,
                &tile_map,
                debug_settings_menu.selected_render_flags());

            tile_selection.draw(&mut render_sys);

            tile_list_menu.draw(
                &mut render_sys,
                &ui_sys,
                &tex_cache,
                &tile_sets,
                cursor_pos,
                &transform,
                tile_selection.has_valid_placement(),
                debug_settings_menu.show_selection_bounds());

            tile_inspector_menu.draw(&mut tile_map, &ui_sys, &transform);
            debug_settings_menu.draw(&mut tile_map_renderer, &ui_sys);

            if debug_settings_menu.show_cursor_pos() {
                debug::draw_cursor_overlay(&ui_sys, &transform);
            }

            if debug_settings_menu.show_screen_origin() {
                debug::draw_screen_origin_marker(&mut render_sys);
            }
        }
        render_sys.end_frame(&tex_cache);
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }
}
