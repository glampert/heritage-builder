#![allow(dead_code)]

mod utils;
mod app;
mod render;
mod ui;
mod tile;

use utils::*;
use app::*;
use app::input::*;
use ui::*;
use render::*;
use tile::def::*;
use tile::sets::*;
use tile::map::*;

// ----------------------------------------------
// TestUiState
// ----------------------------------------------

#[derive(Default)]
pub struct TestUiState<'a> {
    scaling: i32,
    offset_x: i32,
    offset_y: i32,
    tile_spacing: i32,
    grid_thickness: f32,

    draw_terrain: bool,
    draw_buildings: bool,
    draw_units: bool,
    draw_grid: bool,
    draw_grid_ignore_depth: bool,

    draw_terrain_debug_info: bool,
    draw_buildings_debug_info: bool,
    draw_units_debug_info: bool,
    draw_tile_debug_bounds: bool,
    draw_selected_tile_bounds: bool,
    draw_cursor_pos: bool,

    selected_tile: Option<&'a TileDef>,
    selected_index: [Option<usize>; TILE_MAP_LAYER_COUNT],
}

impl<'a> TestUiState<'a> {
    pub fn new() -> Self {
        Self {
            scaling: 2,
            offset_x: 448,
            offset_y: 600,
            tile_spacing: 4,
            grid_thickness: 3.0,
            draw_terrain: true,
            draw_buildings: true,
            draw_units: true,
            draw_grid: true,
            ..Default::default()
        }
    }

