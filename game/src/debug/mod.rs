use std::sync::atomic::{AtomicBool, Ordering};
use std::any::Any;

use inspector::TileInspectorDevMenu;
use palette::TilePaletteDevMenu;
use settings::DebugSettingsDevMenu;

use crate::{
    singleton_late_init,
    render::TextureCache,
    ui::{self, UiTheme, widgets::UiWidgetContext},
    save::{Load, PreLoadContext, PostLoadContext, Save},
    game::{sim, config::GameConfigs, GameLoop, menu::*},
    utils::{coords::{Cell, CellRange}, mem::{self, SingleThreadStatic}},
    tile::{rendering::TileMapRenderFlags, TileMap, TileMapLayerKind, minimap::DevUiMinimapRenderer},
};

pub mod log_viewer;
pub mod popups;
pub mod utils;

mod inspector;
mod palette;
mod settings;

// ----------------------------------------------
// DevEditorMenus
// ----------------------------------------------

pub struct DevEditorMenus;

impl DevEditorMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::Dev);
        // Register TileMap global callbacks & debug ref:
        register_tile_map_debug_callbacks(context.tile_map);
        Self
    }
}

impl GameMenusSystem for DevEditorMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenusMode {
        GameMenusMode::DevEditor
    }

    fn tile_placement(&mut self) -> Option<&mut TilePlacement> {
        Some(&mut DevEditorMenusSingleton::get_mut().tile_placement)
    }

    fn tile_palette(&mut self) -> Option<&mut dyn TilePalette> {
        Some(&mut DevEditorMenusSingleton::get_mut().tile_palette_menu)
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        let singleton = DevEditorMenusSingleton::get_mut();
        if singleton.enable_tile_inspector {
            Some(&mut singleton.tile_inspector_menu)
        } else {
            None
        }
    }

    fn selected_render_flags(&self) -> TileMapRenderFlags {
        DevEditorMenusSingleton::get().debug_settings_menu.selected_render_flags()
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, visible_range: CellRange) {
        DevEditorMenusSingleton::get_mut().draw_debug_menus(context, visible_range);
    }
}

// ----------------------------------------------
// Drop for DevEditorMenus
// ----------------------------------------------

impl Drop for DevEditorMenus {
    fn drop(&mut self) {
        // Make sure tile inspector is closed.
        DevEditorMenusSingleton::get_mut().close_tile_inspector();

        // Clear the cached global tile map ptr.
        TILE_MAP_DEBUG_PTR.set(None);
    }
}

// ----------------------------------------------
// Save/Load for DevEditorMenus
// ----------------------------------------------

impl Save for DevEditorMenus {}

impl Load for DevEditorMenus {
    fn pre_load(&mut self, _context: &PreLoadContext) {
        // Make sure tile inspector is closed.
        DevEditorMenusSingleton::get_mut().close_tile_inspector();

        // Clear all registered callbacks and global tile map ref.
        remove_tile_map_debug_callbacks();
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        // Make sure tile inspector is closed.
        DevEditorMenusSingleton::get_mut().close_tile_inspector();

        // Re-register debug editor callbacks and reset the global tile map ref.
        register_tile_map_debug_callbacks(context.tile_map_mut());
    }
}

// ----------------------------------------------
// DevEditorMenusSingleton
// ----------------------------------------------

struct DevEditorMenusSingleton {
    tile_placement: TilePlacement,
    debug_settings_menu: DebugSettingsDevMenu,
    tile_palette_menu: TilePaletteDevMenu,
    tile_inspector_menu: TileInspectorDevMenu,
    enable_tile_inspector: bool,
    minimap_renderer: DevUiMinimapRenderer,
}

impl DevEditorMenusSingleton {
    fn new(tex_cache: &mut dyn TextureCache, tile_palette_open: bool, enable_tile_inspector: bool) -> Self {
        Self {
            tile_placement: TilePlacement::new(),
            debug_settings_menu: DebugSettingsDevMenu::new(),
            tile_palette_menu: TilePaletteDevMenu::new(tile_palette_open, tex_cache),
            tile_inspector_menu: TileInspectorDevMenu::default(),
            enable_tile_inspector,
            minimap_renderer: DevUiMinimapRenderer::new(),
        }
    }

    fn close_tile_inspector(&mut self) {
        self.tile_inspector_menu.close();
    }

