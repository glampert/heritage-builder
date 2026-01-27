use std::sync::atomic::{AtomicBool, Ordering};
use std::any::Any;

use inspector::TileInspectorDevMenu;
use palette::TilePaletteDevMenu;
use settings::DebugSettingsDevMenu;

use crate::{
    singleton_late_init,
    render::TextureCache,
    ui::{UiTheme, widgets::UiWidgetContext},
    save::{Load, PreLoadContext, PostLoadContext, Save},
    game::{sim, config::GameConfigs, GameLoop, menu::*},
    utils::{coords::{Cell, CellRange}, mem::{self, SingleThreadStatic}},
    tile::{rendering::TileMapRenderFlags, TileMap, TileMapLayerKind, minimap::DevUiMinimapRenderer},

    // TEMP
    ui::widgets::*
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

pub struct DevEditorMenus {
    // TEMP
    test_menu: UiMenuStrongRef,
    //test_slideshow: UiSlideshow,
}

impl DevEditorMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        //context.ui_sys.set_ui_theme(UiTheme::Dev);
        // Register TileMap global callbacks & debug ref:
        register_tile_map_debug_callbacks(context.tile_map);

        // TEMP
        context.ui_sys.set_ui_theme(UiTheme::InGame);

        use crate::utils::Vec2;
        use crate::log;

        let slideshow = UiSlideshow::new(context,
            UiSlideshowParams {
                flags: UiSlideshowFlags::default(),
                loop_mode: UiSlideshowLoopMode::WholeAnim,
                frame_duration_secs: 0.5,
                frames: &["misc/home_menu_anim/frame0.jpg", "misc/home_menu_anim/frame1.jpg", "misc/home_menu_anim/frame2.jpg"],
                size: Some(Vec2::new(0.0, 256.0)),
                margin_left: 50.0,
                margin_right: 50.0,
            }
        );

        let test_menu_rc = UiMenu::new(
            context,
            Some("Test Window".into()),
            UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter,
            Some(Vec2::new(512.0, 700.0)),
            None,
            Some("misc/wide_page_bg.png"),
            UiMenu::NO_OPEN_CLOSE_CALLBACK
        );

        let mut slideshow_group = UiWidgetGroup::new(0.0, true, true, true);
        slideshow_group.add_widget(slideshow);
        test_menu_rc.as_mut().add_widget(slideshow_group);

        if false {

        let test_menu = test_menu_rc.as_mut();

        let mut group = UiLabeledWidgetGroup::new(5.0, 5.0, false, true);

        test_menu.add_widget(UiMenuHeading::new(
            context,
            1.8,
            vec!["Settings".into()],
            Some("misc/brush_stroke_divider_2.png"),
            50.0,
            0.0
        ));
        let slider1 = UiSlider::from_u32(
            None,
            1.0,
            0,
            100,
            |_slider, _context| -> u32 { 42 },
            |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
        );
        group.add_widget("Master Volume:".into(), slider1);

        let slider2 = UiSlider::from_u32(
            None,
            1.0,
            0,
            100,
            |_slider, _context| -> u32 { 42 },
            |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
        );
        group.add_widget("Sfx Volume:".into(), slider2);

        let slider3 = UiSlider::from_u32(
            None,
            1.0,
            0,
            100,
            |_slider, _context| -> u32 { 42 },
            |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
        );
        group.add_widget("Music Volume:".into(), slider3);

        let checkbox = UiCheckbox::new(
            None,
            1.0,
            |_checkbox, _context| -> bool { true },
            |_checkbox, _context, new_value: bool| { log::info!("Updated checkbox value: {new_value}") },
        );
        group.add_widget("Enable Volume:".into(), checkbox);

        let text_input = UiTextInput::new(
            None,
            1.0,
            |_input, _context| -> String { "Hello".into() },
            |_input, _context, new_value: String| { log::info!("Updated text input value: {new_value}") },
        );
        group.add_widget("Player Name:".into(), text_input);

        use strum::VariantArray;
        let dropdown = UiDropdown::from_values(
            None,
            1.0,
            0,
            crate::render::TextureFilter::VARIANTS,
            |_dropdown, _context, selection_index, selection_string| { log::info!("Updated dropdown: {selection_index}, {selection_string}") }
        );
        group.add_widget("Texture Filter:".into(), dropdown);

        /*
        test_menu.add_widget(UiMenuHeading::new(
            context,
            1.8,
            vec!["Test Heading Line".into(), "Second Line".into()],
            Some("misc/brush_stroke_divider_2.png"),
            50.0,
            0.0
        ));

        let slider = UiSlider::from_u32(
            Some("Master Volume".into()),
            1.0,
            0,
            100,
            |_slider, _context| -> u32 { 42 },
            |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
        );

        let checkbox = UiCheckbox::new(
            Some("Enable Volume".into()),
            1.0,
            |_checkbox, _context| -> bool { true },
            |_checkbox, _context, new_value: bool| { log::info!("Updated checkbox value: {new_value}") },
        );

        let text_input = UiTextInput::new(
            Some("Name".into()),
            1.0,
            |_input, _context| -> String { "Hello".into() },
            |_input, _context, new_value: String| { log::info!("Updated text input value: {new_value}") },
        );

        use strum::VariantArray;
        let dropdown = UiDropdown::from_values(
            Some("Texture Filter".into()),
            1.0,
            0,
            crate::render::TextureFilter::VARIANTS,
            |_dropdown, _context, selection_index, selection_string| { log::info!("Updated dropdown: {selection_index}, {selection_string}") }
        );

        let mut group = UiWidgetGroup::new(15.0, false, true);

        group.add_widget(slider);
        group.add_widget(checkbox);
        group.add_widget(text_input);
        group.add_widget(dropdown);
        */

        /*
        let tooltip = UiTooltipText::new(context, "This is a button".into(), 0.8, Some("misc/wide_page_bg.png"));
        group.add_widget(UiSpriteButton::new(
            context,
            "palette/housing".into(),
            Some(tooltip.clone()),
            true,
            Vec2::new(50.0, 50.0),
            UiSpriteButtonState::Idle,
            0.0,
        ));

        group.add_widget(UiSpriteButton::new(
            context,
            "palette/roads".into(),
            Some(tooltip.clone()),
            true,
            Vec2::new(50.0, 50.0),
            UiSpriteButtonState::Idle,
            0.0,
        ));

        group.add_widget(UiSpriteButton::new(
            context,
            "palette/food_and_farming".into(),
            Some(tooltip.clone()),
            true,
            Vec2::new(50.0, 50.0),
            UiSpriteButtonState::Disabled,
            0.0,
        ));
        */

        /*
        group.add_widget(UiTextButton::new(
            context,
            "Small Button".into(),
            UiTextButtonSize::Small,
            Some("misc/brush_stroke_divider_2.png"),
            true,
            |button, _context| log::info!("Pressed: {}", button.label())
        ));

        group.add_widget(UiTextButton::new(
            context,
            "Normal Button".into(),
            UiTextButtonSize::Small,
            Some("misc/brush_stroke_divider_2.png"),
            true,
            |button, _context| log::info!("Pressed: {}", button.label())
        ));

        group.add_widget(UiTextButton::new(
            context,
            "Large Button".into(),
            UiTextButtonSize::Small,
            Some("misc/brush_stroke_divider_2.png"),
            true,
            |button, _context| log::info!("Pressed: {}", button.label())
        ));

        group.add_widget(UiTextButton::new(
            context,
            "Disabled Button".into(),
            UiTextButtonSize::Small,
            Some("misc/brush_stroke_divider_2.png"),
            false,
            |button, _context| log::info!("Pressed: {}", button.label())
        ));
        */

        test_menu.add_widget(group);

        let item_list = UiItemList::from_strings(
            Some("Item List".into()),
            1.0,
            Some(Vec2::new(0.0, 128.0)), // use whole parent window width - margin, fixed height
            30.0,
            30.0,
            UiItemListFlags::Border | UiItemListFlags::TextInputField,
            Some(2),
            vec!["One".into(), "Two".into(), "Three".into()],
            |_list, _context, selection_index, selection_string| { log::info!("Updated list: {selection_index:?}, {selection_string}") }
        );

        test_menu.add_widget(item_list);

        use std::rc::Rc;
        let weak_menu = Rc::downgrade(&test_menu_rc);

        test_menu.add_widget(UiTextButton::new(
            context,
            "Open Message Box".into(),
            UiTextButtonSize::Normal,
            Some("misc/brush_stroke_divider_2.png"),
            true,
            move |button, context| {
                log::info!("Pressed: {}", button.label());

                if let Some(menu_rc) = weak_menu.upgrade() {
                    let menu_ref_ok_btn = weak_menu.clone();
                    let menu_ref_cancel_btn = weak_menu.clone();

                    let params = UiMessageBoxParams {
                        label: Some("Test Popup".into()),
                        background: Some("misc/wide_page_bg.png"),
                        contents: vec![
                            UiWidgetImpl::from(UiMenuHeading::new(
                                context,
                                1.2,
                                vec!["Quit to main menu?".into(), "Unsaved progress will be lost".into()],
                                Some("misc/brush_stroke_divider_2.png"),
                                20.0,
                                0.0
                            ))
                        ],
                        buttons: vec![
                            UiWidgetImpl::from(UiTextButton::new(
                                context,
                                "Ok".into(),
                                UiTextButtonSize::Small,
                                Some("misc/brush_stroke_divider_2.png"),
                                true,
                                move |button, context| {
                                    log::info!("Pressed: {}", button.label());
                                    if let Some(menu) = menu_ref_ok_btn.upgrade() {
                                        menu.as_mut().close_message_box(context);
                                    }
                                }
                            )),
                            UiWidgetImpl::from(UiTextButton::new(
                                context,
                                "Cancel".into(),
                                UiTextButtonSize::Small,
                                Some("misc/brush_stroke_divider_2.png"),
                                true,
                                move |button, context| {
                                    log::info!("Pressed: {}", button.label());
                                    if let Some(menu) = menu_ref_cancel_btn.upgrade() {
                                        menu.as_mut().close_message_box(context);
                                    }
                                }
                            ))
                        ],
                        ..Default::default()
                    };

                    menu_rc.as_mut().open_message_box(context, params);
                }
            }
        ));

        }

        //Self { test_menu: test_menu_rc, test_slideshow: slideshow }
        Self { test_menu: test_menu_rc }
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

        // TEMP
        let mut widgets_context = UiWidgetContext::new(context.sim, context.world, context.tile_map, context.engine);
        let menu = self.test_menu.as_mut();
        menu.draw(&mut widgets_context);
    
        //self.test_slideshow.draw(&mut widgets_context);
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
        let show_render_perf_stats = self.debug_settings_menu.show_render_perf_stats();
        let show_world_perf_stats = self.debug_settings_menu.show_world_perf_stats();
        let show_selection_bounds = self.debug_settings_menu.show_selection_bounds();
        let show_log_viewer_window = self.debug_settings_menu.show_log_viewer_window();

        let game_loop = GameLoop::get_mut();

        if *show_log_viewer_window {
            let log_viewer = game_loop.engine_mut().log_viewer();
            log_viewer.show(true);
            *show_log_viewer_window = log_viewer.draw(menu_context.engine.ui_system());
        }

        let mut sim_context = sim::debug::DebugContext {
            ui_sys: menu_context.engine.ui_system(),
            world: menu_context.world,
            systems: menu_context.systems,
            tile_map: menu_context.tile_map,
            transform: menu_context.camera.transform(),
            delta_time_secs: menu_context.delta_time_secs
        };

        self.tile_palette_menu.draw(&mut sim_context,
                                    menu_context.sim,
                                    game_loop.engine_mut().debug_draw(),
                                    menu_context.cursor_screen_pos,
                                    has_valid_placement,
                                    show_selection_bounds);

        self.debug_settings_menu.draw(&mut sim_context,
                                      game_loop,
                                      menu_context.sim,
                                      &mut self.enable_tile_inspector);

        if self.enable_tile_inspector {
            self.tile_inspector_menu.draw(&mut sim_context, menu_context.sim);
        }

        if show_popup_messages() {
            menu_context.sim.draw_game_object_debug_popups(&mut sim_context, visible_range);
        }

        sim_context.tile_map.minimap_mut().draw(&mut self.minimap_renderer,
                                                menu_context.engine.render_system(),
                                                menu_context.camera,
                                                sim_context.ui_sys);

        game_loop.camera().draw_debug(GameLoop::get_mut().engine_mut().debug_draw(), sim_context.ui_sys);

        if show_cursor_pos {
            utils::draw_cursor_overlay(menu_context.engine.ui_system(),
                                       menu_context.camera.transform(),
                                       menu_context.cursor_screen_pos,
                                       None);
        }

        if show_render_perf_stats {
            let engine = game_loop.engine();
            utils::draw_render_perf_stats(menu_context.engine.ui_system(),
                                          engine.render_stats(),
                                          engine.tile_map_render_stats());
        }

        if show_world_perf_stats {
            utils::draw_world_perf_stats(menu_context.engine.ui_system(),
                                         menu_context.world,
                                         menu_context.tile_map,
                                         visible_range);
        }

        if show_screen_origin {
            let engine = game_loop.engine_mut();
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
