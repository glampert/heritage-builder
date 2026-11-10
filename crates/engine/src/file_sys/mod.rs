use std::{
    io,
    path::{Path, PathBuf},
};

use bitflags::bitflags;

pub mod paths;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
type FileSystemBackendImpl = desktop::StandardFileSystemBackend;

#[cfg(feature = "web")]
mod web;
#[cfg(feature = "web")]
type FileSystemBackendImpl = web::WebFileSystemBackend;
#[cfg(feature = "web")]
pub use web::preload_asset_cache;

// ----------------------------------------------
// FileSystemBackend
// ----------------------------------------------

// File System operations wrapper.
// - On desktop, accesses the filesystem directly using std::fs.
// - On Web/WASM, reads from the pre-loaded asset cache.
trait FileSystemBackend: Sized {
    // Tries to set the current working directory.
    fn set_working_directory(&self, path: impl AsRef<Path>);

    // Absolute path where the application runs from. Parent of assets_path.
    fn base_path(&self) -> &'static paths::FixedPath;

    // Returns the absolute path to the game's assets directory.
    // On MacOS, this will point inside `.app/Contents/Resources/assets`.
    // On other platforms or in dev runs, it falls back to `./assets`.
    fn assets_path(&self) -> &'static paths::AssetPath;

    // Test if the path exists (might be a directory or a file).
    fn exists(&self, path: impl AsRef<Path>) -> bool;

    // Load file contents into memory.
    fn load_bytes(&mut self, path: impl AsRef<Path>) -> io::Result<Vec<u8>>;
    fn load_string(&mut self, path: impl AsRef<Path>) -> io::Result<String>;

    // Create/remove files/directories.
    fn write_file(&self, path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()>;
    fn remove_file(&self, path: impl AsRef<Path>) -> io::Result<()>;
    fn create_path(&self, path: impl AsRef<Path>) -> io::Result<()>;

    // Scan directory contents.
    fn collect_dir_entries(
        &self,
        path: impl AsRef<Path>,
        flags: CollectFlags,
        extension: Option<&str>,
    ) -> io::Result<Vec<PathBuf>>;
}

// ----------------------------------------------
// Listing preloaded files (web)
// ----------------------------------------------

// The entries of the directory `dir` (a normalized path, without "./") among `paths`, the files
// the web build preloaded, as collect_dir_entries lists them: its files (by name only with
// FilenamesOnly; with `extension`, only those), then its subdirectories, each sorted. The files
// are listed without directories, so a directory is there while a file under it is. With
// ErrorIfPathDoesNotExist, an error if nothing is under `dir` at all. Compiled on every target,
// so it's tested on desktop.
fn list_preloaded_entries<'a>(
    paths: impl Iterator<Item = &'a str> + Clone,
    dir: &str,
    flags: CollectFlags,
    extension: Option<&str>,
) -> io::Result<Vec<PathBuf>> {
    let prefix = format!("{}/", dir.trim_end_matches('/'));

    let mut files = Vec::new();
    let mut sub_dirs = Vec::new();

    for path in paths.clone() {
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };

        match relative.split_once('/') {
            None if flags.intersects(CollectFlags::Files) => {
                let matches = extension.is_none_or(|extension| Path::new(relative).extension().is_some_and(|ext| ext == extension));
                if matches {
                    files.push(if flags.intersects(CollectFlags::FilenamesOnly) { PathBuf::from(relative) } else { PathBuf::from(path) });
                }
            }
            Some((sub_dir, _)) if flags.intersects(CollectFlags::SubDirs) => {
                let sub_dir = PathBuf::from(format!("{prefix}{sub_dir}"));
                if !sub_dirs.contains(&sub_dir) {
                    sub_dirs.push(sub_dir);
                }
            }
            _ => {}
        }
    }

    if files.is_empty() && sub_dirs.is_empty() && flags.intersects(CollectFlags::ErrorIfPathDoesNotExist) {
        let mut all = paths;
        if !all.any(|path| path.starts_with(&prefix)) {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("No preloaded assets under {dir}")));
        }
    }

    // The web cache is a hash map: sort, for the same order on every run.
    files.sort();
    sub_dirs.sort();
    files.extend(sub_dirs);
    Ok(files)
}

// ----------------------------------------------
// Platform-abstracted file operations
// ----------------------------------------------

