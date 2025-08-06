use std::cell::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    imgui_ui::{UiSystem, UiInputEvent},
    render::{RenderStats, RenderSystem, TextureCache},
    app::input::{MouseButton, InputAction, InputKey, InputModifiers},
    utils::{
        Vec2,
        UnsafeWeakRef,
        UnsafeMutable,
        coords::{Cell, CellRange, WorldToScreenTransform}
    },
    tile::{
        camera::Camera,
        placement::PlacementOp,
        selection::TileSelection,
        sets::{TileSets, TileKind},
        map::{TileMap, TileMapLayerKind, TileFlags},
        rendering::{TileMapRenderer, TileMapRenderStats, TileMapRenderFlags},
    },
    game::{
        sim::{self, Simulation, world::World}
    },
    pathfind::{
        Node,
        NodeKind,
        Graph,
        AStarUniformCostHeuristic,
        Search,
        SearchResult
    }
};

pub mod inspector;
pub mod palette;
pub mod popups;
pub mod settings;
pub mod utils;

use inspector::TileInspectorMenu;
use palette::TilePaletteMenu;
use settings::DebugSettingsMenu;

// ----------------------------------------------
// Args helper structs
// ----------------------------------------------

pub struct DebugMenusOnInputArgs<'world, 'config, 'tile_map, 'tile_sets> {
    pub world: &'world mut World<'config>,
    pub tile_map: &'tile_map mut TileMap<'tile_sets>,
    pub tile_selection: &'tile_map mut TileSelection,
    pub transform: WorldToScreenTransform,
    pub cursor_screen_pos: Vec2,
}

pub struct DebugMenusBeginFrameArgs<'sim, 'ui, 'world, 'config, 'tile_map, 'tile_sets> {
    pub ui_sys: &'ui UiSystem,
    pub sim: &'sim mut Simulation<'config>,
    pub world: &'world mut World<'config>,
    pub tile_map: &'tile_map mut TileMap<'tile_sets>,
    pub tile_selection: &'tile_map mut TileSelection,
    pub tile_sets: &'tile_sets TileSets,
    pub transform: WorldToScreenTransform,
    pub cursor_screen_pos: Vec2,
}

pub struct DebugMenusEndFrameArgs<'rs, 'cam, 'sim, 'ui, 'world, 'config, 'tile_map, 'tile_sets, RS: RenderSystem> {
    pub context: sim::debug::DebugContext<'config, 'ui, 'world, 'tile_map, 'tile_sets>,
    pub sim: &'sim mut Simulation<'config>,
    pub camera: &'cam mut Camera,
    pub render_sys: &'rs mut RS,
    pub render_sys_stats: &'rs RenderStats,
    pub tile_map_renderer: &'rs mut TileMapRenderer,
    pub tile_render_stats: &'rs TileMapRenderStats,
    pub tile_selection: &'tile_map TileSelection,
    pub visible_range: CellRange,
    pub cursor_screen_pos: Vec2,
}

// ----------------------------------------------
// DebugMenusSystem
// ----------------------------------------------

pub struct DebugMenusSystem;

impl DebugMenusSystem {
    pub fn new(tile_map: &mut TileMap, tex_cache: &mut impl TextureCache) -> Self {
        const DEBUG_SETTINGS_OPEN: bool = false;
        const TILE_PALETTE_OPEN: bool = true;

        // Initialize the singleton exactly once:
        init_singleton(tex_cache, DEBUG_SETTINGS_OPEN, TILE_PALETTE_OPEN);

        init_tile_map_debug_ref(tile_map);

        // Register TileMap global callbacks:
        tile_map.set_tile_placed_callback(Some(|tile, did_reallocate| {
            use_singleton(|instance| {
                instance.tile_inspector_menu.on_tile_placed(tile, did_reallocate);
            });
        }));

        tile_map.set_removing_tile_callback(Some(|tile| {
            use_singleton(|instance| {
                instance.tile_inspector_menu.on_removing_tile(tile);
            });
        }));

        tile_map.set_map_reset_callback(Some(|_| {
            use_singleton(|instance| {
                instance.tile_inspector_menu.close();
            });
        }));

        Self
    }

