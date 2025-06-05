use crate::{
    app::input::{InputAction, MouseButton},
    imgui_ui::{UiInputEvent, UiSystem},
    render::{RenderSystem, TextureCache, TextureHandle},
    utils::{self, Cell, Color, Rect, RectTexCoords, Size, Vec2, WorldToScreenTransform}
};

use super::{
    camera::Camera,
    sets::{TileDefHandle, TileSets, TileDef, TileKind, BASE_TILE_SIZE},
    map::{self, Tile, TileFlags, TileMap, TileMapLayerKind, TILE_MAP_LAYER_COUNT},
    rendering::{TileMapRenderFlags, TileMapRenderer, INVALID_TILE_COLOR, MAX_GRID_LINE_THICKNESS, MIN_GRID_LINE_THICKNESS}
};

// ----------------------------------------------
// DebugSettingsMenu
// ----------------------------------------------

#[derive(Default)]
pub struct DebugSettingsMenu {
    start_open: bool,
    draw_terrain: bool,
    draw_buildings: bool,
    draw_units: bool,
    draw_grid: bool,
    draw_grid_ignore_depth: bool,
    show_terrain_debug: bool,
    show_buildings_debug: bool,
    show_blockers: bool,
    show_units_debug: bool,
    show_tile_bounds: bool,
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
            draw_units: true,
            draw_grid: true,
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
        if self.draw_units             { flags.insert(TileMapRenderFlags::DrawUnits); }
        if self.draw_grid              { flags.insert(TileMapRenderFlags::DrawGrid); }
        if self.draw_grid_ignore_depth { flags.insert(TileMapRenderFlags::DrawGridIgnoreDepth); }
        if self.show_terrain_debug     { flags.insert(TileMapRenderFlags::DrawTerrainTileDebugInfo); }
        if self.show_buildings_debug   { flags.insert(TileMapRenderFlags::DrawBuildingsTileDebugInfo); }
        if self.show_blockers          { flags.insert(TileMapRenderFlags::DrawBlockerTilesDebug); }
        if self.show_units_debug       { flags.insert(TileMapRenderFlags::DrawUnitsTileDebugInfo); }
        if self.show_tile_bounds       { flags.insert(TileMapRenderFlags::DrawTileDebugBounds); }
        flags
    }

    pub fn draw<'a>(&mut self,
                    camera: &mut Camera,
                    tile_map_renderer: &mut TileMapRenderer,
                    tile_map: &mut TileMap<'a>,
                    tile_sets: &'a TileSets,
                    ui_sys: &UiSystem) {

        let ui = ui_sys.builder();

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        ui.window("Debug Settings")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position([5.0, 5.0], imgui::Condition::FirstUseEver)
            .build(|| {
                if ui.button("Re-center camera") {
                    camera.center();
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

                let tile_spacing_limits = camera.tile_spacing_limits();
                let mut tile_spacing = camera.current_tile_spacing();
                if ui.slider_config("Tile spacing", tile_spacing_limits.0, tile_spacing_limits.1)
                    .display_format("%.1f")
                    .build(&mut tile_spacing) {
                    camera.set_tile_spacing(tile_spacing);
                }

                let mut line_thickness = tile_map_renderer.grid_line_thickness();
                if ui.slider_config("Grid thickness", MIN_GRID_LINE_THICKNESS, MAX_GRID_LINE_THICKNESS)
                    .display_format("%.1f")
                    .build(&mut line_thickness) {
                    tile_map_renderer.set_grid_line_thickness(line_thickness);
                }

                if ui.collapsing_header("Debug draw options", imgui::TreeNodeFlags::empty()) {
                    ui.checkbox("Draw terrain", &mut self.draw_terrain);
                    ui.checkbox("Draw buildings", &mut self.draw_buildings);
                    ui.checkbox("Draw units", &mut self.draw_units);
                    ui.checkbox("Draw grid", &mut self.draw_grid);
                    ui.checkbox("Draw grid (ignore depth)", &mut self.draw_grid_ignore_depth);
                    ui.checkbox("Show terrain debug", &mut self.show_terrain_debug);
                    ui.checkbox("Show buildings debug", &mut self.show_buildings_debug);
                    ui.checkbox("Show blocker tiles", &mut self.show_blockers);
                    ui.checkbox("Show units debug", &mut self.show_units_debug);
                    ui.checkbox("Show tile bounds", &mut self.show_tile_bounds);
                    ui.checkbox("Show selection bounds", &mut self.show_selection_bounds);
                    ui.checkbox("Show cursor pos", &mut self.show_cursor_pos);
                    ui.checkbox("Show screen origin", &mut self.show_screen_origin);
                    ui.checkbox("Show render stats", &mut self.show_render_stats);
                }

                if ui.collapsing_header("Clear map options", imgui::TreeNodeFlags::empty()) {
                    if ui.button("Clear empty") {
                        tile_map.clear(TileDef::empty());
                    }

                    if ui.button("Clear dirt") {
                        let dirt_tile = tile_sets.find_tile_by_name(
                            TileMapLayerKind::Terrain,
                            "ground",
                            "dirt").unwrap_or(TileDef::empty());
                        tile_map.clear(dirt_tile);
                    }

                    if ui.button("Clear grass") {
                        let grass_tile = tile_sets.find_tile_by_name(
                            TileMapLayerKind::Terrain,
                            "ground",
                            "grass").unwrap_or(TileDef::empty());
                        tile_map.clear(grass_tile);
                    }
                }
            });
    }
}

