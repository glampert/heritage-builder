# Faith and festivals

**Status (2026-11-16): Reviewed; not started.** The user answered Q1 (dedication on construction, a tile variation per patron, picked from a submenu in the build palette), and approved Q4, Q6, Q8, Q13 and Q14 on 2026-11-16 (Q4 and Q6 to be balanced later). The other [open decisions](#open-decisions) keep their defaults unless the user decides otherwise. This plan is milestone M8 of the [roadmap](roadmap.md). It covers GDD 5.11:
- patron spirits or deities per realm;
- favour, raised by shrines and temples;
- blessings and misfortunes;
- festivals.

Mission I-3 teaches "the shrine, the temple … the first festival" and I-4 "the Mountain Lord and the Water Lord, and patron favour" ([campaigns.md](../design/campaigns.md#3-the-village-of-spirits)). It was researched against `fe12e57` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows. The retail screens follow the realm UI's skins and mockups ([realm-ui.md](realm-ui.md)).
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo +nightly fmt --all`, both clippy runs, `cargo test --workspace` and the scenario scripts before committing.
  - Follow the Saves workflow for the format change in Phase 0.
  - Generate the new art with the Blender pipeline (art on demand).

## Goals

1. **Each realm has its patrons, as data.** Four per realm, from the Heritage mockups (`docs/design/mockups/generator/pages_content.py`, `HERITAGE`). Each has a name, a domain, and a blessing and a misfortune. Nothing realm-specific is hard-coded (roadmap conventions).
2. **Shrines and temples are dedicated to a patron when they're built**, as in Pharaoh and Caesar: each patron has its own shrine and temple, a variation of the tile with its own art, picked in the build palette. They raise its favour by the residents in their reach. A patron nobody tends loses favour.
3. **Favour has consequences.** A pleased patron now and then sends a blessing; a neglected one a misfortune. Effects that need systems the game already has work now: a rich harvest, blight, a windfall, a fire, a sickness. Ones that need later systems (floods, trade, scholars) wait for them as data.
4. **Festivals.** Each realm has a festival year, a festival per slot with its patron. The player plans one from the festival square at a scale. It costs gold and goods, and raises its patron's favour. The festivals held are kept for M9's Culture and Legitimacy.
5. **The player can see it.**
   - The City book's Heritage page: its Patrons and Festivals tabs, after the mockup.
   - The minimap's Faith lens.
   - The build palette's submenu of a shrine's or a temple's patrons.
   - The inspector's patron line on a shrine or a temple.
   - Messages for blessings, misfortunes and festivals.
6. **Later milestones can add to it.**
   - M7's floods read the Mountain Lord's and the Water Lord's favour (GDD 5.11).
   - M9's ratings read the festivals held and the favour.
   - M10's trade gets its windfall.
   - M11's advisors speak for it.

## What exists today

**Design:**
- The GDD (5.11) gives each realm 3 to 5 patrons, blessings and misfortunes, and festivals from a festival square or temple. Festivals cost goods and gold, raise Culture and favour, and calm unrest; each realm has its own festival calendar.
- [world.md](../design/world.md) describes each realm's faith, fictionalised and never named (principle 5).
- The Heritage mockups (`city-heritage-<realm>.jpg`, from `pages_content.py`) have the content this plan uses as its starting data:

  | Realm | Patrons, with their domain (favour shown, mood, blessing or misfortune) | Festivals (month, patron) |
  |---|---|---|
  | Đại Long | The Mountain Lord: dykes, hills and patience (74, Pleased, a gentle flood). The Water Lord: the river and its floods (38, Restless, a severe flood). The Village Guardian: homes, harvests and the communal hall (61, Content, a rich harvest). The Ancestors: learning, duty and the family altar (55, Content, scholars come from afar). | New Year (1, every patron); Dragon-boat races (5, Water Lord); Floodwater offering (8, Water Lord); Mid-autumn lanterns (9, Village Guardian) |
  | Kambara | The Serpent King: the lake, the rains and the barays (66, Pleased, full reservoirs). The Sky Deva: the king's mandate from the heavens (58, Content, envoys bring gifts). The Lady of the Earth: rice fields and the harvest (47, Watchful, a rich harvest). The Compassionate One: hospitals, rest houses and the poor (29, Neglected, sickness in the quarters). | New Year (4, every patron); Ploughing ceremony (5, Lady of the Earth); Festival of the waters (11, Serpent King); Lamps on the baray (12, Sky Deva) |
  | Tampadesa | The Teaching of the Elders: monasteries, monks and alms (71, Pleased, novices join the monasteries). The Lord of the Mountain: the hills, the mines and the house post (52, Content, rich seams in the mines). The River Spirit: the river, the weirs and the boats (44, Watchful, easy river trade). The House Guardian: homes, kitchens and fire (33, Restless, fires in the quarters). | Relic procession (3, Teaching of the Elders); Water festival (4, River Spirit); Robe offering (8, Teaching of the Elders); Festival of lights (10, every patron) |

  The festival planner offers three scales (Modest, Grand, Imperial or Royal). Each lists its costs in gold and goods (Đại Long's Floodwater offering, Grand: gold 240, rice 60, wine 12) and its effects:
  - favour to its patron;
  - Culture;
  - the realm's Legitimacy;
  - "Unrest: calmed for a season".

  The mood words are Pleased, Content, Watchful, Restless and Neglected.

**Buildings** (`building/mod.rs`):
- `BuildingKind` is a `u32` bitflag with bits 0–23 used; saves write a kind by name.
- `Shrine` and `Temple` are service buildings (`assets/configs/buildings/configs.json`):
  - The shrine has no workers and an `effect_radius` of 10.
  - The temple has 1 to 5 workers, an `effect_radius` of 40, and a monk on patrol (`UnitConfigKey::Monk`).
- House levels need them: level 2 a shrine, level 3 a temple, level 4 both. A house has a service when one is near its road link (`Building::has_access_to_service`, `mod.rs:1803`), not by a walker's visit (known-issues).
- `ServiceBuilding` (`building/service.rs:194`) holds workers, a stock, a runner, a patrol and its timer. It has no per-building choice a player can set yet.
- **Tile variations:**
  - The shrine's tile def has two variations (`var0`, `var1`) and the temple's three, all drawing the same hand-made sprite (`assets/tiles/objects/tile_set.json`).
  - Every placed building gets a random variation (`Building::set_random_variation`, called from `ObjectSpawner::try_spawn_building_with_tile_def`, `world/object.rs:648`, and from the settlers).
  - A tile's variation is saved by index, so variations are append-only, but a variation can be renamed.
  - The tile generator owns a tile only if it wrote its sprites; `build.py --adopt TILE` takes over hand-made art (its README).
- **The build palette** (`menu/in_game/palette.rs`): a sector's grid of icon tiles, one per tile def. A click picks the tile (`TileSelection`, `tile/selection.rs`) and closes the grid. There's no way to pick a variation; the developer editor's `Tile Selection` has none either.
- There is no festival square, communal hall or other venue.

**The simulation** (`sim/mod.rs`):
- `Simulation` saves `finance`, `calendar` and `risks` beside the RNG and the task manager. That's where a city-wide `CityFaith` belongs, as `CityRisks` does.
- Game systems (`system/mod.rs`) get `on_new_month`, `on_new_season` and `on_new_year` after the world step that crosses into them, on a read-only context with `SimCmds`.
- The calendar knows the month and the season; a mission sets its wet months.

**Goods and gold:**
- `Finance` records spending by `LedgerCategory` (`Taxes`, `Construction`, `Wages`, `Upkeep`). A new category needs no save-version bump.
- `CityStock` (`world/city_stock.rs`) withdraws materials from the storage yards only. Festival goods include rice, which lives in granaries, so festivals need a withdrawal across both.
- Wine and Pottery are goods.

**Missions** (`campaign/config.rs`):
- `MissionGoal` has `Population`, `Employment`, `Treasury` and `Resource`.
- Campaign I's three missions in `campaigns.json` are placeholders until M13 writes I-1 to I-5. Campaigns II and III have none.
- A mission's `risks` block turns systems off per mission; faith gets the same.

**UI:**
- The City book's Heritage page (`menu/dialog/city_book/heritage.rs`) has its tabs, Monument, Patrons and Festivals, each an empty card saying it comes later.
- The minimap has `MinimapLens::Faith`, with its button sprite (`lens_faith`) and label, but no provider.
- The inspector (`crate::inspect`, `menu/in_game/inspector/`) paints a panel per building kind from an `InspectReport`'s sections.
- The HUD's message log (`messages.rs`) takes `Construction`, `Warning` and `Finance` messages, each with a line icon.

**Save version:** 9.

## Design

### 1. Patrons and festivals as data (Phase 0)

- **One file per realm**, `assets/configs/game/faith/<realm>.json`, loaded once after `campaigns.json` (`FaithConfigs::load`) and checked: unique ids, references, months 1 to 12, goods by name, text keys present. It holds:
  - `patrons`: an `id` (`mountain_lord`), a `name` and `domain` (Realm keys, `patron.<realm>.<id>` and `.domain`), an `icon` (a HUD line icon), a `colour` for the Faith lens, an optional `appears` (`{ "campaign": ..., "mission": ... }`, decision Q14), and `blessings` and `misfortunes` (lists of event ids, §3);
  - `festivals`: an `id`, a `name` and `text` (Realm and Story keys), a `month`, a `patron` id or `"all"`, a `venue` text key, and `scales`. Each scale has a name key, `gold`, `goods` (by name), the `favour` it gives, and `culture` and `legitimacy` (recorded for M9, Q12);
  - `events`: the blessings and misfortunes (§3).
- **The mockups' content is the starting data**: the four patrons and four festivals per realm in the table above, with their costs and effects. Kambara's and Tampadesa's are used by custom games and the hidden test campaigns until Campaigns II and III have missions.
- **Strings:** names, domains, festival texts and venues in `en.json`. Patron and place names go under `Realm`; festival texts under `Story`.
- **A mission's `faith` block** in `campaigns.json` turns parts off: `favour` (favour moves and events roll) and `festivals` (festivals can be planned). Both default to on. A custom game has them on (Q13).

### 2. Favour (Phases 0 and 2)

- **State:** `sim::faith::CityFaith`, saved in `Simulation` and read through `SimContext::faith()`. It holds:
  - favour per patron, 0 to 100, by patron id (saved by name, as goods are);
  - each patron's months since its last event (§3);
  - the festival plans and the log of festivals held (§4);
  - the effects under way (§3).

  A patron the realm's file no longer lists is dropped on load; a new one starts at the starting favour.
- **Dedication** (Q1, §2a): a shrine's or a temple's patron is its tile's variation, named by the patron's id. It's chosen in the build palette and never changes: rebuild to rededicate. A variation that names no patron of the game's realm (a Kambara shrine before its art exists) tends no one.
- **Coverage** (Q2, Q3), once a month (`on_new_month`):
  - For each patron, count the residents of the houses within the effect radius of its shrines and staffed temples. Count along the grid, as the Water lens does, with no path search.
  - Weight them: a temple counts `temple_weight` (3) times a shrine, and a house reached by several counts once per patron at its best weight.
  - Coverage is that count over the city's population times `coverage_needed` (0.5), capped at 1. A patron tended in half the city is fully covered, so four patrons don't each need the whole city.
- **Drift** (Q4):
  - Each month a patron's favour moves `favour_drift` (25%) of the way to its target, 100 × coverage.
  - With no shrines or temples the target is 0 and favour falls.
  - A festival adds to favour at once, and it drifts back from there.
  - Favour starts at `starting_favour` (50) in a new game or mission.
  - Difficulty scales the drift later (M9 and M13, GDD: "how fast favour fades").
- **Mood** (Q5): Pleased from 65, Content from 50, Watchful from 40, Restless from 30, Neglected below. These fit every value in the mockups.
- **While `favour` is off for the mission**, favour holds and no event rolls. Dedication and festivals still work.

### 2a. Dedicated shrines and temples, and picking a variation (Phase 1)

- **A variation per patron:**
  - The shrine's and the temple's variations are named by patron id: `mountain_lord`, `water_lord`, `village_guardian`, `ancestors` for Đại Long.
  - Saves keep a variation by index, so the existing ones are renamed in place. The shrine's `var0` and `var1` become the Mountain Lord's and the Water Lord's, the temple's `var0` to `var2` the first three; the rest are appended.
  - A shrine or temple in an old save thus keeps the patron its random index now names, with no migration for it.
  - Kambara's and Tampadesa's patrons get variations (appended) when their art is made, not in M8.
- **Art** (Đại Long only, the user's call):
  - Four shrines (1 × 1) and four temples (3 × 3) from the Blender pipeline, each with its patron's motif:
    - the Mountain Lord: grey stone, a mountain stele;
    - the Water Lord: blue-green tiles, a carp or wave ridge, a water basin;
    - the Village Guardian: a communal roof, red lanterns, a banyan;
    - the Ancestors: an incense altar, gilded boards, a scholar's gate.
  - Each has its own recipe parameters and the same footprint and draw size as the shrine or temple it replaces.
  - The generator adopts the two hand-made tiles (`--adopt shrine temple`) and its catalog then owns them.
- **Pickable variations, for any tile** (the user asked for it to be general):
  - A tile def can say its variations are picked by the player: `"pick_variation": true` in its `tile_set` fields (catalog and `tile_set.json`).
  - Each variation takes an optional `realms` list, the realms it's offered in, and its label is a Menu string, `tile_variation.<tile>.<variation>` (the patron's name for these).
  - A variation without `realms` is offered everywhere. A tile with no variation for the realm is placed as before.
  - Other tiles can use the same fields later (a house style, a statue's figure) with no code change.
- **The build palette:**
  - The grid shows one tile for the shrine and one for the temple, as now.
  - A click on a tile that picks its variation opens a child submenu by it, instead of picking the tile at once: a text button per variation offered in the realm, in the realm's style (`skin::button`, the `secondary` role), with its label, and a tooltip with the patron's domain where there's one.
  - A click on a button picks the tile and that variation, and closes the submenu and the grid. Escape or a right click closes the submenu and leaves the grid open.
  - With one variation offered, the tile is picked at once.
  - Paths: `TilePaletteVariationMenu: <tile title>/<label>`, e.g. `TilePaletteVariationMenu: Shrine/The Water Lord`.
- **Placement:**
  - `TileSelection` carries an optional variation. `TilePlacement::place` places that variation instead of a random one, and so do undo and redo, which already keep the tile.
  - A picked tile's cost, tooltip and disabled state are the tile's, as now.
  - A tile that picks its variation, placed with none chosen (presets, `SimCmds` spawns), gets a random one of those offered in the realm.
- **The developer editor:** its `Tile Selection` palette shows a `Variation` dropdown under the tools while a tile that picks its variation is selected, set to the first offered.
- **Automation:** `world.place` takes `variation=` (a name), and `tile.at` and `world.building_at` report it.
- **The inspector** shows "Dedicated to <patron>" with the patron's mood on a shrine's or a temple's panel. It's read-only.

### 3. Blessings and misfortunes (Phase 3)

- **Rolls** (Q6): once a month, after the drift, each patron rolls the simulation's RNG, at most one event per patron every `event_cooldown_months` (6):
  - a patron at or above `blessing_favour` (70) has `blessing_chance` (0.15) of one of its blessings;
  - one at or below `misfortune_favour` (30) has `misfortune_chance` (0.15) of one of its misfortunes;
  - between the two, nothing happens.

  The order is fixed (patrons in file order), so runs repeat.
- **Events** are data: an `id`, a message key, and an `effect` from a fixed set the code knows (Q7). The effects M8 builds:

  | Effect | What it does |
  |---|---|
  | `harvest { scale, months }` | Farms produce `scale` times as much for `months` (a rich harvest: 1.5 for 3) |
  | `blight { scale, months }` | Farms produce `scale` times as much (blight: 0.5 for 3) |
  | `windfall { gold }` | Gold into the treasury, recorded as a new `LedgerCategory::Offerings` (income) |
  | `fire { count }` | Sets that many random buildings that can burn on fire (M6's `ignite`) |
  | `sickness { count }` | Starts an outbreak in that many random houses with people (M6's `start_outbreak`) |
  | `upkeep { scale, months }` | Building upkeep scaled (a lean year, or a blessed one) |

  Effects that need later systems are listed with a `requires` (`floods`, `trade`, `education`), skipped until it lands:
  - the gentle and severe floods (M7);
  - the envoys' gifts and easy river trade (M10);
  - the scholars and novices (later phases).

  Patrons whose every event waits (the Water Lord until M7) still have favour, which M7 reads.
- **Effects under way** are kept in `CityFaith` with their months left, and farms read their scale through `SimContext::faith()`.
- **Messages:** a blessing is a new `MessageKind::Faith` message; a misfortune a `Warning` with a minimap alert where it lands (a fire's and an outbreak's alerts already come from M6).

### 4. Festivals (Phase 4)

- **The venue** (Q8): a new building, the festival square. It's a `Service` archetype with workers, `BuildingKind::FestivalSquare`, a 3 × 3 open square, its art made with the Blender pipeline. It's renamed per realm through Realm strings ("festival ground" in Tampadesa). A festival needs a staffed one.
- **The festival year** is the realm's festivals, each once a year in its month.
- **Planning** (Q9):
  - The player plans the next festival from the Heritage page (or automation) at a scale, any time before its month.
  - At the start of that month (`on_new_month`) it's held if the venue is staffed and the treasury and the storage yards and granaries hold its costs. The costs are taken then: gold through `Finance::spend(Festivals)`, goods through a withdrawal across yards and granaries.
  - If something is short, it isn't held, and a message says what was short.
  - A plan can be cancelled. One plan at a time (Q10).
- **Effects:**
  - Its patron's favour, or every patron's for an "every patron" festival, rises by the scale's `favour`, capped at 100.
  - The festival is logged (date, id, scale, culture, legitimacy) for M9, which turns it into Culture and Legitimacy (Q12).
  - While held, the square shows a festive overlay for the month (bunting and lanterns, a Blender overlay as the scaffolds are).
  - Unrest doesn't exist until game Phase 2, so "calmed for a season" is recorded only.
- **A new goal**, `MissionGoal::FestivalsHeld { min }`, for I-3's "one festival held". Favour goals wait for M9's ratings (Divine Favour and Merit are Legitimacy there).

### 5. What the player sees (Phases 1 to 4)

- **Heritage page, Patrons tab** (Phase 2): a row per patron after the mockup's `patron_row`: icon, name, domain, mood and favour, a meter, and its blessing or misfortune. Under it, the shrines and temples per patron.
- **Heritage page, Festivals tab** (Phase 4), after `city-heritage-<realm>.jpg`:
  - the festival year as a month strip, with "now" and dots for festivals;
  - the list of festivals with their patron, month and state (held this year, next, to come), and "Last held";
  - the planner: name, venue, text, the scales as a step selector, "Hold in month N" (or "Cancel"), and the costs against what the city holds and the effects.

  The Monument tab stays empty until game Phase 2.
- **The Faith lens** (Phase 2):
  - shrines and temples in their patron's colour;
  - houses by how many patrons tend them, from none to all;
  - a heat field of the shrines' and temples' reach.

  The legend lists the patrons.
- **The inspector:**
  - A shrine's or a temple's panel gets a "Dedicated to" line with its patron's mood (Phase 1, §2a).
  - The festival square's panel shows the next festival planned (Phase 4).
- **Messages:** blessings, misfortunes, festivals held and festivals that couldn't be held.

### 6. Automation, tests and scenario

- **Commands:**
  - `faith.state` reports favour, mood, coverage and the shrines and temples per patron, the effects under way, the festival year with each festival's state, the plan, and the log.
  - `faith.set` takes `patron` and `favour`, and `enabled` or `festivals` for the mission's switches.
  - `faith.event patron=... event=...` sets off an event now.
  - `faith.plan festival=... scale=...` and `faith.cancel` plan and cancel a festival.
  - `world.building_at` reports a shrine's or a temple's `patron`; `world.place` takes `variation=` (§2a).
- **Debug:** World Debug gets a Faith tab: favour sliders, the switches, the plan and the log, and buttons to set off each event.
- **Tests** (`crates/game/tests/faith.rs`, a new `[[test]]`):
  - the data loads and checks;
  - a shrine's patron is its variation's, and one with no patron of the realm tends no one;
  - a chosen variation is placed, kept through undo and redo, and a random offered one when none is chosen;
  - coverage counts weights and staffing;
  - favour drifts and starts as configured;
  - mood bands;
  - rolls are deterministic and respect the cooldown and thresholds;
  - each effect does what it says and expires;
  - a festival is held, short or cancelled;
  - the goal;
  - the state round-trips through a save.
- **Scenario** (`faith.jsonl`):
  - builds a small city with shrines and a temple;
  - checks favour rises over months and falls without them;
  - builds a shrine to the Water Lord from the palette's submenu and checks its variation and patron;
  - sets off a fire misfortune;
  - plans a festival from the Heritage page at Grand;
  - fast-forwards to its month;
  - checks its costs were taken and favour rose;
  - checks a short festival isn't held.

## UI changes

| Where | Change | Phase | Files |
|---|---|---|---|
| City book, Heritage page | The Patrons tab's rows (Phase 2) and the Festivals tab's year, list and planner (Phase 4), after the mockup. The Monument tab is unchanged. | 2, 4 | `menu/dialog/city_book/heritage.rs` |
| Minimap lens bar | The Faith lens button shows (its sprite exists), with its legend. | 2 | `tile/minimap/lens.rs`, minimap styles (`faith_*` colours) |
| Build palette | A tile that picks its variation (the shrine, the temple) opens a submenu of text buttons in the realm's style, one per variation offered, beside the grid; a button picks the tile and the variation. General for any tile. | 1 | `menu/in_game/palette.rs`, `tile/selection.rs` |
| Developer UI only | `Tile Selection`'s `Variation` dropdown. | 1 | `debug/palette.rs` |
| Inspector, shrine and temple | "Dedicated to" with the patron's mood, read-only. | 1 | `inspect/building.rs`, `menu/in_game/inspector/` |
| Inspector, festival square | The next festival planned. | 4 | same |
| Build palette | The festival square, data only. | 4 | `tile_set.json`, `configs.json` |
| Message log | A `Faith` message kind with its line icon. | 3 | `messages.rs`, `menu/in_game/message_log.rs` |
| Developer UI only | World Debug's Faith tab. | 0 | `debug/world.rs` |
| Map drawing | Each patron's shrine and temple (Phase 1); a festival square's festive overlay while a festival is held (Phase 4). Not UI, but visible. | 1, 4 | `tile/rendering.rs`, tile sets |

## Phases

### Phase 0: Data, state and the save format (medium)

- [ ] `assets/configs/game/faith/<realm>.json` for the three realms, from the mockups; `FaithConfigs` loads and checks them; strings in `en.json`.
- [ ] The knobs in `GameConfigs::sim.faith` (`FaithRules`, beside `RiskConfigs`): starting favour, drift, coverage needed, temple weight, the thresholds, chances and cooldown.
- [ ] `sim::faith::CityFaith` in `Simulation`, `SimContext::faith()` and `SimContextMut::faith_mut()`, started per mission (`GameSession::start_mission`, `start_custom_game`).
- [ ] The mission's `faith` block (`MissionFaithDef`).
- [ ] Save v10, `v9_to_v10.py` (the faith state at its start), migrated sample saves, `--smoke-test-saves`.
- [ ] `faith.state`, `faith.set`; World Debug's Faith tab.
- [ ] Tests: the data, the round trip.

### Phase 1: Dedicated shrines and temples (medium)

- [ ] `pick_variation` and per-variation `realms` in tile defs and the generator's catalog; labels as `tile_variation.<tile>.<variation>` strings.
- [ ] Art: adopt `shrine` and `temple`, a recipe each with a patron parameter, Đại Long's four variations of each (renaming `var0` onwards in place, appending the rest), `tilegen.sh`.
- [ ] `TileSelection` with a variation; `TilePlacement::place`, undo and redo place it; spawns without one pick a random offered one.
- [ ] The build palette's variation submenu (general for any tile that picks its variation), in the realm's style; the developer editor's `Variation` dropdown.
- [ ] A shrine's or a temple's patron from its variation; the inspector's "Dedicated to" line.
- [ ] `world.place variation=`, `tile.at` and `world.building_at` report it.
- [ ] Tests: the variation picked and kept through undo and redo; a random offered one; the patron. A `ui_build`-style check of the submenu (`ui.layout_issues` 0).

### Phase 2: Favour, the Patrons tab and the lens (medium)

- [ ] Coverage and drift on each new month; mood.
- [ ] The Heritage page's Patrons tab.
- [ ] The Faith lens.
- [ ] Tests: coverage, drift, mood, the switch.

### Phase 3: Blessings and misfortunes (small to medium)

- [ ] The rolls and the cooldown; the effects in the table, with `requires` for later systems.
- [ ] Farms' scale, the windfall's ledger category, the fire and sickness through M6.
- [ ] `MessageKind::Faith`; misfortunes' alerts.
- [ ] `faith.event`.
- [ ] Tests: rolls, each effect, expiry.

### Phase 4: Festivals (medium)

- [ ] The festival square: building kind, config, art (Blender recipe and catalog entry, and the festive overlay).
- [ ] Plans, holding at the month's start, the costs across yards and granaries, `LedgerCategory::Festivals`, effects and the log.
- [ ] `MissionGoal::FestivalsHeld`.
- [ ] The Heritage page's Festivals tab; the festival square's inspector line.
- [ ] `faith.plan`, `faith.cancel`.
- [ ] Tests and `faith.jsonl`.

### Phase 5: Documentation (small)

- [ ] AGENTS.md (a Faith entry under Runtime architecture, the palette's variation submenu, `faith.jsonl` in the scenario list, `faith` in the test list), the generator's README (`pick_variation`, `realms`), the roadmap's M8 row and save-version log, known-issues for what's left (Kambara's and Tampadesa's shrine and temple art).

## Risks

- **Balance.** Four patrons per realm is a lot to tend for an early city. `coverage_needed` and the drift are the knobs; M13 tunes them per mission. The I-3 and I-4 missions can turn favour off until it's taught.
- **Misfortunes on top of M6.** A fire or a sickness from a neglected patron lands on a city that may already be at risk. The cooldown and the thresholds keep them rare; the message says which patron sent it, so the player knows what to tend.
- **Festival goods in granaries.** `CityStock` only reads storage yards today. A withdrawal across yards and granaries must keep the world stats and `CityStock` in step, as destruction does (M6).
- **Data before campaigns.** Kambara's and Tampadesa's patrons are written before their campaigns. Their files may change when Campaigns II and III are designed in detail; the save keeps favour by id, so renaming one drops its favour.
- **Old saves' shrines and temples.** Their random variation index now names a patron, so an old city's shrines are dedicated at random among the first two patrons (temples the first three). That's harmless, and better than a migration that can't know what the player meant.
- **The palette's submenu at small sizes.** A child of a child: it must fit beside the grid on a 1280 × 720 window and at 200% UI scale. `ui.layout_issues` checks it.
- **Respect for living faiths** (world.md principle 5). The patrons are fictionalised and their names descriptive; festival texts must stay respectful. The user reviews the strings.

## Open decisions

Each has a default; proceed with it unless the user decides otherwise.

- **Q1. Dedication.** *Answered 2026-11-16: on construction, as in Pharaoh and Caesar.* A shrine and a temple per patron, as tile variations with their own art (Đại Long's first), picked from a submenu of text buttons that a single palette tile opens; the submenu is general, for any tile whose variations the player picks (§2a). The default was a dedication set in the inspector.
- **Q2. Coverage.** *Default: residents within the effect radius along the grid, once a month, no path search.* A path search per house per patron would cost as M6's well check does, for little gain.
- **Q3. Shrine and temple weights.** *Default: a temple counts 3 times a shrine, and only while staffed.* A shrine is cheap and small; a temple has priests.
- **Q4. Favour model.** *Approved 2026-11-16, to be balanced later: favour drifts each month a quarter of the way to 100 × coverage; starts at 50; festivals add at once.* A drift keeps it readable on the Patrons tab and gives festivals a lasting but fading effect.
- **Q5. Mood bands.** *Default: Pleased from 65, Content 50, Watchful 40, Restless 30, Neglected below.* They fit every value in the mockups.
- **Q6. Events.** *Approved 2026-11-16, to be balanced later: one roll a month per patron; a blessing at favour 70 or more, a misfortune at 30 or less, each 15% a month, at most one per patron every 6 months.*
- **Q7. Effects without their system.** *Default: data with a `requires`, skipped until the system lands.* The Water Lord's floods wait for M7.
- **Q8. The venue.** *Approved 2026-11-16: a new festival square in every realm, renamed per realm.* The GDD allows a temple too; one venue keeps the planner simple.
- **Q9. Timing.** *Default: plan ahead, held at the start of its month if the venue and costs are there; nothing is taken until then.*
- **Q10. How many.** *Default: each festival once a year, in its month, and one plan at a time.*
- **Q11. Where the goods come from.** *Default: storage yards and granaries.* Rice lives in granaries.
- **Q12. Culture, Legitimacy and unrest.** *Default: recorded in the festival log for M9 and game Phase 2.* The planner shows them as the mockup does.
- **Q13. Mission switches.** *Approved 2026-11-16: a mission's `faith` block, `favour` and `festivals`, both on by default.* M13 turns favour off for I-1 to I-3 if it's taught in I-4.
- **Q14. Patrons that appear later.** *Approved 2026-11-16: an optional `appears` (campaign and mission) per patron; until Campaign II exists, all show.* Kambara's Compassionate One appears with the shift in faith (II-4).
- **Q15. Difficulty.** *Default: not in M8.* The drift is one knob that M9's difficulty can scale.
- **Q16. The communal hall.** *Default: not in M8.* It's a Đại Long signature building that I-3 teaches; M13 adds it with the mission.
- **Q17. Patron names.** *Default: the mockups' names, descriptive and fictional.*
