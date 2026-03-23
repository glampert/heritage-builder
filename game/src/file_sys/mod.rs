use std::{io, path::{Path, PathBuf}};
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
trait FileSystemBackend {
    // Tries to set the current working directory.
    fn set_working_directory(&mut self, path: impl AsRef<Path>);

    // Absolute path where the application runs from. Parent of assets_path.
    fn base_path(&self) -> &'static paths::FixedPath;

    // Returns the absolute path to the game's assets directory.
    // On MacOS, this will point inside `.app/Contents/Resources/assets`.
    // On other platforms or in dev runs, it falls back to `./assets`.
    fn assets_path(&self) -> &'static paths::AssetPath;

    // Test if the path exists (might be a directory or a file).
    fn exists(&self, path: impl AsRef<Path>) -> bool;

    // Load file contents into memory.
    fn load_bytes(&mut self, path: impl AsRef<Path>)  -> io::Result<Vec<u8>>;
    fn load_string(&mut self, path: impl AsRef<Path>) -> io::Result<String>;

    // Create/remove files/directories.
    fn write_file(&mut self, path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()>;
    fn remove_file(&mut self, path: impl AsRef<Path>) -> io::Result<()>;
    fn create_path(&mut self, path: impl AsRef<Path>) -> io::Result<()>;

    // Scan directory contents.
    fn collect_dir_entries(&mut self,
                           path: impl AsRef<Path>,
                           flags: CollectFlags,
                           extension: Option<&str>) -> io::Result<Vec<PathBuf>>;
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
    FileSystemBackendImpl::get_mut().write_file(path, data)
}

// Removes a file at the given path.
#[inline]
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    FileSystemBackendImpl::get_mut().remove_file(path)
}

// Creates a directory (and all parent directories).
#[inline]
pub fn create_path(path: impl AsRef<Path>) -> io::Result<()> {
    FileSystemBackendImpl::get_mut().create_path(path)
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
pub fn collect_sub_dirs(path: impl AsRef<Path>,
                        flags: CollectFlags) -> io::Result<Vec<PathBuf>>
{
    collect_dir_entries(path, flags | CollectFlags::SubDirs, None)
}

#[inline]
pub fn collect_files(path: impl AsRef<Path>,
                     flags: CollectFlags,
                     extension: Option<&str>) -> io::Result<Vec<PathBuf>>
{
    collect_dir_entries(path, flags | CollectFlags::Files, extension)
}

#[inline]
pub fn collect_dir_entries(path: impl AsRef<Path>,
                           flags: CollectFlags,
                           extension: Option<&str>) -> io::Result<Vec<PathBuf>>
{
    FileSystemBackendImpl::get_mut().collect_dir_entries(path, flags, extension)
}
