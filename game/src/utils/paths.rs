use std::{sync::LazyLock, env, path::{Path, PathBuf}};
use crate::log;

// ----------------------------------------------
// Platform Path Handling
// ----------------------------------------------

// Platform-aware helpers for resolving game resource paths.
// Works in both dev (unbundled) and release (bundled) builds.

// Sets the current working directory to base_path.
pub fn set_default_working_directory() {
    let path = base_path();
    if let Err(err) = env::set_current_dir(path) {
        log::warning!("Failed to set default working directory: {err}");
    }
}

// Absolute path where the application runs from. Parent of assets_path.
pub fn base_path() -> &'static PathBuf {
    &CACHED_PATH_BUFS.base_path
}

pub fn base_path_str() -> &'static str {
    CACHED_PATH_STRS.base_path_str
}

// Joins base_path and the given relative path.
pub fn prepend_base_path(relative_path: impl AsRef<Path>) -> PathBuf {
    CACHED_PATH_BUFS.base_path.join(relative_path)
}

// Returns the absolute path to the game's assets directory.
// On MacOS, this will point inside `.app/Contents/Resources/assets`.
// On other platforms or in dev runs, it falls back to `./assets`.
pub fn assets_path() -> &'static PathBuf {
    &CACHED_PATH_BUFS.assets_path
}

pub fn assets_path_str() -> &'static str {
    CACHED_PATH_STRS.assets_path_str
}

// Joins assets_path and the given relative path.
pub fn prepend_assets_path(relative_path: impl AsRef<Path>) -> PathBuf {
    CACHED_PATH_BUFS.assets_path.join(relative_path)
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

struct CachedPathBufs {
    base_path: PathBuf,
    assets_path: PathBuf,
}

struct CachedPathStrs {
    base_path_str: &'static str,
    assets_path_str: &'static str,
}

// Cached on first use.
static CACHED_PATH_BUFS: LazyLock<CachedPathBufs> = LazyLock::new(|| {
    CachedPathBufs {
        base_path: cache_base_path(),
        assets_path: cache_assets_path(),
    }
});

static CACHED_PATH_STRS: LazyLock<CachedPathStrs> = LazyLock::new(|| {
    CachedPathStrs {
        base_path_str: CACHED_PATH_BUFS.base_path.to_str().unwrap(),
        assets_path_str: CACHED_PATH_BUFS.assets_path.to_str().unwrap(),
    }
});

fn cache_base_path() -> PathBuf {
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

fn cache_assets_path() -> PathBuf {
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
fn project_relative(relative_path: impl AsRef<Path>) -> PathBuf {
    // Try CARGO_MANIFEST_DIR for a stable dev path:
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(manifest_dir).join(relative_path);
    }

    // Fallback: current working directory.
    let mut dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !relative_path.as_ref().as_os_str().is_empty() {
        dir = dir.join(relative_path);
    }
    dir
}

// Internal helper to find the Resources directory when inside a MacOS bundle.
#[cfg(target_os = "macos")]
fn macos_bundle_resources_path() -> Option<PathBuf> {
    // Example: /MyGame.app/Contents/MacOS/MyGame
    let exe_path = env::current_exe().ok()?;
    let mut path = exe_path.parent()?.to_path_buf();

    for _ in 0..3 {
        if path.ends_with("MacOS") {
            let contents = path.parent()?.to_path_buf();
            if contents.ends_with("Contents") {
                return Some(contents.join("Resources"));
            }
        }
        path = path.parent()?.to_path_buf();
    }
    None
}