    pub fn selected_render_flags(&self) -> TileMapRenderFlags {
        let mut flags = TileMapRenderFlags::None;
        if self.draw_terrain              { flags.insert(TileMapRenderFlags::DrawTerrain); }
        if self.draw_buildings            { flags.insert(TileMapRenderFlags::DrawBuildings); }
        if self.draw_units                { flags.insert(TileMapRenderFlags::DrawUnits); }
        if self.draw_grid                 { flags.insert(TileMapRenderFlags::DrawGrid); }
        if self.draw_grid_ignore_depth    { flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth); }
        if self.draw_terrain_debug_info   { flags.insert(TileMapRenderFlags::DrawTerrainTileDebugInfo); }
        if self.draw_buildings_debug_info { flags.insert(TileMapRenderFlags::DrawBuildingsTileDebugInfo); }
        if self.draw_units_debug_info     { flags.insert(TileMapRenderFlags::DrawUnitsTileDebugInfo); }
        if self.draw_tile_debug_bounds    { flags.insert(TileMapRenderFlags::DrawTileDebugBounds); }
        flags
    }

    pub fn draw_debug_controls(&mut self, tile_map_renderer: &mut TileMapRenderer, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        ui.window("CitySim")
            .flags(window_flags)
            .position([5.0, 5.0], imgui::Condition::FirstUseEver)
            .build(|| {
                if ui.slider("Scaling", 1, 10, &mut self.scaling) {
                    tile_map_renderer.set_draw_scaling(self.scaling);
                }

                if ui.slider("Offset X", -2048, 2048, &mut self.offset_x) {
                    tile_map_renderer.set_draw_offset(Point2D::new(self.offset_x, self.offset_y));
                }

                if ui.slider("Offset Y", -2048, 2048, &mut self.offset_y) {
                    tile_map_renderer.set_draw_offset(Point2D::new(self.offset_x, self.offset_y));
                }

                if ui.slider("Tile spacing", 0, 10, &mut self.tile_spacing) {
                    tile_map_renderer.set_tile_spacing(self.tile_spacing);
                }

                if ui.slider("Grid thickness", 0.5, 20.0, &mut self.grid_thickness) {
                    tile_map_renderer.set_grid_line_thickness(self.grid_thickness);
                }

                ui.checkbox("Draw terrain", &mut self.draw_terrain);
                ui.checkbox("Draw buildings", &mut self.draw_buildings);
                ui.checkbox("Draw units", &mut self.draw_units);
                ui.checkbox("Draw grid", &mut self.draw_grid);
                ui.checkbox("Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);
                ui.checkbox("Draw terrain debug", &mut self.draw_terrain_debug_info);
                ui.checkbox("Draw buildings debug", &mut self.draw_buildings_debug_info);
                ui.checkbox("Draw units debug", &mut self.draw_units_debug_info);
                ui.checkbox("Draw tile bounds", &mut self.draw_tile_debug_bounds);
                ui.checkbox("Draw selection bounds", &mut self.draw_selected_tile_bounds);
                ui.checkbox("Draw cursor pos", &mut self.draw_cursor_pos);
            });
    }

    pub fn draw_selected_tile(&self, render_sys: &mut RenderSystem, cursor_pos: Point2D, transform: &WorldToScreenTransform) {
        if let Some(selected_tile) = self.selected_tile {

            let rect = Rect2D::new(cursor_pos, selected_tile.draw_size);
            let offset = Point2D::new(-(selected_tile.draw_size.width / 2), -(selected_tile.draw_size.height / 2));
            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset, 0);

            render_sys.draw_textured_colored_rect(
                cursor_transform.scale_and_offset_rect(rect),
                &selected_tile.tex_info.coords,
                selected_tile.tex_info.texture,
                selected_tile.color);

            if self.draw_selected_tile_bounds {
                render_sys.draw_wireframe_rect_fast(cursor_transform.scale_and_offset_rect(rect), Color::red());
            }
        }
    }

    pub fn draw_tile_selection_menu(&mut self, ui_sys: &UiSystem, tex_cache: &TextureCache, tile_sets: &'a TileSets) {
        let ui = ui_sys.builder();

        let tile_size = [ BASE_TILE_SIZE.width as f32, BASE_TILE_SIZE.height as f32 ];
        let tiles_per_row = 2;
        let padding_between_tiles = 4.0;

        let window_width = (tile_size[0] + padding_between_tiles) * tiles_per_row as f32;
        let window_margin = 35.0; // pixels from the right edge

        // X position = screen width - estimated window width - margin
        // Y position = 10px
        let window_pos = [
            ui.io().display_size[0] - window_width - window_margin,
            5.0
        ];

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        ui.window("Tile Selection")
            .flags(window_flags)
            .position(window_pos, imgui::Condition::FirstUseEver)
            .build(|| {
                self.draw_tile_list(TileMapLayerKind::Terrain,
                                    ui_sys,
                                    tex_cache,
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);

                ui.separator();

                self.draw_tile_list(TileMapLayerKind::Buildings,
                                    ui_sys,
                                    tex_cache,
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);

                ui.separator();

                self.draw_tile_list(TileMapLayerKind::Units,
                                    ui_sys,
                                    tex_cache,
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);
            });
    }

    pub fn clear_selection(&mut self) {
        self.selected_tile  = None;
        self.selected_index = [None; TILE_MAP_LAYER_COUNT];
    }

    fn draw_tile_list(&mut self,
                      layer: TileMapLayerKind,
                      ui_sys: &UiSystem,
                      tex_cache: &TextureCache,
                      tile_sets: &'a TileSets,
                      tile_size: [f32; 2],
                      tiles_per_row: usize,
                      padding_between_tiles: f32) {

        let ui = ui_sys.builder();
        ui.text(layer.to_string());

        let tile_sets_iter = tile_sets.defs(
            move |tile_def| {
                match layer {
                    TileMapLayerKind::Terrain   => tile_def.is_terrain(),
                    TileMapLayerKind::Buildings => tile_def.is_building(),
                    TileMapLayerKind::Units     => tile_def.is_unit(),
                }
            });

        for (i, tile_def) in tile_sets_iter.enumerate() {
            let ui_texture = ui_sys.to_ui_texture(tex_cache, tile_def.tex_info.texture);

            let is_selected = self.selected_index[layer as usize] == Some(i);
            let bg_color = if is_selected {
                Color::white().to_array()
            } else {
                Color::gray().to_array()
            };

            let clicked = ui.image_button_config(tile_def.name.as_str(), ui_texture, tile_size)
                .background_col(bg_color)
                .tint_col(tile_def.color.to_array())
                .build();

            // Show tooltip when hovered:
            if ui.is_item_hovered() {
                ui.tooltip_text(&tile_def.name);
            }

            if clicked {
                self.clear_selection();

                self.selected_tile = Some(tile_def);
                self.selected_index[layer as usize] = Some(i);
            }

            // Move to next column unless it's the last in row.
            if (i + 1) % tiles_per_row != 0 {
                ui.same_line_with_spacing(0.0, padding_between_tiles);
            }
        }
    }
}

