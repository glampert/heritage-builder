use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const GAME_CRATE:    &str = "HeritageBuilder";
const ASSETS_DIR:    &str = "assets";
const WEB_DIR:       &str = "web";
const PKG_DIR:       &str = "web/pkg";
const MANIFEST_FILE: &str = "web/asset_manifest.json";

fn main() {
    let project_root = find_project_root();
    println!("🌐 Web Builder — project root: {}", project_root.display());

    let mode = env::args()
        .nth(1)
        .unwrap_or_else(|| "debug".to_string());

    let release = match mode.as_str() {
        "debug" => false,
        "release" => true,
        other => panic!("❌ Unknown mode '{other}'. Use 'debug' or 'release'."),
    };

    // Step 1: Generate the asset manifest.
    println!("\n📋 Generating asset manifest...");
    generate_asset_manifest(&project_root);

    // Step 2: Build the WASM binary.
    println!("\n🔨 Building WASM binary ({mode})...");
    cargo_build_wasm(&project_root, release);

    // Step 3: Run wasm-bindgen.
    println!("\n🔗 Running wasm-bindgen...");
    run_wasm_bindgen(&project_root, release);

    // Step 4: Optionally run wasm-opt on release builds.
    if release {
        println!("\n⚡ Running wasm-opt...");
        run_wasm_opt(&project_root);
    }

    println!("\n✅ Web build complete!");
    println!("   Output: {}/", PKG_DIR);
    println!("   Assets: {}/assets/", WEB_DIR);
    println!("\n   To test locally:");
    println!("   cd {} && python3 -m http.server 8080", project_root.join(WEB_DIR).display());
}

// -----------------------------------------------
// Step 1: Asset Manifest
// -----------------------------------------------

fn generate_asset_manifest(project_root: &Path) {
    let assets_src = project_root.join(ASSETS_DIR);
    let web_assets_dst = project_root.join(WEB_DIR).join("assets");

    if !assets_src.exists() {
        panic!("❌ Assets directory not found: {}", assets_src.display());
    }

    // Collect all asset file paths (relative to web/ root, e.g. "assets/configs/game/configs.json").
    let mut paths: Vec<String> = Vec::new();
    collect_asset_paths(&assets_src, &assets_src, &mut paths);
    paths.sort();

    println!("   Found {} assets.", paths.len());

    // Write the manifest.
    let manifest_path = project_root.join(MANIFEST_FILE);
    fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();

    let json = serde_json::to_string_pretty(&paths)
        .expect("Failed to serialize asset manifest");

    fs::write(&manifest_path, &json)
        .unwrap_or_else(|e| panic!("❌ Failed to write manifest: {e}"));

    println!("   Wrote manifest: {}", manifest_path.display());

    // Sync assets to web/assets/ (copy or symlink).
    sync_assets(&assets_src, &web_assets_dst);
}

fn collect_asset_paths(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("❌ Failed to read directory {}: {e}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = entry.file_name();

        // Skip hidden files (.DS_Store, etc.).
        if file_name.to_string_lossy().starts_with('.') {
            continue;
        }

        if path.is_dir() {
            collect_asset_paths(base, &path, out);
        } else {
            // Path relative to the assets parent (project root), so it starts with "assets/".
            let rel = path.strip_prefix(base.parent().unwrap())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
}

fn sync_assets(src: &Path, dst: &Path) {
    // Use a symlink if possible (avoids large dir copy during development).
    if dst.exists() || dst.is_symlink() {
        // Check if it's already a symlink to the right place.
        if dst.is_symlink() {
            if let Ok(target) = fs::read_link(dst) && target == src {
                println!("   Assets symlink already up to date.");
                return;
            }
            fs::remove_file(dst).unwrap();
        } else {
            fs::remove_dir_all(dst).unwrap();
        }
    }

    // Create parent dir.
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    // Try symlink first, fall back to copy.
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(src, dst).is_ok() {
            println!("   Symlinked assets: {} -> {}", dst.display(), src.display());
            return;
        }
    }

    println!("   Copying assets to {}...", dst.display());
    copy_dir_recursive(src, dst);
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();

    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name();

        if file_name.to_string_lossy().starts_with('.') {
            continue;
        }

        let dst_path = dst.join(&file_name);

        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            fs::copy(entry.path(), &dst_path)
                .unwrap_or_else(|e| panic!("❌ Copy failed: {} -> {}: {e}",
                    entry.path().display(), dst_path.display()));
        }
    }
}

// -----------------------------------------------
// Step 2: Cargo Build
// -----------------------------------------------

