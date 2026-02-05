use super::*;

// ----------------------------------------------
// SettingsDialog
// ----------------------------------------------

pub struct SettingsDialog {
    // TODO / WIP
}

impl DialogMenu for SettingsDialog {
    fn kind(&self) -> DialogKind {
        DialogKind::Settings
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

impl SettingsDialog {
    pub fn new(_context: &mut UiWidgetContext) -> Rc<Self> {
        Rc::new(Self {})
    }
}
