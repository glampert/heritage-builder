use std::fs;
use std::path::{Path, PathBuf};

// ----------------------------------------------
// collect_files / collect_sub_dirs
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq)]
pub enum CollectDirEntriesFilter {
    Files,
    SubDirs,
}

pub fn collect_files(path: &Path) -> Vec<PathBuf> {
    collect_dir_entries(path, CollectDirEntriesFilter::Files)
}

pub fn collect_sub_dirs(path: &Path) -> Vec<PathBuf> {
    collect_dir_entries(path, CollectDirEntriesFilter::SubDirs)
}

pub fn collect_dir_entries(path: &Path, filter: CollectDirEntriesFilter) -> Vec<PathBuf> {
    let mut result = Vec::new();

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("Failed to read directory '{}': {}", path.display(), err);
            return result;
        },
    };

    for entry_result in entries {
        match entry_result {
            Ok(entry) => {
                let path = entry.path();
                if path.is_file() && filter == CollectDirEntriesFilter::Files {
                    result.push(path);
                } else if path.is_dir() && filter == CollectDirEntriesFilter::SubDirs {
                    result.push(path);
                }
            },
            Err(err) => {
                eprintln!("Error reading directory entry: {}", err);
                continue;
            }
        }
    }

    result
}
