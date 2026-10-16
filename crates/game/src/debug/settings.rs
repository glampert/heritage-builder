use common::{Color, Size, format_small};
use engine::{
    Engine,
    file_sys::paths::PathRef,
    log,
    save,
    ui::{self, DrawDebugUi, UiStaticVar, dev},
};
use proc_macros::DrawDebugUi;

use super::{log_viewer, preset_maps};
use crate::{
    cheats,
    GameLoop,
    config::GameConfigs,
    ui_context::GameUiContext,
    unit::config::UnitConfigs,
    building::config::BuildingConfigs,
    prop::config::PropConfigs,
    campaign::config::CampaignConfigs,
    tile::{
        rendering::{MAX_GRID_LINE_THICKNESS, MIN_GRID_LINE_THICKNESS, TileMapRenderFlags},
        sets::PresetTiles,
    },
};

// ----------------------------------------------
// DebugSettingsDevMenu
// ----------------------------------------------

#[derive(Default, DrawDebugUi)]
pub struct DebugSettingsDevMenu {
    #[debug_ui(skip)]
    draw_grid: bool,
    #[debug_ui(skip)]
    draw_grid_ignore_depth: bool,

    #[debug_ui(skip)]
    preset_tile_map_number: usize,
    #[debug_ui(skip)]
    save_file_name: String,
    #[debug_ui(skip)]
    save_file_selected: usize,

    #[debug_ui(edit)]
    draw_terrain: bool,
    #[debug_ui(edit)]
    draw_buildings: bool,
    #[debug_ui(edit)]
    draw_props: bool,
    #[debug_ui(edit)]
    draw_units: bool,
    #[debug_ui(edit)]
    draw_vegetation: bool,
    #[debug_ui(edit, separator)]
    cull_occluded_terrain: bool,

    #[debug_ui(edit)]
    show_terrain_debug: bool,
    #[debug_ui(edit)]
    show_buildings_debug: bool,
    #[debug_ui(edit)]
    show_props_debug: bool,
    #[debug_ui(edit)]
    show_units_debug: bool,
    #[debug_ui(edit)]
    show_vegetation_debug: bool,
    #[debug_ui(edit)]
    show_blocker_tiles_debug: bool,
    #[debug_ui(edit, separator)]
    show_search_graph_debug: bool,

    #[debug_ui(edit)]
    show_tile_bounds: bool,
    #[debug_ui(edit)]
    show_selection_bounds: bool,
    #[debug_ui(edit)]
    show_cursor_pos: bool,
    #[debug_ui(edit)]
    show_screen_origin: bool,
    #[debug_ui(edit)]
    show_sample_menus: bool,
    #[debug_ui(edit)]
    show_world_perf_stats: bool,
    #[debug_ui(edit)]
    show_render_perf_stats: bool,
    #[debug_ui(edit)]
    show_texture_settings: bool,
    #[debug_ui(edit, separator)]
    show_sound_settings: bool,

    #[debug_ui(edit)]
    show_game_configs_debug: bool,
    #[debug_ui(edit)]
    show_game_world_debug: bool,
    #[debug_ui(edit)]
    show_game_systems_debug: bool,
    #[debug_ui(edit)]
    show_log_viewer_window: bool,
}

impl DebugSettingsDevMenu {
    pub fn new() -> Self {
        Self {
            draw_terrain: true,
            draw_buildings: true,
            draw_props: true,
            draw_units: true,
            draw_vegetation: true,
            ..Default::default()
        }
    }

    pub fn show_selection_bounds(&self) -> bool {
        self.show_selection_bounds
    }

    pub fn show_cursor_pos(&self) -> bool {
        self.show_cursor_pos
    }

    pub fn show_screen_origin(&self) -> bool {
        self.show_screen_origin
    }

    pub fn show_sample_menus(&self) -> bool {
        self.show_sample_menus
    }

