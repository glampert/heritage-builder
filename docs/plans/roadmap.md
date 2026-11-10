# Technical roadmap

**Status (2026-11-09): Phase 1 in progress: M1, M3, M4 and M5 done; M6 under way.** This is the top-level implementation plan. It turns the design in [docs/design/](../design/README.md) into milestones, puts them in order, and links to a detailed plan for each one. It was researched against `d0696fe` on `main`.

## How to use this file

- **One plan per milestone.** Each milestone gets its own plan in `docs/plans/` when work on it starts. That plan holds the detail and the checkboxes. This file keeps the order, the dependencies and the status.
- Plans written so far:
  - [game-time.md](game-time.md) (M1);
  - [construction.md](construction.md) (M3);
  - [placeholder-art.md](placeholder-art.md) (M2);
  - [terrain-height.md](terrain-height.md) (T);
  - [unit-movement.md](unit-movement.md) (U);
  - [minimap.md](minimap.md) (MM);
  - [realm-ui.md](realm-ui.md) (UI);
  - [city-risks.md](city-risks.md) (M6).
- When a milestone starts, finishes or changes scope, update its row in the [Phase 1 table](#phase-1-campaign-i-missions-15-the-vertical-slice) and its entry in the [save-version log](#save-version-log).
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - keep desktop and web compiling;
  - run `cargo test --workspace` and the scenarios;
  - follow the Saves workflow for every change to the save format;
  - route edits through `TilePlacement`, `SimCmds` and the `TileMap` setters;
  - list each new plan in AGENTS.md's Plans section.

## Phases

These follow [overview.md](../design/overview.md#development-phases).

| Phase | Result | Plans |
|---|---|---|
| **0. Prototype** | Done: engine, renderers, egui UI, saves, housing, services, production, walkers, a campaign framework, automation | [agent-automation.md](agent-automation.md), [egui-migration.md](egui-migration.md) |
| **1. Core city sim** | Campaign I, missions 1–5 playable from start to finish with placeholder art, on desktop and web | This file (M1–M13, T) |
| **2. Advanced systems** | All of Campaign I: elite housing, monuments and guilds, defence and attacks, diplomacy, sea trade, story presentation in full | Outlined [below](#phase-2-outline) |
| **3. Kambara** | The realm layer as data, the Kambara art set, the water network (barays and canals), the lake's pulse, temple estates, rest houses. Campaign II | Later |
| **4. Tampadesa** | The merit economy, policies, two peoples, calling on allies. Campaign III and the epilogue | Later |
| **5. Polish and release** | Balance, difficulty, Custom Game, localisation, accessibility, performance, store pages | Later |

## Phase 1: Campaign I, missions 1–5 (the vertical slice)

**Goal:** a player can start the campaign, play missions 1–5 with their briefings, meet each mission's goals, and see the debrief. Every system those missions teach works (see [campaigns.md](../design/campaigns.md), mission I-1 to I-5, and the GDD).

**Done when:**
- missions 1–5 can be finished on desktop and on the web;
- every mission has a script scenario that plays it to victory (with time compressed by `time.fast_forward`), running in CI;
- all existing tests and scenarios pass.

| # | Milestone | Size | Depends on | Plan | Status |
|---|---|---|---|---|---|
| M1 | Game time and calendar | Small | — | [game-time.md](game-time.md) | Done (hand check on macOS pending) |
| M2 | Placeholder art pipeline | Medium | — (runs alongside) | [placeholder-art.md](placeholder-art.md) | In progress: the pipeline is done (Phases 0–2); recipes and the building set next |
| M3 | Building costs and construction | Medium to large | M1 | [construction.md](construction.md) | Done (fast build option waits for Custom Game) |
| M4 | Finance | Medium | M1 | [finance.md](finance.md) | Done (the tax level's and wage's effects on ratings, and the overdue debt's failure, come with M9) |
| M5 | Goods and production for missions I-1 to I-5 | Medium | M3 | [goods.md](goods.md) | Done (vegetables, house level 5 and month-based consumption rates deferred; see the plan) |
| M6 | City risks: fire and collapse | Medium | M3, M1 | [city-risks.md](city-risks.md) | In progress: art and Phases 0–2 (risk state, save v7; fire, collapse, rubble; fire watch, builders' guild) done; Phase 3 (Risk lens, info panel) next |
| M7 | Seasons, floods and irrigation | Large | M1, M5 | — | Not started |
| M8 | Faith and festivals | Medium | M1 | — | Not started |
| M9 | Ratings, goals and failure | Medium | M4, M7, M8 | — | Not started |
| M10 | Trade v1 and court requests | Large | M1, M4, M5 | — | Not started |
| M11 | Messages and advisors | Medium | M9 (grows with each system) | — | Not started |
| M12 | Story presentation v1 | Small to medium | — | — | Not started |
| M13 | Missions I-1 to I-5: maps, configs, balance, scenarios | Large | All of the above | — | Not started |
| T | Terrain height | Medium to large | M3 (spike after) | [terrain-height.md](terrain-height.md) | Not started |
| U | Eight-direction movement for off-road walkers | Medium | M3; art from M2 Phase 6 | [unit-movement.md](unit-movement.md) | Not started |
| MM | Minimap: painted relief, lenses and tools | Medium | — (lenses grow with M6, M7, M8) | [minimap.md](minimap.md) | Done (Risk, Faith, Flood and Fertility lenses come with M6, M8, M7) |
| UI | Realm UI: menus, world map, intermission, comics, HUD and city pages in each realm's style | Large | — (screens fill in with M4, M9, M10, M11, M12) | [realm-ui.md](realm-ui.md) | Phase 0 in progress |

Suggested order:
- **First:** M1 → M3 → M5, with M2 running alongside from the start.
- **Then, in parallel:** M4, M6 and M8.
- **Then:** M7, then M10, M9 and M11.
- **Last:** M13, with M12 slotted in anywhere.
- **Terrain height:** the T spike runs after M3, and Stages 1–4 come before M13 builds mission 5's map.
- **Minimap:** MM can run any time; its Risk, Faith, Flood and Fertility lenses come with M6, M8 and M7.
- **Realm UI:** UI's Phase 0 (skins, widgets, art pipeline) should land before M4, M9, M10, M11 and M12 build their screens, so they build them in the new style. Its later phases ship with today's data and leave hooks those milestones fill.
- **Eight-direction movement:** U runs after M3 and, ideally, before T's pathfinding stage (Stage 4), since both change the same step check. Its art waits on the Blender walker experiment (M2, placeholder-art Phase 6), with a fallback to existing directions until then.

### M1: Game time and calendar (small)

Plan: [game-time.md](game-time.md), which records the decisions (month length, where the settings come from, the event order).

**Problems today:**
- The only game-time unit is `SimConfigs::seconds_per_day = 600`, used only by house consumption.
- `Simulation::elapsed_secs` isn't saved.
- **Bug:** the world update's delta is multiplied by speed twice, so every building timer runs at speed squared (`sim/mod.rs:165–166`; `time_since_last_secs()` has already been scaled once when it's multiplied by `self.speed`). Every timer-driven system after this depends on the fix.

**Tasks:**
- Fix the speed-squared delta and add a regression test: building timers run at 1× and 2× speed for the same simulated span.
- Add `GameCalendar`, saved:
  - a game-time clock, with day, month and year, measured in game seconds;
  - the season, wet or dry, from a per-mission season table (start month, wet months);
  - the World Year plus the realm's era name, for display (see [world.md](../design/world.md#shared-timeline)).
- **Events:** `on_new_month`, `on_new_season` and `on_new_year`, broadcast to systems through the `GameSystem` trait.
- **HUD:** date and season in the top bar (`menu/in_game/bars.rs`).
- **Automation:** `sim.state` reports the date, and `wait.until` can wait for a date.
- Bump the save version: version 3, the calendar alone (M3 didn't land with it).

### M2: Placeholder art pipeline (medium, alongside)

See [placeholder-art.md](placeholder-art.md):
- move the Blender generator into the repo;
- fit its camera and anchor to the engine;
- add a catalog, a batch driver and `tile_set.json` syncing;
- write generic recipes and render the Phase 1 building set, scaffolds, rubble and the new terrain kinds.

Each later milestone adds its buildings to the catalog, when it needs them (see the conventions below).

### M3: Building costs and construction (medium to large)

See [construction.md](construction.md):
- material costs taken from a city-wide stock at placement;
- construction sites on a game-time timer, refunded in full when cancelled;
- house renovation and the downgrade grace period;
- scaffold rendering;
- save v3.

### M4: Finance (medium)

Plan: [finance.md](finance.md). It starts by putting all the city's gold in one signed treasury.

GDD 5.7: wages, tax rate, upkeep, yearly accounts and debt. Building on `GlobalTreasury` (`sim/resources.rs:739`), the tax collectors and the existing finances dialog (`menu/dialog/city_management/finances`):
- **Wages:** paid monthly per employed worker from the treasury, at a rate the player can set; wages affect Prosperity later.
- **Tax rate:** set by the player, scaling `HouseBuilding::generate_tax`.
- **Upkeep:** an optional monthly cost per building, from its config.
- **Yearly accounts:** income and spending by category, last year against this year, in the finances dialog.
- **Debt:** the treasury can go negative. Being in debt for a while sets a failure flag that M9 uses.
- **Tests:** a new `finance.rs` test.

### M5: Goods and production for missions I-1 to I-5 (medium)

Plan: [goods.md](goods.md). It starts by saving goods by name, so that adding a good later needs no save-version bump.

- **New `ResourceKind`s** that missions 1–5 use, such as roof tiles and vegetables (see GDD §8). `ResourceKind` is a `u32` bitflag with 10 bits used. Check how it serializes, because adding kinds may change the save format.
- **New producers:** a roof-tile kiln, plus any others the missions need. Config entries in `buildings/configs.json`, art from M2.
- **Rebalance storage capacity** (8 slots × 4 units today) against construction costs, and against the house goods needs for common levels 1–5.
- **Common house level 5**, if missions 1–5 need it.

### M6: City risks: fire and collapse (medium)

See [city-risks.md](city-risks.md). GDD 5.9:
- **Risk build-up:** fire and collapse risk per building, from its config and age; it goes up each month and is reset by a patrol walker's visit.
- **New service buildings:** the fire watch and the builders' guild. Both reuse the `ServiceBuilding` patrol machinery (`unit/task/patrol.rs`).
- **Events:**
  - a fire spreads to neighbours and leaves rubble;
  - a collapse leaves rubble;
  - rubble is a tile that can be cleared.
- **Overlay:** a risk overlay.
- **Tests:** tests and a scenario.

(Disease and crime come in Phase 2, with hospitals and unrest.)

### M7: Seasons, floods and irrigation (large)

GDD 5.10 and Đại Long's water layer (GDD §6):
- **Floodplain:** a floodplain terrain kind or flag and a per-cell fertility value, which floods raise.
- **Flood event:** once each wet season, the mission's flood profile (height and extent) floods unprotected floodplain cells. Buildings there are damaged or destroyed and fields are reset.
- **Dykes:** a linear building with junction variations, like roads (art from M2). Dykes block the flood. They can **breach** if under-maintained, which ties into M6's collapse.
- **Sluice gates:** let water into chosen paddies to irrigate them.
- **Farm yield** depends on fertility and irrigation; irrigation allows a second harvest.
- **Interaction with terrain height:** floods only reach cells below the flood level (terrain-height §9).
- **Goal type:** `SeasonsSurvived` (shared with M9).
- **Overlays:** flood risk and fertility.

### M8: Faith and festivals (medium)

GDD 5.11:
- **Patrons:** 3–5 per realm, defined as data (names fictionalised; see world.md). Shrines and temples are dedicated to a patron.
- **Favour** per patron rises with coverage and festivals, and fades with neglect.
- **Blessings and misfortunes** come from an event table, through the message system (M11).
- **Festivals:** a festival square building; hosting a festival costs gold and goods and raises Culture, favour and calm.
- **Đại Long:** the Mountain Lord and the Water Lord hook into the flood severity (M7).
- **UI:** the City book's Heritage page, Patrons and Festivals tabs (realm-ui.md Phase 6; mockup `city-heritage-<realm>.jpg`).

### M9: Ratings, goals and failure (medium)

GDD 5.17 and 7:
- **Ratings:** Prosperity, Culture, Security and Legitimacy (the Mandate), recomputed monthly, each with a breakdown of what drives it.
- **Goals:** extend `MissionGoal` (`campaign/config.rs:53`) with `Rating`, `HousesAtLevel`, `TradeIncome`, `Stockpile`, `RequestsFulfilled`, `SeasonsSurvived`, `SurviveUntil` and `SurviveAttacks`. The last two are for Phase 2.
- **Failure conditions:** debt (M4: `Finance::is_debt_overdue`, at `FinanceConfigs::debt_limit_months`), Legitimacy near zero, and time limits. Fair taxes and wages feed Legitimacy and Prosperity from the tax level and wage M4 records.
- **Optional distinctions** for the stele.
- **Goals panel** in the HUD, live; a ratings dialog.
- **Tests:** extend `tests/campaign.rs`.

### M10: Trade v1 and court requests (large)

GDD 5.12 and 5.14:
- **World map:** a mission-defined world map with partner cities and routes, set up in mission config; a world-map dialog.
- **Opening a route** costs gold.
- **Trade post:** buys and sells within limits set per good and per partner. A **caravan** walker comes from the map edge's entry point, goes to the trade post and trades.
- **Imports** fill storage yards through deliveries; this is how construction materials are imported (GDD 5.1.1).
- **Prices** come from a per-partner table with simple supply and demand.
- **Court requests:** "deliver N goods within M months"; goods go out through a dispatch building or the trade post. Fulfilling one raises Legitimacy, and failing one lowers it.
- **Scope:** Phase 1 needs only land routes (mission I-5). Docks and sea trade come in Phase 2 (I-6).

### M11: Messages and advisors (medium)

- **Message log:** a typed message system with priorities, a log panel, and a popup for important events (floods, fires, requests, blessings, goals).
- **Advisor screens:** labour, food, finance (M4) and ratings (M9), at a basic level.
- **Automation:** `messages.list` for scenarios.

### M12: Story presentation v1 (small to medium)

- **Briefing dialog:** a scroll with text, goals and an image, from mission config.
- **Debrief:** stele lines added, then the stele shown.
- **Comic viewer:** a sequence of full-screen images with caption and dialogue text, from data. It's used for the I-A and I-B sequences ([story-script.md](../design/story-script.md)).
- **Text** lives in the localized strings (`assets/configs/ui/text/<language>.json`) or a new story file, ready for localisation later.

### M13: Missions I-1 to I-5 (large, content)

- Build the five maps in the developer editor, as saves (`campaigns.json` `MissionMap::SaveGame`). Mission 5 uses terrain height if T has landed.
- **Starting conditions:** gold, a pre-placed storage yard with starting stock (bootstrap rule, GDD 5.1.1), the season table, the flood profile, world-map partners and the request schedule.
- **Unlocks per mission:** which buildings appear in the build menu (a new field in the mission config).
- **Balance** against the 1-hour target. House consumption rates are still per `seconds_per_day` (600 s), not per calendar month; move them to months here if retuning them (M5 left them as they were, [goods.md](goods.md)).
- **One scenario per mission** that plays it to victory with `time.fast_forward`, in CI.

### T: Terrain height (medium to large)

See [terrain-height.md](terrain-height.md):
- Stage 0, a go/no-go spike after M3;
- then data, rendering, picking, pathfinding and art.

### U: Eight-direction movement for off-road walkers (medium)

See [unit-movement.md](unit-movement.md):
- units with `diagonal_movement` (settler, dog, buffalo, bird) path through 8 neighbours, without cutting corners, at costs of 10 and 14;
- road walkers don't change;
- `UnitDirection` gets N, S, E and W, with a fallback to existing art;
- the new walk angles come from the Blender walker experiment, or by hand.

No save-version bump: the new enum variants go at the end, so old saves still load.

### MM: Minimap (medium)

See [minimap.md](minimap.md):
- a painted base baked on the CPU, with buildings, roads and icons drawn as egui shapes on top;
- lenses (sectors and water first; risks, faith, floods and fertility with their milestones);
- tools: zoom, alerts, home, world map, collapse;
- realm styles as JSON, and a `minimap` block on tile defs for fields and landmarks.

No save-version bump: the minimap is rebuilt on load.


### UI: Realm UI (large)

See [realm-ui.md](realm-ui.md):
- skins per realm as data (9-slice panels, buttons, controls, fonts, colours), and a script that draws their placeholder art from the mockup generator;
- the main menus with a persistent profile (Continue, campaign progress, Custom Game);
- the world map, the intermission and briefing, the comic viewer;
- the HUD restyle (goals tracker, rating badges, message log) and the City book of management pages;
- Đại Long moves to the mockup style, and the prototype's UI sprites go.

No save-version bump: progress lives in a profile file in the user configs folder (`saves/user/profile.json`).

## Phase 2 outline

These get their own plans when Phase 2 begins:

- **Elite housing:** the second housing track and elite lots (GDD 5.2.1).
- **Monuments and guilds:** staged construction with materials delivered and guild workers; the Temple of Learning and the Imperial Citadel (GDD 5.15).
- **New walkers** (militia, raiders, caravans) use 8-way movement (U) and get all 8 directions of art from the start.
- **Defence and attacks:**
  - walls, gatehouses, moats, towers, watchtowers and fortresses with automatic militia, and the weapons workshop;
  - attacker AI, raids, warnings and damage (GDD 5.16);
  - plunder: losing a tax office to an attack takes gold from the city treasury (decided in [finance.md](finance.md));
  - uses terrain height for chokepoints.
- **Sea trade and docks;** foreign partners on the world map; gifts, relations and tribute (GDD 5.12 and 5.14).
- **The silk chain;** Đại Long's signature goods.
- **Examinations and scholar-officials** (GDD §6).
- **Disease and hospitals;** crime and unrest.
- **Map choices** for missions 6–8 (two maps each).
- **Story presentation:** the remaining comics, in-mission story events, and the stele screen.
- **Missions I-6 to I-10.**

## Fixes the research turned up

These are bugs or gaps, each tied to the milestone that fixes it:

| Issue | Where | Fixed in |
|---|---|---|
| The world update's delta is multiplied by speed twice | `sim/mod.rs:165–166` | M1 (fixed, with the frame the step fires on, which it also left out) |
| `SimCmds` building spawns charge the tile cost through `Spawner` defaults | `sim/commands.rs:684`, `:851` | M3 Phase 0 (fixed) |
| Service access doesn't check that the service building is operational | `sim/context.rs:510` | Decided under construction.md Q2 |
| Undo refunds gold without updating `world.stats` | `undo_redo.rs`, `sim/resources.rs` | M3 Phase 1 (fixed) |
| Storage accessors see only the first slot holding a good, so walkers fetch at most 4 at a time; a granary reports room for goods it can't hold, and a runner taking wine there crashes | `building/storage.rs` | After M3 (fixed) |
| A market full of one good still asks for 0 of it; with nothing else in storage, its runner stays at a storage holding that good, taking nothing, and the market can't send another | `building/service.rs`, `unit/task/fetch.rs` | After M3 (fixed) |
| Demolishing a tax office throws away the gold it holds | `building/service.rs` | M4 Phase 0 (fixed: collectors pay into one city treasury) |
| Save and load failures are only logged (known-issues) | `session.rs` | M11, which can now show them |

## Save-version log

Bumps should be grouped where milestones land close together. Each bump needs a migration script, migrated sample saves and `--smoke-test-saves`.

| Version | Contents | Milestone | Status |
|---|---|---|---|
| 2 | Current | — | Shipped |
| 3 | Calendar | M1 | Done |
| 4 | Building construction state; house renovation timers | M3 | Done |
| 5 | Goods saved by name, not position | M5 | Done |
| 6 | One signed treasury (tax offices hold no gold); the yearly ledger, saved by category name; the finance rates and debt counter | M4 | Done |
| 7 | Fire and collapse risk per building; the city's risks switch | M6 | Done |
| 8 | Elevation map | T | Planned |
| … | Flood, favour and ratings state | M7–M9 | Planned; group them where possible |

## Conventions for every milestone

- **Art on demand** (decided 2026-11-05): a milestone that adds a building, overlay or terrain kind without art generates it with the Blender pipeline as part of its own work: a recipe, a catalog entry and `tilegen.sh` ([placeholder-art.md](placeholder-art.md), the generator's README). Art isn't rendered ahead of the milestone that needs it.
- **Data first:** new buildings, goods, events, patrons and mission settings live in `assets/configs/` JSON, so Phases 3 and 4 can add realms without code changes where possible. Nothing realm-specific is hard-coded, not even Đại Long names.
- **Determinism:** new systems draw from the simulation's RNG and run in a fixed order. `determinism.jsonl` keeps passing.
- **Automation:** every new system can be read through a query command (for `assert.*` and `wait.until`) and driven by a command where it makes sense. Each milestone adds a scenario.
- **Tests:** unit and integration tests use the custom harness in `crates/game/tests/`, one file per system.
- **UI:** retail UI goes through the `engine::ui` API and the style files; new screens pass `ui.layout_issues`.
- **Docs:** each milestone updates AGENTS.md or `.claude/rules/` with anything durable it learned, and ticks its plan.
