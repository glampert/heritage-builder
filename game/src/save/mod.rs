use crate::{
    tile::TileMap,
    game::unit::config::UnitConfigs,
    game::building::config::BuildingConfigs
};

// ----------------------------------------------
// PostLoadContext
// ----------------------------------------------

pub struct PostLoadContext<'world> {
    pub tile_map: &'world TileMap<'world>,
    pub unit_configs: &'world UnitConfigs,
    pub building_configs: &'world BuildingConfigs,
}

// ----------------------------------------------
// PostLoad
// ----------------------------------------------

pub trait PostLoad<'world> {
    fn post_load(&mut self, context: &PostLoadContext<'world>);
}
