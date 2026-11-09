# City risks: fire and collapse

**Status (2026-11-09): Planned; not started.** This plan is milestone M6 of the [roadmap](roadmap.md). It covers the fire and collapse rows of GDD 5.9: each building's fire and collapse risk builds up over time, a patrol walker from the fire watch or the builders' guild resets it, and a building whose risk runs out catches fire or collapses and leaves rubble. Mission I-3 teaches "the fire watch and the builders' guild (fire and collapse risk)" ([campaigns.md](../design/campaigns.md#3-the-village-of-spirits)). Disease and crime come in Phase 2 of the roadmap. It was researched against `063d306` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- The user is redesigning the UI on another branch. Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo test --workspace` and the scenario scripts before pushing.
  - Follow the Saves workflow for the format change in Phase 0.
  - Generate the new art with the Blender pipeline (art on demand).

## Goals

1. **Every building has a fire risk and a collapse risk.** Each builds up at a rate set in its building config: timber and thatch houses and kilns burn, and every building eventually needs repair.
2. **Patrols keep them down.** A fire watcher's visit resets a building's fire risk, and a builder's visit resets its collapse risk. A well nearby slows the fire risk (GDD 5.9: "also wells and canals nearby").
3. **Neglect has consequences.** A building whose fire risk runs out catches fire: it stops working, the fire spreads to its neighbours, and unless a fire watcher puts it out, it burns down. A building whose collapse risk runs out collapses. Either leaves rubble, which the player clears.
4. **The player can see it.** The building's info panel shows its risks, the minimap's Risk lens shows them across the city, and a fire or a collapse raises a toast and a minimap alert.
5. **Later milestones can add to it.** M7's floods add collapse risk and its dyke breaches are collapses, M8's misfortunes can start a fire, and Phase 2's raiders set fires. Each needs only a call into the same risk state.

## What exists today

**Buildings** (`building/mod.rs`):
- `Building { id, map_cells, road_link, kind, workers_update_timer, archetype, construction }` (`mod.rs:249`). It's saved as part of the world's building pools.
- `BuildingKind` is a `u32` bitflag with bits 0–21 used (`mod.rs:89`). The service kinds are listed in `BuildingKind::services()`. Saves write a kind by name (`"kind": "TaxOffice"`), so new kinds don't move old ones.
- `Building::update` (`mod.rs:270`) returns early for a construction site, then runs the archetype's update. `Building::visited_by` (`mod.rs:627`) refuses visitors while under construction, then dispatches to the archetype. Each archetype refuses walkers it doesn't know: `HouseBuilding::visited_by` (`house.rs:346`) handles only settlers, market vendors and tax collectors.
- Configs are per archetype in `assets/configs/buildings/configs.json` (`house_levels`, `service_configs`, `producer_configs`, `storage_configs`), behind the `BuildingConfig` trait (`config.rs:25`). Tile defs in `assets/tiles/objects/tile_set.json` hold the cost, materials, upkeep, construction tier and build-menu sector.
- Nothing records a building's age, condition or risk.

**Service patrols** (`building/service.rs`, `unit/patrol.rs`, `unit/task/patrol.rs`):
- A `ServiceConfig` with `has_patrol_unit` sends a `patrol_unit` walker every `patrol_frequency_secs`, up to `effect_radius` away (`service.rs:507`). The police station's guard is one: `effect_radius` 40, every 40 s.
- `send_out_patrol_unit` always passes `Some(BuildingKind::House)` as the buildings to visit (`service.rs:528`). `UnitTaskRandomizedPatrol::visit_buildings_along_way` (`patrol.rs:343`) queues a `SimCmds::visit_building` for each matching building next to a road-link cell the walker stands on.
- `SimCmd::VisitBuilding` panics if the building is gone when it runs (`commands.rs:981`). Nothing destroys buildings during a world step today, so that hasn't mattered.

**Despawning:** `SimCmds::despawn_building_with_id` and `despawn_tile_at_cell` exist (`commands.rs:485`, `:569`). The player's demolition goes through `TilePlacement::clear` (`menu/mod.rs:624`). A building's `despawned` hook discards its walkers' spawn promises (`service.rs:174`, `house.rs:315`).

**Tiles:**
- `TileKind::Rocks` tiles are object-layer tiles with no game object (boulders). They block placement and walking (`PathNodeKind::Rocks`), show as rock cover on the minimap, and the clear tool removes them. Their sprites are drawn centred on one cell (`tile/mod.rs:642`).
- `TileFlags` is a `u16` with all 16 bits used; `UnderConstruction`, `ConstructionStage0/1` and `Renovating` took the last four (`tile/mod.rs:125`). Saves write flags by name (`"flags": "BuildingRoadLink"`), so widening it to `u32` doesn't change the format.
- The renderer draws `scaffold_NxN` or `renovation_NxN` from the objects tile set's `overlays` category for a flagged building (`tile/rendering.rs`, `TileMapRenderer::draw_tile`).

**Wells:** a house has water when a small or large well is within reach, by a path search (`SimContext::is_near_building`, `sim/context.rs:520`). It's a search per call, so it's done on a timer, not every frame.

**Feedback:**
- Toasts: finance notices go to a queue that `GameLoop::notify_finances` (`game_loop.rs:525`) shows with `UiSystem::show_toast`.
- Minimap alerts: the minimap keeps alerts by cell, which its alerts button cycles through (`tile/minimap/mod.rs`, `next_alert`, `resolve_alerts`).
- Minimap lenses: `MinimapLens::Risk` exists, with its button sprites for all three realms (`assets/ui/buttons/minimap*/lens_risk_*.png`), but it has no provider and isn't in `MinimapLens::available()` (`tile/minimap/lens.rs:68`).
- The retail tile inspector (`menu/in_game/inspector/`) shows a building as a title, subheadings and body lines of text.

**Walkers:** units are keyed by `UnitConfigKey` (`unit/config.rs:16`), a hash of the unit tile def's name. There's no fire watcher or builder walker, and no walker art beyond the hand-made set.

**Art:** the Blender pipeline has recipes for the roof-tile kiln and the overlays (`crates/tools/blender-tile-generator/hbgen/recipes/`). The generic recipes of placeholder-art Phase 3 aren't written yet, so the fire watch and the builders' guild each need one.

**Cheats:** `instant_construction` and `ignore_worker_requirements` exist (`cheats.rs:78`); the test harness turns on `instant_construction`.

## Design

### 1. Risk state (Phase 0)

- **`BuildingRisks`**, a new saved field on `Building`:
  - `fire` and `collapse`, each 0 to 100;
  - `rates`: this building's own rate for each, fixed when it's built (see the jitter below);
  - `near_water`: whether a well reaches it, refreshed on a timer;
  - `burning`: `None`, or the fire's state (Phase 1).
- **Rates per config.** Every building config gets an optional `risks` block, rates in points per calendar month:

  ```json
  "risks": { "fire": 8.0, "collapse": 3.0 }
  ```

  Left out, both are 0, so a building with no block never burns or collapses. Proposed starting values (balanced in M13):

  | Buildings | Fire | Collapse |
  |---|---|---|
  | House levels 0–1 (thatch) | 8 | 4 |
  | House levels 2–4 (tile roofs) | 5 | 3 |
  | Kilns, brick maker, distillery | 12 | 3 |
  | Other producers and storage | 4 | 3 |
  | Services | 3 | 2 |
  | Wells, gardens | 0 | 1 |

  At these rates a thatch house that no fire watcher visits catches fire after about 12 months, which is 12 minutes at 1×.
- **Jitter.** A building's own rates are its config's times a factor from the simulation's RNG (0.75 to 1.25), drawn when it's built. Without it, a row of identical houses built together would all catch fire in the same frame. The factor is saved, so a save keeps it.
- **Build-up.** `Building::update` adds `rate × delta / seconds_per_month` to each risk, so risks build up smoothly at a per-month rate (decision Q1). Sites, and buildings that are burning, don't build up risk.
- **Water.** A building that a small or large well reaches (`is_near_building`, from its road link, the well's `effect_radius`) builds up fire risk at `SimConfigs::risks.near_water_fire_scale` (0.5) of its rate. It's checked every `water_check_secs` (10 s), not every frame.
- **Visits.** `Building::visited_by` checks the walker before the archetype does: a fire watcher sets the fire risk to 0, a builder sets the collapse risk to 0, and both are accepted. Other walkers go to the archetype as now.
- **Settings:** `SimConfigs::risks` holds `enabled`, `near_water_fire_scale`, `rate_jitter`, `water_check_secs` and the Phase 1 fire settings. A mission's `risks` block in `campaigns.json` can turn risks off (decision Q6).
- **Cheat:** `no_city_risks` stops all build-up and events. The test harness turns it on, as it does `instant_construction`, and `tests/risks.rs` turns it off.

### 2. Fire and collapse (Phase 1)

- **Ignition.** A building whose fire risk reaches 100 catches fire. `burning` becomes `Some { secs_left }`, starting at `SimConfigs::risks.burn_secs` (30). A burning building:
  - isn't operational: it produces, serves and stores nothing, and refuses visitors other than fire watchers;
  - keeps its residents and workers until it burns down, so the stats don't jump twice;
  - adds `spread_per_sec` (5) fire risk per second to every building next to its footprint, which brings their own fires closer.
- **Putting a fire out.** A fire watcher who passes a burning building puts it out: `burning` goes back to `None` and its fire risk to 0. The building isn't damaged (decision Q9). In Phase 2 a fire watch also sends its walker straight to a fire within its `effect_radius` (decision Q3).
- **Burning down.** When `secs_left` reaches 0, the building is destroyed and leaves rubble.
- **Collapse.** A building whose collapse risk reaches 100 collapses at once and leaves rubble.
- **Destruction.** Destroying a building goes through `SimCmds`: `despawn_building_with_id`, then a rubble tile per cell of its footprint. Its residents and workers go as when the player demolishes it. Nothing is refunded. A building that's burning or collapsing is marked as destroyed when the event fires, so it isn't destroyed twice and no new patrol starts from it.
- **Visits to a building that's gone.** `SimCmd::VisitBuilding` and the other deferred building commands skip a building that no longer exists and log it at verbose level, instead of panicking. A walker can queue a visit in the same step that a fire destroys the building.
- **Rubble** (decision Q4) is a 1×1 `TileKind::Rocks` tile per cell, in a new objects tile set category, `rubble`, with a few variations. Like a boulder it blocks building and walking, the clear tool removes it for free, and the minimap shows it as rock cover. A rubble tile has no game object, so it costs nothing to keep.
- **Drawing a fire.** A new `TileFlags::Burning` (after widening `TileFlags` to `u32`) marks a burning building's tile. `TileMapRenderer::draw_tile` draws it darkened, with an animated `fire_1x1` overlay on each cell of its footprint. Smoke and sound are left for later.
- **Notices.** A fire, a fire put out, a building burnt down and a collapse are logged on a new `risks` log channel and become toasts, as finance notices do. A fire or a collapse also raises a minimap alert at the building, which is resolved when the fire is put out or the rubble is cleared.
- **Later milestones** call `Building::add_risk(RiskKind, amount)` and `Building::ignite()`: M7's floods add collapse risk to flooded buildings, M8's "fire" misfortune ignites one, and Phase 2's raiders set fires.

### 3. The fire watch and the builders' guild (Phase 2)

- **Kinds.** `BuildingKind::FireWatch` and `BuildingKind::BuildersGuild` (bits 22 and 23), added to `BuildingKind::services()`. They're `ServiceBuilding`s with patrol units, like the police station.
- **Walkers.** `UnitConfigKey::FireWatcher` and `UnitConfigKey::Builder`, with unit configs and tile defs. Until the Blender walker experiment (placeholder-art Phase 6), they reuse the guard's and the peasant's sprites (decision Q7).
- **Patrols that visit every building.** `ServiceConfig` gets `visits: "Houses" | "AllBuildings"` (default `Houses`, today's behaviour). The fire watch and the builders' guild visit all buildings, so their walkers reset the risks of workshops, storage and services as well as houses.
- **Dispatch to a fire.** When a building catches fire, the nearest fire watch within its `effect_radius` whose walker is at home sends it straight to the fire: it walks to the building's road link, puts the fire out on arrival, and goes home on a normal patrol. This is a new short task built from the existing follow-path and patrol tasks. If no walker is free, the fire burns until a patrolling one passes.
- **Configs:**

  | | Fire watch | Builders' guild |
  |---|---|---|
  | Footprint | 1×1 | 2×2 |
  | Workers | 2–4 | 2–6 |
  | `effect_radius` | 30 | 30 |
  | `patrol_frequency_secs` | 30 | 40 |
  | Cost and materials | 10 gold, 3 wood | 15 gold, 4 wood, 2 bricks |
  | Upkeep (gold a year) | 6 | 6 |
  | Construction tier | Basic | Civic |
  | Sector | Services | Services |

- **Art:** two new recipes and catalog entries in `buildings_daiLong.json`: the fire watch as a raised timber lookout with a bell and water jars, and the builders' guild as a workshop yard with stacked timber and scaffold poles. Both are rendered with `tilegen.sh`. They start placeholder-art Phase 3's primitives (`timber_stack`, `scaffold_poles`, `jars`) and the `workshop` recipe.

### 4. The Risk lens and the info panel (Phase 3)

- **Minimap Risk lens.** A `RiskLens` provider in `tile/minimap/lens.rs`, added to `MinimapLens::available()` after Water:
  - each building is coloured by its higher risk, from the style's `risk_low` to `risk_high`, and burning buildings get `risk_burning`;
  - the heat field is the risk at each building's cells, faded over a cell or two, with the style's `risk_ramp`;
  - one lens shows both risks (decision Q10).

  The colours go in `assets/configs/ui/minimap/dailong.json` under `lenses`, beside `water_ramp`. The button sprites already exist.
- **Info panel.** The retail tile inspector's building panel gets two body lines, "Fire risk: Low" and "Collapse risk: Medium" (Low under 40, Medium under 75, High from 75), and "On fire!" while it burns. They're text lines in the existing panel, with no new widgets.
- **Developer tools.** The developer Tile Inspector's Building panel shows the raw values, with buttons to ignite and to collapse the building. The World Debug window gets a Risks tab: counts, the riskiest buildings, and the `enabled` switch.

### 5. Automation, tests and scenario

- **Queries and commands:**
  - `risks.state`: whether risks are on, the counts of buildings burning and at high risk, the buildings by risk (top 20), and the fires burning;
  - `world.building_at` reports `risks` (`fire`, `collapse`, `near_water`, `burning`);
  - `risks.set cell=[x,y] fire=N collapse=N`;
  - `risks.ignite cell=[x,y]` and `risks.collapse cell=[x,y]`;
  - `world.stats` adds the counts.
- **Tests** in `crates/game/tests/risks.rs`:
  - build-up at the config rate, with the water scale and the jitter in range;
  - visits by a fire watcher and by a builder reset the right risk;
  - a building at 100 catches fire, burns for `burn_secs`, spreads to a neighbour, and leaves rubble on every cell of its footprint;
  - a fire watcher puts out a fire; a collapse leaves rubble at once;
  - a visit queued in the same step as the destruction doesn't panic;
  - the risks round-trip through a save;
  - the cheat and a mission's `risks` block stop everything.
- **Scenario** `crates/game/tests/scenarios/risks.jsonl`:
  - a street of houses without a fire watch, fast-forwarded until one burns down; rubble where it stood, cleared with `world.clear`;
  - the same street with a fire watch and a builders' guild, run as long without a fire or a collapse;
  - `risks.ignite` beside the fire watch: its walker puts it out;
  - the Risk lens, through `minimap.set lens=risk` and `minimap.state`.

  It runs in CI with the other scenarios.
- **Existing scenarios:** long runs (`finance.jsonl`, `goods.jsonl`, `construction.jsonl`, `determinism.jsonl`) set `cheats.set name=no_city_risks value=true` so a fire can't change what they check. `determinism.jsonl` keeps checking that two runs agree, risks included.

## UI changes

The user is redesigning the UI on another branch, so this is every UI change this milestone makes. None adds a dialog, a widget type or a style file.

| Where | Change | Phase | Files |
|---|---|---|---|
| Build menu, Services | Two new buildings, the fire watch and the builders' guild. Data only: tile defs and configs. | 2 | `assets/tiles/objects/tile_set.json`, `assets/configs/buildings/configs.json` |
| Toasts | Fire, fire out, burnt down and collapse notices, through the existing toast call. | 1 | `game_loop.rs` (a `notify_risks` beside `notify_finances`) |
| Minimap alerts | A fire or collapse raises an alert at the building, through the existing alerts. | 1 | `tile/minimap/mod.rs` (call sites only) |
| Minimap lens bar | A fourth lens button, Risks, after Water. Its sprites exist. | 3 | `tile/minimap/lens.rs`, `assets/configs/ui/minimap/dailong.json` (`lenses`: `risk_ramp`, `risk_low`, `risk_high`, `risk_burning`) |
| Tile inspector, building panel | Two or three more body text lines: fire risk, collapse risk, on fire. | 3 | `menu/in_game/inspector/mod.rs` |
| Developer UI only | Tile Inspector risk section, World Debug Risks tab. Not retail. | 3 | `debug/` |
| Map drawing | Burning buildings darkened with a fire overlay; rubble tiles. Not UI, but visible. | 1 | `tile/rendering.rs`, tile sets |

## Phases

### Phase 0: Risk state and the save format (medium)

- [ ] `BuildingRisks` on `Building`, with `rates`, `near_water` and `burning` (always `None` until Phase 1); `RiskKind` (`Fire`, `Collapse`).
- [ ] An optional `risks` block in every building config type (house levels, services, producers, storage), reached through the `BuildingConfig` trait; the starting values in `configs.json`.
- [ ] Rates with jitter from the simulation's RNG when a building is built or finishes construction.
- [ ] Build-up in `Building::update`; the well check on its timer.
- [ ] Reset by visits in `Building::visited_by`, keyed on the walker's config (`FireWatcher`, `Builder`), ready for Phase 2's walkers.
- [ ] `SimConfigs::risks`; a mission's `risks` block (`MissionDef::risks`, applied in `campaign::active_mission_*` as the calendar and finance are); the `no_city_risks` cheat, on in the test harness.
- [ ] Save version 7: `CURRENT_SAVE_VERSION`, `v6_to_v7.py` (adds a `risks` block with zero risks to every building and draws no jitter: old buildings get factor 1), migrate the local saves, run `--smoke-test-saves`, regenerate `saves/sample_saves.zip`.
- [ ] `risks.state`, `risks.set`, `world.building_at`'s `risks`.
- [ ] `tests/risks.rs`, build-up, water, visits and save round-trip cases; a `[[test]]` entry in `crates/game/Cargo.toml`.
- [ ] `no_city_risks` in the long scenarios.

### Phase 1: Fire, collapse and rubble (medium)

- [ ] `TileFlags` widened to `u32`; `TileFlags::Burning`.
- [ ] Ignition, burning (not operational, spreading), burning down, collapse; `Building::add_risk` and `Building::ignite`.
- [ ] Destruction through `SimCmds`, with rubble on each cell; the destroyed mark against double destruction.
- [ ] Deferred building commands skip a building that's gone.
- [ ] The `rubble` category and its tile def; the burnt and collapsed rubble variations.
- [ ] Art: `rubble` (1×1, 4 variations) and `fire_1x1` (an animated overlay, 4 frames) in the Blender catalog, rendered with `tilegen.sh`.
- [ ] Drawing: the darkened building and its fire overlay in `TileMapRenderer::draw_tile`.
- [ ] Notices on the `risks` channel, toasts (`GameLoop::notify_risks`) and minimap alerts.
- [ ] `risks.ignite`, `risks.collapse`.
- [ ] Tests: ignition, spread, burning down, collapse, rubble, visits to a destroyed building.

### Phase 2: The fire watch and the builders' guild (medium)

- [ ] `BuildingKind::FireWatch`, `BuildingKind::BuildersGuild`; their service configs and tile defs (cost, materials, upkeep, tier, sector).
- [ ] `UnitConfigKey::FireWatcher`, `UnitConfigKey::Builder`, their unit configs and tile defs on the guard's and the peasant's sprites.
- [ ] `ServiceConfig::visits`; the patrol's buildings to visit follow it.
- [ ] A fire watcher puts out a fire it passes; a fire watch sends its free walker to a fire in its radius.
- [ ] Art: the `fire_watch` and `builders_guild` recipes and catalog entries, rendered with `tilegen.sh`; the primitives they need ticked in placeholder-art Phase 3.
- [ ] Tests: patrols visit all buildings, putting out a fire, dispatch.
- [ ] `risks.jsonl`, in CI.

### Phase 3: The Risk lens, the info panel and documentation (small)

- [ ] `RiskLens` and its style colours; `MinimapLens::available()`; tick the Risk provider in [minimap.md](minimap.md).
- [ ] The inspector's risk lines.
- [ ] Developer tools: the Tile Inspector section and the World Debug Risks tab.
- [ ] `risks.jsonl` covers the lens; `ui.layout_issues` stays 0 on the inspector with the new lines.
- [ ] AGENTS.md (a City risks entry under Runtime architecture, `risks.jsonl` in the scenario list, `risks` in the test list), the roadmap's M6 row and save-version log, known-issues for what's left.

## Risks

- **Fires breaking existing tests and scenarios.** Any long run could now lose a building. The cheat is on in the test harness, and the long scenarios turn it on. A new scenario that wants risks turns it off.
- **Destroying buildings mid-step.** Nothing has done that before. Visits, deliveries and runners can target a building that's gone in the same queue. Making the deferred building commands skip missing buildings, plus tests that queue a visit in the same step, covers the known paths; a walker whose home burns down must despawn as when the player demolishes it.
- **Fire storms.** Spread plus jitter could burn down a whole district before the player can react. `spread_per_sec`, `burn_secs` and the dispatch are the knobs, and the scenario checks a fire watch keeps a street safe. Balance is M13's.
- **The well check's cost.** A path search per building every 10 s is about 100 searches a second on a 1,000-building city, each within a well's radius. If it shows in a profile, cache coverage per cell from the wells instead, as the Water lens's heat field already does.
- **`TileFlags` widening.** Tiles get 2 bytes larger; the flags are saved by name, so the format doesn't change.

## Open decisions

Each has a default; proceed with it unless the user decides otherwise.

- **Q1. How risk builds up.** *Default: continuously, at a rate per calendar month.* The roadmap says "goes up each month". A step at each month's end would set off every due fire at the same moment, and a patrol's reset would be worth more or less depending on when in the month it came.
- **Q2. Age.** *Default: no age.* The roadmap says risk comes "from its config and age". The risk built up since the last visit plays the part of age: an unvisited building only gets riskier. A building's age can raise its rates later, if balance needs it.
- **Q3. Fighting fires.** *Default: a fire watcher puts out a fire it passes, and a fire watch sends a free walker to a fire in its radius (Phase 2).* Patrols alone leave a fire to chance.
- **Q4. Rubble.** *Default: one 1×1 rubble tile per cell, free to clear.* One sprite per footprint size (1×1 to 5×5) looks better, but needs a multi-cell object tile with no game object, which the tile map doesn't have, and five times the art.
- **Q5. Construction sites.** *Default: no risk.* The GDD says fire can damage a site; that comes with raiders in Phase 2.
- **Q6. Missions without risks.** *Default: risks are on outside the campaign, and a mission's `risks` block can turn them off.* Missions I-1 and I-2 come before the fire watch exists, so M13 turns risks off for them.
- **Q7. Walker art.** *Default: the fire watcher uses the guard's sprites and the builder the peasant's.* New walkers wait for the Blender walker experiment.
- **Q8. A burning building's people.** *Default: it stops working but keeps its residents and workers until it burns down.*
- **Q9. Damage from a fire put out.** *Default: none.* The building is as it was, with its fire risk at 0.
- **Q10. One Risk lens or two.** *Default: one lens, coloured by the higher risk.* The minimap plan's `Risk(RiskKind)` would need a toggle in the lens bar, which is more UI while the redesign is under way.
