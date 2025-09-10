use crate::{
    tile::TileMap
};

use super::{
    unit::config::UnitConfigs,
    building::config::BuildingConfigs
};

// ----------------------------------------------
// PostLoadContext
// ----------------------------------------------

pub struct PostLoadContext<'config, 'tile_sets> {
    pub building_configs: &'config BuildingConfigs,
    pub unit_configs: &'config UnitConfigs,
    pub tile_map: &'tile_sets TileMap<'tile_sets>,
}
