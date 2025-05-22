use std::collections::HashMap;

use crate::utils::{self, Cell2D, Color, IsoPoint2D, Point2D, Rect2D, RectTexCoords, Size2D, WorldToScreenTransform};
use crate::{render::RenderSystem, render::TextureCache, render::TextureHandle, ui::UiSystem};
use crate::app::input::{MouseButton, InputAction};

use super::sets::{self, TileSets};
use super::def::{TileKind, TileDef, TileTexInfo, BASE_TILE_SIZE};
use super::map::{self, TileMap, TileMapRenderFlags, TileMapRenderer, Tile, TileMapLayerKind, TILE_MAP_LAYER_COUNT};

// ----------------------------------------------
// Debug Draw Helpers
// ----------------------------------------------

pub fn draw_tile_debug(render_sys: &mut RenderSystem,
                       ui_sys: &UiSystem,
                       tile_iso_coords: IsoPoint2D,
                       tile_rect: Rect2D,
                       transform: &WorldToScreenTransform,
                       tile: &Tile,
                       flags: TileMapRenderFlags) {

    let draw_debug_info =
        if tile.is_terrain() && flags.contains(TileMapRenderFlags::DrawTerrainTileDebugInfo) {
            true
        } else if tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildingsTileDebugInfo) {
            true
        } else if tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnitsTileDebugInfo) {
            true
        } else if tile.is_building_blocker() && flags.contains(TileMapRenderFlags::DrawDebugBuildingBlockers) {
            true
        } else {
            false
        };

    if draw_debug_info {
        draw_tile_info(
            render_sys,
            ui_sys,
            tile_rect.position(),
            tile_iso_coords,
            tile);
    }

    if flags.contains(TileMapRenderFlags::DrawTileDebugBounds) {
        draw_tile_bounds(render_sys, tile_rect, transform, tile);
    }
}

