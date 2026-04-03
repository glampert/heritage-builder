// Game-level save/load traits and contexts.
//
// These depend on game types (GameConfigs, TileMap, RandomGenerator)
// and are separate from the engine-level save infrastructure (SaveState, JsonSaveState).

use super::{
    config::GameConfigs,
    sim::RandomGenerator,
};
use crate::{
    engine::Engine,
    tile::TileMap,
    utils::mem::RcMut,
    save::{SaveStateImpl, SaveResult, LoadResult},
};

// ----------------------------------------------
// Save / Load Traits
// ----------------------------------------------

pub trait Save {
    fn pre_save(&mut self) {}
    fn save(&self, _state: &mut SaveStateImpl) -> SaveResult { Ok(()) }
    fn post_save(&mut self) {}
}

pub trait Load {
    fn pre_load(&mut self, _context: &mut PreLoadContext) {}
    fn load(&mut self, _state: &SaveStateImpl) -> LoadResult { Ok(()) }
    fn post_load(&mut self, _context: &mut PostLoadContext) {}
}

// ----------------------------------------------
// PreLoadContext
// ----------------------------------------------

pub struct PreLoadContext<'game> {
    engine: &'game mut Engine,
}

impl<'game> PreLoadContext<'game> {
    #[inline]
    pub fn new(engine: &'game mut Engine) -> Self {
        Self { engine }
    }

    #[inline]
    pub fn engine(&self) -> &Engine {
        self.engine
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut Engine {
        self.engine
    }
}

// ----------------------------------------------
// PostLoadContext
// ----------------------------------------------

pub struct PostLoadContext<'game> {
    engine: &'game mut Engine,
    configs: &'static GameConfigs,
    rng: RcMut<RandomGenerator>,
    tile_map: RcMut<TileMap>,
}

impl<'game> PostLoadContext<'game> {
    #[inline]
    pub fn new(engine: &'game mut Engine,
               configs: &'static GameConfigs,
               rng: RcMut<RandomGenerator>,
               tile_map: RcMut<TileMap>) -> Self
    {
        Self { engine, configs, rng, tile_map }
    }

    #[inline]
    pub fn rng_mut(&mut self) -> &mut RandomGenerator {
        &mut self.rng
    }

    #[inline]
    pub fn engine(&self) -> &Engine {
        self.engine
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut Engine {
        self.engine
    }

    #[inline]
    pub fn tile_map(&self) -> &TileMap {
        &self.tile_map
    }

    #[inline]
    pub fn tile_map_mut(&mut self) -> &mut TileMap {
        &mut self.tile_map
    }

    #[inline]
    pub fn tile_map_rc(&self) -> RcMut<TileMap> {
        self.tile_map.clone()
    }

    #[inline]
    pub fn configs(&self) -> &'static GameConfigs {
        self.configs
    }

    #[inline]
    pub fn configs_and_engine(&mut self) -> (&'static GameConfigs, &mut Engine) {
        (self.configs, self.engine)
    }
}
