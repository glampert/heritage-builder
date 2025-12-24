#![allow(clippy::too_many_arguments)]

use smallvec::SmallVec;
use std::collections::HashMap;

use crate::{
    game::menu,
    engine::DebugDraw,
    app::input::{InputAction, MouseButton},
    imgui_ui::{self, UiInputEvent, UiSystem},
    render::{TextureCache, TextureHandle},
    utils::{
        self, coords::WorldToScreenTransform,
        Color, Rect, RectTexCoords, Size, Vec2,
    },
    game::{
        sim, building::{config::BuildingConfigs, BuildingArchetypeKind},
        menu::{TilePalette, TilePaletteSelection}, undo_redo,
    },
    tile::{
        TileKind, BASE_TILE_SIZE,
        rendering::INVALID_TILE_COLOR,
        sets::{TileCategory, TileDef, TileDefHandle, TileSet, TileSets},
    },
};

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TilePaletteMenu {
    start_open: bool,
    left_mouse_button_held: bool,
    selection: TilePaletteSelection,
    selected_index: HashMap<TileKind, usize>, // For highlighting the selected button.
    clear_button_image: TextureHandle,
}

impl TilePalette for TilePaletteMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
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

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_held && self.has_selection()
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.selection
    }

    fn clear_selection(&mut self) {
        self.selection = TilePaletteSelection::None;
        self.selected_index = HashMap::default();
        self.left_mouse_button_held = false;
    }
}

impl TilePaletteMenu {
    pub fn new(start_open: bool, tex_cache: &mut dyn TextureCache) -> Self {
        let clear_button_path = menu::ui_assets_path().join("icons/red_x_icon.png");
        let clear_button_image = tex_cache.load_texture_with_settings(clear_button_path.to_str().unwrap(), Some(menu::ui_texture_settings()));
        Self {
            start_open,
            clear_button_image,
            ..Default::default()
        }
    }

    pub fn draw(&mut self,
                context: &mut sim::debug::DebugContext,
                sim: &mut sim::Simulation,
                debug_draw: &mut dyn DebugDraw,
                cursor_screen_pos: Vec2,
                has_valid_placement: bool,
                show_selection_bounds: bool) {
        let ui = context.ui_sys.ui();
        let tex_cache = debug_draw.texture_cache();

        let tiles_per_row = 2;
        let spacing_between_tiles = 4.0;

        let window_width = (BASE_TILE_SIZE.width as f32 + spacing_between_tiles) * tiles_per_row as f32;
        let window_margin = 26.0; // pixels from the right edge

        // X position = screen width - estimated window width - margin
        // Y position = 20px
        let window_position = [ui.io().display_size[0] - window_width - window_margin, 20.0];
        let window_flags = imgui::WindowFlags::ALWAYS_AUTO_RESIZE | imgui::WindowFlags::NO_RESIZE;

        let _item_spacing =
            ui.push_style_var(imgui::StyleVar::ItemSpacing([spacing_between_tiles, spacing_between_tiles]));

        ui.window("Tile Selection")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position(window_position, imgui::Condition::FirstUseEver)
            .build(|| {
                let ui_sys = context.ui_sys;

                ui.text("Tools");
                {
                    if imgui_ui::icon_button(ui_sys, imgui_ui::icons::ICON_UNDO, Some("Undo")) {
                        undo_redo::undo(&sim.new_query(context.world, context.tile_map, context.delta_time_secs));
                    }
                    ui.same_line();
                    if imgui_ui::icon_button(ui_sys, imgui_ui::icons::ICON_REDO, Some("Redo")) {
                        undo_redo::redo(&sim.new_query(context.world, context.tile_map, context.delta_time_secs));
                    }

                    let btn_params = imgui_ui::UiImageButtonParams {
                        id: "Clear",
                        size: BASE_TILE_SIZE.to_vec2(),
                        ui_texture: ui_sys.to_ui_texture(tex_cache, self.clear_button_image),
                        tooltip: Some("Clear Tiles"),
                        normal_color: Some(Color::gray()),
                        hovered_color: Some(Color::new(1.0, 1.0, 0.0, 0.1)), // Faint yellow
                        selected_color: Some(Color::white()),
                        selected: self.current_selection().is_clear(),
                        ..Default::default()
                    };

                    if imgui_ui::image_button(ui_sys, &btn_params) {
                        self.clear_selection();
                        self.selection = TilePaletteSelection::Clear;
                    }
                }

                let sections = [
                    ("Terrain", TileKind::Terrain),
                    ("",        TileKind::Building),
                    ("Props",   TileKind::Rocks | TileKind::Vegetation),
                    ("Units",   TileKind::Unit),
                ];

                for (label, tile_kind) in sections {
                    self.draw_tile_list(label,
                                        tile_kind,
                                        ui_sys,
                                        tex_cache,
                                        tiles_per_row,
                                        spacing_between_tiles);
                }
            });

        self.draw_selected_tile(debug_draw,
                                cursor_screen_pos,
                                context.transform,
                                has_valid_placement,
                                show_selection_bounds);
    }

