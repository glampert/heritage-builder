use bitflags::bitflags;
use std::{fs, path::{Path, PathBuf}};
use crate::log;

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

pub fn collect_files<P>(path: &P, flags: CollectFlags) -> Vec<PathBuf>
    where P: AsRef<Path> + std::fmt::Debug
{
    collect_dir_entries(path, flags | CollectFlags::Files)
}

pub fn collect_sub_dirs<P>(path: &P, flags: CollectFlags) -> Vec<PathBuf>
    where P: AsRef<Path> + std::fmt::Debug
{
    collect_dir_entries(path, flags | CollectFlags::SubDirs)
}

pub fn collect_dir_entries<P>(path: &P, flags: CollectFlags) -> Vec<PathBuf>
    where P: AsRef<Path> + std::fmt::Debug
{
    let mut result = Vec::new();

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            if flags.intersects(CollectFlags::ErrorIfPathDoesNotExist) {
                log::error!(log::channel!("fs"), "Failed to read directory {path:?}: {err}");
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
                        result.push(filename.into());
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
