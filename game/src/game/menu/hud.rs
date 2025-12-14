use std::any::Any;

use super::{
    GameMenusSystem,
    GameMenusContext,
    TilePlacement,
    TileInspector,
    TilePalette,
    TilePaletteSelection,
    palette::TilePaletteWidget,
};
use crate::{
    tile::Tile,
    engine::Engine,
    save::{Save, Load},
    imgui_ui::UiInputEvent,
    app::input::{InputAction, MouseButton},
    utils::{Vec2, coords::{CellRange, WorldToScreenTransform}},
};

// ----------------------------------------------
// HUD -> Heads Up Display, AKA in-game menus
// ----------------------------------------------

pub struct HudMenus {
    tile_placement: TilePlacement,
    tile_palette:   HudTilePalette,
    tile_inspector: HudTileInspector,
}

impl HudMenus {
    pub fn new() -> Self {
        Self {
            tile_placement: TilePlacement::new(),
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
    left_mouse_button_pressed: bool,
}

impl HudTilePalette {
    fn new() -> Self {
        Self {
            widget: TilePaletteWidget::new(),
            left_mouse_button_pressed: false,
        }
    }

    fn draw(&mut self,
            engine: &mut dyn Engine,
            cursor_screen_pos: Vec2,
            transform: WorldToScreenTransform,
            has_valid_placement: bool) {
        self.widget.draw(engine.texture_cache(), engine.ui_system());
        self.widget.draw_selected_tile(engine.render_system(), cursor_screen_pos, transform, has_valid_placement);
    }
}

impl TilePalette for HudTilePalette {
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

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.widget.current_selection
    }

    fn clear_selection(&mut self) {
        self.widget.clear_selection();
        self.left_mouse_button_pressed = false;
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
