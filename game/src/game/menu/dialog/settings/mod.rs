use std::{rc::Rc, convert::TryFrom};

use super::*;
use crate::{
    log,
    utils::mem::{RcMut, WeakMut},
    game::{config::GameConfigs, menu::TEXT_BUTTON_HOVERED_SPRITE},
};

mod main;
pub use main::MainSettings;

mod game;
pub use game::GameSettings;

mod sound;
pub use sound::SoundSettings;

mod graphics;
pub use graphics::GraphicsSettings;

// ----------------------------------------------
// SettingsWidgetKind
// ----------------------------------------------

enum SettingsWidgetKind {
    SliderU32(u32, u32),   // (min, max)
    Dropdown(Vec<String>), // (items)
    Checkbox,
}

// ----------------------------------------------
// SettingsWidgetValue & Conversions
// ----------------------------------------------

#[derive(Copy, Clone)]
enum SettingsWidgetValue {
    U32(u32),
    Usize(usize),
    Bool(bool),
}

impl From<u32> for SettingsWidgetValue {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}

impl TryFrom<SettingsWidgetValue> for u32 {
    type Error = ();
    fn try_from(v: SettingsWidgetValue) -> Result<Self, Self::Error> {
        match v {
            SettingsWidgetValue::U32(x) => Ok(x),
            _ => Err(()),
        }
    }
}

impl From<usize> for SettingsWidgetValue {
    fn from(value: usize) -> Self {
        Self::Usize(value)
    }
}

impl TryFrom<SettingsWidgetValue> for usize {
    type Error = ();
    fn try_from(v: SettingsWidgetValue) -> Result<Self, Self::Error> {
        match v {
            SettingsWidgetValue::Usize(x) => Ok(x),
            _ => Err(()),
        }
    }
}

impl From<bool> for SettingsWidgetValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl TryFrom<SettingsWidgetValue> for bool {
    type Error = ();
    fn try_from(v: SettingsWidgetValue) -> Result<Self, Self::Error> {
        match v {
            SettingsWidgetValue::Bool(x) => Ok(x),
            _ => Err(()),
        }
    }
}

// ----------------------------------------------
// Setting / SettingImpl
// ----------------------------------------------

trait Setting {
    fn read(&mut self);
    fn commit(&self) -> bool;

    fn to_widget_value(&self) -> SettingsWidgetValue;
    fn set_from_widget_value(&mut self, new_value: SettingsWidgetValue);

    fn widget_label(&self) -> String;
    fn create_widget(&self, this: WeakMut<dyn Setting>, context: &mut UiWidgetContext) -> UiWidgetImpl;
}

struct SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
          SettingsWidgetValue: From<T>,
          OnReadFn: Fn() -> T,
          OnCommitFn: Fn(T),
{
    widget_label: &'static str,
    widget_kind: SettingsWidgetKind,
    needs_commit: bool,
    value: T,
    on_read_value: OnReadFn,
    on_commit_value: OnCommitFn,
}

impl<T, OnReadFn, OnCommitFn> SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
          SettingsWidgetValue: From<T>,
          OnReadFn: Fn() -> T,
          OnCommitFn: Fn(T),
{
    fn new(label: &'static str,
           widget_kind: SettingsWidgetKind,
           on_read_value: OnReadFn,
           on_commit_value: OnCommitFn) -> Self {
        Self {
            widget_label: label,
            widget_kind,
            needs_commit: false,
            value: T::default(),
            on_read_value,
            on_commit_value,
        }
    }
}

