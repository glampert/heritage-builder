// Game-level save/load traits and contexts.
//
// These depend on game types (GameConfigs, TileMap, RandomGenerator)
// and are separate from the engine-level save infrastructure (SaveState, JsonSaveState).

use common::mem::RcMut;
use engine::{
    Engine,
    save::{LoadResult, SaveResult, SaveStateImpl},
};

use crate::{
    tile::TileMap,
    config::GameConfigs,
    sim::{RandomGenerator, SimCmds},
};

// ----------------------------------------------
// Save / Load Traits
// ----------------------------------------------

pub trait Save {
    fn pre_save(&mut self, _context: &mut PreSaveContext) {}
    fn save(&self, _state: &mut SaveStateImpl) -> SaveResult { Ok(()) }
    fn post_save(&mut self, _context: &mut PostSaveContext) {}
}

pub trait Load {
    fn pre_load(&mut self, _context: &mut PreLoadContext) {}
    fn load(&mut self, _state: &SaveStateImpl) -> LoadResult { Ok(()) }
    fn post_load(&mut self, _context: &mut PostLoadContext) {}
}

// ----------------------------------------------
// PreSaveContext
// ----------------------------------------------

pub struct PreSaveContext {
    cmds: RcMut<SimCmds>,
}

impl PreSaveContext {
    #[inline]
    pub fn new(cmds: RcMut<SimCmds>) -> Self {
        Self { cmds }
    }

    #[inline]
    pub fn cmds_mut(&mut self) -> &mut SimCmds {
        &mut self.cmds
    }
}

// ----------------------------------------------
// PostSaveContext
// ----------------------------------------------

pub struct PostSaveContext {
}

impl PostSaveContext {
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
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
    pub fn new(
        engine: &'game mut Engine,
        configs: &'static GameConfigs,
        rng: RcMut<RandomGenerator>,
        tile_map: RcMut<TileMap>,
    ) -> Self {
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
