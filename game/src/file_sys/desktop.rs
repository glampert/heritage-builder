use std::{fs, sync::LazyLock};
use super::*;
use crate::log;

// ----------------------------------------------
// StandardFileSystemBackend
// ----------------------------------------------

// FileSystemBackend implementation for any platform that support std::fs.
pub struct StandardFileSystemBackend;

impl FileSystemBackend for StandardFileSystemBackend {
    fn set_working_directory(&mut self, path: impl AsRef<Path>) {
        if let Err(err) = std::env::set_current_dir(path) {
            log::warning!("Failed to set working directory: {err}");
        }
    }

    #[inline]
    fn base_path(&self) -> &'static paths::FixedPath {
        &CACHED_PATHS.base_path
    }

    #[inline]
    fn assets_path(&self) -> &'static paths::AssetPath {
        &CACHED_PATHS.assets_path
    }

    #[inline]
    fn exists(&self, path: impl AsRef<Path>) -> bool {
        fs::exists(path).is_ok_and(|exists| exists)
    }

    #[inline]
    fn load_bytes(&mut self, path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    #[inline]
    fn load_string(&mut self, path: impl AsRef<Path>) -> io::Result<String> {
        fs::read_to_string(path)
    }

    #[inline]
    fn write_file(&mut self, path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
        fs::write(path, data)
    }

    #[inline]
    fn remove_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::remove_file(path)
    }

    #[inline]
    fn create_path(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn collect_dir_entries(&mut self,
                           path: impl AsRef<Path>,
                           flags: CollectFlags,
                           extension: Option<&str>) -> io::Result<Vec<PathBuf>>
    {
        let mut result = Vec::new();

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(err) => {
                if flags.intersects(CollectFlags::ErrorIfPathDoesNotExist) {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory,
                               format!("Failed to read directory: {err}")));
                }
                return Ok(result);
            }
        };

        for entry_result in entries {
            match entry_result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() && flags.intersects(CollectFlags::Files) {
                        if flags.intersects(CollectFlags::FilenamesOnly) {
                            let filename = path.file_name().unwrap();
                            if let Some(extension) = extension {
                                if path.extension().is_some_and(|ext| ext == extension) {
                                    result.push(filename.into());
                                }
                            } else {
                                result.push(filename.into());
                            }
                        } else if let Some(extension) = extension {
                            if path.extension().is_some_and(|ext| ext == extension) {
                                result.push(path);
                            }
                        } else {
                            result.push(path);
                        }
                    } else if path.is_dir() && flags.intersects(CollectFlags::SubDirs) {
                        result.push(path);
                    }
                }
                Err(err) => {
                    log::error!(log::channel!("file_sys"), "Error reading directory entry: {err}");
                    continue;
                }
            }
        }

        Ok(result)
    }
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

// Std file system backend is stateless - simply return a new dummy instance.
impl StandardFileSystemBackend {
    #[inline]
    pub fn get() -> StandardFileSystemBackend {
        StandardFileSystemBackend
    }

    #[inline]
    pub fn get_mut() -> StandardFileSystemBackend {
        StandardFileSystemBackend
    }
}

// ----------------------------------------------
// Platform Path Handling
// ----------------------------------------------

struct CachedPaths {
    base_path: paths::FixedPath,
    assets_path: paths::AssetPath,
}

// Cached on first use.
static CACHED_PATHS: LazyLock<CachedPaths> = LazyLock::new(|| {
    CachedPaths {
        base_path: find_base_path(),
        assets_path: find_assets_path(),
    }
});

fn find_base_path() -> paths::FixedPath {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle_resources) = macos_bundle_resources_path() {
            if bundle_resources.exists() {
                return bundle_resources;
            }
        }
    }

    // Default for dev / non-MacOS.
    project_relative("")
}

fn find_assets_path() -> paths::AssetPath {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle_resources) = macos_bundle_resources_path() {
            if bundle_resources.exists() {
                return bundle_resources.join("assets");
            }
        }
    }

    // Default for dev / non-MacOS.
    project_relative("assets")
}

// Fallback for non-MacOS platforms or when running unbundled.
// Returns a path relative to the project root.
fn project_relative(relative_path: &str) -> paths::FixedPath {
    // Try CARGO_MANIFEST_DIR for a stable dev path:
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return paths::FixedPath::from_str(&manifest_dir).join(relative_path);
    }

    // Fallback: current working directory.
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !relative_path.is_empty() {
        dir = dir.join(relative_path);
    }

    paths::FixedPath::from_path(&dir)
}

// Internal helper to find the Resources directory when inside a MacOS bundle.
#[cfg(target_os = "macos")]
fn macos_bundle_resources_path() -> Option<paths::FixedPath> {
    // Example: /MyGame.app/Contents/MacOS/MyGame
    let exe_path = std::env::current_exe().ok()?;
    let mut path = exe_path.parent()?.to_path_buf();

    for _ in 0..3 {
        if path.ends_with("MacOS") {
            let contents = paths::FixedPath::from_path(path.parent()?);
            if contents.ends_with("Contents") {
                return Some(contents.join("Resources"));
            }
        }
        path = path.parent()?.to_path_buf();
    }

    None
}
