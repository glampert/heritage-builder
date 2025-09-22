use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    log,
    singleton_late_init,
    engine::time::Seconds,
    render::TextureCache,
    imgui_ui::{UiSystem, UiInputEvent},
    save::{Save, Load, PostLoadContext},
    app::input::{MouseButton, InputAction, InputKey, InputModifiers},
    game::{
        GameLoop,
        world::{World, object::Spawner},
        sim::{self, Simulation},
        system::GameSystems,
    },
    utils::{
        Vec2,
        mem,
        coords::{Cell, CellRange, WorldToScreenTransform}
    },
    tile::{
        TileMap,
        TileFlags,
        TileKind,
        TileMapLayerKind,
        PlacementOp,
        camera::Camera,
        selection::TileSelection,
        rendering::TileMapRenderFlags
    },
    pathfind::{
        self,
        Node,
        NodeKind,
        Graph,
        AStarUniformCostHeuristic,
        Search,
        SearchResult
    }
};

use inspector::TileInspectorMenu;
use palette::TilePaletteMenu;
use settings::DebugSettingsMenu;

pub mod utils;
pub mod popups;
pub mod log_viewer;

mod inspector;
mod palette;
mod settings;

// ----------------------------------------------
// Args helper structs
// ----------------------------------------------

pub struct DebugMenusInputArgs<'game> {
    pub world: &'game mut World,
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,
    pub transform: WorldToScreenTransform,
    pub cursor_screen_pos: Vec2,
}

pub struct DebugMenusFrameArgs<'game> {
    // Tile Map:
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,

    // Sim/World:
    pub sim: &'game mut Simulation,
    pub world: &'game mut World,
    pub systems: &'game mut GameSystems,

    // UI/Debug:
    pub ui_sys: &'game UiSystem,

    // Camera/Input:
    pub camera: &'game mut Camera,
    pub visible_range: CellRange,
    pub cursor_screen_pos: Vec2,
    pub delta_time_secs: Seconds,
}

// ----------------------------------------------
// DebugMenusSystem
// ----------------------------------------------

#[derive(Default)]
pub struct DebugMenusSystem;

impl DebugMenusSystem {
    pub fn new(tile_map: &mut TileMap, tex_cache: &mut dyn TextureCache) -> Self {
        // Initialize the singleton exactly once:
        init_singleton_once(tex_cache);

        // Register TileMap global callbacks & debug ref:
        register_tile_map_debug_callbacks(tile_map);

        Self
    }

    pub fn on_key_input(&mut self,
                        args: &mut DebugMenusInputArgs,
                        key: InputKey,
                        action: InputAction) -> UiInputEvent {
        DebugMenusSingleton::get_mut().on_key_input(args, key, action)
    }

    pub fn on_mouse_click(&mut self,
                          args: &mut DebugMenusInputArgs,
                          button: MouseButton,
                          action: InputAction,
                          modifiers: InputModifiers) -> UiInputEvent {
        DebugMenusSingleton::get_mut().on_mouse_click(args, button, action, modifiers)
    }

    pub fn begin_frame(&mut self, args: &mut DebugMenusFrameArgs) -> TileMapRenderFlags {
        DebugMenusSingleton::get_mut().begin_frame(args)
    }

    pub fn end_frame(&mut self, args: &mut DebugMenusFrameArgs) {
        DebugMenusSingleton::get_mut().end_frame(args);
    }
}

impl Drop for DebugMenusSystem {
    fn drop(&mut self) {
        DebugMenusSingleton::get_mut().tile_inspector_menu.close();

        // Clear the cached global tile map ptr.
        TILE_MAP_DEBUG_PTR.set(None);
    }
}

// ----------------------------------------------
// Save/Load for DebugMenusSystem
// ----------------------------------------------

impl Save for DebugMenusSystem {
}

impl Load for DebugMenusSystem {
    fn post_load(&mut self, context: &PostLoadContext) {
        DebugMenusSingleton::get_mut().tile_inspector_menu.close();

        // Re-register debug editor callbacks and reset the global tile map ref.
        register_tile_map_debug_callbacks(context.tile_map_mut());
    }
}

// ----------------------------------------------
// DebugMenusSingleton
// ----------------------------------------------

#[derive(Default)]
struct DebugMenusSingleton {
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

impl DebugMenusSingleton {
    fn new(tex_cache: &mut dyn TextureCache, debug_settings_open: bool, tile_palette_open: bool) -> Self {
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
                    args: &mut DebugMenusInputArgs,
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
            args.tile_map.clear_selection(args.tile_selection);

            // Clear search test state:
            self.search_test_start = Cell::invalid();
            self.search_test_goal  = Cell::invalid();
            args.tile_map.for_each_tile_mut(TileMapLayerKind::Terrain, TileKind::all(),
                |tile| tile.set_flags(TileFlags::Highlighted, false));

            if let Some(ped) = args.world.find_unit_by_name_mut("Ped") {
                ped.follow_path(None);
            }

            return UiInputEvent::Handled;
        }

