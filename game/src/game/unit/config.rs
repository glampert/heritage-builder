use crate::{
    tile::map::Tile,
    utils::hash::{self, StringHash}
};

use super::{
    Unit
};

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
    dummy: UnitConfig,
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

pub fn instantiate<'config>(tile: &Tile, configs: &'config UnitConfigs) -> Option<Unit<'config>> {
    let tile_name_hash = tile.tile_def().hash;
    let config = configs.find_config_by_hash(tile_name_hash);
    Some(Unit::new("Ped", tile.base_cell(), config))
}
