use std::{env, sync::LazyLock, path::{Path, PathBuf}};
use arrayvec::ArrayString;
use smallvec::SmallVec;
use crate::log;

// ----------------------------------------------
// FixedPathBuf
// ----------------------------------------------

// Fixed-size, no allocation alternative to PathBuf.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct FixedPathBuf<const N: usize> {
    buf: ArrayString<N>,
}

impl<const N: usize> FixedPathBuf<N> {
    const PATH_BUF_OVERFLOW: &str = "FixedPathBuf<N> Overflowed!";

    #[inline]
    pub fn new() -> Self {
        Self { buf: ArrayString::new() }
    }

    #[inline]
    pub fn from_ref(r: PathRef) -> Self {
        Self::from_str(r.as_str())
    }

    #[inline]
    pub fn from_str(s: &str) -> Self {
        let mut new = Self::new();
        new.buf.try_push_str(s).expect(Self::PATH_BUF_OVERFLOW);
        new
    }

    #[inline]
    pub fn from_path(path: &Path) -> Self {
        Self::from_str(path.to_str().expect("Invalid Path!"))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.buf.as_str()
    }

    #[inline]
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_str())
    }

    #[inline]
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.as_str())
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    #[inline]
    pub fn starts_with(&self, path: impl AsRef<str>) -> bool {
        self.buf.starts_with(path.as_ref())
    }

    #[inline]
    pub fn ends_with(&self, path: impl AsRef<str>) -> bool {
        self.buf.ends_with(path.as_ref())
    }

    #[inline]
    pub fn parent(&self) -> Option<&str> {
        self.buf.rsplit_once(SEPARATOR_CHAR).map(|(p, _)| p)
    }

    #[inline]
    pub fn file_name(&self) -> Option<&str> {
        self.buf.rsplit(SEPARATOR_CHAR).next()
    }

    #[inline]
    pub fn file_stem(&self) -> Option<&str> {
        self.file_name()
            .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem).or(Some(name)))
    }

    #[inline]
    pub fn extension(&self) -> Option<&str> {
        self.file_name()
            .and_then(|name| name.rsplit_once('.').map(|(_, ext)| ext))
    }

    #[inline]
    #[must_use]
    pub fn with_extension(&self, ext: impl AsRef<str>) -> Self {
        let mut new = FixedPathBuf::<N>::from_str(self.as_str());
        new.set_extension(ext);
        new
    }

    #[inline]
    pub fn exists(&self) -> bool {
        std::fs::exists(self).is_ok_and(|exists| exists)
    }

    #[inline]
    #[must_use]
    pub fn join(&self, path: impl AsRef<str>) -> Self {
        let mut new = FixedPathBuf::<N>::from_str(self.as_str());
        new.push(path);
        new
    }

    pub fn push(&mut self, path: impl AsRef<str>) -> &mut Self {
        if !self.buf.is_empty() && !self.buf.ends_with(SEPARATOR_CHAR) {
            self.buf.try_push(SEPARATOR_CHAR).expect(Self::PATH_BUF_OVERFLOW);
        }

        self.buf.try_push_str(path.as_ref()).expect(Self::PATH_BUF_OVERFLOW);
        self
    }

    pub fn pop(&mut self) -> bool {
        if let Some(parent) = self.parent() {
            let new = FixedPathBuf::<N>::from_str(parent);
            self.buf = new.buf;
            true
        } else if !self.buf.is_empty() {
            self.buf.clear();
            true
        } else {
            false
        }
    }

    pub fn set_extension(&mut self, ext: impl AsRef<str>) -> &mut Self {
        let stem = match self.file_stem() {
            Some(s) => s,
            None => return self,
        };

        let parent = self.parent();
        let mut new = FixedPathBuf::<N>::new();

        if let Some(p) = parent {
            new.buf.try_push_str(p).expect(Self::PATH_BUF_OVERFLOW);
            new.buf.try_push(SEPARATOR_CHAR).expect(Self::PATH_BUF_OVERFLOW);
        }

        new.buf.try_push_str(stem).expect(Self::PATH_BUF_OVERFLOW);

        if !ext.as_ref().is_empty() {
            new.buf.try_push('.').expect(Self::PATH_BUF_OVERFLOW);
            new.buf.try_push_str(ext.as_ref()).expect(Self::PATH_BUF_OVERFLOW);
        }

        self.buf = new.buf;
        self
    }

    pub fn normalize(&mut self) -> &mut Self {
        let new = {
            let mut stack = SmallVec::<[&str; 128]>::new();

            // NOTE: Split by standard forward slash and backslash (Windows-style),
            // so the following pass with normalize all to forward slashes.
            for comp in self.buf.split(&[SEPARATOR_CHAR, '\\']) {
                match comp {
                    "" | "." => {}
                    ".." => {
                        stack.pop();
                    }
                    c => stack.push(c),
                }
            }

            let mut new = FixedPathBuf::<N>::new();

            for (i, comp) in stack.iter().enumerate() {
                if i > 0 {
                    new.buf.try_push(SEPARATOR_CHAR).expect(Self::PATH_BUF_OVERFLOW);
                }
                new.buf.try_push_str(comp).expect(Self::PATH_BUF_OVERFLOW);
            }

            new
        };

        self.buf = new.buf;
        self
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

// FixedPathBuf => &Path
impl<const N: usize> AsRef<Path> for FixedPathBuf<N> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

// FixedPathBuf => &str
impl<const N: usize> AsRef<str> for FixedPathBuf<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> std::fmt::Display for FixedPathBuf<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ----------------------------------------------
// PathRef
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct PathRef<'a> {
    inner: &'a str,
}