    pub fn on_key_input(&mut self,
                        args: &mut DebugMenusOnInputArgs,
                        key: InputKey,
                        action: InputAction) -> UiInputEvent {
        use_singleton(|instance| {
            instance.on_key_input(args, key, action)
        })
    }

    pub fn on_mouse_click(&mut self,
                          args: &mut DebugMenusOnInputArgs,
                          button: MouseButton,
                          action: InputAction,
                          modifiers: InputModifiers) -> UiInputEvent {
        use_singleton(|instance| {
            instance.on_mouse_click(args, button, action, modifiers)
        })
    }

    pub fn begin_frame(&mut self, args: &mut DebugMenusBeginFrameArgs) -> TileMapRenderFlags {
        use_singleton(|instance| {
            instance.begin_frame(args)
        })
    }

    pub fn end_frame(&mut self, args: &mut DebugMenusEndFrameArgs<impl RenderSystem>) {
        use_singleton(|instance| {
            instance.end_frame(args)
        })
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
                    args: &mut DebugMenusOnInputArgs,
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
                    println!("Found a path with {} nodes.", path.len());
                    debug_assert!(!path.is_empty());

                    // Highlight path tiles:
                    for node in path {
                        if let Some(tile) = args.tile_map.try_tile_from_layer_mut(node.cell, TileMapLayerKind::Terrain) {
                            tile.set_flags(TileFlags::Highlighted, true);
                        }
                    }

                    // Make unit follow path:
                    if let Some(ped) = args.world.find_unit_by_name_mut("Ped") {
                        // First teleport it to the start cell of the path:
                        ped.teleport(args.tile_map, path[0].cell);
                        ped.follow_path(Some(path));
                    }
                },
                SearchResult::PathNotFound => println!("No path could be found."),
            }

            return UiInputEvent::Handled;
        }

        UiInputEvent::NotHandled
    }

    fn on_mouse_click(&mut self,
                      args: &mut DebugMenusOnInputArgs,
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
                        &args.transform);
                    self.search_test_start = cursor_cell;
                } else if !self.search_test_goal.is_valid() {
                    let cursor_cell = args.tile_map.find_exact_cell_for_point(
                        TileMapLayerKind::Terrain,
                        args.cursor_screen_pos,
                        &args.transform);
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
                    let target_cell = args.tile_map.find_exact_cell_for_point(
                        tile_def.layer_kind(),
                        args.cursor_screen_pos,
                        &args.transform);

                    if target_cell.is_valid() {
                        if tile_def.is(TileKind::Building) {
                            args.world.try_spawn_building_with_tile_def(args.tile_map, target_cell, tile_def).is_ok()
                        } else if tile_def.is(TileKind::Unit) {
                            args.world.try_spawn_unit_with_tile_def(args.tile_map, target_cell, tile_def).is_ok()
                        } else {
                            // No associated world state, place plain tile.
                            args.tile_map.try_place_tile(target_cell, tile_def).is_ok()
                        }
                    } else {
                        false
                    }
                } else if let Some(tile) = args.tile_map.topmost_tile_at_cursor(args.cursor_screen_pos, &args.transform) {
                    if tile.is(TileKind::Building | TileKind::Blocker) {
                        args.world.despawn_building_at_cell(args.tile_map, tile.base_cell())
                            .expect("Tile removal failed!");
                    } else if tile.is(TileKind::Unit) {
                        args.world.despawn_unit_at_cell(args.tile_map, args.sim.task_manager(), tile.base_cell())
                            .expect("Tile removal failed!");
                    } else {
                        // No world state, just remove the tile directly.
                        args.tile_map.try_clear_tile_at_cursor(args.cursor_screen_pos, &args.transform)
                            .expect("Tile removal failed!");
                    }
                    true
                } else {
                    false
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
            args.sim,
            args.camera,
            args.tile_map_renderer);

        args.sim.draw_building_debug_popups(&mut args.context, args.visible_range);
        args.sim.draw_unit_debug_popups(&mut args.context, args.visible_range);

        if self.search_test_mode {
            self::utils::draw_cursor_overlay(
                args.context.ui_sys,
                args.camera.transform(),
                Some(&format!("Search Test: {} -> {}", self.search_test_start, self.search_test_goal)));
        }

        if show_cursor_pos {
            self::utils::draw_cursor_overlay(args.context.ui_sys, args.camera.transform(), None);
        }

        if show_render_stats {
            self::utils::draw_render_stats(args.context.ui_sys, args.render_sys_stats, args.tile_render_stats);
        }

        if show_screen_origin {
            self::utils::draw_screen_origin_marker(args.render_sys);
        }
    }
}

