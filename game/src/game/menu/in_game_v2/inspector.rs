use crate::{
    tile::Tile,
    game::menu::TileInspector,
    app::input::{InputAction, MouseButton},
    ui::{UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

pub struct TileInspectorMenu {
    // TODO / WIP
}

impl TileInspectorMenu {
    pub fn new(_context: &mut UiWidgetContext) -> Self {
        Self {}
    }
}

impl TileInspector for TileInspectorMenu {
    fn on_mouse_button(&mut self,
                       _button: MouseButton,
                       _action: InputAction,
                       _selected_tile: &Tile)
                       -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn close(&mut self) {
    }
}
