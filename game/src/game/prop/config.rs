use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use crate::{
    log,
    configurations,
    ui::UiSystem,
    engine::time::Seconds,
    game::sim::resources::ResourceKind,
    utils::hash::{self, PreHashedKeyMap, StringHash},
};

// ----------------------------------------------
// PropConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
pub struct PropConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    #[serde(default)]
    pub harvestable_resource: ResourceKind,

    #[serde(default)]
    pub harvestable_amount: u32,

    // How long it takes for the harvestable resource to respawn after depletion.
    #[serde(default)]
    pub respawn_time_secs: Seconds,
}

impl Default for PropConfig {
    #[inline]
    fn default() -> Self {
        Self { name: "Tree".into(),
               tile_def_name: "tree".into(),
               tile_def_name_hash: hash::fnv1a_from_str("tree"),
               harvestable_resource: ResourceKind::Wood,
               harvestable_amount: 20,
               respawn_time_secs: 60.0 }
    }
}

impl PropConfig {
    #[inline]
    pub fn key_hash(&self) -> StringHash {
        self.tile_def_name_hash
    }

    fn post_load(&mut self, index: usize) -> bool {
        // Must have a prop name.
        if self.name.is_empty() {
            log::error!(log::channel!("config"), "PropConfig [{index}]: Invalid empty name!");
            return false;
        }

        // Must have a tile def name.
        if self.tile_def_name.is_empty() {
            log::error!(log::channel!("config"),
                        "PropConfig '{}': Invalid empty TileDef name! Index: [{index}]",
                        self.name);
            return false;
        }

        self.tile_def_name_hash = hash::fnv1a_from_str(&self.tile_def_name);
        debug_assert!(self.tile_def_name_hash != hash::NULL_HASH);

        true
    }
}

// ----------------------------------------------
// PropConfigs
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct PropConfigs {
    // Serialized data:
    configs: Vec<PropConfig>,

    // Runtime lookup:
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,

    // Default fallback config:
    #[serde(skip)]
    default_prop_config: PropConfig,
}

impl PropConfigs {
    pub fn find_config_by_name(&'static self, tile_def_name: &str) -> &'static PropConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_def_name), tile_def_name)
    }

    pub fn find_config_by_hash(&'static self,
                               tile_def_name_hash: StringHash,
                               tile_def_name: &str)
                               -> &'static PropConfig {
        debug_assert!(tile_def_name_hash != hash::NULL_HASH);

        match self.mapping.get(&tile_def_name_hash) {
            Some(index) => &self.configs[*index],
            None => {
                log::error!(log::channel!("config"), "Can't find PropConfig '{tile_def_name}'!");
                &self.default_prop_config
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
                log::error!(log::channel!("config"), "PropConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
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
// PropConfigs Global Singleton
// ----------------------------------------------

configurations! { PROP_CONFIGS_SINGLETON, PropConfigs, "props/configs" }
