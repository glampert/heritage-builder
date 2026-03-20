// Web/WASM in-memory asset cache.
//
// On the web target, all game assets (textures, configs, etc.) are fetched
// over HTTP during startup and stored in this thread-local cache.
// `file_sys::load_bytes` / `load_string` read from here instead of
// the filesystem.

use std::collections::HashMap;
use std::cell::RefCell;
use std::path::Path;

use crate::log;

thread_local! {
    static ASSET_CACHE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

// Insert an asset into the cache. Called during startup asset loading.
pub fn insert(path: &str, data: Vec<u8>) {
    ASSET_CACHE.with(|cache| {
        cache.borrow_mut().insert(normalize_path(path), data);
    });
}

// Look up an asset by path. Returns a clone of the data.
pub fn get(path: impl AsRef<Path>) -> Option<Vec<u8>> {
    ASSET_CACHE.with(|cache| {
        let p = path.as_ref();
        cache.borrow().get(&normalize_path(p.to_str().unwrap())).cloned()
    })
}

// Returns the number of cached assets.
pub fn len() -> usize {
    ASSET_CACHE.with(|cache| cache.borrow().len())
}

// Normalize path separators and strip leading "./" for consistent lookups.
fn normalize_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    p.strip_prefix("./").unwrap_or(&p).to_string()
}

// Fetch all assets listed in the manifest over HTTP and populate the cache.
// This is an async function — call via wasm_bindgen_futures::spawn_local.
pub async fn load_from_manifest(manifest_url: &str) -> Result<usize, String> {
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

    log::info!(log::channel!("fs"),
               "WASM: Loading {} assets from manifest...", paths.len());

    let mut loaded = 0;

    for path in &paths {
        match fetch_binary(&window, path).await {
            Ok(data) => {
                insert(path, data);
                loaded += 1;
            }
            Err(err) => {
                log::error!(log::channel!("fs"),
                            "WASM: Failed to fetch asset '{}': {}", path, err);
            }
        }
    }

    log::info!(log::channel!("fs"),
               "WASM: Loaded {loaded}/{} assets into cache.", paths.len());

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
