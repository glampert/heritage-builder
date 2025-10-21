use proc_macros::DrawDebugUi;

use crate::{
    app::input::{InputAction, MouseButton},
    engine::time::Seconds,
    game::sim::{self, debug::DebugUiMode, Simulation},
    imgui_ui::{self, UiInputEvent},
    pathfind::NodeKind as PathNodeKind,
    tile::{Tile, TileFlags, TileKind, TileMapLayerKind, BASE_TILE_SIZE},
    utils::{coords::Cell, mem, Color, Size},
};

// ----------------------------------------------
// TileWeakRef
// ----------------------------------------------

struct TileWeakRef {
    // SAFETY: Calling code must ensure tile pointer stays valid.
    // We hook up the TileMap editor callbacks to guarantee that.
    // Tile Inspector is used only for debug and development,
    // so this is good enough.
    tile_ptr: mem::RawPtr<Tile>,
    tile_kind: TileKind,
    tile_layer: TileMapLayerKind,
}

impl TileWeakRef {
    fn new(tile: &Tile) -> Self {
        Self { tile_ptr: mem::RawPtr::from_ref(tile),
               tile_kind: tile.kind(),
               tile_layer: tile.layer_kind() }
    }

    fn try_tile(&self) -> Option<&Tile> {
        // Still same layer and kind, chances are our weak pointer is still in good
        // shape.
        if self.tile_ptr.kind() == self.tile_kind && self.tile_ptr.layer_kind() == self.tile_layer {
            return Some(self.tile_ptr.as_ref());
        }
        None
    }

    fn try_tile_mut(&mut self) -> Option<&mut Tile> {
        if self.tile_ptr.kind() == self.tile_kind && self.tile_ptr.layer_kind() == self.tile_layer {
            return Some(self.tile_ptr.as_mut());
        }
        None
    }
}

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TileInspectorMenu {
    is_open: bool,
    selected: Option<TileWeakRef>,
    last_tile_cell: Cell,
}

impl TileInspectorMenu {
    pub fn close(&mut self) {
        self.is_open = false;
        self.selected = None;
    }

