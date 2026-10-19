# Migrate the UI from imgui to egui

**Status (2026-10-19): Phases 0–6 done; Phase 7, the PR into `main`, is next.** Work happens on the `egui-migration` branch, one commit per step, merged into `main` with one PR at the end. This file tracks progress.

Researched against `02d8b3c` on `main`, using egui/epaint 0.37.0 (released 2026-10-15). File references point at that commit, so re-check them before relying on them.

## Context

Every UI in the game, retail and developer, runs on imgui-rs 0.12. That causes four problems:
- **Web build:** imgui's C++ is compiled to wasm, so the web build needs a clang/wasi-sdk toolchain, C library shims (`web/libc.js`, `platform/web/libc.rs`), a linker flag (`--allow-undefined`) and a patch to the generated JS.
- **Styles:** both themes are hardcoded in `UiSystem` (`set_dev_ui_theme` / `set_in_game_ui_theme`, about 75 entries each).
- **Fonts:** fonts are baked in with `include_bytes!`, the font atlas is built once, and font switching relies on imgui's global push/pop font stack.
- **Unsafe code and a leaky boundary:** imgui's types and its `Ui` leak out of the engine (`UiSystem::ui()`, which has about 137 callers). `UiSystem` holds a raw pointer to the frame's `Ui`, and several helpers call `imgui::sys` directly.

The goal is to move every UI to egui, a pure-Rust library, rendered through the engine's own `RenderSystem` and fed by its own `InputSystem`. Only the engine's `ui` module talks to egui. Styles and fonts come from JSON files loaded at boot. The web build loses every imgui hack.

## Decisions

- **One swap.** The backend changes in one step (Phase 3), after Phases 1 and 2 have confined imgui to `engine/src/ui/` and moved the renderer to the new format. imgui and egui never run side by side.
- **The retail UI may change look.** It keeps the same widgets, flows and textures, but spacing and styling can be improved through the style files. It isn't checked pixel by pixel against the old version. The user reviews screenshots instead.
- **Use egui core only.** That means `egui` with the `bytemuck` feature and `default-features = false`. Don't use eframe, egui-winit, egui-wgpu or egui_glow, and never enable epaint's `unity` feature (it changes the vertex layout).
- **No new `unsafe`.** This migration removes unsafe code: the `RawPtr<imgui::Ui>`, the raw `imgui::sys` calls and the whole of `platform/web/libc.rs`. `UiSystem` stops using `Mutable` (an `UnsafeCell` wrapper) and keeps one `RefCell`/`Cell` field per concern.
- **Performance:** prefer stack and small-buffer containers (`SmallVec`, `ArrayVec`, `ArrayString`, `common::fixed_string::FixedWriter`). Keep the capacity of per-frame buffers between frames. Add one upload per frame for UI geometry, the time spent on UI to the frame stats, and `opt-level = 2` for egui, epaint and emath in the dev profile (egui is very slow unoptimized).

## Target architecture

### Module layout (`crates/engine/src/ui/`)

- `mod.rs`: `UiSystem`, the public types (`UiTheme`, `UiTexture`, `UiColor`, ...) and `pub use immediate::*`. Nothing egui-typed is public.
- `immediate.rs` (renamed from `dev.rs`): immediate-mode functions such as `ui::button`, `ui::text` and `ui::window`, shared by the developer UI and `widgets.rs`. The `dev::` prefix and the `UiDev*` type names go away.
- `widgets.rs`: retained retail widgets. Together with `immediate.rs`, it is the only module that calls egui for drawing.
- `backend.rs` (new, private): the egui `Context`, the stack of the current `Ui`, input translation, and turning egui's output into draw data.
- `style.rs` (new): `UiStyleDef` (serde), loading styles and fonts, and conversion to `egui::Style` / `FontDefinitions`.
- `renderer.rs`: builds `UiDrawData` from tessellated primitives and keeps track of egui-managed textures.
- `registry.rs`, `toast.rs`, `text.rs`, `icons.rs`, `sound.rs` keep their roles.

### UiSystem and the safe current-Ui stack

- Every UI function takes `&UiSystem`. Containers take `impl FnOnce(&UiSystem) -> R` closures, which replace the token APIs (`begin_menu -> Option<MenuToken>`, `tab_item`, `collapsing_header_scope`).
- `UiSystem` holds `ui_stack: RefCell<SmallVec<[egui::Ui; 8]>>` of **owned** `Ui`s. The protocol:
  - **Leaf functions** borrow the top `Ui` briefly.
  - **Container functions** first pop the parent `Ui` off the stack by value, then run the egui container on it. Inside egui's closure, they create an owned child with `ui.new_child(UiBuilder::new())`, which inherits the layout and the available rect. They push the child, run the user's closure, pop the child, call `ui.advance_cursor_after_rect(child.min_rect())`, and finally push the parent back.
  - **Top-level windows and areas** start from the egui `Context`.
- Moving a `Ui` is cheap (about 11 fields), and none of this needs `unsafe`.
- **Rules:**
  - Never hold a `RefCell` borrow across a user closure.
  - Never call back into `ui::` from inside `ctx.input/fonts/memory(|..|)`, because egui holds its lock there.
  - Every egui API that takes a closure (`on_hover_ui`, ComboBox, menus, CollapsingHeader, ScrollArea, Grid, Window, Area, Modal) is wrapped as a container.
  - A debug check verifies that the stack depth is balanced.
- **Frame flow:** `begin_frame` calls `ctx.begin_pass(raw_input)` and `end_frame` calls `ctx.end_pass()`, then `tessellate`. `Options::max_passes = 1` is required, because UI code runs inside `GameLoop::update` and can't be re-run. `zoom_with_keyboard` is off and `quit_shortcuts` is empty.
- **Labels:** keep imgui's `"label##id"` convention, parsed in our layer without allocating (`split_label(&str) -> (&str, &str)`). Pass the full label as `id_salt` wherever egui keeps state (CollapsingHeader, ComboBox, TextEdit, ScrollArea, `Window::id`). Fix any id clashes, because egui marks them in red in debug builds.

### API shape (examples)

As built in Phase 1 (`crates/engine/src/ui/immediate.rs`):

```rust
let window = UiWindow::new(&title).name("Tile Inspector").open(&mut open).auto_resize().position(p, UiCondition::FirstUse);
ui::window(ui_sys, window, |ui_sys| { .. });
ui::text(ui_sys, "Population");  ui::text_colored(ui_sys, Color::yellow(), s);  ui::bullet_text(ui_sys, format_args!("{x}"));
ui::horizontal(ui_sys, |ui_sys| { ui::text(ui_sys, "Level:"); ui::text_colored(ui_sys, Color::green(), lvl); });
ui::indent(ui_sys, 10.0, |ui_sys| { .. });  ui::separator(ui_sys);
if ui::button(ui_sys, "Pause") { .. }
let Some(_path) = ui::collapsing_header_scope(ui_sys, "Upgrade", false) else { return }; // Body drawn after it.
ui::main_menu_bar(ui_sys, |ui_sys| { ui::menu(ui_sys, "Debug", |ui_sys| { .. }); });
let Some(tab) = ui::tab_bar(ui_sys, "systems", &["World", "Tax/Treasury"]) else { return }; // Draw tab.index's contents.
ui::slider(ui_sys, "Speed", 0.0, 4.0, UiNumberFormat::Decimals(2), &mut v);
let painter = ui::painter(ui_sys, UiLayer::Window); painter.clipped(r, |p| p.image_quad(texture, corners, uvs, tint));
```

Text functions take `impl UiTextArg` (`&str`, `String`, `ArrayString` or `format_args!`), formatted on the stack. egui still turns every label into a `String`.

### Styles and fonts

