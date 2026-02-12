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
    utils::{coords::{Cell, CellRange}, mem::{SingleThreadStatic, RcMut, WeakMut}},
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
    pub fn new(context: &mut UiWidgetContext, tile_map_rc: RcMut<TileMap>) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::Dev);
        // Register TileMap global callbacks & debug ref:
        register_tile_map_debug_callbacks(tile_map_rc);
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

        // Clear the cached global tile map weak ref.
        TILE_MAP_DEBUG_REF.set(None);
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
        register_tile_map_debug_callbacks(context.tile_map_rc());
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

        let ui_sys = GameLoop::get().engine().ui_system();
        let debug_draw = GameLoop::get_mut().engine_mut().debug_draw();

        if *show_log_viewer_window {
            let log_viewer = GameLoop::get_mut().engine_mut().log_viewer();
            log_viewer.show(true);
            *show_log_viewer_window = log_viewer.draw(ui_sys);
        }

        {
            let mut sim_context = sim::debug::DebugContext {
                ui_sys,
                world: menu_context.world,
                systems: menu_context.systems,
                tile_map: menu_context.tile_map,
                transform: menu_context.camera.transform(),
                delta_time_secs: menu_context.delta_time_secs
            };

            self.tile_palette_menu.draw(&mut sim_context,
                                        menu_context.sim,
                                        debug_draw,
                                        menu_context.cursor_screen_pos,
                                        has_valid_placement,
                                        show_selection_bounds);

            self.debug_settings_menu.draw(&mut sim_context,
                                          menu_context.sim,
                                          GameLoop::get_mut(),
                                          &mut self.enable_tile_inspector);

            if self.enable_tile_inspector {
                self.tile_inspector_menu.draw(&mut sim_context, menu_context.sim);
            }

            if show_popup_messages() {
                menu_context.sim.draw_game_object_debug_popups(&mut sim_context, visible_range);
            }
        }

        if show_sample_menus {
            ui::tests::draw_sample_menus(&mut menu_context.as_ui_widget_context());
        }

        {
            let mut ui_context = UiWidgetContext::new(
                menu_context.sim,
                menu_context.world,
                menu_context.engine
            );
            let minimap = menu_context.tile_map.minimap_mut();
            minimap.draw(&mut self.minimap_renderer, &mut ui_context, menu_context.camera);
        }

        menu_context.camera.draw_debug(debug_draw, ui_sys);

        if show_cursor_pos {
            utils::draw_cursor_overlay(ui_sys,
                                       menu_context.camera.transform(),
                                       menu_context.cursor_screen_pos,
                                       None);
        }

        if show_render_perf_stats {
            utils::draw_render_perf_stats(ui_sys,
                                          menu_context.engine.render_stats(),
                                          menu_context.engine.tile_map_render_stats());
        }

        if show_world_perf_stats {
            utils::draw_world_perf_stats(ui_sys,
                                         menu_context.world,
                                         menu_context.tile_map,
                                         visible_range);
        }

        if show_screen_origin {
            utils::draw_screen_origin_marker(debug_draw);
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
// Global TileMap Debug Ref
// ----------------------------------------------

static TILE_MAP_DEBUG_REF: SingleThreadStatic<Option<WeakMut<TileMap>>> = SingleThreadStatic::new(None);

fn register_tile_map_debug_callbacks(mut tile_map_rc: RcMut<TileMap>) {
    tile_map_rc.set_tile_placed_callback(Some(|tile, did_reallocate| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.on_tile_placed(tile, did_reallocate);
    }));

    tile_map_rc.set_removing_tile_callback(Some(|tile| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.on_removing_tile(tile);
    }));

    tile_map_rc.set_map_reset_callback(Some(|_| {
        DevEditorMenusSingleton::get_mut().tile_inspector_menu.close();
    }));

    // Downgrade and store weak non-owning reference.
    TILE_MAP_DEBUG_REF.set(Some(tile_map_rc.downgrade()));
}

fn remove_tile_map_debug_callbacks() {
    if let Some(tile_map_weak_ref) = TILE_MAP_DEBUG_REF.as_mut() {
        if let Some(mut tile_map_strong_ref) = tile_map_weak_ref.upgrade() {
            tile_map_strong_ref.set_tile_placed_callback(None);
            tile_map_strong_ref.set_removing_tile_callback(None);
            tile_map_strong_ref.set_map_reset_callback(None);
        }
    }

    // Clear the cached global tile map weak ref.
    TILE_MAP_DEBUG_REF.set(None);
}

pub fn tile_name_at(cell: Cell, layer: TileMapLayerKind) -> &'static str {
    if let Some(tile_map_weak_ref) = TILE_MAP_DEBUG_REF.as_ref() {
        if let Some(tile_map_strong_ref) = tile_map_weak_ref.upgrade() {
            return tile_map_strong_ref.try_tile_from_layer(cell, layer)
                .map_or("", |tile| tile.name());
        }
    }
    ""
}