impl<'a> PathRef<'a> {
    #[inline]
    pub const fn from_str(s: &'a str) -> Self {
        Self { inner: s }
    }

    #[inline]
    pub fn from_path(path: &'a Path) -> Self {
        Self::from_str(path.to_str().expect("Invalid Path!"))
    }

    #[inline]
    pub fn as_str(&self) -> &'a str {
        self.inner
    }

    #[inline]
    pub fn as_path(&self) -> &'a Path {
        Path::new(self.inner)
    }

    #[inline]
    pub fn to_path_buf(self) -> PathBuf {
        PathBuf::from(self.as_str())
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn starts_with(&self, path: impl AsRef<str>) -> bool {
        self.inner.starts_with(path.as_ref())
    }

    #[inline]
    pub fn ends_with(&self, path: impl AsRef<str>) -> bool {
        self.inner.ends_with(path.as_ref())
    }

    #[inline]
    pub fn parent(&self) -> Option<Self> {
        self.inner.rsplit_once(SEPARATOR_CHAR).map(|(p, _)| Self::from_str(p))
    }

    #[inline]
    pub fn file_name(&self) -> Option<&'a str> {
        self.inner.rsplit('/').next()
    }

    #[inline]
    pub fn file_stem(&self) -> Option<&'a str> {
        self.file_name()
            .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem).or(Some(name)))
    }

    #[inline]
    pub fn extension(&self) -> Option<&'a str> {
        self.file_name()
            .and_then(|name| name.rsplit_once('.').map(|(_, ext)| ext))
    }

    #[inline]
    pub fn exists(&self) -> bool {
        std::fs::exists(self).is_ok_and(|exists| exists)
    }

    #[inline]
    #[must_use]
    pub fn with_extension<const N: usize>(&self, ext: impl AsRef<str>) -> FixedPathBuf<N> {
        let mut new = FixedPathBuf::<N>::from_str(self.as_str());
        new.set_extension(ext);
        new
    }

    #[inline]
    #[must_use]
    pub fn join<const N: usize>(&self, path: impl AsRef<str>) -> FixedPathBuf<N> {
        let mut new = FixedPathBuf::<N>::from_str(self.as_str());
        new.push(path);
        new
    }
}

// FixedPathBuf.into() => PathRef
impl<'a, const N: usize> From<&'a FixedPathBuf<N>> for PathRef<'a> {
    fn from(p: &'a FixedPathBuf<N>) -> Self {
        PathRef::from_str(p.as_str())
    }
}

// PathRef => &Path
impl<'a> AsRef<Path> for PathRef<'a> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

// PathRef => &str
impl<'a> AsRef<str> for PathRef<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a> std::fmt::Display for PathRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ----------------------------------------------
// Type Aliases
// ----------------------------------------------

pub type FixedPath = FixedPathBuf<1024>;
pub type AssetPath = FixedPathBuf<1024>;

// ----------------------------------------------
// Platform Path Handling
// ----------------------------------------------

// Platform-aware helpers for resolving game resource paths.
// Works in both dev (unbundled) and release (bundled) builds.
// Path separator is always forward slash (/) for both Windows and Unix paths.

pub const SEPARATOR_CHAR: char = '/';
pub const SEPARATOR_STR:  &str = "/";

// Absolute path where the application runs from. Parent of assets_path.
pub fn base_path() -> &'static FixedPath {
    &CACHED_PATHS.base_path
}

// Returns the absolute path to the game's assets directory.
// On MacOS, this will point inside `.app/Contents/Resources/assets`.
// On other platforms or in dev runs, it falls back to `./assets`.
pub fn assets_path() -> &'static AssetPath {
    &CACHED_PATHS.assets_path
}