        // Run search test:
        if key == InputKey::Enter && action == InputAction::Press {
            let graph = Graph::from_tile_map(args.tile_map);
            let heuristic = AStarUniformCostHeuristic::new();
            let traversable_node_kinds = NodeKind::Road;
            let start = Node::new(self.search_test_start);
            let goal = Node::new(self.search_test_goal);
            let mut search = Search::with_graph(&graph);

            match search.find_path(&graph, &heuristic, traversable_node_kinds, start, goal) {
                SearchResult::PathFound(path) => {
                    log::info!("Found a path with {} nodes.", path.len());
                    debug_assert!(!path.is_empty());

                    // Highlight path tiles:
                    pathfind::highlight_path_tiles(args.tile_map, path);

                    // Make unit follow path:
                    if let Some(ped) = args.world.find_unit_by_name_mut("Ped") {
                        // First teleport it to the start cell of the path:
                        ped.teleport(args.tile_map, path[0].cell);
                        ped.follow_path(Some(path));
                    }
                },
                SearchResult::PathNotFound => log::info!("No path could be found."),
            }

            return UiInputEvent::Handled;
        }

        UiInputEvent::NotHandled
    }

    fn on_mouse_click(&mut self,
                      args: &mut DebugMenusInputArgs,
                      button: MouseButton,
                      action: InputAction,
                      modifiers: InputModifiers) -> UiInputEvent {

        if self.tile_palette_menu.has_selection() {
            if self.tile_palette_menu.on_mouse_click(button, action).not_handled() {
                self.tile_palette_menu.clear_selection();
                args.tile_map.clear_selection(args.tile_selection);
            }
        } else {
            if args.tile_selection.on_mouse_click(button, action, args.cursor_screen_pos).not_handled() {
                self.tile_palette_menu.clear_selection();
                args.tile_map.clear_selection(args.tile_selection);
            }

            // Select search test start/goal cells:
            if self.search_test_mode && button == MouseButton::Left && modifiers.intersects(InputModifiers::Control) {
                if !self.search_test_start.is_valid() {
                    let cursor_cell = args.tile_map.find_exact_cell_for_point(
                        TileMapLayerKind::Terrain,
                        args.cursor_screen_pos,
                        args.transform);
                    self.search_test_start = cursor_cell;
                } else if !self.search_test_goal.is_valid() {
                    let cursor_cell = args.tile_map.find_exact_cell_for_point(
                        TileMapLayerKind::Terrain,
                        args.cursor_screen_pos,
                        args.transform);
                    if cursor_cell != self.search_test_start {
                        self.search_test_goal = cursor_cell;
                    }
                }
                return UiInputEvent::Handled;
            }

            if let Some(selected_tile) = args.tile_map.topmost_selected_tile(args.tile_selection) {
                if self.tile_inspector_menu.on_mouse_click(button, action, selected_tile).is_handled() {
                    return UiInputEvent::Handled;
                }
            }
        }

        UiInputEvent::NotHandled
    }

