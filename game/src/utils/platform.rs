use strum_macros::Display;

// ----------------------------------------------
// Platform Path Handling
// ----------------------------------------------

// Platform-aware helpers for resolving game resource paths.
// Works in both dev (unbundled) and distributed (bundled) builds.
pub mod paths {
    use std::{sync::LazyLock, env, path::{Path, PathBuf}};
    use crate::log;

    // Sets the current working directory to base_dir.
    pub fn set_default_working_dir() {
        let path = base_dir();
        if let Err(err) = env::set_current_dir(path) {
            log::warn!("Failed to set default working directory: {err}");
        }
    }

    // Absolute path where the application runs from. Parent of assets_dir.
    pub fn base_dir() -> &'static PathBuf {
        &CACHED_BASE_DIR
    }

    // Joins base_dir and the given relative path.
    pub fn base_path(relative_path: impl AsRef<Path>) -> PathBuf {
        CACHED_BASE_DIR.join(relative_path)
    }

    // Returns the absolute path to the game's assets directory.
    // On MacOS, this will point inside `.app/Contents/Resources/assets`.
    // On other platforms or in dev runs, it falls back to `./assets`.
    pub fn assets_dir() -> &'static PathBuf {
        &CACHED_ASSETS_DIR
    }

    // Resolves a path within the assets directory.
    pub fn asset_path(relative_path: impl AsRef<Path>) -> PathBuf {
        CACHED_ASSETS_DIR.join(relative_path)
    }

    // Cached on first use.
    static CACHED_BASE_DIR:   LazyLock<PathBuf> = LazyLock::new(cache_base_dir);
    static CACHED_ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(cache_assets_dir);

    fn cache_base_dir() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(bundle_resources) = macos_bundle_resources_dir() {
                if bundle_resources.exists() {
                    return bundle_resources;
                }
            }
        }

        // Default for dev / non-MacOS.
        project_relative("")
    }

    fn cache_assets_dir() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(bundle_resources) = macos_bundle_resources_dir() {
                if bundle_resources.exists() {
                    return bundle_resources.join("assets");
                }
            }
        }

        // Default for dev / non-MacOS.
        project_relative("assets")
    }

    // Internal helper to find the Resources directory when inside a MacOS bundle.
    #[cfg(target_os = "macos")]
    fn macos_bundle_resources_dir() -> Option<PathBuf> {
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
}

// ----------------------------------------------
// Build Profile / App Bundle Detection
// ----------------------------------------------

#[derive(Copy, Clone, Display, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Copy, Clone, Display, PartialEq, Eq)]
pub enum RunEnvironment {
    Standalone,
    MacOSAppBundle,
}

pub fn build_profile() -> BuildProfile {
    if cfg!(debug_assertions) {
        BuildProfile::Debug
    } else {
        BuildProfile::Release
    }
}

pub fn run_environment() -> RunEnvironment {
    #[cfg(target_os = "macos")]
    {
        // Example: /Applications/MyGame.app/Contents/MacOS/MyGame
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                if exe_dir
                    .parent()
                    .and_then(|contents| contents.parent())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
                    .is_some()
                {
                    return RunEnvironment::MacOSAppBundle;
                }
            }
        }
    }

    RunEnvironment::Standalone
}

// ----------------------------------------------
// macos_redirect_stderr()
// ----------------------------------------------

// Using this to deal with some TTY spam from the OpenGL loader on MacOS.
#[cfg(target_os = "macos")]
pub fn macos_redirect_stderr<F, R>(f: F, filename: &str) -> R
    where F: FnOnce() -> R
{
    use std::{fs::{self, OpenOptions}, path::Path, os::unix::io::AsRawFd};
    use libc::{close, dup, dup2, STDERR_FILENO};

    let logs_dir = crate::log::logs_dir();
    let _ = fs::create_dir(&logs_dir);

    unsafe {
        let saved_fd = dup(STDERR_FILENO);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(Path::new(&logs_dir).join(filename))
            .expect("Failed to open stderr log file!");
        dup2(file.as_raw_fd(), STDERR_FILENO);
        let result = f();
        dup2(saved_fd, STDERR_FILENO);
        close(saved_fd);
        result
    }
}

#[cfg(not(target_os = "macos"))]
pub fn macos_redirect_stderr<F, R>(f: F, _filename: &str) -> R
    where F: FnOnce() -> R
{
    f() // No-op on non-MacOS
}