- There are two files, `assets/configs/ui/styles/dev.json` and `assets/configs/ui/styles/game.json`. The game loads them in `GameLoop::start` through `UiSystem::load_styles(&[(UiTheme::Dev, path), (UiTheme::InGame, path)])`, before the first UI frame. Fonts are read from `assets/fonts/` through the engine's `file_sys`, so the web build gets them via `asset_manifest.json`.
- `UiStyleDef` is our own `#[serde(default, deny_unknown_fields)]` struct, not a serialized `egui::Style`, which would tie the files to an egui version. It holds:
  - a color palette and widget visuals (normal, hovered, active);
  - spacing, corner radius, strokes, window and popup frames;
  - the modal dim color, which replaces `set_style_color(ModalWindowDimBg)`;
  - `animation_time` and `tooltip_delay` (0 for the dev UI keeps automation instant and matches imgui's tooltips);
  - `fonts`, a map from name to `{ file, tweak }`;
  - `text_styles`, a map from name to `{ font, size, fallbacks }`, with `body`, `heading`, `button`, `small`, `monospace` and `icon` required.
- egui has one font set per Context. Every style's fonts are merged into one `FontDefinitions` with namespaced families (`"dev/mono"`, `"game/body"`). egui's built-in `Proportional`/`Monospace` families are mapped to the dev fonts, because egui uses them internally. A small symbol fallback font is added for egui's text glyphs (submenu arrow, inspection UI).
- The FontAwesome icon font becomes a fallback family with its own size; `icons.rs` stays.
- Each widget picks its size through a `FontId`, which replaces `set_window_font_scale`, `set_tooltip_font_scale` and the global font push.
- Theme switching uses `ctx.set_global_style` around each group of windows, through `ui::with_style`. Popups and tooltips start from the global style, so a per-window style wouldn't reach them (check this in spike 2).
- A `ui.reload_styles` command reloads both files at runtime, for iterating on styles.

### Input (`backend.rs`)

- Build `RawInput` every frame:
  - `screen_rect` in points;
  - `viewports[ROOT].native_pixels_per_point = content_scale`;
  - `time` from the engine's frame clock, so `--fixed-dt` keeps tooltips and animations deterministic;
  - `focused`, and `max_texture_side`.
- **Mouse:** the game polls mouse state from `InputSystem`, and the polled state is turned into `PointerMoved`/`PointerButton` events by comparing it with the previous frame. Virtual input and record/replay therefore keep working. `PointerGone` is sent when the cursor leaves the window, as `input.move park=true` does.
- **Keyboard and text:** keys arrive as `Event::Key { key, physical_key, pressed, repeat, modifiers }`, covering every `InputKey`. Modifiers are sent with `Event::ModifiersChanged` (`command` = Super on macOS, Ctrl elsewhere). `Text` events skip control characters and the macOS private-use range U+F700–F8FF.
- **Scrolling:** `MouseWheel { unit, delta, phase, source, modifiers }`.
- **Copy/paste:** an in-app clipboard buffer handles synthesized `Copy`/`Cut`/`Paste` events and `OutputCommand::CopyText`.
- **Game gating:** the mouse belongs to the UI when `is_pointer_over_egui() || egui_is_using_pointer()`, and the keyboard when `egui_wants_keyboard_input()`. Both use the previous frame's state, as now. `is_any_item_hovered` (`game_loop.rs:451`) becomes a portable query.

### Rendering

- `UiSystem::end_frame` returns an owned `UiDrawData`, which `RenderSystem::end_frame(&UiDrawData)` consumes. It holds the vertices, `u32` indices and commands `{ index range, base vertex, TextureHandle, clip rect in pixels }`.
  - This replaces `UiRenderFrameBundle` and the backend calling back into the UI renderer through an `RcMut<RenderSystem>`.
  - The buffers keep their capacity between frames.
- `UiVertex` is our own `#[repr(C)]` Pod type. It is filled with `bytemuck::cast_slice` from `epaint::Vertex`, with compile-time checks on its size and field offsets.
- Both backends upload once per frame and draw with a base vertex (`glDrawElementsBaseVertex` on GL 3.2; `base_vertex` in wgpu).
- **Premultiplied alpha:** the UI blend becomes `ONE, ONE_MINUS_SRC_ALPHA`. The UI shader outputs `vec4(t.rgb * t.a, t.a) * vertex_color` (`opengl/shaders/ui.frag`, `wgpu/shaders/ui.wgsl`). egui-managed textures are un-premultiplied when uploaded, so every texture in video memory is straight alpha. Both surfaces are non-sRGB, which matches egui's gamma-space blending. Log a warning if wgpu falls back to an sRGB surface.
- **egui textures:** `UiRenderer` maps `TextureId::Managed` to a `TextureHandle` in a `SmallVec<[(u64, TextureHandle); 4]>`.
  - Partial updates use the existing `TextureCache::update_texture` (sub-rect).
  - A full delta recreates the texture under a unique name (a name collision panics).
  - Frees are deferred by one frame, because wgpu encodes the UI pass in `end_frame`.
  - Every `TexturesDelta` is applied or cleared, including frames that skip the UI (`--screenshot-no-ui`), because its `Drop` asserts it is empty.
  - The font atlas is cleared when it is over 80% full, which means a full re-upload.
- `TextureId::User` holds `TextureHandle::pack()` behind our `UiTexture` newtype.

### Mapping the hard parts

- **Retail menus:** use `Area` with `Frame::NONE` and a pivot. Texture backgrounds go into a `Shape::Noop` slot that is filled once `min_rect` is known. `set_cursor_pos` layouts become `ui.put` / `scope_builder(max_rect)`. `fit_next_window_on_screen` becomes `max_height` plus a vertical scroll. Message boxes use `Modal`.
- **Minimap:** the rotated textured quad becomes `Mesh::with_texture` + `Shape::mesh`, drawn through `painter.with_clip_rect`. Rects, circles and tinted icons come from the painter wrapper.
- **Toasts, the badge and debug popups:** drawn with foreground/background layer painters, not one Area per message.
- **Log viewer:** `ScrollArea::show_rows` with `stick_to_bottom`. History is handled by consuming ArrowUp/ArrowDown before the TextEdit, then setting the cursor through `TextEditState`. TextEdit edits the `String` in place, so the `UiDevTextValue` callback workaround goes away.
- **Menus:** `MenuConfig` with `PopupCloseBehavior::CloseOnClickOutside`, and `menu_item` closes the menu explicitly. Without this, `Game/Pause` → `Game/Resume` in one open menu (`ui_dev_menus.jsonl`) breaks. `MenuButton::ui` returns the rect and open state the registry needs.
- **Tab bar:** a row of selectable buttons, with the selected tab kept in `ctx.data_mut`. Columns become `Grid`.
- **imgui's metrics window:** replaced by a dev window showing `inspection_ui` / `memory_ui` / `texture_ui`.
- **Widget registry:** `record_item` takes the egui `Response`: rect, `hovered()`, `ui.is_rect_visible`, and `scroll_to_me` for reveal. The path stack becomes one reused `String` plus a `SmallVec<[u16; 8]>` of segment ends.
- **Window close button:** dev windows draw their own title bar (`title_bar(false)`, then the title and a recorded `Close` button), since egui's close button has no stable id (spike 2).

## Phases

Tick tasks off and add notes in `docs/plans/egui-migration.md` as they land. Every commit builds for desktop and web and passes `cargo test --workspace`.

### Phase 0: Branch, baselines, spikes
- [x] Create the `egui-migration` branch, copy this plan to `docs/plans/egui-migration.md`, and link it from AGENTS.md.
- [x] Add a structural UI snapshot that works across renderers. `ui.tree` gets a `fields` filter, so it can output only paths, kinds, values and enabled states, without rects. Add `crates/game/tests/scenarios/ui_snapshot.jsonl`, which walks the home menu, new game, HUD and dialogs, the dev menus, the tile inspector and the debug windows, and dumps the tree at each step. Save the imgui output to compare against after the swap.
- [x] Take baseline screenshots of the same screens on OpenGL and on Wgpu into `out/baseline/`, using `assert.image` goldens. They're for checking Phases 1 and 2, and as a reference for reviewing the retail look.
- [x] Spikes on a throwaway `egui-spike` branch, with results recorded in the plan:
  1. egui text through both backends and the web build: crispness on retina, clip rects, partial uploads, the atlas reset.
  2. The Ui stack: window → header → horizontal → scroll area → combo → tooltip. Check ids are stable, registry rects and hover are right, global style reaches popups, and the close-button rect can be found.
  3. Automation: a virtual-input click, menu parity, a focused TextEdit with `ui.set` then Enter, history keys.
  4. Retail: a `UiMenu` with a background texture, pivot placement, a centered group, and a modal.

**Phase 0 notes (2026-10-17):**
- **Snapshot scenario:** `ui_snapshot.jsonl` visits 39 screens: the home menu and its dialogs (with a hover and an open dropdown); the HUD, a tooltip, the build palette, the building inspector and every in-game dialog; the dev editor, its tile palette tooltip, its five menus, the minimap debug panel, the tile inspector, the debug windows, the sample menus (every retained widget kind) and a message box. At each it runs `ui.tree fields=[path, kind, enabled, value]` and `assert.image`. The log viewer only gets a tree, since its contents change from run to run.
- **New tooling** (generic, documented in AGENTS.md and the drive-game skill):
  - `--render-api OpenGl|Wgpu` and `--app-api Glfw|Winit` pick the backends without editing `configs.json`;
  - `--golden-dir DIR` and `--update-goldens` let one scenario make and check goldens per backend;
  - `ui.hover` points at a widget without clicking it, for tooltips;
  - `ui.tree fields=[...]` drops fields such as rects.
- **Fixes the snapshot needed:**
  - `dialog.open` closed the home menu's own dialog, which the home menu then reopened over the new one. It now leaves it open underneath.
  - The minimap picked its alternating land, vegetation and rock shades with the thread RNG, so no two runs drew it the same way. The shade now comes from a hash of the cell.
- **Reproducibility:** with those fixes, two runs give identical screenshots (tolerance 0) and identical trees on each backend, and the trees are identical across backends. Runs take a varying number of frames (OpenGL about 1070, Wgpu about 850), which doesn't affect either. (It did, once: see the timing fix in the Phase 2 notes. Runs now take 503 frames.)
- **Window size:** 1280x720, not 1600x900. This Mac's screen fits at most 1440x795, and macOS shrinks larger windows.
- **Baselines** were made from `egui-migration` at `7ee09dd` (imgui), in the debug build, into `out/baseline/{opengl,wgpu}/`: 41 goldens each (about 72 MB), `ui_snapshot.jsonl` (the run's output) and `ui_tree.jsonl` (its trees). Phase 2 made them again from `e54dcdc` (still imgui's renderer path), after making the scenario take the same frames on every run; they're identical except the OpenGL message box. `out/` is git-ignored, so on another machine, check out `e54dcdc` and make them again:
  ```bash
  cargo build -p HeritageBuilder
  for api in OpenGl Wgpu; do dir=out/baseline/$(echo $api | tr A-Z a-z)
    target/debug/HeritageBuilder --script crates/game/tests/scenarios/ui_snapshot.jsonl --window-size 1280x720 --seed 1 \
      --render-api $api --golden-dir $dir --update-goldens 2>/dev/null > $dir/ui_snapshot.jsonl
    jq -c 'select(.command=="ui.tree") | {line, widgets: .result.widgets}' $dir/ui_snapshot.jsonl > $dir/ui_tree.jsonl
  done
  ```
  To compare a later build, run the same command without `--update-goldens` and with `--script-continue-on-error`, then diff the extracted trees. Failed `assert.image` lines name the `-diff.png` files in `out/ui_snapshot/`.
- **Layout problems seen in the baselines:** content overflowing the parchment, a tooltip covering the menu it belongs to, misaligned rows, truncated dev windows. All of them are listed in [Phase 6](#phase-6-layout-pass), the layout pass after the conversion.

**Spike results (2026-10-17).** The spike is commit `1654573` on the local `egui-spike` branch: `crates/engine/src/ui/egui_spike.rs`, about 1000 lines, turned on with `HB_EGUI_SPIKE=1` (always on in web builds there). It runs egui 0.37 next to imgui and renders through the existing UI draw path, converting egui's meshes to imgui's vertex format and u16 indices, with colors un-premultiplied on the CPU. Its backend part (input events, texture deltas, mesh submission) and its `api` module (the owned-Ui stack) are good starting points for Phase 3. Every question has an answer, and the design holds:
1. **Rendering.**
   - egui draws correctly on OpenGL, on Wgpu and on web (WebGPU in headless Chrome, via the smoke test, which passed). Text is sharp at 2x, because glyphs are rasterized at physical pixels.
   - Fonts load from `assets/fonts/` through `file_sys` on desktop and web, and the FontAwesome fallback family draws icons inline.
   - The atlas starts as one full upload, then sends small partial updates as new glyphs appear: 12 to 30 per session, a few thousand texels each.
   - Growing the atlas means a full re-upload. Recreating the texture under a fresh name and freeing the old one a frame later works, even when it happens every frame.
   - The atlas is as wide as `max_texture_side`, so 8192 gave an 8192x2048 atlas (64 MB). Use 2048: epaint asserts a width of at least 1024, and 1024 overflowed with large text sizes, garbling glyphs.
2. **The Ui stack.**
   - Owned `Ui`s on a `RefCell<SmallVec<[egui::Ui; 8]>>` work with no `unsafe`: `with_current` for leaf widgets, `with_parent` (pop, run the egui container, push back) and `scope` (`new_child`, run the closure, `advance_cursor_after_rect`).
   - Window → collapsing header → horizontal → scroll area → combo → tooltip, plus menus, a modal and an area, lay out as plain egui would.
   - The registry gets correct paths, rects, hover and values.
   - Switching the global style around a group of windows (`with_style`) reaches combo popups and tooltips.
   - egui's own close button can't be found reliably (its id comes from `auto_id_with`, a counter), so dev windows get **our own title bar**: `title_bar(false)`, then a row with the title and a recorded `Close` button.
3. **Automation.**
   - `ui.click` (hover, press, release over frames) works, and so do `ui.set`, `ui.hover`, a combo item pick and a modal's buttons.
   - A widget behind a modal is reported as covered, since egui's hover respects layers.
   - Menus with `CloseOnClickOutside` stay open for `Pause` then `Resume`, and `menu_item` closes the menu with `ui.close()`.
   - The console takes `ui.set` while focused, and Enter submits it (`lost_focus() && key_pressed(Enter)`, then `request_focus()` again).
   - History works through `consume_key` before the TextEdit and `TextEditState` to put the cursor at the end, so typed text appends.
   - Tooltips wait for egui's `tooltip_delay` (0.5 s by default). The style files get a `tooltip_delay` (0 for the dev UI, as imgui had).
4. **Retail.** An `Area` with a pivot, `Frame::NONE`, a `Shape::Noop` background slot filled with the parchment mesh after the content, a `vertical_centered` group in the game font, a game-style combo popup, and a `Modal` with a dimmed backdrop all work.

**What else the spike found:**
- **Toolchain:** egui 0.37 needs Rust 1.95. This machine's stable toolchain was 1.94 and is now 1.99.1 (CI uses the latest stable). Phase 3 notes the minimum version in AGENTS.md.
- **API details for 0.37:**
  - `Id::new` is deprecated; use `Id::unique` or `IdSalt`.
  - `Panel`s show inside a `Ui`: the main menu bar goes through `ctx.root_ui(|root| Panel::top(..).show(root, ..))`.
  - `Window` and `Area` still show on the `Context`.
- **Registry:**
  - Record a combo's items under the combo's label (push it as a path segment).
  - Record a text input's `TextEdit` response, not the whole row with its label.
- **Z-order:** a newly shown `Area` goes on top. Retail menus and HUD bars need explicit `Order`s, so a dialog doesn't end up under the HUD.
- **Debug-build cost:** the spike's windows (about 2,500 vertices) added 0.53 ms a frame unoptimized, and 0.23 ms with `opt-level = 2` for egui, epaint, emath and ecolor. Keep that override.
### Phase 1: Portable UI API, still on imgui
- [x] Rename `dev.rs` to `immediate.rs`, re-export it at `ui::`, rename `UiDev*`, and update every `dev::` caller and the proc macro.
- [x] Add the missing functions on imgui: text, colored and bullet text, `text_segments`, layout (`separator`, `spacing`, `new_line`, `horizontal`, `indent`, `dummy`, full-width items), `window` / `overlay` builders (they absorb `record_window` and `fit_next_window_on_screen`), `scroll_area` (stick to bottom), `main_menu_bar` / window menu bar, `tab_bar`, `grid`, `id_scope`, `with_style`, the console text input (hint, submit on Enter, focus, history), and queries (screen size, any item hovered, window focused). Add the painter wrapper (rect, circle, text, image, textured quad, clip rect; window, background and foreground layers) and a portable color type. Move `icon_button`, `image_button`, `dpad_buttons`, `custom_tooltip` and `input_*_xy` / `input_color` from `mod.rs` into the immediate module.
- [x] Remove imgui types from the API: `TreeNodeFlags` becomes `default_open: bool`, printf formats become `UiNumberFormat` (the proc macro converts `%.2f` and the like at compile time), `UiDevNumber` drops `DataTypeKind`, and `UiTextureHandle`/`UiFontHandle` become our own types. Text takes a size, replacing `set_window_font_scale`. A modal dim setter replaces `set_style_color`.
- [x] Port the call sites:
  - `game/src/debug/*`, including `sim.rs`, `systems.rs` and `world.rs`, which call `.ui()` without naming imgui;
  - `tile/minimap.rs`, `tile/rendering.rs`, `camera.rs`, `unit/task/patrol.rs`, `game_loop.rs`, `menu/dialog/mod.rs`;
  - `engine/src/debug/{sound,texture,ui_text,update_timer}.rs`, `ui/toast.rs`;
  - `proc_macros/src/debug_ui.rs`, which now emits `ui::text` / `ui::separator` with no `ui` binding.
- [x] Make `UiSystem::ui()` `pub(super)` (used only by `widgets.rs`, `immediate.rs` and `internal.rs`) and drop `imgui` from `crates/game/Cargo.toml`.
- [x] Leave `TODO(layout)` notes where call sites hardcode sizes or positions involved in a [Phase 6](#phase-6-layout-pass) problem, but don't fix them yet: this phase must leave the screenshots unchanged.
- **Done when:** `rg imgui crates/game crates/proc_macros` finds nothing, every `ui_*.jsonl` scenario passes, the screenshots match the Phase 0 goldens, and the UI snapshot hasn't changed.

**Phase 1 notes (2026-10-17):**
- **One commit, not several:** the API change and every caller depend on each other, so the phase landed in a single commit.
- **Verification:**
  - All 41 `ui_snapshot.jsonl` screenshots match the Phase 0 goldens at tolerance 0, on OpenGL and on Wgpu.
  - The trees match the baselines once each tree's widgets are sorted by path (see the tab bar note below).
  - A rect-level comparison against a build of `7ee09dd` (the baseline commit, in a temporary worktree) found no differences, except the save lists, since the worktree had no local `saves/`.
  - Every scenario passes, `cargo test --workspace` passes, `determinism.jsonl` still gives checksum `ef501950e441eea8`, record and replay match, and the web build compiles with no warnings and passes the smoke test.
- **What the API looks like** (`crates/engine/src/ui/immediate.rs`, re-exported at `engine::ui`):
  - Text functions take `impl UiTextArg`: a `&str`, `String`, `ArrayString` or `format_args!(..)`. Formatted text uses a 256-byte stack buffer and falls back to a `String` only when longer. Callers pass `format_args!` where they used `format!`.
  - Rows are `ui::horizontal(ui_sys, |ui_sys| ..)` (or `horizontal_spaced`). On imgui, a layout stack in `UiSystem` inserts `same_line` before every widget after the first, and the row is an imgui group. `ui::indent`, `ui::id_scope`, `ui::with_font_scale` and `ui::grid` (texture list) are closures too.
  - Windows are `ui::window(ui_sys, UiWindow::new(title).open(..).position(..).size(..).auto_resize()..., |ui_sys| ..)`. The window records itself and its close button, and puts its name in its widgets' paths: callers no longer call `record_window` or `path_scope` for it. `UiWindow::name` sets the automation name when the title differs (Tile Inspector, Minimap Debug). `record_as(kind, value)` with `ui::take_window_set_value` covers the palette's `ui.set`. Other builder options: `fit_on_screen`, `menu_bar`, `item_spacing`, `monospace`, `collapsed`, `no_resize`, `no_scrollbar` and `no_collapse`.
  - Overlays are `ui::overlay(ui_sys, UiOverlay::new(label, pos).bg_alpha(..).bg_color(..).text_color(..).font_scale(..), |ui_sys| ..)`.
  - Other containers: `ui::scroll_area` with `UiScrollArea` (`leave_at_bottom`, `horizontal_scrollbar`, and `ui::scroll_to_bottom`), `ui::tooltip`, `ui::item_tooltip`, `ui::main_menu_bar`, `ui::window_menu_bar` and `ui::menu`, all closures.
  - **Deviation:** collapsing headers stay guard-shaped (`collapsing_header_scope(ui_sys, label, default_open) -> Option<scope>`), since imgui's collapsing header doesn't nest its body. That kept about 50 early-return call sites as they were. For egui, draw the header with `CollapsingState` and the body in the current `Ui`.
  - **Deviation:** tab bars take their labels up front, `ui::tab_bar(ui_sys, id, &labels) -> Option<UiTab>`, and the caller draws the selected tab (`tab.index`) after the bar. egui has no tab widget, and its row of buttons must come before the content. The trees record the tabs before their content as a result.
  - **Deviation:** `text_segments` wasn't added; `horizontal` covers it.
  - Text inputs: `UiTextEdit::new(label)` with `hide_label`, `hint`, `submit_on_enter`, `full_width`, `focus` and `history(UiTextHistory)`, drawn with `ui::text_edit` (the name avoids the retained `UiTextInput`). The log viewer's history callback moved into the engine. `ui::input_text` remains for the simple case.
  - The painter is `ui::painter(ui_sys, UiLayer::Window|Background|Foreground)`: `rect`, `rect_filled`, `circle`, `circle_filled`, `text`, `image`, `image_quad` and `clipped`. UVs use (0, 0) at the texture's top left, as both libraries do. The minimap, debug popups, toasts and the automation badge use it.
  - Numbers take `UiNumberFormat::{Default, Decimals(n)}`. The proc macro's `display_format = "%.2f"` attribute became `decimals = "2"` (nothing used it). The `UiNumber` trait seals its imgui bound in a private module.
- **imgui types out of the API:**
  - Textures are `TextureHandle` everywhere (`UiImageButtonParams::texture`, `UiSpriteIcon::set_sprite`, painter, custom tooltip). `UiTextureHandle` and `INVALID_UI_TEXTURE_HANDLE` are crate-private, used by `widgets.rs` through `immediate::imgui_texture`, and `UiSystem::to_ui_texture` is gone.
  - `UiSystem::ui()`, `fonts()`, `current_ui_style()` and `current_ui_font()` are `pub(crate)`.
  - `set_style_color(ModalWindowDimBg)` became `set_modal_backdrop_color`, and `ui().is_any_item_hovered()` became `UiSystem::is_any_widget_hovered`.
  - What's left public: the renderer's `UiDrawVertex`/`UiDrawIndex`/`UiRenderFrameBundle`, which Phase 2 replaces.
- **Kept until Phase 3:**
  - The Debug menu's "Show ImGui Debug Menu" checkbox keeps its label, so the snapshot stays the same; Phase 3 replaces imgui's metrics window.
  - The comment on `SESSION_SETTLE_FRAMES` (`automation/commands/session.rs`) still describes imgui's popups.
  - `fit_on_screen` still uses imgui's unsafe size callback.
- **Kept for identical screenshots,** marked `TODO(layout)`: the palette's blank line for an empty section, and the trailing tile spacing it counts in its width.
- **Other changes:**
  - The log viewer's channel filter window draws after the viewer instead of inside it.
  - Tab contents get an id scope per tab label.
  - `UpdateTimer`'s id scope uses its address.
  - The scripts that did the mechanical part of the port (text, separators, `dev::` paths, indent pairs) were throwaway; the rows, windows, overlays, menus, tabs and painters were ported by hand.

### Phase 2: Renderer prep, still on imgui
- [x] Add `UiDrawData` and `RenderSystem::end_frame(&UiDrawData)`, and remove `UiRenderFrameBundle`.
- [x] Add `UiVertex` (Pod) and `u32` indices, with one upload per frame and base-vertex draws in both backends.
- [x] Switch the UI pipelines and shaders to premultiplied alpha, premultiplying imgui's vertex colors on the CPU for now.
- [x] Add texture-delta handling to `UiRenderer`: create, partial update, recreate on resize, deferred free. imgui's atlas goes through it too.
- **Done when:** screenshots match the Phase 0 goldens within a small tolerance on OpenGL and Wgpu, the web smoke test passes, and the frame stats show no regression.

**Phase 2 notes (2026-10-18):**
- **What changed:**
  - `render/ui_draw.rs` holds the renderer's UI input: `UiDrawData` (one vertex buffer, `u32` indices, and commands with an index range, a base vertex, a `TextureHandle` and a `UiClipRect` in framebuffer pixels), and `UiVertex`, laid out like `epaint::Vertex` (compile-time checks on its size and field offsets). The buffers keep their capacity between frames.
  - `UiSystem::end_frame(tex_cache, ..)` returns `&UiDrawData` and `RenderSystem::end_frame(&UiDrawData)` draws it. The render module no longer depends on `ui`. Gone: `UiRenderFrameBundle`, `UiDrawVertex`/`UiDrawIndex`, the four `RenderSystem` UI calls, the Wgpu backend's own UI batch, and its `unsafe` byte copy of imgui's vertices.
  - OpenGL uploads once and draws each command with `glDrawElementsBaseVertex`, rebinding the texture only when it changes. Wgpu uploads once, typed.
  - The UI blends with premultiplied alpha (`ONE, ONE_MINUS_SRC_ALPHA`), and its shaders premultiply each texel; the scene keeps straight alpha. `ui/renderer.rs` premultiplies imgui's vertex colors while it copies them. Wgpu logs a warning if it can only get an sRGB surface.
  - `UiManagedTextures` (`ui/renderer.rs`) keeps the UI library's own textures: create, partial update, full update in place, recreate under a new name when the size or settings change, and release a frame after a free or a recreate. It works on a `UiTextureStore` trait (the `TextureCache`, or a test store), so its unit tests cover every path. imgui's atlas is created through it at startup (`ui_texture_0`); imgui never updates its atlas, so the other paths first run for real in Phase 3.
  - Clip rects are rounded to the nearest pixel, as egui's own backends do. imgui's backends, and ours until now, truncated.
  - `debug.stats` has `frame_ms.ui_render`, and the game stats overlay a `Ui Render` line: the time `UiSystem::end_frame` takes to build the draw data. Phase 3 measures egui's tessellation with it.
- **Timing fix (`e54dcdc`, its own commit):** desktop screenshots are encoded on a background thread, so `assert.image` finished after however many frames the encode took, 5 to 30 in a debug build, and the slideshows (home menu, sample menus) were caught at different points from run to run. The OpenGL message box baseline had caught the sample menus' slideshow on another image. Screenshots requested by automation commands are now encoded in the frame they're captured (`ScreenshotRequest::encode_in_frame`), and `ui_snapshot.jsonl` takes 503 frames on every run, on both backends. The baselines were remade from that commit (above); the old ones are in `out/baseline-7ee09dd/`.
- **Verification:**
  - Against the remade baselines, the trees are identical, and on every screen each pixel is within 1 per channel (premultiplied vertex colors are rounded to 8 bits), except the minimap's border: its clip rect has fractional edges, and rounding instead of truncating moves each edge by one pixel (1,125 pixels, on every screen that shows the minimap). A run with the old truncation, made temporarily, left nothing above 1.
  - Every scenario passes on OpenGL and on Wgpu, `cargo test --workspace` passes (new tests for `UiDrawData`, `UiClipRect`, `UiManagedTextures` and premultiplying), `determinism.jsonl` still gives `ef501950e441eea8`, record and replay match, and the release web build passes the smoke test.
  - **Performance,** on a busy developer screen (six debug windows, about 8,400 triangles, 171 draw calls), as mean and median over 360 frames from three alternating runs of each build, end of frame without present, in ms:

    | Build | OpenGL before | OpenGL after | Wgpu before | Wgpu after |
    |---|---|---|---|---|
    | Release | 0.68 / 0.67 | 0.66 / 0.65 | 0.23 / 0.20 | 0.28 / 0.22 |
    | Debug | 0.32 / 0.31 | 0.66 / 0.68 | 0.55 / 0.50 | 0.81 / 0.77 |

    Release is unchanged within noise; building the draw data takes 0.02 to 0.05 ms. Debug builds spend about 0.3 ms more: imgui's vertices and indices are converted one by one (premultiplied, widened to `u32`), which unoptimized iterators make slow, and copying them as bytes would take `unsafe`. Phase 3 drops the conversion: egui's vertices and indices copy as they are.

### Phase 3: The swap to egui (one commit)
- [x] Add `egui = { version = "0.37", default-features = false, features = ["bytemuck"] }` to `crates/engine`, plus the dev-profile `opt-level` overrides (egui, epaint, emath, ecolor). Remove `imgui` from `crates/engine/Cargo.toml`. Note in AGENTS.md that the build needs Rust 1.95 or later. Use a `max_texture_side` of 2048.
- [x] Rewrite `UiSystem` and add `backend.rs`: egui Context, `RefCell`/`Cell` fields, the Ui stack, pass driving, input translation, draw data, textures.
  - Each frame: `UiManagedTextures::release_freed`, then `set` for each `textures_delta.set` (egui's colors un-premultiplied into a reused buffer), then tessellate and build the draw data, then `free` for each `textures_delta.free`.
  - Give `UiDrawData` a slice-based `add_mesh` for egui's meshes (`bytemuck::cast_slice`, then `extend_from_slice`), so debug builds copy them with a memcpy. The iterator version was for imgui's conversion; remove it with imgui.
- [x] Add `style.rs` and the two style files. Remove `include_bytes!` and the hardcoded themes. Add the `ui.reload_styles` command.
- [x] Port `immediate.rs`, `widgets.rs`, `toast.rs`, `internal.rs` (most of it should disappear), the painter, the minimap and the metrics window replacement.
- [x] Revisit the automation details: menu close behavior, the close-button rect, text-input `ui.set` with focus, and the note that session switches finish two frames late (imgui popups held the mouse; check whether egui still needs this).
- [x] Build the retail widgets so the [layout pass](#phase-6-layout-pass) can fix geometry without another rewrite, but keep today's geometry, so screenshots are comparable:
  - a menu's content area comes from its background sprite's insets, kept with the menu's parameters instead of being scattered through `widgets.rs`;
  - text widgets take an optional wrap width;
  - tooltips take a placement (at the pointer, or beside the widget);
  - labeled groups can share one label column across rows (egui `Grid`);
  - every retained menu records its content rect in the widget registry (kind `menu_content`), for the layout check.
  Mark each spot that the pass will change with `TODO(layout)`.
- **Done when:**
  - every scenario passes on OpenGL and Wgpu, and `determinism.jsonl` gives the same checksum before and after;
  - the UI snapshot diff against Phase 0 has been reviewed (only intended differences);
  - the web smoke test passes and `cargo test --workspace` passes;
  - the user has reviewed screenshots of every screen.


**Phase 3 notes (2026-10-19):**
- **What changed:**
  - `crates/engine` depends on `egui 0.37` (`default-features = false`, `bytemuck`) instead of `imgui`; the workspace's dev profile builds egui, epaint, emath and ecolor with `opt-level = 2`. The build needs Rust 1.95 (egui's `rust-version`).
  - `ui/backend.rs` (new) owns the egui `Context` (`max_passes` 1, keyboard zoom and quit shortcuts off, dark theme). Each frame it builds the `RawInput` from the `InputSystem`: the pointer and its buttons are polled and diffed, so virtual input and replays work; key, character and scroll events come from the engine's event dispatch (`Copy`/`Cut`/`Paste` on command+C/X/V, control characters and macOS's U+F700–F8FF dropped); the screen rect and `native_pixels_per_point` from the framebuffer; `max_texture_side` 2048. `end_frame` copies `CopyText` to an in-app clipboard, releases the textures freed last frame, applies this frame's `TexturesDelta`, tessellates, builds the `UiDrawData`, and queues this frame's frees. `warm_up` runs one empty pass at boot so text can be measured before the first frame; the font atlas it produces is kept and applied with the first frame's delta (`TexturesDelta`'s `Drop` asserts it's empty, so `Drop` clears it).
  - `ui/style.rs` (new): `UiStyleDef` (serde, `deny_unknown_fields`, every field defaulted) with `fonts` (name → `{file, scale, y_offset_factor}`), `text_styles` (body, heading, button, small, monospace, icon, each `{font, size, fallbacks}`), `colors` (`#RRGGBB[AA]`), `widgets` (noninteractive, inactive, hovered, active, open), `spacing`, `window` and `popup` frames, `animation_time`, `tooltip_delay` and `modal_backdrop`. `UiStyles::load` reads both files through the engine's file system (no `include_bytes!`), converts each to an `egui::Style` (`wrap_mode` Extend, no shadows, labels not selectable) and merges the fonts into one `FontDefinitions`, namespaced by theme (`dev/mono`, `game/body`); egui's own `Proportional` and `Monospace` families map to the dev fonts, and a file used by both themes is read once. Unit tests cover the colors, unknown fields and missing fonts. The files are `assets/configs/ui/styles/dev.json` and `game.json`, with the two imgui themes' colors.
  - `ui/mod.rs`: `UiSystem` holds the backend, the styles, and one `Cell`/`RefCell` per concern (theme, modal backdrop, font scale, item width, toasts, the Ui stack, the last response, the hovered flag, the registry); `Mutable` and the raw `Ui` pointer are gone. `GameLoop::load_assets` calls `load_styles(&UI_STYLE_PATHS, content_scale)` right after the tile sets and panics if they fail, since no text can be drawn without their fonts; `reload_styles` backs the new `ui.reload_styles` command. `set_ui_theme` sets egui's global style, so popups and tooltips follow it. `end_frame(tex_cache, draw_overlays, badge)` returns the `UiDrawData`. The "UI Debug" window shows egui's settings, inspection, memory and texture UIs; the Debug menu's checkbox is now "Show UI Debug Window".
  - The Ui stack is a `RefCell<SmallVec<[egui::Ui; 8]>>` of owned Uis: `with_ui` for leaf widgets, `with_parent_ui` for egui containers (pops the parent first), and `ui_scope`/`child_ui_scope`/`draw_at` for children (`new_child`, then `advance_cursor_after_rect`), with a debug check that the depth is balanced. Widget callbacks run outside the borrows.
  - `ui/renderer.rs` applies the `TexturesDelta` through `UiManagedTextures` (egui's premultiplied colors un-premultiplied into a reused buffer; full and partial updates, frees a frame later), and builds the draw data from the `ClippedPrimitive`s with `bytemuck::cast_slice`: `UiDrawData::add_mesh` takes slices now. `TextureId::User` carries `TextureHandle::pack()`.
  - Input ownership: the keyboard is the UI's while a text edit has focus, the mouse while the pointer is over egui or egui is using it. egui counts a pointer on the background layer as over the UI unless the frame used the root Ui, so `end_frame` touches `ctx.root_ui` once a frame (nothing is drawn there; the dev menu bar panel uses it too). `is_any_widget_hovered` is our wrappers' own count (last frame's `hovered()` over the recorded widgets), since egui's hover includes a retail menu's background.
  - `immediate.rs`, `internal.rs`, `widgets.rs` and `toast.rs` were ported with their public API, except: `custom_tooltip` takes a `UiTooltipPlacement`; `UiPainter` lost its lifetime; the provisions below added fields. Retail menus are egui `Area`s (fixed position and pivot, `constrain` off) or `Modal`s (Escape closes the top one), with the background sprite in a `Shape::Noop` slot filled once the frame's rect is known. Windows draw their own title bar and wrap their contents in a `ScrollArea` unless `no_scrollbar`. Collapsing headers are drawn by hand (full width, state in temp data), tabs are selectable labels, grids are `egui::Grid`, `input_number` is a `DragValue` with -/+ buttons, text edits consume the history keys before the `TextEdit` and move the cursor to the end through `TextEditState`, and a `ui.set` value is written into the text before drawing. The minimap draws its rotated quad as a `Mesh`.
  - Fonts: epaint sizes a font by its em, imgui by its ascent minus descent, so egui drew the game font 7% and the dev font 26% larger. The per-font `scale` in the style files (Protest Revolution 0.8333, Source Code Pro 0.7955) restores imgui's sizes.
  - Sizing: egui's first frame of an area or window is a sizing pass that offers the whole screen, and a widget that fills its room (a separator, a child window, a centered group) would size the menu to it; such widgets measure zero in a sizing pass (`immediate::fill_size`), as egui's own separator does. Auto-sized windows keep last frame's width for the same reason, and their scroll area stops at the screen bottom. A menu-bar menu taller than the room below the bar scrolls instead of moving up over the bar. A window or retail menu that opens closes the open popups, as imgui's focus change did (`close_popups_on_open`), which the Debug-menu steps of `ui_snapshot` depend on.
  - Automation: menu-bar menus are recorded before their items (`reserve_rect`/`update_rect`); `ui.click`'s reveal scrolls without egui's animation; a text edit's focus request made in a sizing pass (where egui disables and ignores widgets) is kept for the next frame, so the log viewer's console takes `ui.set` and Enter as before; `SESSION_SETTLE_FRAMES` is 0, since egui's popups don't hold the mouse across a session switch, and every scenario passes without the wait.
  - The game style's widget fills are opaque: translucent fills made the sliders' rails and the text inputs disappear on the parchment.
- **Deviations:**
  - No symbol fallback font: nothing in our UI needs egui's text glyphs, and the UI Debug window's own arrows render as missing glyphs, which is fine for a debug window.
  - Dev windows draw their own title bar: its close button records as `<window>/Close`, and it's drawn before the contents, so the registry lists it first, as before.
  - Tooltips default to the pointer with a 12-point gap, as imgui's did; `UiTooltipText::with_placement` and `custom_tooltip`'s placement argument are the provision for Phase 6, with `BesideWidget` unused so far.
  - The labeled group keeps its rows (no egui `Grid`): a `Grid` would change the geometry this phase must keep. `label_column_width` is the shared-column provision.
  - Collapsing headers and tab bars keep the Phase 1 shapes (guard-shaped scope, labels up front).
  - `menu_content` is recorded for retail menus; dev windows already record `window`.
- **Provisions for the layout pass** (`rg 'TODO\(layout\)'` lists the spots): `UiMenuParams::content_insets` (`UiMenuInsets`, the frame's padding when set, the style's window margin otherwise), `UiText::wrap_width` (`with_wrap_width`; measured and painted wrapped when set), `UiLabeledWidgetGroupParams::label_column_width`, `UiTooltipPlacement`, and the `menu_content` records. Every retail menu sets none of them, so its geometry is as it was.
- **Verification (2026-10-19, macOS, 1280x720):**
  - `smoke`, `ui_new_game`, `ui_build`, `ui_dev_menus` and `ui_snapshot` pass on OpenGL and on Wgpu (`ui_snapshot`'s image asserts fail, as they must: the goldens are imgui's). Its widget trees differ from the Phase 0 baselines only as intended: the `menu_content` records; a dropdown's items are listed while it's open (egui's popup records them, `Home/Terrain Kind/Grass`); the three unlabeled item lists went from their menu's bare path to `<menu>/item_list`; and the renamed "Show UI Debug Window" checkbox. The run takes 501 frames instead of 503: session switches no longer wait two frames.
  - `determinism.jsonl` (`--seed 7`): both runs print the same lines and write the same save, and `ui_build` and `determinism` replay to their recordings' state. The checksum is `5605d579c2a4517c` at frame 29, where it was `ef501950e441eea8` at frame 31: the script reaches it two frames earlier for the same reason. With `SESSION_SETTLE_FRAMES` put back to 2 for one run, the egui build gives `ef501950e441eea8` exactly, so the simulation is untouched.
  - `cargo test --workspace` passes (new tests for the style files and the registry's `menu_content` paths), `--smoke-test-saves` loads the 14 local saves, the release web build passes the smoke test in Chrome (Wgpu), and `web/asset_manifest.json` lists the two style files.
  - **Performance,** on the Phase 2 bench screen (six debug windows, about 180 draw calls), mean / median ms over 360 frames from three alternating runs, imgui (the Phase 2 binaries) against egui:

    | Build, backend | UI build: imgui | egui | of which tessellation (`ui_render`): imgui | egui | Render end to present: imgui | egui |
    |---|---|---|---|---|---|---|
    | Release, OpenGL | 0.26 / 0.25 | 0.61 / 0.65 | 0.04 / 0.04 | 0.20 / 0.20 | 0.45 / 0.40 | 0.56 / 0.58 |
    | Release, Wgpu | 0.11 / 0.11 | 0.29 / 0.29 | 0.02 / 0.01 | 0.10 / 0.10 | 0.20 / 0.18 | 0.40 / 0.29 |
    | Debug, OpenGL | 0.89 / 0.93 | 1.28 / 1.30 | 0.39 / 0.42 | 0.19 / 0.19 | 0.64 / 0.68 | 0.45 / 0.46 |
    | Debug, Wgpu | 0.69 / 0.62 | 0.95 / 0.93 | 0.28 / 0.27 | 0.15 / 0.14 | 0.84 / 0.75 | 0.68 / 0.65 |

    "UI build" is `ui_begin` + `ui_end` (the frame's UI code, egui's pass and the draw data); "tessellation" is the part spent in `UiSystem::end_frame`, which for imgui was only the vertex conversion. egui costs about 0.3 ms more a frame on this screen in release, mostly its anti-aliased tessellation, and the draw call count rose from 171 to 178 (OpenGL) and 172 to 180 (Wgpu). Debug builds pay about 0.3–0.4 ms more in UI code and get back the conversion time Phase 2 noted. The retail screens are far lighter.
  - **Screenshots:** every screen of `ui_snapshot` was compared side by side with the imgui baseline (`out/baseline/*`), on both backends. Text is at the same size and positions within a few points; the retail menus keep their geometry. Visible differences: egui's checkboxes, sliders (a rail with a round handle), drag values with -/+ buttons, combo boxes and list rows (full-width, selected row highlighted); dev-menu items are rows and a menu's buttons fill its width; windows have the custom title bar; tooltips use the popup frame. For the user's review.

### Phase 4: Retail style refresh
- [x] Tune `game.json` (spacing, colors, text sizes). Review screenshot sets side by side with the user and iterate using `ui.reload_styles`. Geometry fixes (overflow, alignment, tooltip placement) wait for the [layout pass](#phase-6-layout-pass).
- [x] Update the image goldens used locally.

**Phase 4 notes (2026-10-19):**
- **What was wrong:** the Phase 3 `game.json` gave the selection color the same tan as the widget fill, so a slider's filled part, a selected list row and a dropdown's current item were invisible on the parchment; the empty rail, the scroll bar and the text fields were faint too.
- **What changed (`game.json`):**
  - `selection` is an ink brown (`#A1703F`) with cream `selection_text`: it fills sliders up to the handle, highlights the selected row of a list and the current item of an open dropdown, and marks selected text. Checkboxes keep the widget fill and an ink check mark, since egui colors only selectable buttons with the selection.
  - The widget `fill` (slider rails, checkbox boxes, slider and scroll bar handles) is a tan (`#CDB98F`) that darkens when hovered and pressed, instead of a near-parchment tone that lightened. The `weak_fill` of buttons, dropdowns and number inputs is unchanged.
  - Text fields are a light cream (`text_edit_fill` `#FFF4DAB3`), and the scroll bar track is lighter than the parchment (`extreme_fill` `#FFF4DA`) so the tan handle shows on it.
  - Slider rails are 10 points tall (egui's default is 8): `spacing.slider_rail_height` is a new style field, which `dev.json` sets to 8.
  - Text sizes, spacing and fonts are as in Phase 3: they already matched the imgui sizes.
- **Style code:** the popup frame's `stroke_width` was dropped from `UiStyleDef`: egui's popups and tooltips take the window's border, so it never applied. `popup` now holds only `corner_radius`.
- **Review loop:** a game launched with `hb-ctl`, then `ui.reload_styles` and a screenshot per retail screen (25 screens: the home dialogs, in-game dialogs, tooltips and the inspector) after each edit of `game.json`, compared as crops against the Phase 3 screenshots. Two iterations were needed: the colors, then the rail height (a build, since it's a new field).
- **Goldens:** the imgui baselines moved from `out/baseline/` to `out/baseline-imgui/`, and `out/baseline/{opengl,wgpu}` hold new goldens from this build (`ui_snapshot.jsonl` with `--update-goldens`, debug build, 1280x720), so the layout pass can compare against the egui look.
- **Verification (2026-10-19, macOS, 1280x720):** `ui_snapshot.jsonl` passes on OpenGL and on Wgpu (204 commands each, 501 frames), and its widget trees are the same as Phase 3's; a second OpenGL run matches the 41 new goldens pixel for pixel. `cargo test --workspace` passes and the web build compiles. Only the style files, `style.rs` and this plan changed: no widget code, so no scenario or save could be affected.
- **For the user's review:** a sheet per three screens, imgui baseline on the left and this build on the right, in the session scratchpad under `phase4/sheets/`; the Phase 3 look of each widget is in `phase4/it1_*.png` and `it2_*.png` crops there.
- **Minimap (after the review):** the Phase 0 change that picked a cell's shade from a hash of its coordinates, for repeatable snapshots, gave the minimap a striped look. The minimap draws its shades from its own `Pcg32` again, seeded with a constant and started over whenever the whole minimap is built, so runs still match. The goldens were remade once more after it.

### Phase 5: Cleanup and stale references
- [x] **Web:**
  - Delete `crates/launcher/build.rs` once a web build succeeds without `--allow-undefined`.
  - Delete `web/libc.js`, the importmap in `web/index.html` and `crates/engine/src/platform/web/libc.rs` (with `mod libc;`).
  - In `web-builder`, delete the clang/wasi-sdk setup and `patch_generated_js`, and remove imports that are no longer used.
  - In CI, delete `WASI_SDK_VERSION` and the "Install wasi-sdk" step.
- [x] **Docs:**
  - Rewrite the ImGui references in AGENTS.md (automation notes, the web toolchain bullets, DrawDebugUi, the Engine description), `README.md` (the llvm/wasi-libc steps), and `.claude/rules/rendering.md`.
  - Add a note about styles to AGENTS.md.
  - Add a one-line note at the top of `docs/plans/agent-automation.md` saying the UI has since moved to egui; it's a historical record.
- [x] Run `rg -i 'imgui|im_gui|ImColor|wasi'` over the whole repository (code, comments, scripts, configs, `.claude/`). Also look for printf-style formats, `##` labels whose ids are no longer needed, the `FieldAccessorXY` comment in `common/src/lib.rs`, and dead helpers in `internal.rs`.
- [x] Check the drive-game skill and the `hb-ctl`/MCP docs for descriptions of UI behavior that changed.

**Phase 5 notes (2026-10-19):**
- **Web:** the launcher's `build.rs` (`--allow-undefined`), `web/libc.js` with the page's `env` importmap, and `platform/web/libc.rs` (the C library shims imgui-sys imported: malloc, qsort, vsnprintf, sscanf, 632 lines) are gone, and so are `web-builder`'s clang/wasi-sdk setup and its patch of wasm-bindgen's output. The web build needs only the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` and `wasm-opt`, and CI no longer downloads wasi-sdk. The release build passes the Playwright smoke test in Chrome.
- **Docs:** AGENTS.md says which library the UI runs on, drops the wasi toolchain bullets, and gains the note on the style files; README.md loses the llvm/wasi-libc install step and its explanation; `agent-automation.md` opens with a note that its ImGui details are history. `.claude/rules/rendering.md` was already written for the egui renderer in Phase 2. The drive-game skill and the hb-ctl/MCP docs describe no behavior that changed.
- **What's left on purpose:** the comments in `ui/style.rs` (why fonts have a `scale`) and `automation/commands/session.rs` (why the settle frames are zero) explain decisions by what imgui did, and `internal.rs`'s `TODO(layout)` goes with the layout pass. The `"label##id"` convention stays (the ids keep same-named headers and buttons apart within a window, e.g. the inspector's `Properties` panels), so no `##` suffix was dropped. No helper in `internal.rs` is unused, and the only printf-style format was in the deleted `libc.rs`.
- **Verification (2026-10-19):** the desktop build, the debug and release web builds (no linker flag, no C toolchain) and `cargo test --workspace` pass, and the release web build passes the smoke test in Chrome (load, commands, a placed well, both screenshots, the hotkey download). `rg -i 'imgui|im_gui|ImColor|wasi'` outside the plans finds only the three comments kept above and AGENTS.md's link to this plan.

### Phase 6: Layout pass
Fixes the layout problems seen in the Phase 0 baselines, on top of the egui widgets. Phase 3 leaves `TODO(layout)` notes and the provisions listed there; `rg 'TODO\(layout\)'` finds them. The problems, as seen at 1280x720:
- **Retail: content overflowing the parchment.**
  1. Home menu: the buttons don't fit the scroll's inner area. "About" and "Quit Game" sit on the bottom roller. Sizes and spacing don't adapt to the window's height.
  2. Load Game (home menu), Load / Save and Save Game (in-game): the button row sits on the bottom roller. In Load / Save, the row ("Load Game  Save Game  Cancel") is also wider than the text area and reaches the scroll's edge.
  3. Population: the content fills the scroll, so "Ok" sits on the bottom roller. Resources is close to doing the same.
  4. Building inspector: the status line ("Building not running because it doesn't have the required resources!") isn't wrapped and runs past both edges of the parchment.
- **Retail: tooltips and alignment.**
  5. Build palette: an item's tooltip ("Cost: 4 gold") opens over the item itself and covers its neighbors ("Apothecary", "Hospital").
  6. Settings dialogs: the label and widget columns change from row to row. In Graphics Settings, the checkbox follows its label while the dropdown starts further right. In Game Settings, long labels nearly touch their checkboxes, and the block isn't centered.
- **Developer UI: windows too narrow for their contents.**
  7. Game Systems Debug and Game Configs: tab labels are cut off ("Settle…", "Ambien…", "Engine/…"), and rows are clipped at the window's edge ("Population Per Set…").
  8. Tile Inspector: the title is cut off ("Object | Building - ID(1,40…").

Tasks:
- [x] Fix each problem above, using the Phase 3 provisions: content areas from sprite insets, a scrolling content area when a menu's contents don't fit, wrapped text, tooltips beside the widget, shared label columns, and dev windows that size to their tabs and rows.
- [x] Add a layout check: a `ui.layout_issues` command lists widgets whose rects fall outside their menu's `menu_content` rect, or overlap a sibling. `ui_snapshot.jsonl` runs it on every retail screen.
- [x] Run `ui_snapshot.jsonl` at 1280x720, 1024x768 and 1920x1080, review the screenshots with the user, and update the local goldens.

**Phase 6 notes (2026-10-19):**
- **Content areas (1, 2, 3):** `UiMenuInsets` gained a relative form, `UiMenuInsets::fractions(..)`: fractions of the menu's size, for a menu with a size whose background sprite is stretched to it. The dialogs use the scroll sprite's measured paper area (its rollers and torn edges, `DEFAULT_DIALOG_MENU_CONTENT_INSETS`), so their heading no longer needs a 100-point top margin, the save lists their 95 and 100-point side margins, nor the settings and new-game groups theirs. The message boxes and the building inspector use insets in points for their page sprite. A menu with a size draws its contents in a vertical scroll area (bar only when needed), and `centered_start` no longer centers a group taller than its room above the cursor, so what doesn't fit scrolls instead of being cut.
- **Sizes and spacing (1, 3):** a stacked `UiWidgetGroup` taller than its room closes up its spacing, down to none (`draw_centered_widget_group`), which is what fits the home menu's eight buttons on the paper at 720p. Text lines and the group measurements take the spacing of the container they're in (`internal::item_spacing`, the groups' own spacing) instead of the style's, and the dialog factory passes its spacing to the menu, so the city management lists use a tighter `DEFAULT_DIALOG_MENU_TEXT_SPACING` (4) and Population fits at 720p. Inputs, sliders and dropdowns take their default width from the menu's content width, not its window width.
- **Wrapping and tooltips (4, 5):** the building inspector's body lines wrap at its content width (`UiText::with_wrap_width`). Sprite buttons' tooltips and the build palette's item tooltips are placed beside their widget (`UiTooltipPlacement::BesideWidget`: to its left, else wherever egui finds room, never over it), since those sit along the screen's edges where a tooltip at the pointer covered their neighbors.
- **Settings (6):** the label and widget columns were already shared within a group (Phase 3); the labels' spacing to their widgets went from 5 to 12 points, and the groups center in the content area. No menu has more than one labeled group, so `label_column_width` stays unused.
- **Developer windows (7, 8):** under egui the tab labels wrapped rather than being cut, and the inspector's title bar already allocates its title's width, so what remained was the width: a resizable window now grows to the width of its scrolled rows, up to the screen's edge (`ui::window`), and `ui::tab_bar` asks for its row's width, so Game Configs and Game Systems Debug open wide enough for their tabs and rows. The palette's blank line for an empty section and its trailing padding, kept since Phase 1 for identical screenshots, are gone.
- **Layout check:** `ui.layout_issues` (`automation/commands/ui.rs`, with a unit test) reports each visible widget outside its menu's content area, the innermost `menu_content` whose path prefixes its own, and each pair of visible siblings (same parent path, same menu) that overlap without one holding the other, with a point of tolerance. A dropdown's popup may leave the menu, so list items aren't held to it, and clipped widgets (`visible: false`) are skipped. `ui_snapshot.jsonl` asserts a count of 0 after each of its 23 retail listings.
- **Verification (2026-10-19, macOS):** `ui_snapshot.jsonl` passes at 1280x720 on OpenGL and Wgpu and at 1024x768 and 1440x796 on OpenGL (1920x1080 doesn't fit this display: macOS gives the largest window that does), with every layout assert green, and its widget trees are the same as Phase 4's on both backends; a second 1280x720 OpenGL run matches the new goldens. `smoke`, `ui_new_game`, `ui_build` and `ui_dev_menus` pass on both backends, `determinism.jsonl` gives the same checksum (`5605d579c2a4517c`) and identical runs and saves, `cargo test --workspace` passes, and the release web build passes the smoke test in Chrome. The 1024x768 run first caught a window grown past the screen's edge (the tab row's width), which `ui::tab_bar` now caps at the room on screen. Goldens: `out/baseline/{opengl,wgpu}` (1280x720), `opengl-1024x768` and `opengl-1440x796`; the Phase 4 ones moved to `out/baseline-phase4/`.
- **For the user's review:** sheets with the Phase 4 build on the left and this one on the right, three screens each, in the session scratchpad under `phase6/sheets/`, and the 1024x768 and 1440x796 runs under `phase6/sheets_1024/` and `phase6/sheets_1440/`.

### Phase 7: PR
- [ ] Push `egui-migration`, check CI once, and open the PR into `main`. The PR description summarizes the phases and links the plan.

## Verification

- **Each commit:** `cargo build -p HeritageBuilder`, `cargo test --workspace`, and `cargo run -p web-builder` (web compiles).
- **Scenarios:** run `smoke`, `ui_new_game`, `ui_build`, `ui_dev_menus` and `ui_snapshot` with `--script`, and `determinism.jsonl` twice with the same `--seed` and `--window-size` (identical output and save). Run them on OpenGL (default) and on Wgpu (`"app_api": "Winit"`, `"render_api": "Wgpu"`).
- **Live checks** through the heritage-builder MCP tools or `hb-ctl`: launch with `--map preset:2 --fixed-dt 0.016667`, then use `ui.tree`, `ui.click`, `ui.set`, `input.*` and `screenshot`. Diff screenshots against the goldens with `assert.image path=...`, which writes `-diff.png`.
- **Record/replay:** record `ui_build` and replay it (status 0).
- **Web:** `python3 crates/tools/web-smoke/smoke.py`, plus a manual look at text input and HiDPI crispness in a browser.
- **Saves:** `--smoke-test-saves` after Phase 3. Nothing serialized should change, so no save-version bump is expected.
- **Performance:** compare the frame-stats UI time and draw calls on a busy dev screen (several debug windows open) before and after, in release and debug builds.

## Risks

- **Debug-build speed:** egui is slow when unoptimized. The `opt-level` overrides address this; measure early (spike 1).
- **First-frame sizing:** new Areas and Windows run a hidden sizing pass, and Grid columns settle one frame late. Screenshots and automation should wait a frame after a window opens; `ui.click` already waits for widgets to appear.
- **Automation timing:** egui hit-tests against the previous frame's layout. A click needs the widget to exist on the hover frame, which the current timeline already gives it.
- **Font atlas resets:** a reset re-uploads the whole atlas, which costs a frame spike with large CJK-sized glyph sets. That isn't a concern for the current fonts.
- **egui version churn:** 0.37.0 is two days old. Pin `0.37` and take patch releases; don't upgrade minor versions during the migration.
- **Big swap commit:** Phases 1 and 2 keep it to `engine/src/ui/` plus the two style files. Spikes 1–4 take most of the uncertainty out first.

## Open decisions (defaults apply unless the user decides otherwise)

- **Clipboard:** an in-app buffer for now. The real OS clipboard (glfw/winit/web) comes later.
- **Dev window title bars:** our own (decided by spike 2, see Phase 0).
- **Symbol fallback font:** ship egui's small `emoji-icon-font` (OFL) in `assets/fonts/`.
- **Style file location:** `assets/configs/ui/styles/{dev,game}.json`.

## Working agreements

- Commit each step to `egui-migration`. The user asked for a branch, which overrides the usual commit-to-main habit. Don't push without asking.
- After pushing, check CI once and report the result.
- Keep this plan's checkboxes and notes up to date. It's the source of truth for progress.