fn draw_tile_overlay_text(ui_sys: &UiSystem,
                          debug_overlay_pos: Point2D,
                          tile_screen_pos: Point2D,
                          tile_iso_pos: IsoPoint2D,
                          tile: &Tile) {

    // Make the window background transparent and remove decorations:
    let window_flags =
        imgui::WindowFlags::NO_DECORATION |
        imgui::WindowFlags::NO_MOVE |
        imgui::WindowFlags::NO_SAVED_SETTINGS |
        imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
        imgui::WindowFlags::NO_NAV |
        imgui::WindowFlags::NO_MOUSE_INPUTS;

    // NOTE: Label has to be unique for each tile because it will be used as the ImGui ID for this widget.
    let label = format!("{}_{}_{}", tile.def.name, tile.cell.x, tile.cell.y);
    let position = [ debug_overlay_pos.x as f32, debug_overlay_pos.y as f32 ];

    let bg_color = match tile.def.kind {
        TileKind::BuildingBlocker => Color::red().to_array(),
        TileKind::Building => Color::yellow().to_array(),
        TileKind::Unit => Color::cyan().to_array(),
        _ => Color::black().to_array()
    };

    let text_color = match tile.def.kind {
        TileKind::Terrain => Color::white().to_array(),
        _ => Color::black().to_array()
    };

    let ui = ui_sys.builder();

    // Adjust window background color based on tile kind.
    // The returned tokens take care of popping back to the previous color/font.
    let _0 = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
    let _1 = ui.push_style_color(imgui::StyleColor::Text, text_color);
    let _2 = ui.push_font(ui_sys.fonts().small);

    ui.window(label)
        .position(position, imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.4) // Semi-transparent
        .build(|| {
            ui.text(format!("C:{},{}", tile.cell.x,       tile.cell.y));       // Cell position
            ui.text(format!("S:{},{}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
            ui.text(format!("I:{},{}", tile_iso_pos.x,    tile_iso_pos.y));    // 2D isometric position
        });
}

fn draw_tile_info(render_sys: &mut RenderSystem,
                  ui_sys: &UiSystem,
                  tile_screen_pos: Point2D,
                  tile_iso_pos: IsoPoint2D,
                  tile: &Tile) {

    let debug_overlay_offsets = match tile.def.kind {
        TileKind::Terrain  => (tile.def.logical_size.width / 2, tile.def.logical_size.height / 2),
        TileKind::Building => (tile.def.logical_size.width, tile.def.logical_size.height),
        TileKind::Unit     => (tile.def.draw_size.width, tile.def.draw_size.height),
        _ => (0, 0)
    };

    let debug_overlay_pos = Point2D::new(
        tile_screen_pos.x + debug_overlay_offsets.0,
        tile_screen_pos.y + debug_overlay_offsets.1);

    draw_tile_overlay_text(
        ui_sys,
        debug_overlay_pos,
        tile_screen_pos,
        tile_iso_pos,
        tile);

    // Put a red dot at the tile's center.
    let mut center_pos = tile.calc_tile_center(tile_screen_pos);
    center_pos.x -= 5;
    center_pos.y -= 5;
    render_sys.draw_point_fast(center_pos, Color::red(), 10.0);
}

fn draw_tile_bounds(render_sys: &mut RenderSystem,
                    tile_rect: Rect2D,
                    transform: &WorldToScreenTransform,
                    tile: &Tile) {

    let color = match tile.def.kind {
        TileKind::Building => Color::yellow(),
        TileKind::Unit => Color::cyan(),
        _ => Color::red()
    };

    // Tile isometric "diamond" bounding box:
    let diamond_points = map::cell_to_screen_diamond_points(
        tile.cell,
        tile.def.logical_size,
        transform);

    render_sys.draw_line_fast(diamond_points[0], diamond_points[1], color, color);
    render_sys.draw_line_fast(diamond_points[1], diamond_points[2], color, color);
    render_sys.draw_line_fast(diamond_points[2], diamond_points[3], color, color);
    render_sys.draw_line_fast(diamond_points[3], diamond_points[0], color, color);

    for point in diamond_points {
        render_sys.draw_point_fast(point, Color::green(), 10.0);
    }

    // Tile axis-aligned bounding rectangle of the actual sprite image:
    render_sys.draw_wireframe_rect_fast(tile_rect, color);
}

// Show a small debug overlay under the cursor with its current position.
pub fn draw_cursor_overlay(ui_sys: &UiSystem, transform: &WorldToScreenTransform) {
    let ui = ui_sys.builder();
    let cursor_screen_pos = ui.io().mouse_pos;

    // Make the window background transparent and remove decorations.
    let window_flags =
        imgui::WindowFlags::NO_DECORATION |
        imgui::WindowFlags::NO_MOVE |
        imgui::WindowFlags::NO_SAVED_SETTINGS |
        imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
        imgui::WindowFlags::NO_NAV |
        imgui::WindowFlags::NO_MOUSE_INPUTS;

    // Draw a tiny window near the cursor
    ui.window("Cursor Debug")
        .position([cursor_screen_pos[0] + 10.0, cursor_screen_pos[1] + 10.0], imgui::Condition::Always)
        .flags(window_flags)
        .always_auto_resize(true)
        .bg_alpha(0.6) // Semi-transparent
        .build(|| {
            let cursor_iso_pos = utils::screen_to_iso_point(
                Point2D::new(cursor_screen_pos[0] as i32, cursor_screen_pos[1] as i32),
                transform, BASE_TILE_SIZE, false);

            let cursor_approx_cell = utils::iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);

            ui.text(format!("C:{},{}", cursor_approx_cell.x, cursor_approx_cell.y));
            ui.text(format!("S:{},{}", cursor_screen_pos[0], cursor_screen_pos[1]));
            ui.text(format!("I:{},{}", cursor_iso_pos.x, cursor_iso_pos.y));
        });
}

pub fn draw_screen_origin_marker(render_sys: &mut RenderSystem) {
    // 50x50px white square to mark the origin.
    render_sys.draw_point_fast(
        Point2D::new(0, 0), 
        Color::white(),
        50.0);

    // Red line for the X axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Point2D::new(0, 0),
        Point2D::new(100, 0),
        Color::red(),
        15.0);

    render_sys.draw_colored_rect(
        Rect2D::new(Point2D::new(100, 0), Size2D::new(10, 10)),
        Color::green());

    // Blue line for the Y axis, green square at the end.
    render_sys.draw_line_with_thickness(
        Point2D::new(0, 0),
        Point2D::new(0, 100),
        Color::blue(),
        15.0);

    render_sys.draw_colored_rect(
        Rect2D::new(Point2D::new(0, 100), Size2D::new(10, 10)),
        Color::green());
}

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
                        map::TILE_INVALID_COLOR
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

// ----------------------------------------------
// Test map setup helpers
// ----------------------------------------------

pub fn create_test_tile_sets(tex_cache: &mut TextureCache) -> TileSets {
    println!("Loading test tile sets...");

    // Sprite Textures:
    let tex_dirt  = tex_cache.load_texture(&(sets::PATH_TO_TERRAIN_TILE_SETS.to_string() + "/ground/dirt.png"));
    let tex_grass = tex_cache.load_texture(&(sets::PATH_TO_TERRAIN_TILE_SETS.to_string() + "/ground/grass.png"));
    let tex_house = tex_cache.load_texture(&(sets::PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/house/0.png"));
    let tex_tower = tex_cache.load_texture(&(sets::PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/tower/0.png"));
    let tex_tree0 = tex_cache.load_texture(&(sets::PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/tree/0.png"));
    let tex_tree1 = tex_cache.load_texture(&(sets::PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/tree/1.png"));
    let tex_ped   = tex_cache.load_texture(&(sets::PATH_TO_UNITS_TILE_SETS.to_string() + "/ped/0.png"));

    // Metadata:
    let tile_defs: [TileDef; 7] = [
        TileDef { kind: TileKind::Terrain,  logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 32  }, tex_info: TileTexInfo::new(tex_dirt),  color: Color::white(), name: "dirt".to_string()   },
        TileDef { kind: TileKind::Terrain,  logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 32  }, tex_info: TileTexInfo::new(tex_grass), color: Color::white(), name: "grass".to_string()  },
        TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 128, height: 64 }, draw_size: Size2D{ width: 128, height: 68  }, tex_info: TileTexInfo::new(tex_house), color: Color::white(), name: "house".to_string()  },
        TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 192, height: 96 }, draw_size: Size2D{ width: 192, height: 144 }, tex_info: TileTexInfo::new(tex_tower), color: Color::white(), name: "tower".to_string()  },
        TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 64  }, tex_info: TileTexInfo::new(tex_tree0), color: Color::white(), name: "tree 0".to_string() },
        TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 64  }, tex_info: TileTexInfo::new(tex_tree1), color: Color::white(), name: "tree 1".to_string() },
        TileDef { kind: TileKind::Unit,     logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 16,  height: 24  }, tex_info: TileTexInfo::new(tex_ped),   color: Color::white(), name: "ped".to_string()    },
    ];

    let mut tile_sets = TileSets::new();

    for tile_def in tile_defs {
        tile_sets.add_def(tile_def);
    }

    tile_sets
}

