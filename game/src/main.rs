#![allow(dead_code)]

mod app;
mod debug;
mod game;
mod imgui_ui;
mod pathfind;
mod render;
mod tile;
mod utils;

use imgui_ui::*;
use pathfind::*;
use render::*;
use utils::{
    *,
    coords::*,
    hash::*,
};
use app::{
    *,
    input::*
};
use debug::{
    inspector::*,
    palette::*,
    settings::*
};
use tile::{
    camera::{self, *},
    rendering::{self, *},
    selection::*,
    placement::*,
    sets::*,
    map::*
};
use game::{
    sim::{self, *},
    sim::world::*,
    building::{self, config::BuildingConfigs},
    unit::{self, config::UnitConfigs},
};

//use std::cell::RefCell;
//use std::cell::OnceCell;

//std::thread_local! {
    //static TILE_INSPECTOR_MENU: RefCell<TileInspectorMenu> = const { RefCell::new(TileInspectorMenu::new()) };
//}

//std::thread_local! {
    //static DEBUG_SYSTEM_IMPL: OnceCell<RefCell<DebugMenusSystem>> = const { OnceCell::new() };
//}

// ----------------------------------------------
// DebugMenusSystem
// ----------------------------------------------

struct DebugMenusBeginFrameArgs<'ui, 'world, 'config, 'tile_map, 'tile_sets> {
    ui_sys: &'ui UiSystem,
    world: &'world mut World<'config>,
    tile_map: &'tile_map mut TileMap<'tile_sets>,
    tile_selection: &'tile_map mut TileSelection,
    tile_sets: &'tile_sets TileSets,
    building_configs: &'config BuildingConfigs,
    unit_configs: &'config UnitConfigs,
    transform: WorldToScreenTransform,
    cursor_screen_pos: Vec2,
}

struct DebugMenusEndFrameArgs<'rs, 'cam, 'sim, 'ui, 'world, 'config, 'tile_map, 'tile_sets, RS: RenderSystem> {
    context: sim::debug::DebugContext<'ui, 'world, 'config, 'tile_map, 'tile_sets>,
    sim: &'sim mut Simulation,
    camera: &'cam mut Camera,
    render_sys: &'rs mut RS,
    render_sys_stats: &'rs RenderStats,
    tile_map_renderer: &'rs mut TileMapRenderer,
    tile_render_stats: &'rs TileMapRenderStats,
    tile_selection: &'tile_map TileSelection,
    visible_range: CellRange,
    cursor_screen_pos: Vec2,
}

#[derive(Default)]
struct DebugMenusSystem {
    debug_settings_menu: DebugSettingsMenu,
    tile_palette_menu: TilePaletteMenu,
    tile_inspector_menu: TileInspectorMenu,

    // Test path finding:
    //  [CTRL]+Left-Click places start and end goals.
    //  [ENTER] runs the search and highlights path cells.
    //  [ESCAPE] clears start/end and search results.
    search_test_start: Cell,
    search_test_goal: Cell,
    search_test_mode: bool,
}

impl DebugMenusSystem {
    fn new(tex_cache: &mut impl TextureCache, debug_settings_open: bool, tile_palette_open: bool) -> Self {
        Self {
            debug_settings_menu: DebugSettingsMenu::new(debug_settings_open),
            tile_palette_menu: TilePaletteMenu::new(tile_palette_open, tex_cache),
            tile_inspector_menu: TileInspectorMenu::new(),
            search_test_start: Cell::invalid(),
            search_test_goal: Cell::invalid(),
            ..Default::default()
        }
    }

    fn on_key_input(&mut self,
                    tile_map: &mut TileMap,
                    tile_selection: &mut TileSelection,
                    world: &mut World,
                    key: InputKey,
                    action: InputAction) -> UiInputEvent {

        if key == InputKey::LeftControl || key == InputKey::RightControl {
            if action == InputAction::Press {
                self.search_test_mode = true;
            } else if action == InputAction::Release {
                self.search_test_mode = false;
            }
        }

        if key == InputKey::Escape && action == InputAction::Press {
            self.tile_inspector_menu.close();
            self.tile_palette_menu.clear_selection();
            tile_map.clear_selection(tile_selection);

            // Clear test search:
            self.search_test_start = Cell::invalid();
            self.search_test_goal  = Cell::invalid();
            tile_map.for_each_tile_mut(TileMapLayerKind::Terrain, TileKind::all(),
                |tile| {
                    tile.set_flags(TileFlags::Highlighted, false);
                });

            if let Some(ped) = world.find_unit_by_name_mut("Ped") {
                ped.follow_path(None);
            }

            return UiInputEvent::Handled;
        }

        // Run test search:
        if key == InputKey::Enter && action == InputAction::Press {
            let graph = Graph::from_tile_map(tile_map);
            let heuristic = AStarUniformCostHeuristic::new();
            let traversable_node_kinds = NodeKind::Road;
            let start = Node::new(self.search_test_start);
            let goal = Node::new(self.search_test_goal);
            let mut search = Search::new(&graph);

            match search.find_path(&graph, &heuristic, traversable_node_kinds, start, goal) {
                SearchResult::PathFound(path) => {
                    println!("Found a path with {} nodes.", path.len());

                    // Highlight path tiles:
                    for node in path {
                        if let Some(tile) = tile_map.try_tile_from_layer_mut(node.cell, TileMapLayerKind::Terrain) {
                            tile.set_flags(TileFlags::Highlighted, true);
                        }
                    }

                    // Make unit follow path:
                    if let Some(ped) = world.find_unit_by_name_mut("Ped") {
                        ped.follow_path(Some(path));
                    }
                },
                SearchResult::PathNotFound => println!("No path could be found."),
            }

            return UiInputEvent::Handled;
        }

        UiInputEvent::NotHandled
    }

