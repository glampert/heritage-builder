use super::{
    DialogMenu,
    DialogMenuKind,
};
use crate::{
    ui::widgets::*,
};

// ----------------------------------------------
// MainMenu
// ----------------------------------------------

pub struct MainMenu {
    // TODO / WIP
}

impl DialogMenu for MainMenu {
    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::MainMenu
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

impl MainMenu {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }
}