// ----------------------------------------------
// TileListMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TileListMenu {
    start_open: bool,
    selected_tile: Option<TileDefHandle>,
    selected_index: [Option<usize>; TILE_MAP_LAYER_COUNT], // For highlighting the selected button.
    left_mouse_button_held: bool,
    clear_button_image: TextureHandle,
}

impl TileListMenu {
    pub fn new(tex_cache: &mut TextureCache, start_open: bool) -> Self {
        Self {
            start_open: start_open,
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

    pub fn current_selection<'a>(&self, tile_sets: &'a TileSets) -> Option<&'a TileDef> {
        if let Some(selected_tile) = self.selected_tile {
            tile_sets.handle_to_tile(selected_tile)
        } else {
            None
        }
    }

    pub fn can_place_tile(&self) -> bool {
        self.left_mouse_button_held && self.has_selection()
    }

    pub fn on_mouse_click(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_held = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_held = false;
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn draw(&mut self,
                render_sys: &mut impl RenderSystem,
                ui_sys: &UiSystem,
                tile_sets: &TileSets,
                cursor_screen_pos: Vec2,
                transform: &WorldToScreenTransform,
                has_valid_placement: bool,
                show_selection_bounds: bool) {

        let ui = ui_sys.builder();

        let tile_size = [ BASE_TILE_SIZE.width as f32, BASE_TILE_SIZE.height as f32 ];
        let tiles_per_row = 2;
        let padding_between_tiles = 4.0;

        let window_width = (tile_size[0] + padding_between_tiles) * tiles_per_row as f32;
        let window_margin = 35.0; // pixels from the right edge

        // X position = screen width - estimated window width - margin
        // Y position = 10px
        let window_position = [
            ui.io().display_size[0] - window_width - window_margin,
            5.0
        ];

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        ui.window("Tile Selection")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position(window_position, imgui::Condition::FirstUseEver)
            .build(|| {
                self.draw_tile_list(TileMapLayerKind::Terrain,
                                    ui_sys,
                                    render_sys.texture_cache(),
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);

                ui.separator();

                self.draw_tile_list(TileMapLayerKind::Buildings,
                                    ui_sys,
                                    render_sys.texture_cache(),
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);

                ui.separator();

                self.draw_tile_list(TileMapLayerKind::Units,
                                    ui_sys,
                                    render_sys.texture_cache(),
                                    tile_sets,
                                    tile_size,
                                    tiles_per_row,
                                    padding_between_tiles);

                ui.new_line();
                ui.separator();

                ui.text("Tools");
                {
                    let ui_texture = ui_sys.to_ui_texture(render_sys.texture_cache(), self.clear_button_image);

                    let selected_tile = self.current_selection(tile_sets);
                    let is_selected = selected_tile.is_some_and(|t| t.is_empty());

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
                        self.selected_tile = Some(TileDefHandle::empty());
                    }
                }
            });

        self.draw_selected_tile(render_sys,
                                tile_sets,
                                cursor_screen_pos,
                                transform,
                                has_valid_placement,
                                show_selection_bounds);
    }

