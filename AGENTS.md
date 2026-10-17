# AGENTS.md

This file provides guidance to AI coding agents (Claude Code, Codex, Cursor, etc.) when working with code in this repository.

## Project

2D isometric city builder in Rust (Pharaoh/Caesar-inspired), with custom renderers: OpenGL or Wgpu on desktop, Wgpu on web. Cargo workspace, edition 2024.

## Common commands

Desktop build/run (default feature set):
- `cargo run -p HeritageBuilder` — debug
- `cargo run -p HeritageBuilder --release`
- `cargo build -p HeritageBuilder --release`

Command-line options (`cargo run -p HeritageBuilder -- --help` lists them all):
- `--map preset:N`, `--map empty:WxH` or `--load-save NAME` start in-game on that map; `--skip-home-menu` starts in-game on the map from `configs.json`.
- `--seed N` sets the random seed for new games and `--fixed-dt SECONDS` advances every frame by a fixed step. Together they make runs repeatable.
- `--window-size WxH` overrides the window size.
- `--screenshot-after-frames N --screenshot-out PATH` saves a PNG of frame N and quits: status 0 if it was written, 1 if not. Add `--screenshot-no-ui` to leave the UI out. A relative `PATH` is relative to the directory the game was started from, not `base_path`.
- `--script FILE` runs automation commands from a file and quits: status 0 if all succeeded, 1 if not, 2 if the file can't be read or has invalid lines. See [Script mode](#script-mode).
- `--automation[=PORT]` accepts commands from outside tools over localhost. See [Live control](#live-control).
- `--capture-input` starts with input capture on: the game ignores the real mouse and keyboard and takes simulated input. See [Simulated input and UI automation](#simulated-input-and-ui-automation).
- `--record FILE` records the run, and `--replay FILE` plays a recording back, then exits: status 0 if it ended in the same state, 1 if not. See [Record and replay](#record-and-replay).
- Unknown options exit with status 2. Options are declared in `engine::cli::ENGINE_CLI_OPTIONS` ([cli.rs](crates/engine/src/cli.rs)) and `game::launch::GAME_CLI_OPTIONS` ([launch.rs](crates/game/src/launch.rs)).
- Apply an option where its value is used. Don't write it into `GameConfigs`: the settings dialog saves the whole struct back to `configs.json`.

Headless Linux (CI, cloud agent sessions): install the packages from the CI Ubuntu job plus `xvfb`, then `LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s "-screen 0 1920x1080x24" cargo run -p HeritageBuilder -- <options>` runs the game on Mesa's software OpenGL. Without a sound device, audio init logs an error and the game runs silently.
- The Winit app backend (needed for `"render_api": "Wgpu"`) also needs `libxkbcommon-x11-0`, and Wgpu needs a Vulkan driver: `mesa-vulkan-drivers` provides lavapipe.
- To look at the game, take a screenshot with `--screenshot-after-frames` and read the PNG. `xdotool` can send real key presses to the window, e.g. `xdotool key F12`.
- `xvfb-run` runs its command with `2>&1`, so the log ends up in script mode's JSON output. For `--script`, start one server instead (`Xvfb :99 -screen 0 1920x1080x24 &`) and set `DISPLAY=:99`, as the CI `scenarios` job does.

Screenshots: PrintScreen or F12 saves `screenshots/<window title> <local date and time>.png` next to `saves/`; Shift leaves the UI out. A toast reports the file name. Code: `Engine::request_screenshot` ([screenshot.rs](crates/engine/src/screenshot.rs)) on top of `RenderSystem::request_capture` ([capture.rs](crates/engine/src/render/capture.rs)).

Console and automation commands: the [`] key opens the log viewer, whose input field runs commands, in any menus mode. `help` lists them with their parameters, and `help command=NAME` describes one. Lines take the form `name`, `name key=value ...` (values are JSON or plain text), `name {json}` or `{"method": "name", "params": {...}}`.
- Commands live in `crate::automation` ([automation/mod.rs](crates/game/src/automation/mod.rs)): one `CommandDef` table per area in `automation/commands/`, listed in `COMMAND_GROUPS`. The console, script mode and outside tools (through the engine's request queue, `engine::automation`) all run them.
- They run once per frame, right after the session command queue, where the world is unlocked and no `SimContext` is live. Commands that span frames return `pending(..)` and must keep ids, not references: a session load replaces the world.
- Parse params with `parse_params` into a `#[serde(deny_unknown_fields)]` struct, and validate before calling anything that panics or asserts on bad input (cell bounds, preset numbers, save names). The console only queues typed lines; never run commands from inside UI drawing.
- World edits go through `GameSession::with_sim_context` and the player's own paths (`TilePlacement::place`, `clear`, `place_road_segment`). The world stats (treasury, population) are recounted on simulation steps and when a session is created or loaded, not after an edit; commands that place or report them call `refresh_world_stats` first.

### Script mode

`cargo run -p HeritageBuilder -- --script crates/game/tests/scenarios/smoke.jsonl` runs one command per line, each once the line before it has finished, and prints one JSON result line per command to stdout, then a summary line. Logs go to stderr in this mode, so `2>/dev/null` leaves only the results. It stops at the first failure unless `--script-continue-on-error` is given. Autosave is off.
- Lines use any console form; files usually hold `{"method": ..., "params": {...}}`. Blank lines and lines starting with `#` are skipped. Invalid lines and unknown commands are reported before anything runs.
- Results: `{"line": 3, "command": "world.place", "ok": true, "result": {...}, "frame": 12}`, or `"ok": false` with `"error": {"code": ..., "message": ...}`. The summary has `ok`, `succeeded`, `failed` and `skipped`.
- Assertions are commands: `assert.eq|ne|gt|ge|lt|le|exists` run a query command (e.g. `sim.state`) and compare the value at a JSON pointer (`path`). `wait.until` polls the same way.
- `assert.image golden=FILE` compares a screenshot with a PNG, pixel by pixel (`tolerance` per channel, `max_diff_pixels`); `update=true` saves the screenshot as the golden, and `path` saves it with a `-diff.png` beside it that marks differing pixels in red. Goldens only hold on the machine and render backend that made them, so CI checks state, not images.
- Repeatable runs: set `time.fixed_dt`, start with `session.new_map`, `session.load_preset` or `session.load_save`, and use `time.fast_forward` for long stretches. `session.checksum` hashes the game state as a save would store it.
- Sample scenarios live in [crates/game/tests/scenarios/](crates/game/tests/scenarios/): `smoke.jsonl` builds a settlement and checks settlers move in; `determinism.jsonl` must print the same checksum and write the same `saves/determinism.json` on two runs with the same `--seed`; `ui_new_game.jsonl` and `ui_build.jsonl` play through the menus and HUD, and `ui_dev_menus.jsonl` through the developer editor's menus. The window size is part of the game state (the camera's viewport), so give `--window-size` when comparing runs: macOS shrinks a window that doesn't fit the screen by a pixel more or less from run to run. Debug and release builds give the same checksums.

### Live control

`hb-ctl` ([crates/tools/hb-ctl](crates/tools/hb-ctl/)) drives a running game through its automation server, so there's no socket code to write:
- `cargo run -q -p hb-ctl -- launch --map preset:2 --fixed-dt 0.016667` builds the game, starts it with `--automation=0`, waits until it answers, and prints its pid, port and log file (`logs/game-<pid>.log`). Arguments other than `--release`, `--no-build` and `--wait SECS` go to the game.
- `hb-ctl call world.place tile=small_well 'cell=[5,5]'` (or a JSON object) prints the result as JSON; status 1 if the game reports an error, 2 if it can't be reached. `hb-ctl commands [NAME]` lists the commands.
- Also `hb-ctl screenshot out/x.png [--no-ui]`, `logs [--follow] [--level L]`, `script FILE` (same output as `--script`) and `quit`. Options go before the command: `--pid PID` picks an instance when several run, and `--dir DIR` points at a game directory other than the repository root.
- Protocol: newline-delimited JSON-RPC 2.0 on 127.0.0.1 ([server.rs](crates/engine/src/automation/server.rs)). A connection first sends `auth` with the token from `logs/automation.json`, which also holds the pid and port (`logs/automation-<pid>.json` names one instance; both are removed on exit). Responses carry the frame they finished on, and errors carry the game's code in `error.data.code`. The server answers `logs.subscribe` / `logs.unsubscribe` itself and sends notifications: `log`, `session.changed` and `panic`.
- Every desktop panic writes `logs/crash-<timestamp>.txt` with the message, backtrace, frame and recent log records. `debug.panic` crashes the game on purpose to test this.
- MCP: `hb-ctl mcp` serves the commands as MCP tools over stdio, and the project's [.mcp.json](.mcp.json) registers it with Claude Code as `heritage-builder`. Tools: `game_launch` (quit again when the client disconnects), `game_connect`, `game_call`, and one per command with dots as underscores (`world_place`), listed once a game is connected. `screenshot` returns the image, at window-point resolution unless `scale` says otherwise.
- The [drive-game skill](.claude/skills/drive-game/SKILL.md) has recipes for launching, driving, screenshots and debugging.

### Simulated input and UI automation

- `input.move|click|drag|key|type|scroll` play gestures on a virtual mouse and keyboard over several frames, since input is polled once a frame: a click is a hover, a press on the next frame and a release on the one after. Targets are window points, or map cells (the camera centers on the cell first unless it's in the middle of the screen). The timeline is [automation/input.rs](crates/engine/src/automation/input.rs); `VirtualInput` lives in [app/input.rs](crates/engine/src/app/input.rs).
- Input capture: while on, the `InputSystem` reports the virtual state, real input events are dropped (except quitting and resizing), and cursor confinement stops moving the real cursor. Gestures turn it on; `input.capture on=false` turns it off, and so does a real Shift+Esc. Script mode and `hb-ctl launch` start with it on (`--capture-input`). A badge says so on screen, but not in screenshots.
- `ui.tree` lists the widgets drawn last frame: path (menu and labeled-group labels, then the widget's own), kind, rect in window points, enabled, value, and `visible: false` for one scrolled out of its window's view. Widgets record themselves in their `draw` through `UiSystem::record_widget` ([ui/registry.rs](crates/engine/src/ui/registry.rs)); the registry is off until a `ui.*` command turns it on. `ui.tree` works as an `assert.*` query once it's on.
- `ui.click path=...` takes a full path, the end of one ("Start New Game"), a label or a tooltip, and waits up to 30 frames for the widget to appear. It moves the cursor onto the widget first, following it if it moves (a window resizing to new contents), and fails if the widget doesn't report it's hovered, i.e. a menu or window covers it. A dev-menu widget scrolled out of view is scrolled into view first; any other such widget fails. It finishes once the click's result has drawn once. `ui.set` sets checkboxes, sliders, number and text inputs, dropdowns and lists through the widget's own update callback, scrolled out of view or not, but not read-only ones. A dev-menu text input takes the text even while it has keyboard focus. `dialog.list|current|open|close` wrap `menu::dialog`.
- Session switches (`session.load_preset`, `new_map`, `load_save`, `quit_to_menu`) finish two frames late: the old menus' ImGui popups take the mouse until they close, and gestures sent before then are lost.
- Camera edge scrolling ignores a cursor outside the window, such as the virtual cursor parked by `input.move park=true`. Park it before screenshots so no tile shows the hover highlight.
- Developer menus (DevEditor, `menus.mode mode=DevEditor` or Ctrl+/) are immediate-mode ImGui. Their widgets come from `engine::ui::dev` ([ui/dev.rs](crates/engine/src/ui/dev.rs)), which draws the ImGui widget and records it, and so does `#[derive(DrawDebugUi)]`; ImGui calls made directly aren't recorded, and neither is plain text: `world.stats`, `logs.tail` and the other queries report what those windows show. Use `dev` in new debug UI: `dev::collapsing_header_scope` for a section and `dev::tab_item` for a tab, so their widgets get its label in their paths. Recorded:
  - The menu bar: `Game`, `Save`, `Camera`, `Cheats`, `Debug`. A menu's items are listed only while it's open, so `ui.click path=Debug` before `ui.set path="Debug/Draw grid" value=true`. Clicking a menu toggles it, and a click that lands outside closes it. A window that opens (e.g. "Show Game Configs Debug") takes focus and closes the menu.
  - The tile palette, `Tile Selection`. Most of it is scrolled out of view, so `ui.set path="Tile Selection" value="Small Well"` selects a tile by button label or tile name (`small_well`); `"Clear"` picks the clear tool and `""` drops the selection. Tools: `Tile Selection/Tools/Undo|Redo|Clear`.
  - The minimap's `Minimap/debug` button opens `Minimap Debug`. Windows record themselves (kind `window`) with a `<window>/Close` button for their title bar's close button.
  - The tile inspector: `input.click cell=[x,y]` with no palette tile selected opens it on the tile there. The `Tile Inspector` window's value is that tile (`tile`, `kind`, `cell`); `Tile Inspector/Close` or Escape closes it. Its panels are collapsing headers: `Tile Inspector/Building`, `/Unit` or `/Prop` for the game object, `Tile Inspector/Tile` for the tile (just `Tile Inspector/Properties`, `/Flags`, ... for a bare tile), each with sections inside, e.g. `ui.click path="Tile Inspector/Building/Upgrade"`, then `ui.set path="Tile Inspector/Building/Upgrade/Level" value=1`. Clicking a header toggles it, and ImGui remembers a header's state per window, so check its `value` (open or not) in `ui.tree` first. A section's widgets are listed only while it's open.
  - The windows the Debug menu's `Show ...` checkboxes open: `Game Configs`, `World Debug`, `Game Systems Debug`, `Texture Settings` and `Sound Settings`, e.g. `ui.set path="Debug/Show Game World Debug" value=true`. Their collapsing headers (kind `header`) and tabs (kind `tab`, value: selected) open with a click, and the selected tab's widgets have its label in their paths. A slash in a label becomes `∕` (division slash) in paths, so click such a tab by its label: `ui.click path="Tax/Treasury"`, then `ui.set path="World Debug/Tax∕Treasury/Gold Units" value=250`.
  - The log viewer, `Log Viewer`, in any menus mode: [`] or `Debug/Show Log Viewer Window` opens it. Its input, `Log Viewer/Console`, has keyboard focus while the viewer does: `ui.set path="Log Viewer/Console" value="sim.pause"`, or `input.type`, then `input.key key=Enter` runs the command. `Log Viewer/Filter/Channels` opens `Log Channel Filter`, a checkbox per log channel.

### Record and replay

`--record FILE` writes the run as JSON lines as it goes, and `--replay FILE` plays it back, e.g. to reproduce a bug report. Code: [engine automation/recording.rs](crates/engine/src/automation/recording.rs), driven from `Engine::begin_frame` and `Engine::finish_run`, and the game's part in [game automation/recording.rs](crates/game/src/automation/recording.rs).
- A recording holds what the game can't recompute: each frame's time step, the real mouse and keyboard (sampled once a frame, below input capture, so simulated input comes from the replayed commands), and the automation commands from scripts and outside tools, on the frames they started. Console commands are left out, since the replayed typing runs them again.
- The header holds the command-line options, the window size, the input capture state, and the game's version, autosave setting and `GameConfigs`. Options for run modes that the recording covers (`--script`, `--automation`, `--screenshot-*`) are left out. The last line holds the frame count and the final state's checksum, as `session.checksum` hashes it.
- A replay runs with the recorded options, window size and configs. Options given with `--replay` come after the recorded ones and win, so `--automation` lets `hb-ctl` watch it, but commands sent then change the run. Real input is ignored, except quitting and Shift+Esc, which stops the replay and gives the input back. At the end, the replay compares the frame count and checksum: exit status 0 if they match, 1 if not.
- Each line is written as it happens, so a crash leaves a recording without its last line. Replaying it crashes the same way, or gives the input back after the last recorded frame.
- Saving changes the game state: `pre_save` resolves pending spawns early. That's why a replay autosaves exactly when the recorded run did, and why `session.checksum` counts as a save.
- Limits: the window size and display scale must match the recording, and warnings say when they don't. The window size is part of the state, so record with `--window-size` if the window might not fit another screen. A replay can't resize the window. `--load-save` needs the same save file. A replay repeats the run's side effects: saves, screenshots and autosaves.

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
- Automation on web: the page exposes `await window.heritage.exec(method, params)` ([web/bootstrap.js](web/bootstrap.js) over [automation/web.rs](crates/engine/src/automation/web.rs)), which resolves with a command's result or rejects with `{code, message}` (`NotReady` while loading). Screenshots come back as base64 PNG (`png_base64`); a screenshot "file" on web, like the F12 hotkey's, is a browser download.
- Web smoke test: `python3 crates/tools/web-smoke/smoke.py [--swiftshader]` serves `web/`, opens the game in headless Chromium with Playwright (`pip install playwright && playwright install chromium`) and drives it through the bridge. It needs Playwright's full Chromium (it uses the `chromium` channel): the separate headless shell's WebGPU rejects buffer maps, so captures fail there. `--swiftshader` renders on the CPU, for machines without a GPU; on Linux it also turns on the Vulkan switches that headless Chrome needs for WebGPU.

Tests — every integration test in `crates/game/tests/` (`sim_cmds`, `search_graph`, `unit_tasks`, `campaign`, `house_consumption`) uses a **custom harness** (`harness = false`):
- `cargo test -p game --test <name>` runs one file; `cargo test -p game` runs them all.
- Add cases by editing the `test_utils::run_tests(&[...])` list in the relevant file, e.g. [sim_cmds.rs](crates/game/tests/sim_cmds.rs). The harness does not accept filter args.
- The harness calls `setup()` once on the main thread because several globals use `SingleThreadStatic` and will assert if touched from a different thread. Do not parallelize tests.

CI — [.github/workflows/ci.yml](.github/workflows/ci.yml) on every push/PR: desktop build + `cargo test --workspace` on Ubuntu and macOS, plus a release web build via `web-builder`. Latest stable Rust; warnings don't fail the build. The Ubuntu job installs ALSA, X11 and Wayland dev packages for glfw/kira.
- The `scenarios` job runs the script scenarios under Xvfb (Mesa software OpenGL), runs `determinism.jsonl` twice and diffs the outputs and saves, records `ui_build` and `determinism` and replays them, and uploads `out/` and `logs/` as an artifact.
- The web job runs the Playwright smoke test on the release build with SwiftShader and uploads `out/web/`.

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
- `crates/tools/hb-ctl` — CLI client for the automation server (`--automation`): launch the game, run commands, take screenshots, follow logs, and serve the commands over MCP (`hb-ctl mcp`). Std and `serde_json` only.
- `crates/tools/web-builder` — WASM build driver (used by `web.sh`).
- `crates/tools/save-migration-scripts` — Python scripts (`v<N>_to_v<N+1>.py`) that migrate save JSON between format versions. Not a Cargo crate.
- `crates/tools/web-smoke` — Playwright smoke test for the web build (`smoke.py`). Not a Cargo crate.
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
  2. Add `crates/tools/save-migration-scripts/v<N>_to_v<N+1>.py`, modeled on the existing scripts (idempotent, rewrites `saves/*.json` in place, rejects saves from older versions).
  3. Migrate the local `saves/`, run the smoke test, and regenerate `saves/sample_saves.zip` from the migrated files (entries are stored with their `saves/` prefix).
- Don't add `#[serde(default)]` to save-state fields as a compatibility shim. Save format v2 deliberately removed those shims because they risk silent corruption. `#[serde(skip)]` is still right for runtime-only state that is rebuilt in `post_load` (e.g. the pathfinding graph on `TileMap`).

## Conventions

- `#![allow(dead_code)]` is set crate-wide in `game` and `engine` — dead-code warnings from the compiler are silenced there.
- Many hot data structures prefer stack allocation: `arrayvec`, `smallvec`, `small-map`, `smallbox`, `slab`, `bitvec`. Prefer these over `Vec`/`HashMap` when capacity is bounded.
- `#[enum_dispatch]` is used for polymorphism over variant sets (e.g. building archetypes) instead of `dyn Trait`.
- Bitflag types are declared with the `bitflags_with_display!` macro from `common` so they format nicely in logs and debug UI.
- Use `engine::log` (channel-based logger), not `println!` / `eprintln!`, except in tools/bundler and for script mode's result lines, which are the run's machine-readable output on stdout (`game::automation::script`).
- The engine's path types (`FixedPathBuf`/`FixedPath`/`AssetPath`/`PathRef` in `engine/src/file_sys/paths.rs`) differ from `std::path`: `join`/`push` with a path that starts with a separator appends it instead of replacing the base.

## More project knowledge

Topic notes live in `.claude/rules/`. Claude Code loads them automatically; other agents should read the relevant file before working in that area.
- [.claude/rules/known-issues.md](.claude/rules/known-issues.md) — unfinished features, open TODOs, and failure-handling gaps.
- [.claude/rules/rendering.md](.claude/rules/rendering.md) — renderer internals: backend selection, batching, offscreen target, texture cache, culling.

Skills live in `.claude/skills/`:
- [.claude/skills/drive-game/SKILL.md](.claude/skills/drive-game/SKILL.md) — launch, drive, screenshot and debug the running game through MCP, `hb-ctl` or `--script`.

When you learn something durable about the codebase, add it here or to a `.claude/rules/` file (and list new files above) so every agent picks it up.

## Plans

- [docs/plans/agent-automation.md](docs/plans/agent-automation.md) — agent automation (command system, script mode, control server, MCP, simulated input, UI automation, web bridge, record and replay) and in-game screenshots. All phases are done; its notes record what was built, the deviations, and what was verified.
- [docs/plans/egui-migration.md](docs/plans/egui-migration.md) — replacing imgui with egui for every UI: a portable `ui::` API, styles and fonts loaded from JSON, the renderer and input glue, and removing the web build's imgui workarounds. In progress on the `egui-migration` branch.
