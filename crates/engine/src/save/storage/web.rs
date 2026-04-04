use super::*;
use common::singleton;

// ----------------------------------------------
// WebSaveGameStorageBackend
// ----------------------------------------------

pub struct WebSaveGameStorageBackend;

impl WebSaveGameStorageBackend {
    const fn new() -> Self {
        Self
    }

    const SAVE_KEY_PREFIX: &str = "heritage_builder/saves/";

    fn make_save_key(&self, save_file: PathRef) -> FixedPath {
        debug_assert!(!save_file.is_empty());
        self.save_files_path().join(save_file).with_extension("json")
    }

    fn browser_local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }
}

impl SaveGameStorageBackend for WebSaveGameStorageBackend {
    fn save_files_path(&self) -> FixedPath {
        FixedPath::from_str(Self::SAVE_KEY_PREFIX)
    }

    fn list_save_files(&self) -> Vec<PathBuf> {
        let storage = match Self::browser_local_storage() {
            Some(storage) => storage,
            None => return Vec::new(),
        };

        let len = storage.length().unwrap_or(0);
        let mut files = Vec::with_capacity(len as usize);

        for i in 0..len {
            if let Ok(Some(key)) = storage.key(i) {
                if let Some(name) = key.strip_prefix(Self::SAVE_KEY_PREFIX) {
                    // Return file name without path or extension.
                    files.push(Path::new(name).with_extension(""));
                }
            }
        }

        files
    }

    #[inline]
    fn can_write_save_file(&self, _save_file: PathRef) -> bool {
        if Self::browser_local_storage().is_none() {
            return false;
        }

        // localStorage is always writable within size limits (usually 5MB max).
        true
    }

    fn load_save_file<T>(&self, save_file: PathRef) -> Result<T, String>
        where T: DeserializeOwned
    {
        let storage = Self::browser_local_storage()
            .ok_or_else(|| "Browser Local Storage not available.".to_string())?;

        let key = self.make_save_key(save_file);

        let storage_result = storage.get_item(key.as_str())
            .map_err(|_| format!("Failed to read save '{save_file}' from Browser Local Storage."))?
            .ok_or_else(|| format!("Save file '{save_file}' not found in Browser Local Storage."));

        let save_data = match storage_result {
            Ok(save_data) => save_data,
            Err(err) => return Err(err),
        };

        let state = new_json_save_state_with_data(false, save_data);

        // Load into a temporary instance so that if we fail we'll avoid modifying any state.
        match state.load_new_instance() {
            instance @ Ok(_) => instance,
            Err(err) => Err(format!("Failed to load save game from '{key}': {err}")),
        }
    }

    fn write_save_file<T>(&self, save_file: PathRef, instance: &T) -> SaveResult
        where T: Serialize
    {
        let storage = Self::browser_local_storage()
            .ok_or_else(|| "Browser Local Storage not available.".to_string())?;

        let key = self.make_save_key(save_file);

        let mut state = new_json_save_state(true);

        if let Err(err) = state.save(instance) {
            return Err(format!("Failed to save game: {err}"));
        }

        let json = state.as_any().downcast_ref::<JsonSaveState>().unwrap();

        storage.set_item(key.as_str(), json.to_str())
            .map_err(|_| format!("Failed to write save '{save_file}' to Browser Local Storage (quota exceeded?)"))
    }

    fn delete_save_file(&self, save_file: PathRef) -> SaveResult {
        let storage = Self::browser_local_storage()
            .ok_or_else(|| "Browser Local Storage not available.".to_string())?;

        let key = self.make_save_key(save_file);

        storage.remove_item(key.as_str())
            .map_err(|_| format!("Failed to delete save '{save_file}' from Browser Local Storage."))
    }
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

// NOTE: Web/WASM code is single-threaded. Safe to use a global singleton.
singleton! { WEB_SAVE_GAME_STORAGE_BACKEND_SINGLETON, WebSaveGameStorageBackend }
