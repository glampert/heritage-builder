use crate::{
    tile::Tile,
    game::menu::TileInspector,
    utils::mem::{RcMut, WeakMut, WeakRef},
    app::input::{InputAction, MouseButton},
    ui::{UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

pub struct TileInspectorMenu {
    // TODO / WIP
}

pub type TileInspectorMenuRcMut   = RcMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakMut = WeakMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakRef = WeakRef<TileInspectorMenu>;

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

impl TileInspectorMenu {
    pub fn new(_context: &mut UiWidgetContext) -> TileInspectorMenuRcMut {
        TileInspectorMenuRcMut::new(Self {})
    }

    pub fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}
