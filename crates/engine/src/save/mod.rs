use std::{io, path::Path, any::Any};
use enum_dispatch::enum_dispatch;
use serde::{de::DeserializeOwned, Serialize};

use crate::file_sys;

pub mod storage;

// ----------------------------------------------
// SaveState Helpers
// ----------------------------------------------

pub type SaveResult = Result<(), String>;
pub type LoadResult = Result<(), String>;

#[enum_dispatch(SaveStateImpl)]
pub trait SaveState: Any {
    fn as_any(&self) -> &dyn Any;

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
    JsonSaveState,
}

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

    pub fn with_data(pretty_print: bool, data: String) -> Self {
        Self { pretty: pretty_print, buffer: data }
    }

    pub fn to_str(&self) -> &str {
        &self.buffer
    }
}

impl SaveState for JsonSaveState {
    fn as_any(&self) -> &dyn Any {
        self
    }

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

#[inline]
pub fn new_json_save_state_with_data(pretty_print: bool, data: String) -> SaveStateImpl {
    SaveStateImpl::from(JsonSaveState::with_data(pretty_print, data))
}
