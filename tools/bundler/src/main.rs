use std::{fs, path::Path, process::Command};

// NOTE: APP_NAME must match `[package.metadata.bundle] name = ".."` in game/Cargo.toml!
const APP_NAME: &str = "Heritage Builder.app";
const ASSETS_DIR: &str = "assets";

fn main() {
    println!("🏠 Running bundler from {:?}", std::env::current_dir().unwrap());

    // Two options "debug" or "release".
    let bundle_kind = std::env::args()
        .nth(1)
        .unwrap_or_else(|| panic!("❌ Missing 'debug' or 'release' flag!"));

    let release_flag = {
        if bundle_kind == "debug" {
            None
        } else if bundle_kind == "release" {
            Some("--release")
        } else {
            panic!("❌ Invoke bundler with either 'debug' or 'release' flag!");
        }
    };

    let game_dir = Path::new("../game");
    if !game_dir.exists() {
        panic!("❌ Run this from `tools/bundler` directory or adjust path!");
    };

    println!("🧩 Building bundle for '{APP_NAME}' in {bundle_kind} mode...");

    // Step 1: Build the target executable:
    let mut build_cmd = Command::new("cargo");
    build_cmd.arg("build");
    if let Some(flag) = release_flag {
        build_cmd.arg(flag);
    }
    build_cmd.current_dir(game_dir);

    if !build_cmd.status().expect("❌ Failed to run cargo build").success() {
        panic!("❌ Cargo build failed!");
    }

    // Step 2: Bundle it:
    let mut bundle_cmd = Command::new("cargo");
    bundle_cmd.arg("bundle");
    if let Some(flag) = release_flag {
        bundle_cmd.arg(flag);
    }
    bundle_cmd.current_dir(game_dir);

    if !bundle_cmd.status().expect("❌ Failed to run cargo bundle").success() {
        panic!("❌ Cargo bundle failed!");
    }

    // Step 3: Copy assets:
    let bundle_resources_dir = Path::new("../")
        .join("target")
        .join(bundle_kind)
        .join("bundle")
        .join("osx")
        .join(APP_NAME)
        .join("Contents/Resources");

    if bundle_resources_dir.exists() {
        let assets_src = Path::new("../").join(ASSETS_DIR);
        println!("📦 Copying assets from {:?} to {:?}", assets_src, bundle_resources_dir);
        copy_dir_recursive(&assets_src, &bundle_resources_dir.join(ASSETS_DIR));
    } else {
        println!("⚠️ Resources directory ({bundle_resources_dir:?}) not found, skipping asset copy.");
    }

    println!("✅ Bundle complete!");
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    if dst.exists() {
        // Clean existing:
        fs::remove_dir_all(dst).expect("Failed to remove old directory");
    }

    let _ = fs::create_dir_all(dst);

    for entry in fs::read_dir(src)
        .unwrap_or_else(|_| panic!("❌ Failed to read dir: {src:?}"))
    {
        let entry = entry
            .unwrap_or_else(|_| panic!("❌ Failed to read dir entry for: {src:?}"));

        let file_type = entry.file_type().expect("Failed to query file type!");
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else if let Err(err) = fs::copy(entry.path(), &dst_path) {
            panic!("❌ Failed to copy from: {:?} to: {:?}. {}", entry.path(), dst_path, err);
        }
    }
}
