# City risks: fire and collapse

**Status (2026-11-09): The art, Phase 0 and Phase 1 are done; Phase 2 next.** This plan is milestone M6 of the [roadmap](roadmap.md). It covers the fire and collapse rows of GDD 5.9: each building's fire and collapse risk builds up over time, a patrol walker from the fire watch or the builders' guild resets it, and a building whose risk runs out catches fire or collapses and leaves rubble. Mission I-3 teaches "the fire watch and the builders' guild (fire and collapse risk)" ([campaigns.md](../design/campaigns.md#3-the-village-of-spirits)). Disease and crime come in Phase 2 of the roadmap. It was researched against `063d306` on `claude`. File and line references point at that commit, so re-check them before relying on them.

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

**Art:** the Blender pipeline has recipes for the roof-tile kiln and the overlays (`crates/tools/blender-tile-generator/hbgen/recipes/`). The generic recipes of placeholder-art Phase 3 aren't written yet, so the fire watch and the builders' guild each need one. (Done ahead of Phase 0 on 2026-11-09; see the [art notes](#art-notes-2026-11-09).)

**Cheats:** `instant_construction` and `ignore_worker_requirements` exist (`cheats.rs:78`); the test harness turns on `instant_construction`.

## Design

### 1. Risk state (Phase 0)

- **`BuildingRisks`**, a new saved field on `Building`:
  - `fire` and `collapse`, each 0 to 100;
  - `near_water`: whether a well reaches it, refreshed on a timer (saved with the timer);
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
- **Jitter.** A building's rates are its config's times a factor from 0.75 to 1.25. Without it, a row of identical houses built together would all catch fire in the same frame. The factor is a hash of the building's base cell and kind, not a draw from the simulation's RNG, so adding risks didn't shift every random sequence after it. It isn't saved: it's recomputed on spawn and load. The rates are read from the config on every step, so a house's rates follow its level and a config change reaches old saves.
- **Build-up.** `Building::update` adds `rate × delta / seconds_per_month` to each risk, so risks build up smoothly at a per-month rate (decision Q1). Sites, and buildings that are burning, don't build up risk.
- **Water.** A building that a small or large well reaches (`is_near_building`, from its road link, the well's `effect_radius`) builds up fire risk at `SimConfigs::risks.near_water_fire_scale` (0.5) of its rate. It's checked every `water_check_secs` (10 s), not every frame.
- **Visits.** `Building::visited_by` checks the walker before the archetype does: a fire watcher sets the fire risk to 0, a builder sets the collapse risk to 0, and both are accepted. Other walkers go to the archetype as now. Walkers are told apart by the building their patrol came from (`risks::risk_reset_by`), as tax collectors are, not by their unit config: unit configs are keyed by their tile def's name, so a builder on the peasant's sprites would be a peasant.
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
- **Goods are lost.** Whatever a building holds is lost with it: a storage yard's or granary's slots, a producer's stock, a market's or a house's goods. The counts stay in sync:
  - The world stats are recounted only on the next step, so destruction subtracts the building's goods from them at once, as `CityStock` does when it takes or deposits materials (`world/city_stock.rs`, `subtract_from_stats`). The HUD, `world.stats` and the construction checks never count goods that are gone.
  - `CityStock` reads the yards live, so a burnt yard's materials stop counting for placement at once.
  - No gold moves, so the finance accounts get no entry: the ledger records gold only (M4).
  - Walkers that already left with goods keep them. A runner on its way to fetch from a building that's gone comes back empty, as when the player demolishes it. A delivery on its way to one goes to another building with room, or back to its producer, through the fetch and delivery tasks' recovery paths.
  - The burnt-down notice lists what was lost: "The storage yard burnt down. Lost: 40 rice, 12 wood."
- **Visits to a building that's gone.** `SimCmd::VisitBuilding` and the other deferred building commands skip a building that no longer exists and log it at verbose level, instead of panicking. A walker can queue a visit in the same step that a fire destroys the building.
- **Rubble** (decision Q4) is a 1×1 tile per cell, the `rubble` tile def in the objects tile set's `rocks` category, so it's a `TileKind::Rocks` tile. A new category would need its own `TileKind` mapping, and an unknown category panics at load (`TileKind::specialized_kind_for_category`). Like a boulder it blocks building and walking, the clear tool removes it for free, and the minimap shows it as rock cover. It has no game object, so it costs nothing to keep.
- **Rubble piles from pieces**, as water shores and road junctions are built:
  - **Variation by neighbours.** A rubble tile's variation is the set of its four sides that border more rubble, as four digits in the order north, east, south, west, where 1 means rubble. This is the same mask `road::update_junctions` uses and the same naming as the water tile's shores ([terrain-tiles.md](../../.claude/rules/terrain-tiles.md)).
  - **What a building leaves.** A burnt-down 3×3 building leaves:
    - four corners (`1100`, `0110`, `0011`, `1001`);
    - four sides (`1110`, `0111`, `1011`, `1101`);
    - a middle (`1111`).

    A 1×1 building leaves a single heap (`0000`). A 2×2 leaves the four corners, and 4×4 and 5×5 buildings add more sides and middles.
  - **Clearing part of a pile.** That leaves the other six: four ends (`1000`, ...) and two straight strips (`1010`, `0101`). All 16 are rendered so any pattern has a piece.
  - **Merging piles.** Rubble from neighbouring buildings joins into one pile.
  - **When variations are picked.** The `Spawner` and `SimCmds` tile spawns don't update neighbour variations (terrain-tiles.md). Destruction therefore picks the variations itself, once the whole footprint has its rubble: over the footprint and the ring of cells around it. `TilePlacement::clear` updates the four neighbours of a cleared rubble tile, as it does for roads.
  - **Seamless edges.** The pieces must meet without seams, so rubble sits on its cell's diamond like a 1×1 building. The rocks anchor puts a sprite 8 logical px higher (`tile/mod.rs:642`), so every rubble variation carries `iso_offset {x: 0, y: 8}`. No renderer change is needed.
  - **Saves.** Saves store the variation as an index, so the list is append-only, as the water tile's is.
- **Drawing a fire.** A new `TileFlags::Burning` (after widening `TileFlags` to `u32`) marks a burning building's tile. `TileMapRenderer::draw_tile` draws it darkened, with an animated `fire_1x1` overlay on each cell of its footprint. Smoke and sound are left for later.
- **Notices.** A fire, a fire put out, a building burnt down and a collapse are logged on a new `risks` log channel and become toasts, as finance notices do. A fire or a collapse also raises a minimap alert at the building, which is resolved when the fire is put out or the rubble is cleared.
- **Later milestones** call `Building::add_risk(RiskKind, amount)` and `Building::ignite()`: M7's floods add collapse risk to flooded buildings, M8's "fire" misfortune ignites one, and Phase 2's raiders set fires.

### 3. The fire watch and the builders' guild (Phase 2)

- **Kinds.** `BuildingKind::FireWatch` and `BuildingKind::BuildersGuild` (bits 22 and 23), added to `BuildingKind::services()`. They're `ServiceBuilding`s with patrol units, like the police station.
- **Walkers** (decision Q7):
  - **The builder** is a peasant: the builders' guild's `patrol_unit` is `Peasant`, so it needs no unit config or art of its own. Its patrol's origin makes it a builder (§1, Visits).
  - **The fire watcher** is the guard recoloured: the same sprites with a red jacket and cap, so it stands out from guards. A script makes them, `crates/tools/unit-recolor/recolor.py` (Pillow, as `ui-icons` uses), from a JSON recipe:
    - the source unit;
    - the new unit's name;
    - a map from each flat source colour to its new one;
    - the colours to keep.

    It writes the frames to `assets/tiles/objects/units/fire_watcher/`, with a `fire_watcher` tile def copied from the guard's, and records them in a manifest, so it never overwrites hand-made art. The fire watcher gets `UnitConfigKey::FireWatcher` and a unit config on that tile def.
  - **How the recolour works.** The guard is drawn in flat colours with a dark outline (`#1a0b03`). A pixel is matched as a blend of the outline and one jacket colour, and gets the same blend of that colour's red:
    - `#765729` becomes `#9a221a`;
    - `#8e7644` becomes `#c43a26`;
    - `#887951` becomes `#d06048`.

    The spear shaft (`#503b1a`) and the leg wraps (`#40381f`, `#393016`, `#2a2313`) lie between the same colours, so they're listed to keep. A prototype on three frames (2026-11-09) gave a red jacket and cap with the spear and legs unchanged; the leg laces pick up a slight tint, which the keep list can fix.
  - Both walkers wait on the Blender walker experiment (placeholder-art Phase 6) for art of their own.
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
- **Existing scenarios:** the long runs, `finance.jsonl` (about 8 months) and `goods.jsonl` (5), set `cheats.set name=no_city_risks value=true` so a fire can't change what they check. The others are too short for any risk to reach 100. `determinism.jsonl` keeps risks on, so two runs agreeing covers them too; its 5 months can't take a building to 100.

### Art notes (2026-11-09)

The art was made first, ahead of Phase 0, with the Blender pipeline:
- **Recipes:** `hbgen/recipes/risks.py` (`rubble`, `fire`) and `hbgen/recipes/services.py` (`fire_watch`, `builders_guild`).
- **Catalogs:**
  - `catalog/risks.json` holds `rubble` (16 variations, in the `rocks` category) and `fire_1x1` (4 frames, in `overlays`);
  - `buildings_daiLong.json` holds the two buildings.
- **Rubble:**
  - Each piece is a height field over its cell, sloping to the ground at an open side and ending raggedly a little inside the cell. Charred beams, bricks, tile shards, stones and now and then a stub of wall are kept clear of the cell's edges.
  - Laid out as the engine will (`scr = 32·(x+y), 16·(x−y)`, back to front), 3×3 and 2×2 piles show no seams. Clearing a cell out of a pile leaves small V-notches at the diagonal corners, since the mask looks only at the four sides; fine for placeholder art.
  - In the game, `world.place tile=rubble` puts the `0000` piece centred on its cell next to a hand-made well.
- **Core change:** `setup_outlines` takes `border`, and `generate_engine` reads `B.outline_border`. The cut edges where pieces join must get no ink. Changing those core modules re-rendered every generated tile, and the kiln's and the overlays' pixels came out the same (at most one level off), so their committed files were kept.
- **Fire:** the flames are seven tongues with yellow cores and rising embers, 4 frames of a loop; their heights and sway follow the frame's phase. Over a darkened 2×2 building, one per cell, they read clearly as a fire.
- **Buildings:**
  - The fire watch is a timber lookout with a hipped roof, a bell, a red pennant, a ladder and water jars. It matches the hand-made 1×1 shrine's scale.
  - The builders' guild is a plastered hall, a timber shed, brick pallets, a log pile, a sawhorse and a section of bamboo scaffolding. It sits well beside the generated roof-tile kiln.
  - Placing either fails cleanly until Phase 2 adds its building config ("Can't find Building config").

## UI changes

The user is redesigning the UI on another branch, so this is every UI change this milestone makes. None adds a dialog, a widget type or a style file. The user confirmed on 2026-11-09 that only the tile inspector is likely to conflict with the redesign.

| Where | Change | Phase | Files |
|---|---|---|---|
| Build menu, Services | Two new buildings, the fire watch and the builders' guild. Data only: tile defs and configs. | 2 | `assets/tiles/objects/tile_set.json`, `assets/configs/buildings/configs.json` |
| Toasts | Fire, fire out, burnt down and collapse notices, through the existing toast call. | 1 | `game_loop.rs` (a `notify_risks` beside `notify_finances`) |
| Minimap alerts | A fire or collapse raises an alert at the building, through the existing alerts. | 1 | Call sites only, in `building/` and `menu/mod.rs`; `tile/minimap/` is unchanged |
| Minimap lens bar | A fourth lens button, Risks, after Water. Its sprites exist. | 3 | `tile/minimap/lens.rs`, `assets/configs/ui/minimap/dailong.json` (`lenses`: `risk_ramp`, `risk_low`, `risk_high`, `risk_burning`) |
| Tile inspector, building panel | Two or three more body text lines: fire risk, collapse risk, on fire. The redesign is likely to change this panel, so expect to move these lines when the branches merge. | 3 | `menu/in_game/inspector/mod.rs` |
| Developer UI only | Tile Inspector risk section, World Debug Risks tab. Not retail. | 3 | `debug/` |
| Map drawing | Burning buildings darkened with a fire overlay; rubble tiles. Not UI, but visible. | 1 | `tile/rendering.rs`, tile sets |

## Phases

### Phase 0: Risk state and the save format (medium)

- [x] `BuildingRisks` on `Building` (`building/risks.rs`), with `near_water`, its timer and `burning` (always `None` until Phase 1); `RiskKind` (`Fire`, `Collapse`); `RiskRates`.
- [x] An optional `risks` block in every building config type (house levels, services, producers, storage), reached through `BuildingConfig::risk_rates`; the starting values in `configs.json`.
- [x] Each building's jitter, from its cell and kind.
- [x] Build-up in `Building::update`; the well check on its timer.
- [x] Reset by visits in `Building::visited_by`, by the patrol's origin (`BuildingKind::FireWatch`, `BuildingKind::BuildersGuild`, added now), ready for Phase 2's walkers.
- [x] `SimConfigs::risks`; `sim::risks::CityRisks` (saved, read through `SimContext::risks()`); a mission's `risks` block (`MissionDef::risks`, `campaign::active_mission_risks`, applied in `GameSession::start_mission`); the `no_city_risks` cheat, on in the test harness.
- [x] Save version 7: `CURRENT_SAVE_VERSION`, `v6_to_v7.py`, the local saves migrated, `--smoke-test-saves` passed, `saves/sample_saves.zip` regenerated.
- [x] `risks.state`, `risks.set`, `world.building_at`'s `risks`.
- [x] `tests/risks.rs`: build-up, the top, jitter, water, house levels, sites, the switch and the cheat, the mission block, visits and the save round trip; a `[[test]]` entry in `crates/game/Cargo.toml`.
- [x] `no_city_risks` in the long scenarios.
- [x] `risks.jsonl`, started here and in CI: build-up, the well, `risks.set` and the switch.

Notes (2026-11-09):
- **Jitter from a hash, not the RNG.** The factor comes from the building's cell and kind and is recomputed on spawn and load; nothing about rates is saved (§1). Drawing from the simulation's RNG would have shifted every random sequence after it, and saving it would have kept stale rates after a config change.
- **Saved state:** a building's `risks` is `fire`, `collapse`, `near_water`, `water_check_timer` and `burning`. `Simulation::risks` is `{ "enabled": bool }`. `v6_to_v7.py` writes zero risks, no water (the first check, 10 s after the load, finds it) and `enabled: true`.
- **Walkers by origin.** Visits key on the patrol's origin building, not the unit config (§1, Visits), so the two building kinds came in now and the builder needs no unit config.
- **`world.stats`** doesn't count risks; `risks.state` reports the counts.
- **The new well check** is a path search per building every 10 s, only for buildings with a fire rate. It hasn't been profiled yet; see [Risks](#risks).
- **Verified:**
  - `cargo test --workspace`;
  - all CI scenarios, `risks.jsonl` included;
  - `determinism.jsonl` twice: the same checksum, `a4ec9f1fd0104fa6` (`--seed 7 --window-size 1280x720`), changed from `3801c7bcb809de76` by the new save fields;
  - record and replay of `ui_build` and `determinism`;
  - the save smoke test on all 13 local saves;
  - the web check.

### Phase 1: Fire, collapse and rubble (medium)

- [x] `TileFlags` widened to `u32`; `TileFlags::Burning`.
- [x] Ignition, burning (not operational, spreading), burning down, collapse; `Building::add_risk` and `Building::ignite`.
- [x] Destruction through `SimCmds`, with rubble on each cell; the destroyed mark against double destruction.
- [x] Lost goods subtracted from the world stats at once; the burnt-down notice lists them.
- [x] Deferred building commands skip a building that's gone.
- [x] The `rubble` tile def (`rocks` category) with the 16 neighbour-mask variations and their `iso_offset`. Done with the art.
- [x] `rubble::update_variations` (after destruction, over the footprint and its ring) and its call from `TilePlacement::clear`.
- [x] Art, in the Blender catalog, rendered with `tilegen.sh`:
  - `rubble`: 16 pieces from one recipe. Each is a cut of the same pile surface: full height at a side that joins more rubble, sloping to the ground at an open side, so the pieces meet without seams. Debris (beams, bricks, tiles) is seeded per piece and kept inside the cell.
  - `fire_1x1`: an animated overlay, 4 frames.
- [x] Drawing: the darkened building and its fire overlay in `TileMapRenderer::draw_tile`.
- [x] Notices on the `risks` channel, toasts (`GameLoop::notify_risks`) and minimap alerts.
- [x] `risks.ignite`, `risks.collapse`.
- [x] Tests:
  - ignition, spread, burning down and collapse;
  - rubble variations for 1×1 to 5×5 footprints, after clearing part of a pile, and for two piles side by side;
  - a visit to a destroyed building;
  - a storage yard full of goods burns down: `world.stats`, `CityStock` and the finance accounts agree with the yards that are left, at once and after the next step, and a runner heading there comes back empty.

Notes (2026-11-09):
- **Code:** `building/destruction.rs` (`destroy_building`, `Destruction`), `tile/rubble.rs` (`place_pile`, `update_variations`), and the fire in `Building::update`, `update_fire`, `ignite`, `collapse`, `add_risk` and `visited_by` (`building/mod.rs`).
- **Destruction runs last in its step.** `SimCmd::DestroyBuilding` is a delayed command, like `UpgradeHouse`, so the visits and task steps the same step queued for the building run first. The next step finds rubble, as after a demolition between steps, which every walker task already handles. Deferred building commands that name a building that's gone are skipped and logged at verbose level as well (§2), for commands from earlier steps.
- **The world stats are recounted in full** after a destruction (`World::refresh_stats`), not only the goods subtracted: the building's residents and workers stop counting at once too. A destruction is rare, so the cost doesn't matter.
- **A burning building** skips its archetype's update and its workers' search, takes and hands out no goods (`available_resources`, `receivable_resources` and the like return 0), and isn't operational. The city stock skips a burning storage yard, so construction can't take its materials, and a house can't merge with or expand into a burning neighbour. The stats keep counting its people and goods until it burns down (Q8).
- **A fire watcher puts out a fire it visits** already (`Building::visited_by`), ahead of Phase 2: a burning building turns every other walker away. Phase 2 still has the patrols that visit every building and the dispatch to a fire.
- **A fire put out in time** cancels its burning down: the queued destruction checks the building still burns. The building is unharmed, with no fire risk (Q9).
- **While risks are off** (the switch or the cheat), a fire neither spreads nor burns down; the building stays burning.
- **What can burn or collapse:** only a finished building with that rate in its config (`can_catch_fire`, `can_collapse`). A well has no fire rate, so a fire next to it doesn't spread to it; `risks.ignite` refuses it.
- **Notices:** "Fire! The storage yard is burning.", "The fire at the storage yard is out.", "The storage yard burnt down. Lost: 20 wood, 12 bricks." and "The storage yard collapsed." Any house is "the house". A fire's minimap alert is resolved when it's put out or the building is demolished; a destruction raises one at the base cell, resolved when that cell's rubble is cleared, or when it ages out after 60 s.
- **The flames' frame time** is a renderer constant, 0.15 s (`FIRE_FRAME_SECS`), since the tile generator's catalog has no anim duration; a duration in the tile def would win. Each cell starts at its own frame, so a big fire doesn't flicker in step. `TileMap::overlay_anim_secs` is their clock: game time, so they stop while paused.
- **Undo:** a building put back by undo comes back without its `Burning` flag, with new risks. Undoing the placement of a building that has since burnt down finds rubble and does nothing.
- **A Phase 0 bug fixed:** a house that expands moves its base cell, but kept the jitter of its old one, while a load computes it from the new one, so a loaded game drifted from the one that saved it. The jitter now follows the base cell (`BuildingRisks::refresh_jitter`, also after a debug teleport). It changed `determinism.jsonl`'s checksum from `a4ec9f1fd0104fa6` to `b10f4eeae3845660`; with that line reverted, the checksum is the old one again.
- **`risks.state`** lists the fires burning (`fires`, with their `secs_left`).
- **Verified:**
  - `cargo test --workspace`, including the 21 tests in `tests/risks.rs`;
  - all 12 CI scenarios in release, `risks.jsonl` extended with a fire, the rubble and a collapse;
  - `determinism.jsonl` twice: the same output and save;
  - record and replay of `ui_build` and `determinism`;
  - the save smoke test on all 13 local saves (no format change: the flags are saved by name, and the new state is runtime only);
  - the web check;
  - screenshots of a burning storage yard and house, then their rubble and a collapsed market's, with the fire spread to both houses next door.

### Phase 2: The fire watch and the builders' guild (medium)

- [ ] Service configs for `BuildingKind::FireWatch` and `BuildingKind::BuildersGuild` (the kinds came with Phase 0), and `"sector": "Services"` in their catalog entries.
- [ ] `UnitConfigKey::FireWatcher` and its unit config on a new `fire_watcher` tile def. The builder is a `Peasant` (§3).
- [ ] `crates/tools/unit-recolor/` (`recolor.py`, its recipe and manifest, a README section or file); the red `fire_watcher` frames from the guard's; the web asset manifest refreshed. List the tool in AGENTS.md's workspace layout.
- [ ] `ServiceConfig::visits`; the patrol's buildings to visit follow it.
- [ ] A fire watcher puts out a fire it passes (its visit does since Phase 1; patrols reach every building with `visits` above); a fire watch sends its free walker to a fire in its radius.
- [x] Art: the `fire_watch` and `builders_guild` recipes and catalog entries, rendered with `tilegen.sh`. Their tile defs have no `sector` yet, so the build menu leaves them out until their configs land; add `"sector": "Services"` to their catalog entries then.
- [ ] Tests: patrols visit all buildings, putting out a fire, dispatch.
- [ ] `risks.jsonl`, in CI.

### Phase 3: The Risk lens, the info panel and documentation (small)

- [ ] `RiskLens` and its style colours; `MinimapLens::available()`; tick the Risk provider in [minimap.md](minimap.md).
- [ ] The inspector's risk lines.
- [ ] Developer tools: the Tile Inspector section and the World Debug Risks tab.
- [ ] `risks.jsonl` covers the lens; `ui.layout_issues` stays 0 on the inspector with the new lines.
- [x] `.claude/rules/terrain-tiles.md`: a Rubble section beside Roads (the neighbour mask, when variations are picked, the anchor). Written with Phase 1.
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
- **Q4. Rubble.** *Decided (2026-11-09): piles built from pieces, like the water tile's shores.* Each cell gets a 1×1 rubble piece picked from which of its sides border more rubble: corners, sides and a middle make up a larger pile, and a single heap stands alone (§2). Rubble is free to clear. One sprite per footprint size would need a multi-cell object tile with no game object, which the tile map doesn't have.
- **Q5. Construction sites.** *Default: no risk.* The GDD says fire can damage a site; that comes with raiders in Phase 2.
- **Q6. Missions without risks.** *Default: risks are on outside the campaign, and a mission's `risks` block can turn them off.* Missions I-1 and I-2 come before the fire watch exists, so M13 turns risks off for them.
- **Q7. Walker art.** *Decided (2026-11-09): the builder uses the peasant's sprites as they are, and the fire watcher the guard's, recoloured with a red jacket so it stands out from guards* (§3). Art of their own waits for the Blender walker experiment.
- **Q8. A burning building's people.** *Default: it stops working but keeps its residents and workers until it burns down.*
- **Q9. Damage from a fire put out.** *Default: none.* The building is as it was, with its fire risk at 0.
- **Q10. One Risk lens or two.** *Default: one lens, coloured by the higher risk.* The minimap plan's `Risk(RiskKind)` would need a toggle in the lens bar, which is more UI while the redesign is under way.
