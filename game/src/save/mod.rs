use crate::{
    tile::{TileMap, sets::TileSets},
    game::unit::config::UnitConfigs,
    game::building::config::BuildingConfigs
};

// ----------------------------------------------
// PostLoadContext
// ----------------------------------------------

pub struct PostLoadContext<'loader> {
    pub tile_map: &'loader TileMap<'loader>,
    pub tile_sets: &'loader TileSets,
    pub unit_configs: &'loader UnitConfigs,
    pub building_configs: &'loader BuildingConfigs,
}

// ----------------------------------------------
// PostLoad
// ----------------------------------------------

pub trait PostLoad<'loader> {
    fn post_load(&mut self, context: &PostLoadContext<'loader>);
}
