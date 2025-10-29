// ----------------------------------------------
// Platform Path Handling
// ----------------------------------------------

// Platform-aware helpers for resolving game resource paths.
// Works in both dev (unbundled) and distributed (bundled) builds.
pub mod paths {
    use std::{sync::LazyLock, env, path::{Path, PathBuf}};
    use crate::log;

    pub fn set_default_working_dir() {
        let mut path = assets_dir().clone();
        path.pop(); // E.g. back to `Contents/Resources` on MacOS.

        if let Err(err) = env::set_current_dir(path) {
            log::warn!("Failed to set default working directory: {err}");
        }
    }

    // Returns the absolute path to the game's assets directory.
    // On MacOS, this will point inside `.app/Contents/Resources/assets`.
    // On other platforms or in dev runs, it falls back to `./assets`.
    pub fn assets_dir() -> &'static PathBuf {
        &CACHED_ASSETS_DIR
    }

    // Resolves a path within the assets directory.
    pub fn asset_path(rel: impl AsRef<Path>) -> PathBuf {
        CACHED_ASSETS_DIR.join(rel)
    }

    // Cached on first use.
    static CACHED_ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(find_assets_dir);

    fn find_assets_dir() -> PathBuf {
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
    fn project_relative(rel: impl AsRef<Path>) -> PathBuf {
        // Try CARGO_MANIFEST_DIR for a stable dev path:
        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            return PathBuf::from(manifest_dir).join(rel);
        }

        // Fallback: current working directory.
        env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(rel)
    }
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

    let _ = fs::create_dir("logs");

    unsafe {
        let saved_fd = dup(STDERR_FILENO);
        let file = OpenOptions::new().create(true)
                                     .write(true)
                                     .truncate(true)
                                     .open(Path::new("logs").join(filename))
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
