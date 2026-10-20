use std::{convert::TryFrom, rc::Rc};

use common::mem::{RcMut, WeakMut};
use engine::log;

use super::*;

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
    fn create_widget(&self, this: WeakMut<dyn Setting>) -> UiWidgetImpl;
}

struct SettingImpl<T, OnReadFn, OnCommitFn>
where
    T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
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
where
    T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
    SettingsWidgetValue: From<T>,
    OnReadFn: Fn() -> T,
    OnCommitFn: Fn(T),
{
    fn new(
        label: &'static str,
        widget_kind: SettingsWidgetKind,
        on_read_value: OnReadFn,
        on_commit_value: OnCommitFn,
    ) -> Self {
        Self { widget_label: label, widget_kind, needs_commit: false, value: T::default(), on_read_value, on_commit_value }
    }
}

impl<T, OnReadFn, OnCommitFn> Setting for SettingImpl<T, OnReadFn, OnCommitFn>
where
    T: Copy + Clone + Default + TryFrom<SettingsWidgetValue>,
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

    fn create_widget(&self, this: WeakMut<dyn Setting>) -> UiWidgetImpl {
        let read_weak_ref = this.clone().into_not_mut();
        let write_weak_ref = this;

        match self.widget_kind {
            SettingsWidgetKind::SliderU32(min, max) => UiWidgetImpl::from(
                UiSlider::new(min..=max)
                    .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
                    .on_read_value(move |_, _| -> u32 {
                        let setting = read_weak_ref.upgrade().unwrap();
                        let widget_value = setting.to_widget_value();
                        widget_value.try_into().unwrap_or_else(|_| panic!("Expected u32!"))
                    })
                    .on_update_value(move |_, _, new_value: u32| {
                        let mut setting = write_weak_ref.upgrade().unwrap();
                        setting.set_from_widget_value(new_value.into());
                    }),
            ),
            SettingsWidgetKind::Dropdown(ref items) => UiWidgetImpl::from(
                UiDropdown::new(items)
                    .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
                    .on_get_current_selection(move |_, _| {
                        let setting = read_weak_ref.upgrade().unwrap();
                        let widget_value = setting.to_widget_value();
                        widget_value.try_into().unwrap_or_else(|_| panic!("Expected usize!"))
                    })
                    .on_selection_changed(move |dropdown, _| {
                        let mut setting = write_weak_ref.upgrade().unwrap();
                        let selection_index = dropdown.current_selection_index();
                        setting.set_from_widget_value(selection_index.into());
                    }),
            ),
            SettingsWidgetKind::Checkbox => UiWidgetImpl::from(
                UiCheckbox::new()
                    .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
                    .on_read_value(move |_, _| {
                        let setting = read_weak_ref.upgrade().unwrap();
                        let widget_value = setting.to_widget_value();
                        widget_value.try_into().unwrap_or_else(|_| panic!("Expected bool!"))
                    })
                    .on_update_value(move |_, _, new_value| {
                        let mut setting = write_weak_ref.upgrade().unwrap();
                        setting.set_from_widget_value(new_value.into());
                    }),
            ),
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
            // Leaves out runtime changes made by automation commands (config.set).
            crate::automation::save_game_configs();
            log::info!(log::channel!("settings"), "GameConfigs saved successfully.");
        }
    }

    fn add_setting<S: Setting + 'static>(&mut self, setting: S) -> &mut Self {
        let rc: Rc<dyn Setting> = Rc::new(setting);
        self.settings.push(RcMut::from(rc));
        self
    }

    fn build_menu(
        &self,
        this: SettingsCategoryWeakMut,
        context: &mut GameUiContext,
        dialog_menu_kind: DialogMenuKind,
        heading_title: &[&str],
    ) -> UiMenuRcMut {
        // -------------
        // Widgets:
        // -------------

        let mut labeled_widget_group = UiLabeledWidgetGroup::new()
            .with_label_spacing(DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING)
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING.y);

        for setting in &self.settings {
            labeled_widget_group.add_widget(setting.widget_label(), setting.create_widget(setting.downgrade()));
        }

        // -------------
        // Buttons:
        // -------------

        let ok_button_weak_ref = this.clone().into_not_mut();
        let ok_button = make_text_button(context, "Ok", move |_, context| {
            let settings = ok_button_weak_ref.upgrade().unwrap();
            settings.commit_settings();
            super::close_current(context_as_mut::<GameUiContext>(context));
        });

        let side_by_side_button_group = UiWidgetGroup::new()
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING * 4.0)
            .with_stack_vertically(false)
            .with_widget(ok_button)
            .with_widget(make_dialog_close_button(context, "Cancel"));

        // -------------
        // Menu:
        // -------------

        let mut menu = make_default_layout_dialog_menu(
            context,
            dialog_menu_kind,
            heading_title,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            [],
        );

        // Refresh settings categories when the menu opens.
        let menu_open_weak_ref = this.clone();
        menu.set_open_close_callback(move |_, _, is_open| {
            if is_open {
                let mut settings = menu_open_weak_ref.upgrade().unwrap();
                settings.read_settings();
            }
        });

        menu.add_widget(labeled_widget_group);
        menu.add_widget(UiSeparator::new().with_thickness(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x));
        menu.add_widget(side_by_side_button_group);

        menu
    }
}
