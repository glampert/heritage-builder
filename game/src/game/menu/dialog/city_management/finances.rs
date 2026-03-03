use super::*;
use crate::{
    game::menu::TEXT_BUTTON_HOVERED_SPRITE,
};

// ----------------------------------------------
// FinancesManagement
// ----------------------------------------------

pub struct FinancesManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { FinancesManagement, ["Finances"] }

impl FinancesManagement {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                ..Default::default()
            }
        );

        let ok_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Ok".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(|_, context| {
                    super::close_current(context);
                }),
                ..Default::default()
            }
        );

        button_group.add_widget(ok_button);
        menu.add_widget(button_group);

        Self { menu }
    }
}