// Sets the current working directory to base_path.
pub fn set_default_working_directory() {
    let path = base_path();
    if let Err(err) = env::set_current_dir(path) {
        log::warning!("Failed to set default working directory: {err}");
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

struct CachedPaths {
    base_path: FixedPath,
    assets_path: AssetPath,
}

// Cached on first use.
static CACHED_PATHS: LazyLock<CachedPaths> = LazyLock::new(|| {
    CachedPaths {
        base_path: find_base_path(),
        assets_path: find_assets_path(),
    }
});

fn find_base_path() -> FixedPath {
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

fn find_assets_path() -> AssetPath {
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
fn project_relative(relative_path: &str) -> FixedPath {
    // Try CARGO_MANIFEST_DIR for a stable dev path:
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return FixedPath::from_str(&manifest_dir).join(relative_path);
    }

    // Fallback: current working directory.
    let mut dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !relative_path.is_empty() {
        dir = dir.join(relative_path);
    }

    FixedPath::from_path(&dir)
}

// Internal helper to find the Resources directory when inside a MacOS bundle.
#[cfg(target_os = "macos")]
fn macos_bundle_resources_path() -> Option<FixedPath> {
    // Example: /MyGame.app/Contents/MacOS/MyGame
    let exe_path = env::current_exe().ok()?;
    let mut path = exe_path.parent()?.to_path_buf();

    for _ in 0..3 {
        if path.ends_with("MacOS") {
            let contents = FixedPath::from_path(path.parent()?);
            if contents.ends_with("Contents") {
                return Some(contents.join("Resources"));
            }
        }
        path = path.parent()?.to_path_buf();
    }

    None
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path() {
        let mut p: FixedPathBuf<64> = FixedPathBuf::from_str("a/./b/../c");
        p.normalize();
        assert_eq!(p.as_str(), "a/c");

        p = FixedPathBuf::from_str("a/b/c/text.txt");
        p.normalize();
        assert_eq!(p.as_str(), "a/b/c/text.txt"); // expect unchanged.

        p = FixedPathBuf::from_str("a\\b/c\\d/file.txt");
        p.normalize();
        assert_eq!(p.as_str(), "a/b/c/d/file.txt"); // expect mixed path separators fixed.
    }

    #[test]
    fn path_building() {
        let mut p: FixedPathBuf<64> = FixedPathBuf::from_str("assets");
        p.push("textures");
        p.push("wall.png");

        assert_eq!(p.as_str(), "assets/textures/wall.png");
        assert_eq!(p.extension(), Some("png"));
        assert_eq!(p.file_name(), Some("wall.png"));
    }

    #[test]
    fn parent_path() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("a/b/c.txt");
        assert_eq!(p.parent(), Some("a/b"));
    }

    #[test]
    fn pop_component() {
        let mut p: FixedPathBuf<64> = FixedPathBuf::from_str("a/b/c");

        p.pop();
        assert_eq!(p.as_str(), "a/b");

        p.pop();
        assert_eq!(p.as_str(), "a");

        p.pop();
        assert!(p.is_empty());
        assert_eq!(p.as_str(), "");
    }

    #[test]
    fn extension_change() {
        let mut p: FixedPathBuf<64> = FixedPathBuf::from_str("a/b/c.png");
        p.set_extension("dds");
        assert_eq!(p.as_str(), "a/b/c.dds");
    }

    #[test]
    fn remove_extension() {
        let mut p: FixedPathBuf<64> = FixedPathBuf::from_str("a/test.png");
        p.set_extension("");
        assert_eq!(p.as_str(), "a/test");
    }

    #[test]
    fn file_name_basic() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("assets/tree.png");
        assert_eq!(p.file_name(), Some("tree.png"));
    }

    #[test]
    fn file_name_nested() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("a/b/c/d.txt");
        assert_eq!(p.file_name(), Some("d.txt"));
    }

    #[test]
    fn file_name_no_parent() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("file.txt");
        assert_eq!(p.file_name(), Some("file.txt"));
    }

    #[test]
    fn file_name_directory_path() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("assets/textures");
        assert_eq!(p.file_name(), Some("textures"));
    }

    #[test]
    fn file_stem_basic() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("assets/tree.png");
        assert_eq!(p.file_stem(), Some("tree"));
    }

    #[test]
    fn file_stem_multiple_dots() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("archive.tar.gz");
        assert_eq!(p.file_stem(), Some("archive.tar"));
    }

    #[test]
    fn file_stem_no_extension() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("README");
        assert_eq!(p.file_stem(), Some("README"));
    }

    #[test]
    fn extension_basic() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("assets/tree.png");
        assert_eq!(p.extension(), Some("png"));
    }

    #[test]
    fn extension_multiple_dots() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("archive.tar.gz");
        assert_eq!(p.extension(), Some("gz"));
    }

    #[test]
    fn extension_none() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("LICENSE");
        assert_eq!(p.extension(), None);
    }

    #[test]
    fn extension_dot_file() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str(".gitignore");
        assert_eq!(p.extension(), Some("gitignore"));
    }

    #[test]
    fn extension_nested_path() {
        let p: FixedPathBuf<64> = FixedPathBuf::from_str("assets/shaders/basic.vert");
        assert_eq!(p.file_name(), Some("basic.vert"));
        assert_eq!(p.file_stem(), Some("basic"));
        assert_eq!(p.extension(), Some("vert"));
    }
}
