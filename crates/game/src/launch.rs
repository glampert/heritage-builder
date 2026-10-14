// Command-line options understood by the game. The engine parses the command line
// (see engine::cli); this module reads the game's options from it once at startup.

use std::{path::PathBuf, sync::OnceLock};

use engine::cli::{self, CliOption, CommandLine};

use crate::{
    config::{GameConfigs, LoadMapSetting},
    debug::preset_maps,
    tile::sets::TERRAIN_LAND_CATEGORY,
};

pub const GAME_CLI_OPTIONS: &[CliOption] = &[
    CliOption {
        name: "seed",
        value_name: Some("N"),
        help: "Random seed for new games, instead of sim.random_seed from configs.json.",
    },
    CliOption {
        name: "map",
        value_name: Some("preset:N|empty:WxH"),
        help: "Start in-game on a debug preset map or on an empty map of WxH cells.",
    },
    CliOption { name: "load-save", value_name: Some("NAME"), help: "Start in-game by loading saves/NAME.json." },
    CliOption {
        name: "skip-home-menu",
        value_name: None,
        help: "Start in-game on the map set in configs.json instead of the home menu.",
    },
    CliOption {
        name: "smoke-test-saves",
        value_name: None,
        help: "Load every save in saves/, tick each for 10 seconds, then exit. Panics on a failed load.",
    },
];

// Terrain for `--map empty:WxH` when configs.json doesn't set up an empty map.
const DEFAULT_EMPTY_MAP_TERRAIN_TILE: &str = "grass";

// ----------------------------------------------
// LaunchOptions
// ----------------------------------------------

#[derive(Default)]
pub struct LaunchOptions {
    pub seed: Option<u64>,                 // --seed
    pub start_map: Option<LoadMapSetting>, // --map or --load-save
    pub skip_home_menu: bool,              // --skip-home-menu
    pub smoke_test_saves: bool,            // --smoke-test-saves
}

impl LaunchOptions {
    // Reads the game's options from the command line. A value the game can't use
    // ends the process with an error (see engine::cli::fail).
    pub fn from_command_line(command_line: &CommandLine, configs: &GameConfigs) -> Self {
        let seed = command_line.parse_value::<u64>("seed").unwrap_or_else(|err| cli::fail(&err));

        let map = command_line
            .value("map")
            .map(|value| parse_map(value, configs).unwrap_or_else(|err| cli::fail(&format!("--map: {err}"))));

        let load_save =
            command_line.value("load-save").map(|name| LoadMapSetting::SaveGame { save_file: PathBuf::from(name) });

        if map.is_some() && load_save.is_some() {
            cli::fail("Use either --map or --load-save, not both.");
        }

        Self {
            seed,
            start_map: map.or(load_save),
            skip_home_menu: command_line.has("skip-home-menu"),
            smoke_test_saves: command_line.has("smoke-test-saves"),
        }
    }

    // Seed for a new game's random generator.
    #[inline]
    pub fn random_seed(&self, configs: &GameConfigs) -> u64 {
        self.seed.unwrap_or(configs.sim.random_seed)
    }
}

fn parse_map(value: &str, configs: &GameConfigs) -> Result<LoadMapSetting, String> {
    if let Some(number) = value.strip_prefix("preset:") {
        let preset_count = preset_maps::preset_tile_maps_list().len();

        let preset_number = number
            .parse::<usize>()
            .ok()
            .filter(|preset_number| *preset_number < preset_count)
            .ok_or_else(|| format!("Expected a preset number from 0 to {}, got '{number}'.", preset_count - 1))?;

        return Ok(LoadMapSetting::Preset { preset_number });
    }

    if let Some(size) = value.strip_prefix("empty:") {
        let size_in_cells = cli::parse_size(size)?;

        // Use the same terrain as the empty map in configs.json, if it sets one up.
        let (terrain_tile_category, terrain_tile_name) = match &configs.save.load_map_setting {
            LoadMapSetting::EmptyMap { terrain_tile_category, terrain_tile_name, .. } => {
                (terrain_tile_category.clone(), terrain_tile_name.clone())
            }
            _ => (TERRAIN_LAND_CATEGORY.string.to_string(), DEFAULT_EMPTY_MAP_TERRAIN_TILE.to_string()),
        };

        return Ok(LoadMapSetting::EmptyMap { size_in_cells, terrain_tile_category, terrain_tile_name });
    }

    Err(format!("Expected preset:N or empty:WxH, got '{value}'."))
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

static LAUNCH_OPTIONS: OnceLock<LaunchOptions> = OnceLock::new();

// Reads the options from the engine's parsed command line. Called once by GameLoop::start.
pub fn initialize(configs: &GameConfigs) -> &'static LaunchOptions {
    if LAUNCH_OPTIONS.set(LaunchOptions::from_command_line(cli::get(), configs)).is_err() {
        panic!("Launch options already initialized!");
    }
    get()
}

// Options for this run. All defaults if initialize() hasn't run (in tests, for instance).
pub fn get() -> &'static LaunchOptions {
    static DEFAULT: LaunchOptions =
        LaunchOptions { seed: None, start_map: None, skip_home_menu: false, smoke_test_saves: false };
    LAUNCH_OPTIONS.get().unwrap_or(&DEFAULT)
}
