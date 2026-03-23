use std::path::PathBuf;

// TODO: Same idea as file_sys: define a trait and impl for each target.

// ----------------------------------------------
// Platform-abstracted save file storage.
//
// On desktop, delegates to the filesystem.
// On Web/WASM, uses browser localStorage.
// ----------------------------------------------

// Returns the directory/prefix where save files are stored.
#[cfg(feature = "desktop")]
fn save_files_path() -> crate::file_sys::paths::FixedPath {
    crate::file_sys::paths::base_path().join("saves")
}

// Lists all available save file names (without directory prefix).
pub fn list_save_files() -> Vec<PathBuf> {
    #[cfg(feature = "desktop")]
    {
        use crate::file_sys;
        file_sys::collect_files(save_files_path(),
                                file_sys::CollectFlags::FilenamesOnly,
                                Some("json"))
                                .unwrap_or_default()
    }

    #[cfg(feature = "web")]
    {
        let storage = match local_storage() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut result = Vec::new();
        let len = storage.length().unwrap_or(0);

        for i in 0..len {
            if let Ok(Some(key)) = storage.key(i) {
                if let Some(name) = key.strip_prefix(SAVE_KEY_PREFIX) {
                    result.push(PathBuf::from(name));
                }
            }
        }

        result
    }
}

// Reads a save file by name and returns its contents as a string.
pub fn read_save(name: &str) -> Result<String, String> {
    #[cfg(feature = "desktop")]
    {
        use crate::file_sys;

        let path = save_files_path()
            .join(name)
            .with_extension("json");

        file_sys::load_string(&path)
            .map_err(|err| format!("Failed to read save file '{}': {}", path, err))
    }

    #[cfg(feature = "web")]
    {
        let storage = local_storage()
            .ok_or_else(|| "Web localStorage not available".to_string())?;

        let key = save_key(name);

        storage.get_item(&key)
            .map_err(|_| format!("Failed to read save '{}' from Web localStorage", name))?
            .ok_or_else(|| format!("Save file '{}' not found in Web localStorage", name))
    }
}

// Writes save data to a named save file.
pub fn write_save(name: &str, data: &str) -> Result<(), String> {
    #[cfg(feature = "desktop")]
    {
        use crate::file_sys;

        let dir = save_files_path();
        let _ = file_sys::create_path(&dir);

        let path = dir
            .join(name)
            .with_extension("json");

        file_sys::write_file(&path, data)
            .map_err(|err| format!("Failed to write save file '{}': {}", path, err))
    }

    #[cfg(feature = "web")]
    {
        let storage = local_storage()
            .ok_or_else(|| "localStorage not available".to_string())?;

        let key = save_key(name);

        storage.set_item(&key, data)
            .map_err(|_| format!("Failed to write save '{}' to localStorage (quota exceeded?)", name))
    }
}

// Deletes a named save file.
pub fn delete_save(name: &str) -> Result<(), String> {
    #[cfg(feature = "desktop")]
    {
        use crate::file_sys;

        let path = save_files_path()
            .join(name)
            .with_extension("json");

        file_sys::remove_file(&path)
            .map_err(|err| format!("Failed to delete save file '{}': {}", path, err))
    }

    #[cfg(feature = "web")]
    {
        let storage = local_storage()
            .ok_or_else(|| "Web localStorage not available".to_string())?;

        let key = save_key(name);

        storage.remove_item(&key)
            .map_err(|_| format!("Failed to delete save '{}' from Web localStorage", name))
    }
}

// Checks if a save file path is writable (desktop) or available (WASM).
pub fn can_write(name: &str) -> bool {
    #[cfg(feature = "desktop")]
    {
        use crate::file_sys;

        let dir = save_files_path();
        let _ = file_sys::create_path(&dir);

        let path = dir
            .join(name)
            .with_extension("json");

        // Probe by writing a dummy file.
        file_sys::write_file(&path, name).is_ok()
    }

    #[cfg(feature = "web")]
    {
        // localStorage is always writable (within size limits).
        let _ = name;
        true
    }
}

// ----------------------------------------------
// Web/WASM localStorage helpers
// ----------------------------------------------

#[cfg(feature = "web")]
const SAVE_KEY_PREFIX: &str = "heritage_builder/save/";

#[cfg(feature = "web")]
fn save_key(name: &str) -> String {
    format!("{}{}", SAVE_KEY_PREFIX, name)
}

#[cfg(feature = "web")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