    fn begin_frame(&mut self, args: &mut DebugMenusFrameArgs) -> TileMapRenderFlags {
        // If we're not hovering over an ImGui menu...
        if !args.ui_sys.is_handling_mouse_input() {
            // Tile hovering and selection:
            let placement_op = {
                if let Some(tile_def) = self.tile_palette_menu.current_selection() {
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
                args.camera.transform(),
                placement_op);
        }

        if self.tile_palette_menu.can_place_tile() {
            let placement_candidate =
                self.tile_palette_menu.current_selection();

            let did_place_or_clear = {
                // If we have a selection place it, otherwise we want to try clearing the tile under the cursor.
                if let Some(tile_def) = placement_candidate {
                    let target_cell = args.tile_map.find_exact_cell_for_point(
                        tile_def.layer_kind(),
                        args.cursor_screen_pos,
                        args.camera.transform());

                    if target_cell.is_valid() {
                        let query = args.sim.new_query(args.world, args.tile_map, args.delta_time_secs);
                        Spawner::new(&query).try_spawn_tile_with_def(target_cell, tile_def).is_ok()
                    } else {
                        false
                    }
                } else {
                    let query = args.sim.new_query(args.world, args.tile_map, args.delta_time_secs);
                    if let Some(tile) = query.tile_map().topmost_tile_at_cursor(args.cursor_screen_pos, args.camera.transform()) {
                        Spawner::new(&query).despawn_tile(tile);
                        true
                    } else {
                        false
                    }
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

    fn end_frame(&mut self, args: &mut DebugMenusFrameArgs) {
        let has_valid_placement = args.tile_selection.has_valid_placement();
        let show_cursor_pos = self.debug_settings_menu.show_cursor_pos();
        let show_screen_origin = self.debug_settings_menu.show_screen_origin();
        let show_render_stats = self.debug_settings_menu.show_render_stats();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let show_log_viewer_window = self.debug_settings_menu.show_log_viewer_window();

        let game_loop = GameLoop::get_mut();

        if *show_log_viewer_window {
            let log_viewer = game_loop.engine().log_viewer();
            log_viewer.show(true);
            *show_log_viewer_window = log_viewer.draw(args.ui_sys);
        }

        let mut context = sim::debug::DebugContext {
            ui_sys: args.ui_sys,
            world: args.world,
            systems: args.systems,
            tile_map: args.tile_map,
            transform: args.camera.transform(),
            delta_time_secs: args.delta_time_secs,
        };

        self.tile_palette_menu.draw(
            &mut context,
            game_loop.engine_mut().debug_draw(),
            args.cursor_screen_pos,
            has_valid_placement,
            show_selection_bounds);

        self.tile_inspector_menu.draw(
            &mut context,
            args.sim);

        self.debug_settings_menu.draw(
            &mut context,
            args.sim,
            args.camera,
            game_loop);

        if show_popup_messages() {
            args.sim.draw_game_object_debug_popups(&mut context, args.visible_range);
        }

        if self.search_test_mode {
            utils::draw_cursor_overlay(
                args.ui_sys,
                args.camera.transform(),
                Some(&format!("Search Test: {} -> {}", self.search_test_start, self.search_test_goal)));
        }

        if show_cursor_pos {
            utils::draw_cursor_overlay(args.ui_sys, args.camera.transform(), None);
        }

        if show_render_stats {
            let engine = game_loop.engine();
            utils::draw_render_stats(args.ui_sys, engine.render_stats(), engine.tile_map_render_stats());
        }

        if show_screen_origin {
            let engine = game_loop.engine_mut();
            utils::draw_screen_origin_marker(engine.debug_draw());
        }
    }
}

// ----------------------------------------------
// DebugMenusSingleton Instance
// ----------------------------------------------

singleton_late_init! { DEBUG_MENUS_SINGLETON, DebugMenusSingleton }

fn init_singleton_once(tex_cache: &mut dyn TextureCache) {
    if DEBUG_MENUS_SINGLETON.is_initialized() {
        return; // Already initialized.
    }

    const DEBUG_SETTINGS_OPEN: bool = false;
    const TILE_PALETTE_OPEN:   bool = true;

    DEBUG_MENUS_SINGLETON.initialize(
        DebugMenusSingleton::new(tex_cache, DEBUG_SETTINGS_OPEN, TILE_PALETTE_OPEN)
    );
}

// ----------------------------------------------
// Global Debug Popups Switch
// ----------------------------------------------

static SHOW_DEBUG_POPUP_MESSAGES: AtomicBool = AtomicBool::new(false);

pub fn set_show_popup_messages(show: bool) {
    SHOW_DEBUG_POPUP_MESSAGES.store(show, Ordering::Relaxed);
}

pub fn show_popup_messages() -> bool {
    SHOW_DEBUG_POPUP_MESSAGES.load(Ordering::Relaxed)
}

// ----------------------------------------------
// Global TileMap Debug Pointer
// ----------------------------------------------

struct TileMapRawPtr(mem::RawPtr<TileMap>);

impl TileMapRawPtr {
    fn new(tile_map: &TileMap) -> Self {
        Self(mem::RawPtr::from_ref(tile_map))
    }
}

// Using this to get tile names from cells directly for debugging & logging.
// SAFETY: Must make sure the tile map pointer set on initialization stays
// valid until app termination or until it is reset.
static TILE_MAP_DEBUG_PTR: mem::SingleThreadStatic<Option<TileMapRawPtr>> = mem::SingleThreadStatic::new(None);

fn register_tile_map_debug_callbacks(tile_map: &mut TileMap) {
    TILE_MAP_DEBUG_PTR.set(Some(TileMapRawPtr::new(tile_map)));

    tile_map.set_tile_placed_callback(Some(|tile, did_reallocate| {
        DebugMenusSingleton::get_mut().tile_inspector_menu.on_tile_placed(tile, did_reallocate);
    }));

    tile_map.set_removing_tile_callback(Some(|tile| {
        DebugMenusSingleton::get_mut().tile_inspector_menu.on_removing_tile(tile);
    }));

    tile_map.set_map_reset_callback(Some(|_| {
        DebugMenusSingleton::get_mut().tile_inspector_menu.close();
    }));
}

pub fn tile_name_at(cell: Cell, layer: TileMapLayerKind) -> &'static str {
    if let Some(tile_map) = TILE_MAP_DEBUG_PTR.as_ref() {
        return tile_map.0.try_tile_from_layer(cell, layer)
            .map_or("", |tile| tile.name());
    }
    ""
}
