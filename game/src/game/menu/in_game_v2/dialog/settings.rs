use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::game::menu::ButtonDef;

// ----------------------------------------------
// SettingsMenuButtonKind
// ----------------------------------------------

const SETTINGS_MENU_BUTTON_SPACING: f32 = 8.0;
const SETTINGS_MENU_BUTTON_COUNT: usize = SettingsMenuButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum SettingsMenuButtonKind {
    #[strum(props(Label = "Game"))]
    Game,

    #[strum(props(Label = "Sound"))]
    Sound,

    #[strum(props(Label = "Graphics"))]
    Graphics,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for SettingsMenuButtonKind {}

// ----------------------------------------------
// Settings
// ----------------------------------------------

const SETTINGS_MENU_HEADING_TITLE: &str = "Settings";

pub struct Settings {
    menu: UiMenuRcMut,
}

impl DialogMenu for Settings {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        DialogMenuKind::Settings
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        &mut self.menu
    }
}

impl Settings {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut buttons = ArrayVec::<UiWidgetImpl, SETTINGS_MENU_BUTTON_COUNT>::new();

        for button_kind in SettingsMenuButtonKind::iter() {
            let on_pressed = UiTextButtonPressed::with_fn(
                |_button, _context| {
                    // TODO: button action
                }
            );

            buttons.push(UiWidgetImpl::from(
                button_kind.new_text_button(
                    context,
                    UiTextButtonSize::Large,
                    true,
                    on_pressed
                )
            ));
        }

        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::Settings,
                SETTINGS_MENU_HEADING_TITLE,
                SETTINGS_MENU_BUTTON_SPACING,
                buttons
            )
        }
    }
}