    fn draw_selected_tile(&self,
                          render_sys: &mut impl RenderSystem,
                          tile_sets: &TileSets,
                          cursor_screen_pos: Vec2,
                          transform: &WorldToScreenTransform,
                          has_valid_placement: bool,
                          show_selection_bounds: bool) {

        if let Some(selected_tile) = self.current_selection(tile_sets) {
            let is_clear_selected = selected_tile.is_empty();
            if is_clear_selected {
                const CLEAR_ICON_SIZE: Size = Size::new(64, 32);

                let rect = Rect::from_pos_and_size(
                    Vec2::new(
                        cursor_screen_pos.x - (CLEAR_ICON_SIZE.width  / 2) as f32,
                        cursor_screen_pos.y - (CLEAR_ICON_SIZE.height / 2) as f32
                    ),
                    CLEAR_ICON_SIZE
                );

                render_sys.draw_textured_colored_rect(
                    rect,
                    RectTexCoords::default(),
                    self.clear_button_image,
                    Color::white());
            } else {
                let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size);

                let offset =
                    if selected_tile.is_building() {
                        Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32))
                    } else {
                        Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32 / 2.0))
                    };

                let cursor_transform = 
                    WorldToScreenTransform::new(transform.scaling, offset, 0.0);

                let highlight_color =
                    if has_valid_placement {
                        Color::white()
                    } else {
                        INVALID_TILE_COLOR
                    };

                if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                    render_sys.draw_textured_colored_rect(
                        cursor_transform.scale_and_offset_rect(rect),
                        &sprite_frame.tex_info.coords,
                        sprite_frame.tex_info.texture,
                        Color::new(selected_tile.color.r, selected_tile.color.g, selected_tile.color.b, 0.7) * highlight_color);
                }

                if show_selection_bounds {
                    render_sys.draw_wireframe_rect_fast(cursor_transform.scale_and_offset_rect(rect), Color::red());
                }
            }
        }
    }

    fn draw_tile_list(&mut self,
                      layer: TileMapLayerKind,
                      ui_sys: &UiSystem,
                      tex_cache: &TextureCache,
                      tile_sets: &TileSets,
                      tile_size: [f32; 2],
                      tiles_per_row: usize,
                      padding_between_tiles: f32) {

        let ui = ui_sys.builder();
        ui.text(layer.to_string());

        let mut tile_index = 0;

        tile_sets.for_each_tile(|tile_set, tile_category, tile_def| {
            if tile_def.kind == map::layer_to_tile_kind(layer) {
                let tile_texture = tile_def.texture_by_index(0, 0, 0);
                let ui_texture = ui_sys.to_ui_texture(tex_cache, tile_texture);

                let is_selected = self.selected_index[layer as usize] == Some(tile_index);
                let bg_color = if is_selected {
                    Color::white().to_array()
                } else {
                    Color::gray().to_array()
                };

                let button_text = format!("{}/{}", tile_category.name, tile_def.name);

                let clicked = ui.image_button_config(&button_text, ui_texture, tile_size)
                    .background_col(bg_color)
                    .tint_col(tile_def.color.to_array())
                    .build();

                // Show tooltip when hovered:
                if ui.is_item_hovered() {
                    ui.tooltip_text(&button_text);
                }

                if clicked {
                    self.clear_selection();
                    self.selected_tile = Some(TileDefHandle::new(tile_set, tile_category, tile_def));
                    self.selected_index[layer as usize] = Some(tile_index);
                }

                // Move to next column unless it's the last in row.
                if (tile_index + 1) % tiles_per_row != 0 {
                    ui.same_line_with_spacing(0.0, padding_between_tiles);
                }

                tile_index += 1;
            }

            true
        });
    }
}

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TileInspectorMenu {
    is_open: bool,
    selected: Option<(Cell, TileKind)>,

    hide_tile: bool,
    show_tile_debug: bool,
    show_tile_bounds: bool,
    show_building_blockers: bool,
}

