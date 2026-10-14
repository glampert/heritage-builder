# Heritage Builder — Homebrew City Builder Game

An ongoing project of a 2D isometric city builder game based on the ancient civilizations of Asia.
Inspired by classic city builders like [Pharaoh](https://en.wikipedia.org/wiki/Pharaoh_(video_game)) and [Caesar III](https://en.wikipedia.org/wiki/Caesar_III).

Written from the ground up in the [Rust](https://www.rust-lang.org/) programming language, with custom
**OpenGL** (desktop) and **Wgpu** (web) renderer backends.

---

## Platform support

The engine is designed to be portable, but only macOS and the web are actively tested today:

| Target              | Renderer | Windowing   | Audio    | Status              |
| ------------------- | -------- | ----------- | -------- | ------------------- |
| **macOS (desktop)** | OpenGL   | GLFW        | Kira     | ✅ Tested            |
| **Web browser**     | Wgpu     | Winit/WASM  | WebAudio | ✅ Tested            |
| **Linux (desktop)** | OpenGL   | GLFW        | Kira     | 🟡 Builds & tests pass in CI, not play-tested |

---

## Prerequisites

### Common

- A recent **stable Rust** toolchain. The workspace uses **edition 2024**, so you need **Rust 1.85 or newer**
  (developed against 1.94). Install via [rustup](https://rustup.rs/).

### Desktop (macOS)

The desktop target pulls in `gl` / `glfw` / `glutin` for rendering and windowing and `kira` for audio.
On macOS these build out of the box with the standard Rust toolchain and Xcode Command Line Tools:

```bash
xcode-select --install   # if you don't already have the CLT
```

### Web / WASM

The web build cross-compiles to `wasm32-unknown-unknown` and needs a few extra tools. On macOS with
[Homebrew](https://brew.sh/):

```bash
# 1. The WASM compile target
rustup target add wasm32-unknown-unknown

# 2. wasm-bindgen CLI (generates the JS glue)
cargo install wasm-bindgen-cli

# 3. LLVM + a WASI sysroot — needed to cross-compile the C++ in imgui-sys
brew install llvm wasi-libc

# 4. wasm-opt for size/speed optimization on release builds (part of binaryen)
brew install binaryen

# 5. Python 3 to serve the built files locally (usually already present)
python3 --version
```

> **Why LLVM + wasi-libc?** The ImGui debug UI is a C++ library. The web-builder tool wires up
> `/opt/homebrew/opt/llvm/bin/clang++` and the `/opt/homebrew/opt/wasi-libc` sysroot to cross-compile it
> for `wasm32`. If either is missing the build prints a warning telling you what to install. Paths are
> Homebrew/Apple-Silicon defaults — adjust in [`crates/tools/web-builder/src/main.rs`](crates/tools/web-builder/src/main.rs)
> if yours differ.
>
> **On Linux (or anywhere without Homebrew)**, download a [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
> release and set `WASI_SDK_PATH` to its root; web-builder then uses its `clang++`, `llvm-ar` and sysroot instead.
> Use **wasi-sdk 30–32**: the headers in 33+ refuse to compile for `wasm32-unknown-unknown`. This is what CI does.

---

## Building & running

### Desktop

```bash
cargo run -p HeritageBuilder                 # debug
cargo run -p HeritageBuilder --release       # release
cargo build -p HeritageBuilder --release     # build only
```

`HeritageBuilder` (in [`crates/launcher`](crates/launcher)) is the only executable target for the game.

### macOS `.app` bundle

Wraps `cargo bundle` plus asset copying to produce `Heritage Builder.app`:

```bash
./bundle.sh                    # debug bundle
./bundle.sh release            # release bundle
./bundle.sh --install release  # install cargo-bundle first, then build
```

Output lands in `target/<mode>/bundle/osx/Heritage Builder.app`.

### Web / WASM

```bash
./web.sh --build --serve           # build (debug) then serve on :8080
./web.sh --build release --serve   # release build (+ wasm-opt) then serve
./web.sh --serve                   # just serve an existing build
```

Then open <http://localhost:8080>. Under the hood this runs the `web-builder` tool and a Python static
server; the equivalent manual steps are:

```bash
cargo run -p web-builder -- release      # build the WASM binary
cd web && python3 -m http.server 8080    # serve it
```

> **Note:** crates default to the `desktop` feature. The `web-builder` tool builds with
> `--no-default-features --features web` for you — don't run a plain `cargo build` for the web target.

---

## Playing the game

### Debug / developer UI — `Ctrl` + `/`

The game ships with an extensive **ImGui-based developer editor**. Press **`Ctrl` + `/`** at any time to
toggle between the normal in-game **HUD** and the **DevEditor** menu. The DevEditor exposes map editing,
building/unit/prop spawning, simulation inspectors, config tweaking, and per-system debug panels — most of
the game's config structs auto-render their controls there.

### Sample maps

A set of ready-made maps and test scenes is shipped, but because uncompressed saves are large (100+ MB in
total) they're committed **compressed** as [`saves/sample_saves.zip`](saves/sample_saves.zip). The game
**cannot read the archive directly** — you must extract it first. From the repository root:

```bash
unzip saves/sample_saves.zip     # extracts the .json saves back into saves/
```

The files are stored with their `saves/` path, so they land straight in the right place. Once extracted
you'll have maps like `test_map_64x64.json`, `test_map_128x128.json`, and `tiny_island.json` — load any of
them from the in-game menu to explore an already-built city instead of starting from scratch.

> **Heads-up:** everything under `saves/` (plus `logs/` and `cache/`) is **git-ignored** runtime state —
> only `sample_saves.zip` is committed. Your own saves and autosaves stay local and are never overwritten
> by the archive. Prefer a blank slate? Just create a new map from the main menu.

---

## Project structure

Cargo workspace, edition 2024. High-level layout:

```
crates/
  launcher/          Binary crate `HeritageBuilder` — the only game executable. main() just runs the loop.
  common/            Platform-agnostic utilities (coords, time, hashing, fixed strings) and shared macros.
  engine/            Platform, rendering (OpenGL + Wgpu), UI (ImGui), sound (Kira), saves, run loop.
  game/              All gameplay: simulation, world, tilemap, units, buildings, props, pathfinding, menus, debug.
  proc_macros/       #[derive(DrawDebugUi)] — auto-generates ImGui debug panels for config structs.
  tools/
    bundler/         Driver for `cargo bundle` + assets (used by bundle.sh).
    web-builder/     WASM build driver (used by web.sh).
    save_migration_scripts/  Python scripts that migrate save files between format versions.
    debug/           macOS heap-corruption debugging wrapper script.
assets/              configs/, tiles/, sounds/, fonts/, ui/ — copied into the bundle.
saves/               Save games and sample maps (git-ignored).
web/                 WASM host page (index.html, JS glue, asset manifest).
```

### Technical overview

- **Entry flow:** `launcher::main` → `engine::runner::run::<GameLoop>()` → a platform runner (a synchronous
  loop on desktop, `requestAnimationFrame` in the browser) → `GameLoop::update` each frame.
- **`Engine`** is a singleton owning the platform, renderer, UI, sound, and save subsystems. It's handed to
  the game by `&mut` every frame.
- **`GameSession`** is the fully serializable game state — camera, tile map, world, and simulation. Saving
  and loading round-trips this whole struct through serde/JSON.
- **`Simulation`** holds the core, deterministic game logic. It owns a seeded PCG64 RNG and runs on its own
  fixed-step timer (not every frame), while unit navigation runs every frame for smooth movement.
- **`World`** stores entities in generational-arena pools (one per building archetype, plus units and props).
- **`TileMap`** is a layered 2D grid (terrain / objects) with bitflag tile kinds, kept in sync with the
  pathfinding graph.

For a deeper tour of the runtime architecture, deferred-command queues, configs, and the save system, see
[`AGENTS.md`](AGENTS.md).

---

## Testing

The integration tests in [`crates/game/tests/`](crates/game/tests/) (`sim_cmds`, `search_graph`,
`unit_tasks`, `campaign`, `house_consumption`) all use a **custom harness** (`harness = false`):

```bash
cargo test -p game                   # all of them
cargo test -p game --test sim_cmds   # just one
```

Add cases by editing the `test_utils::run_tests(&[...])` list in the relevant file, e.g.
[`crates/game/tests/sim_cmds.rs`](crates/game/tests/sim_cmds.rs). The harness runs everything on the main
thread (several globals are single-thread statics) and does **not** accept filter arguments.

### Continuous integration

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push and pull request:

- **Desktop** (Ubuntu and macOS): `cargo build --workspace --all-targets` + `cargo test --workspace`.
- **Web**: `cargo run -p web-builder -- release` using wasi-sdk, a `wasm-bindgen-cli` matching the resolved
  `wasm-bindgen` crate, and `wasm-opt`. It also warns if `web/asset_manifest.json` is out of date.

CI uses the latest stable Rust and treats compiler warnings as warnings, not errors.

### Save-compatibility smoke test

After changing any serialized type, verify existing saves still load. Extract the sample saves first if
you haven't (see [Sample maps](#sample-maps)):

```bash
unzip saves/sample_saves.zip
cargo run -p HeritageBuilder -- --smoke-test-saves
```

This loads every file in `saves/` in turn, ticks each for ~10s, and **panics on any failed load** (autosave
is disabled so nothing is overwritten).

### Save format versioning

Save files carry a `save_version`, and the game refuses to load any save whose version doesn't match
`CURRENT_SAVE_VERSION` in [`crates/game/src/session.rs`](crates/game/src/session.rs). When a change alters
the serialized layout so older saves no longer deserialize as-is:

1. Bump `CURRENT_SAVE_VERSION`.
2. Add a `v<N>_to_v<N+1>.py` migration script to
   [`crates/tools/save_migration_scripts/`](crates/tools/save_migration_scripts/), modeled on the existing
   ones (idempotent, rewrites `saves/*.json` in place).
3. Migrate your local `saves/`, run the smoke test, then regenerate `saves/sample_saves.zip` from the
   migrated files.

Don't add `#[serde(default)]` to save-state fields to paper over layout changes. Write a migration
instead. `#[serde(skip)]` is still the right choice for runtime-only state that is rebuilt after loading.

---

## Gotchas & good-to-knows

- **Keep both targets compiling.** Every crate exposes `desktop` (default) and `web` features that fan out
  through the workspace. When touching `common`, `engine`, or `game`, check both build. Don't import
  platform crates (`glfw`, `kira`, `web-sys`, …) directly from `common` or `game` — route through `engine`.
- **Mutate through deferred command queues.** Spawning/despawning entities or editing the world mid-update
  is routed through `SimCmds` / `GameSessionCmdQueue` so iteration isn't invalidated. If you're tempted to
  mutate the world inside a system callback, queue it instead.
- **Route tile edits through `TileMap` wrappers** (`set_tile_flags`, `on_tile_def_edited`, …). Direct grid
  edits bypass the pathfinding graph update and corrupt navigation.
- **Configs and tile/unit/prop/building sets** are loaded once into single-thread-static globals and read
  via `::get()`. Only the thread that loaded them may read them — which is why tests reuse the main thread.
- **Prefer stack-friendly containers** (`arrayvec`, `smallvec`, `small-map`, `smallbox`, `slab`, `bitvec`)
  over `Vec`/`HashMap` when capacity is bounded — the hot paths lean on these.
- **Logging:** use `engine::log` (the channel-based logger), not `println!` / `eprintln!` (except in the
  `tools/` binaries).

---

## Screenshots

#### Main Menu
![Main Menu](https://github.com/glampert/heritage-builder/blob/main/assets/screenshots/prototype_apr_2026_1.jpg)

#### Tiny Island (Sample Map)
![Tiny Island](https://github.com/glampert/heritage-builder/blob/main/assets/screenshots/prototype_apr_2026_2.jpg)

#### City Closeup
![City Closeup](https://github.com/glampert/heritage-builder/blob/main/assets/screenshots/prototype_apr_2026_3.jpg)

---

## License

Released under the [MIT License](LICENSE). Copyright © 2026 Guilherme R. Lampert.
