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

// TODO: For now these are all the same until we have more unit sprites to work with.
pub const UNIT_PED: UnitConfigKey = UnitConfigKey::from_str("ped");
pub const UNIT_RUNNER: UnitConfigKey = UnitConfigKey::from_str("ped");

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

// ----------------------------------------------
// UnitConfigs
// ----------------------------------------------

pub struct UnitConfigs {
    // TODO: Temporary. Should be loaded from a file eventually.
    pub ped_config: UnitConfig,
}

impl UnitConfigs {
    pub fn load() -> Self {
        Self {
            ped_config: UnitConfig {
                name: "Ped".to_string(),
                tile_def_name: "ped".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("ped"),
            },
        }
    }

    pub fn find_config_by_name(&self, tile_name: &str) -> &UnitConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_name))
    }

    pub fn find_config_by_hash(&self, _tile_name_hash: StringHash) -> &UnitConfig {
        &self.ped_config
    }
}
