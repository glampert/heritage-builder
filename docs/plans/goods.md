# Goods and production for missions I-1 to I-5

**Status (2026-11-06): Phases 0 and 1 done; Phase 2, the house ladder and storage balance, next.** This plan is milestone M5 of the [roadmap](roadmap.md). It adds the goods and producers that missions I-1 to I-5 need ([campaigns.md](../design/campaigns.md#campaign-i--the-dragon-legacy-đại-long), GDD §8), and rebalances storage against construction costs and house needs. First it changes how goods are saved, so that adding a good later never breaks the save format (user request, 2026-11-06). It was researched against `9635bff` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo test --workspace` and the scenario scripts before pushing.
  - Follow the Saves workflow for the format change in Phase 0.
  - New buildings get their art from the Blender pipeline (roadmap convention "Art on demand").

## Goals

1. **Adding a good doesn't break saves.** After Phase 0, a new `ResourceKind` can be added, at the end or in the middle of the list, without bumping the save version or writing a migration script. Older saves load and the new good starts at zero. Removing or renaming a good still breaks the format, on purpose, and needs a migration.
2. **Buildings follow their config after a load.** The goods a stock accepts and its capacities come from the building's config every time a save loads, so a good added to a storage yard's config reaches the yards in old saves. This also fixes the known issue about stocks keeping their accepted kinds.
3. **Roof tiles** as a construction material, made by the roof-tile kiln from clay, and charged by the civic buildings the GDD's tier table lists (GDD 5.1.1).
4. **The house goods ladder matches the missions.** Mission I-2 teaches "level 2 housing, which needs pottery".
5. **Storage holds what construction costs.** One storage yard can hold the materials for the largest civic building of missions 1–5.

## What exists today

**Goods** (`crates/game/src/sim/resources.rs:29–110`):
- `ResourceKind` is a `bitflags` `u32`, with 10 flags used: Rice, Meat, Fish, Wine, Pottery, Wood, Metal, Clay, Bricks and Gold. Groups (`foods()`, `consumer_goods()`, `raw_materials()`) are spelled out in code.
- A flag's bit position is its index: `ResourceKind::index()` and `bit_index()` take `trailing_zeros()`, and `RESOURCE_KIND_COUNT` sizes the per-good arrays.
- **Capacity:** the GDD's goods table names about 36 goods across the three realms, more than a `u32` holds.

**How goods are saved** (checked in `saves/autosave.json`, version 4):
- A `ResourceKind` serializes by name, through the `bitflags` serde support: `"Rice | Meat | Fish"`. Unit inventories, producer output, storage slot allocations, tasks and stock acceptance all use this, so they don't depend on bit positions.
- **Three arrays are positional**, indexed by bit position:
  - `ResourceStock::counts: [u16; N]` (`resources.rs:830`), e.g. `[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]`;
  - `BuildingStock::capacities: [u8; N]` (`building/mod.rs:1357`);
  - `HouseBuilding::consumption_accumulator: [f32; N]` (`building/house.rs:275`).
- The first two have custom deserializers that pad a shorter array with zeros, so a good added *at the end* already loads. A good inserted in the middle shifts every count after it onto the wrong good without an error.
- `consumption_accumulator` has no such deserializer. A fixed-size serde array needs its exact length, so **adding any good today fails to load every save with a house.** It carries a `#[serde(default)]` shim, which AGENTS.md rules out for save state.

**Accepted goods are frozen at build time:**
- Storage slots copy `resources_accepted` from the config when the yard is built (`storage.rs:481`). Houses take `foods() | consumer_goods()` (`house.rs:546`). Markets and producers take their config's `resources_required`.
- All of them are saved and never refreshed. That is why sample saves made before the storage yard took pottery and bricks still refuse them ([known-issues](../../.claude/rules/known-issues.md)).
- `StorageSlots::slot_capacity` is saved the same way, so a storage rebalance wouldn't reach old saves either.

**Production:** producers are config-driven (`producer_configs` in `assets/configs/buildings/configs.json`). The brick maker is the model for a kiln: `Factory`, `resources_required: ["Clay"]`, output `Bricks`, `deliver_to_storage_kinds: "StorageYard"`, `fetch_from_storage_kinds: "StorageYard"`. The roof-tile kiln already has art and a tile def (`roof_tile_kiln`), but no config and no sector.

**Storage and costs:**
- A storage yard has 8 slots of 4 units. Each slot holds one good, so a yard holds 32 units, at most 4 of each good per slot.
- The costliest civic buildings: the university (8 Wood, 10 Bricks), the governor's palace (16 Bricks, 8 Wood, 4 Metal) and the citadel (24 Bricks, 8 Wood, 8 Metal).
- House levels need: L1 any food; L2 Rice and Meat or Fish; L3 and L4 also Wine. Nothing needs pottery yet, though markets and houses stock it.

**UI:** the Resources dialog (`menu/dialog/city_management/resources.rs`) lists every good but gold, one line each, so each new good adds a line.

## Design

### 1. Name-keyed save format (Phase 0)

The rule is that **a save never stores a good's position, only its name.**

- `ResourceStock::counts` serializes as a map from good name to count, zero entries left out: `"counts": { "Rice": 2 }`. It deserializes into the array by looking up each name. An unknown name is an error: the good was removed or renamed, and that needs a migration.
- `BuildingStock::capacities` serializes the same way, over the accepted goods only.
- `HouseBuilding::consumption_accumulator` gets the same treatment (zero entries left out), and loses its `#[serde(default)]`.
- One helper does all three: `serialize_per_kind` / `deserialize_per_kind`, generic over the value type, in `resources.rs`. Lookups go through `ResourceKind::from_name` (the `bitflags` `from_name`).
- `ResourceKind` widens to `u64`, room for 63 goods besides gold. Names don't change, so the JSON doesn't change for this part.
- **Order of flags:** the declaration order becomes free, so goods can be grouped by kind in the source. Bit order still sets the order of iteration, so it must stay the same on every run (it does: it's compile-time), and changing it changes which good a mixed request such as `Meat | Fish` takes first. That changes game behaviour, not the save.

Save version 5 carries the change. `v4_to_v5.py` rewrites the three arrays into maps, using the version 4 bit order (Rice, Meat, Fish, Wine, Pottery, Wood, Metal, Clay, Bricks, Gold). It also changes nothing else.

**Test:** a unit test serializes a stock, edits the JSON to put a good the code doesn't have yet in the middle, removes it, reorders the keys, and checks the counts come back on the right goods. Phase 1 checks it for real: a version 5 save made before roof tiles exist loads after they're added.

### 2. Config wins on load (Phase 0)

In each building's `post_load`, the goods a stock accepts and its capacities are set again from the building's config, and the counts are kept:
- **Storage:** each slot accepts the config's `resources_accepted`, and `slot_capacity` comes from the config.
- **House:** foods and consumer goods, and the current level's `stock_capacity` (`update_capacities`, as an upgrade does).
- **Service (market) and producer input:** the config's `resources_required` and stock capacity.

A saved good the config no longer accepts stays accepted while the stock holds any of it, so nothing is lost to a changed list. A lowered capacity clamps the counts, as a house downgrade already does, since the code assumes no stock holds more than its capacity. A storage yard gains slots when its config has more, and never loses any. The accepted goods stay in the save (removing them would make every stock depend on its owner rebuilding it); they are just overwritten.

### 3. Roof tiles and the kiln (Phase 1)

- **Good:** `RoofTiles`, grouped with the raw materials (construction materials). The player reads "Roof Tiles" (`ResourceKind::display_name()`). The storage yard accepts it.
- **Producer:** the roof-tile kiln, `Factory`, clay in, 1 roof tile per 20 s (like the brick maker), delivered to and fetched from storage yards. `sector: IndustryAndResources` and cost 10, as the brick maker. The art is already in.
- **Walker art:** the delivery walker reuses the brick maker's runner sprite, if goods aren't drawn per kind. Check this when the config lands.
- **Costs:** roof tiles go on the civic buildings that come from mission 3 on: the temple, hospital, theatre, university, governor's palace and citadel. Mission I-2's tax office, police station, apothecary and distillery keep wood and bricks, so I-2 teaches materials with the goods it introduces (see the open decisions).

### 4. House ladder and balance (Phase 2)

- **Level 2 needs pottery,** as mission I-2 says: L2 becomes Rice, Meat or Fish, and Pottery. L3 and L4 keep Wine and add Pottery. The consumption rate for pottery (0.2) stays.
- **Storage:** `slot_capacity` 4 → 8 for the storage yard and granary, so a yard holds 64 units. Today the citadel's cost (24 bricks, 8 wood, 8 metal) needs 10 slots, more than one yard has; with 8 per slot it needs 5, plus room for roof tiles. Phase 2 measures it before choosing: the sizes are the open decision below.
- **`world.add_stock` partial stores:** it checks each good's room separately ([known-issues](../../.claude/rules/known-issues.md)). Fix it to check all the goods together, since the rebalance scenario uses it.
- **Resources dialog:** check it with `ui.layout_issues` once roof tiles add a line, and make it scroll or split into columns if it overflows.

### 5. Automation and scenario

- `world.add_stock`, `world.building_at` and `sim.state` report goods by name already, so roof tiles come for free.
- **New scenario** `crates/game/tests/scenarios/goods.jsonl`: build a clay pit, roof-tile kiln and storage yard, wait for roof tiles in the city stock, then place a temple and check its roof tiles were taken. It runs in CI with the other scenarios.

## Phases

### Phase 0: Save format for goods (medium)

- [x] `ResourceKind: u64`; the per-kind map helpers; `ResourceStock`, `BuildingStock` and `HouseBuilding` serialize by name; drop the `consumption_accumulator` shim.
- [x] `post_load` refreshes accepted goods and capacities from the config (storage, houses, services, producers).
- [x] Save version 5, `v4_to_v5.py`, migrate local saves, `--smoke-test-saves`, regenerate `saves/sample_saves.zip`.
- [x] Unit tests: name-keyed round trip, unknown name rejected, reordered keys, a good added in the middle; and a stock in a save keeps its counts while taking the config's accepted goods.
- [x] Remove the known-issues entry about stocks keeping their accepted goods.

**Notes (2026-11-06):**
- `per_kind_map` in `sim/resources.rs` serializes the three per-good arrays as `{ name: value }` maps without zero entries; `ResourceKind::name()` and the generated `from_name` do the lookups. A compile-time assertion checks the bits stay contiguous from 0. `ResourceStock` and `BuildingStock` lost their hand-written deserializers (the padding they did is no longer needed).
- `BuildingStock` now keeps capacities for its accepted goods only (0 for the rest), so a save lists only those.
- Each archetype's `post_load` calls a `refresh_from_config`: `BuildingStock::refresh_from_config` (houses: foods and consumer goods at the level's capacity; markets: `resources_required` at `stock_capacity`), `StorageSlots::refresh_from_config` (accepted goods, slot capacity, added slots; an allocated slot that's empty and no longer accepted is freed), and the producer input and output stocks (the config's inputs, keeping the counts of those it still asks for; the output's capacity, and its kind once empty).
- **Deviation:** the "good added in the middle" test can't add a flag at test time; `stock_loads_counts_in_any_order` covers it with goods missing from the save and keys out of bit order, and Phase 1 checks it for real.
- The sample saves' houses had no `consumption_accumulator` at all (it was `#[serde(default)]`), so `v4_to_v5.py` adds `{}`.
- Verified: `cargo test --workspace`; all CI scenarios; `determinism.jsonl` twice with the same checksum; the web target checks; `--smoke-test-saves` loads all 13 local saves (the stale v2 `mcp_extended_test.json` set aside). In `test_map_64x64`, saved before the yard took bricks and pottery, `world.add_stock` now stores both in an old yard.

### Phase 1: Roof tiles and the kiln (small to medium)

- [x] `RoofTiles` good, accepted by the storage yard.
- [x] Roof-tile kiln producer config; `sector` and cost on its tile def; remove its known-issues entry.
- [x] Roof tiles on the civic buildings' `materials`.
- [x] A version 5 save from before this phase loads (the Phase 0 promise, checked for real).
- [x] `goods.jsonl` scenario, added to CI.

**Notes (2026-11-06):**
- `RoofTiles` is bit 9, between `Bricks` and `Gold`, so `Gold` moved to bit 10: a good inserted mid-list, with no save-version bump. The version 5 sample saves (made before it) pass `--smoke-test-saves`; `tiny_island`'s yards still read 32 Wine, and an old yard in `test_map_64x64` takes roof tiles.
- `ResourceKind::display_name()` spaces a name's words ("Roof Tiles"). Cost texts, the missing-materials message, the Resources dialog and the inspectors use it; saves, configs and automation keep `RoofTiles`.
- The producer config is named "Roof Tile Kiln", matching the build menu's label, which comes from the tile name. The sector went in through the tile generator's catalog entry (`tile_set.sector`), since the tile def is generated.
- Roof tiles added: temple 4, hospital 3, theater 4, university 5, governor's palace 8, citadel 8. The palace and citadel now use all four material slots (`MAX_CONSTRUCTION_MATERIALS`).
- `construction.jsonl` now places two storage yards: two temples' materials (12 wood, 16 bricks, 8 roof tiles) need 9 slots of 4, one more than a yard has. Phase 2's storage sizes may make one enough again.
- `goods.jsonl`: water beside a clay pit (it must be placed near water), the kiln and a yard; after 300 simulated seconds the yard holds 8 roof tiles, and a temple takes 4. It saves `out/goods.png`. The kiln's delivery walker needed no new sprite.
- Verified: `cargo test --workspace`, every CI scenario including `goods`, `determinism.jsonl` twice, the web target, `ui.layout_issues` on the industry build menu and the Resources dialog, and `sync_tileset.py --check`.

### Phase 2: House ladder and storage balance (small)

- [ ] Pottery in house levels 2–4.
- [ ] Storage slot capacity, measured against the civic costs and house needs.
- [ ] `world.add_stock` checks all goods together; remove its known-issues entry.
- [ ] The Resources dialog passes `ui.layout_issues` with the new goods.
- [ ] Update `renovation.jsonl`, `smoke.jsonl` and the tests if the ladder change moves them.

### Phase 3: Documentation (small)

- [ ] AGENTS.md: the goods save rule ("names, not positions") and how to add a good.
- [ ] Roadmap: M5 status and the save-version log.

## Risks

- **Iteration order:** widening and reordering flags must keep the simulation deterministic. `determinism.jsonl` runs twice in CI and catches it.
- **Missed holder:** a stock whose owner doesn't refresh it on load keeps its saved goods, which is today's behaviour, not a crash.
- **Ladder change:** making level 2 need pottery could stall the sample saves' houses, which have no pottery chain. It's the mission design, but check the smoke test and the sample saves after Phase 2.

## Open decisions

1. **Vegetables and other new foods.** The roadmap named vegetables, but missions 1–5 don't need them. Default: leave them out of M5. Phase 0 makes adding them later cheap, when a mission or M7's farming needs them.
2. **Which buildings cost roof tiles.** Default: the civic buildings from mission 3 on (temple, hospital, theatre, university, palace, citadel), not mission 2's tax office, police station, apothecary and distillery.
3. **Storage sizes.** Default: slot capacity 8 for the storage yard and granary, confirmed by the Phase 2 measurement.
4. **Common house level 5.** Missions 1–5 ask for levels 1 and 2 (mission 2's goal) and Prosperity 30 (mission 4), which levels 1–4 can give. Default: no level 5 in M5; revisit with M9's ratings.
5. **A data-driven goods list.** Goods stay a compile-time `ResourceKind`. A goods list in `assets/configs/` would follow the roadmap's "data first" convention better, for realm-specific goods in phases 3 and 4. Default: not in M5. The name-keyed saves from Phase 0 are what such a change would need, so it stays open.
