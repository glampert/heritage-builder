use std::{path::Path, fs, io};
use enum_dispatch::enum_dispatch;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    utils::mem,
    tile::{TileMap, sets::TileSets},
    game::unit::config::UnitConfigs,
    game::building::config::BuildingConfigs
};

// ----------------------------------------------
// Save / Load Traits
// ----------------------------------------------

pub trait Save {
    fn pre_save(&mut self) {
    }

    fn save(&self, _state: &mut SaveStateImpl) -> SaveResult {
        Ok(())
    }

    fn post_save(&mut self) {
    }
}

pub trait Load<'tile_sets, 'config> {
    fn pre_load(&mut self) {
    }

    fn load(&mut self, _state: &SaveStateImpl) -> LoadResult {
        Ok(())
    }

    fn post_load(&mut self, _context: &PostLoadContext<'tile_sets, 'config>) {
    }
}

// ----------------------------------------------
// SaveState Helpers / PostLoadContext
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

pub struct PostLoadContext<'tile_sets, 'config> {
    pub tile_map: mem::RawPtr<TileMap<'tile_sets>>,
    pub tile_sets: &'tile_sets TileSets,
    pub unit_configs: &'config UnitConfigs,
    pub building_configs: &'config BuildingConfigs,
}

impl<'tile_sets, 'config> PostLoadContext<'tile_sets, 'config> {
    #[inline]
    pub fn new(tile_map: &TileMap<'tile_sets>,
               tile_sets: &'tile_sets TileSets,
               unit_configs: &'config UnitConfigs,
               building_configs: &'config BuildingConfigs) -> Self {
        Self {
            tile_map: mem::RawPtr::from_ref(tile_map),
            tile_sets,
            unit_configs,
            building_configs,
        }
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
        Self {
            pretty: pretty_print,
            buffer: String::new(),
        }
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
            Err(err)  => return Err(err.to_string()),
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
        self.buffer = fs::read_to_string(path)?;
        Ok(())
    }

    fn write_file<P>(&self, path: P) -> io::Result<()>
        where P: AsRef<Path>
    {
        fs::write(path, &self.buffer)
    }
}

#[inline]
pub fn new_json_save_state(pretty_print: bool) -> SaveStateImpl {
    SaveStateImpl::from(JsonSaveState::new(pretty_print))
}
}
