use proc_macros::DrawDebugUi;

use crate::{
    app::input::{InputAction, MouseButton},
    imgui_ui::{self, UiInputEvent, UiSystem},
    pathfind::NodeKind as PathNodeKind,
    game::sim::{
        self,
        Simulation
    },
    utils::{
        Size,
        Color,
        Seconds,
        coords::Cell
    },
    tile::{
        map::{Tile, TileFlags, TileMap, TileMapLayerKind, TileEditor},
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
        *self = Self::default();
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
                context: &mut sim::debug::DebugContext,
                sim: &mut Simulation) {

        if !self.is_open || self.selected.is_none() {
            self.close();
            return;
        }

        let (mut cell, tile_kind) = self.selected.unwrap();
        if !cell.is_valid() {
            self.close();
            return;
        }

        let layer_kind = TileMapLayerKind::from_tile_kind(tile_kind);
        let tile = match context.tile_map.try_tile_from_layer(cell, layer_kind) {
            Some(tile) => tile,
            None => {
                self.close();
                return;
            }
        };

        let tile_screen_rect = tile.screen_rect(&context.transform);
        let is_building = tile.is(TileKind::Building);
        let is_unit = tile.is(TileKind::Unit);

        let window_position = [
            tile_screen_rect.center().x - 30.0,
            tile_screen_rect.center().y - 30.0
        ];

        let window_flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE |
            imgui::WindowFlags::NO_SCROLLBAR;

        let ui = context.ui_sys.builder();
        let mut is_open = self.is_open;

        ui.window(format!("{} {}", tile.name(), cell))
            .opened(&mut is_open)
            .flags(window_flags)
            .position(window_position, imgui::Condition::Appearing)
            .build(|| {
                if ui.collapsing_header("Tile", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    cell = self.tile_properties_dropdown(context, cell, layer_kind);
                    Self::tile_variations_dropdown(context.ui_sys, context.tile_map, cell, layer_kind);
                    Self::tile_animations_dropdown(context.ui_sys, context.tile_map, cell, layer_kind);
                    Self::tile_debug_opts_dropdown(context.ui_sys, context.tile_map, cell, layer_kind);
                    Self::tile_def_editor_dropdown(context.ui_sys, context.tile_map, cell, layer_kind, context.tile_sets);
                    ui.unindent_by(10.0);
                }

                if is_building && ui.collapsing_header("Building", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    sim.draw_building_debug_ui(context, cell);
                    ui.unindent_by(10.0);
                }

                if is_unit && ui.collapsing_header("Unit", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    sim.draw_unit_debug_ui(context, cell);
                    ui.unindent_by(10.0);
                }
            });

        self.is_open = is_open;
    }

    fn tile_properties_dropdown(&mut self,
                                context: &mut sim::debug::DebugContext,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) -> Cell {

        let ui = context.ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Building/Properties.
        if !ui.collapsing_header("Properties##_tile_properties", imgui::TreeNodeFlags::empty()) {
            return cell; // collapsed.
        }

        let tile = context.tile_map.try_tile_from_layer(cell, layer_kind).unwrap();
        let mut tile_editor = TileEditor::new(context.tile_map, layer_kind, cell);

        // Display-only properties:
        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables<'a> {
            name: &'a str,
            category: &'a str,
            kind: TileKind,
            flags: TileFlags,
            path_kind: PathNodeKind,
            has_game_state: bool,
            size_in_cells: Size,
            draw_size: Size,
            logical_size: Size,
            color: Color,
        }

        let debug_vars = DrawDebugUiVariables {
            name: tile.name(),
            category: tile.category_name(context.tile_sets),
            kind: tile.kind(),
            flags: tile.flags(),
            path_kind: tile.tile_def().path_kind,
            has_game_state: tile.game_state_handle().is_valid(),
            size_in_cells: tile.size_in_cells(),
            draw_size: tile.draw_size(),
            logical_size: tile.logical_size(),
            color: tile.tint_color(),
        };

        debug_vars.draw_debug_ui(context.ui_sys);
        ui.separator();

        // Editing the cell only for single cell Tiles for now; no building blockers support.
        let read_only_cell = tile.occupies_multiple_cells();
        let mut updated_cell = cell;

        // Editable properties:
        let mut start_cell = tile.cell_range().start;
        if imgui_ui::input_i32_xy(ui, "Start Cell:", &mut start_cell, read_only_cell, None, None)
            && tile_editor.set_base_cell(start_cell) {
            updated_cell = start_cell;
        }

        let mut end_cell = tile.cell_range().end;
        imgui_ui::input_i32_xy(ui, "End Cell:", &mut end_cell, true, None, None);

        let mut iso_coords = tile.iso_coords();
        imgui_ui::input_i32_xy(ui, "Iso Coords:", &mut iso_coords, true, None, None);

        let mut iso_adjusted = tile.adjusted_iso_coords();
        if imgui_ui::input_i32_xy(ui, "Adjusted Iso Coords:", &mut iso_adjusted, false, None, None) {
            tile_editor.set_adjusted_iso_coords(iso_adjusted);
        }

        let mut screen_coords = tile.screen_rect(&context.transform).position();
        imgui_ui::input_f32_xy(ui, "Screen Coords:", &mut screen_coords, true, None, None);

        let mut z_sort_key = tile.z_sort_key();
        if imgui_ui::input_i32(ui, "Z Sort Key:", &mut z_sort_key, false, None) {
            tile_editor.set_z_sort_key(z_sort_key);
        }

        // If we've moved the tile, update the Inspector's selected cell and game-side state.
        if updated_cell != cell {
            debug_assert!(tile.base_cell() == updated_cell);

            if let Some((_, tile_kind)) = self.selected {
                self.selected = Some((updated_cell, tile_kind));
            }

            if tile.is(TileKind::Building) {
                if let Some(building) = context.world.find_building_for_tile_mut(tile) {
                    building.set_cell_range(tile.cell_range());
                }
            }

            if tile.is(TileKind::Unit) {
                if let Some(unit) = context.world.find_unit_for_tile_mut(tile) {
                    unit.set_cell(updated_cell);
                }
            }
        }

        updated_cell
    }

    fn tile_variations_dropdown(ui_sys: &UiSystem,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();
        if !tile.has_variations() {
            return;
        }

        let ui = ui_sys.builder();
        if !ui.collapsing_header("Variations", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut variation_index = tile.variation_index();
        if ui.input_scalar("Var idx", &mut variation_index).step(1).build() {
            tile.set_variation_index(variation_index);
        }

        ui.text(format!("Variations    : {}", tile.variation_count()));
        ui.text(format!("Variation idx : {}, {}", tile.variation_index(), tile.variation_name()));    
    }

    fn tile_animations_dropdown(ui_sys: &UiSystem,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();
        if !tile.has_animations() {
            return;
        }

        let ui = ui_sys.builder();
        if !ui.collapsing_header("Animations", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let anim_set_count = tile.anim_sets_count();
        let mut anim_set_names = Vec::with_capacity(anim_set_count);
        for i in 0..anim_set_count {
            let anim_set_name = tile.tile_def().anim_set_name(tile.variation_index(), i);
            anim_set_names.push(anim_set_name);
        }

        let mut anim_set_index: usize = tile.anim_set_index();
        if ui.combo_simple_string("Anim Set", &mut anim_set_index, &anim_set_names) {
            tile.set_anim_set_index(anim_set_index);
        }

        anim_set_index = tile.anim_set_index();
        if ui.input_scalar("Anim Set idx", &mut anim_set_index).step(1).build() {
            tile.set_anim_set_index(anim_set_index);
        }

        let anim_set = tile.anim_set();

        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables {
            anim_set_count: usize,
            anim_frames_count: usize,
            anim_duration_secs: Seconds,
            #[debug_ui(separator)] looping: bool,

            frame_index: usize,
            frame_duration_secs: Seconds,
            frame_play_time_secs: Seconds,
        }

        let debug_vars = DrawDebugUiVariables {
            anim_set_count,
            anim_frames_count: tile.anim_frames_count(),
            anim_duration_secs: anim_set.anim_duration_secs(),
            looping: anim_set.looping,
            frame_index: tile.anim_frame_index(),
            frame_duration_secs: anim_set.frame_duration_secs(),
            frame_play_time_secs: tile.anim_frame_play_time_secs(),
        };

        debug_vars.draw_debug_ui(ui_sys);
    }

    fn tile_debug_opts_dropdown(ui_sys: &UiSystem,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind) {

        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Building/Debug Options.
        if !ui.collapsing_header("Debug Options##_tile_debug_opts", imgui::TreeNodeFlags::empty()) {
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
    fn tile_def_editor_dropdown(ui_sys: &UiSystem,
                                tile_map: &mut TileMap,
                                cell: Cell,
                                layer_kind: TileMapLayerKind,
                                tile_sets: &TileSets) {

        let tile = tile_map.try_tile_from_layer_mut(cell, layer_kind).unwrap();

        if tile.is(TileKind::Blocker) {
            return;
        }

        let ui = ui_sys.builder();
        if !ui.collapsing_header("Edit TileDef", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let mut color = tile.tint_color();
        if imgui_ui::input_color(ui, "Color:", &mut color) {
            if let Some(editable_def) = tile.try_get_editable_tile_def(tile_sets) {
                // Prevent invalid values.
                editable_def.color = color.clamp();
            }
        }

        ui.separator();

        let mut draw_size = tile.draw_size();
        if imgui_ui::input_i32_xy(
            ui,
            "Draw Size:",
            &mut draw_size,
            false,
            None,
            Some([ "W", "H" ])) {

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

        let mut logical_size = tile.logical_size();
        if imgui_ui::input_i32_xy(
            ui,
            "Logical Size:",
            &mut logical_size,
            false,
            Some([ BASE_TILE_SIZE.width, BASE_TILE_SIZE.height ]),
            Some([ "W", "H" ])) {

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
}