// ----------------------------------------------
// DebugMenusSingleton Instance
// ----------------------------------------------

std::thread_local! {
    static DEBUG_MENUS_SINGLETON: OnceCell<UnsafeMutable<DebugMenusSingleton>> = const { OnceCell::new() };
}

fn init_singleton(tex_cache: &mut impl TextureCache, debug_settings_open: bool, tile_palette_open: bool) {
    DEBUG_MENUS_SINGLETON.with(|once_cell| {
        once_cell.set(UnsafeMutable::new(DebugMenusSingleton::new(tex_cache, debug_settings_open, tile_palette_open)))
            .unwrap_or_else(|_| panic!("DebugMenusSystem was already initialized! Only one instance permitted."));
    });
}

fn use_singleton<F, R>(mut closure: F) -> R
    where F:
        FnMut(&mut DebugMenusSingleton) -> R
{
    DEBUG_MENUS_SINGLETON.with(|once_cell| {
        if let Some(instance) = once_cell.get() {
            closure(instance.as_mut())
        } else {
            panic!("DebugMenusSystem singleton instance is not initialized!")
        }
    })
}

static SHOW_DEBUG_POPUP_MESSAGES: AtomicBool = AtomicBool::new(false);

pub fn set_show_popup_messages(show: bool) {
    SHOW_DEBUG_POPUP_MESSAGES.store(show, Ordering::SeqCst);
}

pub fn show_popup_messages() -> bool {
    SHOW_DEBUG_POPUP_MESSAGES.load(Ordering::SeqCst)
}

// ----------------------------------------------
// Global TileMap debug ref
// ----------------------------------------------

std::thread_local! {
    // Using this to get tile names from cells directly for debugging & logging.
    // SAFETY: Must make sure the tile map instance set on initialization stays valid until app termination.
    static TILE_MAP_DEBUG_REF: OnceCell<UnsafeWeakRef<TileMap<'static>>> = const { OnceCell::new() };
}

fn init_tile_map_debug_ref(tile_map: &TileMap) {
    // Strip away lifetime (pretend it is static).
    #[allow(clippy::unnecessary_cast)] // cast to TileMap<'_> is needed to then cast away lifetime as 'static.
    let tile_map_ptr = tile_map as *const TileMap<'_> as *const TileMap<'static>;
    let weak_ref = UnsafeWeakRef::from_ptr(tile_map_ptr);

    TILE_MAP_DEBUG_REF.with(|once_cell| {
        once_cell.set(weak_ref).unwrap_or_else(|_| panic!("TILE_MAP_DEBUG_REF was already set!"));
    });
}

pub fn tile_name_at(cell: Cell, layer: TileMapLayerKind) -> &'static str {
    TILE_MAP_DEBUG_REF.with(|once_cell| {
        if let Some(tile_map) = once_cell.get() {
            return tile_map.try_tile_from_layer(cell, layer)
                .map_or("", |tile| tile.name());
        }
        ""
    })
}
