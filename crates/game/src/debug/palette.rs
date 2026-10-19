#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use common::{self, Color, Rect, RectTexCoords, Vec2, constants::*, coords::WorldToScreenTransform};
use engine::{
    app::input::{InputAction, MouseButton},
    file_sys::paths::PathRef,
    render::{debug::DebugDraw, texture::TextureHandle},
    ui::{self, UiCondition, UiInputEvent, UiSystem, UiWindow, widgets::UiWidgetContext},
};
use smallvec::SmallVec;

use crate::{
    building::{BuildingArchetypeKind, config::BuildingConfigs},
    menu::{TilePalette, TilePaletteSelection},
    tile::{
        TileKind,
        rendering::INVALID_TILE_COLOR,
        sets::{TileCategory, TileDef, TileDefHandle, TileSet, TileSets},
    },
    ui_context::GameUiContext,
    undo_redo,
};

// ----------------------------------------------
// TilePaletteDevMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TilePaletteDevMenu {
    start_open: bool,
    left_mouse_button_pressed: bool,
    current_selection: TilePaletteSelection,
    selected_index: HashMap<TileKind, usize>, // For highlighting the selected button.
    clear_button_image: TextureHandle,
}

impl TilePalette for TilePaletteDevMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_pressed = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_pressed = false;
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn on_tile_placement_canceled(&mut self, _context: &mut GameUiContext) {
        self.reset_selection_internal();
    }

    fn clear_current_selection(&mut self, _context: &mut GameUiContext) {
        self.reset_selection_internal();
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.current_selection
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }
}

impl TilePaletteDevMenu {
    pub fn new(context: &mut GameUiContext) -> Self {
        Self {
            start_open: true,
            clear_button_image: context.load_texture(PathRef::from_str("icons/red_x_icon.png")),
            ..Default::default()
        }
    }

    pub fn draw(&mut self, context: &mut GameUiContext, debug_draw: &mut DebugDraw, show_selection_bounds: bool) {
        let ui_sys = context.ui_sys;

        let tiles_per_row = 2;
        let spacing_between_tiles = 4.0;

        let window_width = (BASE_TILE_WIDTH_F32 + spacing_between_tiles) * tiles_per_row as f32;
        let window_margin = 26.0; // pixels from the right edge

        // X position = screen width - estimated window width - margin
        // Y position = 20px
        let window_position = Vec2::new(ui::screen_size(ui_sys).x - window_width - window_margin, 20.0);

        // UI automation: ui.set on the palette selects a tile by its button label or its
        // tile name ("Small Well" or "small_well"), "Clear", or nothing ("").
        let selection = ui_sys.is_widget_registry_enabled().then(|| self.selection_json());

        let window = UiWindow::new("Tile Selection")
            .record_as("tile_palette", selection)
            .auto_resize()
            .no_resize()
            .collapsed(!self.start_open, UiCondition::FirstUse)
            .position(window_position, UiCondition::FirstUse)
            .item_spacing(Vec2::new(spacing_between_tiles, spacing_between_tiles));

        ui::window(ui_sys, window, |ui_sys| {
            let set_selection = ui::take_window_set_value(ui_sys);
            let set_selection = set_selection.as_ref().and_then(|value| value.as_str());

            match set_selection {
                Some("") => self.reset_selection_internal(),
                Some(name) if name.eq_ignore_ascii_case("Clear") => {
                    self.reset_selection_internal();
                    self.current_selection = TilePaletteSelection::Clear;
                }
                _ => {}
            }

            ui::text(ui_sys, "Tools");
            {
                let _path = ui::path_scope(ui_sys, "Tools");

                ui::horizontal(ui_sys, |ui_sys| {
                    if ui::icon_button(ui_sys, ui::icons::ICON_UNDO, Some("Undo")) {
                        undo_redo::undo(&context.new_sim_context());
                    }
                    if ui::icon_button(ui_sys, ui::icons::ICON_REDO, Some("Redo")) {
                        undo_redo::redo(&context.new_sim_context());
                    }
                });

                let btn_params = ui::UiImageButtonParams {
                    id: "Clear",
                    size: BASE_TILE_SIZE_F32,
                    texture: self.clear_button_image,
                    tooltip: Some("Clear Land"),
                    normal_color: Some(Color::gray()),
                    hovered_color: Some(Color::new(1.0, 1.0, 0.0, 0.1)), // Faint yellow
                    selected_color: Some(Color::white()),
                    selected: self.current_selection().is_clear(),
                    ..Default::default()
                };

                if ui::image_button(ui_sys, &btn_params) {
                    self.reset_selection_internal();
                    self.current_selection = TilePaletteSelection::Clear;
                }
            }

            let sections = [
                ("Terrain", TileKind::Terrain),
                ("", TileKind::Building),
                ("Props", TileKind::Rocks | TileKind::Vegetation),
                ("Units", TileKind::Unit),
            ];

            for (label, tile_kind) in sections {
                let _path = ui::path_scope(ui_sys, if label.is_empty() { "Buildings" } else { label });
                self.draw_tile_list(label, tile_kind, ui_sys, tiles_per_row, spacing_between_tiles, set_selection);
            }
        });

        self.draw_selected_tile(context, debug_draw, show_selection_bounds);
    }

