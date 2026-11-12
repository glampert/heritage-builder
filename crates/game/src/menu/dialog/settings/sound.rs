// The Sound tab: the master volume and each kind's, in percent, and muting in the background.

use super::*;

const VOLUME_RANGE: std::ops::RangeInclusive<u32> = 0..=100;

fn percent(volume: f32) -> SettingValue {
    SettingValue::Number((volume.clamp(0.0, 1.0) * 100.0).round() as u32)
}

fn volume(value: SettingValue) -> f32 {
    value.as_number().min(*VOLUME_RANGE.end()) as f32 / 100.0
}

pub(super) fn settings() -> Vec<SettingDef> {
    let tab = SettingsTab::Sound;
    let slider = |label: UiString, read: fn(&GameConfigs) -> SettingValue, write: fn(&mut GameConfigs, SettingValue)| SettingDef {
        tab,
        label,
        control: SettingControl::Slider { range: VOLUME_RANGE, suffix: "%" },
        restart: false,
        read,
        write,
    };

    vec![
        slider(
            UiString::key(UiTextCategory::Menu, "settings.master_volume"),
            |configs| percent(configs.engine.sound_settings.master_volume),
            |configs, value| configs.engine.sound_settings.master_volume = volume(value),
        ),
        slider(
            UiString::key(UiTextCategory::Menu, "settings.music_volume"),
            |configs| percent(configs.engine.sound_settings.music_master_volume),
            |configs, value| configs.engine.sound_settings.music_master_volume = volume(value),
        ),
        slider(
            UiString::key(UiTextCategory::Menu, "settings.sfx_volume"),
            |configs| percent(configs.engine.sound_settings.sfx_master_volume),
            |configs, value| configs.engine.sound_settings.sfx_master_volume = volume(value),
        ),
        slider(
            UiString::key(UiTextCategory::Menu, "settings.ambience_volume"),
            |configs| percent(configs.engine.sound_settings.ambience_master_volume),
            |configs, value| configs.engine.sound_settings.ambience_master_volume = volume(value),
        ),
        slider(
            UiString::key(UiTextCategory::Menu, "settings.narration_volume"),
            |configs| percent(configs.engine.sound_settings.narration_master_volume),
            |configs, value| configs.engine.sound_settings.narration_master_volume = volume(value),
        ),
        slider(
            UiString::key(UiTextCategory::Menu, "settings.spatial_volume"),
            |configs| percent(configs.engine.sound_settings.spatial_master_volume),
            |configs, value| configs.engine.sound_settings.spatial_master_volume = volume(value),
        ),
        // Stored only: see EngineConfigs::mute_in_background's TODO(realm-ui).
        SettingDef {
            tab,
            label: UiString::key(UiTextCategory::Menu, "settings.mute_in_background"),
            control: SettingControl::Toggle,
            restart: false,
            read: |configs| SettingValue::Bool(configs.engine.mute_in_background),
            write: |configs, value| configs.engine.mute_in_background = value.as_bool(),
        },
    ]
}