    pub fn on_mouse_click(&mut self,
                          button: MouseButton,
                          action: InputAction,
                          selected_tile: &Tile)
                          -> UiInputEvent {
        if button == MouseButton::Left && action == InputAction::Press {
            self.is_open = true;
            self.selected = Some(TileWeakRef::new(selected_tile));
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    pub fn on_tile_placed(&mut self, tile: &Tile, did_reallocate: bool) {
        if did_reallocate {
            // Tidy any local Tile references if the tile map has
            // reallocated its slab as a result of the new tile added.
            self.close();
        }

        // Handle the case where we remove a tile and re-add something to the
        // same cell, like when upgrading a building. Re-open with the new tile.
        if self.last_tile_cell == tile.base_cell() {
            self.is_open = true;
            self.selected = Some(TileWeakRef::new(tile));
        }

        self.last_tile_cell = Cell::invalid();
    }

    pub fn on_removing_tile(&mut self, tile: &Tile) {
        // Tidy cached tile reference if it is being removed.
        if let Some(selected_tile) = self.try_get_selected_tile() {
            if selected_tile.base_cell() == tile.base_cell() {
                self.last_tile_cell = selected_tile.base_cell();
                self.close();
            }
        }
    }

    pub fn draw(&mut self, context: &mut sim::debug::DebugContext, sim: &mut Simulation) {
        let (tile_screen_rect, window_label) = {
            let tile = match self.try_get_selected_tile() {
                Some(tile) => tile,
                None => {
                    self.close();
                    return;
                }
            };
            (tile.screen_rect(context.transform), Self::make_stable_imgui_window_label(tile))
        };

        let window_position =
            [tile_screen_rect.center().x - 30.0, tile_screen_rect.center().y - 30.0];

        let window_flags = imgui::WindowFlags::ALWAYS_AUTO_RESIZE;

        let ui = context.ui_sys.builder();
        let mut is_open = self.is_open;

        ui.window(window_label)
          .opened(&mut is_open)
          .flags(window_flags)
          .position(window_position, imgui::Condition::FirstUseEver)
          .build(|| {
              let tile = match self.try_get_selected_tile_mut() {
                  Some(tile) => tile,
                  None => return,
              };

              // Overview:
              sim.draw_game_object_debug_ui(context, tile, DebugUiMode::Overview);

              // Detailed:
              if tile.game_object_handle().is_valid() {
                  // If the tile has a game object, we'll have different dropdowns besides "Tile"
                  // (e.g.: "Building", "Unit"), so nest tile debug under its own collapsing
                  // header.
                  if ui.collapsing_header("Tile", imgui::TreeNodeFlags::empty()) {
                      ui.indent_by(10.0);
                      Self::draw_tile_debug_ui(context, tile);
                      ui.unindent_by(10.0);
                  }
              } else {
                  // Draw tile debug ui directly without nesting under a collapsing header.
                  Self::draw_tile_debug_ui(context, tile);
              }

              sim.draw_game_object_debug_ui(context, tile, DebugUiMode::Detailed);
          });

        self.is_open = is_open;
    }

    fn try_get_selected_tile(&self) -> Option<&Tile> {
        if !self.is_open {
            return None;
        }
        let tile_ref = match &self.selected {
            Some(tile_ref) => tile_ref,
            None => return None,
        };
        tile_ref.try_tile()
    }

    fn try_get_selected_tile_mut(&mut self) -> Option<&mut Tile> {
        if !self.is_open {
            return None;
        }
        let tile_ref = match &mut self.selected {
            Some(tile_ref) => tile_ref,
            None => return None,
        };
        tile_ref.try_tile_mut()
    }

    fn make_stable_imgui_window_label(tile: &Tile) -> String {
        // If the tile has an associated game object, we'll use it as the imgui window
        // ID, since it is the most stable handle we can get.
        let game_object_handle = tile.game_object_handle();
        if game_object_handle.is_valid() {
            return format!("{} - ID({},{:x})",
                           tile.kind(),
                           game_object_handle.index(),
                           game_object_handle.kind());
        }

        // Use the tile cell as a fallback. This is fine as long as the
        // tile doesn't move, so should be OK for terrain & prop tiles.
        format!("Tile: {} @ {}", tile.name(), tile.base_cell())
    }

    fn draw_tile_debug_ui(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        Self::tile_properties_dropdown(context, tile);
        Self::tile_flags_dropdown(context, tile);
        Self::tile_variations_dropdown(context, tile);
        Self::tile_animations_dropdown(context, tile);
        Self::tile_debug_opts_dropdown(context, tile);
        Self::tile_def_editor_dropdown(context, tile);
    }

    fn tile_properties_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        let ui = context.ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Building/Properties.
        if !ui.collapsing_header("Properties##_tile_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        // Display-only properties:
        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables {
            name: &'static str,
            category: &'static str,
            kind: TileKind,
            flags: TileFlags,
            path_kind: PathNodeKind,
            has_game_object: bool,
            size_in_cells: Size,
            draw_size: Size,
            logical_size: Size,
            color: Color,
        }

        let debug_vars = DrawDebugUiVariables {
            name: tile.name(),
            category: tile.category_name(),
            kind: tile.kind(),
            flags: tile.flags(),
            path_kind: tile.path_kind(),
            has_game_object: tile.game_object_handle().is_valid(),
            size_in_cells: tile.size_in_cells(),
            draw_size: tile.draw_size(),
            logical_size: tile.logical_size(),
            color: tile.tint_color()
        };

        debug_vars.draw_debug_ui(context.ui_sys);
        ui.separator();

        // Editing the cell only for single cell Object Tiles for now; no building
        // blockers/terrain support.
        let read_only_cell = tile.occupies_multiple_cells() || tile.is(TileKind::Terrain);

        // Editable properties:
        let mut start_cell = tile.cell_range().start;
        if imgui_ui::input_i32_xy(ui, "Start Cell:", &mut start_cell, read_only_cell, None, None) {
            // If we've moved the tile, update the game-side state:
            if tile.is(TileKind::Building) {
                if let Some(building) = context.world.find_building_for_tile_mut(tile) {
                    building.teleport(context.tile_map, start_cell);
                }
            } else if tile.is(TileKind::Unit) {
                if let Some(unit) = context.world.find_unit_for_tile_mut(tile) {
                    unit.teleport(context.tile_map, start_cell);
                }
            } else {
                // No associated game object to update, just try to move the tile alone.
                context.tile_map.try_move_tile(tile.base_cell(), start_cell, tile.layer_kind());
            }
        }

        let mut end_cell = tile.cell_range().end;
        imgui_ui::input_i32_xy(ui, "End Cell:", &mut end_cell, true, None, None);

        let mut iso_coords = tile.iso_coords();
        if imgui_ui::input_i32_xy(ui, "Iso Coords:", &mut iso_coords, false, None, None) {
            tile.set_iso_coords(iso_coords);
        }

        let mut screen_coords = tile.screen_rect(context.transform).position();
        imgui_ui::input_f32_xy(ui, "Screen Coords:", &mut screen_coords, true, None, None);

        let mut z_sort_key = tile.z_sort_key();
        if imgui_ui::input_i32(ui, "Z Sort Key:", &mut z_sort_key, false, None) {
            tile.set_z_sort_key(z_sort_key);
        }

        if tile.is_stacked() {
            ui.separator();
            ui.text("Stack:");

            let mut maybe_next_tile: Option<&Tile> = Some(tile);

            while let Some(next_tile) = maybe_next_tile {
                ui.text(format!("{} {} ->", next_tile.name(), next_tile.base_cell()));
                ui.same_line();
                maybe_next_tile = context.tile_map.next_tile(next_tile);
            }

            ui.text("~");
        }
    }

    fn tile_flags_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Flags", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        macro_rules! tile_flag_ui_checkbox {
            ($ui:ident, $tile:ident, $flag_name:ident) => {
                let mut value = $tile.has_flags(TileFlags::$flag_name);
                if $ui.checkbox(stringify!($flag_name), &mut value) {
                    $tile.set_flags(TileFlags::$flag_name, value);
                }
            };
        }

        tile_flag_ui_checkbox!(ui, tile, Hidden);
        tile_flag_ui_checkbox!(ui, tile, Highlighted);
        tile_flag_ui_checkbox!(ui, tile, Invalidated);
        tile_flag_ui_checkbox!(ui, tile, OccludesTerrain);
        tile_flag_ui_checkbox!(ui, tile, BuildingRoadLink);
        tile_flag_ui_checkbox!(ui, tile, SettlersSpawnPoint);
        tile_flag_ui_checkbox!(ui, tile, DirtRoadPlacement);
        tile_flag_ui_checkbox!(ui, tile, PavedRoadPlacement);
        tile_flag_ui_checkbox!(ui, tile, RandomizePlacement);
        tile_flag_ui_checkbox!(ui, tile, DrawDebugInfo);
        tile_flag_ui_checkbox!(ui, tile, DrawDebugBounds);
        tile_flag_ui_checkbox!(ui, tile, DrawBlockerInfo);
    }

