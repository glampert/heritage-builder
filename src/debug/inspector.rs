use crate::{
    app::input::{InputAction, MouseButton},
    imgui_ui::{UiInputEvent, UiSystem},
    game::sim::world::World,
    utils::coords::{
        self,
        Cell,
        WorldToScreenTransform
    },
    tile::{
        map::{Tile, TileFlags, TileMap, TileMapLayerKind},
        sets::{TileKind, TileSets, BASE_TILE_SIZE}
    }
};

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TileInspectorMenu {
    is_open: bool,
    selected: Option<(Cell, TileKind)>,
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
            self.is_open  = true;
            self.selected = Some((selected_tile.base_cell(), selected_tile.kind()));
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn draw(&mut self,
                world: &mut World,
                tile_map: &mut TileMap,
                tile_sets: &TileSets,
                ui_sys: &UiSystem,
                transform: &WorldToScreenTransform) {

        if !self.is_open || self.selected.is_none() {
            return;
        }

        let (cell, tile_kind) = self.selected.unwrap();
        if !cell.is_valid() {
            return;
        }

        let layer_kind = TileMapLayerKind::from_tile_kind(tile_kind);
        let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();
        let tile_screen_rect = tile.calc_screen_rect(transform);
        let is_building = tile.is(TileKind::Building);

        let window_position = [
            tile_screen_rect.center().x - 30.0,
            tile_screen_rect.center().y - 30.0
        ];

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        let ui = ui_sys.builder();

        ui.window(format!("{} ({},{})", tile.name(), cell.x, cell.y))
            .opened(&mut self.is_open)
            .flags(window_flags)
            .position(window_position, imgui::Condition::Appearing)
            .build(|| {
                if ui.collapsing_header("Tile", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    Self::tile_properties_dropdown(ui, tile_map, cell, layer_kind, tile_sets, transform);
                    Self::tile_variations_dropdown(ui, tile_map, cell, layer_kind);
                    Self::tile_animations_dropdown(ui, tile_map, cell, layer_kind);
                    Self::tile_debug_opts_dropdown(ui, tile_map, cell, layer_kind);
                    Self::tile_def_editor_dropdown(ui, tile_map, cell, layer_kind, tile_sets);
                    ui.unindent_by(10.0);
                }

                if is_building && ui.collapsing_header("Building", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    Self::building_debug_ui(world, tile_map, ui_sys, cell, layer_kind);
                    ui.unindent_by(10.0);
                }
            });
    }

    fn tile_properties_dropdown(ui: &imgui::Ui,
                                tile_map: &TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind,
                                tile_sets: &TileSets,
                                transform: &WorldToScreenTransform) {

        // NOTE: Use the special ##id here so we don't collide with Building/Properties.
        if !ui.collapsing_header("Properties##_tile_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();
        let cell_range = tile.cell_range();

        let category_name = tile.category_name(tile_sets);
        let color = tile.tint_color();

        let tile_iso_pos = coords::cell_to_iso(cell, BASE_TILE_SIZE);
        let tile_iso_adjusted = tile.calc_adjusted_iso_coords();

        let tile_screen_rect = tile.calc_screen_rect(transform);
        let tile_screen_pos = tile_screen_rect.position();

        ui.text(format!("Name..........: '{}'", tile.name()));
        ui.text(format!("Category......: '{}'", category_name));
        ui.text(format!("Kind..........: {}", tile.kind()));
        ui.text(format!("Flags.........: {}", tile.flags()));
        ui.text(format!("Has Game State: {}", tile.game_state_handle().is_valid()));
        ui.text(format!("Cells.........: [{},{}; {},{}]", cell_range.start.x, cell_range.start.y, cell_range.end.x, cell_range.end.y));
        ui.text(format!("Iso pos.......: {},{}", tile_iso_pos.x, tile_iso_pos.y));
        ui.text(format!("Iso adjusted..: {},{}", tile_iso_adjusted.x, tile_iso_adjusted.y));
        ui.text(format!("Screen pos....: {:.1},{:.1}", tile_screen_pos.x, tile_screen_pos.y));
        ui.text(format!("Draw size.....: {},{}", tile.draw_size().width, tile.draw_size().height));
        ui.text(format!("Logical size..: {},{}", tile.logical_size().width, tile.logical_size().height));
        ui.text(format!("Cells size....: {},{}", tile.size_in_cells().width, tile.size_in_cells().height));
        ui.text(format!("Z-sort........: {}", tile.calc_z_sort()));
        ui.text(format!("Color RGBA....: [{},{},{},{}]", color.r, color.g, color.b, color.a));
    }

    fn tile_variations_dropdown(ui: &imgui::Ui,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

        if !tile.has_variations() {
            return;
        }

        if !ui.collapsing_header("Variations", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut variation_index = tile.variation_index();
        if ui.input_scalar("Var idx", &mut variation_index).step(1).build() {
            tile.set_variation_index(variation_index);
        }

        ui.text(format!("Variations....: {}", tile.variation_count()));
        ui.text(format!("Variation idx.: {}, '{}'", tile.variation_index(), tile.variation_name()));    
    }

    fn tile_animations_dropdown(ui: &imgui::Ui,
                                tile_map: &TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();

        if !tile.has_animations() {
            return;
        }

        if !ui.collapsing_header("Animations", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        ui.text(format!("Anim sets.....: {}", tile.anim_sets_count()));
        ui.text(format!("Anim set idx..: {}, '{}'", tile.anim_set_index(), tile.anim_set_name()));
        ui.text(format!("Anim frames...: {}", tile.anim_frames_count()));
        ui.text(format!("Frame idx.....: {}", tile.anim_frame_index()));
        ui.text(format!("Frame time....: {:.2}", tile.anim_frame_play_time_secs()));
    }

    fn tile_debug_opts_dropdown(ui: &imgui::Ui,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        if !ui.collapsing_header("Debug Options", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

        let mut hide_tile = tile.has_flags(TileFlags::Hidden);
        if ui.checkbox("Hide tile", &mut hide_tile) {
            tile.set_flags(TileFlags::Hidden, hide_tile);
        }

        let mut show_tile_debug = tile.has_flags(TileFlags::DrawDebugInfo);
        if ui.checkbox("Show debug overlay", &mut show_tile_debug) {
            tile.set_flags(TileFlags::DrawDebugInfo, show_tile_debug);
        }

        let mut show_tile_bounds = tile.has_flags(TileFlags::DrawDebugBounds);
        if ui.checkbox("Show tile bounds", &mut show_tile_bounds) {
            tile.set_flags(TileFlags::DrawDebugBounds, show_tile_bounds);
        }

        if tile.is(TileKind::Building) {
            let mut show_building_blockers = tile.has_flags(TileFlags::DrawBlockerInfo);
            if ui.checkbox("Show blocker tiles", &mut show_building_blockers) {
                tile.set_flags(TileFlags::DrawBlockerInfo, show_building_blockers);
            }
        }
    }

    // Edit the underlying TileDef, which will apply to *all* tiles sharing this TileDef.
    fn tile_def_editor_dropdown(ui: &imgui::Ui,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind,
                                tile_sets: &TileSets) {

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

        if tile.is(TileKind::Blocker) {
            return;
        }

        if !ui.collapsing_header("Edit TileDef", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut draw_size_changed = false;
        let mut draw_size = tile.draw_size();

        draw_size_changed |= ui.input_int("Draw W", &mut draw_size.width).build();
        draw_size_changed |= ui.input_int("Draw H", &mut draw_size.height).build();

        if draw_size_changed {
            if let Some(editable_def) = tile.try_get_editable_tile_def(tile_sets) {
                if draw_size.is_valid() {
                    editable_def.draw_size = draw_size;
                }
            }
        }

        // Terrain tile logical size is always fixed - disallow editing.
        if tile.is(TileKind::Terrain) {
            return;
        }

        ui.separator();

        let mut logical_size_changed = false;
        let mut logical_size = tile.logical_size();

        logical_size_changed |= ui.input_scalar("Logical W", &mut logical_size.width)
            .step(BASE_TILE_SIZE.width)
            .build();
        logical_size_changed |= ui.input_scalar("Logical H", &mut logical_size.height)
            .step(BASE_TILE_SIZE.height)
            .build();

        if logical_size_changed {
            if let Some(editable_def) = tile.try_get_editable_tile_def(tile_sets) {
                if logical_size.is_valid() // Must be a multiple of BASE_TILE_SIZE.
                    && (logical_size.width  % BASE_TILE_SIZE.width)  == 0
                    && (logical_size.height % BASE_TILE_SIZE.height) == 0 {
                    editable_def.logical_size = logical_size;
                }
            }
        }

        ui.separator();

        let mut occludes_terrain = tile.has_flags(TileFlags::OccludesTerrain);
        if ui.checkbox("Occludes terrain", &mut occludes_terrain) {
            if let Some(editable_def) = tile.try_get_editable_tile_def(tile_sets) {
                editable_def.occludes_terrain = occludes_terrain;
            }
            tile.set_flags(TileFlags::OccludesTerrain, occludes_terrain);
        }
    }

    fn building_debug_ui(world: &mut World,
                         tile_map: &TileMap,
                         ui_sys: &UiSystem,
                         cell: Cell,
                         layer_kind: TileMapLayerKind) {

        let tile = tile_map.try_tile_from_layer(cell, layer_kind).unwrap();
        if let Some(building) = world.find_building_for_tile_mut(tile) {
            building.draw_debug_ui(ui_sys);
        }
    }
}
