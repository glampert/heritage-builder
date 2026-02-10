use super::*;
use crate::{
    implement_dialog_menu,
    tile::camera::CameraGlobalSettings,
    game::GameLoop,
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

        let menu = category.build_menu(category.downgrade(), context, Self::KIND, Self::TITLE);
        Self { menu, category }
    }
}
