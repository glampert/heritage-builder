use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    implement_dialog_menu,
    game::menu::ButtonDef,
};

// ----------------------------------------------
// SettingsMainButtonKind
// ----------------------------------------------

const SETTINGS_MAIN_BUTTON_COUNT: usize = SettingsMainButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum SettingsMainButtonKind {
    #[strum(props(Label = "Game"))]
    Game,

    #[strum(props(Label = "Sound"))]
    Sound,

    #[strum(props(Label = "Graphics"))]
    Graphics,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl SettingsMainButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::Game     => super::open(DialogMenuKind::SettingsGame, CLOSE_ALL_OTHERS, context),
            Self::Sound    => super::open(DialogMenuKind::SettingsSound, CLOSE_ALL_OTHERS, context),
            Self::Graphics => super::open(DialogMenuKind::SettingsGraphics, CLOSE_ALL_OTHERS, context),
            Self::Back     => super::close_current(context),
        }
    }
}

impl ButtonDef for SettingsMainButtonKind {}

// ----------------------------------------------
// SettingsMain
// ----------------------------------------------

pub struct SettingsMain {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsMain, "Settings" }

impl SettingsMain {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut buttons = ArrayVec::<UiWidgetImpl, SETTINGS_MAIN_BUTTON_COUNT>::new();

        for button_kind in SettingsMainButtonKind::iter() {
            let on_pressed = UiTextButtonPressed::with_closure(
                move |_button, context| {
                    button_kind.on_pressed(context);
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
                DialogMenuKind::SettingsMain,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Some(buttons)
            )
        }
    }
}

// ----------------------------------------------
// SettingsGame
// ----------------------------------------------

pub struct SettingsGame {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsGame, "Game Settings" }

impl SettingsGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::SettingsMain,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Option::<Vec<UiWidgetImpl>>::None
            )
        }
    }
}

// ----------------------------------------------
// SettingsSound
// ----------------------------------------------

pub struct SettingsSound {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsSound, "Sound Settings" }

impl SettingsSound {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::SettingsMain,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Option::<Vec<UiWidgetImpl>>::None
            )
        }
    }
}

// ----------------------------------------------
// SettingsGraphics
// ----------------------------------------------

pub struct SettingsGraphics {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsGraphics, "Graphics Settings" }

impl SettingsGraphics {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            menu: make_default_dialog_menu_layout(
                context,
                DialogMenuKind::SettingsMain,
                Self::TITLE,
                DEFAULT_DIALOG_MENU_BUTTON_SPACING,
                Option::<Vec<UiWidgetImpl>>::None
            )
        }
    }
}

// ----------------------------------------------
// SettingsWidgetKind
// ----------------------------------------------

#[derive(Copy, Clone)]
enum SettingsWidgetKind {
    SliderU32,
    Dropdown,
    Checkbox,
}

// ----------------------------------------------
// SettingsCategoryKind
// ----------------------------------------------

const SETTINGS_CATEGORY_COUNT: usize = SettingsCategoryKind::COUNT;

#[derive(Copy, Clone, EnumCount)]
enum SettingsCategoryKind {
    Game,
    Sound,
    Graphics,
}

// ----------------------------------------------
// Setting
// ----------------------------------------------

trait Setting {
    fn read(&mut self);
    fn commit(&self);
}

struct SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone,
          OnReadFn: Fn() -> T + 'static,
          OnCommitFn: Fn(T) + 'static,
{
    value: T,
    on_read_value: OnReadFn,
    on_commit_value: OnCommitFn,
    widget_kind: SettingsWidgetKind,
    category_kind: SettingsCategoryKind,
}

// ----------------------------------------------
// SettingsCategory
// ----------------------------------------------

struct SettingsCategory {
    kind: SettingsCategoryKind,
    settings: Vec<Box<dyn Setting>>,
}

impl SettingsCategory {
    fn new(kind: SettingsCategoryKind) -> Self {
        Self { kind, settings: Vec::new() }
    }

    fn read_settings(&mut self) {
        for setting in &mut self.settings {
            setting.read();
        }
    }

    fn commit_settings(&self) {
        for setting in &self.settings {
            setting.commit();
        }
    }

    fn add_setting<S: Setting + 'static>(&mut self, setting: S) {
        self.settings.push(Box::new(setting));
    }
}
