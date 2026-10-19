use common::{Color, Size, Vec2, format_small};
use engine::{
    Engine,
    file_sys::paths::PathRef,
    log,
    save,
    ui::{self, DrawDebugUi, UiCondition, UiNumberFormat, UiStaticVar, UiWindow},
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
        let ui_sys = context.ui_sys;

        // Menus record themselves for UI automation (engine::ui): "Debug", then
        // "Debug/Draw Terrain" while it's open.
        ui::main_menu_bar(ui_sys, |ui_sys| {
            ui::menu(ui_sys, "Game", |_| self.game_menu(context));
            ui::menu(ui_sys, "Save", |_| self.save_game_menu(context));
            ui::menu(ui_sys, "Camera", |_| self.camera_menu(context));
            ui::menu(ui_sys, "Cheats", |_| self.cheats_menu(context));
            ui::menu(ui_sys, "Debug", |_| self.debug_options_menu(context, enable_dev_tile_inspector));

            self.menu_bar_text(context, log_viewer);
        });

        self.draw_child_windows(context);
    }

    fn menu_bar_text(&self, context: &mut GameUiContext, log_viewer: &log_viewer::LogViewer) {
        let ui_sys = context.ui_sys;

        // Log error/warning count:
        {
            let (log_error_count, log_warning_count) = log_viewer.error_and_warning_count();

            if log_error_count != 0 || log_warning_count != 0 {
                ui::separator(ui_sys);
                ui::text_colored(ui_sys, Color::red(), format_small!(" Errs: {log_error_count} "));
                ui::text_colored(ui_sys, Color::yellow(), format_small!("Warns: {log_warning_count} "));
            }
        }

        // Gold units | population:
        {
            ui::separator(ui_sys);

            let gold_units_total = context.world.stats().treasury.gold_units_total;
            let gold_units_text = format_small!(" Gold: {gold_units_total} ");

            if gold_units_total == 0 {
                ui::text_colored(ui_sys, Color::red(), gold_units_text);
            } else {
                ui::text(ui_sys, gold_units_text);
            }

            let population = context.world.stats().population.total;
            ui::text(ui_sys, format_small!("Pop: {population} "));
        }
    }

    fn cheats_menu(&self, context: &mut GameUiContext) {
        cheats::get_mut().draw_debug_ui(context.ui_sys);
    }

    fn game_menu(&mut self, context: &mut GameUiContext) {
        let ui_sys = context.ui_sys;
        let game_loop = GameLoop::get_mut();

        // Quit game:
        if ui::button(ui_sys, "Quit") {
            game_loop.quit_game();
        }

        // Reset map options:
        ui::separator(ui_sys);

        if ui::button(ui_sys, "Reset to empty map") {
            game_loop.reset_session(None, None);
        }

        if ui::button(ui_sys, "Reset to dirt tiles") {
            let dirt_tile_def = PresetTiles::Dirt.find_tile_def();
            game_loop.reset_session(dirt_tile_def, None);
        }

        if ui::button(ui_sys, "Reset to grass tiles") {
            let grass_tile_def = PresetTiles::Grass.find_tile_def();
            game_loop.reset_session(grass_tile_def, None);
        }

        if ui::button(ui_sys, "Reset to water tiles") {
            let water_tile_def = PresetTiles::Water.find_tile_def();
            game_loop.reset_session(water_tile_def, None);
        }

        // Map presets:
        ui::separator(ui_sys);

        let preset_tile_map_names = preset_maps::preset_tile_maps_list();

        if ui::combo(ui_sys, "Preset Map", &mut self.preset_tile_map_number, &preset_tile_map_names) {
            self.preset_tile_map_number = self.preset_tile_map_number.min(preset_tile_map_names.len());
        }

        if ui::button(ui_sys, "Load Preset") {
            log::info!(
                log::channel!("debug"),
                "Loading preset tile map '{}' ...",
                preset_tile_map_names[self.preset_tile_map_number]
            );
            game_loop.load_preset_map(self.preset_tile_map_number);
        }

        // New game options:
        ui::separator(ui_sys);

        static NEW_MAP_SIZE: UiStaticVar<Size> = UiStaticVar::new(Size::new(64, 64));
        let new_map_size = NEW_MAP_SIZE.as_mut();
        if ui::input_i32_xy(ui_sys, "New Map Size:", new_map_size, false, Some([32, 32]), Some(["Width", "Height"])) {
            NEW_MAP_SIZE.set(NEW_MAP_SIZE.as_ref().clamp(Size::zero(), Size::new(1024, 1024)));
        }

        if ui::button(ui_sys, "New Game") {
            let grass_tile_def = PresetTiles::Grass.find_tile_def();
            game_loop.reset_session(grass_tile_def, Some(*NEW_MAP_SIZE));
        }

        // Simulation/game speed:
        ui::separator(ui_sys);
        ui::text(ui_sys, "Game Speed:");

        ui::horizontal(ui_sys, |ui_sys| {
            if ui::icon_button(ui_sys, ui::icons::ICON_PAUSE, Some("Pause")) {
                context.sim.pause();
            }
            if ui::icon_button(ui_sys, ui::icons::ICON_PLAY, Some("Resume")) {
                context.sim.resume();
            }
            if ui::icon_button(ui_sys, ui::icons::ICON_FAST_FORWARD, Some("Speedup")) {
                context.sim.speedup();
            }
            if ui::icon_button(ui_sys, ui::icons::ICON_FAST_BACKWARD, Some("Slowdown")) {
                context.sim.slowdown();
            }

            ui::text(ui_sys, "|");

            if context.sim.is_paused() {
                ui::text_colored(ui_sys, Color::red(), "Paused");
            } else {
                ui::text(ui_sys, format_small!("Speed: {:1}x", context.sim.speed()));
            }
        });
    }

    fn camera_menu(&self, context: &mut GameUiContext) {
        context.camera.draw_debug_ui(context.ui_sys);
    }

    fn debug_options_menu(&mut self, context: &mut GameUiContext, enable_dev_tile_inspector: &mut bool) {
        let ui_sys = context.ui_sys;

        self.draw_debug_ui(context.ui_sys);


        let mut show_popup_messages = super::show_popup_messages();
        if ui::checkbox(ui_sys, "Show Popup Messages", &mut show_popup_messages) {
            super::set_show_popup_messages(show_popup_messages);
        }

        let mut show_ui_debug_window = context.ui_sys.is_showing_ui_debug_window();
        if ui::checkbox(ui_sys, "Show UI Debug Window", &mut show_ui_debug_window) {
            context.ui_sys.show_ui_debug_window(show_ui_debug_window);
        }

        ui::checkbox(ui_sys, "Enable Tile Inspector", enable_dev_tile_inspector);

        // Debug grid options:
        ui::separator(ui_sys);

        let game_loop = GameLoop::get_mut();

        let mut line_thickness = game_loop.grid_line_thickness();
        let (min_thickness, max_thickness) = (MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS);
        if ui::slider(ui_sys, "Grid thickness", min_thickness, max_thickness, UiNumberFormat::Decimals(1), &mut line_thickness) {
            game_loop.set_grid_line_thickness(line_thickness);
        }

        ui::checkbox(ui_sys, "Draw grid", &mut self.draw_grid);
        ui::checkbox(ui_sys, "Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);

        ui::separator(ui_sys);

        ui::horizontal(ui_sys, |ui_sys| {
            if ui::button(ui_sys, "Panic Now!") {
                panic!("Testing a runtime panic!");
            }

            if ui::button(ui_sys, "Crash Now!") {
                unsafe {
                    let bad_ptr: *mut i64 = core::ptr::null_mut();
                    *bad_ptr = 1;
                }
            }
        });
    }

    fn save_game_menu(&mut self, context: &mut GameUiContext) {
        let ui_sys = context.ui_sys;
        let game_loop = GameLoop::get_mut();

        // Autosave:
        let mut autosave_enabled = game_loop.is_autosave_enabled();
        if ui::checkbox(ui_sys, "Autosave", &mut autosave_enabled) {
            game_loop.enable_autosave(autosave_enabled);
        }

        if ui::button(ui_sys, "Load Autosave") {
            game_loop.load_save_game(save::storage::AUTOSAVE_FILE_NAME);
        }

        // Save game:
        ui::separator(ui_sys);

        if self.save_file_name.is_empty() {
            self.save_file_name = save::storage::DEFAULT_SAVE_FILE_NAME.to_string();
        }

        ui::input_text(ui_sys, "Save File", &mut self.save_file_name, false);

        if ui::button(ui_sys, "Save") {
            if !self.save_file_name.is_empty() {
                game_loop.save_game(PathRef::from_str(&self.save_file_name));
            } else {
                log::error!(log::channel!("debug"), "No save file name provided!");
            }
        }

        // Load save game:
        ui::separator(ui_sys);

        let save_files = save::storage::list_save_files();
        let save_file_names: Vec<_> = save_files.iter().map(|save_file| save_file.to_string_lossy()).collect();

        if ui::combo(ui_sys, "Load File", &mut self.save_file_selected, &save_file_names) {
            self.save_file_selected = self.save_file_selected.min(save_files.len());
        }

        if ui::button(ui_sys, "Load") && !save_files.is_empty() {
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
        // TODO(layout): Too narrow for its tab labels ("Engine/…", "Buildin…").
        let window = UiWindow::new("Game Configs")
            .open(&mut self.show_game_configs_debug)
            .position(Vec2::new(200.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(400.0, 350.0), UiCondition::FirstUse);

        ui::window(context.ui_sys, window, |ui_sys| {
            const TABS: [&str; 6] = ["Engine/Game", "Buildings", "Units", "Props", "Campaigns", "Ui Text"];
            let Some(tab) = ui::tab_bar(ui_sys, "Configs Tab Bar", &TABS) else {
                return;
            };

            match tab.index {
                0 => GameConfigs::get_mut().draw_debug_ui_with_header("GameConfigs", ui_sys),
                1 => BuildingConfigs::get_mut().draw_debug_ui_with_header("BuildingConfigs", ui_sys),
                2 => UnitConfigs::get_mut().draw_debug_ui_with_header("UnitConfigs", ui_sys),
                3 => PropConfigs::get_mut().draw_debug_ui_with_header("PropConfigs", ui_sys),
                4 => CampaignConfigs::get_mut().draw_debug_ui_with_header("CampaignConfigs", ui_sys),
                _ => ui::text::draw_debug_ui(ui_sys),
            }
        });
    }

    fn draw_world_debug_window(&mut self, context: &mut GameUiContext) {
        let window = UiWindow::new("World Debug")
            .open(&mut self.show_game_world_debug)
            .position(Vec2::new(300.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(400.0, 350.0), UiCondition::FirstUse);

        ui::window(context.ui_sys, window, |_| {
            let sim = GameLoop::get_mut().sim_mut();
            sim.draw_world_debug_ui(context);
        });
    }

    fn draw_game_systems_debug_window(&mut self, context: &mut GameUiContext) {
        // TODO(layout): Too narrow for its tab labels ("Settle…", "Ambien…") and some rows
        // ("Population Per Set…").
        let window = UiWindow::new("Game Systems Debug")
            .open(&mut self.show_game_systems_debug)
            .position(Vec2::new(400.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(400.0, 350.0), UiCondition::FirstUse);

        ui::window(context.ui_sys, window, |_| {
            let engine = Engine::get_mut();
            let sim = GameLoop::get_mut().sim_mut();
            let systems = GameLoop::get_mut().systems_mut();
            sim.draw_game_systems_debug_ui(context, engine, systems);
        });
    }

    fn draw_texture_settings_window(&mut self, context: &mut GameUiContext) {
        let window = UiWindow::new("Texture Settings")
            .open(&mut self.show_texture_settings)
            .position(Vec2::new(500.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(300.0, 150.0), UiCondition::FirstUse);

        ui::window(context.ui_sys, window, |ui_sys| {
            let tex_cache = Engine::get_mut().texture_cache_mut();
            tex_cache.draw_debug_ui(ui_sys);
        });
    }

    fn draw_sound_settings_window(&mut self, context: &mut GameUiContext) {
        let window = UiWindow::new("Sound Settings")
            .open(&mut self.show_sound_settings)
            .position(Vec2::new(350.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(500.0, 400.0), UiCondition::FirstUse);

        ui::window(context.ui_sys, window, |ui_sys| {
            let sound_sys = Engine::get_mut().sound_system_mut();
            sound_sys.draw_debug_ui(ui_sys);
        });
    }
}