    #[allow(clippy::too_many_arguments)]
    fn on_mouse_click(&mut self,
                      tile_map: &mut TileMap,
                      tile_selection: &mut TileSelection,
                      button: MouseButton,
                      action: InputAction,
                      modifiers: InputModifiers,
                      transform: WorldToScreenTransform,
                      cursor_screen_pos: Vec2) -> UiInputEvent {

        if self.tile_palette_menu.has_selection() {
            if self.tile_palette_menu.on_mouse_click(button, action).not_handled() {
                self.tile_palette_menu.clear_selection();
                tile_map.clear_selection(tile_selection);
            }
        } else {
            if tile_selection.on_mouse_click(button, action, cursor_screen_pos).not_handled() {
                self.tile_palette_menu.clear_selection();
                tile_map.clear_selection(tile_selection);
            }

            // Select search test start/goal cells:
            if self.search_test_mode && button == MouseButton::Left && modifiers.intersects(InputModifiers::Control) {
                if !self.search_test_start.is_valid() {
                    let cursor_cell = tile_map.find_exact_cell_for_point(
                        TileMapLayerKind::Terrain,
                        cursor_screen_pos,
                        &transform);
                    self.search_test_start = cursor_cell;
                } else if !self.search_test_goal.is_valid() {
                    let cursor_cell = tile_map.find_exact_cell_for_point(
                        TileMapLayerKind::Terrain,
                        cursor_screen_pos,
                        &transform);
                    if cursor_cell != self.search_test_start {
                        self.search_test_goal = cursor_cell;
                    }
                }
                return UiInputEvent::Handled;
            }

            if let Some(selected_tile) = tile_map.topmost_selected_tile(tile_selection) {
                if self.tile_inspector_menu.on_mouse_click(button, action, selected_tile).is_handled() {
                    return UiInputEvent::Handled;
                }
            }
        }

        UiInputEvent::NotHandled
    }