    fn draw_debug_menus(&mut self, menu_context: &mut GameMenusContext, visible_range: CellRange) {
        let has_valid_placement = menu_context.tile_selection.has_valid_placement();
        let show_cursor_pos = self.debug_settings_menu.show_cursor_pos();
        let show_screen_origin = self.debug_settings_menu.show_screen_origin();
        let show_sample_menus = self.debug_settings_menu.show_sample_menus();
        let show_render_perf_stats = self.debug_settings_menu.show_render_perf_stats();
        let show_world_perf_stats = self.debug_settings_menu.show_world_perf_stats();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let show_log_viewer_window = self.debug_settings_menu.show_log_viewer_window();

        let game_loop = GameLoop::get_mut();
        let engine = GameLoop::get_mut().engine_mut();

        if *show_log_viewer_window {
            let log_viewer = engine.log_viewer();
            log_viewer.show(true);
            *show_log_viewer_window = log_viewer.draw(menu_context.engine.ui_system());
        }

        let mut sim_context = sim::debug::DebugContext {
            ui_sys: engine.ui_system(),
            world: menu_context.world,
            systems: menu_context.systems,
            tile_map: menu_context.tile_map,
            transform: menu_context.camera.transform(),
            delta_time_secs: menu_context.delta_time_secs
        };

        self.tile_palette_menu.draw(&mut sim_context,
                                    menu_context.sim,
                                    menu_context.engine.debug_draw(),
                                    menu_context.cursor_screen_pos,
                                    has_valid_placement,
                                    show_selection_bounds);

        self.debug_settings_menu.draw(&mut sim_context,
                                      menu_context.sim,
                                      game_loop,
                                      &mut self.enable_tile_inspector);

        if self.enable_tile_inspector {
            self.tile_inspector_menu.draw(&mut sim_context, menu_context.sim);
        }

        if show_sample_menus {
            let mut ui_context = UiWidgetContext::new(
                menu_context.sim,
                sim_context.world,
                sim_context.tile_map,
                engine
            );
            ui::tests::draw_sample_menus(&mut ui_context);
        }

        if show_popup_messages() {
            menu_context.sim.draw_game_object_debug_popups(&mut sim_context, visible_range);
        }

        sim_context.tile_map.minimap_mut().draw(&mut self.minimap_renderer,
                                                engine.render_system(),
                                                menu_context.camera,
                                                sim_context.ui_sys);

        game_loop.camera().draw_debug(menu_context.engine.debug_draw(), sim_context.ui_sys);

        if show_cursor_pos {
            utils::draw_cursor_overlay(engine.ui_system(),
                                       menu_context.camera.transform(),
                                       menu_context.cursor_screen_pos,
                                       None);
        }

        if show_render_perf_stats {
            utils::draw_render_perf_stats(engine.ui_system(),
                                          engine.render_stats(),
                                          engine.tile_map_render_stats());
        }

        if show_world_perf_stats {
            utils::draw_world_perf_stats(engine.ui_system(),
                                         menu_context.world,
                                         menu_context.tile_map,
                                         visible_range);
        }

        if show_screen_origin {
            utils::draw_screen_origin_marker(engine.debug_draw());
        }
    }
}

// ----------------------------------------------
// DevEditorMenusSingleton Instance
// ----------------------------------------------

singleton_late_init! { DEV_EDITOR_MENUS_SINGLETON, DevEditorMenusSingleton }

pub fn init_dev_editor_menus(configs: &GameConfigs, tex_cache: &mut dyn TextureCache) {
    if DEV_EDITOR_MENUS_SINGLETON.is_initialized() {
        return; // Already initialized.
    }

    DEV_EDITOR_MENUS_SINGLETON.initialize(
        DevEditorMenusSingleton::new(
            tex_cache,
            configs.debug.tile_palette_open,
            configs.debug.enable_tile_inspector)
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
static TILE_MAP_DEBUG_PTR: SingleThreadStatic<Option<TileMapRawPtr>> = SingleThreadStatic::new(None);

fn register_tile_map_debug_callbacks(tile_map: &mut TileMap) {
    TILE_MAP_DEBUG_PTR.set(Some(TileMapRawPtr::new(tile_map)));

    tile_map.set_tile_placed_callback(Some(|tile, did_reallocate| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.on_tile_placed(tile, did_reallocate);
    }));

    tile_map.set_removing_tile_callback(Some(|tile| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.on_removing_tile(tile);
    }));

    tile_map.set_map_reset_callback(Some(|_| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.close();
    }));
}

fn remove_tile_map_debug_callbacks() {
    if let Some(tile_map) = TILE_MAP_DEBUG_PTR.as_mut() {
        tile_map.0.set_tile_placed_callback(None);
        tile_map.0.set_removing_tile_callback(None);
        tile_map.0.set_map_reset_callback(None);
    }

    // Clear the cached global tile map ptr.
    TILE_MAP_DEBUG_PTR.set(None);
}

pub fn tile_name_at(cell: Cell, layer: TileMapLayerKind) -> &'static str {
    if let Some(tile_map) = TILE_MAP_DEBUG_PTR.as_ref() {
        return tile_map.0.try_tile_from_layer(cell, layer).map_or("", |tile| tile.name());
    }
    ""
}
