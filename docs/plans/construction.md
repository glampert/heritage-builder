# Building costs, construction sites and house renovation

**Status (2026-11-05): In progress. Phases 0, 1 and 2 done.** This plan implements GDD sections 5.1.1 (building costs) and 5.1.2 (construction cycle) in [docs/design/gdd.md](../design/gdd.md). It is milestone M3 of the [roadmap](roadmap.md). It was researched against `d0696fe` on `main`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo test --workspace` and the scenario scripts before pushing.
  - Route world edits through `TilePlacement`, `SimCmds` and `TileMap` setters.
  - Follow the Saves workflow for the save-format change in Phase 2.

## Goals

1. Buildings cost gold plus construction materials. Materials are taken at placement from a city-wide pool made up of every storage yard. Basic buildings cost gold only.
2. A placed building becomes an inert building site for a short game-time timer before it starts working:
   - roads, clearing and plazas stay instant;
   - decorative buildings take about 5–10 s at normal speed;
   - basic buildings about 10–20 s;
   - civic buildings about 30–45 s;
   - grand buildings about 60–90 s.

   Undo, or clearing a site, refunds everything. Demolishing a finished building refunds nothing.
3. House upgrades become short renovations. A house only starts renovating once its needs have been met for a while. Downgrades wait out a grace period. This stops houses flickering between levels.
4. A config value scales construction time, where 0 means instant. Tests, scenarios and a "fast build" option use it.
5. No labour cost for construction. Workers stay reserved for monuments (user decision, 2026-11-05).

## What exists today

**Placement:**
- The player and the developer editor share one path: `TilePlacement::place(ctx, cell, def, subtract_tile_cost, undo_redo)` (`crates/game/src/menu/mod.rs:538`). It runs the `Spawner`, then updates road junctions and water shores, then records undo.
- `Spawner` (`crates/game/src/world/object.rs:520`) charges gold in `try_spawn_building_with_tile_def` (`:632`), through `can_afford_tile` and `subtract_tile_cost` (`:768–787`).
- **Trap:** the spawn commands in `sim/commands.rs:684` and `:851` build a `Spawner` with default flags. Deferred spawns, such as a settler's house, would therefore also be charged. Today only zero-cost tiles go through that path.

**Costs and treasury:**
- The cost is `TileDef.cost: u32` (`tile/sets.rs:602`), in `assets/tiles/objects/tile_set.json`.
- Money is `GlobalTreasury { gold_units }` (`sim/resources.rs:739`) plus the gold held by tax offices.
- `can_afford` (`:772`) compares against `world.stats()`, which is recounted every sim step rather than read live.

**Storage:**
- `StorageBuilding.storage_slots` holds up to 8 slots of 4 units each (`building/storage.rs:102`; configs at `assets/configs/buildings/configs.json:448`).
- The removal accessors return 0 unless the yard is staffed (`:234–266`). `StorageSlots::remove_resources` (`:612`) has no such check.
- There is no live city-wide stock query. `world.stats().resources` is a tally.

**Buildings:**
- `Building { id, map_cells, road_link, kind, workers_update_timer, archetype }` (`building/mod.rs:252`).
- There is no state enum, and `is_operational` (`:1048`) means staffed plus supplied.
- `has_access_to_service` → `is_near_building` (`sim/context.rs:510`) doesn't check that the service building is operational.

**Houses:**
- `upgrade_update` (`building/house.rs:728`) runs every 10 s and queues `upgrade_house` as soon as `can_upgrade` or `can_downgrade` holds. There is no hysteresis.
- `perform_upgrade` (`:798`) swaps the tile through `house_upgrade::try_replace_tile` and keeps the building id.

**Undo:** a 16-deep stack that isn't saved (`undo_redo.rs`). Undoing a placement refunds the tile's gold into `GlobalTreasury`, but `world.stats` isn't updated.

**Time:** sim steps run every 0.5 s. The only game-time unit is `SimConfigs::seconds_per_day = 600`.

**Bug (fixed by roadmap M1 before this plan starts):** the world update's delta is multiplied by speed twice, so building timers run at speed squared (`sim/mod.rs:165–166`):
```rust
let scaled_delta_time_secs = delta_time_secs * self.speed;
let world_update_delta_time_secs = self.update_timer.time_since_last_secs() * self.speed; // already scaled
```

**Rendering:** the renderer only sees tiles, never the building (`tile/rendering.rs:435`). `TileFlags` is a `u16` with bits 12–15 free (`tile/mod.rs:123`).

**Tests that construction will touch:**
- The harness turns on `ignore_tile_cost` and `ignore_worker_requirements` (`tests/test_utils/mod.rs:59`).
- `house_consumption.rs::test_house_downgrades_when_requirements_unmet` expects a downgrade within about 30 s.
- Scenarios: `ui_build.jsonl` asserts the gold left after placing; `smoke.jsonl` and `determinism.jsonl` expect population after a fixed time; `ui_dev_menus.jsonl` sets a house level directly.

## Design

### 1. Cost data

The cost stays on the tile def, where gold costs already live and where the palette and `tiles.list` read them. Two optional fields are added to `TileDef` in `tile_set.json`:

```json
{ "name": "temple", "cost": 60,
  "materials": { "Wood": 6, "Bricks": 8 },
  "construction": "Civic" }
