#![allow(clippy::too_many_arguments)]

use smallvec::SmallVec;
use std::cmp::Reverse;
use std::collections::HashMap;

use crate::{
    app::input::{InputAction, MouseButton},
    engine::DebugDraw,
    game::{sim, building::{config::BuildingConfigs, BuildingArchetypeKind}},
    imgui_ui::{UiInputEvent, UiSystem},
    render::{TextureCache, TextureHandle},
    utils::{self, coords::WorldToScreenTransform, Color, Rect, RectTexCoords, Size, Vec2},
    tile::{
        TileKind, BASE_TILE_SIZE,
        rendering::INVALID_TILE_COLOR,
        sets::{TileCategory, TileDef, TileDefHandle, TileSet, TileSets},
    },
};

// ----------------------------------------------
// SelectionState
// ----------------------------------------------

#[derive(Default)]
enum SelectionState {
    #[default]
    NoSelection,
    TileSelected(TileDefHandle),
    ClearSelected,
}

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TilePaletteMenu {
    start_open: bool,
    left_mouse_button_held: bool,
    selection: SelectionState,
    selected_index: HashMap<TileKind, usize>, // For highlighting the selected button.
    clear_button_image: TextureHandle,
}

impl TilePaletteMenu {
    pub fn new(start_open: bool, tex_cache: &mut dyn TextureCache) -> Self {
        Self { start_open,
               clear_button_image: tex_cache.load_texture("assets/ui/x.png"),
               ..Default::default() }
    }

    pub fn clear_selection(&mut self) {
        self.selection = SelectionState::NoSelection;
        self.selected_index = HashMap::default();
        self.left_mouse_button_held = false;
    }

    pub fn has_selection(&self) -> bool {
        !matches!(self.selection, SelectionState::NoSelection)
    }

    pub fn is_clear_selected(&self) -> bool {
        matches!(self.selection, SelectionState::ClearSelected)
    }

    pub fn current_selection(&self) -> Option<&'static TileDef> {
        match self.selection {
            SelectionState::TileSelected(selected_tile_def_handle) => {
                TileSets::get().handle_to_tile_def(selected_tile_def_handle)
            }
            _ => None,
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
                context: &mut sim::debug::DebugContext,
                debug_draw: &mut dyn DebugDraw,
                cursor_screen_pos: Vec2,
                has_valid_placement: bool,
                show_selection_bounds: bool) {
        let ui = context.ui_sys.builder();
        let tex_cache = debug_draw.texture_cache();

        let tile_size = [BASE_TILE_SIZE.width as f32, BASE_TILE_SIZE.height as f32];
        let tiles_per_row = 2;
        let padding_between_tiles = 4.0;

        let window_width = (tile_size[0] + padding_between_tiles) * tiles_per_row as f32;
        let window_margin = 45.0; // pixels from the right edge

        // X position = screen width - estimated window width - margin
        // Y position = 20px
        let window_position = [ui.io().display_size[0] - window_width - window_margin, 20.0];

        let window_flags = imgui::WindowFlags::ALWAYS_AUTO_RESIZE | imgui::WindowFlags::NO_RESIZE;

        ui.window("Tile Selection")
            .flags(window_flags)
            .collapsed(!self.start_open, imgui::Condition::FirstUseEver)
            .position(window_position, imgui::Condition::FirstUseEver)
            .build(|| {
                ui.text("Tools");
                {
                    let ui_texture = context.ui_sys.to_ui_texture(tex_cache, self.clear_button_image);

                    let bg_color = if self.is_clear_selected() {
                        Color::white().to_array()
                    } else {
                        Color::gray().to_array()
                    };

                    let clicked = ui
                        .image_button_config("Clear", ui_texture, tile_size)
                        .background_col(bg_color)
                        .build();

                    if ui.is_item_hovered() {
                        ui.tooltip_text("Clear Tiles");
                    }

                    if clicked {
                        self.clear_selection();
                        self.selection = SelectionState::ClearSelected;
                    }
                }

                let sections = [
                    ("Terrain", TileKind::Terrain),
                    ("",        TileKind::Building),
                    ("Props",   TileKind::Prop | TileKind::Vegetation),
                    ("Units",   TileKind::Unit),
                ];

                for (label, tile_kind) in sections {
                    self.draw_tile_list(label,
                                        tile_kind,
                                        context.ui_sys,
                                        tex_cache,
                                        tile_size,
                                        tiles_per_row,
                                        padding_between_tiles);
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
        if self.is_clear_selected() {
            const CLEAR_ICON_SIZE: Size = Size::new(64, 32);

            let rect = Rect::from_pos_and_size(Vec2::new(cursor_screen_pos.x
                                                         - (CLEAR_ICON_SIZE.width / 2) as f32,
                                                         cursor_screen_pos.y
                                                         - (CLEAR_ICON_SIZE.height / 2) as f32),
                                                     CLEAR_ICON_SIZE);

            debug_draw.textured_colored_rect(rect,
                                             &RectTexCoords::DEFAULT,
                                             self.clear_button_image,
                                             Color::white());
        } else {
            let selected_tile = self.current_selection().unwrap();
            let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size);

            let offset =
                if selected_tile.is(TileKind::Building | TileKind::Prop | TileKind::Vegetation) {
                    Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32))
                } else {
                    Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0),
                              -(selected_tile.draw_size.height as f32 / 2.0))
                };

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                debug_draw.textured_colored_rect(cursor_transform.scale_and_offset_rect(rect),
                                                 &sprite_frame.tex_info.coords,
                                                 sprite_frame.tex_info.texture,
                                                 Color::new(selected_tile.color.r,
                                                            selected_tile.color.g,
                                                            selected_tile.color.b,
                                                            0.7)
                                                 * highlight_color);
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
                      tile_size: [f32; 2],
                      tiles_per_row: usize,
                      padding_between_tiles: f32) {
        let ui = ui_sys.builder();

        if !label.is_empty() {
            ui.text(label);
        }

        let mut tile_index = 0;
        let mut draw_tile_button = |tile_set: &TileSet, tile_category: &TileCategory, tile_def: &TileDef| {
            if !tile_def.is(tile_kind) {
                return true;
            }

            let tile_texture = tile_def.texture_by_index(0, 0, 0);
            let ui_texture = ui_sys.to_ui_texture(tex_cache, tile_texture);

            let is_selected = self.selected_index.get(&tile_kind) == Some(&tile_index);
            let bg_color = if is_selected {
                Color::white().to_array()
            } else {
                Color::gray().to_array()
            };

            let button_text = utils::snake_case_to_title::<64>(&tile_def.name);

            let clicked =
                ui.image_button_config(button_text, ui_texture, tile_size)
                    .background_col(bg_color)
                    .tint_col(tile_def.color.to_array())
                    .build();

            // Show tooltip when hovered:
            if ui.is_item_hovered() {
                ui.tooltip_text(button_text);

                if tile_def.cost != 0 {
                    ui.tooltip_text(format!("Cost: {} gold", tile_def.cost));
                }
            }

            if clicked {
                self.clear_selection();
                self.selection =
                    SelectionState::TileSelected(TileDefHandle::new(tile_set,
                                                                    tile_category,
                                                                    tile_def));
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
        tile_defs.sort_by(|a, b| Reverse(a.3).cmp(&Reverse(b.3)));

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
