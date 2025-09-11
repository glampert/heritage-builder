#![allow(dead_code)]

mod app;
mod debug;
mod game;
mod imgui_ui;
mod log;
mod pathfind;
mod render;
mod tile;
mod utils;
mod save;

use imgui_ui::*;
use render::*;
use utils::*;
use app::*;
use app::input::*;
use debug::*;
use save::*;
use tile::{
    camera::{self, *},
    rendering::{self, *},
    selection::*,
    sets::*
};
use game::{
    sim::{self, *},
    world::*,
    system::*,
    cheats,
    building::config::BuildingConfigs,
    unit::config::UnitConfigs,
};

// TEMP - TEST
fn serialization_tests(tile_map: &tile::TileMap,
                       tile_sets: &TileSets,
                       world: &World,
                       sim: &Simulation,
                       systems: &GameSystems,
                       camera: &Camera,
                       building_configs: &BuildingConfigs,
                       unit_configs: &UnitConfigs) {
    use std::fs;

    let context = PostLoadContext {
        tile_map,
        tile_sets,
        unit_configs,
        building_configs,
    };

    // TileMap
    {
        let json = match serde_json::to_string_pretty(tile_map) {
            Ok(json) => {
                Some(json)
            },
            Err(err) => {
                log::error!("Failed to serialize tile map: {err}");
                None
            },
        };

        if let Some(json) = json {
            let maybe_map = match serde_json::from_str::<tile::TileMap>(&json) {
                Ok(w) => Some(w),
                Err(err) => {
                    log::error!("Failed to deserialize tile map: {err}");
                    None
                }
            };

            if let Some(mut tile_map2) = maybe_map {
                // fixup all references/callbacks
                tile_map2.post_load(&context);
                log::info!("TileMap deserialization: Ok");
            }

            fs::write("tile_map.json", json).expect("Failed to write file");
        }
    }

    // World:
    {
        let json = match serde_json::to_string_pretty(world) {
            Ok(json) => {
                Some(json)
            },
            Err(err) => {
                log::error!("Failed to serialize world state: {err}");
                None
            },
        };

        if let Some(json) = json {
            let maybe_world = match serde_json::from_str::<World>(&json) {
                Ok(w) => Some(w),
                Err(err) => {
                    log::error!("Failed to deserialize world state: {err}");
                    None
                }
            };

            if let Some(mut world2) = maybe_world {
                // fixup all references/callbacks
                world2.post_load(&context);
                log::info!("World deserialization: Ok");
            }

            fs::write("world.json", json).expect("Failed to write file");
        }
    }

    // Sim:
    {
        let json = match serde_json::to_string_pretty(sim) {
            Ok(json) => {
                Some(json)
            },
            Err(err) => {
                log::error!("Failed to serialize simulation state: {err}");
                None
            },
        };

        if let Some(json) = json {
            let maybe_sim = match serde_json::from_str::<Simulation>(&json) {
                Ok(s) => Some(s),
                Err(err) => {
                    log::error!("Failed to deserialize simulation state: {err}");
                    None
                }
            };

            if let Some(mut sim2) = maybe_sim {
                // fixup all references/callbacks
                sim2.post_load(&context);
                log::info!("Sim deserialization: Ok");
            }

            fs::write("sim.json", json).expect("Failed to write file");
        }
    }

    // Systems:
    {
        let json = match serde_json::to_string_pretty(systems) {
            Ok(json) => {
                Some(json)
            },
            Err(err) => {
                log::error!("Failed to serialize game systems state: {err}");
                None
            },
        };

        if let Some(json) = json {
            let maybe_sys = match serde_json::from_str::<GameSystems>(&json) {
                Ok(s) => Some(s),
                Err(err) => {
                    log::error!("Failed to deserialize game systems state: {err}");
                    None
                }
            };

            if let Some(mut systems2) = maybe_sys {
                // fixup all references/callbacks
                systems2.post_load(&context);
                log::info!("Game systems deserialization: Ok");
            }

            fs::write("systems.json", json).expect("Failed to write file");
        }
    }

    // Camera
    {
        let json = match serde_json::to_string_pretty(camera) {
            Ok(json) => {
                Some(json)
            },
            Err(err) => {
                log::error!("Failed to serialize camera state: {err}");
                None
            },
        };

        if let Some(json) = json {
            let maybe_cam = match serde_json::from_str::<Camera>(&json) {
                Ok(c) => Some(c),
                Err(err) => {
                    log::error!("Failed to deserialize camera state: {err}");
                    None
                }
            };

            if let Some(mut camera2) = maybe_cam {
                camera2.post_load(&context);
                log::info!("Camera deserialization: Ok");
            }

            fs::write("camera.json", json).expect("Failed to write file");
        }
    }
}

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let log_viewer = log_viewer::LogViewerWindow::new(false, 32);

    let cwd = std::env::current_dir().unwrap();
    log::info!("The current directory is \"{}\".", cwd.display());

    let mut app = ApplicationBuilder::new()
        .window_title("CitySim")
        .window_size(Size::new(1024, 768))
        .fullscreen(false)
        .confine_cursor_to_window(Camera::confine_cursor_to_window())
        .build();

    let input_sys = app.create_input_system();

    let mut render_sys = RenderSystemBuilder::new()
        .viewport_size(app.window_size())
        .clear_color(rendering::MAP_BACKGROUND_COLOR)
        .build();

    let mut ui_sys = UiSystem::new(&app);

    debug::set_show_popup_messages(true);
    cheats::initialize();

    let building_configs = BuildingConfigs::load();
    let unit_configs = UnitConfigs::load();
    let tile_sets = TileSets::load(render_sys.texture_cache_mut());

    let mut world = World::new(&building_configs, &unit_configs);

    // Test map with preset tiles:
    let mut tile_map = debug::utils::create_test_tile_map_preset(&mut world, &tile_sets, 0);

    // Empty map (dirt tiles):
    /*
    let mut tile_map = tile::TileMap::with_terrain_tile(
        Size::new(64, 64),
        &tile_sets,
        TERRAIN_GROUND_CATEGORY,
        utils::hash::StrHashPair::from_str("dirt")
    );
    */

    let mut systems = GameSystems::new();
    systems.register(settlers::SettlersSpawnSystem::new());

    let mut sim = Simulation::new(&tile_map, &building_configs, &unit_configs);

    let mut tile_selection = TileSelection::new();
    let mut tile_map_renderer = TileMapRenderer::new(
        rendering::DEFAULT_GRID_COLOR,
        1.0);

    let mut camera = Camera::new(
        render_sys.viewport().size(),
        tile_map.size_in_cells(),
        CameraZoom::MIN,
        CameraOffset::Center);

    let mut debug_menus = DebugMenusSystem::new(&mut tile_map, render_sys.texture_cache_mut());

    let mut render_sys_stats = RenderStats::default();
    let mut frame_clock = FrameClock::new();

    serialization_tests(&tile_map, &tile_sets, &world, &sim, &systems, &camera, &building_configs, &unit_configs);

    while !app.should_quit() {
        frame_clock.begin_frame();

        let cursor_screen_pos = input_sys.cursor_pos();
        let delta_time_secs = frame_clock.delta_time();

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

                    if debug_menus.on_key_input(&mut DebugMenusOnInputArgs {
                                                    tile_map: &mut tile_map,
                                                    tile_selection: &mut tile_selection,
                                                    world: &mut world,
                                                    transform: *camera.transform(),
                                                    cursor_screen_pos,
                                                },
                                                key, action).is_handled() {
                        continue;
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
                        camera.request_zoom(camera::CameraZoom::In);
                    } else if amount.y > 0.0 {
                        camera.request_zoom(camera::CameraZoom::Out);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if ui_sys.on_mouse_click(button, action, modifiers).is_handled() {
                        continue;
                    }

                    if debug_menus.on_mouse_click(&mut DebugMenusOnInputArgs {
                                                      tile_map: &mut tile_map,
                                                      tile_selection: &mut tile_selection,
                                                      world: &mut world,
                                                      transform: *camera.transform(),
                                                      cursor_screen_pos,
                                                  },
                                                  button, action, modifiers).is_handled() {
                        continue;
                    }
                }
            }
        }

        sim.update(&mut world, &mut systems, &mut tile_map, &tile_sets, delta_time_secs);

        camera.update_zooming(delta_time_secs);

        // Map scrolling:
        if !ui_sys.is_handling_mouse_input() {
            camera.update_scrolling(cursor_screen_pos, delta_time_secs);
        }

        let visible_range = camera.visible_cells_range();

        tile_map.update_anims(visible_range, delta_time_secs);

        ui_sys.begin_frame(&app, &input_sys, delta_time_secs);
        render_sys.begin_frame();

        let selected_render_flags =
            debug_menus.begin_frame(&mut DebugMenusBeginFrameArgs {
                ui_sys: &ui_sys,
                sim: &mut sim,
                world: &mut world,
                tile_map: &mut tile_map,
                tile_selection: &mut tile_selection,
                tile_sets: &tile_sets,
                transform: *camera.transform(),
                cursor_screen_pos,
                delta_time_secs,
            });

        let tile_render_stats =
            tile_map_renderer.draw_map(
                &mut render_sys,
                &ui_sys,
                &tile_map,
                camera.transform(),
                visible_range,
                selected_render_flags);

        tile_selection.draw(&mut render_sys);

        debug_menus.end_frame(&mut DebugMenusEndFrameArgs {
            context: sim::debug::DebugContext {
                ui_sys: &ui_sys,
                world: &mut world,
                systems: &mut systems,
                tile_map: &mut tile_map,
                tile_sets: &tile_sets,
                transform: *camera.transform(),
                delta_time_secs,
            },
            sim: &mut sim,
            log_viewer: &log_viewer,
            camera: &mut camera,
            render_sys: &mut render_sys,
            render_sys_stats: &render_sys_stats,
            tile_map_renderer: &mut tile_map_renderer,
            tile_render_stats: &tile_render_stats,
            tile_selection: &tile_selection,
            visible_range,
            cursor_screen_pos,
        });

        render_sys_stats = render_sys.end_frame();
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }

    sim.reset(&mut world, &mut systems, &mut tile_map, &tile_sets);
}