```

- **`materials`** is a `ResourceKind → u16` map that defaults to empty. It is parsed into a small fixed list, `ConstructionMaterials(ArrayVec<StockItem, 4>)`.
- **`construction`** is a tier: `Instant | Decorative | Basic | Civic | Grand`. It defaults to `Basic` for buildings and `Instant` for terrain.

  Durations per tier live in `SimConfigs.construction`, in game seconds:

  | Tier | Starting value |
  |---|---|
  | `Decorative` | 8 |
  | `Basic` | 15 |
  | `Civic` | 40 |
  | `Grand` | 75 |

  Alongside them sit `time_scale: f32` (1.0; 0 means instant) and `house_build_secs` (4). An optional per-def `construction_secs` overrides the tier.

- Monuments don't use these fields. They will get their own staged plan.

Initial tiers and materials for the existing buildings (placeholder balance; roof tiles replace some bricks when M5 adds them):

| Tier | Buildings | Materials |
|---|---|---|
| Basic | small_well, large_well, market, shrine, rice_farm, livestock_farm, fishing_wharf, lumberyard, clay_pit, mine, brick_maker, pottery_kiln, granary, storage_yard, house plots | none |
| Decorative | garden | none |
| Civic | tax_office, police_station, apothecary, distillery | Wood 3–4, Bricks 2–4 |
| Civic | temple, theater, hospital, university | Wood 6–8, Bricks 4–10 |
| Grand | governor_palace | Bricks 16, Wood 8, Metal 4 |
| Grand | citadel | Bricks 24, Wood 8, Metal 8 |

A storage yard holds 32 units in total, so grand costs must fit in what two or three yards can hold. See the [risks](#risks).

### 2. City-wide stock: query and withdraw

A small module, `world/city_stock.rs`, works over every storage yard:

- `CityStock::available(world, kind) -> u32` sums `StorageSlots` across every spawned storage yard that isn't a site. It ignores the staffing check, because construction takes goods instantly; see Q1.
- `CityStock::can_afford(world, treasury, gold, &materials) -> Result<(), MissingCost>` reports what is missing, both gold and each material.
- `CityStock::withdraw(world, &materials) -> Withdrawal` removes from the yards **in pool order**, which keeps it deterministic. It returns a `Withdrawal(SmallVec<(BuildingId, ResourceKind, u16)>)` saying which yard gave what.
- `CityStock::refund(world, &Withdrawal)` puts goods back into the same yards. If a yard has gone or is full, the rest goes to any yard with room, and anything left after that is lost and reported.
- After a withdrawal or refund, the caller adjusts `world.stats` the same way `subtract_gold_units_global` does, so the HUD and goals see the change straight away.

### 3. Charging only when the player builds

- The `Spawner` default becomes `subtract_tile_cost = false`. `TilePlacement::place` already passes the flag explicitly, so player and editor placement keep charging. Deferred spawns (settler houses, `sim/commands.rs`) and presets no longer charge. This removes the trap described above.
- `Spawner::can_afford_tile`, `subtract_tile_cost` and `restore_tile_cost` cover materials too:
  - `can_afford_tile` calls `CityStock::can_afford`;
  - `subtract_tile_cost` charges gold, then calls `withdraw`;
  - `restore_tile_cost` refunds both.

  The `Withdrawal` is kept on the building's site state (§4), so a refund can return materials where they came from.
- **Messages:**
  - `TilePlacementErrReason` gains `CannotAffordMaterials`.
  - `menu::placement_operation` (`menu/mod.rs:492`) greys the building out.
  - The palette tooltip (`menu/in_game/palette.rs:179`) lists gold and materials, showing missing ones in red.
- **Starting stock** is map content. A mission save places a storage yard holding the stock, so the engine needs no change. [Roadmap](roadmap.md) M13 checks each mission against the bootstrap rule.

### 4. Construction sites

A site is a normal building object with construction state. This keeps building ids, road links, blocking tiles and undo working unchanged.

```rust
// building/construction.rs
#[derive(Serialize, Deserialize)]
pub struct ConstructionSite {
    total_secs: Seconds,     // tier duration × time_scale, fixed at placement
    elapsed_secs: Seconds,   // game time
    paid_gold: u32,
    withdrawal: Withdrawal,  // for exact refunds
}