// Checks if a file or directory exists.
#[inline]
pub fn exists(path: impl AsRef<Path>) -> bool {
    FileSystemBackendImpl::get().exists(path)
}

// Reads the entire contents of a file into a byte vector.
#[inline]
pub fn load_bytes(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    FileSystemBackendImpl::get_mut().load_bytes(path)
}

// Reads the entire contents of a file into a string.
#[inline]
pub fn load_string(path: impl AsRef<Path>) -> io::Result<String> {
    FileSystemBackendImpl::get_mut().load_string(path)
}

// Writes data to a file at the given path.
#[inline]
pub fn write_file(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    FileSystemBackendImpl::get().write_file(path, data)
}

// Removes a file at the given path.
#[inline]
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    FileSystemBackendImpl::get().remove_file(path)
}

// Creates the directories leading to a file path, so the file can then be written.
// Pass the file's path: the last component is treated as the file name and isn't created.
#[inline]
pub fn create_path(path: impl AsRef<Path>) -> io::Result<()> {
    FileSystemBackendImpl::get().create_path(path)
}

// ----------------------------------------------
// Directory scanning helpers
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct CollectFlags: u32 {
        const Files                   = 1 << 0;
        const SubDirs                 = 1 << 1;
        const FilenamesOnly           = 1 << 2;
        const ErrorIfPathDoesNotExist = 1 << 3;
    }
}

#[inline]
pub fn collect_sub_dirs(path: impl AsRef<Path>, flags: CollectFlags) -> io::Result<Vec<PathBuf>> {
    collect_dir_entries(path, flags | CollectFlags::SubDirs, None)
}

#[inline]
pub fn collect_files(path: impl AsRef<Path>, flags: CollectFlags, extension: Option<&str>) -> io::Result<Vec<PathBuf>> {
    collect_dir_entries(path, flags | CollectFlags::Files, extension)
}

#[inline]
pub fn collect_dir_entries(
    path: impl AsRef<Path>,
    flags: CollectFlags,
    extension: Option<&str>,
) -> io::Result<Vec<PathBuf>> {
    FileSystemBackendImpl::get().collect_dir_entries(path, flags, extension)
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PRELOADED: [&str; 6] = [
        "assets/configs/ui/text/vi.json",
        "assets/configs/ui/text/en.json",
        "assets/configs/ui/text/README.md",
        "assets/configs/ui/skins/dailong.json",
        "assets/configs/ui/skins/old/kambara.json",
        "assets/configs/ui/textures.json", // Not under "text/": the prefix ends with a slash.
    ];

    fn list(dir: &str, flags: CollectFlags, extension: Option<&str>) -> io::Result<Vec<String>> {
        let entries = list_preloaded_entries(PRELOADED.iter().copied(), dir, flags, extension)?;
        Ok(entries.into_iter().map(|path| path.to_string_lossy().into_owned()).collect())
    }

    #[test]
    fn preloaded_files_by_extension() {
        assert_eq!(
            list("assets/configs/ui/text", CollectFlags::Files, Some("json")).unwrap(),
            ["assets/configs/ui/text/en.json", "assets/configs/ui/text/vi.json"],
        );
        assert_eq!(list("assets/configs/ui/text/", CollectFlags::Files | CollectFlags::FilenamesOnly, None).unwrap(), ["README.md", "en.json", "vi.json"]);
    }

    #[test]
    fn preloaded_sub_dirs_once_each_after_the_files() {
        assert_eq!(
            list("assets/configs/ui", CollectFlags::Files | CollectFlags::SubDirs, None).unwrap(),
            ["assets/configs/ui/textures.json", "assets/configs/ui/skins", "assets/configs/ui/text"],
        );
        assert_eq!(list("assets/configs", CollectFlags::SubDirs, None).unwrap(), ["assets/configs/ui"]);
        assert_eq!(list("assets/configs/ui/skins", CollectFlags::Files, None).unwrap(), ["assets/configs/ui/skins/dailong.json"]);
    }

    #[test]
    fn missing_preloaded_dir() {
        assert!(list("assets/sounds", CollectFlags::Files, None).unwrap().is_empty());
        let err = list("assets/sounds", CollectFlags::Files | CollectFlags::ErrorIfPathDoesNotExist, None).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        // A directory with files, none matching, exists.
        assert!(list("assets/configs/ui/text", CollectFlags::Files | CollectFlags::ErrorIfPathDoesNotExist, Some("png")).unwrap().is_empty());
    }
}
