use strum::EnumCount;
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::game::menu::ButtonDef;

mod workers;
pub use workers::WorkersManagement;

mod resources;
pub use resources::ResourcesManagement;

mod finances;
pub use finances::FinancesManagement;

// ----------------------------------------------
// CityManagementButtonKind
// ----------------------------------------------

const CITY_MANAGEMENT_BUTTON_COUNT: usize = CityManagementButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum CityManagementButtonKind {
    #[strum(props(Label = "Workers"))]
    Workers,

    #[strum(props(Label = "Resources"))]
    Resources,

    #[strum(props(Label = "Finances"))]
    Finances,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for CityManagementButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::Workers   => super::open(DialogMenuKind::WorkersManagement,   CLOSE_ALL_OTHERS, context),
            Self::Resources => super::open(DialogMenuKind::ResourcesManagement, CLOSE_ALL_OTHERS, context),
            Self::Finances  => super::open(DialogMenuKind::FinancesManagement,  CLOSE_ALL_OTHERS, context),
            Self::Back      => super::close_current(context),
        }
    }
}

// ----------------------------------------------
// CityManagement
// ----------------------------------------------

pub struct CityManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { CityManagement, ["City Management"] }

impl CityManagement {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let buttons = make_dialog_button_widgets::<CityManagementButtonKind, CITY_MANAGEMENT_BUTTON_COUNT>(context);

        Self {
            menu: make_default_layout_dialog_menu(
                context,
                Self::KIND,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Some(buttons)
            )
        }
    }
}