fn cargo_build_wasm(project_root: &Path, release: bool) {
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
       .arg("--target").arg("wasm32-unknown-unknown")
       .arg("--no-default-features")
       .arg("--features").arg("web")
       .arg("-p").arg(GAME_CRATE);

    if release {
        cmd.arg("--release");
    }

    // Set up clang for imgui-sys cross-compilation.
    let llvm_clang = PathBuf::from("/opt/homebrew/opt/llvm/bin/clang++");
    let wasi_sysroot = PathBuf::from("/opt/homebrew/opt/wasi-libc/share/wasi-sysroot");

    if llvm_clang.exists() {
        cmd.env("CXX_wasm32_unknown_unknown", &llvm_clang);

        if wasi_sysroot.exists() {
            let flags = format!(
                "--sysroot={} -I{}/include/wasm32-wasi",
                wasi_sysroot.display(),
                wasi_sysroot.display(),
            );
            cmd.env("CXXFLAGS_wasm32_unknown_unknown", &flags);
        } else {
            println!("   ⚠️  wasi-libc sysroot not found at {}", wasi_sysroot.display());
            println!("   Install it: brew install wasi-libc");
        }

        // Tell the cc crate not to link a C++ stdlib — imgui is compiled with
        // -fno-exceptions -fno-rtti and doesn't need it.
        cmd.env("CXXSTDLIB_wasm32_unknown_unknown", "");
    } else {
        println!("   ⚠️  LLVM clang++ not found at {}", llvm_clang.display());
        println!("   Install it: brew install llvm");
    }

    cmd.current_dir(project_root);

    let status = cmd.status().expect("❌ Failed to run cargo build");
    if !status.success() {
        panic!("❌ Cargo build failed!");
    }
}

// -----------------------------------------------
// Step 3: wasm-bindgen
// -----------------------------------------------

fn run_wasm_bindgen(project_root: &Path, release: bool) {
    let profile = if release { "release" } else { "debug" };

    let wasm_path = project_root
        .join("target/wasm32-unknown-unknown")
        .join(profile)
        .join(format!("{GAME_CRATE}.wasm"));

    if !wasm_path.exists() {
        panic!("❌ WASM binary not found: {}", wasm_path.display());
    }

    let out_dir = project_root.join(PKG_DIR);
    fs::create_dir_all(&out_dir).unwrap();

    let status = Command::new("wasm-bindgen")
        .arg(&wasm_path)
        .arg("--out-dir").arg(&out_dir)
        .arg("--target").arg("web")
        .arg("--no-typescript") // Skip generating typescript bindings. We use bootstrap.js to run our wasm binary, plain JS.
        .status()
        .expect("❌ Failed to run wasm-bindgen. Install it: cargo install wasm-bindgen-cli");

    if !status.success() {
        panic!("❌ wasm-bindgen failed!");
    }

    println!("   Output: {}", out_dir.display());

    // Post-process: inject libc.js setWasm() call into the generated JS.
    // wasm-bindgen's __wbg_finalize_init sets `wasm = instance.exports` then calls
    // `wasm.__wbindgen_start()` (which runs Rust main()). We need libc.js to have
    // the WASM exports *before* main() runs, so we inject a setWasm() call between
    // the two lines.
    patch_generated_js(&out_dir);
}

// Patch generate HeritageBuilder.js to load libc.js, which contains missing C library wrappers used by ImGui.
// NOTE: If we move away from ImGui in the future this shouldn't be necessary anymore.
fn patch_generated_js(pkg_dir: &Path) {
    let js_path = pkg_dir.join(format!("{GAME_CRATE}.js"));
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|e| panic!("❌ Failed to read {}: {e}", js_path.display()));

    // Add import for libc.js setWasm at the top (after the existing env imports).
    let js = if !js.contains("import { setWasm }") {
        js.replacen(
            "\nfunction __wbg_get_imports()",
            "\nimport { setWasm as __envSetWasm } from \"env\"\n\nfunction __wbg_get_imports()",
            1,
        )
    } else {
        js
    };

    // Inject __envSetWasm(wasm) before __wbindgen_start().
    let js = js.replace(
        "wasm.__wbindgen_start();",
        "__envSetWasm(wasm);\n    wasm.__wbindgen_start();",
    );

    fs::write(&js_path, &js)
        .unwrap_or_else(|e| panic!("❌ Failed to write {}: {e}", js_path.display()));

    println!("   Patched {} with libc.js integration.", js_path.display());
}

// -----------------------------------------------
// Step 4: wasm-opt (release only)
// -----------------------------------------------

fn run_wasm_opt(project_root: &Path) {
    let wasm_bg = project_root
        .join(PKG_DIR)
        .join(format!("{GAME_CRATE}_bg.wasm"));

    if !wasm_bg.exists() {
        println!("   ⚠️  Skipping wasm-opt: {} not found.", wasm_bg.display());
        return;
    }

    let status = Command::new("wasm-opt")
        .arg(&wasm_bg)
        .arg("-O2")
        .arg("-o").arg(&wasm_bg)
        .status();

    match status {
        Ok(s) if s.success() => println!("   Optimized: {}", wasm_bg.display()),
        Ok(_)  => println!("   ⚠️  wasm-opt failed (non-zero exit). Continuing without optimization."),
        Err(_) => println!("   ⚠️  wasm-opt not found. Install it: brew install binaryen"),
    }
}

// -----------------------------------------------
// Helpers
// -----------------------------------------------

fn find_project_root() -> PathBuf {
    // Look for workspace Cargo.toml by walking up from current dir.
    let mut dir = env::current_dir().expect("Failed to get current directory");

    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let contents = fs::read_to_string(&cargo_toml).unwrap();
            if contents.contains("[workspace]") {
                return dir;
            }
        }

        if !dir.pop() {
            // Fallback: assume we're run from the project root or tools/web-builder.
            let cwd = env::current_dir().unwrap();
            if cwd.join("Cargo.toml").exists() && cwd.join("game").exists() {
                return cwd;
            }
            panic!("❌ Could not find workspace root. Run from the project root directory.");
        }
    }
}
