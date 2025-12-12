use std::any::Any;

use super::{
    GameMenusSystem,
    GameMenusContext,
    TilePlacement,
    TileInspector,
    TilePalette,
    TilePaletteSelection,
    widgets::TilePaletteWidget,
};
use crate::{
    save::{Save, Load},
    engine::Engine,
    imgui_ui::UiInputEvent,
    render::{RenderSystem, TextureHandle},
    app::input::{InputAction, MouseButton},
    tile::{self, Tile, TileKind, BASE_TILE_SIZE},
    utils::{Size, Color, Vec2, Rect, RectTexCoords, coords::{CellRange, WorldToScreenTransform}},
};

// ----------------------------------------------
// HUD -> Heads Up Display, AKA in-game menus
// ----------------------------------------------

type HudTilePlacement = TilePlacement;

pub struct HudMenus {
    tile_placement: HudTilePlacement,
    tile_palette:   HudTilePalette,
    tile_inspector: HudTileInspector,
}

impl HudMenus {
    pub fn new() -> Self {
        Self {
            tile_placement: HudTilePlacement::new(),
            tile_palette:   HudTilePalette::new(),
            tile_inspector: HudTileInspector::new(),
        }
    }
}

impl GameMenusSystem for HudMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn tile_placement(&mut self) -> &mut TilePlacement {
        &mut self.tile_placement
    }

    fn tile_palette(&mut self) -> &mut dyn TilePalette {
        &mut self.tile_palette
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        Some(&mut self.tile_inspector)
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        self.tile_palette.draw(context.engine,
                               context.cursor_screen_pos,
                               context.camera.transform(),
                               context.tile_selection.has_valid_placement());
    }
}

// ----------------------------------------------
// Save/Load for HudMenus
// ----------------------------------------------

impl Save for HudMenus {}

impl Load for HudMenus {}

// ----------------------------------------------
// HudTilePalette
// ----------------------------------------------

struct HudTilePalette {
    widget: TilePaletteWidget,
    clear_icon_sprite: TextureHandle,
    left_mouse_button_held: bool,
}

impl HudTilePalette {
    fn new() -> Self {
        Self {
            widget: TilePaletteWidget::new(),
            clear_icon_sprite: TextureHandle::invalid(),
            left_mouse_button_held: false,
        }
    }

    fn draw(&mut self,
            engine: &mut dyn Engine,
            cursor_screen_pos: Vec2,
            transform: WorldToScreenTransform,
            has_valid_placement: bool) {
        // Lazily loaded on first rendered frame.
        if !self.clear_icon_sprite.is_valid() {
            let file_path = super::ui_assets_path().join("red_x_icon.png");
            self.clear_icon_sprite = engine.texture_cache().load_texture(file_path.to_str().unwrap());
        }

        self.widget.draw(engine.texture_cache(), engine.ui_system());
        self.draw_selected_tile(engine.render_system(), cursor_screen_pos, transform, has_valid_placement);
    }

    fn draw_selected_tile(&self,
                          render_sys: &mut dyn RenderSystem,
                          cursor_screen_pos: Vec2,
                          transform: WorldToScreenTransform,
                          has_valid_placement: bool) {
        if !self.has_selection() {
            return;
        }

        // Draw clear icon under the cursor:
        if self.current_selection().is_clear() {
            const CLEAR_ICON_SIZE: Size = BASE_TILE_SIZE;

            let rect = Rect::from_pos_and_size(
                Vec2::new(
                    cursor_screen_pos.x - (CLEAR_ICON_SIZE.width  / 2) as f32,
                    cursor_screen_pos.y - (CLEAR_ICON_SIZE.height / 2) as f32),
                CLEAR_ICON_SIZE
            );

            render_sys.draw_textured_colored_rect(rect,
                                                  &RectTexCoords::DEFAULT,
                                                  self.clear_icon_sprite,
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
            let highlight_color = if has_valid_placement { Color::white() } else { tile::rendering::INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7 // Semi-transparent
                );

                render_sys.draw_textured_colored_rect(cursor_transform.scale_and_offset_rect(rect),
                                                      &sprite_frame.tex_info.coords,
                                                      sprite_frame.tex_info.texture,
                                                      tile_color * highlight_color);
            }
        }
    }
}

impl TilePalette for HudTilePalette {
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
        self.widget.current_selection
    }

    fn clear_selection(&mut self) {
        self.widget.clear_selection();
        self.left_mouse_button_held = false;
    }
}

// ----------------------------------------------
// HudTileInspector
// ----------------------------------------------

struct HudTileInspector {
    // TODO / WIP
}

impl HudTileInspector {
    fn new() -> Self {
        Self {}
    }
}

impl TileInspector for HudTileInspector {
    fn on_mouse_button(&mut self,
                       _button: MouseButton,
                       _action: InputAction,
                       _selected_tile: &Tile)
                       -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn close(&mut self) {
    }
}
