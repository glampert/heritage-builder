use super::*;

// ----------------------------------------------
// HUD -> Heads Up Display, AKA in-game menus
// ----------------------------------------------

#[derive(Default)]
pub struct HudMenus {
    // TODO / WIP
}

impl HudMenus {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameMenusSystem for HudMenus {
    fn handle_input(&mut self, _args: &mut GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn begin_frame(&mut self, _args: &mut GameMenusFrameArgs) -> TileMapRenderFlags {
        TileMapRenderFlags::DrawTerrainAndObjects
    }

    fn end_frame(&mut self, _args: &mut GameMenusFrameArgs) {
    }
}

// ----------------------------------------------
// Save/Load for HudMenus
// ----------------------------------------------

impl Save for HudMenus {}

impl Load for HudMenus {}
