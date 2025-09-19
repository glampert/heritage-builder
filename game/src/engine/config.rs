use std::path::Path;
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use proc_macros::DrawDebugUi;

use crate::{
    log,
    imgui_ui::UiSystem,
    save::{self, *},
    tile::rendering,
    utils::{Color, Size}
};

// ----------------------------------------------
// Configs
// ----------------------------------------------

pub const CONFIGS_DIR_PATH: &str = "assets/configs";

pub trait Configs {
    fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
    }

    fn post_load(&mut self) {
    }

    // Saves current configs to file.
    fn save_file(&self, config_file_name: &str) -> bool
        where Self: Configs + Sized + Serialize
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path = Path::new(CONFIGS_DIR_PATH)
            .join(config_file_name)
            .with_extension("json");

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = std::fs::create_dir_all(CONFIGS_DIR_PATH);

        let mut state = save::backend::new_json_save_state(true);

        if let Err(err) = state.save(self) {
            log::error!(log::channel!("config"), "Failed to save config file {config_json_path:?}: {err}");
            return false;
        }

        if let Err(err) = state.write_file(&config_json_path) {
            log::error!(log::channel!("config"), "Failed to write config file {config_json_path:?}: {err}");
            return false;
        }

        true
    }

    // Either succeeds loading the config file or returns a default config.
    fn load_file<T>(config_file_name: &str) -> T
        where T: Configs + Sized + Default + DeserializeOwned
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path = Path::new(CONFIGS_DIR_PATH)
            .join(config_file_name)
            .with_extension("json");

        let mut state = save::backend::new_json_save_state(false);

        if let Err(err) = state.read_file(&config_json_path) {
            log::error!(log::channel!("config"), "Failed to read config file from path {config_json_path:?}: {err}");
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
            fn draw_debug_ui(&self, ui_sys: &$crate::imgui_ui::UiSystem) {
                self.draw_debug_ui_with_header(stringify!($configs_type), ui_sys);
            }
        }
        impl $configs_type {
            pub fn load() -> &'static $configs_type {
                use $crate::engine::config::Configs;
                <$configs_type>::initialize(<$configs_type>::load_file($configs_path));
                let instance = <$configs_type>::get_mut();
                instance.post_load();
                instance
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
    // Window/Rendering:
    pub window_title: String,
    pub window_size: Size,
    pub window_background_color: Color,
    pub fullscreen: bool,
    pub confine_cursor_to_window: bool,

    // Debug Grid:
    pub grid_color: Color,
    pub grid_line_thickness: f32,

    // Debug Log:
    pub log_level: log::Level,
    pub log_viewer_start_open: bool,
    pub log_viewer_max_lines: usize,
}

impl Default for EngineConfigs {
    fn default() -> Self {
        Self {
            // Window/Rendering:
            window_title: "CitySim".into(),
            window_size: Size::new(1024, 768),
            window_background_color: rendering::MAP_BACKGROUND_COLOR,
            fullscreen: false,
            confine_cursor_to_window: true,

            // Debug Grid:
            grid_color: rendering::DEFAULT_GRID_COLOR,
            grid_line_thickness: 1.0,

            // Debug Log:
            log_level: log::Level::Verbose,
            log_viewer_start_open: false,
            log_viewer_max_lines: 32,
        }
    }
}