pub fn create_test_tile_map(tile_sets: &TileSets) -> TileMap {
    println!("Creating test tile map...");

    const MAP_WIDTH:  i32 = 8;
    const MAP_HEIGHT: i32 = 8;

    const G:  i32 = 0; // ground:grass (empty)
    const R:  i32 = 1; // ground:road/dirt (empty)
    const U:  i32 = 2; // unit:ped
    const HH: i32 = 3; // building:house (2x2)
    const TT: i32 = 4; // building:tower (3x3)
    const B0: i32 = 5; // special blocker for the 3x3 building.
    const B1: i32 = 6; // special blocker for the 2x2 building.
    const B2: i32 = 7; // special blocker for the 2x2 building.
    const B3: i32 = 8; // special blocker for the 2x2 building.
    const B4: i32 = 9; // special blocker for the 2x2 building.

    const TILE_NAMES: [&str; 5] = [ "grass", "dirt", "ped", "house", "tower" ];

    const TERRAIN_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,G,G,G,G,G,G,R,
        R,R,R,R,R,R,R,R,
    ];

    const BUILDINGS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        G, G, G, G, G, G, G, G, // <-- start, tile zero is the leftmost (top-left)
        G, TT,B0,B0,G, HH,B1,G,
        G, B0,B0,B0,G, B1,B1,G,
        G, B0,B0,B0,G, HH,B2,G,
        G, G, G, G, G, B2,B2,G,
        G, HH,B4,G, G, HH,B3,G,
        G, B4,B4,G, G, B3,B3,G,
        G, G, G, G, G, G, G, G,
    ];

    const UNITS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
        U,U,U,U,U,U,U,U, // <-- start, tile zero is the leftmost (top-left)
        U,G,G,G,U,G,G,U,
        U,G,G,G,U,G,G,U,
        U,G,G,G,U,G,G,U,
        U,U,U,U,U,G,G,U,
        U,G,G,U,U,G,G,U,
        U,G,G,U,U,G,G,U,
        U,U,U,U,U,U,U,U,
    ];

    let blockers_mapping = HashMap::from([
        (B0, Cell2D::new(1, 1)),
        (B1, Cell2D::new(5, 1)),
        (B2, Cell2D::new(5, 3)),
        (B3, Cell2D::new(5, 5)),
        (B4, Cell2D::new(1, 5)),
    ]);

    let mut tile_map = TileMap::new(Size2D::new(MAP_WIDTH, MAP_HEIGHT));

    // Terrain:
    {
        let terrain_layer = tile_map.layer_mut(TileMapLayerKind::Terrain);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = TERRAIN_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let tile_def = tile_sets.find_by_name(TILE_NAMES[tile_id as usize]);
                terrain_layer.add_tile(Cell2D::new(x, y), tile_def);
            }
        }
    }

    // Buildings:
    {
        let buildings_layer = tile_map.layer_mut(TileMapLayerKind::Buildings);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = BUILDINGS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let cell = Cell2D::new(x, y);

                if tile_id == G { // ground/empty
                    buildings_layer.add_empty_tile(cell);
                } else if tile_id >= B0 { // building blocker
                    let owner_cell = blockers_mapping.get(&tile_id).unwrap();
                    buildings_layer.add_building_blocker_tile(cell, *owner_cell);
                } else { // building tile
                    let tile_def = tile_sets.find_by_name(TILE_NAMES[tile_id as usize]);
                    buildings_layer.add_tile(cell, tile_def);
                }
            }
        }
    }

    // Units:
    {
        let units_layer = tile_map.layer_mut(TileMapLayerKind::Units);

        for y in (0..MAP_HEIGHT).rev() {
            for x in (0..MAP_WIDTH).rev() {
                let tile_id = UNITS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                let cell = Cell2D::new(x, y);

                if tile_id == G { // ground/empty
                    units_layer.add_empty_tile(cell);
                } else { // unit tile
                    let tile_def = tile_sets.find_by_name(TILE_NAMES[tile_id as usize]);
                    units_layer.add_tile(cell, tile_def);
                }
            }
        }
    }

    tile_map
}
