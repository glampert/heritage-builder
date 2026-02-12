use crate::{
    ui::widgets::*,
    utils::{Vec2, mem::{RcMut, WeakMut, WeakRef}},
    game::menu::{TileInspector, GameMenusContext},
};

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

pub struct TileInspectorMenu {
    menu: UiMenuRcMut,
}

pub type TileInspectorMenuRcMut   = RcMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakMut = WeakMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakRef = WeakRef<TileInspectorMenu>;

impl TileInspector for TileInspectorMenu {
    fn open(&mut self, context: &mut GameMenusContext) {
        self.menu.open(&mut context.as_ui_widget_context());
    }

    fn close(&mut self, context: &mut GameMenusContext) {
        self.menu.close(&mut context.as_ui_widget_context());
    }
}

impl TileInspectorMenu {
    pub fn new(context: &mut UiWidgetContext) -> TileInspectorMenuRcMut {
        let menu_size = Vec2::new(
            context.viewport_size.width  as f32 * 0.5 - 120.0,
            context.viewport_size.height as f32 * 0.5
        );

        let menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("TileInspector".into()),
                flags: UiMenuFlags::PauseSimIfOpen | UiMenuFlags::AlignCenter,
                size: Some(menu_size),
                background: Some("misc/square_page_bg.png"),
                ..Default::default()
            }
        );

        TileInspectorMenuRcMut::new(Self { menu })
    }

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        self.menu.draw(context);
    }
}