    fn begin_frame(&mut self, args: &mut DebugMenusBeginFrameArgs) -> TileMapRenderFlags {
        // If we're not hovering over an ImGui menu...
        if !args.ui_sys.is_handling_mouse_input() {
            // Tile hovering and selection:
            let placement_op = {
                if let Some(tile_def) = self.tile_palette_menu.current_selection(args.tile_sets) {
                    PlacementOp::Place(tile_def)
                } else if self.tile_palette_menu.is_clear_selected() {
                    PlacementOp::Clear
                } else {
                    PlacementOp::None
                }
            };

            args.tile_map.update_selection(
                args.tile_selection,
                args.cursor_screen_pos,
                &args.transform,
                placement_op);
        }

        if self.tile_palette_menu.can_place_tile() {
            let placement_candidate =
                self.tile_palette_menu.current_selection(args.tile_sets);

            let did_place_or_clear = {
                // If we have a selection place it, otherwise we want to try clearing the tile under the cursor.
                if let Some(tile_def) = placement_candidate {
                    let place_result = args.tile_map.try_place_tile_at_cursor(
                        args.cursor_screen_pos,
                        &args.transform,
                        tile_def);

                    if let Some(tile) = place_result {
                        if tile_def.is(TileKind::Building) {
                            if let Some(building) = building::config::instantiate(tile, args.building_configs) {
                                args.world.add_building(tile, building);
                            }
                        } else if tile_def.is(TileKind::Unit) {
                            if let Some(unit) = unit::config::instantiate(tile, args.unit_configs) {
                                args.world.add_unit(tile, unit);
                            }
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    if let Some(tile) = args.tile_map.topmost_tile_at_cursor(args.cursor_screen_pos, &args.transform) {
                        if tile.is(TileKind::Building | TileKind::Blocker) {
                            args.world.remove_building(tile);
                        } else if tile.is(TileKind::Unit) {
                            args.world.remove_unit(tile);
                        }
                    }

                    args.tile_map.try_clear_tile_at_cursor(
                        args.cursor_screen_pos,
                        &args.transform)
                }
            };

            let placing_an_object = placement_candidate.is_some_and(|def| def.is(TileKind::Object));
            let clearing_a_tile   = self.tile_palette_menu.is_clear_selected();

            if did_place_or_clear && (placing_an_object || clearing_a_tile) {
                // Place or remove building/unit and exit tile placement mode.
                self.tile_palette_menu.clear_selection();
                args.tile_map.clear_selection(args.tile_selection);
            }
        }

        self.debug_settings_menu.selected_render_flags()
    }

    fn end_frame(&mut self, args: &mut DebugMenusEndFrameArgs<impl RenderSystem>) {
        let show_cursor_pos = self.debug_settings_menu.show_cursor_pos();
        let show_screen_origin = self.debug_settings_menu.show_screen_origin();
        let show_render_stats = self.debug_settings_menu.show_render_stats();
        let show_popup_messages = self.debug_settings_menu.show_popup_messages();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let has_valid_placement = args.tile_selection.has_valid_placement();

        self.tile_palette_menu.draw(
            &mut args.context,
            args.render_sys,
            args.cursor_screen_pos,
            has_valid_placement,
            show_selection_bounds);

        self.tile_inspector_menu.draw(
            &mut args.context,
            args.sim);

        self.debug_settings_menu.draw(
            &mut args.context,
            args.camera,
            args.tile_map_renderer);

        args.sim.draw_building_debug_popups(&mut args.context, args.visible_range, show_popup_messages);
        args.sim.draw_unit_debug_popups(&mut args.context, args.visible_range, show_popup_messages);

        if self.search_test_mode {
            debug::utils::draw_cursor_overlay(
                args.context.ui_sys,
                args.camera.transform(),
                Some(&format!("Search Test: {} -> {}", self.search_test_start, self.search_test_goal)));
        }

        if show_cursor_pos {
            debug::utils::draw_cursor_overlay(args.context.ui_sys, args.camera.transform(), None);
        }

        if show_render_stats {
            debug::utils::draw_render_stats(args.context.ui_sys, args.render_sys_stats, args.tile_render_stats);
        }

        if show_screen_origin {
            debug::utils::draw_screen_origin_marker(args.render_sys);
        }
    }
}

fn register_tile_map_callbacks() {
    //TODO REGISTER THESE WITH THE NEW DebugMenusSystem
    /*
    TileEditor::set_tile_placed_callback(|tile, did_reallocate| {
        TILE_INSPECTOR_MENU.with(|inspector| {
            let mut inspector = inspector.borrow_mut();
            inspector.on_tile_placed(tile, did_reallocate);
        });
    });

    TileEditor::set_removing_tile_callback(|tile| {
        TILE_INSPECTOR_MENU.with(|inspector| {
            let mut inspector = inspector.borrow_mut();
            inspector.on_removing_tile(tile);
        });
    });

    TileEditor::set_map_reset_callback(|_| {
        TILE_INSPECTOR_MENU.with(|inspector| {
            let mut inspector = inspector.borrow_mut();
            inspector.close();
        });
    });
    */
}

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

    let building_configs = BuildingConfigs::load();
    let unit_configs = UnitConfigs::load();

    let mut sim = Simulation::new();
    let mut world = World::new();

    let tile_sets = TileSets::load(render_sys.texture_cache_mut());

    /*
    let mut tile_map = create_test_tile_map(&tile_sets);
    tile_map.for_each_tile_mut(TileMapLayerKind::Objects, TileKind::Building, |tile| {
        // NOTE: This is temporary while testing only. Map should start empty.
        if let Some(building) = building::config::instantiate(tile, &building_configs) {
            world.add_building(tile, building);
        }
    });
    */

    let mut tile_map = TileMap::with_terrain_tile(
        Size::new(64, 64),
        &tile_sets,
        TERRAIN_GROUND_CATEGORY,
        StrHashPair::from_str("dirt")
    );

    let mut tile_selection = TileSelection::new();
    let mut tile_map_renderer = TileMapRenderer::new(
        rendering::DEFAULT_GRID_COLOR,
        1.0);

    let mut camera = Camera::new(
        render_sys.viewport().size(),
        tile_map.size_in_cells(),
        camera::MIN_ZOOM,
        camera::Offset::Center);

    const DEBUG_SETTINGS_OPEN: bool = false;
    const TILE_PALETTE_OPEN: bool = true;
    let mut debug_menus = DebugMenusSystem::new(
        render_sys.texture_cache_mut(),
        DEBUG_SETTINGS_OPEN,
        TILE_PALETTE_OPEN);

    let mut render_sys_stats = RenderStats::default();
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

                    if debug_menus.on_key_input(&mut tile_map,
                                                &mut tile_selection,
                                                &mut world,
                                                key,
                                                action).is_handled() {
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
                        camera.request_zoom(camera::Zoom::In);
                    } else if amount.y > 0.0 {
                        camera.request_zoom(camera::Zoom::Out);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if ui_sys.on_mouse_click(button, action, modifiers).is_handled() {
                        continue;
                    }

                    if debug_menus.on_mouse_click(&mut tile_map,
                                                  &mut tile_selection,
                                                  button,
                                                  action,
                                                  modifiers,
                                                  *camera.transform(),
                                                  cursor_screen_pos).is_handled() {
                        continue;
                    }
                }
            }
        }

        sim.update(&mut world, &mut tile_map, &tile_sets, frame_clock.delta_time());

        camera.update_zooming(frame_clock.delta_time());

        // Map scrolling:
        if !ui_sys.is_handling_mouse_input() {
            camera.update_scrolling(cursor_screen_pos, frame_clock.delta_time());
        }

        let visible_range = camera.visible_cells_range();

        tile_map.update_anims(visible_range, frame_clock.delta_time());

        ui_sys.begin_frame(&app, &input_sys, frame_clock.delta_time());
        render_sys.begin_frame();

        let selected_render_flags =
            debug_menus.begin_frame(&mut DebugMenusBeginFrameArgs {
                ui_sys: &ui_sys,
                world: &mut world,
                tile_map: &mut tile_map,
                tile_selection: &mut tile_selection,
                tile_sets: &tile_sets,
                building_configs: &building_configs,
                unit_configs: &unit_configs,
                transform: *camera.transform(),
                cursor_screen_pos,
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
                tile_map: &mut tile_map,
                tile_sets: &tile_sets,
                transform: *camera.transform(),
                delta_time_secs: frame_clock.delta_time().as_secs_f32(),
            },
            sim: &mut sim,
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
}

