use strum::EnumCount;
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    game::menu::ButtonDef,
    utils,
};

// ----------------------------------------------
// AboutButtonKind
// ----------------------------------------------

const ABOUT_BUTTON_COUNT: usize = AboutButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum AboutButtonKind {
    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for AboutButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        match self {
            Self::Back => super::close_current(context),
        }
    }
}

// ----------------------------------------------
// About
// ----------------------------------------------

pub struct About {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { About, ["Heritage Builder", "The Dragon Legacy"] }

impl About {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let about_text = vec![
            UiText::new("A City Builder by Core System Games".into(), DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
            UiText::new("Copyright (C) 2026. All Rights Reserved".into(), DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
            UiText::new(format!("Version {}", utils::version()), DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
        ];

        menu.add_widget(UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: about_text,
                margin_bottom: DEFAULT_DIALOG_MENU_HEADING_MARGINS.1,
                ..Default::default()
            }
        ));

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                center_vertically: false,
                center_horizontally: true,
                ..Default::default()
            }
        );

        let buttons = make_dialog_button_widgets::<AboutButtonKind, ABOUT_BUTTON_COUNT>(context);
        for button in buttons {
            button_group.add_widget(button);
        }

        menu.add_widget(button_group);

        Self { menu }
    }
}
