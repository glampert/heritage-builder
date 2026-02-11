use super::*;
use crate::{
    implement_dialog_menu,
    game::{GameLoop, config::GameConfigs},
};

// ----------------------------------------------
// Helper macros
// ----------------------------------------------

macro_rules! read_master_volume_u32 {
    ($volume_field:ident) => {{
        let sound_settings = GameLoop::get_mut().engine_mut().sound_system().current_sound_settings();
        (sound_settings.$volume_field * 100.0) as u32 // Scale from [0,1] f32 to [0,100] u32
    }};
}

macro_rules! write_mater_volume_u32 {
    ($volume_field:ident, $volume:ident) => {{
        let sound_sys = GameLoop::get_mut().engine_mut().sound_system();
        let mut sound_settings = sound_sys.current_sound_settings();
        sound_settings.$volume_field = $volume as f32 / 100.0; // Back to [0,1] f32 range.
        sound_sys.change_sound_settings(sound_settings);
        GameConfigs::get_mut().engine.sound_settings = sound_settings;
    }};
}

const VOLUME_MIN: u32 = 0;
const VOLUME_MAX: u32 = 100;

// ----------------------------------------------
// SoundSettings
// ----------------------------------------------

pub struct SoundSettings {
    category: SettingsCategoryRcMut,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SoundSettings, ["Sound Settings"] }

impl SoundSettings {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut category = SettingsCategory::new();

        category
        .add_setting(SettingImpl::new(
            "SFX Volume",
            SettingsWidgetKind::SliderU32(VOLUME_MIN, VOLUME_MAX),
            || read_master_volume_u32!(sfx_master_volume),
            |volume| write_mater_volume_u32!(sfx_master_volume, volume),
        ))
        .add_setting(SettingImpl::new(
            "Music Volume",
            SettingsWidgetKind::SliderU32(VOLUME_MIN, VOLUME_MAX),
            || read_master_volume_u32!(music_master_volume),
            |volume| write_mater_volume_u32!(music_master_volume, volume),
        ))
        .add_setting(SettingImpl::new(
            "Ambience Volume",
            SettingsWidgetKind::SliderU32(VOLUME_MIN, VOLUME_MAX),
            || read_master_volume_u32!(ambience_master_volume),
            |volume| write_mater_volume_u32!(ambience_master_volume, volume),
        ))
        .add_setting(SettingImpl::new(
            "Narration Volume",
            SettingsWidgetKind::SliderU32(VOLUME_MIN, VOLUME_MAX),
            || read_master_volume_u32!(narration_master_volume),
            |volume| write_mater_volume_u32!(narration_master_volume, volume),
        ))
        .add_setting(SettingImpl::new(
            "Spatial Volume",
            SettingsWidgetKind::SliderU32(VOLUME_MIN, VOLUME_MAX),
            || read_master_volume_u32!(spatial_master_volume),
            |volume| write_mater_volume_u32!(spatial_master_volume, volume),
        ));

        let menu = category.build_menu(
            category.downgrade(),
            context, Self::KIND,
            Self::TITLE,
            // Margins:
            50.0,
            30.0
        );

        Self { menu, category }
    }
}
