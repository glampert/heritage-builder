use super::{
    DialogMenu,
    DialogMenuKind,
};
use crate::{
    ui::widgets::*,
};

// ----------------------------------------------
// NewGame
// ----------------------------------------------

pub struct NewGame {
    // TODO / WIP
}

impl DialogMenu for NewGame {
    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::NewGame
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

impl NewGame {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }
}