    pub fn show_world_perf_stats(&self) -> bool {
        self.show_world_perf_stats
    }

    pub fn show_render_perf_stats(&self) -> bool {
        self.show_render_perf_stats
    }

    pub fn show_log_viewer_window(&mut self) -> &mut bool {
        &mut self.show_log_viewer_window
    }

    pub fn selected_render_flags(&self) -> TileMapRenderFlags {
        let mut flags = TileMapRenderFlags::empty();
        if self.draw_terrain {
            flags.insert(TileMapRenderFlags::DrawTerrain);
        }
        if self.draw_buildings {
            flags.insert(TileMapRenderFlags::DrawBuildings);
        }
        if self.draw_props {
            flags.insert(TileMapRenderFlags::DrawProps);
        }
        if self.draw_units {
            flags.insert(TileMapRenderFlags::DrawUnits);
        }
        if self.draw_vegetation {
            flags.insert(TileMapRenderFlags::DrawVegetation);
        }
        if self.cull_occluded_terrain {
            flags.insert(TileMapRenderFlags::CullOccludedTerrainTiles);
        }
        if self.draw_grid {
            flags.insert(TileMapRenderFlags::DrawGrid);
        }
        if self.draw_grid_ignore_depth {
            flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth);
        }
        if self.show_tile_bounds {
            flags.insert(TileMapRenderFlags::DrawDebugBounds);
        }
        if self.show_terrain_debug {
            flags.insert(TileMapRenderFlags::DrawTerrainTileDebug);
        }
        if self.show_buildings_debug {
            flags.insert(TileMapRenderFlags::DrawBuildingsTileDebug);
        }
        if self.show_props_debug {
            flags.insert(TileMapRenderFlags::DrawPropsTileDebug);
        }
        if self.show_units_debug {
            flags.insert(TileMapRenderFlags::DrawUnitsTileDebug);
        }
        if self.show_vegetation_debug {
            flags.insert(TileMapRenderFlags::DrawVegetationTileDebug);
        }
        if self.show_blocker_tiles_debug {
            flags.insert(TileMapRenderFlags::DrawBlockersTileDebug);
        }
        if self.show_search_graph_debug {
            flags.insert(TileMapRenderFlags::DrawSearchGraphDebug);
        }
        flags
    }

    pub fn draw(
        &mut self,
        context: &mut GameUiContext,
        log_viewer: &log_viewer::LogViewer,
        enable_dev_tile_inspector: &mut bool,
    ) {
        let ui = context.ui_sys.ui();
        let ui_sys = context.ui_sys;

        // Menus record themselves for UI automation (engine::ui::dev): "Debug", then
        // "Debug/Draw Terrain" while it's open.
        if let Some(_menu_bar) = ui.begin_main_menu_bar() {
            if let Some(_menu) = dev::begin_menu(ui_sys, "Game") {
                self.game_menu(context);
            }

            if let Some(_menu) = dev::begin_menu(ui_sys, "Save") {
                self.save_game_menu(context);
            }

            if let Some(_menu) = dev::begin_menu(ui_sys, "Camera") {
                self.camera_menu(context);
            }

            if let Some(_menu) = dev::begin_menu(ui_sys, "Cheats") {
                self.cheats_menu(context);
            }

            if let Some(_menu) = dev::begin_menu(ui_sys, "Debug") {
                self.debug_options_menu(context, enable_dev_tile_inspector);
            }

            self.menu_bar_text(context, log_viewer);
        }

        self.draw_child_windows(context);
    }

    fn menu_bar_text(&self, context: &mut GameUiContext, log_viewer: &log_viewer::LogViewer) {
        let ui = context.ui_sys.ui();

        // Log error/warning count:
        {
            let (log_error_count, log_warning_count) = log_viewer.error_and_warning_count();

            if log_error_count != 0 || log_warning_count != 0 {
                ui.separator();
                ui.text_colored(Color::red().to_array(), format_small!(" Errs: {log_error_count} "));
                ui.text_colored(Color::yellow().to_array(), format_small!("Warns: {log_warning_count} "));
            }
        }

        // Gold units | population:
        {
            ui.separator();

            let gold_units_total = context.world.stats().treasury.gold_units_total;
            let gold_units_text = format_small!(" Gold: {gold_units_total} ");

            if gold_units_total == 0 {
                ui.text_colored(Color::red().to_array(), gold_units_text);
            } else {
                ui.text(gold_units_text);
            }

            let population = context.world.stats().population.total;
            ui.text(format_small!("Pop: {population} "));
        }
    }

    fn cheats_menu(&self, context: &mut GameUiContext) {
        cheats::get_mut().draw_debug_ui(context.ui_sys);
    }

    fn game_menu(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();
        let ui_sys = context.ui_sys;
        let game_loop = GameLoop::get_mut();

        // Quit game:
        if dev::button(ui_sys, "Quit") {
            game_loop.quit_game();
        }

        // Reset map options:
        ui.separator();

        if dev::button(ui_sys, "Reset to empty map") {
            game_loop.reset_session(None, None);
        }

        if dev::button(ui_sys, "Reset to dirt tiles") {
            let dirt_tile_def = PresetTiles::Dirt.find_tile_def();
            game_loop.reset_session(dirt_tile_def, None);
        }

        if dev::button(ui_sys, "Reset to grass tiles") {
            let grass_tile_def = PresetTiles::Grass.find_tile_def();
            game_loop.reset_session(grass_tile_def, None);
        }

        if dev::button(ui_sys, "Reset to water tiles") {
            let water_tile_def = PresetTiles::Water.find_tile_def();
            game_loop.reset_session(water_tile_def, None);
        }

        // Map presets:
        ui.separator();

        let preset_tile_map_names = preset_maps::preset_tile_maps_list();

        if dev::combo(ui_sys, "Preset Map", &mut self.preset_tile_map_number, &preset_tile_map_names) {
            self.preset_tile_map_number = self.preset_tile_map_number.min(preset_tile_map_names.len());
        }

        if dev::button(ui_sys, "Load Preset") {
            log::info!(
                log::channel!("debug"),
                "Loading preset tile map '{}' ...",
                preset_tile_map_names[self.preset_tile_map_number]
            );
            game_loop.load_preset_map(self.preset_tile_map_number);
        }

        // New game options:
        ui.separator();

        static NEW_MAP_SIZE: UiStaticVar<Size> = UiStaticVar::new(Size::new(64, 64));
        let new_map_size = NEW_MAP_SIZE.as_mut();
        if ui::input_i32_xy(ui_sys, "New Map Size:", new_map_size, false, Some([32, 32]), Some(["Width", "Height"])) {
            NEW_MAP_SIZE.set(NEW_MAP_SIZE.as_ref().clamp(Size::zero(), Size::new(1024, 1024)));
        }

        if dev::button(ui_sys, "New Game") {
            let grass_tile_def = PresetTiles::Grass.find_tile_def();
            game_loop.reset_session(grass_tile_def, Some(*NEW_MAP_SIZE));
        }

        // Simulation/game speed:
        ui.separator();
        ui.text("Game Speed:");

        if ui::icon_button(context.ui_sys, ui::icons::ICON_PAUSE, Some("Pause")) {
            context.sim.pause();
        }
        ui.same_line();
        if ui::icon_button(context.ui_sys, ui::icons::ICON_PLAY, Some("Resume")) {
            context.sim.resume();
        }
        ui.same_line();
        if ui::icon_button(context.ui_sys, ui::icons::ICON_FAST_FORWARD, Some("Speedup")) {
            context.sim.speedup();
        }
        ui.same_line();
        if ui::icon_button(context.ui_sys, ui::icons::ICON_FAST_BACKWARD, Some("Slowdown")) {
            context.sim.slowdown();
        }

        ui.same_line();
        ui.text("|");
        ui.same_line();

        if context.sim.is_paused() {
            ui.text_colored(Color::red().to_array(), "Paused");
        } else {
            ui.text(format_small!("Speed: {:1}x", context.sim.speed()));
        }
    }

    fn camera_menu(&self, context: &mut GameUiContext) {
        context.camera.draw_debug_ui(context.ui_sys);
    }

    fn debug_options_menu(&mut self, context: &mut GameUiContext, enable_dev_tile_inspector: &mut bool) {
        let ui = context.ui_sys.ui();

        self.draw_debug_ui(context.ui_sys);

        let ui_sys = context.ui_sys;

        let mut show_popup_messages = super::show_popup_messages();
        if dev::checkbox(ui_sys, "Show Popup Messages", &mut show_popup_messages) {
            super::set_show_popup_messages(show_popup_messages);
        }

        let mut show_ui_debug_menu = context.ui_sys.is_showing_ui_debug_menu();
        if dev::checkbox(ui_sys, "Show ImGui Debug Menu", &mut show_ui_debug_menu) {
            context.ui_sys.show_ui_debug_menu(show_ui_debug_menu);
        }

        dev::checkbox(ui_sys, "Enable Tile Inspector", enable_dev_tile_inspector);

        // Debug grid options:
        ui.separator();

        let game_loop = GameLoop::get_mut();

        let mut line_thickness = game_loop.grid_line_thickness();
        let (min_thickness, max_thickness) = (MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS);
        if dev::slider(ui_sys, "Grid thickness", min_thickness, max_thickness, Some("%.1f"), &mut line_thickness) {
            game_loop.set_grid_line_thickness(line_thickness);
        }

        dev::checkbox(ui_sys, "Draw grid", &mut self.draw_grid);
        dev::checkbox(ui_sys, "Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);

        ui.separator();

        if dev::button(ui_sys, "Panic Now!") {
            panic!("Testing a runtime panic!");
        }

        ui.same_line();

        if dev::button(ui_sys, "Crash Now!") {
            unsafe {
                let bad_ptr: *mut i64 = core::ptr::null_mut();
                *bad_ptr = 1;
            }
        }
    }

    fn save_game_menu(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();
        let ui_sys = context.ui_sys;
        let game_loop = GameLoop::get_mut();

        // Autosave:
        let mut autosave_enabled = game_loop.is_autosave_enabled();
        if dev::checkbox(ui_sys, "Autosave", &mut autosave_enabled) {
            game_loop.enable_autosave(autosave_enabled);
        }

        if dev::button(ui_sys, "Load Autosave") {
            game_loop.load_save_game(save::storage::AUTOSAVE_FILE_NAME);
        }

        // Save game:
        ui.separator();

        if self.save_file_name.is_empty() {
            self.save_file_name = save::storage::DEFAULT_SAVE_FILE_NAME.to_string();
        }

        dev::input_text(ui_sys, "Save File", &mut self.save_file_name, false);

        if dev::button(ui_sys, "Save") {
            if !self.save_file_name.is_empty() {
                game_loop.save_game(PathRef::from_str(&self.save_file_name));
            } else {
                log::error!(log::channel!("debug"), "No save file name provided!");
            }
        }

        // Load save game:
        ui.separator();

        let save_files = save::storage::list_save_files();
        let save_file_names: Vec<_> = save_files.iter().map(|save_file| save_file.to_string_lossy()).collect();

        if dev::combo(ui_sys, "Load File", &mut self.save_file_selected, &save_file_names) {
            self.save_file_selected = self.save_file_selected.min(save_files.len());
        }

        if dev::button(ui_sys, "Load") && !save_files.is_empty() {
            game_loop.load_save_game(PathRef::from_str(save_files[self.save_file_selected].to_str().unwrap()));
        }
    }

    fn draw_child_windows(&mut self, context: &mut GameUiContext) {
        if self.show_game_configs_debug {
            self.draw_game_configs_window(context);
        }

        if self.show_game_world_debug {
            self.draw_world_debug_window(context);
        }

        if self.show_game_systems_debug {
            self.draw_game_systems_debug_window(context);
        }

        if self.show_texture_settings {
            self.draw_texture_settings_window(context);
        }

        if self.show_sound_settings {
            self.draw_sound_settings_window(context);
        }
    }

    fn draw_game_configs_window(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();

        ui.window("Game Configs")
            .opened(&mut self.show_game_configs_debug)
            .position([200.0, 20.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| {
                dev::record_window(context.ui_sys, "window", "Game Configs", None, true);
                let _path = dev::path_scope(context.ui_sys, "Game Configs");
                if let Some(_tab_bar) = ui.tab_bar("Configs Tab Bar") {
                    if let Some(_tab) = ui.tab_item("Engine/Game") {
                        GameConfigs::get_mut().draw_debug_ui_with_header("GameConfigs", context.ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Buildings") {
                        BuildingConfigs::get_mut().draw_debug_ui_with_header("BuildingConfigs", context.ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Units") {
                        UnitConfigs::get_mut().draw_debug_ui_with_header("UnitConfigs", context.ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Props") {
                        PropConfigs::get_mut().draw_debug_ui_with_header("PropConfigs", context.ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Campaigns") {
                        CampaignConfigs::get_mut().draw_debug_ui_with_header("CampaignConfigs", context.ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Ui Text") {
                        ui::text::draw_debug_ui(context.ui_sys);
                    }
                }
            });
    }

    fn draw_world_debug_window(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();

        ui.window("World Debug")
            .opened(&mut self.show_game_world_debug)
            .position([300.0, 20.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| {
                dev::record_window(context.ui_sys, "window", "World Debug", None, true);
                let _path = dev::path_scope(context.ui_sys, "World Debug");
                let sim = GameLoop::get_mut().sim_mut();
                sim.draw_world_debug_ui(context);
            });
    }

    fn draw_game_systems_debug_window(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();

        ui.window("Game Systems Debug")
            .opened(&mut self.show_game_systems_debug)
            .position([400.0, 20.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| {
                dev::record_window(context.ui_sys, "window", "Game Systems Debug", None, true);
                let _path = dev::path_scope(context.ui_sys, "Game Systems Debug");
                let engine = Engine::get_mut();
                let sim = GameLoop::get_mut().sim_mut();
                let systems = GameLoop::get_mut().systems_mut();
                sim.draw_game_systems_debug_ui(context, engine, systems);
            });
    }

    fn draw_texture_settings_window(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();

        ui.window("Texture Settings")
            .opened(&mut self.show_texture_settings)
            .position([500.0, 20.0], imgui::Condition::FirstUseEver)
            .size([300.0, 150.0], imgui::Condition::FirstUseEver)
            .build(|| {
                dev::record_window(context.ui_sys, "window", "Texture Settings", None, true);
                let _path = dev::path_scope(context.ui_sys, "Texture Settings");
                let tex_cache = Engine::get_mut().texture_cache_mut();
                tex_cache.draw_debug_ui(context.ui_sys);
            });
    }

    fn draw_sound_settings_window(&mut self, context: &mut GameUiContext) {
        let ui = context.ui_sys.ui();

        ui.window("Sound Settings")
            .opened(&mut self.show_sound_settings)
            .position([350.0, 20.0], imgui::Condition::FirstUseEver)
            .size([500.0, 400.0], imgui::Condition::FirstUseEver)
            .build(|| {
                dev::record_window(context.ui_sys, "window", "Sound Settings", None, true);
                let _path = dev::path_scope(context.ui_sys, "Sound Settings");
                let sound_sys = Engine::get_mut().sound_system_mut();
                sound_sys.draw_debug_ui(context.ui_sys);
            });
    }
}
