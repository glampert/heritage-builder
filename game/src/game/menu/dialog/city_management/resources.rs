use super::*;

// ----------------------------------------------
// ResourcesManagement
// ----------------------------------------------

pub struct ResourcesManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { ResourcesManagement, ["Resources"] }

impl ResourcesManagement {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        // TODO

        Self { menu }
    }
}
