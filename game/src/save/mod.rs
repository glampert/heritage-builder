use std::{path::Path, fs, io};
use enum_dispatch::enum_dispatch;

use crate::{
    tile::{TileMap, sets::TileSets},
    game::unit::config::UnitConfigs,
    game::building::config::BuildingConfigs
};

// ----------------------------------------------
// Save / Load Traits
// ----------------------------------------------

pub trait Save {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult;
}

pub trait Load<'tile_map, 'tile_sets, 'config> {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult;
    fn post_load(&mut self, context: &PostLoadContext<'tile_map, 'tile_sets, 'config>);
}

// ----------------------------------------------
// SaveState Helpers / PostLoadContext
// ----------------------------------------------

pub type SaveResult = Result<(), String>;
pub type LoadResult = Result<(), String>;

#[enum_dispatch(SaveStateImpl)]
pub trait SaveState {
    fn save<T>(&mut self, instance: &T) -> SaveResult
        where T: serde::Serialize;

    fn load<'de, T>(&'de self, instance: &mut T) -> LoadResult
        where T: serde::Deserialize<'de>;

    fn write_file<P>(&self, path: P) -> io::Result<()>
        where P: AsRef<Path>;
}

#[enum_dispatch]
pub enum SaveStateImpl {
    Json(backends::JsonSaveState),
}

pub struct PostLoadContext<'tile_map, 'tile_sets, 'config> {
    pub tile_map: Option<&'tile_map TileMap<'tile_sets>>,
    pub tile_sets: &'tile_sets TileSets,
    pub unit_configs: &'config UnitConfigs,
    pub building_configs: &'config BuildingConfigs,
}

// ----------------------------------------------
// SaveState Implementations
// ----------------------------------------------

pub mod backends {
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
        where T: serde::Serialize
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

    fn load<'de, T>(&'de self, instance: &mut T) -> LoadResult
        where T: serde::Deserialize<'de>
    {
        if self.buffer.is_empty() {
            return Err("JsonSaveState has no state to load!".into());
        }

        let deserialized = match serde_json::from_str::<T>(&self.buffer) {
            Ok(deserialized) => deserialized,
            Err(err)  => return Err(err.to_string()),
        };

        *instance = deserialized;
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
