use crate::{
    ui::{UiFontScale, widgets::*},
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
        TileInspectorMenuRcMut::new_cyclic(|inspector_weak_ref| {
            Self::build_menu(inspector_weak_ref, context)
        })
    }

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        self.menu.draw(context);
    }

    fn build_menu(inspector_weak_ref: TileInspectorMenuWeakMut, context: &mut UiWidgetContext) -> Self {
        let menu_size = Vec2::new(
            context.viewport_size.width  as f32 * 0.5 - 120.0,
            context.viewport_size.height as f32 * 0.5
        );

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("TileInspector".into()),
                flags: UiMenuFlags::PauseSimIfOpen | UiMenuFlags::AlignCenter,
                size: Some(menu_size),
                background: Some("misc/square_page_bg.png"),
                ..Default::default()
            }
        );

        let icon = UiSpriteIcon::new(
            context,
            UiSpriteIconParams {
                sprite: "icons/app_icon_bg_large.png",
                size: Vec2::new(128.0, 128.0),
                clip_to_parent_menu: true,
                ..Default::default()
            }
        );

        let name = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: vec![
                    ("Unit Name Here".into(), UiFontScale(1.5)),
                    ("Inventory: 5 Gold".into(), UiFontScale(1.0)),
                ],
                center_vertically: false,
                center_horizontally: false,
                ..Default::default()
            }
        );

        let dialog_text = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: vec![
                    ("Some dialog this unit will say.".into(), UiFontScale(1.1)),
                    ("When clicked by the player...".into(), UiFontScale(1.1)),
                ],
                center_vertically: false,
                center_horizontally: true,
                ..Default::default()
            }
        );

        let close_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Close".into(),
                size: UiTextButtonSize::Normal,
                hover: Some("misc/brush_stroke_divider.png"),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                    let mut inspector = inspector_weak_ref.upgrade().unwrap();
                    inspector.menu.close(context);
                }),
                ..Default::default()
            }
        );

        let mut side_by_side_widget_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: 20.0,
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );
        side_by_side_widget_group.add_widget(icon);
        side_by_side_widget_group.add_widget(name);

        let mut side_by_side_button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );
        side_by_side_button_group.add_widget(close_button);

        let separator = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(10.0),
                ..Default::default()
            }
        );

        menu.add_widget(separator.clone());
        menu.add_widget(side_by_side_widget_group);
        menu.add_widget(separator.clone());
        menu.add_widget(dialog_text);
        menu.add_widget(separator.clone()); 
        menu.add_widget(side_by_side_button_group);

        Self { menu }
    }
}
