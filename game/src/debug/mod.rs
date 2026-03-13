use std::sync::atomic::{AtomicBool, Ordering};
use std::any::Any;

use inspector::TileInspectorDevMenu;
use palette::TilePaletteDevMenu;
use settings::DebugSettingsDevMenu;
use log_viewer::LogViewerWindow;

use crate::{
    ui::{self, UiTheme, widgets::UiWidgetContext},
    save::{Load, PreLoadContext, PostLoadContext, Save},
    game::{config::GameConfigs, GameLoop, menu::*},
    utils::{coords::{Cell, CellRange}, mem::{SingleThreadStatic, RcMut, WeakMut, singleton_late_init}},
    tile::{rendering::TileMapRenderFlags, TileMap, TileMapLayerKind, minimap::{MinimapRenderer, DevUiMinimapRenderer}},
};

pub mod log_viewer;
pub mod popups;
pub mod utils;

mod inspector;
mod palette;
mod settings;

// ----------------------------------------------
// DebugUiMode
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DebugUiMode {
    Overview,
    Detailed,
}

// ----------------------------------------------
// DevEditorMenus
// ----------------------------------------------

pub struct DevEditorMenus;

impl DevEditorMenus {
    pub fn new(context: &mut UiWidgetContext, tile_map_rc: RcMut<TileMap>) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::Dev);
        init_dev_editor_menus_singleton_once(context);
        register_tile_map_debug_callbacks(tile_map_rc); // Register TileMap global callbacks & debug ref.
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
        if singleton.enable_dev_tile_inspector {
            Some(&mut singleton.tile_inspector_menu)
        } else {
            None
        }
    }

    fn selected_render_flags(&self) -> TileMapRenderFlags {
        DevEditorMenusSingleton::get().debug_settings_menu.selected_render_flags()
    }

    fn end_frame(&mut self, context: &mut UiWidgetContext, visible_range: CellRange) {
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
    fn pre_load(&mut self, _context: &mut PreLoadContext) {
        // Make sure tile inspector is closed.
        DevEditorMenusSingleton::get_mut().close_tile_inspector();

        // Clear all registered callbacks and global tile map ref.
        remove_tile_map_debug_callbacks();
    }

    fn post_load(&mut self, context: &mut PostLoadContext) {
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
    enable_dev_tile_inspector: bool,
    minimap_renderer: DevUiMinimapRenderer,
    log_viewer: LogViewerWindow,
}

impl DevEditorMenusSingleton {
    fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            tile_placement: TilePlacement::new(),
            debug_settings_menu: DebugSettingsDevMenu::new(),
            tile_palette_menu: TilePaletteDevMenu::new(context),
            tile_inspector_menu: TileInspectorDevMenu::default(),
            enable_dev_tile_inspector: GameConfigs::get().debug.enable_dev_tile_inspector,
            minimap_renderer: DevUiMinimapRenderer::new(context),
            log_viewer: LogViewerWindow::new(),
        }
    }

    fn close_tile_inspector(&mut self) {
        self.tile_inspector_menu.close();
    }

    fn draw_debug_menus(&mut self, context: &mut UiWidgetContext, visible_range: CellRange) {
        let show_cursor_pos = self.debug_settings_menu.show_cursor_pos();
        let show_screen_origin = self.debug_settings_menu.show_screen_origin();
        let show_sample_menus = self.debug_settings_menu.show_sample_menus();
        let show_render_perf_stats = self.debug_settings_menu.show_render_perf_stats();
        let show_world_perf_stats = self.debug_settings_menu.show_world_perf_stats();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let show_log_viewer_window = self.debug_settings_menu.show_log_viewer_window();

        let engine = GameLoop::get_mut().engine_mut();

        if *show_log_viewer_window {
            self.log_viewer.show(true);
            *show_log_viewer_window = self.log_viewer.draw(context.ui_sys);
        }

        self.tile_palette_menu.draw(context,
                                    engine.debug_draw_mut(),
                                    show_selection_bounds);

        self.debug_settings_menu.draw(context,
                                      &self.log_viewer,
                                      &mut self.enable_dev_tile_inspector);

        if self.enable_dev_tile_inspector {
            self.tile_inspector_menu.draw(context);
        }

        self.minimap_renderer.draw(context);
        context.camera.draw_debug(engine.debug_draw_mut(), context.ui_sys);

        if show_popup_messages() {
            GameLoop::get_mut().sim_mut().draw_game_object_debug_popups(context, visible_range);
        }

        if show_sample_menus {
            ui::tests::draw_sample_menus(context);
        }

        if show_cursor_pos {
            utils::draw_cursor_overlay(context.ui_sys,
                                       context.camera.transform(),
                                       context.cursor_screen_pos,
                                       None);
        }

        if show_render_perf_stats {
            utils::draw_render_perf_stats(context.ui_sys,
                                          engine.render_stats(),
                                          engine.tile_map_render_stats());
        }

        if show_world_perf_stats {
            utils::draw_world_perf_stats(context.ui_sys,
                                         context.world,
                                         context.tile_map,
                                         visible_range);
        }

        if show_screen_origin {
            utils::draw_screen_origin_marker(engine.debug_draw_mut());
        }
    }
}

// ----------------------------------------------
// DevEditorMenusSingleton Instance
// ----------------------------------------------

singleton_late_init! { DEV_EDITOR_MENUS_SINGLETON, DevEditorMenusSingleton }

fn init_dev_editor_menus_singleton_once(context: &mut UiWidgetContext) {
    if DevEditorMenusSingleton::is_initialized() {
        return; // Already initialized.
    }

    DevEditorMenusSingleton::initialize(DevEditorMenusSingleton::new(context));
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
