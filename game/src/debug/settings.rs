use proc_macros::DrawDebugUi;

use crate::{
    log,
    debug,
    imgui_ui::UiSystem,
    utils::hash::{self},
    engine::{Engine, config::Configs},
    game::{
        self,
        cheats,
        GameLoop,
        sim::{self, Simulation},
        config::GameConfigs,
        unit::config::UnitConfigs,
        building::config::BuildingConfigs
    },
    tile::{
        TileMapLayerKind,
        camera::Camera,
        sets::TERRAIN_GROUND_CATEGORY,
        rendering::{
            TileMapRenderFlags,
            MAX_GRID_LINE_THICKNESS,
            MIN_GRID_LINE_THICKNESS
        }
    }
};

// ----------------------------------------------
// DebugSettingsMenu
// ----------------------------------------------

#[derive(Default, DrawDebugUi)]
pub struct DebugSettingsMenu {
    #[debug_ui(skip)] start_open: bool,

    #[debug_ui(skip)] draw_grid: bool,
    #[debug_ui(skip)] draw_grid_ignore_depth: bool,

    #[debug_ui(skip)] preset_tile_map_number: usize,
    #[debug_ui(skip)] save_file_name: String,
    #[debug_ui(skip)] save_file_selected: usize,

    #[debug_ui(edit)] draw_terrain: bool,
    #[debug_ui(edit)] draw_buildings: bool,
    #[debug_ui(edit)] draw_props: bool,
    #[debug_ui(edit)] draw_units: bool,
    #[debug_ui(edit, separator)] draw_vegetation: bool,

    #[debug_ui(edit)] show_terrain_debug: bool,
    #[debug_ui(edit)] show_buildings_debug: bool,
    #[debug_ui(edit)] show_props_debug: bool,
    #[debug_ui(edit)] show_units_debug: bool,
    #[debug_ui(edit)] show_vegetation_debug: bool,
    #[debug_ui(edit, separator)] show_blocker_tiles_debug: bool,

    #[debug_ui(edit)] show_tile_bounds: bool,
    #[debug_ui(edit)] show_selection_bounds: bool,
    #[debug_ui(edit)] show_cursor_pos: bool,
    #[debug_ui(edit)] show_screen_origin: bool,
    #[debug_ui(edit)] show_render_stats: bool,
    #[debug_ui(edit)] show_game_configs: bool,
    #[debug_ui(edit)] show_world_debug: bool,
    #[debug_ui(edit)] show_game_systems_debug: bool,
    #[debug_ui(edit)] show_log_viewer_window: bool,
}

impl DebugSettingsMenu {
    pub fn new(start_open: bool) -> Self {
        Self {
            start_open,
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

    pub fn show_render_stats(&self) -> bool {
        self.show_render_stats
    }

    pub fn show_log_viewer_window(&mut self) -> &mut bool {
        &mut self.show_log_viewer_window
    }

    pub fn selected_render_flags(&self) -> TileMapRenderFlags {
        let mut flags = TileMapRenderFlags::empty();
        if self.draw_terrain             { flags.insert(TileMapRenderFlags::DrawTerrain); }
        if self.draw_buildings           { flags.insert(TileMapRenderFlags::DrawBuildings); }
        if self.draw_props               { flags.insert(TileMapRenderFlags::DrawProps); }
        if self.draw_units               { flags.insert(TileMapRenderFlags::DrawUnits); }
        if self.draw_vegetation          { flags.insert(TileMapRenderFlags::DrawVegetation); }
        if self.draw_grid                { flags.insert(TileMapRenderFlags::DrawGrid); }
        if self.draw_grid_ignore_depth   { flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth); }
        if self.show_tile_bounds         { flags.insert(TileMapRenderFlags::DrawDebugBounds); }
        if self.show_terrain_debug       { flags.insert(TileMapRenderFlags::DrawTerrainTileDebug); }
        if self.show_buildings_debug     { flags.insert(TileMapRenderFlags::DrawBuildingsTileDebug); }
        if self.show_props_debug         { flags.insert(TileMapRenderFlags::DrawPropsTileDebug); }
        if self.show_units_debug         { flags.insert(TileMapRenderFlags::DrawUnitsTileDebug); }
        if self.show_vegetation_debug    { flags.insert(TileMapRenderFlags::DrawVegetationTileDebug); }
        if self.show_blocker_tiles_debug { flags.insert(TileMapRenderFlags::DrawBlockersTileDebug); }
        flags
    }

    pub fn draw<'config>(&mut self,
                         context: &mut sim::debug::DebugContext<'config, '_, '_, '_, 'config>,
                         sim: &mut Simulation<'config>,
                         camera: &mut Camera,
                         engine: &mut dyn Engine,
                         game_loop: &mut GameLoop<'config>) {

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE;

        let ui = context.ui_sys.builder();

        ui.window("Debug Settings")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position([5.0, 5.0], imgui::Condition::FirstUseEver)
            .build(|| {
                self.camera_dropdown(context, camera);
                self.map_grid_dropdown(context, engine);
                self.debug_draw_dropdown(context);
                self.reset_map_dropdown(context, game_loop);
                self.preset_maps_dropdown(context, game_loop);
                self.save_game_dropdown(context, game_loop);
                cheats::draw_debug_ui(context.ui_sys);
            });

        if self.show_game_configs {
            self.draw_game_configs_window(context.ui_sys);
        }

        if self.show_world_debug {
            self.draw_world_debug_window(context, sim);
        }

        if self.show_game_systems_debug {
            self.draw_game_systems_debug_window(context, sim);
        }
    }

