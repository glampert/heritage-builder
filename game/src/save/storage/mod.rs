use std::path::PathBuf;
use crate::file_sys::paths::{FixedPath, PathRef};
use super::*;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
type SaveGameStorageBackendImpl = desktop::FileSysSaveGameStorageBackend;

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
type SaveGameStorageBackendImpl = web::WebSaveGameStorageBackend;

// ----------------------------------------------
// SaveGameStorageBackend
// ----------------------------------------------

// Platform-abstracted save file storage.
// - On desktop, delegates to the filesystem.
// - On Web/WASM, uses browser localStorage.
trait SaveGameStorageBackend: Sized {
    // Returns the directory/prefix where save files are stored.
    fn save_files_path(&self) -> FixedPath;

    // Lists all available save file names (without directory prefix and extension).
    fn list_save_files(&self) -> Vec<PathBuf>;

    // Checks if a save file path is writable (desktop) or available (WASM).
    // `save_file` is relative to save_files_path.
    fn can_write_save_file(&self, save_file: PathRef) -> bool;

    // Reads a save file and returns its contents as a new instance, or an error description string.
    // `save_file` is relative to save_files_path.
    fn load_save_file<T>(&self, save_file: PathRef) -> Result<T, String>
        where T: DeserializeOwned + Load;

    // Writes save data to a named save file. Overwrites any existing file with the same name.
    // `save_file` is relative to save_files_path.
    fn write_save_file<T>(&self, save_file: PathRef, instance: &T) -> SaveResult
        where T: Serialize + Save;

    // Deletes a named save file.
    // `save_file` is relative to save_files_path.
    fn delete_save_file(&self, save_file: PathRef) -> SaveResult;
}

// ----------------------------------------------
// Public API
// ----------------------------------------------

pub const AUTOSAVE_FILE_NAME:     PathRef = PathRef::from_str("autosave");
pub const DEFAULT_SAVE_FILE_NAME: PathRef = PathRef::from_str("save_game");

#[inline]
pub fn save_files_path() -> FixedPath {
    SaveGameStorageBackendImpl::get().save_files_path()
}

#[inline]
pub fn list_save_files() -> Vec<PathBuf> {
    SaveGameStorageBackendImpl::get().list_save_files()
}

#[inline]
pub fn can_write_save_file(save_file: PathRef) -> bool {
    SaveGameStorageBackendImpl::get().can_write_save_file(save_file)
}

#[inline]
pub fn load_save_file<T>(save_file: PathRef) -> Result<T, String>
    where T: DeserializeOwned + Load
{
    SaveGameStorageBackendImpl::get().load_save_file(save_file)
}

#[inline]
pub fn write_save_file<T>(save_file: PathRef, instance: &T) -> SaveResult
    where T: Serialize + Save
{
    SaveGameStorageBackendImpl::get().write_save_file(save_file, instance)
}

#[inline]
pub fn delete_save_file(save_file: PathRef) -> SaveResult {
    SaveGameStorageBackendImpl::get().delete_save_file(save_file)
}
