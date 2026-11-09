use std::sync::Mutex;

use common::{Color, Size};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{
    app::{ApplicationApi, ApplicationContentScale, ApplicationWindowMode},
    cli,
    file_sys::paths::{self, AssetPath, PathRef},
    log,
    render::{RenderApi, texture::TextureSettings},
    save::{self, storage, *},
    sound::SoundGlobalSettings,
};

// ----------------------------------------------
// Configs
// ----------------------------------------------

// Configs are read-only assets under assets/configs. Those that the player can change (the game
// configs, through the settings dialog) also have a user configs file: only the values that
// differ from the assets' defaults, as JSON, in saves/user/<name>.json on desktop and under the
// same key in the browser's localStorage on the web (see save::storage's user files). Loading
// reads the defaults and applies the user configs over them; saving writes the user configs
// again, or deletes them once nothing differs, so the assets are never written.
pub fn configs_path() -> AssetPath {
    paths::assets_path().join("configs")
}

pub trait Configs {
    fn post_load(&'static mut self) {}
}

// Either succeeds loading the config file under assets/configs or returns a default config.
pub fn load_asset_file<T>(config_file_name: PathRef) -> T
where
    T: Default + DeserializeOwned,
{
    debug_assert!(!config_file_name.is_empty());

    let config_json_path = configs_path().join(config_file_name).with_extension("json");

    let mut state = save::new_json_save_state(false);

    if let Err(err) = state.read_file(&config_json_path) {
        log::error!(log::channel!("config"), "Failed to read config file from path {config_json_path}: {err}");
        return T::default();
    }

    match state.load_new_instance::<T>() {
        Ok(configs) => configs,
        Err(err) => {
            log::error!(log::channel!("config"), "Failed to deserialize config file from path {config_json_path}: {err}");
            T::default()
        }
    }
}

// ----------------------------------------------
// User configs
// ----------------------------------------------

// The defaults each user configs file was loaded over, as JSON, by user file name: saving
// compares with them. Kept from the load since the web build can read an asset only once.
static USER_CONFIGS_DEFAULTS: Mutex<Vec<(&'static str, Value)>> = Mutex::new(Vec::new());

// Loads the defaults from assets/configs/<config_file_name>.json, then, if
// `apply_user_configs` and the `--default-configs` option isn't given, applies the user
// configs <user_file_name> over them. A user configs file that fails to read or doesn't fit
// the configs is logged and left out.
pub fn load_with_user_configs<T>(config_file_name: PathRef, user_file_name: &'static str, apply_user_configs: bool) -> T
where
    T: Default + Serialize + DeserializeOwned,
{
    let defaults: T = load_asset_file(config_file_name);

    // Configs are plain data; serializing them can't fail.
    let defaults_json = serde_json::to_value(&defaults).expect("Configs should serialize to JSON");
    remember_defaults(user_file_name, defaults_json.clone());

    if !apply_user_configs {
        return defaults;
    }

    let user_file = PathRef::from_str(user_file_name);
    let user_file_path = storage::user_file_path(user_file);

    if cli::get().has("default-configs") {
        log::info!(log::channel!("config"), "--default-configs: leaving out the user configs in {user_file_path}.");
        return defaults;
    }

    let user_configs = match storage::read_user_file(user_file) {
        Ok(Some(text)) => text,
        Ok(None) => return defaults,
        Err(err) => {
            log::error!(log::channel!("config"), "Using the default configs: {err}");
            return defaults;
        }
    };

    let user_configs: Value = match serde_json::from_str(&user_configs) {
        Ok(user_configs) => user_configs,
        Err(err) => {
            log::error!(log::channel!("config"), "Using the default configs: {user_file_path} isn't valid JSON: {err}");
            return defaults;
        }
    };

    let mut merged = defaults_json;
    apply_overlay(&mut merged, user_configs);

    match serde_json::from_value(merged) {
        Ok(configs) => {
            log::info!(log::channel!("config"), "Applied the user configs from {user_file_path}.");
            configs
        }
        Err(err) => {
            log::error!(log::channel!("config"), "Using the default configs: {user_file_path} doesn't fit them: {err}");
            defaults
        }
    }
}

// Writes the values of `configs` that differ from the defaults they were loaded over to the
// user configs file <user_file_name>, or deletes it if none do.
pub fn save_user_configs<T>(user_file_name: &'static str, configs: &T) -> bool
where
    T: Serialize,
{
    let Some(defaults) = remembered_defaults(user_file_name) else {
        log::error!(log::channel!("config"), "Can't save the user configs '{user_file_name}': their configs weren't loaded.");
        return false;
    };

    let user_file = PathRef::from_str(user_file_name);
    let user_file_path = storage::user_file_path(user_file);

    let current = serde_json::to_value(configs).expect("Configs should serialize to JSON");

    let result = match overlay_diff(&defaults, &current) {
        Some(user_configs) => {
            let text = serde_json::to_string_pretty(&user_configs).expect("JSON values should serialize");
            storage::write_user_file(user_file, &text)
        }
        None => storage::delete_user_file(user_file),
    };

    match result {
        Ok(()) => {
            log::info!(log::channel!("config"), "Saved the user configs to {user_file_path}.");
            true
        }
        Err(err) => {
            log::error!(log::channel!("config"), "Failed to save the user configs: {err}");
            false
        }
    }
}

fn remember_defaults(user_file_name: &'static str, defaults: Value) {
    let mut all_defaults = USER_CONFIGS_DEFAULTS.lock().unwrap();
    match all_defaults.iter_mut().find(|(name, _)| *name == user_file_name) {
        Some((_, entry)) => *entry = defaults,
        None => all_defaults.push((user_file_name, defaults)),
    }
}

fn remembered_defaults(user_file_name: &str) -> Option<Value> {
    let all_defaults = USER_CONFIGS_DEFAULTS.lock().unwrap();
    all_defaults.iter().find(|(name, _)| *name == user_file_name).map(|(_, defaults)| defaults.clone())
}

// An externally tagged enum's value with fields serializes as an object with one key, its
// variant ({"Preset": {...}}). An overlay object that shares no key with such an object is
// another variant, so it replaces the object instead of merging into it.
fn is_other_enum_variant(base: &Map<String, Value>, overlay: &Map<String, Value>) -> bool {
    base.len() == 1 && overlay.len() == 1 && !overlay.keys().any(|key| base.contains_key(key))
}

// Applies `overlay` over `base`: objects merge key by key, recursively, and any other value
// (numbers, strings, arrays) replaces the base's. Keys `base` lacks are added, and the configs'
// deserialization ignores them, e.g. a setting that was removed.
fn apply_overlay(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) if !is_other_enum_variant(base, &overlay) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(base_value) => apply_overlay(base_value, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

// What apply_overlay needs to turn `base` into `current`: the values that differ, under their
// objects' keys. None if nothing differs. It can't remove a key, which configs structs never
// need (serde writes every field); a map-typed setting would have to be saved whole.
fn overlay_diff(base: &Value, current: &Value) -> Option<Value> {
    match (base, current) {
        (Value::Object(base), Value::Object(current)) if !is_other_enum_variant(base, current) => {
            let mut diff = Map::new();
            for (key, value) in current {
                let value_diff = match base.get(key) {
                    Some(base_value) => overlay_diff(base_value, value),
                    None => Some(value.clone()),
                };
                if let Some(value_diff) = value_diff {
                    diff.insert(key.clone(), value_diff);
                }
            }
            (!diff.is_empty()).then_some(Value::Object(diff))
        }
        (base, current) => (base != current).then(|| current.clone()),
    }
}

// A value this build can't represent (e.g. a desktop-only enum variant on the web) reads as the
// default instead of failing the whole configs file.
#[cfg(feature = "web")]
fn deserialize_or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

// ----------------------------------------------
// Macro: configurations
// ----------------------------------------------

// Declares a configs singleton loaded from assets/configs/<configs_path>.json. With
// `user_configs: "<name>"`, it also has a user configs file (see Configs above): load() applies
// it, load_defaults() leaves it out (tests), and save() writes it.
#[macro_export]
macro_rules! configurations {
    ($configs_singleton:ident, $configs_type:ty, $configs_path:literal) => {
        ::common::singleton_late_init! { $configs_singleton, $configs_type }
        impl $crate::config::Configs for $configs_type {}
        impl $configs_type {
            pub fn load() -> &'static $configs_type {
                #[allow(unused_imports)] // Types with their own post_load don't need it.
                use $crate::config::Configs;
                <$configs_type>::initialize($crate::config::load_asset_file::<$configs_type>(
                    $crate::file_sys::paths::PathRef::from_str($configs_path),
                ));
                <$configs_type>::get_mut().post_load();
                <$configs_type>::get()
            }
        }
    };
    ($configs_singleton:ident, $configs_type:ty, $configs_path:literal, user_configs: $user_file:literal) => {
        ::common::singleton_late_init! { $configs_singleton, $configs_type }
        impl $crate::config::Configs for $configs_type {}
        impl $configs_type {
            // The defaults from the assets with the user configs over them.
            pub fn load() -> &'static $configs_type {
                <$configs_type>::load_impl(true)
            }
            // The defaults from the assets only.
            pub fn load_defaults() -> &'static $configs_type {
                <$configs_type>::load_impl(false)
            }
            // Writes the user configs: what differs from the defaults.
            pub fn save() -> bool {
                $crate::config::save_user_configs($user_file, <$configs_type>::get())
            }
            fn load_impl(apply_user_configs: bool) -> &'static $configs_type {
                #[allow(unused_imports)] // Types with their own post_load don't need it.
                use $crate::config::Configs;
                <$configs_type>::initialize($crate::config::load_with_user_configs::<$configs_type>(
                    $crate::file_sys::paths::PathRef::from_str($configs_path),
                    $user_file,
                    apply_user_configs,
                ));
                <$configs_type>::get_mut().post_load();
                <$configs_type>::get()
            }
        }
    };
}

// ----------------------------------------------
// EngineConfigs
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)] // Missing fields in the config file get defaults from EngineConfigs::default().
pub struct EngineConfigs {
    // Window:
    pub window_title: String,
    pub window_size: Size,
    pub window_background_color: Color,
    pub window_mode: ApplicationWindowMode,
    pub resizable_window: bool,
    pub confine_cursor_to_window: bool,
    pub content_scale: ApplicationContentScale, // Optional override. Defaults to System.

