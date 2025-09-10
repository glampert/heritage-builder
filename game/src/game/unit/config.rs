use proc_macros::DrawDebugUi;

use crate::{
    imgui_ui::UiSystem,
    utils::hash::{
        self,
        StringHash,
        StrHashPair
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

#[derive(DrawDebugUi)]
pub struct UnitConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    pub tile_def_name_hash: StringHash,

    // TODO
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
}

// ----------------------------------------------
// UnitConfigs
// ----------------------------------------------

pub struct UnitConfigs {
    // TODO: Temporary. These should be loaded from a file eventually.
    ped_config: UnitConfig,
    runner_config: UnitConfig,
    patrol_config: UnitConfig,
    settler_config: UnitConfig,
}

impl UnitConfigs {
    pub fn load() -> Self {
        Self {
            ped_config: UnitConfig {
                name: "Ped".to_string(),
                tile_def_name: "ped".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("ped"),
            },
            runner_config: UnitConfig {
                name: "Runner".to_string(),
                tile_def_name: "runner".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("runner"),
            },
            patrol_config: UnitConfig {
                name: "Patrol".to_string(),
                tile_def_name: "patrol".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("patrol"),
            },
            settler_config: UnitConfig {
                name: "Settler".to_string(),
                tile_def_name: "settler".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("settler"),
            },
        }
    }

    pub fn find_config_by_name(&self, tile_name: &str) -> &UnitConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_name))
    }

    pub fn find_config_by_hash(&self, tile_name_hash: StringHash) -> &UnitConfig {
        if tile_name_hash == hash::fnv1a_from_str("ped") {
            &self.ped_config
        } else if tile_name_hash == hash::fnv1a_from_str("runner") {
            &self.runner_config
        } else if tile_name_hash == hash::fnv1a_from_str("patrol") {
            &self.patrol_config
        } else if tile_name_hash == hash::fnv1a_from_str("settler") {
            &self.settler_config
        } else { panic!("Unknown unit config!") }
    }
}