impl TileInspectorMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn on_mouse_click(&mut self,
                          button: MouseButton,
                          action: InputAction,
                          selected_tile: &Tile) -> UiInputEvent {

        if button == MouseButton::Left && action == InputAction::Press {
            self.is_open                = true;
            self.selected               = Some((selected_tile.cell, selected_tile.kind()));
            self.hide_tile              = selected_tile.flags.contains(TileFlags::Hidden);
            self.show_tile_debug        = selected_tile.flags.contains(TileFlags::DrawDebugInfo);
            self.show_tile_bounds       = selected_tile.flags.contains(TileFlags::DrawDebugBounds);
            self.show_building_blockers = selected_tile.flags.contains(TileFlags::DrawBlockerInfo);
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn draw(&mut self,
                tile_map: &mut TileMap,
                tile_sets: &TileSets,
                ui_sys: &UiSystem,
                transform: &WorldToScreenTransform) {

        if !self.is_open || self.selected.is_none() {
            return;
        }

        let (cell, tile_kind) = self.selected.unwrap();
        if !cell.is_valid() || tile_kind == TileKind::Empty {
            return;
        }

        let layer_kind = map::tile_kind_to_layer(tile_kind);
        let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();

        let tile_iso_pos = utils::cell_to_iso(cell, BASE_TILE_SIZE);
        let tile_iso_adjusted = tile.calc_adjusted_iso_coords();

        let tile_screen_pos = utils::iso_to_screen_rect(
            tile_iso_adjusted,
            tile.draw_size(),
            transform,
            if !tile.is_unit() { true } else { false });

        let window_position = [
            tile_screen_pos.center().x - 30.0,
            tile_screen_pos.center().y - 30.0
        ];

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        let ui = ui_sys.builder();
        ui.window(format!("{} ({},{})", tile.name(), tile.cell.x, tile.cell.y))
            .opened(&mut self.is_open)
            .flags(window_flags)
            .position(window_position, imgui::Condition::Appearing)
            .build(|| {
                {
                    let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

                    let category_name = 
                        if let Some(category) = tile_sets.find_category_for_tile(&tile.def) {
                            &category.name
                        } else {
                            "<none>"
                        };

                    if ui.collapsing_header("Properties", imgui::TreeNodeFlags::empty()) {
                        ui.text(format!("Name..........: '{}'", tile.name()));
                        ui.text(format!("Category......: '{}'", category_name));
                        ui.text(format!("Kind..........: {:?}", tile.kind()));
                        ui.text(format!("Cell..........: {},{}", tile.cell.x, tile.cell.y));
                        ui.text(format!("Iso pos.......: {},{}", tile_iso_pos.x, tile_iso_pos.y));
                        ui.text(format!("Iso adjusted..: {},{}", tile_iso_adjusted.x, tile_iso_adjusted.y));
                        ui.text(format!("Screen pos....: {:.1},{:.1}", tile_screen_pos.x(), tile_screen_pos.y()));
                        ui.text(format!("Draw size.....: {},{}", tile.draw_size().width, tile.draw_size().height));
                        ui.text(format!("Logical size..: {},{}", tile.logical_size().width, tile.logical_size().height));
                        ui.text(format!("Cells size....: {},{}", tile.size_in_cells().width, tile.size_in_cells().height));
                        ui.text(format!("Z-sort........: {}", tile.calc_z_sort()));

                        let color = tile.tint_color();
                        ui.text(format!("Color RGBA....: [{},{},{},{}]", color.r, color.g, color.b, color.a));
                    }

                    if tile.has_variations() {
                        if ui.collapsing_header("Variations", imgui::TreeNodeFlags::empty()) {
                            let mut variation_index = tile.variation_index();
                            if ui.input_scalar("Var", &mut variation_index).step(1).build() {
                                tile.set_variation_index(variation_index);
                            }

                            ui.text(format!("Variations....: {}", tile.variation_count()));
                            ui.text(format!("Variation idx.: {}, '{}'", tile.variation_index(), tile.variation_name()));
                        }
                    }

                    if tile.is_animated() {
                        if ui.collapsing_header("Animation", imgui::TreeNodeFlags::empty()) {
                            ui.text(format!("Anim sets.....: {}", tile.anim_sets_count()));
                            ui.text(format!("Anim set idx..: {}, '{}'", tile.anim_set_index(), tile.anim_set_name()));
                            ui.text(format!("Anim frames...: {}", tile.anim_frames_count()));
                            ui.text(format!("Frame idx.....: {}", tile.anim_frame_index()));
                            ui.text(format!("Frame time....: {:.2}", tile.anim_frame_play_time_secs()));
                        }
                    }
                }

                if ui.collapsing_header("Debug Options", imgui::TreeNodeFlags::empty()) {
                    let (tile_cell, is_building) = {
                        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

                        if ui.checkbox("Hide tile", &mut self.hide_tile) {
                            tile.flags.set(TileFlags::Hidden, self.hide_tile);
                        }

                        if ui.checkbox("Show debug overlay", &mut self.show_tile_debug) {
                            tile.flags.set(TileFlags::DrawDebugInfo, self.show_tile_debug);
                        }

                        if ui.checkbox("Show tile bounds", &mut self.show_tile_bounds) {
                            tile.flags.set(TileFlags::DrawDebugBounds, self.show_tile_bounds);
                        }

                        (tile.cell, tile.is_building())
                    };

                    if is_building {
                        if ui.checkbox("Show blocker tiles", &mut self.show_building_blockers) {
                            tile_map.for_each_building_footprint_tile_mut(tile_cell, |tile| {
                                tile.flags.set(TileFlags::DrawBlockerInfo, self.show_building_blockers);
                            });
                        }
                    }
                }

                // Edit the underlying TileDef, which will apply to *all* tiles sharing this TileDef.
                {
                    let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();

                    // Terrain tile size is always fixed - disallow editing.
                    if !tile.is_empty() && !tile.is_blocker() && !tile.is_terrain() {
                        if ui.collapsing_header("Edit TileDef", imgui::TreeNodeFlags::empty()) {
                            let mut draw_size = tile.draw_size().to_array();
                            if ui.input_int2("Draw size", &mut draw_size).build() {
                                if let Some(editable_def) = tile_sets.try_get_editable_tile_debug(&tile.def) {
                                    let new_size = Size::from_array(draw_size);
                                    if new_size.is_valid() {
                                        editable_def.draw_size = new_size;
                                    }
                                }
                            }

                            let mut logical_size = tile.logical_size().to_array();
                            if ui.input_int2("Logical size", &mut logical_size).build() {
                                if let Some(editable_def) = tile_sets.try_get_editable_tile_debug(&tile.def) {
                                    let new_size = Size::from_array(logical_size);
                                    if new_size.is_valid() // Must be a multiple of BASE_TILE_SIZE.
                                        && (new_size.width  % BASE_TILE_SIZE.width)  == 0
                                        && (new_size.height % BASE_TILE_SIZE.height) == 0 {
                                        editable_def.logical_size = new_size;
                                    }
                                }
                            }
                        }
                    }
                }
            });
    }
}
