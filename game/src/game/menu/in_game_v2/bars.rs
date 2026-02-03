use crate::{
    game::menu::GameMenusInputArgs,
    utils::mem::{RcMut, WeakMut, WeakRef},
    ui::{UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// InGameMenuBars
// ----------------------------------------------

pub struct InGameMenuBars {
    // TODO / WIP
}

pub type InGameMenuBarsRcMut   = RcMut<InGameMenuBars>;
pub type InGameMenuBarsWeakMut = WeakMut<InGameMenuBars>;
pub type InGameMenuBarsWeakRef = WeakRef<InGameMenuBars>;

impl InGameMenuBars {
    pub fn new(_context: &mut UiWidgetContext) -> InGameMenuBarsRcMut {
        InGameMenuBarsRcMut::new(Self {})
    }

    pub fn handle_input(&mut self, _context: &mut UiWidgetContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    pub fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}
