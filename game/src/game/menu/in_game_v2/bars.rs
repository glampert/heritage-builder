use crate::{
    game::menu::GameMenusInputArgs,
    ui::{UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// InGameMenuBars
// ----------------------------------------------

pub struct InGameMenuBars {
    // TODO / WIP
}

impl InGameMenuBars {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }

    pub fn handle_input(&mut self, _context: &mut UiWidgetContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    pub fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}
