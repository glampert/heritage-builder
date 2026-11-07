use super::*;

// ----------------------------------------------
// FileSysSaveGameStorageBackend
// ----------------------------------------------

pub struct FileSysSaveGameStorageBackend;

impl FileSysSaveGameStorageBackend {
    fn make_absolute_save_path(&self, save_file: PathRef) -> FixedPath {
        debug_assert!(!save_file.is_empty());
        self.save_files_path().join(save_file).with_extension("json")
    }
}

impl SaveGameStorageBackend for FileSysSaveGameStorageBackend {
    fn save_files_path(&self) -> FixedPath {
        file_sys::paths::base_path().join("saves")
    }

    fn list_save_files(&self) -> Vec<PathBuf> {
        let files = file_sys::collect_files(self.save_files_path(), file_sys::CollectFlags::FilenamesOnly, Some("json"))
            .unwrap_or_default();

        // Strip file extension:
        files.iter().map(|path| path.with_extension("")).collect()
    }

    fn can_write_save_file(&self, save_file: PathRef) -> bool {
        let absolute_path = self.make_absolute_save_path(save_file);

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = file_sys::create_path(&absolute_path);

        // Probe by writing a dummy file with the save file path as its contents.
        file_sys::write_file(&absolute_path, absolute_path.as_str()).is_ok()
    }

    fn load_save_file<T>(&self, save_file: PathRef) -> Result<T, String>
    where
        T: DeserializeOwned,
    {
        let absolute_path = self.make_absolute_save_path(save_file);
        let mut state = new_json_save_state(false);

        if let Err(err) = state.read_file(&absolute_path) {
            return Err(format!("Failed to read save game file '{absolute_path}': {err}"));
        }

        // Load into a temporary instance so that if we fail we'll avoid modifying any state.
        match state.load_new_instance() {
            instance @ Ok(_) => instance,
            Err(err) => Err(format!("Failed to load save game from '{absolute_path}': {err}")),
        }
    }

    fn write_save_file<T>(&self, save_file: PathRef, instance: &T) -> SaveResult
    where
        T: Serialize,
    {
        let absolute_path = self.make_absolute_save_path(save_file);

        // First make sure the save directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = file_sys::create_path(&absolute_path);

        let mut state = new_json_save_state(true);

        if let Err(err) = state.save(instance) {
            return Err(format!("Failed to save game: {err}"));
        }

        if let Err(err) = state.write_file(&absolute_path) {
            return Err(format!("Failed to write save game file '{absolute_path}': {err}"));
        }

        Ok(())
    }

    fn delete_save_file(&self, save_file: PathRef) -> SaveResult {
        let absolute_path = self.make_absolute_save_path(save_file);

        file_sys::remove_file(&absolute_path).map_err(|err| format!("Failed to delete save file '{absolute_path}': {err}"))
    }

    // saves/user/: list_save_files only looks at the files directly in saves/.
    fn user_files_path(&self) -> FixedPath {
        self.save_files_path().join("user")
    }

    fn read_user_file(&self, user_file: PathRef) -> Result<Option<String>, String> {
        let absolute_path = super::user_file_path(user_file);

        if !file_sys::exists(&absolute_path) {
            return Ok(None);
        }

        file_sys::load_string(&absolute_path)
            .map(Some)
            .map_err(|err| format!("Failed to read user file '{absolute_path}': {err}"))
    }

    fn write_user_file(&self, user_file: PathRef, contents: &str) -> SaveResult {
        let absolute_path = super::user_file_path(user_file);

        // First make sure the directory exists. Ignore any errors since
        // this function might fail if any element of the path already exists.
        let _ = file_sys::create_path(&absolute_path);

        file_sys::write_file(&absolute_path, contents)
            .map_err(|err| format!("Failed to write user file '{absolute_path}': {err}"))
    }

    fn delete_user_file(&self, user_file: PathRef) -> SaveResult {
        let absolute_path = super::user_file_path(user_file);

        if !file_sys::exists(&absolute_path) {
            return Ok(());
        }

        file_sys::remove_file(&absolute_path).map_err(|err| format!("Failed to delete user file '{absolute_path}': {err}"))
    }
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

// FileSysSaveGameStorageBackend is stateless - simply return a new dummy instance.
impl FileSysSaveGameStorageBackend {
    #[inline]
    pub fn get() -> FileSysSaveGameStorageBackend {
        FileSysSaveGameStorageBackend
    }

    #[inline]
    pub fn get_mut() -> FileSysSaveGameStorageBackend {
        FileSysSaveGameStorageBackend
    }
}
