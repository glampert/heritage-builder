use super::*;

// ----------------------------------------------
// MainMenuDialog
// ----------------------------------------------

pub struct MainMenuDialog {
    // TODO / WIP
}

impl DialogMenu for MainMenuDialog {
    fn kind(&self) -> DialogKind {
        DialogKind::MainMenu
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

impl MainMenuDialog {
    pub fn new(_context: &mut UiWidgetContext) -> Rc<Self> {
        Rc::new(Self {})
    }
}
