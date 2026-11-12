# SimContext refactor: read-only and mutable contexts

**Status (2026-11-12): Done; follow-ups open.** A technical-debt cleanup, outside the [roadmap](roadmap.md)'s milestones. It splits `SimContext` into a read-only `SimContext<'game>` and a `SimContextMut<'game>`, so that the world step's "no mutation" rule is checked by the compiler instead of by `debug_assert!(!self.is_read_only)`, and gives both a lifetime so a context can't outlive what it points into. Researched against `1cf4bcfc` on `claude`.

## How to resume this work

- Work through the [steps](#steps) in order and tick them as they land, with a short dated note (deviations, read-path mutations found, what was verified). This file is the source of truth for progress.
- The [follow-ups](#follow-ups) are open work for later, not part of this change.
- Follow [AGENTS.md](../../AGENTS.md): keep desktop and web compiling, run `cargo test --workspace` and the scenario scripts.

## Why

`SimContext` ([sim/context.rs](../../crates/game/src/sim/context.rs)) bundles `RawPtr`s to the RNG, `Search`, the unit task manager, `World`, `TileMap`, `Finance`, `GameCalendar`, `CityRisks` and `SimCmds`. It is always passed as `&SimContext` and hands out `&mut` through those pointers. The world step's update (`World::update`, `GameSystems::update` and `on_calendar_events`) gets a "read-only" context, and every mutating accessor asserts `!self.is_read_only`. So a mutation on the read path is caught only at runtime, in a debug build, and only when a test or scenario reaches it. And the context has no lifetime: nothing but a comment stops it from being kept past the borrows it was made from.

## Decisions

- **Lifetime-bound wrapper, not a redesign.** The raw pointers stay, private to `context.rs`. `World::update(&mut self, ..)` iterates `&mut Building` / `&mut Unit` while the same context reads the `World` (and units moving read the `TileMap`), which plain references can't express. A `PhantomData<&'game mut ()>` ties each context to the borrows it was built from.
- **The RNG, the task manager and `Search` stay on the read-only `SimContext`** as shared resources the read path may mutate, as before: `rng_mut`, `random_range`, `task_manager_mut` and the path queries. Building and unit updates roll the RNG (outbreaks, patrol biases, shuffles) and allocate unit tasks for spawn requests; deferring the draws would change determinism and the scenario checksums.
- **`SimContextMut`'s methods take `&self`** and return `&mut` through the pointers, as before. `&mut self` would break the pattern `let b = ctx.find_building_mut(..); b.foo(ctx)` all over the code; see the follow-ups.

## Design

```rust
pub struct SimContext<'game> {
    rng, search, task_manager, world, tile_map, finance, calendar, risks: RawPtr<..>,
    delta_time_secs: Seconds,
    is_world_teardown: bool,
    _borrow: PhantomData<&'game mut ()>, // covariant: &'a SimContext<'g> coerces to &'a SimContext<'a>
}

pub struct SimContextMut<'game> {
    base: SimContext<'game>,
    cmds: RawPtr<SimCmds>, // only the mutable context reaches the command queue
}

