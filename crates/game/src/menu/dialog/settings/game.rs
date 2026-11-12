// The Game tab: autosave and its interval, the default game speed, edge scrolling, the three
// camera zoom switches, the UI scale, colour-blind-safe overlays and the language.

use super::*;
use crate::config::DisplayConfigs;

// The autosave intervals offered, in minutes.
const AUTOSAVE_MINUTES: [u32; 6] = [1, 2, 5, 10, 15, 30];

// The default game speeds offered (the simulation's speed, 1 to 10 times).
const GAME_SPEEDS: [u32; 5] = [1, 2, 3, 5, 10];

fn autosave_interval_items() -> Vec<String> {
    AUTOSAVE_MINUTES.iter().map(|&minutes| ui::text::format_count(UiTextCategory::Menu, "settings.autosave_every", i64::from(minutes), &[])).collect()
}

fn game_speed_items() -> Vec<String> {
    GAME_SPEEDS
        .iter()
        .map(|&speed| match speed {
            1 => ui::text::get(UiTextCategory::Menu, "settings.speed.normal").to_string(),
            _ => ui::text::format(UiTextCategory::Menu, "settings.speed.times", &[("speed", &speed.to_string())]),
        })
        .collect()
}

// The languages loaded (assets/configs/ui/text/), each by its own name where the strings have one.
fn languages() -> Vec<&'static str> {
    ui::text::languages()
}

fn language_items() -> Vec<String> {
    languages()
        .into_iter()
        .map(|language| match language {
            ui::text::ENGLISH => ui::text::get(UiTextCategory::Menu, "settings.language.en").to_string(),
            other => other.to_string(),
        })
        .collect()
}

pub(super) fn settings() -> Vec<SettingDef> {
    let tab = SettingsTab::Game;
    let toggle = |label: UiString, read: fn(&GameConfigs) -> SettingValue, write: fn(&mut GameConfigs, SettingValue)| SettingDef {
        tab,
        label,
        control: SettingControl::Toggle,
        restart: false,
        read,
        write,
    };

    vec![
        // Turning autosave on is ignored in the runs that never autosave (GameLoop::apply_autosave_settings).
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.autosave"),
            |configs| SettingValue::Bool(configs.save.enable_autosave),
            |configs, value| configs.save.enable_autosave = value.as_bool(),
        ),
        SettingDef {
            tab,
            label: UiString::key(UiTextCategory::Menu, "settings.autosave_interval"),
            control: SettingControl::Dropdown { items: autosave_interval_items() },
            restart: false,
            read: |configs| {
                let minutes = (configs.save.autosave_frequency_secs / 60.0).round().max(0.0) as u32;
                SettingValue::Choice(nearest_choice(&AUTOSAVE_MINUTES, minutes))
            },
            write: |configs, value| {
                let minutes = AUTOSAVE_MINUTES.get(value.as_choice()).copied().unwrap_or(AUTOSAVE_MINUTES[0]);
                configs.save.autosave_frequency_secs = (minutes * 60) as Seconds;
            },
        },
        SettingDef {
            tab,
            label: UiString::key(UiTextCategory::Menu, "settings.default_speed"),
            control: SettingControl::Dropdown { items: game_speed_items() },
            restart: false,
            read: |configs| SettingValue::Choice(nearest_choice(&GAME_SPEEDS, configs.sim.default_speed.round().max(0.0) as u32)),
            write: |configs, value| {
                configs.sim.default_speed = GAME_SPEEDS.get(value.as_choice()).copied().unwrap_or(GAME_SPEEDS[0]) as f32;
            },
        },
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.edge_scrolling"),
            |configs| SettingValue::Bool(configs.camera.edge_scrolling),
            |configs, value| configs.camera.edge_scrolling = value.as_bool(),
        ),
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.key_zoom"),
            |configs| SettingValue::Bool(!configs.camera.disable_key_shortcut_zoom),
            |configs, value| configs.camera.disable_key_shortcut_zoom = !value.as_bool(),
        ),
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.scroll_zoom"),
            |configs| SettingValue::Bool(!configs.camera.disable_mouse_scroll_zoom),
            |configs, value| configs.camera.disable_mouse_scroll_zoom = !value.as_bool(),
        ),
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.smooth_scroll_zoom"),
            |configs| SettingValue::Bool(!configs.camera.disable_smooth_mouse_scroll_zoom),
            |configs, value| configs.camera.disable_smooth_mouse_scroll_zoom = !value.as_bool(),
        ),
        SettingDef {
            tab,
            label: UiString::key(UiTextCategory::Menu, "settings.ui_scale"),
            control: SettingControl::Slider { range: DisplayConfigs::UI_SCALE_PERCENT_RANGE, suffix: "%" },
            restart: false,
            read: |configs| {
                let range = DisplayConfigs::UI_SCALE_PERCENT_RANGE;
                SettingValue::Number(configs.display.ui_scale_percent.clamp(*range.start(), *range.end()))
            },
            write: |configs, value| configs.display.ui_scale_percent = value.as_number(),
        },
        toggle(
            UiString::key(UiTextCategory::Menu, "settings.color_blind_overlays"),
            |configs| SettingValue::Bool(configs.display.color_blind_overlays),
            |configs, value| configs.display.color_blind_overlays = value.as_bool(),
        ),
        SettingDef {
            tab,
            label: UiString::key(UiTextCategory::Menu, "settings.language"),
            control: SettingControl::Dropdown { items: language_items() },
            restart: false,
            read: |configs| SettingValue::Choice(languages().iter().position(|language| *language == configs.engine.language).unwrap_or(0)),
            write: |configs, value| {
                if let Some(language) = languages().get(value.as_choice()) {
                    configs.engine.language = language.to_string();
                }
            },
        },
    ]
}
