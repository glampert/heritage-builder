use std::any::Any;
use super::*;

// ----------------------------------------------
// HUD -> Heads Up Display, AKA in-game menus
// ----------------------------------------------

pub struct HudMenus {
    // TODO / WIP
}

impl HudMenus {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameMenusSystem for HudMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn handle_input(&mut self, _context: &mut GameMenusContext, _args: &GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn begin_frame(&mut self, _context: &mut GameMenusContext) -> TileMapRenderFlags {
        TileMapRenderFlags::DrawTerrainAndObjects
    }

    fn end_frame(&mut self, _context: &mut GameMenusContext, _visible_range: CellRange) {
    }
}

// ----------------------------------------------
// Save/Load for HudMenus
// ----------------------------------------------

impl Save for HudMenus {}

impl Load for HudMenus {}