pub struct Building { ..., construction: Option<Box<ConstructionSite>>, ... }
```

**Who creates sites:**
- `TilePlacement::place` sets `Spawner::set_build_as_site(true)` for player placement whenever the tier isn't `Instant` and the time scale is above 0.
- The developer editor places finished buildings unless the cheat `build_sites_in_editor` is on (Q3).
- World spawns, presets and loaded maps create finished buildings.
- Settler houses are a special case: `Settler::on_settled` (`system/settlers.rs:218`) spawns house0 as a site of `house_build_secs`, and the population moves in when it completes.

**While a building is a site:**
- `Building::update` (`building/mod.rs:271`) only advances `elapsed_secs` by the world delta, then returns. There are no worker updates, no archetype update, no services and no production.
- `is_operational` returns false, and so do the resource accessors, `visited_by` and `tally`. Sites count towards nothing in `world.stats`.
- `is_near_building` (`sim/context.rs:510`) and `find_storage_fetch_candidate` (`unit/task/common.rs`) skip sites. Whether they should also skip unstaffed buildings is Q2.
- The tiles block walkers as a building does. The site's road link is still computed, so the building works the moment it completes.
- Fire and attacks (future milestones) treat a site as a building with low hit points.

**On completion:** clear `construction`, clear the tile flags (§6), and reset `workers_update_timer` so hiring starts at once. Queue a `SimCmd::ConstructionCompleted` for sound, a toast and future messages.

**Cancel and undo:**
- `TilePlacement::clear` (`menu/mod.rs:591`) refunds a site in full (gold plus `restore`), and refunds nothing for a finished building.
- Undo of a placement already calls `clear_tiles(restore_tile_cost = true)` (`undo_redo.rs:205`). It refunds the same way, finished or not, matching the GDD rule that undo refunds in full.
- `undo_redo_record` stores the site state, so redo places a fresh site and charges again.

### 5. House renovation and downgrade grace

These changes stay inside the house code, and `perform_upgrade` (`house.rs:798`) is unchanged:

- **New fields:** `HouseUpgradeState` (`house.rs:1355`) gains `upgrade_ready_secs`, `downgrade_pending_secs` and `renovation: Option<CountdownTimer>`. `HouseConfig`, which has `serde(default)`, gains:
  - `renovation_delay_secs` (needs met continuously before starting, default 30);
  - `renovation_secs` (default 10);
  - `downgrade_grace_secs` (default 40).
- **Each `upgrade_update` tick (every 10 s):**
  - If the house can upgrade, add to `upgrade_ready_secs`; otherwise reset it to 0. Once it passes the delay and no renovation is running, start one and set the `Renovating` flag.
  - Tick the renovation down. When it ends, if the house still can upgrade, queue `cmds.upgrade_house(Upgrade)` as today; otherwise abort. Clear the flag either way.
  - If the house should downgrade, add to `downgrade_pending_secs`; otherwise reset it. Once it passes the grace period, queue the downgrade. A downgrade cancels any running renovation.
- **Expansion:** upgrades that expand into neighbouring cells (`try_expand_house`) check those cells only at completion, as today. If the expansion fails, the renovation aborts and the delay starts again.
- **Debug setter:** the inspector's level setter and `UpgradeHouse` commands bypass the timers, so `ui_dev_menus.jsonl` keeps working.

### 6. Visuals

The renderer can't see buildings, so site state reaches it through the four free `TileFlags` bits (Q4):

- **Bit 12, `UnderConstruction`:** draw a scaffold sprite instead of the building's own.
- **Bits 13–14, `ConstructionStage`:** 0–3, set from progress at 0%, 34% and 67%.
- **Bit 15, `Renovating`:** draw the house sprite, then a scaffold overlay.

**Updating the flags:** `Building::update` writes them through a `SimCmd`, because the tile map is locked during the update. It only does so when the stage changes, so at most three edits per site.

**Drawing:** `TileMapRenderer::draw_tile` (`tile/rendering.rs:435`) looks up `scaffold_{n}x{n}` by footprint from a cache built at load, and draws frame `stage`. A missing size falls back to the nearest smaller one, drawn larger.

**Art** comes from the placeholder pipeline ([placeholder-art.md](placeholder-art.md)): scaffolds for 1×1 to 5×5 in 3 stages, plus a renovation overlay for each house size.

**Inspector:**
- `BuildingInspector` (`menu/in_game/inspector/mod.rs:211`) refreshes the construction progress every frame; it's a snapshot today.
- `gather_building_stats` shows "Under construction: 62%".
- The house panel shows renovation progress and the downgrade warning.

### 7. Configuration, cheats and automation

- `SimConfigs.construction.time_scale` can be edited in the debug UI and with `config.set`. A cheat, `instant_construction`, forces 0.
- The test harness sets the scale to 0, and so does script mode unless a scenario sets it. That keeps every existing test's timing.
- **Automation reports:**
  - `world.place` adds `site: bool` and `completes_in_secs`.
  - `world.building_at` and `world.inspect` add `construction_progress` and `renovation_progress`.
  - `world.stats` adds `city_stock`.
- **New scenarios:**
  - `construction.jsonl`: place a civic building with materials in storage and check what was taken. Check it isn't working while a site, `wait.sim_seconds` past its duration, then check it works. Then cancel a second site and check the full refund.
  - `renovation.jsonl`: check a house waits out the delay before upgrading, and the grace period before downgrading.

### 8. Saves

`Building.construction` and the `HouseUpgradeState` fields change the save format:
- bump `CURRENT_SAVE_VERSION` to 4 (M1's calendar took version 3);
- add `crates/tools/save-migration-scripts/v3_to_v4.py`, which sets `construction: null` on every building and zeroes the new house fields;
- migrate `saves/` and run `--smoke-test-saves`;
- regenerate `saves/sample_saves.zip`.

Do this once, in Phase 2, with all the serialized fields from Phases 2 and 3 together (Q5). The `TileDef` fields come from asset JSON, so they don't affect saves.

## Phases

### Phase 0: Prerequisites (small)

- [x] The speed-squared fix from [roadmap](roadmap.md) M1 is in, so building timers run at true game speed.
- [x] Change the `Spawner` default to `subtract_tile_cost = false`, and check that every player path passes `true` (`menu/mod.rs`, `automation/commands/world.rs` `cmd_place`, undo's `place_tiles`).
- [x] Add a test: a settler house spawned through `SimCmds` doesn't change the treasury with the cost cheat off.

Done when: `cargo test --workspace` and all scenarios pass unchanged.

**Notes (2026-11-05):**
- M1 is on the `claude` branch (`6e685b4`), not yet merged into `main`.
- Two player paths relied on the old default and now turn it on:
  - `TilePlacement::place_road_segment`, which charges per road tile;
  - the palette's `placement_operation`, whose `can_afford_tile` check passes everything when the flag is off.
- `cmd_place` and undo's `place_tiles` already went through `TilePlacement::place`, which passes the flag. Undo of a clear passes `false` and redo of a placement `true`, as before.
- Houses cost 0, so a settler's house was never really charged. The new test in `tests/construction.rs` spawns a small well (cost 2) through `SimCmds` instead. It fails with the old default (98 gold left, not 100).
- Verified: `cargo test --workspace` and every CI scenario pass, and the determinism checksum is unchanged (`1e4ac1bec6ae8ac5`).

### Phase 1: Material costs (medium)

- [x] Add `materials` and `construction` to `TileDef` (`tile/sets.rs`), with defaults, and fill in `assets/tiles/objects/tile_set.json` from the table in §1.
- [x] Add `world/city_stock.rs`: `available`, `can_afford` (returning `MissingCost`), `withdraw` (in pool order, returning a `Withdrawal`), and `refund`. Adjust `world.stats` straight away.
- [x] `Spawner` charges and refunds materials; add `TilePlacementErrReason::CannotAffordMaterials`.
- [x] Undo of a placement refunds materials. Clearing a finished building refunds nothing.
- [x] Palette tooltip and greying out; `tiles.list` reports materials.
- [x] Unit tests (a new `tests/construction.rs` with the custom harness): affordability across several yards; deterministic withdrawal order; exact refund; refund into another yard when the first is gone.

Done when: on a test map, a temple can't be placed without bricks, is placed once bricks are delivered, and undoing it returns the bricks.

**Notes (2026-11-05):**
- **Types:**
  - `ConstructionMaterials` and `ConstructionTier` live in `building/construction.rs`; Phase 2 adds `ConstructionSite` there.
  - Materials are written as a JSON map and taken in the file's order. A count is a `u32`, like `StockItem`'s.
  - `TileDef::construction_tier()` defaults to Basic for buildings and Instant otherwise.
- **Storage slots:** `StorageSlots::available_resources` and `remove_resources` only see the first slot holding a kind, so a yard with 6 bricks in two slots reported 4. The city stock uses new helpers over all slots instead: `total_resources`, `take_resources`, `put_resources` and `receivable_total`. The old accessors, which walkers use, are unchanged.
- **Refunds:**
  - Undo refunds gold through `GlobalTreasury::add_gold_units_global`, which also updates `world.stats`, fixing the roadmap's undo bug.
  - Materials go into any yard with room (`CityStock::deposit`), since a finished building keeps no `Withdrawal`. `CityStock::refund` returns a `Withdrawal` to its own yards, for Phase 2's sites.
- **Messages:**
  - A refusal names what's missing, e.g. "Cannot afford tile 'temple'. Cost: 15 gold, 6 Wood, 8 Bricks. Missing: 6 Wood, 8 Bricks".
  - The HUD and developer palette tooltips list materials.
  - **Deviation:** the tooltip is static text, so missing materials aren't shown in red. The placement preview still turns invalid when the city can't pay.
- **Automation:**
  - `tiles.list` reports `materials` and `construction`, and `world.stats` reports `city_stock` (the storage yards' tally).
  - The new `world.add_stock` fills the yards, adding nothing if they lack room.
- **Scenarios:**
  - `ui_build.jsonl` places its Apothecary through the HUD, so it now places a storage yard and stocks it first. It also checks the stock is empty afterwards; the gold left goes from 88 to 78.
  - The drive-game skill's build recipe says the same.
  - `ui_snapshot.jsonl` hovers the Apothecary, whose tooltip is now longer, so its goldens need remaking.
- **Verified:**
  - `cargo test --workspace` passes, including `tests/construction.rs`.
  - Every CI scenario passes, and the determinism checksum is unchanged.
  - In the game, `world.place` refuses a temple without materials, places it once `world.add_stock` provides them and leaves exactly 1 spare brick, and the HUD tooltip reads "Cost: 15 gold, 6 Wood, 8 Bricks".

### Phase 2: Construction sites (medium to large)

- [x] Add `ConstructionSite` and `Building.construction`. `Spawner::set_build_as_site`; `TilePlacement::place` sets it for non-instant tiers.
- [x] Gate `Building::update`, `update_workers`, `is_operational`, the resource accessors, `visited_by` and `tally`. `is_near_building` and `find_storage_fetch_candidate` skip sites.
- [x] Completion, `SimCmd::ConstructionCompleted`, and the stage flags written through a `SimCmd`.
- [x] Cancelling a site (clearing or undo) gives a full refund.
- [x] Settler houses spawn as short sites.
- [x] `SimConfigs.construction` (tier durations, `time_scale`, `house_build_secs`), the `instant_construction` cheat, and scale 0 in the test harness and script mode.
- [x] Renderer: the `UnderConstruction` flag and stage → scaffold sprite lookup, with a fallback, using placeholder scaffold sprites until the art pipeline delivers them.
- [x] Inspector progress with a per-frame refresh; automation fields.
- [x] Save v4 and migration, together with Phase 3's fields (Q5).
- [x] `construction.jsonl` scenario, added to CI.

Done when: a placed building shows scaffolding, does nothing until the timer runs out, then works; cancelling refunds everything; all old tests and scenarios pass with scale 0.

**Notes (2026-11-05):**
- **Placement:**
  - `TilePlacement::place` takes a `ConstructionMode` (Site or Finished), passed on to `Spawner::set_construction_mode`.
  - `TilePlacement::new()` builds sites (the in-game HUD); `TilePlacement::for_editor()` builds finished buildings unless the `build_sites_in_editor` cheat is on (Q3's default).
  - `world.place` builds sites.
  - Undo and redo place a site again when the recorded tile carried `UnderConstruction`, charging it, since the site was refunded. Without that, undoing the clear of a site would give a free building.
- **Completion:** done through `cmds.defer_building_update` rather than a new `SimCmd` variant. The closure clears the tile flags, moves in a settler house's residents, and logs "<name> completed at <cell>." on the `construction` channel. Phase 4's sound and toast go there too.
- **Skipping sites:**
  - `is_near_building` (service coverage), the settler task's fallback to houses with room, and `CityStock` skip sites.
  - `find_storage_fetch_candidate` and deliveries skip them already, through the gated resource accessors.
  - `Building::add_population` refuses a site; its residents move in on completion.
- **Rendering (deviation):** there are no scaffold sprites yet, and adding scaffold tile defs would need a new tile category. Instead, a site draws its building twice:
  - a faint, sandy outline of all of it;
  - the part built so far, solid and tinted, rising from the ground: the bottom quarter at stage 0, all of it at stage 3.

  The tile flags use bits 12–14 (`UnderConstruction`, `ConstructionStage0/1`); bit 15 stays free for Phase 3's `Renovating`. M2's scaffold art can replace the drawing in `TileMapRenderer::draw_tile` (`construction_site_rect`).
- **Script mode (deviation):** script mode doesn't default to instant construction. No scenario needed it except `ui_build.jsonl`, which now waits 16 s for its storage yard. The determinism checksum changed (now `7d03fb459db4f1bf`, the same in debug and release builds), since its buildings are sites now and the save version is 4. The test harness turns on `instant_construction`.
- **Not done:** a per-tile-def `construction_secs` override (§1); no building needs one yet.
- **Inspector:** the HUD inspector remembers the building it shows and redraws it whenever a site's whole-percent progress changes: "Under construction: 61%" and the seconds left. It switches to the normal panel on completion.
- **Automation:**
  - Building JSON (`world.place`'s `object`, `world.building_at`, `world.list`, `world.inspect`) has `construction` (progress, stage, remaining and total seconds) while it's a site.
  - `world.add_stock` and `CityStock` skip yards that are still sites.
- **Save version 4** adds `Building::construction` (`v3_to_v4.py`). Phase 3's house fields will join the same version, since v4 hasn't been merged yet.
- **Verified:**
  - `cargo test --workspace` passes, including 13 tests in `tests/construction.rs`: a site is inert until built, clearing refunds exactly, undo/redo, no refund for a finished building, settlers move in on completion, and the editor mode.
  - Every CI scenario passes, as does `construction.jsonl` (30 steps), on debug and release builds, with determinism and record/replay.
  - The 12 local saves migrate to v4 and pass `--smoke-test-saves`, and `sample_saves.zip` holds 11 v4 saves. A site saved at 54% loads and completes.
  - The web target builds.
  - Screenshots show sites at each stage and the inspector following a site until it completes.

### Phase 3: House renovation (small to medium)

- [ ] `HouseUpgradeState` timers and the `HouseConfig` values.
- [ ] Renovation start, completion and abort; the downgrade grace period; the `Renovating` flag and overlay.
- [ ] Inspector shows renovation and downgrade warnings.
- [ ] Update `test_house_downgrades_when_requirements_unmet` for the grace period, and add a test that a house whose needs flicker doesn't change level.
- [ ] `renovation.jsonl` scenario.

Done when: a house that keeps its needs met upgrades after the delay plus the renovation time; a house that loses a service for less than the grace period keeps its level.

### Phase 4: Polish and documentation (small)

- [ ] Sound and toast on completion.
- [ ] A "fast build" option in Custom Game settings, which is just `time_scale`.
- [ ] Update AGENTS.md (construction section, test notes) and `.claude/rules/known-issues.md`.

## Risks

| Risk | Mitigation |
|---|---|
| Storage yards hold only 32 units each, so grand costs could need several yards | Keep costs within about 16 per kind for now; revisit yard capacity in the economy pass (M5) |
| Taking materials from unstaffed yards may surprise players who expect staffing to matter | Q1. The rule is easy to flip, since it's one check in `CityStock` |
| Using all four free `TileFlags` bits leaves none for later features | Q4. The alternative is a small per-tile overlay byte, which also means a save change |
| The scale-0 default in tests hides timing bugs | The `construction` and `renovation` scenarios run with real durations |
| Renovation delays change how fast the economy grows | Balance in M13 against the 1-hour mission target |

## Open decisions

**Q1. Can construction take materials from unstaffed storage yards?**
**Decided (2026-11-05):** yes, for paying for a building at placement. Deliveries to monument sites, which are carried by walkers, need a staffed yard; that belongs to the monument plan.

**Q2. Should service coverage ignore unstaffed service buildings as well as sites?**
*Default: no change in this plan.* It's tracked in known-issues; a decision on service coverage (GDD open question) will cover it.

**Q3. Does the developer editor build instantly?**
*Default: yes,* with a cheat to make it build sites when testing.

**Q4. How is site state passed to the renderer: `TileFlags` bits or a new per-tile field?**
*Default: `TileFlags` bits 12–15.*

**Q5. Should all of this go into a single save-version bump?**
*Default: yes, v4 covers Phases 2 and 3.* If Phase 3 slips past another change that bumps the version, it gets its own bump.
