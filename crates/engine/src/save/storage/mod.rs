use std::path::PathBuf;

use super::*;
use crate::file_sys::paths::{FixedPath, PathRef};

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
    where
        T: DeserializeOwned;

    // Writes save data to a named save file. Overwrites any existing file with the same name.
    // `save_file` is relative to save_files_path.
    fn write_save_file<T>(&self, save_file: PathRef, instance: &T) -> SaveResult
    where
        T: Serialize;

    // Deletes a named save file.
    // `save_file` is relative to save_files_path.
    fn delete_save_file(&self, save_file: PathRef) -> SaveResult;

    // User files: the player's own settings, such as the user configs (engine::config), kept in
    // `user_files_path`, a subdirectory of the saves that list_save_files leaves out. `user_file`
    // is a name without extension; the file is JSON text.

    // Returns the directory/prefix where user files are stored.
    fn user_files_path(&self) -> FixedPath;

    // The file's contents, or None if there's no such file.
    fn read_user_file(&self, user_file: PathRef) -> Result<Option<String>, String>;

    // Writes a user file. Overwrites any existing file with the same name.
    fn write_user_file(&self, user_file: PathRef, contents: &str) -> SaveResult;

    // Deletes a user file. Succeeds if there's no such file.
    fn delete_user_file(&self, user_file: PathRef) -> SaveResult;
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
    let mut saves = SaveGameStorageBackendImpl::get().list_save_files();
    // Return the save files list deterministically sorted.
    saves.sort();
    saves
}

#[inline]
pub fn can_write_save_file(save_file: PathRef) -> bool {
    SaveGameStorageBackendImpl::get().can_write_save_file(save_file)
}

#[inline]
pub fn load_save_file<T>(save_file: PathRef) -> Result<T, String>
where
    T: DeserializeOwned,
{
    SaveGameStorageBackendImpl::get().load_save_file(save_file)
}

#[inline]
pub fn write_save_file<T>(save_file: PathRef, instance: &T) -> SaveResult
where
    T: Serialize,
{
    SaveGameStorageBackendImpl::get().write_save_file(save_file, instance)
}

#[inline]
pub fn delete_save_file(save_file: PathRef) -> SaveResult {
    SaveGameStorageBackendImpl::get().delete_save_file(save_file)
}

// User files (see SaveGameStorageBackend): saves/user/ on desktop, browser localStorage on the web.

#[inline]
pub fn user_file_path(user_file: PathRef) -> FixedPath {
    SaveGameStorageBackendImpl::get().user_files_path().join(user_file).with_extension("json")
}

#[inline]
pub fn read_user_file(user_file: PathRef) -> Result<Option<String>, String> {
    SaveGameStorageBackendImpl::get().read_user_file(user_file)
}

#[inline]
pub fn write_user_file(user_file: PathRef, contents: &str) -> SaveResult {
    SaveGameStorageBackendImpl::get().write_user_file(user_file, contents)
}

#[inline]
pub fn delete_user_file(user_file: PathRef) -> SaveResult {
    SaveGameStorageBackendImpl::get().delete_user_file(user_file)
}