fn create_test_tile_map(tile_sets: &TileSets) -> TileMap {
    println!("Creating test tile map...");

    const MAP_WIDTH:  i32 = 8;
    const MAP_HEIGHT: i32 = 8;

    const G: i32 = 0; // grass
    const D: i32 = 1; // dirt
    const H: i32 = 2; // house
    const W: i32 = 3; // well_small
    const B: i32 = 4; // well_big
    const M: i32 = 5; // market

    const TILE_NAMES: [&str; 6] = [ "grass", "dirt", "house0", "well_small", "well_big", "market" ];
    const TILE_CATEGORIES: [&str; 6] = [ "ground", "ground", "buildings", "buildings", "buildings", "buildings" ];

    let find_tile = |layer_kind: TileMapLayerKind, tile_id: i32| {
        let tile_name = TILE_NAMES[tile_id as usize];
        let category_name = TILE_CATEGORIES[tile_id as usize];
        tile_sets.find_tile_def_by_name(layer_kind, category_name, tile_name)
    };

    const TERRAIN_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        D,D,D,D,D,D,D,D, // <-- start, tile zero is the leftmost (top-left)
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,D,D,D,D,D,D,D,
    ];

    const BUILDINGS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        D,D,D,D,D,D,D,D, // <-- start, tile zero is the leftmost (top-left)
        D,H,G,B,G,M,G,D,
        D,G,G,G,G,G,G,D,
        D,G,W,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,G,G,G,G,G,G,D,
        D,D,D,D,D,D,D,D,
    ];

    let mut tile_map = TileMap::new(Size::new(MAP_WIDTH, MAP_HEIGHT), None);

    // Terrain:
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile_id = TERRAIN_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
            if let Some(tile_def) = find_tile(TileMapLayerKind::Terrain, tile_id) {
                let place_result = tile_map.try_place_tile_in_layer(Cell::new(x, y), TileMapLayerKind::Terrain, tile_def);
                debug_assert!(place_result.is_some());
            }
        }
    }

    // Buildings:
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile_id = BUILDINGS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
            if tile_id == G || tile_id == D {
                    // ground/empty
            } else {
                // building tile
                if let Some(tile_def) = find_tile(TileMapLayerKind::Objects, tile_id) {
                    let place_result = tile_map.try_place_tile_in_layer(Cell::new(x, y), TileMapLayerKind::Objects, tile_def);
                    debug_assert!(place_result.is_some());
                }
            }
        }
    }

    tile_map
}
