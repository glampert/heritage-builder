use std::path::{Path, PathBuf};
use proc_macros::DrawDebugUi;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    log,
    imgui_ui::UiSystem,
    render::TextureSettings,
    save::{self, *},
    tile::rendering,
    utils::{platform::paths, Color, Size},
};

// ----------------------------------------------
// Configs
// ----------------------------------------------

pub fn configs_path() -> PathBuf {
    paths::asset_path("configs")
}

pub trait Configs {
    fn draw_debug_ui(&'static self, _ui_sys: &UiSystem) {}
    fn post_load(&'static mut self) {}

    // Saves current configs to file.
    fn save_file(&'static self, config_file_name: &str) -> bool
        where Self: Configs + Sized + Serialize
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path =
            Path::new(&configs_path()).join(config_file_name).with_extension("json");

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = std::fs::create_dir_all(configs_path());

        let mut state = save::backend::new_json_save_state(true);

        if let Err(err) = state.save(self) {
            log::error!(log::channel!("config"),
                        "Failed to save config file {config_json_path:?}: {err}");
            return false;
        }

        if let Err(err) = state.write_file(&config_json_path) {
            log::error!(log::channel!("config"),
                        "Failed to write config file {config_json_path:?}: {err}");
            return false;
        }

        true
    }

    // Either succeeds loading the config file or returns a default config.
    fn load_file<T>(config_file_name: &str) -> T
        where T: Configs + Sized + Default + DeserializeOwned
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path =
            Path::new(&configs_path()).join(config_file_name).with_extension("json");

        let mut state = save::backend::new_json_save_state(false);

        if let Err(err) = state.read_file(&config_json_path) {
            log::error!(log::channel!("config"),
                        "Failed to read config file from path {config_json_path:?}: {err}");
            return T::default();
        }

        match state.load_new_instance::<T>() {
            Ok(configs) => configs,
            Err(err) => {
                log::error!(log::channel!("config"), "Failed to deserialize config file from path {config_json_path:?}: {err}");
                T::default()
            }
        }
    }
}

// ----------------------------------------------
// Macro: configurations
// ----------------------------------------------

#[macro_export]
macro_rules! configurations {
    ($configs_singleton:ident, $configs_type:ty, $configs_path:literal) => {
        $crate::singleton_late_init! { $configs_singleton, $configs_type }
        impl $crate::engine::config::Configs for $configs_type {
            fn draw_debug_ui(&'static self, ui_sys: &$crate::imgui_ui::UiSystem) {
                self.draw_debug_ui_with_header(stringify!($configs_type), ui_sys);
            }
        }
        impl $configs_type {
            pub fn load() -> &'static $configs_type {
                use $crate::engine::config::Configs;
                <$configs_type>::initialize(<$configs_type>::load_file($configs_path));
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
    pub fullscreen: bool,
    pub resizable_window: bool,
    pub confine_cursor_to_window: bool,

    // Graphics:
    pub use_packed_texture_atlas: bool,
    #[debug_ui(nested)]
    pub texture_settings: TextureSettings,

    // Debug Grid:
    pub grid_color: Color,
    pub grid_line_thickness: f32,

    // Debug Log:
    pub log_level: log::Level,
}

impl Default for EngineConfigs {
    fn default() -> Self {
        Self { // Window/Rendering:
               window_title: "Heritage Builder".into(),
               window_size: Size::new(1024, 768),
               window_background_color: rendering::MAP_BACKGROUND_COLOR,
               fullscreen: false,
               resizable_window: false,
               confine_cursor_to_window: true,

               // Graphics:
               use_packed_texture_atlas: false,
               texture_settings: TextureSettings::default(),

               // Debug Grid:
               grid_color: rendering::DEFAULT_GRID_COLOR,
               grid_line_thickness: 1.0,

               // Debug Log:
               log_level: log::Level::Verbose }
    }
}
