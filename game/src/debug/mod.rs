use std::sync::atomic::{AtomicBool, Ordering};

use inspector::TileInspectorMenu;
use palette::TilePaletteMenu;
use settings::DebugSettingsMenu;

use crate::{
    singleton_late_init,
    app::input::{InputAction, InputKey, InputModifiers, MouseButton},
    engine::time::Seconds,
    game::{
        config::GameConfigs,
        sim::{self, Simulation},
        system::GameSystems,
        world::{object::{Spawner, SpawnerResult}, World},
        GameLoop,
    },
    imgui_ui::{UiInputEvent, UiSystem},
    render::TextureCache,
    save::{Load, PostLoadContext, Save},
    tile::{
        camera::Camera, rendering::TileMapRenderFlags, selection::TileSelection, PlacementOp,
        TileKind, TileMap, TileMapLayerKind,
        road::{self, RoadSegment},
        water,
    },
    utils::{
        coords::{Cell, CellRange, WorldToScreenTransform},
        mem, Vec2,
    },
};

pub mod log_viewer;
pub mod popups;
pub mod utils;

mod inspector;
mod palette;
mod settings;

// ----------------------------------------------
// Args helper structs
// ----------------------------------------------

pub struct DebugMenusInputArgs<'game> {
    pub sim: &'game mut Simulation,
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
        init_debug_menus_singleton_once(tex_cache);

        // Register TileMap global callbacks & debug ref:
        register_tile_map_debug_callbacks(tile_map);

        Self
    }

    pub fn on_key_input(&mut self,
                        args: &mut DebugMenusInputArgs,
                        key: InputKey,
                        action: InputAction)
                        -> UiInputEvent {
        DebugMenusSingleton::get_mut().on_key_input(args, key, action)
    }

    pub fn on_mouse_click(&mut self,
                          args: &mut DebugMenusInputArgs,
                          button: MouseButton,
                          action: InputAction,
                          modifiers: InputModifiers)
                          -> UiInputEvent {
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

impl Save for DebugMenusSystem {}

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
    enable_tile_inspector: bool,
    current_road_segment: RoadSegment, // For road placement.
}

impl DebugMenusSingleton {
    fn new(tex_cache: &mut dyn TextureCache, tile_palette_open: bool, enable_tile_inspector: bool) -> Self {
        Self {
            debug_settings_menu: DebugSettingsMenu::new(),
            tile_palette_menu: TilePaletteMenu::new(tile_palette_open, tex_cache),
            enable_tile_inspector,
            ..Default::default()
        }
    }

    fn on_key_input(&mut self,
                    args: &mut DebugMenusInputArgs,
                    key: InputKey,
                    action: InputAction)
                    -> UiInputEvent {
        if key == InputKey::Escape && action == InputAction::Press {
            self.tile_inspector_menu.close();
            self.tile_palette_menu.clear_selection();
            args.tile_map.clear_selection(args.tile_selection);
            return UiInputEvent::Handled;
        }

        UiInputEvent::NotHandled
    }