    // The web build always runs Winit and Wgpu, and lacks the desktop-only values (Glfw, OpenGl):
    // it reads those as the defaults, so the rest of configs.json still loads.
    #[cfg_attr(feature = "web", serde(deserialize_with = "deserialize_or_default"))]
    pub app_api: ApplicationApi,

    // Graphics:
    #[cfg_attr(feature = "web", serde(deserialize_with = "deserialize_or_default"))]
    pub render_api: RenderApi,
    pub use_packed_texture_atlas: bool,
    #[debug_ui(nested)]
    pub texture_settings: TextureSettings,

    // Sound System:
    #[debug_ui(skip)]
    pub sound_settings: SoundGlobalSettings,

    // Debug Grid:
    pub grid_color: Color,
    pub grid_line_thickness: f32,

    // Debug Log:
    pub log_level: log::Level,

    // The language of the player's text: assets/configs/ui/text/<language>.json (see ui::text).
    // TODO(realm-ui): Phase 1's Game settings tab sets it, with ui::text::set_language, and saves
    // it in the user configs.
    pub language: String,
}

impl Default for EngineConfigs {
    fn default() -> Self {
        Self {
            // Window:
            window_title: "Heritage Builder".into(),
            window_size: Size::new(1024, 768),
            window_background_color: Color::black(),
            window_mode: ApplicationWindowMode::Windowed,
            resizable_window: false,
            confine_cursor_to_window: true,
            content_scale: ApplicationContentScale::default(),
            app_api: ApplicationApi::default(),

            // Graphics:
            render_api: RenderApi::default(),
            use_packed_texture_atlas: false,
            texture_settings: TextureSettings::default(),

            // Sound System:
            sound_settings: SoundGlobalSettings::default(),

            // Debug Grid:
            grid_color: Color::white(),
            grid_line_thickness: 1.0,

            // Debug Log:
            log_level: log::Level::Verbose,

            language: crate::ui::text::ENGLISH.into(),
        }
    }
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn round_trip(base: Value, current: Value) -> Option<Value> {
        let diff = overlay_diff(&base, &current);
        let mut merged = base;
        if let Some(diff) = diff.clone() {
            apply_overlay(&mut merged, diff);
        }
        assert_eq!(merged, current);
        diff
    }

