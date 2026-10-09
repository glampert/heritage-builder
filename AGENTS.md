# AGENTS.md

This file provides guidance to AI coding agents (Claude Code, Codex, Cursor, etc.) when working with code in this repository.

## Project

2D isometric city builder in Rust (Pharaoh/Caesar-inspired), with custom renderers: OpenGL or Wgpu on desktop, Wgpu on web. Cargo workspace, edition 2024.

## Common commands

Desktop build/run (default feature set):
- `cargo run -p HeritageBuilder` — debug
- `cargo run -p HeritageBuilder --release`
- `cargo build -p HeritageBuilder --release`

MacOS `.app` bundle (wraps `cargo bundle` + asset copy):
- `./bundle.sh` / `./bundle.sh release`
- `./bundle.sh --install [release]` installs `cargo-bundle` first
- Output: `target/<mode>/bundle/osx/Heritage Builder.app`

Web / WASM:
- `./web.sh --build [release] --serve` (release adds `wasm-opt`)
- Equivalent: `cargo run -p web-builder [-- release]`, then `cd web && python3 -m http.server 8080`
- Requires switching features: crates are `default = ["desktop"]`; the `web-builder` tool drives the `web` feature.
- Toolchain for imgui-sys: Homebrew `llvm` + `wasi-libc` on MacOS, or set `WASI_SDK_PATH` to a wasi-sdk 30–32 install (Linux/CI; 33+ headers reject `wasm32-unknown-unknown`).
- `crates/launcher/build.rs` passes `--allow-undefined` to the wasm linker: imgui-sys's C library symbols are imported from `env` (provided by `web/libc.js`), and Rust 1.96+ no longer allows undefined symbols by default. Don't remove it.
- `web-builder` regenerates `web/asset_manifest.json` from `assets/`. The web runner preloads only the files listed there, so after adding or removing assets, rebuild web and commit the updated manifest.

Tests — every integration test in `crates/game/tests/` (`sim_cmds`, `search_graph`, `unit_tasks`, `campaign`, `house_consumption`) uses a **custom harness** (`harness = false`):
- `cargo test -p game --test <name>` runs one file; `cargo test -p game` runs them all.
- Add cases by editing the `test_utils::run_tests(&[...])` list in the relevant file, e.g. [sim_cmds.rs](crates/game/tests/sim_cmds.rs). The harness does not accept filter args.
- The harness calls `setup()` once on the main thread because several globals use `SingleThreadStatic` and will assert if touched from a different thread. Do not parallelize tests.

CI — [.github/workflows/ci.yml](.github/workflows/ci.yml) on every push/PR: desktop build + `cargo test --workspace` on Ubuntu and macOS, plus a release web build via `web-builder`. Latest stable Rust; warnings don't fail the build. The Ubuntu job installs ALSA, X11 and Wayland dev packages for glfw/kira.

