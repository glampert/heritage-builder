use crate::{
    ui::UiSystem,
    app::input::{MouseButton, InputAction},
    render::{RenderSystem, TextureCache, TextureHandle},
    utils::{Color, Point2D, Rect2D, Size2D, RectTexCoords, WorldToScreenTransform}
};

use super::{
    sets::TileSets,
    def::{TileDef, BASE_TILE_SIZE},
    map::{TileMapLayerKind, TILE_MAP_LAYER_COUNT},
    rendering::{TileMapRenderFlags, TileMapRenderer, TILE_INVALID_COLOR}
};

// ----------------------------------------------
// DebugSettingsMenu
// ----------------------------------------------

#[derive(Default)]
pub struct DebugSettingsMenu {
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
    draw_debug_building_blockers: bool,
    draw_units_debug_info: bool,
    draw_tile_debug_bounds: bool,
    draw_selected_tile_bounds: bool,
    draw_cursor_pos: bool,
    draw_screen_origin: bool,
}

impl DebugSettingsMenu {
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

    pub fn show_selected_tile_bounds(&self) -> bool {
        self.draw_selected_tile_bounds
    }

    pub fn show_cursor_pos(&self) -> bool {
        self.draw_cursor_pos
    }

    pub fn show_screen_origin(&self) -> bool {
        self.draw_screen_origin
    }

    pub fn selected_render_flags(&self) -> TileMapRenderFlags {
        let mut flags = TileMapRenderFlags::None;
        if self.draw_terrain                 { flags.insert(TileMapRenderFlags::DrawTerrain); }
        if self.draw_buildings               { flags.insert(TileMapRenderFlags::DrawBuildings); }
        if self.draw_units                   { flags.insert(TileMapRenderFlags::DrawUnits); }
        if self.draw_grid                    { flags.insert(TileMapRenderFlags::DrawGrid); }
        if self.draw_grid_ignore_depth       { flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth); }
        if self.draw_terrain_debug_info      { flags.insert(TileMapRenderFlags::DrawTerrainTileDebugInfo); }
        if self.draw_buildings_debug_info    { flags.insert(TileMapRenderFlags::DrawBuildingsTileDebugInfo); }
        if self.draw_debug_building_blockers { flags.insert(TileMapRenderFlags::DrawDebugBuildingBlockers); }
        if self.draw_units_debug_info        { flags.insert(TileMapRenderFlags::DrawUnitsTileDebugInfo); }
        if self.draw_tile_debug_bounds       { flags.insert(TileMapRenderFlags::DrawTileDebugBounds); }
        flags
    }

    pub fn apply_render_settings(&self, tile_map_renderer: &mut TileMapRenderer) {
        tile_map_renderer
            .set_draw_scaling(self.scaling)
            .set_draw_offset(Point2D::new(self.offset_x, self.offset_y))
            .set_grid_line_thickness(self.grid_thickness)
            .set_tile_spacing(self.tile_spacing);
    }

    pub fn draw(&mut self, tile_map_renderer: &mut TileMapRenderer, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        ui.window("Debug Settings")
            .flags(window_flags)
            .collapsed(true, imgui::Condition::FirstUseEver)
            .position([5.0, 5.0], imgui::Condition::FirstUseEver)
            .build(|| {
                if ui.slider("Scaling/Zoom", 1, 10, &mut self.scaling) {
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
                ui.checkbox("Draw building blockers", &mut self.draw_debug_building_blockers);
                ui.checkbox("Draw units debug", &mut self.draw_units_debug_info);
                ui.checkbox("Draw tile bounds", &mut self.draw_tile_debug_bounds);
                ui.checkbox("Draw selection bounds", &mut self.draw_selected_tile_bounds);
                ui.checkbox("Draw cursor pos", &mut self.draw_cursor_pos);
                ui.checkbox("Draw screen origin", &mut self.draw_screen_origin);
            });
    }
}

// ----------------------------------------------
// TileListMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TileListMenu<'a> {
    selected_tile: Option<&'a TileDef>,
    selected_index: [Option<usize>; TILE_MAP_LAYER_COUNT], // For highlighting the selected button.
    left_mouse_button_held: bool,
    clear_button_image: TextureHandle,
}

