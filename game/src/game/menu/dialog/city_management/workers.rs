use super::*;

// ----------------------------------------------
// WorkersManagement
// ----------------------------------------------

pub struct WorkersManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { WorkersManagement, ["Workers"] }

impl WorkersManagement {
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