Save-load smoke test (verifies save-file compatibility):
- Sample saves are committed only as `saves/sample_saves.zip`; run `unzip saves/sample_saves.zip` first to extract them into `saves/`.
- `cargo run -p HeritageBuilder -- --smoke-test-saves`
- Run mode of the game binary (see `SaveSmokeTest` in [game_loop.rs](crates/game/src/game_loop.rs)): loads every file in `saves/` in sequence, ticks each ~10s, **panics on any failed load**, then quits with exit 0 on success. Autosave is disabled in this mode so saves aren't overwritten.
- Run it after changing any serialized type (`GameSession` and its fields). See [Saves](#saves) for the versioning/migration workflow.

## Workspace layout

- `crates/launcher` — binary crate `HeritageBuilder`; `main()` is just `runner::run::<GameLoop>()`. This is the only executable target for the game.
- `crates/common` — platform-agnostic utilities (`coords`, `time`, `hash`, `mem`, `callback`, `fixed_string`) plus shared macros (`bitflags_with_display!`, `name_of!`).
- `crates/engine` — platform, rendering, UI, sound, save, runner. `engine::Engine` owns platform/renderer/UI/sound subsystems.
- `crates/game` — all gameplay: simulation, world, tilemap, units, buildings, props, pathfind, menus, debug.
- `crates/proc_macros` — `#[derive(DrawDebugUi)]` for auto-generating ImGui debug panels on config structs.
- `crates/tools/bundler` — driver for `cargo bundle` + assets (used by `bundle.sh`).
- `crates/tools/web-builder` — WASM build driver (used by `web.sh`).
- `crates/tools/save_migration_scripts` — Python scripts (`v<N>_to_v<N+1>.py`) that migrate save JSON between format versions. Not a Cargo crate.
- `crates/tools/debug` — `macos_run_heap_debug.sh`, a MacOS heap-corruption debugging wrapper. Not a Cargo crate.
- `assets/` — `configs/{game,units,props,buildings,ui}`, `tiles/`, `sounds/`, `fonts/`, `ui/`. Copied into bundle.
- `saves/` — Save games (serde JSON), including `autosave.json`. Git-ignored except `sample_saves.zip` (sample maps such as `test_map_64x64.json`, `test_map_128x128.json`, `tiny_island.json`).
- `web/` — WASM host page (`index.html`, JS glue, `asset_manifest.json`).

## Feature gating: desktop vs. web

Every crate exposes `desktop` (default) and `web` features that fan out through the workspace. When touching `common`, `engine`, or `game`, keep both targets compiling.

- Desktop pulls in: `gl`, `glfw`, `glutin`, `kira` (audio), `rayon`, MacOS `objc2-*`.
- Web pulls in: `web-sys`, `js-sys`, `wasm-bindgen`, `web-time`, `console_error_panic_hook`.
- Platform-specific code lives under `engine/src/platform/{desktop,web}` and `engine/src/runner/{desktop.rs,web.rs}`. Don't import platform crates from `common` or `game`; route through `engine`.

## Runtime architecture

Entry point flow: `launcher::main` → `engine::runner::run::<GameLoop>()` → platform `Runner` (desktop synchronous loop vs. browser `requestAnimationFrame`) → `GameLoop::start` → per-frame `GameLoop::update`.

Key types:
- `engine::Engine` — singleton holding platform, renderer, UI (ImGui), sound (Kira), save system. Passed by `&mut` into game code each frame.
- `engine::runner::RunLoop` trait — implemented by `game::GameLoop`. The engine drives it; `GameLoop` does not own the main loop.
- `game::GameLoop` ([game_loop.rs](crates/game/src/game_loop.rs)) — top-level game state holder. Owns the boxed `GameSession` plus command queue, autosave timer, and frame stats.
- `game::session::GameSession` ([session.rs](crates/game/src/session.rs)) — fully serializable game state: camera, tile map, world, simulation, systems. Save/load round-trips through this.
- `game::sim::Simulation` ([sim/mod.rs](crates/game/src/sim/mod.rs)) — core logic. Deterministic: owns a PCG64 RNG seeded from `SimConfigs::random_seed`. Has separate `update_timer` (active) and `paused_update_timer` (paused-state ticks). Fixed-step ticks: the sim runs on its own timer, not every frame — but unit navigation does run every frame for smooth movement.
- `game::sim::SimContext` ([sim/context.rs](crates/game/src/sim/context.rs)) — **stack-local bundle of `RawPtr`s** to World/TileMap/RNG/etc., built via `make_update_context_mut!` at the top of an update. Raw pointers are deliberate: the context is a call-stack-scoped view and must never outlive the update frame. Don't store a `SimContext` in a struct field.
- `game::world::World` ([world/mod.rs](crates/game/src/world/mod.rs)) — generational-arena `SpawnPool`s: one per building archetype, plus unit and prop pools. Iteration yields only spawned entries. `World` has a `locked` flag — during locked periods (e.g., mid-iteration) spawn/despawn is rejected. Must happen via deferred `SimCmds`.
- `game::tile::TileMap` ([tile/mod.rs](crates/game/src/tile/mod.rs)) — layered 2D grid (Terrain / Objects) with bitflag `TileKind`. **Route edits through `TileMap` wrappers** (`set_tile_flags`, `on_tile_def_edited`, etc.); they keep the pathfind `Graph` in sync. Direct edits bypass the graph update and will corrupt pathfinding.

### Deferred commands (important)

Mutation during update is routed through two queues so iteration isn't invalidated:
- `game::session::GameSessionCmdQueue` — session-level (quit, load preset, save game, reset map). Consumed once per `GameLoop::update`.
- `game::sim::SimCmds` — simulation-level (spawn tile/building/unit, despawn, callbacks). Uses a promise-style API (`spawn_*_promise` returns a handle you can poll later for `SpawnReadyResult`).

If you're tempted to mutate the world inside a system callback, queue it instead.

### Configs & singletons

- `game::config::GameConfigs` is a `#[serde(default)]` struct loaded from `assets/configs/game/configs.json`. Sub-configs: `engine`, `save`, `camera`, `sim`, `debug`. Fields are auto-rendered in the debug UI via `#[derive(DrawDebugUi)]`.
- Configs, tile sets, unit/prop/building configs are loaded once into `SingleThreadStatic` globals (`GameConfigs::load()`, `UnitConfigs::load()`, etc.) and accessed via `::get()`. Only the thread that called `load()` can read them; tests reuse the main thread for this reason.

### Saves

- JSON via serde; the whole `GameSession` serializes.
- Saves are versioned: `GameSession::save_version` must equal `CURRENT_SAVE_VERSION` ([session.rs](crates/game/src/session.rs)). `load_save_game` probes the version before the full deserialize and refuses any mismatch.
- When a change alters the serialized layout so older saves no longer deserialize as-is:
  1. Bump `CURRENT_SAVE_VERSION`.
  2. Add `crates/tools/save_migration_scripts/v<N>_to_v<N+1>.py`, modeled on the existing scripts (idempotent, rewrites `saves/*.json` in place, rejects saves from older versions).
  3. Migrate the local `saves/`, run the smoke test, and regenerate `saves/sample_saves.zip` from the migrated files (entries are stored with their `saves/` prefix).
- Don't add `#[serde(default)]` to save-state fields as a compatibility shim. Save format v2 deliberately removed those shims because they risk silent corruption. `#[serde(skip)]` is still right for runtime-only state that is rebuilt in `post_load` (e.g. the pathfinding graph on `TileMap`).

## Conventions

- `#![allow(dead_code)]` is set crate-wide in `game` and `engine` — dead-code warnings from the compiler are silenced there.
- Many hot data structures prefer stack allocation: `arrayvec`, `smallvec`, `small-map`, `smallbox`, `slab`, `bitvec`. Prefer these over `Vec`/`HashMap` when capacity is bounded.
- `#[enum_dispatch]` is used for polymorphism over variant sets (e.g. building archetypes) instead of `dyn Trait`.
- Bitflag types are declared with the `bitflags_with_display!` macro from `common` so they format nicely in logs and debug UI.
- Use `engine::log` (channel-based logger), not `println!` / `eprintln!`, except in tools/bundler.
- The engine's path types (`FixedPathBuf`/`FixedPath`/`AssetPath`/`PathRef` in `engine/src/file_sys/paths.rs`) differ from `std::path`: `join`/`push` with a path that starts with a separator appends it instead of replacing the base.

## More project knowledge

Topic notes live in `.claude/rules/`. Claude Code loads them automatically; other agents should read the relevant file before working in that area.
- [.claude/rules/known-issues.md](.claude/rules/known-issues.md) — unfinished features, open TODOs, and failure-handling gaps.
- [.claude/rules/rendering.md](.claude/rules/rendering.md) — renderer internals: backend selection, batching, offscreen target, texture cache, culling.

When you learn something durable about the codebase, add it here or to a `.claude/rules/` file (and list new files above) so every agent picks it up.
