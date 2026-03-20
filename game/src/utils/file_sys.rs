use std::{io, path::{Path, PathBuf}};
use bitflags::bitflags;
use crate::log;

#[cfg(feature = "desktop")]
use std::fs;

#[cfg(feature = "web")]
use crate::web::asset_cache;

// ----------------------------------------------
// Platform-abstracted file operations
// ----------------------------------------------

// Checks if a file or directory exists.
// On desktop, delegates to std::fs::exists.
// On Web/WASM, always returns false (no filesystem).
pub fn exists(path: impl AsRef<Path>) -> bool {
    #[cfg(feature = "desktop")]
    {
        fs::exists(path).is_ok_and(|exists| exists)
    }

    #[cfg(feature = "web")]
    { let _ = path; false }
}

// Reads the entire contents of a file into a byte vector.
// On desktop, reads from the filesystem.
// On Web/WASM, reads from the pre-loaded asset cache.
pub fn load_bytes<P>(path: P) -> io::Result<Vec<u8>>
    where P: AsRef<Path> + std::fmt::Display
{
    #[cfg(feature = "desktop")]
    {
        fs::read(path)
    }

    #[cfg(feature = "web")]
    {
        asset_cache::get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Asset not found in Web cache"))
    }
}

// Reads the entire contents of a file into a string.
// On desktop, reads from the filesystem.
// On Web/WASM, reads from the pre-loaded asset cache.
pub fn load_string(path: impl AsRef<Path>) -> io::Result<String> {
    #[cfg(feature = "desktop")]
    {
        fs::read_to_string(path)
    }

    #[cfg(feature = "web")]
    {
        let bytes = asset_cache::get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Asset not found in Web cache:"))?;

        String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

// Writes data to a file at the given path.
// On desktop, delegates to std::fs::write.
// On Web/WASM, this is a no-op (save system uses its own web storage abstraction).
pub fn write_file(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    #[cfg(feature = "desktop")]
    {
        fs::write(path, data)
    }

    #[cfg(feature = "web")]
    {
        let _ = (path, data);
        Err(io::Error::new(io::ErrorKind::Unsupported,
            "Direct file write not supported on Web/WASM"))
    }
}

// Removes a file at the given path.
// On desktop, delegates to std::fs::remove_file.
// On Web/WASM, returns an error (no filesystem).
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(feature = "desktop")]
    {
        fs::remove_file(path)
    }

    #[cfg(feature = "web")]
    {
        let _ = path;
        Err(io::Error::new(io::ErrorKind::Unsupported,
            "File removal not supported on Web/WASM"))
    }
}

// Creates a directory (and all parent directories).
// On desktop, delegates to fs::create_dir_all.
// On Web/WASM, this is a no-op (no filesystem).
pub fn create_path(path: impl AsRef<Path>) {
    #[cfg(feature = "desktop")]
    {
        let _ = fs::create_dir_all(path);
    }

    #[cfg(feature = "web")]
    { let _ = path; }
}

// ----------------------------------------------
// collect_files / collect_sub_dirs
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

pub fn collect_files<P>(path: P, flags: CollectFlags, extension: Option<&str>) -> Vec<PathBuf>
    where P: AsRef<Path>
{
    collect_dir_entries(path, flags | CollectFlags::Files, extension)
}

pub fn collect_sub_dirs<P>(path: P, flags: CollectFlags) -> Vec<PathBuf>
    where P: AsRef<Path>
{
    collect_dir_entries(path, flags | CollectFlags::SubDirs, None)
}

#[cfg(feature = "desktop")]
pub fn collect_dir_entries<P>(path: P,
                              flags: CollectFlags,
                              extension: Option<&str>)
                              -> Vec<PathBuf>
    where P: AsRef<Path>
{
    let mut result = Vec::new();

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            if flags.intersects(CollectFlags::ErrorIfPathDoesNotExist) {
                log::error!(log::channel!("fs"), "Failed to read directory: {err}");
            }
            return result;
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
                log::error!(log::channel!("fs"), "Error reading directory entry: {err}");
                continue;
            }
        }
    }

    result
}

#[cfg(feature = "web")]
pub fn collect_dir_entries<P>(_path: P,
                              _flags: CollectFlags,
                              _extension: Option<&str>)
                              -> Vec<PathBuf>
    where P: AsRef<Path>
{
    // No filesystem directory scanning on Web/WASM.
    log::error!(log::channel!("fs"), "collect_dir_entries() not supported on Web/WASM");
    Vec::new()
}