    fn camera_dropdown(&self, context: &mut sim::debug::DebugContext, camera: &mut Camera) {
        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Camera", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let zoom_limits = camera.zoom_limits();
        let mut zoom = camera.current_zoom();

        if ui.slider("Zoom", zoom_limits.0, zoom_limits.1, &mut zoom) {
            camera.set_zoom(zoom);
        }

        let scroll_limits = camera.scroll_limits();
        let mut scroll = camera.current_scroll();

        if ui.slider_config("Scroll X", scroll_limits.0.x, scroll_limits.1.x)
            .display_format("%.1f")
            .build(&mut scroll.x) {
            camera.set_scroll(scroll);
        }

        if ui.slider_config("Scroll Y", scroll_limits.0.y, scroll_limits.1.y)
            .display_format("%.1f")
            .build(&mut scroll.y) {
            camera.set_scroll(scroll);
        }

        if ui.button("Re-center") {
            camera.center();
        }
    }

    fn map_grid_dropdown(&mut self,
                         context: &mut sim::debug::DebugContext,
                         engine: &mut dyn Engine) {

        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Grid", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut line_thickness = engine.grid_line_thickness();
        if ui.slider_config("Grid thickness", MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS)
            .display_format("%.1f")
            .build(&mut line_thickness) {
            engine.set_grid_line_thickness(line_thickness);
        }

        ui.checkbox("Draw grid", &mut self.draw_grid);
        ui.checkbox("Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);
    }

    fn debug_draw_dropdown(&mut self, context: &mut sim::debug::DebugContext) {
        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Debug Draw", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        self.draw_debug_ui(context.ui_sys);

        let mut show_popup_messages = super::show_popup_messages();
        if ui.checkbox("Show Popup Messages", &mut show_popup_messages) {
            super::set_show_popup_messages(show_popup_messages);
        }
    }

    fn reset_map_dropdown<'tile_sets>(&self,
                                      context: &mut sim::debug::DebugContext<'_, '_, '_, '_, 'tile_sets>,
                                      game_loop: &mut GameLoop<'tile_sets>) {

        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Reset Map", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Reset empty") {
            game_loop.reset_session(None);
        }

        if ui.button("Reset to dirt tiles") {
            let dirt_tile_def = context.tile_sets.find_tile_def_by_hash(
                TileMapLayerKind::Terrain,
                TERRAIN_GROUND_CATEGORY.hash,
                hash::fnv1a_from_str("dirt"));

            game_loop.reset_session(dirt_tile_def);
        }

        if ui.button("Reset to grass tiles") {
            let grass_tile_def = context.tile_sets.find_tile_def_by_hash(
                TileMapLayerKind::Terrain,
                TERRAIN_GROUND_CATEGORY.hash,
                hash::fnv1a_from_str("grass"));

            game_loop.reset_session(grass_tile_def);
        }
    }

    fn preset_maps_dropdown(&mut self, context: &mut sim::debug::DebugContext, game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Preset Maps", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let preset_tile_map_names = debug::utils::preset_tile_maps_list();

        if ui.combo_simple_string("Preset", &mut self.preset_tile_map_number, &preset_tile_map_names) {
            self.preset_tile_map_number = self.preset_tile_map_number.min(preset_tile_map_names.len());
        }

        if ui.button("Load Preset") {
            log::info!(log::channel!("debug"), "Loading preset tile map '{}' ...", preset_tile_map_names[self.preset_tile_map_number]);
            game_loop.load_preset_map(self.preset_tile_map_number);
        }
    }

    fn save_game_dropdown(&mut self, context: &mut sim::debug::DebugContext, game_loop: &mut GameLoop) {
        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Save Game", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut autosave_enabled = game_loop.is_autosave_enabled();
        if ui.checkbox("Autosave", &mut autosave_enabled) {
            game_loop.enable_autosave(autosave_enabled);
        }

        if ui.button("Load Autosave") {
            game_loop.load_save_game(game::AUTOSAVE_FILE_NAME);
        }

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

        ui.separator();

        let save_files = game_loop.save_files_list();

        if ui.combo("Load File", &mut self.save_file_selected, &save_files, |s| s.to_string_lossy()) {
            self.save_file_selected = self.save_file_selected.min(save_files.len());
        }

        if ui.button("Load") && !save_files.is_empty() {
            game_loop.load_save_game(&save_files[self.save_file_selected].to_string_lossy());
        }
    }

    fn draw_game_configs_window(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.window("Game Configs")
            .opened(&mut self.show_game_configs)
            .position([300.0, 5.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| {
                if let Some(_tab_bar) = ui.tab_bar("Configs Tab Bar") {
                    if let Some(_tab) = ui.tab_item("Engine/Game") {
                        GameConfigs::get().draw_debug_ui(ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Buildings") {
                        BuildingConfigs::get().draw_debug_ui(ui_sys);
                    }
                    if let Some(_tab) = ui.tab_item("Units") {
                        UnitConfigs::get().draw_debug_ui(ui_sys);
                    }
                }
            });
    }

    fn draw_world_debug_window(&mut self,
                               context: &mut sim::debug::DebugContext,
                               sim: &mut Simulation) {

        let ui = context.ui_sys.builder();
        ui.window("World Debug")
            .opened(&mut self.show_world_debug)
            .position([250.0, 5.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| sim.draw_world_debug_ui(context));
    }

    fn draw_game_systems_debug_window<'config>(&mut self,
                                               context: &mut sim::debug::DebugContext<'config, '_, '_, '_, '_>,
                                               sim: &mut Simulation<'config>) {

        let ui = context.ui_sys.builder();
        ui.window("Game Systems Debug")
            .opened(&mut self.show_game_systems_debug)
            .position([400.0, 5.0], imgui::Condition::FirstUseEver)
            .size([400.0, 350.0], imgui::Condition::FirstUseEver)
            .build(|| sim.draw_game_systems_debug_ui(context));
    }
}
