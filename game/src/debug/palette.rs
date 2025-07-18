use std::collections::HashMap;

use crate::{
    app::input::{InputAction, MouseButton},
    imgui_ui::{UiInputEvent, UiSystem},
    render::{RenderSystem, TextureCache, TextureHandle},
    utils::{
        Color,
        Size,
        Vec2,
        Rect,
        RectTexCoords,
        coords::WorldToScreenTransform
    },
    tile::{
        rendering::INVALID_TILE_COLOR,
        sets::{TileKind, TileDef, TileDefHandle, TileSets, BASE_TILE_SIZE}
    }
};

// ----------------------------------------------
// SelectionState
// ----------------------------------------------

enum SelectionState {
    TileSelected(TileDefHandle),
    ClearSelected,
    NoSelection,
}

impl Default for SelectionState {
    fn default() -> Self { SelectionState::NoSelection }
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
    pub fn new(start_open: bool, tex_cache: &mut impl TextureCache) -> Self {
        Self {
            start_open: start_open,
            clear_button_image: tex_cache.load_texture("assets/ui/x.png"),
            ..Default::default()
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = SelectionState::NoSelection;
        self.selected_index = HashMap::default();
        self.left_mouse_button_held = false;
    }

    pub fn has_selection(&self) -> bool {
        matches!(self.selection, SelectionState::NoSelection) == false
    }

    pub fn is_clear_selected(&self) -> bool {
        matches!(self.selection, SelectionState::ClearSelected) == true
    }

    pub fn current_selection<'tile_sets>(&self, tile_sets: &'tile_sets TileSets) -> Option<&'tile_sets TileDef> {
        match self.selection {
            SelectionState::TileSelected(selected_tile_def_handle) => {
                tile_sets.handle_to_tile_def(selected_tile_def_handle)
            },
            _ => None
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
        let tex_cache = render_sys.texture_cache();

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
                let tile_kinds = [
                    TileKind::Terrain,
                    TileKind::Vegetation,
                    TileKind::Prop,
                    TileKind::Building,
                    TileKind::Unit];

                for (idx, tile_kind) in tile_kinds.into_iter().enumerate() {
                    self.draw_tile_list(tile_kind,
                                        ui_sys,
                                        tex_cache,
                                        tile_sets,
                                        tile_size,
                                        tiles_per_row,
                                        padding_between_tiles);

                    if idx != 0 {
                        ui.new_line();
                    }
                    ui.separator();
                }

                ui.text("Tools");
                {
                    let ui_texture = ui_sys.to_ui_texture(render_sys.texture_cache(), self.clear_button_image);

                    let bg_color = if self.is_clear_selected() {
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
                        self.selection = SelectionState::ClearSelected;
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

        if !self.has_selection() {
            return;
        }

        // Draw clear tile icon under the cursor:
        if self.is_clear_selected() {
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
                RectTexCoords::default_ref(),
                self.clear_button_image,
                Color::white());
        } else {
            let selected_tile = self.current_selection(tile_sets).unwrap();
            let rect = Rect::from_pos_and_size(cursor_screen_pos, selected_tile.draw_size);

            let offset =
                if selected_tile.is(TileKind::Building | TileKind::Prop | TileKind::Vegetation) {
                    Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32))
                } else {
                    Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32 / 2.0))
                };

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);

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

    fn draw_tile_list(&mut self,
                      tile_kind: TileKind,
                      ui_sys: &UiSystem,
                      tex_cache: &impl TextureCache,
                      tile_sets: &TileSets,
                      tile_size: [f32; 2],
                      tiles_per_row: usize,
                      padding_between_tiles: f32) {

        let ui = ui_sys.builder();
        ui.text(tile_kind.to_string());

        let mut tile_index = 0;

        tile_sets.for_each_tile_def(|tile_set, tile_category, tile_def| {
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
                self.selection = SelectionState::TileSelected(TileDefHandle::new(tile_set, tile_category, tile_def));
                self.selected_index.insert(tile_kind, tile_index);
            }

            // Move to next column unless it's the last in row.
            if (tile_index + 1) % tiles_per_row != 0 {
                ui.same_line_with_spacing(0.0, padding_between_tiles);
            }

            tile_index += 1;
            true
        });
    }
}