    #[test]
    fn diff_holds_only_changed_values() {
        let base = json!({ "camera": { "zoom": 1.0, "speed": 500 }, "title": "HB", "list": [1, 2] });
        let current = json!({ "camera": { "zoom": 2.0, "speed": 500 }, "title": "HB", "list": [1, 3] });
        assert_eq!(round_trip(base.clone(), current), Some(json!({ "camera": { "zoom": 2.0 }, "list": [1, 3] })));
        assert_eq!(round_trip(base.clone(), base), None);
    }

    #[test]
    fn enum_variants_replace_each_other() {
        let base = json!({ "map": { "EmptyMap": { "size": [64, 64], "name": "grass" } } });
        let other_variant = json!({ "map": { "Preset": { "number": 2 } } });
        assert_eq!(round_trip(base.clone(), other_variant.clone()), Some(other_variant));

        let same_variant = json!({ "map": { "EmptyMap": { "size": [32, 32], "name": "grass" } } });
        assert_eq!(round_trip(base.clone(), same_variant), Some(json!({ "map": { "EmptyMap": { "size": [32, 32] } } })));

        let unit_variant = json!({ "map": "None" });
        assert_eq!(round_trip(base, unit_variant.clone()), Some(unit_variant));
    }

    #[test]
    fn overlay_keeps_unknown_keys_out_of_the_way() {
        // A user configs file naming a removed setting still applies the rest.
        let mut base = json!({ "camera": { "zoom": 1.0, "speed": 500 } });
        apply_overlay(&mut base, json!({ "camera": { "zoom": 2.0, "removed": true } }));
        assert_eq!(base, json!({ "camera": { "zoom": 2.0, "speed": 500, "removed": true } }));
    }
}
