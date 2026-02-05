use super::*;

// ----------------------------------------------
// NewGameDialog
// ----------------------------------------------

pub struct NewGameDialog {
    // TODO / WIP
}

impl DialogMenu for NewGameDialog {
    fn kind(&self) -> DialogKind {
        DialogKind::NewGame
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

impl NewGameDialog {
    pub fn new(_context: &mut UiWidgetContext) -> Rc<Self> {
        Rc::new(Self {})
    }
}
