use super::{
    DialogMenu,
    DialogMenuKind,
};
use crate::{
    ui::widgets::*,
};

// ----------------------------------------------
// Settings
// ----------------------------------------------

pub struct Settings {
    // TODO / WIP
}

impl DialogMenu for Settings {
    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::Settings
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

impl Settings {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }
}
