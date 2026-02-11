use super::*;
use crate::{
    implement_dialog_menu,
    game::{GameLoop, config::GameConfigs},
};

// ----------------------------------------------
// GameSettings
// ----------------------------------------------

pub struct GameSettings {
    category: SettingsCategoryRcMut,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { GameSettings, "Game Settings" }

impl GameSettings {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut category = SettingsCategory::new();
    
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
            || !GameConfigs::get().camera.disable_key_shortcut_zoom,
            |enable| GameConfigs::get_mut().camera.disable_key_shortcut_zoom = !enable
        ))
        .add_setting(SettingImpl::new(
            "Mouse Scroll Camera Zoom",
            SettingsWidgetKind::Checkbox,
            || !GameConfigs::get().camera.disable_mouse_scroll_zoom,
            |enable| GameConfigs::get_mut().camera.disable_mouse_scroll_zoom = !enable
        ))
        .add_setting(SettingImpl::new(
            "Smooth Mouse Scroll Camera Zoom",
            SettingsWidgetKind::Checkbox,
            || !GameConfigs::get().camera.disable_smooth_mouse_scroll_zoom,
            |enable| GameConfigs::get_mut().camera.disable_smooth_mouse_scroll_zoom = !enable
        ));

        let menu = category.build_menu(
            category.downgrade(),
            context, Self::KIND,
            Self::TITLE,
            // Margins:
            0.0,
            0.0
        );

        Self { menu, category }
    }
}
