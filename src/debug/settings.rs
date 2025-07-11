use crate::{
    imgui_ui::UiSystem,
    game::sim::world::World,
    tile::{
        camera::Camera,
        sets::TileSets,
        map::{
            TileMap,
            TileMapLayerKind
        },
        rendering::{
            TileMapRenderFlags,
            TileMapRenderer,
            MAX_GRID_LINE_THICKNESS,
            MIN_GRID_LINE_THICKNESS
        }
    }
};

// ----------------------------------------------
// DebugSettingsMenu
// ----------------------------------------------

#[derive(Default)]
pub struct DebugSettingsMenu {
    start_open: bool,
    draw_terrain: bool,
    draw_buildings: bool,
    draw_props: bool,
    draw_units: bool,
    draw_vegetation: bool,
    draw_grid: bool,
    draw_grid_ignore_depth: bool,
    show_tile_bounds: bool,
    show_terrain_debug: bool,
    show_buildings_debug: bool,
    show_props_debug: bool,
    show_units_debug: bool,
    show_vegetation_debug: bool,
    show_blockers: bool,
    show_selection_bounds: bool,
    show_cursor_pos: bool,
    show_screen_origin: bool,
    show_render_stats: bool,
}

impl DebugSettingsMenu {
    pub fn new(start_open: bool) -> Self {
        Self {
            start_open: start_open,
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

    pub fn selected_render_flags(&self) -> TileMapRenderFlags {
        let mut flags = TileMapRenderFlags::empty();
        if self.draw_terrain           { flags.insert(TileMapRenderFlags::DrawTerrain); }
        if self.draw_buildings         { flags.insert(TileMapRenderFlags::DrawBuildings); }
        if self.draw_props             { flags.insert(TileMapRenderFlags::DrawProps); }
        if self.draw_units             { flags.insert(TileMapRenderFlags::DrawUnits); }
        if self.draw_vegetation        { flags.insert(TileMapRenderFlags::DrawVegetation); }
        if self.draw_grid              { flags.insert(TileMapRenderFlags::DrawGrid); }
        if self.draw_grid_ignore_depth { flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth); }
        if self.show_tile_bounds       { flags.insert(TileMapRenderFlags::DrawDebugBounds); }
        if self.show_terrain_debug     { flags.insert(TileMapRenderFlags::DrawTerrainTileDebug); }
        if self.show_buildings_debug   { flags.insert(TileMapRenderFlags::DrawBuildingsTileDebug); }
        if self.show_props_debug       { flags.insert(TileMapRenderFlags::DrawPropsTileDebug); }
        if self.show_units_debug       { flags.insert(TileMapRenderFlags::DrawUnitsTileDebug); }
        if self.show_vegetation_debug  { flags.insert(TileMapRenderFlags::DrawVegetationTileDebug); }
        if self.show_blockers          { flags.insert(TileMapRenderFlags::DrawBlockersTileDebug); }
        flags
    }

    pub fn draw<'tile_sets>(&mut self,
                            camera: &mut Camera,
                            world: &mut World,
                            tile_map_renderer: &mut TileMapRenderer,
                            tile_map: &mut TileMap<'tile_sets>,
                            tile_sets: &'tile_sets TileSets,
                            ui_sys: &UiSystem) {

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        let ui = ui_sys.builder();

        ui.window("Debug Settings")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position([5.0, 5.0], imgui::Condition::FirstUseEver)
            .build(|| {
                self.camera_dropdown(ui, camera);
                self.map_grid_dropdown(ui, tile_map_renderer);
                self.debug_draw_dropdown(ui);
                self.reset_map_dropdown(ui, world, tile_map, tile_sets);
            });
    }

    fn camera_dropdown(&self, ui: &imgui::Ui, camera: &mut Camera) {

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
                         ui: &imgui::Ui,
                         tile_map_renderer: &mut TileMapRenderer) {

        if !ui.collapsing_header("Grid", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut line_thickness = tile_map_renderer.grid_line_thickness();
        if ui.slider_config("Grid thickness", MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS)
            .display_format("%.1f")
            .build(&mut line_thickness) {
            tile_map_renderer.set_grid_line_thickness(line_thickness);
        }

        ui.checkbox("Draw grid", &mut self.draw_grid);
        ui.checkbox("Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);
    }

    fn debug_draw_dropdown(&mut self, ui: &imgui::Ui) {

        if !ui.collapsing_header("Debug Draw", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        ui.checkbox("Draw terrain", &mut self.draw_terrain);
        ui.checkbox("Draw buildings", &mut self.draw_buildings);
        ui.checkbox("Draw props", &mut self.draw_props);
        ui.checkbox("Draw units", &mut self.draw_units);
        ui.checkbox("Draw vegetation", &mut self.draw_vegetation);
        ui.separator();
        ui.checkbox("Show terrain debug", &mut self.show_terrain_debug);
        ui.checkbox("Show buildings debug", &mut self.show_buildings_debug);
        ui.checkbox("Show props debug", &mut self.show_props_debug);
        ui.checkbox("Show units debug", &mut self.show_units_debug);
        ui.checkbox("Show vegetation debug", &mut self.show_vegetation_debug);
        ui.checkbox("Show blocker tiles debug", &mut self.show_blockers);
        ui.separator();
        ui.checkbox("Show tile bounds", &mut self.show_tile_bounds);
        ui.checkbox("Show selection bounds", &mut self.show_selection_bounds);
        ui.checkbox("Show cursor pos", &mut self.show_cursor_pos);
        ui.checkbox("Show screen origin", &mut self.show_screen_origin);
        ui.checkbox("Show render stats", &mut self.show_render_stats);
    }

    fn reset_map_dropdown<'tile_sets>(&self,
                                      ui: &imgui::Ui,
                                      world: &mut World,
                                      tile_map: &mut TileMap<'tile_sets>,
                                      tile_sets: &'tile_sets TileSets) {

        if !ui.collapsing_header("Reset Map", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.                    
        }

        if ui.button("Reset empty") {
            tile_map.reset(None);
            world.reset();
        }

        if ui.button("Reset to dirt tiles") {
            let dirt_til_def = tile_sets.find_tile_def_by_name(
                TileMapLayerKind::Terrain,
                "ground",
                "dirt");
            tile_map.reset(dirt_til_def);
            world.reset();
        }

        if ui.button("Reset to grass tiles") {
            let grass_tile_def = tile_sets.find_tile_def_by_name(
                TileMapLayerKind::Terrain,
                "ground",
                "grass");
            tile_map.reset(grass_tile_def);
            world.reset();
        }
    }
}
