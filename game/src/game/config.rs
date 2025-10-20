use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::{
    configurations,
    engine::{config::EngineConfigs, time::Seconds},
    tile::camera::*,
    utils::Size,
};

// ----------------------------------------------
// GameConfigs
// ----------------------------------------------

#[derive(Default, DrawDebugUi, Serialize, Deserialize)]
#[serde(default)] // Missing fields in the config file get defaults from GameConfigs::default().
pub struct GameConfigs {
    // Low-level Engine:
    #[debug_ui(nested)]
    pub engine: EngineConfigs,

    // Save Games:
    #[debug_ui(nested)]
    pub save: SaveGameConfigs,

    // Camera:
    #[debug_ui(nested)]
    pub camera: CameraConfigs,

    // Simulation/World:
    #[debug_ui(nested)]
    pub sim: SimConfigs,

    // Debug:
    #[debug_ui(nested)]
    pub debug: DebugConfigs,
}

// ----------------------------------------------
// Sub Config Categories
// ----------------------------------------------

#[derive(Default, Display, Serialize, Deserialize)]
pub enum LoadMapSetting {
    #[default]
    None,
    EmptyMap {
        size_in_cells: Size,
        terrain_tile_category: String,
        terrain_tile_name: String,
    },
    Preset {
        preset_number: usize,
    },
    SaveGame {
        save_file_path: String,
    },
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveGameConfigs {
    pub load_map_setting: LoadMapSetting,
    pub enable_autosave: bool,
    pub autosave_frequency_secs: Seconds,
}

impl Default for SaveGameConfigs {
    fn default() -> Self {
        Self { load_map_setting: LoadMapSetting::default(),
               enable_autosave: true,
               autosave_frequency_secs: 60.0 }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfigs {
    pub zoom: f32,
    pub offset: CameraOffset,
}

impl Default for CameraConfigs {
    fn default() -> Self {
        Self { zoom: CameraZoom::MIN, offset: CameraOffset::Center }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfigs {
    // Simulation:
    pub random_seed: u64,
    pub update_frequency_secs: Seconds,
    pub starting_gold_units: u32,

    // Workers/Population:
    pub workers_search_radius: i32,
    pub workers_update_frequency_secs: Seconds,

    // Game Systems:
    pub settlers_spawn_frequency_secs: Seconds,
    pub population_per_settler_unit: u32,
}

impl Default for SimConfigs {
    fn default() -> Self {
        Self { // Simulation:
               random_seed: 0xCAFE1CAFE2CAFE3A,
               update_frequency_secs: 0.5,
               starting_gold_units: 0,
               // Workers/Population:
               workers_search_radius: 20,
               workers_update_frequency_secs: 20.0,
               // Game Systems:
               settlers_spawn_frequency_secs: 20.0,
               population_per_settler_unit: 1 }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfigs {
    pub show_popups: bool,
    pub tile_palette_open: bool,
    pub enable_tile_inspector: bool,
}

impl Default for DebugConfigs {
    fn default() -> Self {
        Self { show_popups: true,
               tile_palette_open: true,
               enable_tile_inspector: true }
    }
}

// ----------------------------------------------
// GameConfigs Global Singleton
// ----------------------------------------------

configurations! { GAME_CONFIGS_SINGLETON, GameConfigs, "game/configs" }
