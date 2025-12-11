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
    tile::Tile,
    save::{Save, Load},
    render::TextureCache,
    utils::coords::CellRange,
    imgui_ui::{UiInputEvent, UiSystem},
    app::input::{InputAction, MouseButton},
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
        self.tile_palette.draw(context.tex_cache, context.ui_sys);
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
}

impl HudTilePalette {
    fn new() -> Self {
        Self { widget: TilePaletteWidget::new() }
    }

    #[inline]
    fn draw(&mut self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) {
        self.widget.draw(tex_cache, ui_sys);
    }
}

// TODO / WIP
impl TilePalette for HudTilePalette {
    fn on_mouse_button(&mut self,
                       _button: MouseButton,
                       _action: InputAction)
                       -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        false
    }

    fn current_selection(&self) -> TilePaletteSelection {
        TilePaletteSelection::None
    }

    fn clear_selection(&mut self) {
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