    fn tile_variations_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        if !tile.has_variations() {
            return;
        }

        let ui = context.ui_sys.builder();
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

    fn tile_animations_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        if !tile.has_animations() {
            return;
        }

        let ui = context.ui_sys.builder();
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
            #[debug_ui(separator)]
            looping: bool,

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
            frame_play_time_secs: tile.anim_frame_play_time_secs()
        };

        debug_vars.draw_debug_ui(context.ui_sys);
    }

    fn tile_debug_opts_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        let ui = context.ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Building/Debug
        // Options.
        if !ui.collapsing_header("Debug Options##_tile_debug_opts", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

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
    fn tile_def_editor_dropdown(context: &mut sim::debug::DebugContext, tile: &mut Tile) {
        if tile.is(TileKind::Blocker) {
            return;
        }

        let ui = context.ui_sys.builder();
        if !ui.collapsing_header("Edit TileDef", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        ui.indent_by(3.0);
        if ui.collapsing_header("Path Flags", imgui::TreeNodeFlags::empty()) {
            if let Some(editable_def) = tile.try_get_editable_tile_def() {
                ui.indent_by(5.0);
                editable_def.path_kind.draw_debug_ui(context.ui_sys);
                ui.unindent_by(5.0);
            }
        }
        ui.unindent_by(3.0);

        if ui.button("Refresh all Tiles") {
            super::utils::refresh_cached_tile_visuals(context.tile_map);
        }

        ui.separator();

        let mut color = tile.tint_color();
        if imgui_ui::input_color(ui, "Color:", &mut color) {
            if let Some(editable_def) = tile.try_get_editable_tile_def() {
                // Prevent invalid values.
                editable_def.color = color.clamp();
            }
        }

        ui.separator();

        let mut draw_size = tile.draw_size();
        if imgui_ui::input_i32_xy(ui, "Draw Size:", &mut draw_size, false, None, Some(["W", "H"])) {
            if let Some(editable_def) = tile.try_get_editable_tile_def() {
                if draw_size.is_valid() {
                    editable_def.draw_size = draw_size;
                    tile.on_tile_def_edited();
                }
            }
        }

        // Terrain tile logical size is always fixed - disallow editing.
        if tile.is(TileKind::Terrain) {
            return;
        }

        ui.separator();

        let mut logical_size = tile.logical_size();
        if imgui_ui::input_i32_xy(ui,
                                  "Logical Size:",
                                  &mut logical_size,
                                  false,
                                  Some([BASE_TILE_SIZE.width, BASE_TILE_SIZE.height]),
                                  Some(["W", "H"]))
        {
            if let Some(editable_def) = tile.try_get_editable_tile_def() {
                if logical_size.is_valid() // Must be a multiple of BASE_TILE_SIZE.
                    && (logical_size.width  % BASE_TILE_SIZE.width)  == 0
                    && (logical_size.height % BASE_TILE_SIZE.height) == 0
                {
                    editable_def.logical_size = logical_size;
                    tile.on_tile_def_edited();
                }
            }
        }

        ui.separator();

        let mut occludes_terrain = tile.has_flags(TileFlags::OccludesTerrain);
        if ui.checkbox("Occludes terrain", &mut occludes_terrain) {
            if let Some(editable_def) = tile.try_get_editable_tile_def() {
                editable_def.occludes_terrain = occludes_terrain;
            }
            tile.set_flags(TileFlags::OccludesTerrain, occludes_terrain);
        }
    }
}
