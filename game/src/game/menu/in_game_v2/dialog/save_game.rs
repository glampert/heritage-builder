use super::{
    DialogMenu,
    DialogMenuKind,
};
use crate::{
    ui::widgets::*,
};

// ----------------------------------------------
// SaveGame
// ----------------------------------------------

pub struct SaveGame {
    // TODO / WIP
}

impl DialogMenu for SaveGame {
    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::SaveGame
    }

    fn is_open(&self) -> bool {
        false
    }

    fn open(&mut self, _context: &mut UiWidgetContext) {

    }

    fn close(&mut self, _context: &mut UiWidgetContext) {

    }

    fn draw(&mut self, _context: &mut UiWidgetContext) {

    }
}

impl SaveGame {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }
}
