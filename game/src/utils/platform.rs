use strum_macros::Display;

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
