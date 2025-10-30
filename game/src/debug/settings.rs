use proc_macros::DrawDebugUi;

use crate::{
    debug,
    engine::config::Configs,
    game::{
        self,
        building::config::BuildingConfigs,
        cheats,
        config::GameConfigs,
        sim::{self, Simulation},
        unit::config::UnitConfigs,
        prop::config::PropConfigs,
        GameLoop,
    },
    log,
    tile::{
        camera::{Camera, CameraGlobalSettings},
        rendering::{TileMapRenderFlags, MAX_GRID_LINE_THICKNESS, MIN_GRID_LINE_THICKNESS},
        sets::{TileSets, TERRAIN_GROUND_CATEGORY, TERRAIN_WATER_CATEGORY},
        TileMapLayerKind,
    },
    utils::{hash, Color},
};

// ----------------------------------------------
// DebugSettingsMenu
// ----------------------------------------------

#[derive(Default, DrawDebugUi)]
pub struct DebugSettingsMenu {
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
    #[debug_ui(edit, separator)]
    show_blocker_tiles_debug: bool,

    #[debug_ui(edit)]
    show_tile_bounds: bool,
    #[debug_ui(edit)]
    show_selection_bounds: bool,
    #[debug_ui(edit)]
    show_cursor_pos: bool,
    #[debug_ui(edit)]
    show_screen_origin: bool,
    #[debug_ui(edit)]
    show_world_perf_stats: bool,
    #[debug_ui(edit, separator)]
    show_render_perf_stats: bool,

    #[debug_ui(edit)]
    show_game_configs_debug: bool,
    #[debug_ui(edit)]
    show_game_world_debug: bool,
    #[debug_ui(edit)]
    show_game_systems_debug: bool,
    #[debug_ui(edit)]
    show_log_viewer_window: bool,
}

impl DebugSettingsMenu {
    pub fn new() -> Self {
        Self { draw_terrain: true,
               draw_buildings: true,
               draw_props: true,
               draw_units: true,
               draw_vegetation: true,
               ..Default::default() }
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
        flags
    }

    pub fn draw(&mut self,
                context: &mut sim::debug::DebugContext,
                sim: &mut Simulation,
                camera: &mut Camera,
                game_loop: &mut GameLoop,
                enable_tile_inspector: &mut bool) {
        let ui = context.ui_sys.builder();

        if let Some(_menu_bar) = ui.begin_main_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Game") {
                self.game_menu(context, game_loop, enable_tile_inspector);
            }

            if let Some(_menu) = ui.begin_menu("Camera") {
                self.camera_menu(context, camera);
            }

            if let Some(_menu) = ui.begin_menu("Save") {
                self.save_game_menu(context, game_loop);
            }

            if let Some(_menu) = ui.begin_menu("Map") {
                self.tile_map_menu(context, game_loop);
            }

            if let Some(_menu) = ui.begin_menu("Cheats") {
                self.cheats_menu(context);
            }

            if let Some(_menu) = ui.begin_menu("Debug") {
                self.debug_options_menu(context, game_loop);
            }