impl<'a> TileListMenu<'a> {
    pub fn new(tex_cache: &mut TextureCache) -> Self {
        Self {
            clear_button_image: tex_cache.load_texture("assets/ui/x.png"),
            ..Default::default()
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_tile = None;
        self.selected_index = [None; TILE_MAP_LAYER_COUNT];
        self.left_mouse_button_held = false;
    }

    pub fn has_selection(&self) -> bool {
        self.selected_tile.is_some()
    }

    pub fn current_selection(&self) -> Option<&'a TileDef> {
        self.selected_tile
    }

    pub fn can_place_tile(&self) -> bool {
        self.left_mouse_button_held && self.has_selection()
    }

    pub fn on_mouse_click(&mut self, button: MouseButton, action: InputAction) -> bool {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_held = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_held = false;
            }
            return true;
        } else if button == MouseButton::Right {
            return false;
        }
        return false;
    }

    pub fn draw(&mut self,
                render_sys: &mut RenderSystem,
                ui_sys: &UiSystem,
                tex_cache: &TextureCache,
                tile_sets: &'a TileSets,
                cursor_pos: Point2D,
                transform: &WorldToScreenTransform,
                has_valid_placement: bool,
                draw_tile_bounds: bool) {

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

                ui.separator();

                ui.text("Tools");
                {
                    let ui_texture = ui_sys.to_ui_texture(tex_cache, self.clear_button_image);

                    let is_selected = self.selected_tile.is_some_and(|t| t.is_empty());
                    let bg_color = if is_selected {
                        Color::white().to_array()
                    } else {
                        Color::gray().to_array()
                    };

                    let clicked = ui.image_button_config("Clear", ui_texture, tile_size)
                        .background_col(bg_color)
                        .build();

                    if ui.is_item_hovered() {
                        ui.tooltip_text("Clear tiles");
                    }

                    if clicked {
                        self.clear_selection();
                        self.selected_tile = Some(TileDef::empty());
                    }
                }
            });

        self.draw_selected_tile(render_sys, cursor_pos, transform, has_valid_placement, draw_tile_bounds);
    }

    fn draw_selected_tile(&self,
                          render_sys: &mut RenderSystem,
                          cursor_pos: Point2D,
                          transform: &WorldToScreenTransform,
                          has_valid_placement: bool,
                          draw_tile_bounds: bool) {

        if let Some(selected_tile) = self.selected_tile {
            let is_clear_selected = self.selected_tile.is_some_and(|t| t.is_empty());
            if is_clear_selected {
                const CLEAR_ICON_SIZE: Size2D = Size2D::new(64, 32);

                let rect = Rect2D::new(
                    Point2D::new(cursor_pos.x - CLEAR_ICON_SIZE.width / 2, cursor_pos.y - CLEAR_ICON_SIZE.height / 2),
                    CLEAR_ICON_SIZE);

                render_sys.draw_textured_colored_rect(
                    rect,
                    &RectTexCoords::default(),
                    self.clear_button_image,
                    Color::white());
            } else {
                let rect = Rect2D::new(cursor_pos, selected_tile.draw_size);

                let offset =
                    if selected_tile.is_building() {
                        Point2D::new(-(selected_tile.draw_size.width / 2), -selected_tile.draw_size.height)
                    } else {
                        Point2D::new(-(selected_tile.draw_size.width / 2), -(selected_tile.draw_size.height / 2))
                    };

                let cursor_transform = 
                    WorldToScreenTransform::new(transform.scaling, offset, 0);

                let highlight_color =
                    if has_valid_placement {
                        Color::white()
                    } else {
                        TILE_INVALID_COLOR
                    };

                render_sys.draw_textured_colored_rect(
                    cursor_transform.scale_and_offset_rect(rect),
                    &selected_tile.tex_info.coords,
                    selected_tile.tex_info.texture,
                    Color::new(selected_tile.color.r, selected_tile.color.g, selected_tile.color.b, 0.7) * highlight_color);

                if draw_tile_bounds {
                    render_sys.draw_wireframe_rect_fast(cursor_transform.scale_and_offset_rect(rect), Color::red());
                }
            }
        }
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
