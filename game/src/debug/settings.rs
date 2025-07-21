use proc_macros::DrawDebugUi;

use crate::{
    imgui_ui::UiSystem,
    game::sim::world::World,
    utils::hash::{self},
    tile::{
        camera::Camera,
        sets::{
            TileSets,
            TERRAIN_GROUND_CATEGORY
        },
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

#[derive(Default, DrawDebugUi)]
pub struct DebugSettingsMenu {
    #[debug_ui(skip)]
    start_open: bool,

    #[debug_ui(skip)]
    draw_grid: bool,
    #[debug_ui(skip)]
    draw_grid_ignore_depth: bool,

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
    #[debug_ui(edit)] show_popup_messages: bool,
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

    pub fn show_popup_messages(&self) -> bool {
        self.show_popup_messages
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
                self.debug_draw_dropdown(ui_sys);
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

    fn debug_draw_dropdown(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Debug Draw", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        self.draw_debug_ui(ui_sys);
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
            let dirt_til_def = tile_sets.find_tile_def_by_hash(
                TileMapLayerKind::Terrain,
                TERRAIN_GROUND_CATEGORY.hash,
                hash::fnv1a_from_str("dirt"));
            tile_map.reset(dirt_til_def);
            world.reset();
        }

        if ui.button("Reset to grass tiles") {
            let grass_tile_def = tile_sets.find_tile_def_by_hash(
                TileMapLayerKind::Terrain,
                TERRAIN_GROUND_CATEGORY.hash,
                hash::fnv1a_from_str("grass"));
            tile_map.reset(grass_tile_def);
            world.reset();
        }
    }
}