    fn on_mouse_click(&mut self,
                      args: &mut DebugMenusInputArgs,
                      button: MouseButton,
                      action: InputAction,
                      _modifiers: InputModifiers)
                      -> UiInputEvent {
        if self.tile_palette_menu.has_selection() && !self.tile_palette_menu.is_road_tile_selected() {
            if self.tile_palette_menu.on_mouse_click(button, action).not_handled() {
                self.tile_palette_menu.clear_selection();
                args.tile_map.clear_selection(args.tile_selection);
            }
        } else {
            if args.tile_selection
                   .on_mouse_click(button, action, args.tile_map, args.cursor_screen_pos, args.transform)
                   .not_handled()
            {
                // Place road segment if valid & we can afford it:
                let is_valid_road_placement =
                    self.current_road_segment.is_valid &&
                    args.sim.treasury().can_afford(args.world, self.current_road_segment.cost());

                if is_valid_road_placement {
                    let query = args.sim.new_query(args.world, args.tile_map, 0.0);
                    let spawner = Spawner::new(&query);

                    // Place tiles:
                    for cell in &self.current_road_segment.path {
                        spawner.try_spawn_tile_with_def(*cell, self.current_road_segment.tile_def());
                    }

                    // Update road junctions (each junction is a different variation of the same tile).
                    for cell in &self.current_road_segment.path {
                        road::update_junctions(args.tile_map, *cell);
                    }
                } else {
                    self.tile_palette_menu.clear_selection();
                }

                // Clear road segment highlight:
                road::mark_tiles(args.tile_map, &self.current_road_segment, false, false);
                self.current_road_segment.clear();

                args.tile_map.clear_selection(args.tile_selection);
            }

            // Open inspector only if we're not in road placement mode.
            if !self.tile_palette_menu.is_road_tile_selected() && self.enable_tile_inspector {
                if let Some(selected_tile) = args.tile_map.topmost_selected_tile(args.tile_selection) {
                    if self.tile_inspector_menu
                        .on_mouse_click(button, action, selected_tile)
                        .is_handled()
                    {
                        return UiInputEvent::Handled;
                    }
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
                    let query = args.sim.new_query(args.world, args.tile_map, args.delta_time_secs);
                    if Spawner::new(&query).can_afford_tile(tile_def) {
                        PlacementOp::Place(tile_def)
                    } else {
                        PlacementOp::Invalidate(tile_def)
                    }
                } else if self.tile_palette_menu.is_clear_selected() {
                    PlacementOp::Clear
                } else {
                    PlacementOp::None
                }
            };

            args.tile_map.update_selection(args.tile_selection,
                                           args.cursor_screen_pos,
                                           args.camera.transform(),
                                           placement_op);

            if self.tile_palette_menu.is_road_tile_selected() {
                if let Some((start, end)) = args.tile_selection.range_selection_cells(
                    args.tile_map,
                    args.cursor_screen_pos,
                    args.camera.transform())
                {
                    // Clear previous segment highlight:
                    road::mark_tiles(args.tile_map, &self.current_road_segment, false, false);

                    let road_kind = self.tile_palette_menu.selected_road_kind();
                    self.current_road_segment = road::build_segment(args.tile_map, start, end, road_kind);

                    let is_valid_road_placement =
                        self.current_road_segment.is_valid &&
                        args.sim.treasury().can_afford(args.world, self.current_road_segment.cost());

                    // Highlight new segment:
                    road::mark_tiles(args.tile_map, &self.current_road_segment, true, is_valid_road_placement);
                }
            }
        }

        if self.tile_palette_menu.can_place_tile() && !self.tile_palette_menu.is_road_tile_selected() {
            let placement_candidate = self.tile_palette_menu.current_selection();

            let did_place_or_clear = {
                // If we have a selection place it, otherwise we want to try clearing the tile
                // under the cursor.
                if let Some(tile_def) = placement_candidate {
                    let target_cell = args.tile_map
                                          .find_exact_cell_for_point(tile_def.layer_kind(),
                                                                     args.cursor_screen_pos,
                                                                     args.camera.transform());

                    if target_cell.is_valid() {
                        let query = args.sim.new_query(args.world, args.tile_map, args.delta_time_secs);
                        let spawner = Spawner::new(&query);
                        let spawn_result = spawner.try_spawn_tile_with_def(target_cell, tile_def);
                        match &spawn_result {
                            SpawnerResult::Tile(tile) if tile.is(TileKind::Terrain) => {
                                // In case we've replaced a road tile with terrain.
                                road::update_junctions(args.tile_map, target_cell);
                                // In case we've placed a water tile or replaced water with terrain.
                                water::update_transitions(args.tile_map, target_cell);
                            }
                            SpawnerResult::Building(_) if water::is_port_or_wharf(tile_def) => {
                                // If we've placed a port/wharf, select the correct
                                // tile orientation in relation to the water.
                                water::update_port_wharf_orientation(args.tile_map, target_cell);
                            }
                            _ => {}
                        }
                        spawn_result.is_ok()
                    } else {
                        false
                    }
                } else {
                    // Clear/remove tile:
                    let query = args.sim.new_query(args.world, args.tile_map, args.delta_time_secs);
                    if let Some(tile) = query.tile_map().topmost_tile_at_cursor(args.cursor_screen_pos,
                                                                                args.camera.transform())
                    {
                        let is_road  = tile.path_kind().is_road();
                        let is_water = tile.path_kind().is_water();
                        let target_cell = tile.base_cell();
                        Spawner::new(&query).despawn_tile(tile);
                        // Update road junctions / water transitions around the removed tile cell.
                        if is_road {
                            road::update_junctions(args.tile_map, target_cell);
                        }
                        if is_water {
                            water::update_transitions(args.tile_map, target_cell);
                        }
                        true
                    } else {
                        false
                    }
                }
            };

            let placing_building_or_unit =
                placement_candidate
                    .is_some_and(|def| def.is(TileKind::Building | TileKind::Unit));

            let clearing_a_tile = self.tile_palette_menu.is_clear_selected();

            if did_place_or_clear && (placing_building_or_unit || clearing_a_tile) {
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
        let show_render_perf_stats = self.debug_settings_menu.show_render_perf_stats();
        let show_world_perf_stats = self.debug_settings_menu.show_world_perf_stats();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let show_log_viewer_window = self.debug_settings_menu.show_log_viewer_window();

        let game_loop = GameLoop::get_mut();

        if *show_log_viewer_window {
            let log_viewer = game_loop.engine_mut().log_viewer();
            log_viewer.show(true);
            *show_log_viewer_window = log_viewer.draw(args.ui_sys);
        }

        let mut context = sim::debug::DebugContext {
            ui_sys: args.ui_sys,
            world: args.world,
            systems: args.systems,
            tile_map: args.tile_map,
            transform: args.camera.transform(),
            delta_time_secs: args.delta_time_secs
        };

        self.tile_palette_menu.draw(&mut context,
                                    game_loop.engine_mut().debug_draw(),
                                    args.cursor_screen_pos,
                                    has_valid_placement,
                                    show_selection_bounds);

        self.debug_settings_menu.draw(&mut context,
                                      args.sim,
                                      args.camera,
                                      game_loop,
                                      &mut self.enable_tile_inspector);

        if self.enable_tile_inspector {
            self.tile_inspector_menu.draw(&mut context, args.sim);
        }

        if show_popup_messages() {
            args.sim.draw_game_object_debug_popups(&mut context, args.visible_range);
        }

        if show_cursor_pos {
            utils::draw_cursor_overlay(args.ui_sys, args.camera.transform(), None);
        }

        if show_render_perf_stats {
            let engine = game_loop.engine();
            utils::draw_render_perf_stats(args.ui_sys,
                                          engine.render_stats(),
                                          engine.tile_map_render_stats());
        }

        if show_world_perf_stats {
            utils::draw_world_perf_stats(args.ui_sys, args.world);
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

fn init_debug_menus_singleton_once(tex_cache: &mut dyn TextureCache) {
    if DEBUG_MENUS_SINGLETON.is_initialized() {
        return; // Already initialized.
    }

    let tile_palette_open = GameConfigs::get().debug.tile_palette_open;
    let enable_tile_inspector = GameConfigs::get().debug.enable_tile_inspector;

    DEBUG_MENUS_SINGLETON.initialize(DebugMenusSingleton::new(
        tex_cache, tile_palette_open, enable_tile_inspector));
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
        return tile_map.0.try_tile_from_layer(cell, layer).map_or("", |tile| tile.name());
    }
    ""
}
