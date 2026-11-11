use std::path::PathBuf;

use common::{Size, time::Seconds};
use engine::{config::EngineConfigs, runner::RunLoopConfigs};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount};

use crate::{
    camera::*,
    sim::{calendar::CalendarSettings, finance::TaxLevel},
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
        save_file: PathBuf,
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
        Self { load_map_setting: LoadMapSetting::default(), enable_autosave: true, autosave_frequency_secs: 60.0 }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfigs {
    pub zoom: f32,
    pub offset: CameraOffset,

    // For fixed step zoom with CTRL +/= key shortcuts.
    pub fixed_step_zoom_amount: f32,

    // Use fixed step zoom with mouse scroll zoom instead of smooth interpolation.
    pub disable_smooth_mouse_scroll_zoom: bool,

    // Disables mouse scroll zoom altogether.
    pub disable_mouse_scroll_zoom: bool,

    // Disables zooming with keyboard shortcuts.
    pub disable_key_shortcut_zoom: bool,

    // Constrain camera movement to inner map diamond playable area? (debug option).
    pub constrain_to_playable_map_area: bool,

    // Constrain camera movement to map AABB? This is a superset of the playable area. (debug option).
    pub clamp_to_map_bounds: bool,

    // Display map debug bounds and camera debug overlays.
    pub enable_debug_draw: bool,

    // Camera scroll/movement speed in pixels per second.
    pub scroll_speed: f32,

    // In pixels from screen edge.
    pub scroll_margin: f32,
}

impl Default for CameraConfigs {
    fn default() -> Self {
        Self {
            zoom: CameraZoom::DEFAULT,
            offset: CameraOffset::Center,
            fixed_step_zoom_amount: 0.5,
            disable_smooth_mouse_scroll_zoom: false,
            disable_mouse_scroll_zoom: false,
            disable_key_shortcut_zoom: false,
            constrain_to_playable_map_area: true,
            clamp_to_map_bounds: true,
            enable_debug_draw: false,
            scroll_speed: 500.0,
            scroll_margin: 20.0,
        }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfigs {
    // Simulation:
    pub random_seed: u64,
    pub update_frequency_secs: Seconds,
    pub paused_update_frequency_secs: Seconds,
    pub starting_gold_units: u32,
    pub start_paused: bool,

    // Length of one in-game day, in simulation seconds. Lets rates (e.g. house
    // resource consumption) be expressed in intuitive units-per-day.
    pub seconds_per_day: Seconds,

    // Calendar: simulated seconds per month, days per month, and the calendar of games outside
    // a campaign. A campaign mission's start applies its own (see campaign::config).
    pub seconds_per_month: Seconds,
    pub days_per_month: u32,
    #[debug_ui(skip)]
    pub calendar: CalendarSettings,

    // How long construction sites take (docs/plans/construction.md).
    #[debug_ui(nested)]
    pub construction: ConstructionConfigs,

    // Wages, tax levels and debt (docs/plans/finance.md).
    #[debug_ui(nested)]
    pub finance: FinanceConfigs,

    // Fire and collapse risk (docs/plans/city-risks.md).
    #[debug_ui(nested)]
    pub risks: RiskConfigs,

    // Workers/Population:
    pub workers_search_radius: i32,
    pub workers_update_frequency_secs: Seconds,

    // Game Systems:
    pub birds_spawn_frequency: Seconds,
    pub settlers_spawn_frequency_secs: Seconds,
    pub population_per_settler_unit: u32,
}

impl Default for SimConfigs {
    fn default() -> Self {
        Self {
            // Simulation:
            random_seed: 0xCAFE1CAFE2CAFE3A,
            update_frequency_secs: 0.5,
            paused_update_frequency_secs: 0.5,
            starting_gold_units: 0,
            start_paused: false,
            seconds_per_day: 600.0,
            // Calendar:
            seconds_per_month: 60.0,
            days_per_month: 30,
            calendar: CalendarSettings::default(),
            construction: ConstructionConfigs::default(),
            finance: FinanceConfigs::default(),
            risks: RiskConfigs::default(),
            // Workers/Population:
            workers_search_radius: 20,
            workers_update_frequency_secs: 20.0,
            // Game Systems:
            birds_spawn_frequency: 20.0,
            settlers_spawn_frequency_secs: 20.0,
            population_per_settler_unit: 1,
        }
    }
}

// Simulated seconds a construction site takes, by construction tier, before `time_scale`.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstructionConfigs {
    pub decorative_secs: Seconds,
    pub basic_secs: Seconds,
    pub civic_secs: Seconds,
    pub grand_secs: Seconds,

    // The house a settler builds on a vacant lot.
    pub house_build_secs: Seconds,

    // Multiplies every duration: 0 builds everything at once, 2 takes twice as long.
    pub time_scale: f32,
}

impl Default for ConstructionConfigs {
    fn default() -> Self {
        Self {
            decorative_secs: 8.0,
            basic_secs: 15.0,
            civic_secs: 40.0,
            grand_secs: 75.0,
            house_build_secs: 4.0,
            time_scale: 1.0,
        }
    }
}

// The defaults of a new game's finances, and the tax levels' rates (sim::finance).
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct FinanceConfigs {
    // Gold per employed worker per year, paid monthly.
    pub default_wage: u32,

    #[debug_ui(skip)]
    pub default_tax_level: TaxLevel,

    // Percentage of the houses' base tax each level pays, in TaxLevel order: None, Low, Normal,
    // High, Harsh.
    #[debug_ui(skip)]
    pub tax_level_percent: [u32; TaxLevel::COUNT],

    // Months in debt before the debt is overdue (a mission failure from M9).
    pub debt_limit_months: u32,
}

impl Default for FinanceConfigs {
    fn default() -> Self {
        Self {
            default_wage: 6,
            default_tax_level: TaxLevel::Normal,
            tax_level_percent: [0, 50, 100, 150, 200],
            debt_limit_months: 12,
        }
    }
}

// Fire, collapse and disease risk (building::risks). Each building's rates come from its building config.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskConfigs {
    // Whether a new game outside a campaign has risks. A mission's `risks` block decides for it.
    pub enabled: bool,

    // A building that a small or large well reaches builds up fire risk at this fraction of its rate.
    pub near_water_fire_scale: f32,

    // Each building's rates vary by up to this fraction either way, so neighbours built together
    // don't all burn at once.
    pub rate_jitter: f32,

    // Simulated seconds between a building's checks for a well within reach.
    pub water_check_secs: Seconds,

    // How long a building burns before it burns down, in simulated seconds, unless a fire
    // watcher puts the fire out first.
    pub burn_secs: Seconds,

    // Fire risk a burning building adds to each building next to it, per simulated second.
    pub spread_per_sec: f32,

    // Disease (houses only). A house's disease risk builds up at its level's rate times how full
    // it is, faster without water or food, slower with an apothecary in reach; a medic's visit
    // resets it. Every `disease_check_secs` a house with people rolls for an outbreak, at
    // `outbreak_chance_at_max` a month times (risk / 100) squared.
    pub disease_check_secs: Seconds,
    pub outbreak_chance_at_max: f32,

    // Added to the disease rate's needs factor (1) without a well in reach, and without food.
    pub missing_water_scale: f32,
    pub missing_food_scale: f32,

    // An apothecary in reach scales the disease rate and an outbreak's deaths by this.
    pub apothecary_scale: f32,

    // Disease risk a sick house adds to every house within `disease_spread_cells` of it, per
    // simulated second.
    pub disease_spread_per_sec: f32,
    pub disease_spread_cells: i32,

    // Deaths in a sick house: none for `death_grace_secs`, then every `death_interval_secs`
    // round(population x fraction) residents die, at least one. The fraction starts at
    // `death_fraction` and rises by `death_fraction_step` each time, up to `death_fraction_max`.
    pub death_grace_secs: Seconds,
    pub death_interval_secs: Seconds,
    pub death_fraction: f32,
    pub death_fraction_step: f32,
    pub death_fraction_max: f32,
}

impl Default for RiskConfigs {
    fn default() -> Self {
        Self {
            enabled: true,
            near_water_fire_scale: 0.5,
            rate_jitter: 0.25,
            water_check_secs: 10.0,
            burn_secs: 30.0,
            spread_per_sec: 5.0,
            disease_check_secs: 10.0,
            outbreak_chance_at_max: 0.5,
            missing_water_scale: 1.0,
            missing_food_scale: 1.0,
            apothecary_scale: 0.5,
            disease_spread_per_sec: 1.0,
            disease_spread_cells: 2,
            death_grace_secs: 30.0,
            death_interval_secs: 20.0,
            death_fraction: 0.1,
            death_fraction_step: 0.05,
            death_fraction_max: 0.3,
        }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfigs {
    pub skip_home_menu: bool,
    pub start_in_dev_editor_mode: bool,
    pub skip_loading_tile_sets: bool,
    pub show_popups: bool,
    pub enable_dev_tile_inspector: bool,
    pub disable_ambient_music: bool,
    pub disable_ambient_sounds: bool,
}

impl Default for DebugConfigs {
    fn default() -> Self {
        Self {
            skip_home_menu: false,
            start_in_dev_editor_mode: false,
            skip_loading_tile_sets: false,
            show_popups: false,
            enable_dev_tile_inspector: true,
            disable_ambient_music: false,
            disable_ambient_sounds: false,
        }
    }
}

impl RunLoopConfigs for GameConfigs {
    fn engine(&self) -> &EngineConfigs {
        &self.engine
    }

    fn load() -> &'static Self {
        GameConfigs::load()
    }

    fn get() -> &'static Self {
        GameConfigs::get()
    }
}

// ----------------------------------------------
// GameConfigs Global Singleton
// ----------------------------------------------

// Defaults in assets/configs/game/configs.json; the settings dialog's changes go to the user
// configs, saves/user/configs.json (see engine::config).
engine::configurations! { GAME_CONFIGS_SINGLETON, GameConfigs, "game/configs", user_configs: "configs" }
