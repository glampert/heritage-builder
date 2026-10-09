// Web/WASM linking fix-up.
//
// On wasm32, imgui-sys (Dear ImGui compiled from C++) leaves C library symbols
// such as malloc, vsnprintf and __assert_fail undefined. They are meant to become
// imports from the "env" module, which web/libc.js provides at runtime.
//
// Rust 1.96 stopped linking WebAssembly targets with `--allow-undefined` by default,
// so those symbols now fail to link. Opt back in for the wasm32 build only. See:
// https://blog.rust-lang.org/2026/04/04/changes-to-webassembly-targets-and-handling-undefined-symbols/

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-bins=--allow-undefined");
    }
}
