use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    implement_dialog_menu,
    tile::camera::CameraGlobalSettings,
    game::{GameLoop, menu::ButtonDef},
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

// Settings main menu / entry point.
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
                Self::KIND,
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
    category: SettingsCategory,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsGame, "Game Settings" }

impl SettingsGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut category = SettingsCategory::default();
    
        category
        .add_setting(SettingImpl::new(
            "Autosave",
            SettingsWidgetKind::Checkbox,
            || GameLoop::get().is_autosave_enabled(),
            |enable| GameLoop::get_mut().enable_autosave(enable)
        ))
        .add_setting(SettingImpl::new(
            "Keyboard Shortcut Camera Zoom",
            SettingsWidgetKind::Checkbox,
            || !CameraGlobalSettings::get().disable_key_shortcut_zoom,
            |enable| CameraGlobalSettings::get_mut().disable_key_shortcut_zoom = !enable
        ))
        .add_setting(SettingImpl::new(
            "Mouse Scroll Camera Zoom",
            SettingsWidgetKind::Checkbox,
            || !CameraGlobalSettings::get().disable_mouse_scroll_zoom,
            |enable| CameraGlobalSettings::get_mut().disable_mouse_scroll_zoom = !enable
        ))
        .add_setting(SettingImpl::new(
            "Smooth Mouse Scroll Camera Zoom",
            SettingsWidgetKind::Checkbox,
            || !CameraGlobalSettings::get().disable_smooth_mouse_scroll_zoom,
            |enable| CameraGlobalSettings::get_mut().disable_smooth_mouse_scroll_zoom = !enable
        ));

        let menu = category.build_menu(context, Self::KIND, Self::TITLE);
        Self { menu, category }
    }
}

// ----------------------------------------------
// SettingsSound
// ----------------------------------------------

pub struct SettingsSound {
    category: SettingsCategory,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsSound, "Sound Settings" }

impl SettingsSound {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let category = SettingsCategory::default();
        // TODO
        let menu = category.build_menu(context, Self::KIND, Self::TITLE);
        Self { menu, category }
    }
}

// ----------------------------------------------
// SettingsGraphics
// ----------------------------------------------

pub struct SettingsGraphics {
    category: SettingsCategory,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SettingsGraphics, "Graphics Settings" }

impl SettingsGraphics {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let category = SettingsCategory::default();
        // TODO
        let menu = category.build_menu(context, Self::KIND, Self::TITLE);
        Self { menu, category }
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
// Setting / SettingImpl
// ----------------------------------------------

trait Setting {
    fn read(&mut self);
    fn commit(&self);
    fn create_widget(&self, context: &mut UiWidgetContext) -> UiWidgetImpl;
}

struct SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default,
          OnReadFn: Fn() -> T + 'static,
          OnCommitFn: Fn(T) + 'static,
{
    label: &'static str,
    widget_kind: SettingsWidgetKind,
    value: T,
    on_read_value: OnReadFn,
    on_commit_value: OnCommitFn,
}

impl<T, OnReadFn, OnCommitFn> Setting for SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default,
          OnReadFn: Fn() -> T + 'static,
          OnCommitFn: Fn(T) + 'static,
{
    fn read(&mut self) {
        todo!() // TODO
    }

    fn commit(&self) {
        todo!() // TODO
    }

    fn create_widget(&self, _context: &mut UiWidgetContext) -> UiWidgetImpl {
        todo!() // TODO
    }
}

impl<T, OnReadFn, OnCommitFn> SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default,
          OnReadFn: Fn() -> T + 'static,
          OnCommitFn: Fn(T) + 'static,
{
    fn new(label: &'static str,
           widget_kind: SettingsWidgetKind,
           on_read_value: OnReadFn,
           on_commit_value: OnCommitFn) -> Self {
        Self {
            label,
            widget_kind,
            value: T::default(),
            on_read_value,
            on_commit_value,
        }
    }
}

// ----------------------------------------------
// SettingsCategory
// ----------------------------------------------

#[derive(Default)]
struct SettingsCategory {
    settings: Vec<Box<dyn Setting>>,
}

impl SettingsCategory {
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

    fn add_setting<S: Setting + 'static>(&mut self, setting: S) -> &mut Self {
        self.settings.push(Box::new(setting));
        self
    }

    fn build_menu(&self,
                  context: &mut UiWidgetContext,
                  dialog_menu_kind: DialogMenuKind,
                  heading_title: &str)
                  -> UiMenuRcMut
    {
        let mut widgets = Vec::with_capacity(self.settings.len());

        for setting in &self.settings {
            let widget = setting.create_widget(context);
            widgets.push(widget);
        }

        // TODO: We want a labeled widget group instead!

        let menu = make_default_dialog_menu_layout(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_BUTTON_SPACING,
            Some(widgets)
        );

        menu
    }
}
