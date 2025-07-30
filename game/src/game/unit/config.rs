use crate::{
    tile::map::Tile,
    utils::hash::{
        self,
        StringHash,
        StrHashPair
    }
};

use super::{
    Unit
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

pub struct UnitConfig {
    // TODO
}

// ----------------------------------------------
// UnitConfigs
// ----------------------------------------------

pub struct UnitConfigs {
    // TODO: Temporary. Should be loaded from a file.
    pub dummy: UnitConfig,
}

impl UnitConfigs {
    pub fn load() -> Self {
        Self {
            dummy: UnitConfig {}
        }
    }

    pub fn find_config_by_name(&self, tile_name: &str) -> &UnitConfig {
        self.find_config_by_hash(hash::fnv1a_from_str(tile_name))
    }

    pub fn find_config_by_hash(&self, _tile_name_hash: StringHash) -> &UnitConfig {
        &self.dummy
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

pub fn instantiate<'config>(tile: &mut Tile, configs: &'config UnitConfigs) -> Option<Unit<'config>> {
    let tile_name_hash = tile.tile_def().hash;
    let config = configs.find_config_by_hash(tile_name_hash);
    Some(Unit::new("Ped", tile, config))
}
