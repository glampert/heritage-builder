use proc_macros::DrawDebugUi;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    log,
    save::{self, *},
    ui::UiSystem,
    utils::{Color, Size},
    sound::SoundGlobalSettings,
    render::{RenderApi, texture::TextureSettings},
    file_sys::{self, paths::{self, PathRef, AssetPath}},
    app::{ApplicationApi, ApplicationWindowMode, ApplicationContentScale},
};

// ----------------------------------------------
// Configs
// ----------------------------------------------

pub fn configs_path() -> AssetPath {
    paths::assets_path().join("configs")
}

pub trait Configs {
    fn draw_debug_ui(&'static self, _ui_sys: &UiSystem) {}
    fn post_load(&'static mut self) {}

    // Saves current configs to file.
    fn save_file(&'static self, config_file_name: PathRef) -> bool
        where Self: Configs + Sized + Serialize
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path = configs_path()
            .join(config_file_name)
            .with_extension("json");

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = file_sys::create_path(configs_path());

        let mut state = save::new_json_save_state(true);

        if let Err(err) = state.save(self) {
            log::error!(log::channel!("config"),
                        "Failed to save config file {config_json_path}: {err}");
            return false;
        }

        if let Err(err) = state.write_file(&config_json_path) {
            log::error!(log::channel!("config"),
                        "Failed to write config file {config_json_path}: {err}");
            return false;
        }

        true
    }

    // Either succeeds loading the config file or returns a default config.
    fn load_file<T>(config_file_name: PathRef) -> T
        where T: Configs + Sized + Default + DeserializeOwned
    {
        debug_assert!(!config_file_name.is_empty());

        let config_json_path = configs_path()
            .join(config_file_name)
            .with_extension("json");

        let mut state = save::new_json_save_state(false);

        if let Err(err) = state.read_file(&config_json_path) {
            log::error!(log::channel!("config"),
                        "Failed to read config file from path {config_json_path}: {err}");
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
}

// ----------------------------------------------
// Macro: configurations
// ----------------------------------------------

macro_rules! configurations {
    ($configs_singleton:ident, $configs_type:ty, $configs_path:literal) => {
        ::common::singleton_late_init! { $configs_singleton, $configs_type }
        impl $crate::engine::config::Configs for $configs_type {
            fn draw_debug_ui(&'static self, ui_sys: &$crate::ui::UiSystem) {
                self.draw_debug_ui_with_header(stringify!($configs_type), ui_sys);
            }
        }
        impl $configs_type {
            pub fn load() -> &'static $configs_type {
                use $crate::engine::config::Configs;
                <$configs_type>::initialize(<$configs_type>::load_file($crate::file_sys::paths::PathRef::from_str($configs_path)));
                <$configs_type>::get_mut().post_load();
                <$configs_type>::get()
            }
            pub fn save() -> bool {
                use $crate::engine::config::Configs;
                <$configs_type>::get().save_file($crate::file_sys::paths::PathRef::from_str($configs_path))
            }
        }
    };
}

pub(crate) use configurations;

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
    pub app_api: ApplicationApi,

    // Graphics:
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
        }
    }
}
