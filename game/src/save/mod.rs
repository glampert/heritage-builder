use std::{io, path::Path};
use enum_dispatch::enum_dispatch;
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    engine::Engine,
    tile::TileMap,
    utils::{mem::{self, RawPtr, RcMut}, file_sys},
    game::sim::{Simulation, RandomGenerator},
};

pub mod storage;

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
// SaveState Helpers / Pre-PostLoadContext
// ----------------------------------------------

pub type SaveResult = Result<(), String>;
pub type LoadResult = Result<(), String>;

#[enum_dispatch(SaveStateImpl)]
pub trait SaveState {
    fn save<T>(&mut self, instance: &T) -> SaveResult
        where T: Serialize;

    fn load<T>(&self, instance: &mut T) -> LoadResult
        where T: DeserializeOwned;

    fn load_new_instance<T>(&self) -> Result<T, String>
        where T: DeserializeOwned;

    fn read_file<P>(&mut self, path: P) -> io::Result<()>
        where P: AsRef<Path>;

    fn write_file<P>(&self, path: P) -> io::Result<()>
        where P: AsRef<Path>;
}

#[enum_dispatch]
pub enum SaveStateImpl {
    Json(backend::JsonSaveState),
}

pub struct PreLoadContext<'game> {
    engine: &'game mut dyn Engine,
}

impl<'game> PreLoadContext<'game> {
    #[inline]
    pub fn new(engine: &'game mut dyn Engine) -> Self {
        Self { engine }
    }

    #[inline]
    pub fn engine(&self) -> &dyn Engine {
        self.engine
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut dyn Engine {
        self.engine
    }
}

pub struct PostLoadContext<'game> {
    rng: RawPtr<RandomGenerator>,
    engine: &'game mut dyn Engine,
    tile_map: RcMut<TileMap>,
}

impl<'game> PostLoadContext<'game> {
    #[inline]
    pub fn new(engine: &'game mut dyn Engine, sim: &Simulation, tile_map: RcMut<TileMap>) -> Self {
        Self {
            rng: RawPtr::from_ref(mem::mut_ref_cast(sim).rng_mut()),
            engine,
            tile_map,
        }
    }

    #[inline]
    pub fn rng_mut(&mut self) -> &mut RandomGenerator {
        &mut self.rng
    }

    #[inline]
    pub fn engine(&self) -> &dyn Engine {
        self.engine
    }

    #[inline]
    pub fn engine_mut(&mut self) -> &mut dyn Engine {
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
}

// ----------------------------------------------
// SaveState Implementations
// ----------------------------------------------

pub mod backend {
    use super::*;

    // ----------------------------------------------
    // JsonSaveState
    // ----------------------------------------------

    pub struct JsonSaveState {
        pretty: bool,
        buffer: String,
    }

    impl JsonSaveState {
        pub fn new(pretty_print: bool) -> Self {
            Self { pretty: pretty_print, buffer: String::new() }
        }
    }

    impl SaveState for JsonSaveState {
        fn save<T>(&mut self, instance: &T) -> SaveResult
            where T: Serialize
        {
            let result = {
                if self.pretty {
                    serde_json::to_string_pretty(instance)
                } else {
                    serde_json::to_string(instance)
                }
            };

            let json = match result {
                Ok(json) => json,
                Err(err) => return Err(err.to_string()),
            };

            self.buffer = json;
            Ok(())
        }

        fn load<T>(&self, instance: &mut T) -> LoadResult
            where T: DeserializeOwned
        {
            // Load in place:
            *instance = self.load_new_instance()?;
            Ok(())
        }

        fn load_new_instance<T>(&self) -> Result<T, String>
            where T: DeserializeOwned
        {
            if self.buffer.is_empty() {
                return Err("JsonSaveState has no state to load!".into());
            }

            match serde_json::from_str::<T>(&self.buffer) {
                Ok(instance) => Ok(instance),
                Err(err) => Err(err.to_string()),
            }
        }

        fn read_file<P>(&mut self, path: P) -> io::Result<()>
            where P: AsRef<Path>
        {
            self.buffer = file_sys::load_string(path)?;
            Ok(())
        }

        fn write_file<P>(&self, path: P) -> io::Result<()>
            where P: AsRef<Path>
        {
            file_sys::write_file(path, &self.buffer)
        }
    }

    #[inline]
    pub fn new_json_save_state(pretty_print: bool) -> SaveStateImpl {
        SaveStateImpl::from(JsonSaveState::new(pretty_print))
    }
}