impl<T, OnReadFn, OnCommitFn> Setting for SettingImpl<T, OnReadFn, OnCommitFn>
    where T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
          SettingsWidgetValue: From<T>,
          OnReadFn: Fn() -> T,
          OnCommitFn: Fn(T),
{
    fn read(&mut self) {
        self.value = (self.on_read_value)();
        self.needs_commit = false;
    }

    fn commit(&self) -> bool {
        if self.needs_commit {
            (self.on_commit_value)(self.value);
        }
        self.needs_commit
    }

    fn to_widget_value(&self) -> SettingsWidgetValue {
        self.value.into()
    }

    fn set_from_widget_value(&mut self, new_value: SettingsWidgetValue) {
        self.value = new_value.try_into().unwrap_or_else(|_| panic!("Unknown SettingsWidgetValue kind!"));
        self.needs_commit = true; // Commit only if modified by the widget.
    }

    fn widget_label(&self) -> String {
        self.widget_label.into()
    }

    fn create_widget(&self, this: WeakMut<dyn Setting>, context: &mut UiWidgetContext) -> UiWidgetImpl {
        match self.widget_kind {
            SettingsWidgetKind::SliderU32(min, max) => {
                let read_weak_ref  = this.clone().into_not_mut();
                let write_weak_ref = this.clone();
                UiWidgetImpl::from(UiSlider::new(
                    context,
                    UiSliderParams {
                        font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                        min,
                        max,
                        on_read_value: UiSliderReadValue::with_closure(move |_, _| {
                            let setting = read_weak_ref.upgrade().unwrap();
                            let widget_value = setting.to_widget_value();
                            widget_value.try_into().unwrap_or_else(|_| panic!("Expected u32!"))
                        }),
                        on_update_value: UiSliderUpdateValue::with_closure(move |_, _, new_value: u32| {
                            let mut setting = write_weak_ref.upgrade().unwrap();
                            setting.set_from_widget_value(new_value.into());
                        }),
                        ..Default::default()
                    }
                ))
            }
            SettingsWidgetKind::Dropdown(ref items) => {
                let read_weak_ref  = this.clone().into_not_mut();
                let write_weak_ref = this.clone();
                UiWidgetImpl::from(UiDropdown::new(
                    context,
                    UiDropdownParams {
                        font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                        items: items.clone(),
                        on_get_current_selection: UiDropdownGetCurrentSelection::with_closure(move |_, _| {
                            let setting = read_weak_ref.upgrade().unwrap();
                            let widget_value = setting.to_widget_value();
                            widget_value.try_into().unwrap_or_else(|_| panic!("Expected usize!"))
                        }),
                        on_selection_changed: UiDropdownSelectionChanged::with_closure(move |dropdown, _| {
                            let mut setting = write_weak_ref.upgrade().unwrap();
                            let selection_index = dropdown.current_selection_index();
                            setting.set_from_widget_value(selection_index.into());
                        }),
                        ..Default::default()
                    }
                ))
            }
            SettingsWidgetKind::Checkbox => {
                let read_weak_ref  = this.clone().into_not_mut();
                let write_weak_ref = this.clone();
                UiWidgetImpl::from(UiCheckbox::new(
                    context,
                    UiCheckboxParams {
                        font_scale: DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE,
                        on_read_value: UiCheckboxReadValue::with_closure(move |_, _| {
                            let setting = read_weak_ref.upgrade().unwrap();
                            let widget_value = setting.to_widget_value();
                            widget_value.try_into().unwrap_or_else(|_| panic!("Expected bool!"))
                        }),
                        on_update_value: UiCheckboxUpdateValue::with_closure(move |_, _, new_value: bool| {
                            let mut setting = write_weak_ref.upgrade().unwrap();
                            setting.set_from_widget_value(new_value.into());
                        }),
                        ..Default::default()
                    }
                ))
            }
        }
    }
}

// ----------------------------------------------
// SettingsCategory
// ----------------------------------------------

struct SettingsCategory {
    settings: Vec<RcMut<dyn Setting>>,
}

type SettingsCategoryRcMut   = RcMut<SettingsCategory>;
type SettingsCategoryWeakMut = WeakMut<SettingsCategory>;

impl SettingsCategory {
    fn new() -> SettingsCategoryRcMut {
        RcMut::new(Self { settings: Vec::new() })
    }

    fn read_settings(&mut self) {
        for setting in &mut self.settings {
            setting.read();
        }
    }

    fn commit_settings(&self) {
        let mut any_commited = false;

        for setting in &self.settings {
            any_commited |= setting.commit();
        }

        if any_commited {
            GameConfigs::save();
            log::info!(log::channel!("settings"), "GameConfigs saved successfully.");
        }
    }

    fn add_setting<S: Setting + 'static>(&mut self, setting: S) -> &mut Self {
        let rc: Rc<dyn Setting> = Rc::new(setting);
        self.settings.push(RcMut::from(rc));
        self
    }

    fn build_menu(&self,
                  this: SettingsCategoryWeakMut,
                  context: &mut UiWidgetContext,
                  dialog_menu_kind: DialogMenuKind,
                  heading_title: &str)
                  -> UiMenuRcMut
    {
        // -------------
        // Widgets:
        // -------------

        let mut labeled_widget_group = UiLabeledWidgetGroup::new(
            context,
            UiLabeledWidgetGroupParams {
                label_spacing: DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING,
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING,
                center_vertically: false,
                center_horizontally: true,
            }
        );

        for setting in &self.settings {
            let widget = setting.create_widget(setting.downgrade(), context);
            labeled_widget_group.add_widget(setting.widget_label(), widget);
        }

        // -------------
        // Buttons:
        // -------------

        let ok_button_weak_ref = this.clone().into_not_mut();
        let ok_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Ok".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                    let settings = ok_button_weak_ref.upgrade().unwrap();
                    settings.commit_settings();
                    DialogMenusSingleton::get_mut().close_current(context);
                }),
                ..Default::default()
            }
        );

        let cancel_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Cancel".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(|_, context| {
                    DialogMenusSingleton::get_mut().close_current(context);
                }),
                ..Default::default()
            }
        );

        let mut side_by_side_button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: DEFAULT_DIALOG_MENU_WIDGET_SPACING * 4.0,
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
            }
        );

        side_by_side_button_group.add_widget(ok_button);
        side_by_side_button_group.add_widget(cancel_button);

        // -------------
        // Menu:
        // -------------

        let mut menu = make_default_dialog_menu_layout(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        // Refresh settings categories when the menu opens.
        let menu_open_weak_ref = this.clone();
        menu.set_open_close_callback(UiMenuOpenClose::with_closure(move |_, _, is_open| {
            if is_open {
                let mut settings = menu_open_weak_ref.upgrade().unwrap();
                settings.read_settings();
            }
        }));

        let spacing = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(DEFAULT_DIALOG_MENU_WIDGET_SPACING),
                ..Default::default()
            }
        );

        menu.add_widget(labeled_widget_group);
        menu.add_widget(spacing);
        menu.add_widget(side_by_side_button_group);

        menu
    }
}