    fn draw_selected_tile(&self, context: &mut GameUiContext, debug_draw: &mut DebugDraw, show_selection_bounds: bool) {
        if !self.has_selection() {
            return;
        }

        // Draw clear tile icon under the cursor:
        if self.current_selection().is_clear() {
            const CLEAR_ICON_SIZE: Vec2 = BASE_TILE_SIZE_F32;

            let rect = Rect::from_pos_and_size(context.cursor_screen_pos - (CLEAR_ICON_SIZE * 0.5), CLEAR_ICON_SIZE);

            debug_draw.textured_colored_rect(rect, &RectTexCoords::DEFAULT, self.clear_button_image, Color::white());
        } else {
            let selected_tile = self.current_selection().as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(context.cursor_screen_pos, selected_tile.draw_size.to_vec2());

            let offset = if selected_tile.is(TileKind::Building | TileKind::Rocks | TileKind::Vegetation) {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32))
            } else {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32 / 2.0))
            };

            let has_valid_placement = context.tile_selection.has_valid_placement();
            let transform = context.camera.transform();

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7, // Semi-transparent
                );

                debug_draw.textured_colored_rect(
                    cursor_transform.scale_and_offset_rect(rect),
                    &sprite_frame.tex_info.coords,
                    sprite_frame.tex_info.texture,
                    tile_color * highlight_color,
                );
            }

            if show_selection_bounds {
                debug_draw.wireframe_rect(cursor_transform.scale_and_offset_rect(rect), Color::red());
            }
        }
    }

    // `select`: a tile to select as if its button was clicked, by button label or tile name.
    fn draw_tile_list(
        &mut self,
        label: &str,
        tile_kind: TileKind,
        ui_sys: &UiSystem,
        tiles_per_row: usize,
        padding_between_tiles: f32,
        select: Option<&str>,
    ) {
        if !label.is_empty() {
            ui::text(ui_sys, label);
        }

        let mut tile_defs = SmallVec::<[(&TileSet, &TileCategory, &TileDef, Option<BuildingArchetypeKind>); 32]>::new();

        // Gather relevant tiles:
        TileSets::get().for_each_tile_def(|tile_set, tile_category, tile_def| {
            if tile_def.is(tile_kind) {
                let building_archetype = BuildingConfigs::get().find_building_archetype_kind_for_tile_def(tile_def);

                tile_defs.push((tile_set, tile_category, tile_def, building_archetype));
            }
            true
        });

        // Group by building archetype kind (if any):
        tile_defs.sort_by_key(|entry| {
            if let Some(archetype) = entry.3 {
                match archetype {
                    // Custom sort order.
                    BuildingArchetypeKind::ServiceBuilding  => 0,
                    BuildingArchetypeKind::ProducerBuilding => 1,
                    BuildingArchetypeKind::StorageBuilding  => 2,
                    BuildingArchetypeKind::HouseBuilding    => 3,
                }
            } else {
                4
            }
        });

        // Rows of up to `tiles_per_row` buttons. Each named building group starts on a new row,
        // under its name.
        let mut tile_index = 0;
        let mut row = SmallVec::<[(&TileSet, &TileCategory, &TileDef); 4]>::new();
        let mut prev_building_archetype: Option<BuildingArchetypeKind> = None;

        for (tile_set, tile_category, tile_def, building_archetype) in tile_defs {
            if row.len() == tiles_per_row {
                self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
                row.clear();
            }

            // New named building group?
            if building_archetype != prev_building_archetype {
                if let Some(archetype_kind) = building_archetype {
                    if !row.is_empty() {
                        self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
                        row.clear();
                    }
                    ui::text(ui_sys, Self::building_archetype_kind_label(archetype_kind));
                }
                prev_building_archetype = building_archetype;
            }

            row.push((tile_set, tile_category, tile_def));
        }

        if !row.is_empty() {
            self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
        }
    }

    fn draw_tile_row(
        &mut self,
        ui_sys: &UiSystem,
        tile_kind: TileKind,
        row: &[(&TileSet, &TileCategory, &TileDef)],
        tile_index: &mut usize,
        padding_between_tiles: f32,
        select: Option<&str>,
    ) {
        ui::horizontal_spaced(ui_sys, padding_between_tiles, |ui_sys| {
            for &(tile_set, tile_category, tile_def) in row {
                self.draw_tile_button(ui_sys, tile_kind, *tile_index, tile_set, tile_category, tile_def, select);
                *tile_index += 1;
            }
        });
    }

    fn draw_tile_button(
        &mut self,
        ui_sys: &UiSystem,
        tile_kind: TileKind,
        tile_index: usize,
        tile_set: &TileSet,
        tile_category: &TileCategory,
        tile_def: &TileDef,
        select: Option<&str>,
    ) {
        let selected = self.selected_index.get(&tile_kind) == Some(&tile_index);

        let tile_sprite = tile_def.texture_by_index(0, 0, 0);

        let btn_id = common::fixed_string::snake_case_to_title::<64>(&tile_def.name);
        let btn_tooltip_buf;
        let btn_tooltip = if tile_def.cost != 0 {
            btn_tooltip_buf = common::format_small!("{}\nCost: {} gold", btn_id, tile_def.cost);
            btn_tooltip_buf.as_str()
        } else {
            btn_id.as_str()
        };

        let btn_params = ui::UiImageButtonParams {
            id: &btn_id,
            size: BASE_TILE_SIZE_F32,
            texture: tile_sprite.texture,
            tooltip: Some(btn_tooltip),
            normal_color: Some(Color::gray()),
            hovered_color: Some(Color::new(1.0, 1.0, 0.0, 0.1)), // Faint yellow
            selected_color: Some(Color::white()),
            tint_color: Some(tile_def.color),
            top_left_uvs: Some(tile_sprite.coords.top_left()),
            bottom_right_uvs: Some(tile_sprite.coords.bottom_right()),
            selected,
        };

        let clicked = ui::image_button(ui_sys, &btn_params);
        let selected_by_name =
            select.is_some_and(|name| name.eq_ignore_ascii_case(&btn_id) || name.eq_ignore_ascii_case(&tile_def.name));

        if clicked || selected_by_name {
            self.reset_selection_internal();
            self.current_selection = TilePaletteSelection::Tile(TileDefHandle::new(tile_set, tile_category, tile_def));
            self.selected_index.insert(tile_kind, tile_index);
        }
    }

    fn building_archetype_kind_label(archetype_kind: BuildingArchetypeKind) -> &'static str {
        match archetype_kind {
            BuildingArchetypeKind::ProducerBuilding => "Production",
            BuildingArchetypeKind::StorageBuilding  => "Storage",
            BuildingArchetypeKind::ServiceBuilding  => "Services",
            BuildingArchetypeKind::HouseBuilding    => "Houses",
        }
    }

    // The selection as the palette records it for UI automation: the button label, "Clear" or null.
    fn selection_json(&self) -> serde_json::Value {
        if self.current_selection.is_clear() {
            return serde_json::Value::from("Clear");
        }
        match self.current_selection.as_tile_def() {
            Some(tile_def) => {
                serde_json::Value::from(common::fixed_string::snake_case_to_title::<64>(&tile_def.name).as_str())
            }
            None => serde_json::Value::Null,
        }
    }

    fn reset_selection_internal(&mut self) {
        self.left_mouse_button_pressed = false;
        self.current_selection = TilePaletteSelection::None;
        self.selected_index.clear();
    }
}
