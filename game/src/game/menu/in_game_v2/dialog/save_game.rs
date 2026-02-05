use super::*;

// ----------------------------------------------
// SaveGameDialog
// ----------------------------------------------

pub struct SaveGameDialog {
    // TODO / WIP
}

impl DialogMenu for SaveGameDialog {
    fn kind(&self) -> DialogKind {
        DialogKind::SaveGame
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

impl SaveGameDialog {
    pub fn new(_context: &mut UiWidgetContext) -> Rc<Self> {
        Rc::new(Self {})
    }
}
