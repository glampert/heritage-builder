#[cfg(all(feature = "desktop", target_os = "macos"))]
mod macos;

#[cfg(all(feature = "desktop", target_os = "macos"))]
pub use macos::*;