    fn draw_selected_tile(&self,
                          debug_draw: &mut dyn DebugDraw,
                          cursor_screen_pos: Vec2,
                          transform: WorldToScreenTransform,
                          has_valid_placement: bool,
                          show_selection_bounds: bool) {
        if !self.has_selection() {
            return;
        }

        // Draw clear tile icon under the cursor:
        if self.current_selection().is_clear() {
            const CLEAR_ICON_SIZE: Size = BASE_TILE_SIZE;

            let rect = Rect::from_pos_and_size(
                Vec2::new(
                    cursor_screen_pos.x - (CLEAR_ICON_SIZE.width  / 2) as f32,
                    cursor_screen_pos.y - (CLEAR_ICON_SIZE.height / 2) as f32),
                CLEAR_ICON_SIZE
            );

            debug_draw.textured_colored_rect(rect,
                                             &RectTexCoords::DEFAULT,
                                             self.clear_button_image,
                                             Color::white());
        } else {
            let selected_tile = self.current_selection().as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size);

            let offset =
                if selected_tile.is(TileKind::Building | TileKind::Rocks | TileKind::Vegetation) {
                    Vec2::new(-(selected_tile.draw_size.width  as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32))
                } else {
                    Vec2::new(-(selected_tile.draw_size.width  as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32 / 2.0))
                };

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7 // Semi-transparent
                );

                debug_draw.textured_colored_rect(cursor_transform.scale_and_offset_rect(rect),
                                                 &sprite_frame.tex_info.coords,
                                                 sprite_frame.tex_info.texture,
                                                 tile_color * highlight_color);
            }

            if show_selection_bounds {
                debug_draw.wireframe_rect(cursor_transform.scale_and_offset_rect(rect), Color::red());
            }
        }
    }

    fn draw_tile_list(&mut self,
                      label: &str,
                      tile_kind: TileKind,
                      ui_sys: &UiSystem,
                      tex_cache: &dyn TextureCache,
                      tiles_per_row: usize,
                      padding_between_tiles: f32) {
        let ui = ui_sys.ui();

        if !label.is_empty() {
            ui.text(label);
        }

        let mut tile_index = 0;
        let mut draw_tile_button = |tile_set: &TileSet, tile_category: &TileCategory, tile_def: &TileDef| {
            if !tile_def.is(tile_kind) {
                return true;
            }

            let selected = self.selected_index.get(&tile_kind) == Some(&tile_index);

            let tile_sprite = tile_def.texture_by_index(0, 0, 0);
            let ui_texture = ui_sys.to_ui_texture(tex_cache, tile_sprite.texture);

            let btn_id = utils::snake_case_to_title::<64>(&tile_def.name);
            let btn_tooltip = if tile_def.cost != 0 {
                &format!("{}\nCost: {} gold", btn_id, tile_def.cost)
            } else {
                btn_id.as_str()
            };

            let btn_params = imgui_ui::UiImageButtonParams {
                id: &btn_id,
                size: BASE_TILE_SIZE.to_vec2(),
                ui_texture,
                tooltip: Some(btn_tooltip),
                normal_color: Some(Color::gray()),
                hovered_color: Some(Color::new(1.0, 1.0, 0.0, 0.1)), // Faint yellow
                selected_color: Some(Color::white()),
                tint_color: Some(tile_def.color),
                top_left_uvs: Some(tile_sprite.coords.top_left()),
                bottom_right_uvs: Some(tile_sprite.coords.bottom_right()),
                selected,
            };

            if imgui_ui::image_button(ui_sys, &btn_params) {
                self.clear_selection();
                self.selection = TilePaletteSelection::Tile(TileDefHandle::new(tile_set, tile_category, tile_def));
                self.selected_index.insert(tile_kind, tile_index);
            }

            ui.same_line_with_spacing(0.0, padding_between_tiles);
            tile_index += 1;
            true
        };

        let mut tile_defs = SmallVec::<[(&TileSet, &TileCategory, &TileDef, Option<BuildingArchetypeKind>); 32]>::new();

        // Gather relevant tiles:
        TileSets::get().for_each_tile_def(|tile_set, tile_category, tile_def| {
            if tile_def.is(tile_kind) {
                let building_archetype =
                    BuildingConfigs::get().find_building_archetype_kind_for_tile_def(tile_def);

                tile_defs.push((tile_set, tile_category, tile_def, building_archetype));
            }
            true
        });

        // Group by building archetype kind (if any):
        tile_defs.sort_by_key(|entry|
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
        );

        let mut button_count_for_row = 0;
        let mut prev_building_archetype: Option<BuildingArchetypeKind> = None;

        for (tile_set, tile_category, tile_def, building_archetype) in tile_defs {
            if button_count_for_row == tiles_per_row {
                button_count_for_row = 0;
                ui.new_line();
            }

            // New named building group?
            if building_archetype != prev_building_archetype {
                if let Some(archetype_kind) = building_archetype {
                    if button_count_for_row != 0 {
                        button_count_for_row = 0;
                        ui.new_line();
                    }
                    ui.text(Self::building_archetype_kind_label(archetype_kind));
                }
                prev_building_archetype = building_archetype;
            }

            // Draw ImGui button:
            draw_tile_button(tile_set, tile_category, tile_def);
            button_count_for_row += 1;
        }

        if button_count_for_row <= tiles_per_row {
            ui.new_line();
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
}
