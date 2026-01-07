use proc_macros::DrawDebugUi;
use strum_macros::Display;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

use crate::{
    log,
    configurations,
    ui::UiSystem,
    pathfind::NodeKind as PathNodeKind,
    utils::hash::{self, PreHashedKeyMap, StringHash},
};

// ----------------------------------------------
// UnitConfigKey
// ----------------------------------------------

#[repr(u64)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, TryFromPrimitive, Serialize, Deserialize)]
pub enum UnitConfigKey {
    #[default]
    Peasant      = hash::fnv1a_from_str("peasant"),
    Runner       = hash::fnv1a_from_str("runner"),
    Settler      = hash::fnv1a_from_str("settler"),
    Vendor       = hash::fnv1a_from_str("vendor"),
    TaxCollector = hash::fnv1a_from_str("tax_collector"),
    WaterCarrier = hash::fnv1a_from_str("water_carrier"),
    Guard        = hash::fnv1a_from_str("guard"),
    Teacher      = hash::fnv1a_from_str("teacher"),
    Actor        = hash::fnv1a_from_str("actor"),
    Monk         = hash::fnv1a_from_str("monk"),
    Medic        = hash::fnv1a_from_str("medic"),
    Dog          = hash::fnv1a_from_str("dog"),
    Bird         = hash::fnv1a_from_str("bird"),
    Buffalo      = hash::fnv1a_from_str("buffalo"),
}

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
        Self { name: "Peasant".into(),
               tile_def_name: "peasant".into(),
               tile_def_name_hash: UnitConfigKey::Peasant as StringHash,
               traversable_node_kinds: PathNodeKind::default(),
               movement_speed: 1.66 }
    }
}

impl UnitConfig {
    #[inline]
    pub fn is(&self, key: UnitConfigKey) -> bool {
        self.key() == key
    }

    #[inline]
    pub fn key(&self) -> UnitConfigKey {
        UnitConfigKey::try_from_primitive(self.tile_def_name_hash).unwrap()
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
            log::error!(log::channel!("config"),
                        "UnitConfig '{}': Invalid empty TileDef name! Index: [{index}]",
                        self.name);
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
    pub fn find_config_by_key(&'static self, unit_config: UnitConfigKey) -> &'static UnitConfig {
        let tile_def_name = if cfg!(debug_assertions) {
            &unit_config.to_string()
        } else {
            ""
        };
        self.find_config_by_hash(unit_config as StringHash, tile_def_name)
    }

    pub fn find_config_by_name(&'static self, tile_def_name: &str) -> &'static UnitConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_def_name), tile_def_name)
    }

    pub fn find_config_by_hash(&'static self,
                               tile_def_name_hash: StringHash,
                               tile_def_name: &str)
                               -> &'static UnitConfig {
        debug_assert!(tile_def_name_hash != hash::NULL_HASH);

        match self.mapping.get(&tile_def_name_hash) {
            Some(index) => &self.configs[*index],
            None => {
                log::error!(log::channel!("config"), "Can't find UnitConfig '{tile_def_name}'!");
                &self.default_unit_config
            }
        }
    }

    fn post_load(&'static mut self) {
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

    fn draw_debug_ui_with_header(&'static self, _header: &str, ui_sys: &UiSystem) {
        for config in &self.configs {
            config.draw_debug_ui_with_header(&config.name, ui_sys);
        }
    }
}

// ----------------------------------------------
// UnitConfigs Global Singleton
// ----------------------------------------------

configurations! { UNIT_CONFIGS_SINGLETON, UnitConfigs, "units/configs" }
