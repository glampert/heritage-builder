use serde::{Serialize, Deserialize};
use proc_macros::DrawDebugUi;

use crate::{
    log,
    configurations,
    imgui_ui::UiSystem,
    pathfind::{NodeKind as PathNodeKind},
    utils::hash::{
        self,
        StringHash,
        StrHashPair,
        PreHashedKeyMap,
    }
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub type UnitConfigKey = StrHashPair;

pub const UNIT_PED:     UnitConfigKey = UnitConfigKey::from_str("ped");
pub const UNIT_RUNNER:  UnitConfigKey = UnitConfigKey::from_str("runner");
pub const UNIT_PATROL:  UnitConfigKey = UnitConfigKey::from_str("patrol");
pub const UNIT_SETTLER: UnitConfigKey = UnitConfigKey::from_str("settler");

// ----------------------------------------------
// UnitConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
pub struct UnitConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    // Navigation/Pathfind:
    #[serde(default)]
    pub traversable_node_kinds: PathNodeKind,
    pub movement_speed: f32, // in tiles per second.
}

impl Default for UnitConfig {
    #[inline]
    fn default() -> Self {
        Self {
            name: "Ped".into(),
            tile_def_name: UNIT_PED.string.into(),
            tile_def_name_hash: UNIT_PED.hash,
            traversable_node_kinds: PathNodeKind::default(),
            movement_speed: 1.66,
        }
    }
}

impl UnitConfig {
    #[inline]
    pub fn is(&self, key: UnitConfigKey) -> bool {
        self.key_hash() == key.hash
    }

    #[inline]
    pub fn key_hash(&self) -> StringHash {
        self.tile_def_name_hash
    }

    fn post_load(&mut self, index: usize) -> bool {
        // Must have a unit name.
        if self.name.is_empty() {
            log::error!(log::channel!("config"), "UnitConfig [{index}]: Invalid empty name!");
            return false;
        }

        // Must have a tile def name.
        if self.tile_def_name.is_empty() {
            log::error!(log::channel!("config"), "UnitConfig '{}': Invalid empty TileDef name! Index: [{index}]", self.name);
            return false;
        }

        self.tile_def_name_hash = hash::fnv1a_from_str(&self.tile_def_name);
        debug_assert!(self.tile_def_name_hash != hash::NULL_HASH);

        true
    }
}

// ----------------------------------------------
// UnitConfigs
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct UnitConfigs {
    // Serialized data:
    configs: Vec<UnitConfig>,

    // Runtime lookup:
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,

    // Default fallback config:
    #[serde(skip)]
    default_unit_config: UnitConfig,
}

impl UnitConfigs {
    pub fn find_config_by_name(&self, tile_def_name: &str) -> &UnitConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_def_name), tile_def_name)
    }

    pub fn find_config_by_hash(&self, tile_def_name_hash: StringHash, tile_def_name: &str) -> &UnitConfig {
        debug_assert!(tile_def_name_hash != hash::NULL_HASH);

        match self.mapping.get(&tile_def_name_hash) {
            Some(index) => &self.configs[*index],
            None => {
                log::error!(log::channel!("config"), "Can't find UnitConfig '{tile_def_name}'!");
                &self.default_unit_config
            },
        }
    }

    fn post_load(&mut self) {
        for (index, config) in &mut self.configs.iter_mut().enumerate() {
            if !config.post_load(index) {
                // Entries that fail to load will not be visible in the lookup table.
                continue;
            }

            if self.mapping.insert(config.tile_def_name_hash, index).is_some() {
                log::error!(log::channel!("config"), "UnitConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
                            config.name,
                            config.tile_def_name,
                            config.tile_def_name_hash);
            }
        }
    }

    fn draw_debug_ui_with_header(&self, _header: &str, ui_sys: &UiSystem) {
        for config in &self.configs {
            config.draw_debug_ui_with_header(&config.name, ui_sys);
        }
    }
}

// ----------------------------------------------
// UnitConfigs Global Singleton
// ----------------------------------------------

configurations! { UNIT_CONFIGS_SINGLETON, UnitConfigs, "units/configs" }
