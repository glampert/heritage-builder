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

    fn tile_placement(&mut self) -> &mut TilePlacement { todo!() }
    fn tile_palette(&mut self) -> &mut dyn TilePalette { todo!() }
    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> { todo!() }
}

// ----------------------------------------------
// Save/Load for HudMenus
// ----------------------------------------------

impl Save for HudMenus {}

impl Load for HudMenus {}