// ----------------------------------------------
// main()
// ----------------------------------------------

fn main() {
    let cwd = std::env::current_dir().unwrap();
    println!("The current directory is \"{}\".", cwd.display());

    let mut app = ApplicationBuilder::new()
        .window_title("CitySim")
        .window_size(Size2D::new(1024, 768))
        .fullscreen(false)
        .build();

    let input_sys = app.create_input_system();

    let mut render_sys = RenderSystem::new(app.window_size());
    let mut ui_sys = UiSystem::new(&app);
    let mut tex_cache = TextureCache::new(128);

    let tile_sets = TileSets::with_test_tiles(&mut tex_cache);
    let mut tile_map = TileMap::with_test_map(&tile_sets);

    let mut test_ui = TestUiState::new();

    let mut tile_map_renderer = TileMapRenderer::new();
    tile_map_renderer
        .set_draw_scaling(test_ui.scaling)
        .set_draw_offset(Point2D::new(test_ui.offset_x, test_ui.offset_y))
        .set_grid_line_thickness(test_ui.grid_thickness)
        .set_tile_spacing(test_ui.tile_spacing);

    let mut tile_selection = TileSelection::new();

    let mut frame_clock = FrameClock::new();

    while !app.should_quit() {
        frame_clock.begin_frame();

        let transform = tile_map_renderer.world_to_screen_transform();
        let cursor_pos = input_sys.cursor_pos();
        let events = app.poll_events();

        for event in events {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                }
                ApplicationEvent::WindowResize(window_size) => {
                    render_sys.set_window_size(window_size);
                }
                ApplicationEvent::KeyInput(key, action, _modifiers) => {
                    ui_sys.on_key_input(key, action);

                    if key == InputKey::Escape {
                        test_ui.clear_selection();
                    }
                }
                ApplicationEvent::CharInput(c) => {
                    ui_sys.on_char_input(c);
                }
                ApplicationEvent::Scroll(amount) => {
                    ui_sys.on_scroll(amount);
                }
                ApplicationEvent::MouseButton(button, action, _modifiers) => {
                    let range_selecting = tile_selection.on_mouse_click(button, action, cursor_pos);
                    if !range_selecting {
                        tile_map.clear_selection(&mut tile_selection);
                    }
                }
            }
            println!("ApplicationEvent::{:?}", event);
        }

        tile_selection.update(cursor_pos);
        tile_map.update_selection(&mut tile_selection, &transform);

        ui_sys.begin_frame(&app, &input_sys, frame_clock.delta_time());
        render_sys.begin_frame();
        {
            tile_map_renderer.draw_map(
                &mut render_sys,
                &ui_sys,
                &tile_map,
                test_ui.selected_render_flags());

            tile_selection.draw(&mut render_sys);

            test_ui.draw_debug_controls(&mut tile_map_renderer, &ui_sys);
            test_ui.draw_tile_selection_menu(&ui_sys, &tex_cache, &tile_sets);
            test_ui.draw_selected_tile(&mut render_sys, cursor_pos, &transform);

            if test_ui.draw_cursor_pos {
                ui_sys.draw_debug(&mut render_sys);
            }
        }
        render_sys.end_frame(&tex_cache);
        ui_sys.end_frame();

        app.present();

        frame_clock.end_frame();
    }
}