impl<'game> Deref for SimContextMut<'game> { type Target = SimContext<'game>; }
```

- `is_read_only` and its asserts go away. The read API (queries, path searches, `rng_mut`, `random_range`, `task_manager_mut`) is on `SimContext`; everything that changes the world, the tile map, the finances, the risks or queues commands is on `SimContextMut`.
- Through `Deref`, a `&SimContextMut` goes wherever a `&SimContext` is expected, so read-only helpers take `&SimContext` and serve both paths.
- Constructors: `Simulation::new_sim_context` is safe and borrows `&'g mut` its parts, for callers outside `sim/` (the session, the UI, tests). Inside `Simulation`, where the world is aliased on purpose, `unsafe fn from_raw` builds them, with a SAFETY note at each call.
- The world step moves into `Simulation::step_world` (lock, world update, systems, unlock, `SimCmds`) and `Simulation::update_unit_navigation`, which the test harness calls too, so tests step the world exactly as the game does without building aliased contexts.
- Sub-contexts get the same split: `BuildingContext` (read, archetype `update`) and `BuildingContextMut` (spawn, despawn, visits).

## Steps

- [x] **1. `context.rs`**: the two types, `Deref`, the split impls, the constructors; the macros go.
  - 2026-11-12: done. The constructors take a `SimContextParts` (the parts as `&mut`) instead of a long argument list; `SimContext::from_raw` and `SimContextMut::from_raw` are `pub(super) unsafe`. `SimContext` holds no `cmds`: only the mutable context reaches the queue.
- [x] **2. `sim/mod.rs`**: `from_raw` contexts, `step_world`, `update_unit_navigation`, the debug-UI draws.
  - 2026-11-12: done. `Simulation::aliased_context` and `aliased_context_mut` (private, `unsafe`) wrap `from_raw` for the world step, unit navigation, `reset_world` and the debug-UI draws; the destruction requests and the paused update use the safe `new_sim_context`, since nothing else is borrowed there.
- [x] **3. `SimCmds`**: `execute` and every callback take `&SimContextMut`; `ImmediateModeSimCmds` keeps a lifetime-erased pointer; `Spawner` holds a `&SimContextMut`.
  - 2026-11-12: done. So do the unit task completion callbacks that get the building (`PatrolCompletionCallback`, the fetch, deliver, harvest, patrol and respond ones) and `SpawnPool::spawn/despawn/clear`. The follow-path, settler and post-despawn callbacks run from task updates and stay read-only.
- [x] **4. Propagate the types**, compiler-guided: a function takes `&SimContextMut` only if it mutates. A mutation reached from a read-only root (`World::update`, the entity and archetype updates, `GameSystem::update` / `on_calendar_events`, unit task runs) is a latent bug: queue it through `SimCmds` and note it here. `BuildingContext` / `BuildingContextMut` split.
  - 2026-11-12: done. `BuildingContextMut` derefs to `BuildingContext` and its own `sim_ctx` (a `&SimContextMut`) shadows the base's, so mutating code reads as before; `Building::new_context_mut` makes one. Archetypes' `spawned`, `despawned`, `visited_by` and `draw_debug_ui` take it, as do the house's merge and upgrade paths (they run from `SimCmd::UpgradeHouse`). Every `draw_debug_ui` and `GameSystem::paused_update` take `&SimContextMut`.
  - Read-path mutations found: none reached the mutable API at run time, but four functions chose at run time between queuing and applying (`cmds.is_deferred()`), so their types took a mutable context even on the world step: `Building::ignite`, `start_outbreak`, `end_outbreak` and `update_road_link`. Each is now two: one that queues the tile change through `SimCmds` (asserting deferred commands, no mutable context) for the world step, and `ignite_now`, `start_outbreak_now`, `end_outbreak_now` and `update_road_link_now`, which take `&SimContextMut` and apply it at once (the inspector, automation, the test helper, spawning, a medic's cure).
- [x] **5. Callers outside `sim/`**: `GameUiContext::new_sim_context`, `GameSession::with_sim_context`, menus, undo/redo, automation, the minimap's lens refresh (which borrowed the minimap mutably while its context read the tile map).
  - 2026-11-12: done. The borrow checker caught two aliasing spots: the building inspector read the world and the simulation while its context borrowed them (it reads through the context now), and the minimap's lens refresh. `Minimap::refresh_lens` is now an associated function over a `SimContextMut`: it takes the lens data out, computes it while the minimap is only read, and puts it back.
- [x] **6. Tests**: `test_utils::tick` on `step_world`; `new_readonly_sim_context` removed.
  - 2026-11-12: done. `tick` is `Simulation::update_unit_navigation` then `Simulation::step_world` with no game systems. The test callbacks that a deferred command calls take `&SimContextMut`, and the risks test's `infect` helper calls `start_outbreak_now`.
- [x] **7. Docs**: AGENTS.md's `SimContext` and tests notes.
  - 2026-11-12: done. The older plans (city-risks, agent-automation) keep their mentions of `new_readonly_sim_context` and the runtime assert as history.

## Verification

Verified on 2026-11-12, macOS, after the change:

- Desktop (`cargo build -p HeritageBuilder --release`) and web (`cargo check -p HeritageBuilder --target wasm32-unknown-unknown --no-default-features --features web`) build with no warnings in the game crate. `cargo clippy -p game` reports nothing in the changed code.
- `cargo test --workspace`: every suite passes, the 11 custom-harness suites in `crates/game/tests/` included.
- Scenarios under `--default-configs --window-size 1280x720`, all ok: `smoke`, `water_edges`, `calendar`, `construction`, `renovation`, `goods`, `finance`, `risks`, `ui_new_game`, `ui_build`, `ui_dev_menus`, `minimap`.
- `determinism.jsonl` with `--seed 42`: checksum `8a19fd554e690877` before and after the change, and a byte-identical `saves/determinism.json`.
- `--smoke-test-saves`: the 11 sample saves load and run. (A stale local save from version 2, not in `sample_saves.zip`, fails as it did before.)

The plan for checking:

- Desktop and web (`cargo run -p web-builder`) build; `cargo test --workspace` passes.
- Scenarios under `--default-configs`: `smoke`, `construction`, `goods`, `finance`, `risks`, `ui_build`, `ui_dev_menus`.
- `determinism.jsonl` with `--seed 42 --window-size 1280x720` prints the same checksum as before the change (the refactor must not change behaviour).
- `--smoke-test-saves` on the sample saves.

## Follow-ups

Open work for later, not part of this change.

- **Real references instead of raw pointers.** Make the world update stop needing `&mut World` (interior mutability per entity in `SpawnPool`, or take-out/put-back iteration), so the contexts can store `&'game World` / `&'game TileMap` and the `from_raw` constructors go.
- **`SimContextMut` with `&mut self`.** Mutable accessors borrow the context exclusively, so the borrow checker sees aliasing `&mut`s, e.g. a `&mut Building` kept while `find_building_mut` is called again. Needs call sites rewritten around ids instead of held references.
- **The task manager off the read path.** Allocate the unit tasks for spawn requests when the commands run, so `task_manager_mut` moves to `SimContextMut`.
- **The RNG off the read path**, or a per-step stream, if the read side ever needs to be strictly side-effect free; it changes the draw order and so the checksums.
