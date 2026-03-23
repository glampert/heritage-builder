use std::sync::LazyLock;
use super::*;
use crate::{
    log,
    utils::{
        mem::singleton,
        hash::{self, StringHash, PreHashedKeyMap},
    },
};

// ----------------------------------------------
// WebFileSystemBackend
// ----------------------------------------------

// Web/WASM in-memory asset cache.
//
// On the web target, all game assets (textures, configs, etc.)
// are fetched over HTTP during startup and stored in this cache.
// `file_sys::load_bytes` / `load_string` read from here instead
// of the filesystem. A file can be requested once, after which
// it is consumed and removed from the cache.

pub struct WebFileSystemBackend {
    asset_cache: PreHashedKeyMap<StringHash, AssetCacheEntry>,
}

struct AssetCacheEntry {
    path: String,
    data: Vec<u8>,
}

struct CachedPaths {
    base_path:   paths::FixedPath,
    assets_path: paths::AssetPath,
}

// Cached on first use.
static CACHED_PATHS: LazyLock<CachedPaths> = LazyLock::new(|| {
    CachedPaths {
        // On Web/WASM, all paths are relative to the web server root.
        base_path: paths::FixedPath::from_str(""),
        // On Web/WASM, assets are served alongside the WASM binary.
        assets_path: paths::AssetPath::from_str("assets"),
    }
});

impl WebFileSystemBackend {
    const fn new() -> Self {
        Self { asset_cache: hash::new_const_hash_map() }
    }

    // Normalize path separators and strip leading "./" for consistent lookups.
    fn normalize_path(path: &str) -> paths::FixedPath {
        let path_no_prefix = path.strip_prefix("./").unwrap_or(path);
        paths::FixedPath::from_str(path_no_prefix).normalized()
    }

    fn hash_path(path: impl AsRef<Path>) -> (paths::FixedPath, StringHash) {
        let original_path = path.as_ref().to_str().unwrap();
        let normalized_path = Self::normalize_path(original_path);
        let path_hash = hash::fnv1a_from_str(normalized_path.as_str());
        (normalized_path, path_hash)
    }

    fn find_asset(&self, path: impl AsRef<Path>) -> Option<&Vec<u8>> {
        let (normalized_path, path_hash) = Self::hash_path(path);

        self.asset_cache.get(&path_hash).map(|entry| {
            debug_assert!(entry.path == normalized_path.as_str());
            &entry.data
        })
    }

    fn remove_asset(&mut self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        let (normalized_path, path_hash) = Self::hash_path(path);

        self.asset_cache.remove(&path_hash).map(|entry| {
            debug_assert!(entry.path == normalized_path.as_str());
            entry.data
        })
    }

    fn insert_asset(&mut self, path: &str, data: Vec<u8>) {
        let (normalized_path, path_hash) = Self::hash_path(&path);
        let entry = AssetCacheEntry { path: normalized_path.to_string(), data };

        if self.asset_cache.insert(path_hash, entry).is_some() {
            log::error!(log::channel!("file_sys"), "Asset path collision for: '{}' (0x{:x})", path, path_hash);
        }
    }
}

impl FileSystemBackend for WebFileSystemBackend {
    fn set_working_directory(&self, _path: impl AsRef<Path>) {
        // No-op on Web/WASM — no concept of a working directory.
    }

    #[inline]
    fn base_path(&self) -> &'static paths::FixedPath {
        &CACHED_PATHS.base_path
    }

    #[inline]
    fn assets_path(&self) -> &'static paths::AssetPath {
        &CACHED_PATHS.assets_path
    }

    #[inline]
    fn exists(&self, path: impl AsRef<Path>) -> bool {
        self.find_asset(path).is_some()
    }

    #[inline]
    fn load_bytes(&mut self, path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        // NOTE: Data is consumed after one cache lookup. This avoids having to clone the data.
        self.remove_asset(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Asset not found in Web cache"))
    }

    #[inline]
    fn load_string(&mut self, path: impl AsRef<Path>) -> io::Result<String> {
        // NOTE: Data is consumed after one cache lookup. This avoids having to clone the data.
        let bytes = self.remove_asset(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Asset not found in Web cache"))?;

        String::from_utf8(bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    fn write_file(&self, _path: impl AsRef<Path>, _data: impl AsRef<[u8]>) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Direct file write not supported on Web/WASM"))
    }

    fn remove_file(&self, _path: impl AsRef<Path>) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "File removal not supported on Web/WASM"))
    }

    fn create_path(&self, _path: impl AsRef<Path>) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Directory creation not supported on Web/WASM"))
    }

    fn collect_dir_entries(&self,
                           _path: impl AsRef<Path>,
                           _flags: CollectFlags,
                           _extension: Option<&str>) -> io::Result<Vec<PathBuf>>
    {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Directory iteration not supported on Web/WASM"))
    }
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

// NOTE: Web/WASM code is single-threaded. Safe to use a global singleton.
singleton! { WEB_FS_BACKEND_SINGLETON, WebFileSystemBackend }

// ----------------------------------------------
// Web Asset Cache Preloading
// ----------------------------------------------

// Fetch all assets listed in the manifest over HTTP and populate the cache.
// This is an async function — call via wasm_bindgen_futures::spawn_local.
pub async fn preload_asset_cache(manifest_url: &str) -> Result<usize, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window()
        .ok_or("No global window object")?;

    // Fetch the manifest (JSON array of asset paths).
    let manifest_resp = JsFuture::from(window.fetch_with_str(manifest_url))
        .await
        .map_err(|e| format!("Failed to fetch manifest: {e:?}"))?;

    let manifest_resp: web_sys::Response = manifest_resp.dyn_into()
        .map_err(|_| "Manifest fetch did not return a Response")?;

    if !manifest_resp.ok() {
        return Err(format!("Manifest fetch failed with status {}", manifest_resp.status()));
    }

    let manifest_text = JsFuture::from(
        manifest_resp.text().map_err(|e| format!("Failed to get manifest text: {e:?}"))?)
        .await
        .map_err(|e| format!("Failed to read manifest text: {e:?}"))?;

    let manifest_str = manifest_text.as_string()
        .ok_or("Manifest is not a string")?;

    // Parse as JSON array of strings.
    let paths: Vec<String> = serde_json::from_str(&manifest_str)
        .map_err(|e| format!("Failed to parse asset manifest: {e}"))?;

    log::info!(log::channel!("file_sys"), "WASM: Loading {} assets from manifest...", paths.len());

    let mut loaded = 0;

    for path in &paths {
        match fetch_binary(&window, path).await {
            Ok(data) => {
                WebFileSystemBackend::get_mut().insert_asset(path, data);
                loaded += 1;
            }
            Err(err) => {
                log::error!(log::channel!("file_sys"), "WASM: Failed to fetch asset '{}': {}", path, err);
            }
        }
    }

    log::info!(log::channel!("file_sys"), "WASM: Loaded {loaded}/{} assets into cache.", paths.len());
    Ok(loaded)
}

async fn fetch_binary(window: &web_sys::Window, url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let resp = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("Fetch error: {e:?}"))?;

    let resp: web_sys::Response = resp.dyn_into()
        .map_err(|_| "Not a Response")?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let array_buffer = JsFuture::from(
        resp.array_buffer().map_err(|e| format!("array_buffer error: {e:?}"))?)
        .await
        .map_err(|e| format!("Failed to read array_buffer: {e:?}"))?;

    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}
