use strum::{EnumCount, EnumIter, EnumProperty};

use super::*;
use crate::menu::ButtonDef;

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
    fn on_pressed(self, context: &mut GameUiContext) -> bool {
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
    pub fn new(context: &mut GameUiContext) -> Self {
        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_WIDGET_SPACING, []);

        let about_text = UiMenuHeading::new([
            UiText::new("A City Builder by Core System Games", DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
            UiText::new("Copyright © 2026. All Rights Reserved", DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
            UiText::new(format!("Version {}", crate::version()), DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE),
        ])
        .with_margin_bottom(DEFAULT_DIALOG_MENU_HEADING_MARGIN_BOTTOM);

        let mut button_group = UiWidgetGroup::new().with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING);
        for button in make_dialog_button_widgets::<AboutButtonKind, ABOUT_BUTTON_COUNT>(context) {
            button_group.add_widget(button);
        }

        menu.add_widget(about_text);
        menu.add_widget(button_group);

        Self { menu }
    }
}