            self.menu_bar_text(context, game_loop);
        }

        self.draw_child_windows(context, sim);
    }

    fn menu_bar_text(&self, context: &mut sim::debug::DebugContext, game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();

        // Log error/warning count:
        {
            let log_viewer = game_loop.engine_mut().log_viewer();
            let (log_error_count, log_warning_count) = log_viewer.error_and_warning_count();

            if log_error_count != 0 || log_warning_count != 0 {
                ui.separator();
                ui.text_colored(Color::red().to_array(), format!(" Errs: {log_error_count} "));
                ui.text_colored(Color::yellow().to_array(), format!("Warns: {log_warning_count} "));
            }
        }

        // Gold units | population:
        {
            ui.separator();

            let gold_units_total = context.world.stats().treasury.gold_units_total;
            let gold_units_text = format!(" Gold: {gold_units_total} ");

            if gold_units_total == 0 {
                ui.text_colored(Color::red().to_array(), gold_units_text);
            } else {
                ui.text(gold_units_text);
            }

            let population = context.world.stats().population.total;
            ui.text(format!("Pop: {population} "));
        }
    }

    fn cheats_menu(&self, context: &mut sim::debug::DebugContext) {
        cheats::get_mut().draw_debug_ui(context.ui_sys);
    }

    fn game_menu(&mut self,
                 context: &mut sim::debug::DebugContext,
                 game_loop: &mut GameLoop,
                 enable_tile_inspector: &mut bool) {
        let ui = context.ui_sys.builder();

        if ui.menu_item("Quit") {
            game_loop.engine_mut().app().request_quit();
        }

        ui.separator();

        ui.checkbox("Game Configs", &mut self.show_game_configs_debug);
        ui.checkbox("Game World", &mut self.show_game_world_debug);
        ui.checkbox("Game Systems", &mut self.show_game_systems_debug);
        ui.checkbox("Log Viewer", &mut self.show_log_viewer_window);
        ui.checkbox("Tile Inspector", enable_tile_inspector);
        
        let mut show_popup_messages = super::show_popup_messages();
        if ui.checkbox("Popup Messages", &mut show_popup_messages) {
            super::set_show_popup_messages(show_popup_messages);
        }
    }

    fn camera_menu(&self, context: &mut sim::debug::DebugContext, camera: &mut Camera) {
        let ui = context.ui_sys.builder();

        let mut key_shortcut_zoom = !CameraGlobalSettings::get().disable_key_shortcut_zoom;
        if ui.checkbox("Keyboard Zoom", &mut key_shortcut_zoom) {
            CameraGlobalSettings::get_mut().disable_key_shortcut_zoom = !key_shortcut_zoom;
        }

        let mut mouse_scroll_zoom = !CameraGlobalSettings::get().disable_mouse_scroll_zoom;
        if ui.checkbox("Mouse Scroll Zoom", &mut mouse_scroll_zoom) {
            CameraGlobalSettings::get_mut().disable_mouse_scroll_zoom = !mouse_scroll_zoom;
        }

        let mut smooth_mouse_scroll_zoom = !CameraGlobalSettings::get().disable_smooth_mouse_scroll_zoom;
        if ui.checkbox("Smooth Mouse Scroll Zoom", &mut smooth_mouse_scroll_zoom) {
            CameraGlobalSettings::get_mut().disable_smooth_mouse_scroll_zoom = !smooth_mouse_scroll_zoom;
        }

        let (zoom_min, zoom_max) = camera.zoom_limits();
        let mut zoom = camera.current_zoom();

        if ui.slider("Zoom", zoom_min, zoom_max, &mut zoom) {
            camera.set_zoom(zoom);
        }

        let mut step_zoom = CameraGlobalSettings::get().fixed_step_zoom_amount;
        if ui.input_float("Step Zoom", &mut step_zoom)
            .display_format("%.1f")
            .step(0.5)
            .build()
        {
            CameraGlobalSettings::get_mut().fixed_step_zoom_amount = step_zoom.clamp(zoom_min, zoom_max);
        }

        ui.separator();

        let scroll_limits = camera.scroll_limits();
        let mut scroll = camera.current_scroll();

        if ui.slider_config("Scroll X", scroll_limits.0.x, scroll_limits.1.x)
             .display_format("%.1f")
             .build(&mut scroll.x)
        {
            camera.set_scroll(scroll);
        }

        if ui.slider_config("Scroll Y", scroll_limits.0.y, scroll_limits.1.y)
             .display_format("%.1f")
             .build(&mut scroll.y)
        {
            camera.set_scroll(scroll);
        }

        if ui.button("Re-center") {
            camera.center();
        }
    }

    fn debug_options_menu(&mut self,
                          context: &mut sim::debug::DebugContext,
                          game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();

        self.draw_debug_ui(context.ui_sys);

        let mut show_popup_messages = super::show_popup_messages();
        if ui.checkbox("Show Popup Messages", &mut show_popup_messages) {
            super::set_show_popup_messages(show_popup_messages);
        }

        // Debug grid options:
        ui.separator();

        let engine = game_loop.engine_mut();

        let mut line_thickness = engine.grid_line_thickness();
        if ui.slider_config("Grid thickness", MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS)
             .display_format("%.1f")
             .build(&mut line_thickness)
        {
            engine.set_grid_line_thickness(line_thickness);
        }

        ui.checkbox("Draw grid", &mut self.draw_grid);
        ui.checkbox("Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);
    }

    fn tile_map_menu(&mut self, context: &mut sim::debug::DebugContext, game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();

        // Preset tile maps:
        ui.separator();

        let preset_tile_map_names = debug::utils::preset_tile_maps_list();

        if ui.combo_simple_string("Preset",
                                  &mut self.preset_tile_map_number,
                                  &preset_tile_map_names)
        {
            self.preset_tile_map_number =
                self.preset_tile_map_number.min(preset_tile_map_names.len());
        }

        if ui.button("Load Preset") {
            log::info!(log::channel!("debug"),
                       "Loading preset tile map '{}' ...",
                       preset_tile_map_names[self.preset_tile_map_number]);
            game_loop.load_preset_map(self.preset_tile_map_number);
        }

        // Reset map options:
        ui.separator();

        if ui.button("Reset to empty map") {
            game_loop.reset_session(None);
        }

        if ui.button("Reset to dirt tiles") {
            let dirt_tile_def = TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Terrain,
                                                                      TERRAIN_GROUND_CATEGORY.hash,
                                                                      hash::fnv1a_from_str("dirt"));

            game_loop.reset_session(dirt_tile_def);
        }

        if ui.button("Reset to grass tiles") {
            let grass_tile_def =
                TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Terrain,
                                                      TERRAIN_GROUND_CATEGORY.hash,
                                                      hash::fnv1a_from_str("grass"));

            game_loop.reset_session(grass_tile_def);
        }

        if ui.button("Reset to water tiles") {
            let water_tile_def =
                TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Terrain,
                                                      TERRAIN_WATER_CATEGORY.hash,
                                                      hash::fnv1a_from_str("water"));

            game_loop.reset_session(water_tile_def);
        }
    }

    fn save_game_menu(&mut self, context: &mut sim::debug::DebugContext, game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();

        // Autosave:
        let mut autosave_enabled = game_loop.is_autosave_enabled();
        if ui.checkbox("Autosave", &mut autosave_enabled) {
            game_loop.enable_autosave(autosave_enabled);
        }

        if ui.button("Load Autosave") {
            game_loop.load_save_game(game::AUTOSAVE_FILE_NAME);
        }

        // Save game:
        ui.separator();

        if self.save_file_name.is_empty() {
            self.save_file_name = game::DEFAULT_SAVE_FILE_NAME.into();
        }

        ui.input_text("Save File", &mut self.save_file_name).build();

        if ui.button("Save") {
            if !self.save_file_name.is_empty() {
                game_loop.save_game(&self.save_file_name);
            } else {
                log::error!(log::channel!("debug"), "No save file name provided!");
            }
        }

        // Load save game:
        ui.separator();

        let save_files = game_loop.save_files_list();

        if ui.combo("Load File", &mut self.save_file_selected, &save_files, |s| s.to_string_lossy())
        {
            self.save_file_selected = self.save_file_selected.min(save_files.len());
        }

        if ui.button("Load") && !save_files.is_empty() {
            game_loop.load_save_game(&save_files[self.save_file_selected].to_string_lossy());
        }
    }

    fn draw_child_windows(&mut self, context: &mut sim::debug::DebugContext, sim: &mut Simulation) {
        if self.show_game_configs_debug {
            self.draw_game_configs_window(context);
        }

        if self.show_game_world_debug {
            self.draw_world_debug_window(context, sim);
        }

        if self.show_game_systems_debug {
            self.draw_game_systems_debug_window(context, sim);
        }
    }

    fn draw_game_configs_window(&mut self, context: &mut sim::debug::DebugContext) {
        let ui = context.ui_sys.builder();

        ui.window("Game Configs")
          .opened(&mut self.show_game_configs_debug)
          .position([200.0, 20.0], imgui::Condition::FirstUseEver)
          .size([400.0, 350.0], imgui::Condition::FirstUseEver)
          .build(|| {
              if let Some(_tab_bar) = ui.tab_bar("Configs Tab Bar") {
                  if let Some(_tab) = ui.tab_item("Engine/Game") {
                      GameConfigs::get().draw_debug_ui(context.ui_sys);
                  }
                  if let Some(_tab) = ui.tab_item("Buildings") {
                      BuildingConfigs::get().draw_debug_ui(context.ui_sys);
                  }
                  if let Some(_tab) = ui.tab_item("Units") {
                      UnitConfigs::get().draw_debug_ui(context.ui_sys);
                  }
                  if let Some(_tab) = ui.tab_item("Props") {
                      PropConfigs::get().draw_debug_ui(context.ui_sys);
                  }
              }
          });
    }

    fn draw_world_debug_window(&mut self,
                               context: &mut sim::debug::DebugContext,
                               sim: &mut Simulation) {
        let ui = context.ui_sys.builder();

        ui.window("World Debug")
          .opened(&mut self.show_game_world_debug)
          .position([300.0, 20.0], imgui::Condition::FirstUseEver)
          .size([400.0, 350.0], imgui::Condition::FirstUseEver)
          .build(|| sim.draw_world_debug_ui(context));
    }

    fn draw_game_systems_debug_window(&mut self,
                                      context: &mut sim::debug::DebugContext,
                                      sim: &mut Simulation) {
        let ui = context.ui_sys.builder();

        ui.window("Game Systems Debug")
          .opened(&mut self.show_game_systems_debug)
          .position([400.0, 20.0], imgui::Condition::FirstUseEver)
          .size([400.0, 350.0], imgui::Condition::FirstUseEver)
          .build(|| sim.draw_game_systems_debug_ui(context));
    }
}
