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

    // Deletes a named save file, and its thumbnail if it has one.
    // `save_file` is relative to save_files_path.
    fn delete_save_file(&self, save_file: PathRef) -> SaveResult;

    // The first `max_bytes` bytes of a save file's text, or all of it if shorter: enough to read
    // what a save writes before its session (its version and header) without reading the session.
    // Cut at a character boundary.
    fn read_save_file_head(&self, save_file: PathRef, max_bytes: usize) -> Result<String, String>;

    // The last `max_bytes` bytes of a save file's text, or all of it if shorter (cut at a
    // character boundary): where a save rewritten by the migration scripts keeps its version.
    fn read_save_file_tail(&self, save_file: PathRef, max_bytes: usize) -> Result<String, String>;

    // Where a save's thumbnail image goes (a JPEG beside the save, same name), or None where saves
    // keep no thumbnail (web).
    fn save_thumbnail_path(&self, save_file: PathRef) -> Option<FixedPath>;

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

// Deletes the save and its thumbnail.
#[inline]
pub fn delete_save_file(save_file: PathRef) -> SaveResult {
    SaveGameStorageBackendImpl::get().delete_save_file(save_file)
}

// The start of a save's text (see SaveGameStorageBackend::read_save_file_head).
#[inline]
pub fn read_save_file_head(save_file: PathRef, max_bytes: usize) -> Result<String, String> {
    SaveGameStorageBackendImpl::get().read_save_file_head(save_file, max_bytes)
}

// The end of a save's text (see SaveGameStorageBackend::read_save_file_tail).
#[inline]
pub fn read_save_file_tail(save_file: PathRef, max_bytes: usize) -> Result<String, String> {
    SaveGameStorageBackendImpl::get().read_save_file_tail(save_file, max_bytes)
}

// The extension of save thumbnails: JPEG images beside the saves.
pub const SAVE_THUMBNAIL_EXTENSION: &str = "jpg";

// Where the save's thumbnail goes (saves/<name>.jpg), or None where saves keep none: on the web,
// whose saves live in localStorage.
// TODO(realm-ui): a web thumbnail would go in localStorage as base64 beside the save, once web
// captures (which map their buffer a few frames later) are worth that storage.
#[inline]
pub fn save_thumbnail_path(save_file: PathRef) -> Option<FixedPath> {
    SaveGameStorageBackendImpl::get().save_thumbnail_path(save_file)
}

// The save's thumbnail file, if it has one: the encoded image.
pub fn read_save_thumbnail(save_file: PathRef) -> Option<Vec<u8>> {
    let path = save_thumbnail_path(save_file)?;
    if !crate::file_sys::exists(&path) {
        return None;
    }
    crate::file_sys::load_bytes(&path).ok()
}

// Deletes the save's thumbnail file, if it has one: a save that wrote no new thumbnail, so the
// preview doesn't show another game's city.
pub fn delete_save_thumbnail(save_file: PathRef) -> SaveResult {
    let Some(path) = save_thumbnail_path(save_file) else {
        return Ok(());
    };
    if !crate::file_sys::exists(&path) {
        return Ok(());
    }
    crate::file_sys::remove_file(&path).map_err(|err| format!("Failed to delete save thumbnail '{path}': {err}"))
}

// The last `max_bytes` bytes of `text` at most, from a character boundary.
fn tail_at_char_boundary(text: &str, max_bytes: usize) -> String {
    let mut start = text.len().saturating_sub(max_bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

// Text from bytes cut anywhere: the partial characters at either end are dropped.
fn text_from_cut_bytes(bytes: &[u8]) -> String {
    // A UTF-8 character's continuation bytes are 0b10xxxxxx: skip those at the start.
    let start = bytes.iter().take(4).take_while(|byte| (**byte & 0xC0) == 0x80).count();
    match std::str::from_utf8(&bytes[start..]) {
        Ok(text) => text.to_string(),
        Err(err) => String::from_utf8_lossy(&bytes[start..start + err.valid_up_to()]).into_owned(),
    }
}

// Cuts `text` to at most `max_bytes` bytes, at a character boundary.
fn truncate_at_char_boundary(mut text: String, max_bytes: usize) -> String {
    if text.len() > max_bytes {
        let mut end = max_bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
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

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_keep_whole_characters() {
        let text = "ab\u{0110}\u{1ea1}i".to_string(); // a b Đ ạ i: 1 + 1 + 2 + 3 + 1 bytes.
        assert_eq!(truncate_at_char_boundary(text.clone(), 3), "ab");
        assert_eq!(truncate_at_char_boundary(text.clone(), 4), "ab\u{0110}");
        assert_eq!(truncate_at_char_boundary(text.clone(), 100), text);
        assert_eq!(tail_at_char_boundary(&text, 2), "i");
        assert_eq!(tail_at_char_boundary(&text, 4), "\u{1ea1}i");
        assert_eq!(tail_at_char_boundary(&text, 100), text);

        let bytes = text.as_bytes();
        assert_eq!(text_from_cut_bytes(&bytes[3..]), "\u{1ea1}i"); // Starts inside Đ.
        assert_eq!(text_from_cut_bytes(&bytes[..6]), "ab\u{0110}"); // Ends inside ạ.
        assert_eq!(text_from_cut_bytes(bytes), text);
    }
}
