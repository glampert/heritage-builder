# Seasons, floods and irrigation

**Status (2026-11-18): Phases 0 and 1 done (landed together), Phase 2 (the flood) done; Phase 3 (dykes and breaches) next.**
- On 2026-11-17 the user answered:
  - Q1: a dyke is a road-like line, kept up by builders walking it.
  - Q2: a flood's reach is a distance from the river.
  - Q6: discrete harvests, not the default's scaled production.
  - Q8: the sluice gate works by itself while staffed.
- On 2026-11-18 the user chose a farm redesign: a farmstead that gives the workers, and rice fields the player lays around it (Q19). It replaces today's 3 × 3 rice farm. Old saves keep loading: their rice farms become livestock farms (Q23).
- The other [open decisions](#open-decisions) keep their defaults unless the user decides otherwise.

This plan is milestone M7 of the [roadmap](roadmap.md). It covers GDD 5.10 and Đại Long's water layer (GDD §6):
- the wet and dry seasons, felt by the fields;
- the river's flood, once each wet season, which ruins what it reaches and leaves silt that makes the fields fertile;
- dykes that hold it back, and breaches when they're neglected;
- sluice gates that water the paddies behind the dykes, for a second harvest in the dry season;
- the farms that all of this acts on: a farmstead and its fields.

Mission I-4, "Taming the Cinnabar River", teaches "wet and dry seasons; the flood cycle and floodplain fertility; dykes and sluice gates, and dyke breaches; irrigation and a second harvest; … disease after a flood", with the goal "survive 3 flood seasons without a breach" ([campaigns.md](../design/campaigns.md#4-taming-the-cinnabar-river)). M7 is the one milestone M9 (ratings, goals and failure) still waits for.

It was researched against `b8381ae0` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own, except Phases 0 and 1, which land together: Phase 0's save bump removes the rice farm and Phase 1 brings its successor, so there's no build without a rice producer and no save format in between.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows. The retail screens follow the realm UI's skins and mockups ([realm-ui.md](realm-ui.md)); the user's UI redesign lives on the `claude-ui` branch, so flag every UI change.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo +nightly fmt --all`, both clippy runs, `cargo test --workspace` and the scenario scripts before committing.
  - Follow the Saves workflow for the format change in Phase 0.
  - Generate the new art with the Blender pipeline (art on demand).

## Goals

1. **Farms look like farms.** A farmstead stands among paddies the player lays out to fit the land, along a river, between dykes, around a bend, instead of many small 3 × 3 blocks. More food means more fields, not more farmhouses.
2. **The season matters.** Rice is sown in the wet season and reaped months later, in one harvest per field; an unwatered paddy lies fallow through the dry season. The message log says when the river is rising.
3. **The flood is an event the player can read.** Once a year, in the wet season, the river floods the land near it: the water shows on the map for its months, and the Flood lens shows beforehand where an ordinary flood would reach.
4. **A flood ruins and enriches.** Buildings it reaches are damaged through M6's collapse risk, houses also take disease risk ("disease after a flood"), fields under water lose their crop; the silt it leaves raises the land's fertility, which feeds the next harvests.
5. **Dykes protect, at a price.** A dyke line holds the water back, but the land behind it gets no silt, and a dyke that isn't kept up wears and can breach in a flood, flooding the district behind it.
6. **Sluice gates are the answer.** A staffed gate beside a dyke waters the fields in its reach: they're sown again in the dry season (the second harvest), and take the flood's silt without its damage.
7. **Everything is data and works on flat maps.** Each mission's flood (month, reach, months under water, which water floods) is a block in `campaigns.json`. Without terrain height (roadmap T, not started), a flood's reach is a distance from the water; T makes it respect height later.
8. **Faith hooks in.** The Mountain Lord's "gentle flood" blessing and the Water Lord's "severe flood" misfortune, written in M8 and waiting for this milestone, start to come.
9. **Automation and tests** can read and drive all of it, and a scenario plays a farming year and a flood season.

Not in M7 (see [Q14](#open-decisions)): drought (Tampadesa's threat, game Phase 4), storms and typhoons (coastal maps, game Phase 2), Kambara's barays, canals and the Breathing Lake (game Phase 3), the important-event popup of `event-<realm>.jpg` (M11), pastures for the livestock farm, and the mission I-4 itself (M13).

## What exists today

- **Design:** [gdd.md](../design/gdd.md) 5.10 (seasons and environment: "Crops have growing cycles. Some can only be planted in one season, and irrigation adds a second harvest."), §6 (Đại Long's dykes and sluice gates: "the river floods every wet season. Dykes protect, sluices let water into the paddies, and a breached dyke floods the district"), §7 (`SeasonsSurvived`, "3 floods without a breached dyke"; the distinction "never breaching a dyke"); [campaigns.md](../design/campaigns.md) I-1 (rice paddies) and I-4; [overview.md](../design/overview.md) principle 3.
- **Mockups:** `event-dailong.jpg` (`generator/dialogs_content.py:46-50`): "The Cinnabar River floods", month 7 of the wet season, "The river broke over the low fields east of the market. The dykes on the north bank held.", with its effects ("Six rice fields are under water for two months.", "Three houses by the ford were damaged.", "The silt will make the flooded fields richer next year.") and Show on map; its picture shows paddies as separate patches. Its message log reads "The river is rising: the wet season has begun." `minimap-lenses.jpg` shows no Flood or Fertility lens; the City book's Ratings mockup lists "Floods answered" in the Mandate's breakdown (`pages_content.py:43`, M9).
- **The calendar** ([sim/calendar.rs](../../crates/game/src/sim/calendar.rs)): `CalendarSettings::wet_months` (default 5 to 10, `:42`), `season_of(month)` (`:64`), and `GameCalendar::advance` (`:144-171`) pushes `NewMonth(date)`, then `NewSeason(season)` and `NewYear`. A mission's `calendar` block can set `wet_months` (`MissionCalendarDef`, `campaign/config.rs:345`); none does. `GameSystem::on_new_season` exists (`system/mod.rs:60-62`) and nothing implements it; `Simulation::update` runs the finance's month-end and then faith's `events::on_new_month` per `NewMonth` on a `SimContextMut` (`sim/mod.rs:297-310`). The HUD's top bar, the world map's bar and the City book show the season.
- **Farms** (`configs.json:381-410`): the rice farm (`rice_farm`) and the livestock farm (`livestock_farm`), both `BuildingKind::Farm`, 3 × 3, 2 to 4 workers, 2 goods every 18 s, delivered to a granary; the livestock farm also has a buffalo (`ambient_patrol`). `ProducerBuilding::update` (`building/producer.rs:224-268`) scales its progress by `work_efficiency() * faith().production_scale(kind)`. A runner takes the whole output stock in one trip (`deliver_to_storage`, `:584-622`). Nothing knows seasons; there's no field, crop or fertility state. Their tile defs carry `"minimap": { "class": "Field" }`.
- **How a save holds a farm:** its object tile names its tile def by hashes (`SerializableTileDefHandle(layer, category hash, def hash)`, FNV-1a of the names, `tile/sets.rs:1343`), the only place the save names it; its building record is `kind: "Farm"` with a `ProducerBuilding` archetype whose `production_output_stock` holds `{"kind": "Rice", "count": N}`. Rice farms are in 10 of the 13 local saves and the sample saves; the debug presets (`debug/preset_maps.rs`), the tests `game_time`, `minimap`, `house_consumption`, `unit_tasks` and `faith`, and the scenarios `smoke`, `determinism`, `minimap` and `faith` build them.
- **Terrain** (`tile/mod.rs`, `assets/tiles/terrain/tile_set.json`): a cell saves only its tile def, variation and `TileFlags` (`u32`, bits 0 to 18 used, `:127-163`). `TileKind` is a `u8` with bits 0 to 6 used (`:64-88`). Terrain tiles are 1 × 1 and drawn back to front (`tile/rendering.rs:208`, `draw_terrain_layer`), at most about 41 pixels high. Land is `dirt`, `grass`, `vacant_lot`, `dirt_road` and `paved_road` (`path_kind: Road`, 16 junction variations named by mask, `tile/road.rs:309-331`) and `water` (59 shore variations). Nothing in code or data is a floodplain, a marsh, a field, a dyke or a sluice. Rubble (`tile/rubble.rs`) is an objects-layer tile with no game object, picked per cell by variation, and changes terrain at run time (`restore_land_under_lot`, through `Spawner`).
- **Roads** are terrain tiles placed as segments (`TilePlacement::place_road_segment`, `menu/mod.rs:478-517`, over `road::build_segment` and `can_place_road`), with their junctions refreshed by `road::update_junctions`. They're the model for a line the player draws; nothing places an area yet.
- **Risks** (M6): `Building::add_risk(RiskKind, amount)` (`building/mod.rs:783`, its comment already names "later milestones' floods"), a collapse at risk 100 (`destruction::destroy_building`, `destruction.rs:40`), `start_outbreak`, the builders' guild's patrol resetting collapse risk; patrols visit the buildings beside each road node they walk (`unit/task/patrol.rs:341-361`, `visit_buildings_along_way`). [city-risks.md](city-risks.md) planned M7's floods as `add_risk` calls (`:24`, `:127`, `:287`).
- **Faith** (M8): `FaithEffectDef::Flood { severity }` (`faith/config.rs:89`) with `FaithRequirement::Floods`, never met (`:105-115`). Đại Long's Mountain Lord has `gentle_flood` (severity 0.5, a blessing) and the Water Lord `severe_flood` (1.5, a misfortune); Kambara's Serpent King has `full_reservoirs` (0.5), which is about barays rather than a river. The harvest and blight effects scale the farms' production cycle.
- **The minimap** ([tile/minimap/lens.rs](../../crates/game/src/tile/minimap/lens.rs)): `MinimapLens` already has `Flood` and `Fertility` (`:32-41`), their labels (`minimap.lens.flood|fertility`) and the developer minimap's button art (`assets/ui/buttons/minimap*/lens_flood_*`, `lens_fertility_*`, from `crates/tools/ui-icons`); `available()` leaves them out (`:72-74`), and the retail HUD's `MINIMAP_GLYPHS` has no glyph for them (`menu/skin_hud.rs:127`). `MinimapClass::Field` draws a building striped.
- **Missions and custom games:** `MissionDef`'s optional blocks (`calendar`, `finance`, `risks`, `faith`) are applied by `GameSession::start_mission` (`session.rs:653-681`). Custom Game's "Seasons and floods" toggle is stored (`CustomGameSettings::seasons_and_floods`, the save header's record) but always off and disabled, with the tooltip `custom_game.coming_in_m7` (`menu/dialog/custom_game.rs:284`, `:922-928`); `start_custom_game` has a TODO for it (`session.rs:696`).
- **Maps:** Campaign I's three missions are test maps (`tiny_island`, `test_map_64x64`, `test_map_96x96`); the debug presets have no water. I-4's delta map is M13's.
- **Saves:** `CURRENT_SAVE_VERSION` is 10 (`session.rs:965`). The roadmap's log has v11 down for T's elevation map and "flood, favour and ratings state" for M7 to M9.

## Design

### 1. The flood's data and the save (Phase 0)

- **A mission's `floods` block** (`MissionFloodsDef`, beside `risks` and `faith`), every field optional:
  - `enabled` (default false: missions before I-4 have no floods);
  - `month`, the flood's month (default 7; it must be a wet month);
  - `reach`, `[min, max]` cells the year's flood reaches from its water, drawn from the simulation's RNG each year (default `[3, 6]`);
  - `months`, how long the water stays (default 2);
  - `sources`, cells of the water bodies that flood (default: every water body of at least `min_source_cells`, so ponds don't flood);
  - `river`, a Realm text key naming the river for the messages ("the Cinnabar River"; default "the river");
  - `fertility`, the land's starting fertility within and beyond the flood's usual reach (default 70 and 40).
- **The knobs** in `GameConfigs::sim.floods` (`FloodRules`, beside `RiskConfigs` and `FaithRules`): the default profile (for custom games), `min_source_cells`, the damage (collapse and disease risk added at severity 1), the silt, the fertility's depletion and its effect on yield, the dykes' wear and the breach chance. Balance is M13's.
- **The state** is `sim::CityFloods` (saved in `Simulation`, `SimContext::floods()`, `SimContextMut::floods_mut()`): the switch and the resolved profile, a fertility grid (one byte per cell, allocated only while floods are on; without it every cell reads 50, a yield of 1), the dykes' wear (per dyke cell), the flood under way (its cells and months left), the next flood's severity (set by faith), and a log of the floods (date, reach, severity, cells, buildings damaged, fields flooded, breaches) for the goal, the messages and M9's "Floods answered".
- **Started** by `GameSession::start_mission` from the mission's block, and by `start_custom_game` from the "Seasons and floods" switch with the default profile (the toggle is enabled). A game started any other way (presets, `session.new_map`) has floods off.
- **Save v11** (`v10_to_v11.py`):
  - writes `sim.floods` with floods off;
  - turns every rice farm into a livestock farm (the user's answer to [Q23](#open-decisions)): its object tile's def hash from `rice_farm`'s to `livestock_farm`'s (same footprint, same `ProducerBuilding` state), and its output stock emptied and set to `Meat` (the rice it held is lost; a runner already carrying rice delivers it);
  - Phase 0 checks whether a migrated farm gets its buffalo (`ambient_patrol`) on load, and spawns it in `post_load` if not.
  - Phase 1's farmstead is a new building archetype, so its state needs no other bump.

### 2. Farmsteads, fields and harvests (Phase 1)

- **The farmstead** (the user's choice, [Q19](#open-decisions)): a new building, `farmstead` (`BuildingKind::Farmstead`, archetype `FarmsteadBuilding`), 2 × 2, 2 to 6 workers, in the Food and farming sector, delivering to granaries. By itself it produces nothing: its workers tend the fields around it. Realm names by Realm string (`building.<realm>.farmstead`).
- **Fields:** `rice_field`, a 1 × 1 objects-layer tile with no game object, as rubble is ([Q21](#open-decisions)), placed only on empty land within `field_reach` cells (4) of a farmstead's footprint, measured along the grid. A field doesn't burn or collapse and has no risks. Walkers go around it, as around a building ([Q20](#open-decisions)). Its look is its stage, a variation (fallow, sown, growing, ripe), which the farmstead sets as the crop moves on (`set_variation_index`, saved with the tile), so it costs no state of its own on the map. The minimap draws field cells striped (`MinimapClass::Field`).
- **Laying fields:** the palette's Rice field is an area tool: the player drags a rectangle, the preview marks each cell that can take a field (empty land, in a farmstead's reach) and the rest, and the valid cells are placed as one undo step, at `cost` gold a cell (1). The clear tool removes fields for free, as it does rubble.
- **Which farmstead a field belongs to** ([Q22](#open-decisions)):
  - a field placed joins the nearest farmstead whose reach it's in;
  - a farmstead placed claims the fields in its reach that have none;
  - a farmstead demolished, burnt down or collapsed leaves its fields without one: they lie fallow until another farmstead in reach claims them;
  - the farmstead keeps its fields' cells and each one's crop (`FarmsteadBuilding::fields`, saved), and drops a field that's cleared.
- **Tending:** a farmstead tends `cells_per_worker` (4) fields per worker it has, the nearest first, so a full farmstead tends 24. Its other fields lie fallow.
- **Crops** (the user's answer to [Q6](#open-decisions): discrete harvests), per field cell, in every game ([Q7](#open-decisions)):
  - **Sowing:** at the start of a month, a tended field with no crop growing, not under water, is sown if the month is in rice's season (the wet season, `"crop": { "season": "Wet", "grow_months": 4 }` in the farmstead's config), or in the dry season when irrigated (Phase 4), and it hasn't been sown yet that season. A crop the flood took doesn't count as the season's sowing, so the field is sown again once the water goes.
  - **Reaping:** `grow_months` after sowing, at the start of the month, its harvest is `harvest_per_cell` (4 rice) × its cell's fertility (`0.5 + fertility / 100`; 1 without floods) × its farmstead's staffing over the months it grew (each month's share of workers, averaged) × faith's harvest scale while one is under way (`CityFaith::production_scale`, applied to the harvest). The farmstead's fields reaped that month go into its stock together. Reaping can fall in any season.
  - **What that gives** with the defaults (wet months 5 to 10, four months to grow): a field is sown in month 5 and reaped in month 9, one harvest a year; with floods on, a field the flood reaches loses its crop in month 7, is sown again in month 9 and reaped in month 1 (§3); a field a sluice gate waters is also sown in month 11 and reaped in month 3 (§5). So "once a year, twice when irrigated". A full farmstead's harvest is about 96 rice, where a 3 × 3 rice farm made about 80 a year.
- **Delivering a harvest:** the farmstead's output stock holds a whole harvest, and its runner carries at most `runner_load` (16) a trip, going out again until the stock is empty. A harvest the granaries can't take yet waits at the farmstead, and a flood that reaches the farmstead takes it.
- **Faith:** a harvest or blight scales the farmsteads' harvests and the livestock farms' cycle; `events::can_happen` counts a farmstead with fields as something a harvest acts on, and a blight pings the farmsteads on the minimap.
- **The rice farm goes:** its config and tile def are removed (the migration has turned old ones into livestock farms). The debug presets place a farmstead with fields where they had rice farms; the tests and scenarios that build rice farms build a farmstead and fields, or a livestock farm where they only need a producer. `determinism.jsonl`'s checksum changes.
- **Art** from the Blender pipeline: the farmstead (a house on posts, a rice store, a buffalo pen, 2 × 2), and the field's four stages (bunds round a paddy: bare mud, water with seedlings, green, gold), 1 × 1 each, with the build palette's glyphs for both from ui-art.

### 3. The flood (Phase 2)

- **When:** the first month of the wet season posts "The river is rising: the wet season has begun." At the start of the flood's month (`CityFloods::on_new_month`, after the finance's month-end and before faith's rolls, so a festival in the same month sees it), the year's flood comes: its reach drawn from the range, times its severity when that's above 1.
- **Where** ([Q2](#open-decisions)): a breadth-first fill over land from the source water's shores, four neighbours at a time, out to the reach. Buildings, roads and fields don't stop it; dykes do (§4). Each cell it reaches is under water for the flood's months. When T lands, the fill also stops at cells above the year's flood level (terrain-height §9).
- **What it does** to what it reaches, all in one pass, from the RNG in a fixed order:
  - each finished building on a flooded cell gains collapse risk (`add_risk(Collapse, damage × severity)`), so an old or neglected building collapses and a new one survives a flood ([Q4](#open-decisions));
  - each house there also gains disease risk ("disease after a flood");
  - a field under water loses its growing crop and can't be sown while the water stays; a farmstead under water loses the harvest it holds;
  - each flooded cell gains silt: fertility + `silt`, at most 100.
- **On the map:** each flooded cell has a new `TileFlags::Flooded` (bit 19) on its terrain tile, and the renderer draws the water's sprite over the terrain, tinted with silt, under the objects, as it draws a festive square or a scaffold. Nothing new is placed on a flooded cell until the water goes ([Q16](#open-decisions)). Walkers keep walking ([Q5](#open-decisions)).
- **The water goes** at the start of the month after its last; a message says how many fields the silt reached.
- **Messages** (§7): the river rising, the flood (a Warning with the Alert sound: "The Cinnabar River floods: N buildings damaged, N fields under water."), its end. The minimap pings the damaged buildings.
- **Faith** ([Q12](#open-decisions)): `FaithRequirement::Floods` is met while the game's floods are on. A `Flood { severity }` event sets the next flood's severity (the latest wins): the gentle flood (0.5) halves its damage and keeps its silt; the severe flood (1.5) adds half again to its reach and its damage. Kambara's `full_reservoirs` gets a `requires` of its own (`reservoirs`, unmet until game Phase 3), since it isn't a river's flood.

### 4. Dykes (Phase 3)

- **What a dyke is** ([Q1](#open-decisions)): a terrain tile, like a road: the player draws it as a segment, it joins its neighbours (16 junction pieces named by mask, as roads and rubble are), and its top is a road (`path_kind: Road`), so walkers use it and buildings beside it have road access, as the dyke roads of the Sông Son do. A tile def field, `"blocks_floods": true`, is what the flood fill reads, so Kambara's embankments or Tampadesa's weirs can reuse it.
- **Placement:** the Roads sector's grid, after the roads, drawn as a segment through `place_road_segment` generalised to any road-like tile; not on water; gold per cell (more than a paved road). The clear tool removes it, as it removes roads.
- **Wear:** each dyke cell's wear (0 to 100, in `CityFloods`) rises each month at `wear_per_month`, jittered per cell as M6's rates are (hashed, never drawn from the RNG). A builders' guild walker whose patrol walks over a dyke cell resets its wear (a check in `visit_buildings_along_way`). So a long dyke needs a guild within reach of all of it.
- **Breaches:** when the flood reaches a dyke cell (the water is on its other side), the cell breaches with chance (wear / 100)² × severity, drawn from the RNG in cell order ([Q10](#open-decisions)). A breached cell turns to dirt under M6's rubble, and the fill goes on through it with the reach it has left, flooding the district behind. A breach is a Warning with an alert, and the flood's record counts it. The player clears the rubble and draws the dyke again.
- **The goal** ([Q11](#open-decisions)): `MissionGoal::FloodsSurvived { min }`, the floods since the mission's start that breached no dyke (`WorldStats::floods`, tallied with the stats, as `festivals_held` is).

### 5. Fertility and sluice gates (Phase 4)

- **Fertility** is per cell, 0 to 100, while floods are on. It starts at the profile's values (higher within the flood's usual reach), rises with each flood's silt, and falls with each harvest reaped on it (`depletion_per_harvest`). Fields behind a dyke slowly lose it, which is the dyke's price. Phase 1 reads it already (50 everywhere while it isn't kept).
- **The sluice gate:** a small building in the Food and farming sector, placed beside a dyke cell (a rule like a wharf's on water), with 1 or 2 workers. Staffed, it irrigates the fields in its reach (`effect_radius`, measured along the grid as the inspector's reach is): they're sown in the dry season too. When the flood comes, the fields in its reach take the silt, even behind the dyke, without the flood's damage: the water it lets in is controlled. It works by itself (the user's answer to [Q8](#open-decisions)): nothing to open or close.
- **Art:** the sluice gate from the Blender pipeline.

### 6. Lenses (Phases 2 to 4)

- **Flood** (Phase 2, with dykes' wear in Phase 3): the heat field is where an ordinary flood would reach now (the fill at the profile's largest reach, through the dykes as they stand), with the cells under water at the top; buildings and fields in that reach are coloured at risk, dykes from sound to worn. Its legend: Under water, Floods in an ordinary year, At risk, Dyke sound, Dyke worn.
- **Fertility** (Phase 4): the heat field is the land's fertility; field cells coloured by their stage (growing, ripe soon, fallow, irrigated); the sluice gates' reach. Legend: Fertile, Poor, Growing, Fallow, Irrigated.
- Both are offered only while the game's floods are on (`MinimapLens::available` takes the game). The retail HUD needs their glyphs, `flood` and `fertility`, drawn by ui-art for each realm, as Phase 2 of M8 added `religion`; the developer minimap's sprites exist.

### 7. What the player sees

- **Messages:** the river rising and the water going down are a new kind, `MessageKind::Season` (the HUD line icon `water`, the Notification sound); the flood and a breach are Warnings with the Alert sound; a harvest reaped is a `Season` message ("The fields of 3 farmsteads were reaped: 240 rice."). The mockup's popup waits for M11's important-event popups, which will show these same records.
- **The inspector:**
  - a farmstead's panel: its workers, its fields (tended, fallow for want of workers, under water), their crop ("Sown in month 5, reaped in month 9"), the harvest it expects at its staffing so far, and the stock it holds or is carrying away;
  - a field's panel (a new `FieldInspector`, as the terrain and prop panels are): its farmstead (or "No farmstead tends it"), its stage, its land (fertility, a meter, Phase 4), whether a sluice gate waters it, and "The flood took the crop; sown again when the water goes in month 9";
  - a sluice gate's panel: the fields it waters;
  - the terrain panel of a dyke cell: its wear (a meter, "Walked by a builder N months ago"); of a flooded cell: when the water goes.
- **The City book:** the Overview's warnings gain fields left fallow for want of workers, worn dykes ("3 dyke sections are worn: a builder must walk them") and fields under water. Its Goods page's rice follows the harvests.
- **Custom Game:** "Seasons and floods" is enabled, on by default.

### 8. Automation, tests and the scenarios

- `farms.state`: each farmstead (cell, workers, fields tended and fallow, stock, next harvest and its expected size) and each field (cell, farmstead, stage, sown, ripe in, irrigated). `world.place_area tile=rice_field from=[x,y] to=[x,y]` lays fields as the area tool does (reporting the cells placed and refused); `world.place` places a farmstead, a dyke or a sluice gate.
- `floods.state`: the switch and profile, the next flood (month, reach range, severity), the flood under way (cells, months left), the log, the dykes (count, worn, wear by cell on request), fertility (mean, and by cell on request) and `survived`. `floods.set`: `enabled`, a cell's `fertility`, a dyke cell's `wear`, the next `severity`. `floods.flood_now [reach=N]` and `floods.recede`, queued for the start of the next frame (as `events::request` is). `inspect.at` reports the new lines.
- World Debug gains a Floods tab: the profile, the next flood, buttons for a flood now and the water going, the log, the worn dykes with their cells as buttons; and the Tile Inspector a field's crop.
- `tests/farms.rs`: placing fields in and out of reach, ownership (nearest farmstead, claims, a farmstead removed and another claiming), tending by workers, sowing and reaping by season, the harvest's size (staffing, faith), a harvest carried in loads, the variation following the stage, undo of an area, a save's round trip.
- `tests/floods.rs`: the fill (reach, sources, small ponds, dykes, four neighbours), the damage through the risks, a crop lost to the flood and sown again, silt and fertility, irrigation's dry-season crop, wear and the builder's reset, breaches and the fill through them, the goal, faith's severity, a save's round trip. The harness's `tick` fires no calendar events, so both call the month's hooks as `tests/faith.rs` does.
- `farms.jsonl`: a farmstead from the palette, fields dragged around it (the preview's refused cells), its panel, a farming year fast-forwarded, a harvest reaped and carried to the granary in loads, a field's panel.
- `floods.jsonl`: a blank map with a river laid with `world.place`, floods on with `floods.set`, a farmstead and fields, houses, a dyke drawn from the palette, a sluice gate; fast-forward to the flood; check the cells, the damage, the messages, the lens; a worn dyke that breaches; the dry season's irrigated and fallow fields; the goal.

## UI changes

| Where | Change | Phase |
|---|---|---|
| Build palette | The farmstead and the Rice field (an area tool, its preview) in Food and farming, in place of the rice farm | 1 |
| Build palette | A dyke in the Roads grid, the sluice gate in Food and farming | 3, 4 |
| The map | Fields drawn by stage; flooded cells drawn with silt-tinted water | 1, 2 |
| Message log | Harvests, the river rising, the flood, the water going (`MessageKind::Season`, a new kind with the `water` icon), breaches | 1 to 3 |
| Minimap | Fields striped; the Flood and Fertility lens buttons, with ui-art glyphs per realm | 1, 2, 4 |
| Inspector | The farmstead's panel, a new field panel, the sluice gate's fields, a dyke's wear, a flooded cell | 1, 3, 4 |
| City book | Overview warnings: fallow fields, worn dykes, fields under water | 1, 3 |
| Custom Game | "Seasons and floods" enabled | 0 |
| Debrief and goals panel | The `FloodsSurvived` goal's line | 3 |

## Phases

### Phase 0: Data, state and the save format (medium; lands with Phase 1)

- [x] `MissionFloodsDef` and its checks (a wet month, a sane reach, sources on water); `FloodRules` in `GameConfigs::sim.floods`.
- [x] `sim::CityFloods` in `Simulation`, `SimContext::floods()` and `SimContextMut::floods_mut()`, started per mission and per custom game; the fertility grid allocated while floods are on; the sources resolved from the map.
- [x] Custom Game's "Seasons and floods" toggle enabled, applied by `start_custom_game`.
- [x] Save v11, `v10_to_v11.py`: floods off, rice farms turned into livestock farms; migrated local and sample saves, `--smoke-test-saves`; the migrated farms' buffalo checked. The roadmap's save log moves T's elevation map to v12. The state saves everything later phases fill (the dykes' wear, the flood under way, the log), so Phases 2 to 4 need no other bump, as M8's Phase 0 saved the festival plan and log ahead of Phase 4.
- [x] `floods.state`, `floods.set`; World Debug's Floods tab.
- [x] Tests: the data, the start, the round trip, a migrated save's farms.
- **Notes (2026-11-18):**
  - **The state** (`sim/floods.rs`): `CityFloods` holds the switch, the resolved `FloodProfile` (month, reach, months, sources, river, starting fertility), the fertility grid, the dykes' wear, the flood under way, the next flood's severity and the log. A game starts with floods off; `start` lays out the land when they're on: the sources (a cell of each water body of at least `min_source_cells`, or the mission's that are water) and the fertility, `fertility[0]` within the profile's largest reach of them and `fertility[1]` beyond. `set_enabled` does the same for automation and the debug tab, keeping the log.
  - **The fill** (`sim::floods::reach_fill`) is written now, since the starting fertility needs the usual reach: from the sources' water bodies' shores out over land, four neighbours at a time, never into a cell the caller says blocks (Phase 3's dykes). `water_bodies` finds the bodies, in row order.
  - **Fertility** is a byte a cell, saved as runs (`[value, count]`), so a 256 × 256 map of a few values saves in a few hundred bytes. While floods are off it isn't kept, and every cell reads `neutral_fertility` (50), a yield factor of 1.
  - **The start:** a campaign mission with a `floods` block floods unless it says `enabled: false`; one without has none, so missions 1 to 3 don't change. A custom game floods when its switch is on. Presets and `session.new_map` have none.
  - **Custom Game:** the switch is on by default; the details' Hazards say "Floods", "Fire, collapse", "Floods, fire, collapse" or "None". The `custom_game.coming_in_m7` string went.
  - **The World Debug window's Floods tab** turns floods on and off through a request run at the start of the next frame (`floods::request`), since the window draws while the world is borrowed, and sets the next severity. A new `crate::floods` module holds the requests; Phase 2's flood logic goes there too.
  - **The save (v11):** `v10_to_v11.py` writes `sim.floods` off (the shape the game writes, checked against a v11 save), adds `fields: []` to every producer, and turns each rice farm's object tile into a livestock farm's (its def hash, the only place a save names the tile) and its stock into an empty stock of meat. It found 23 rice farms in the 11 sample saves. A livestock farm's buffalo comes from its ambient patrol's timer in `ProducerBuilding::update`, so a migrated farm gets one as any other does, with no `post_load` change. The local saves and the sample zip were migrated; there's no Rust test of the script, which `--smoke-test-saves` covers.

### Phase 1: Farmsteads, fields and harvests (large)

- [x] The farmstead: building kind and archetype, config, Blender art, palette glyph; the rice farm's config and tile def removed.
- [x] The rice field tile with its four stages (Blender), placement in a farmstead's reach, the area tool and its preview, clearing; ownership and claims.
- [x] Tending by workers; sowing and reaping per field by season; the harvest's size; its stock and the runner's loads; the stage on the tile.
- [x] Faith's harvest and blight on farmsteads.
- [x] The farmstead's and the field's inspector panels; the harvest message; the City book's fallow-fields warning.
- [x] `farms.state`, `world.place_area`; the Tile Inspector's field section.
- [x] Presets, tests and scenarios moved off the rice farm; `tests/farms.rs`; `farms.jsonl`.
- **Notes (2026-11-18):**
  - **The farmstead is a producer, not a new archetype (deviation):** `BuildingKind::Farm` with a `crop` block in its producer config (`CropConfig`: the field tile, the season, `grow_months` 4, `harvest_per_cell` 4, `cells_per_worker` 4, `field_reach` 4, `runner_load` 16). It keeps its fields in `ProducerBuilding::fields` (saved, empty for other producers). Being a producer, it reuses the workers, the output stock and its runner, risks, faith's harvest and blight (which act on `Farm`), and the inspector's workshop panel. Its production cycle only paces its runner (every 6 s); it makes nothing. 2 × 2, 2 to 6 workers, 8 gold and no materials (the rice farm cost 5 and none, and I-1 teaches farming before materials), a Basic build.
  - **The field** (`rice_field`) is a plain objects-layer tile in a new `fields` category, `TileKind::Field` (the kind's last free bit) and `NodeKind::Field` (nothing walks it), placed as rubble is, without a game object. Its variation is its stage (fallow, sown, growing, ripe), set as the crop moves on. It's drawn anchored as a 1 × 1 building, and the minimap draws each field cell as a building feature of the `Field` class (striped), with no change to the bake. Undo, the debug palette (with the buildings) and the inspector know it.
  - **Ownership** (`farms::claim_fields`): each farmstead drops the fields that are gone and claims those in its reach that have none, each going to its nearest farmstead (the first in the world's order on a tie), and keeps its fields nearest first. It runs when the player lays a field or a farmstead, clears a field, and at each month's start, which also picks up undo and redo. `TilePlacement::place` refuses a field beyond every farmstead's reach.
  - **The area tool:** the tile selection's range drag, which only the clear tool used, also runs for a field: it marks the cells under the dragged screen rectangle (as the clear tool selects them, not a rectangle of cells), the menus mark those beyond every farmstead's reach invalid, and the release lays a field on each cell that takes one (`TilePlacement::place_area`, one undo step). `world.place_area tile=rice_field from=… to=…` lays a rectangle of cells.
  - **The month** (`farms::on_new_month`, after the finance's month-end and before faith's rolls): the claims, then each field: a crop grows a month (tended or not, counted), is reaped `grow_months` after sowing, and a tended field without one is sown in its crop's season, once a season (`sown_season`, the ordinal month its season began: `farms::season_start`). A field under water loses its crop and may be sown again (Phase 2 floods it).
  - **The harvest (deviation):** a field's harvest is `harvest_per_cell` × its yield factor × the share of its growing months it was tended, where the plan said "the farmstead's staffing over the months it grew". A farmstead's workers tend the nearest `cells_per_worker` fields each, so its staffing already decides which fields are tended; scaling those again by the staffing would count it twice. A farmstead below its minimum workers, or still a site, tends nothing.
  - **Delivery:** the farmstead's stock holds 240; its runner carries at most 16 a trip (`deliver_to_storage`), going out again until it's empty.
  - **Messages:** a new `MessageKind::Season` (`messages.push kind=season`) for the month's harvests ("The harvest is in: 32 rice from 8 fields."), with the Notification sound. Its icon is `food` until Phase 2 draws a `water` line icon for the river's messages.
  - **The panels:** the farmstead's panel has a Fields section (fields and those tended, those fallow for want of workers, the next harvest and its month, what it holds) in place of the production cycle, and says "No fields" while it has none. A field opens the terrain panel (`TerrainInspector`), not a new `FieldInspector` (deviation: it needed no widget of its own): its farmstead, stage, crop and land. The developer Tile Inspector has a Field section. The City book's Overview warns of fields lying fallow for want of workers.
  - **The rice farm** is gone: its config, tile def, sprite and HUD glyphs (ui-art `--prune`). The food sector's glyph still uses its drawing. The mockup generator's palette lists the farmstead and the rice field.
  - **Presets (deviation):** presets 1 and 2 place a livestock farm where the rice farm stood (the same 3 × 3, so their layouts keep), and presets 3 and 5, whose farm feeds a distillery, a farmstead without fields. The plan said a farmstead with fields; the presets have no room for them.
  - **Tests and scenarios moved off the rice farm:** `game_time`, `house_consumption`, `minimap` and `faith` use the livestock farm; `unit_tasks`' fallback test seeds a farmstead's stock, and freezes the distillery's production and fetching, since the farmstead's runner comes sooner than the rice farm's did and the distillery's own runner was left out at the end. `smoke` and `determinism` build a farmstead with fields; `minimap` and `faith` a livestock farm.
  - **Verified (Phases 0 and 1 together):**
    - `cargo test --workspace`, with the new `tests/farms.rs` (17: laying fields in and out of reach, the nearest farmstead, claims after a farmstead goes, clearing, an area as one undo step, nearest first, workers tending, sowing and reaping in season, once a season, months untended, a site, the stages, seasons' starts, faith's harvest, a harvest carried in loads of 16, the reports, a save's round trip) and `tests/floods.rs` (10: no floods by default, a mission's start and its fertility, sources, ponds, the reach fill and a blocked one, the switch, the block's checks, a save's round trip).
    - Clippy on desktop and web, nightly rustfmt; the tile generator's `sync_tileset.py --check` and unit tests (17), ui-art's `--check` (738 sprites) and unit tests, the recolour tool's `--check` and tests.
    - All 26 CI scenarios in release (`farms.jsonl` added to CI's list), `ui_dev_screens` among them.
    - `determinism.jsonl` twice: the same output and save; checksum `2868266e0be074da`, new since the state and the scenario's farm changed.
    - Record and replay of `ui_build` and `determinism`, the replayed save the same as the runs'.
    - `--smoke-test-saves` loads all 13 migrated saves.
    - Screenshots of the fields laid and ripe, the field's panel and the farmstead's (`out/farms/`), and the palette's glyphs in each realm.
  - **Art:** the Blender pipeline's `farms.py`: the farmstead (a house on stilts with a thatched roof and a ladder, a rice store on posts, a buffalo pen with its buffalo, a haystack and jars on a packed-earth yard) and the field's four stages (a bunded paddy: bare mud with stubble, water with seedlings, green rice, bent gold rice). ui-art draws the farmstead's glyph (the realm's house on stilts beside a rice plant) and the field's (the rice farm's paddy) in each realm.

### Phase 2: The flood (medium to large)

- [x] The wet season's message; the flood at its month: the fill, `TileFlags::Flooded`, the months under water, the water going.
- [x] The damage through M6 (collapse and disease risk), fields' crops and farmsteads' harvests lost, silt.
- [x] The flooded cells drawn; placement refused on them.
- [x] `MessageKind::Season` for the flood, its Warning, minimap pings; the flood log.
- [x] Faith's flood events available, the severity; `full_reservoirs`'s own requirement.
- [x] The Flood lens (without dykes), its ui-art glyph.
- [x] `floods.flood_now`, `floods.recede`.
- [x] Tests; `floods.jsonl` up to the flood.
- **Notes (2026-11-18):**
  - **The flood** (`crate::floods`, `floods/mod.rs`): `on_new_month` runs after the finance's month-end and before the farmsteads' month and faith's rolls. The water under way goes once its months are over (`CityFloods::month_passed`); the wet season's first month queues the river-rising notice; the flood's month brings the year's flood. Its reach is drawn from the profile's range (times the severity when that's above 1); `reach_fill` gives its cells, and `CityFloods::begin_flood` keeps them in row order (a binary search answers `is_flooded`), adds the silt and logs the `FloodRecord`. The next flood's severity is used up by it (`take_next_severity`).
  - **The damage:** each building on a flooded cell takes `collapse_damage` × severity collapse risk, a house `disease_damage` × severity disease risk too, through `Building::add_risk`, which now says whether it took the risk; that count is the record's `buildings_damaged`, and each one is pinged on the minimap. Nothing is damaged while risks are off (the harness's `no_city_risks`, a mission's `risks` block): the damage is M6's, as decision Q4 says. A farmstead under water loses the harvest it holds, and the fields under it their crops (`farms::flood_fields`; the farmsteads' month then keeps them fallow until the water goes, and sows them again if the season allows).
  - **On the map (deviation):** a flooded cell's terrain tile is drawn as the open-water sprite with the dirt sprite washed over it (the silt), in place of its land, not over it: drawn over the land, each cell's soft diamond edge let the grass show through as a grid, while drawn in its place it joins its neighbours as terrain does. The river's own cells beside the flood are flagged too, so their grassy shore art doesn't draw a strip of bank between the river and the flood. `TileMap::set_tile_flags` now passes only object tiles' flags to the minimap's features (a terrain flag under a site's base cell would have drawn the site as finished).
  - **Placement:** a flooded cell refuses every object (buildings, fields, props) and roads, with the previews marked; plain land and water can still be painted (the developer editor).
  - **Messages (deviation):** the river rising and the water going down are `MessageKind::Season` messages with the new `water` line icon and the Notification sound; the flood is a Warning with Alert ("The river floods: 2 buildings damaged, 12 fields under water."). The harvests, which Phase 1 posted as Season messages, are a kind of their own, `MessageKind::Harvest`, keeping the `food` icon, so a harvest doesn't show a water drop. The river is the profile's `river` (a Realm key), else "the river".
  - **Faith:** `FaithRequirement::is_met` takes the game's `FaithSystems`; `Floods` is met while its floods are on, so the Mountain Lord's gentle flood and the Water Lord's severe flood come, setting the next flood's severity (the latest wins). Kambara's full reservoirs requires `reservoirs`, a new requirement that waits for its barays.
  - **The Flood lens:** where an ordinary flood would reach now (`CityFloods::ordinary_reach`, the fill at the profile's largest reach with each cell's distance), stronger by the water, the water under way at the top; buildings and fields under water in `flood_under_water`, in the reach in `flood_at_risk`. It's offered only while floods are on: every lens has its button, and the bar skips the ones the game doesn't offer (`MinimapLens::is_offered`), so nothing is rebuilt; a minimap left on it goes back to the city when floods are turned off. ui-art draws its `flood` glyph and the message log's `water` icon in each realm.
  - **The panels:** a flooded cell's terrain panel has a section saying when the water goes; a flooded field's crop line says so too. The City book warns of fields under the flood (planned for Phase 3 with the dykes' warnings; it came with the flood).
  - **Automation:** `floods.flood_now [reach=N]` and `floods.recede` run at once (the commands run where the world is unlocked), not queued as the plan said; the World Debug tab's Flood Now and Recede Now are queued (`floods::request`), and the tab lists the log. `floods.set enabled=false` takes the water away first (`floods::set_enabled`). `floods.state` reports `ordinary_reach`, the flood's `recedes_in` and a cell's `under_water`. `world.place_area` also lays plain terrain (a river for `floods.jsonl`).
  - **Verified:**
    - `cargo test --workspace`, with `tests/floods.rs` grown to 24 (the river rising, the flood at its month and its cells, the water going after its months, the damage and none while risks are off, the crops and a farmstead's harvest lost and the fields sown again, silt, a severe and a gentle flood, faith's flood events, nothing placed under water, a flood now and the water going, floods turned off, the Flood lens, a flood under way round-tripping a save); `tests/faith.rs`' availability test checks the floods and the reservoirs.
    - Clippy on desktop and web, nightly rustfmt; ui-art's `--check` (747 sprites) and unit tests, the tile generator's and the recolour tool's checks.
    - All 27 CI scenarios in release, the new `floods.jsonl` among them (added to CI's list); `farms.jsonl` now expects a Harvest message.
    - `determinism.jsonl` twice: the same output and save, checksum still `2868266e0be074da` (its game has no floods); record and replay of `ui_build` and `determinism` matched, the replayed save the same as the runs'.
    - `--smoke-test-saves` loads every local save (no save change: the flood under way was already in v11's state).
    - Screenshots (`out/floods/`): the Flood lens before the flood, the flood with its messages, a flooded cell's panel.

### Phase 3: Dykes and breaches (medium to large)

- [ ] The dyke tile def (`blocks_floods`, `path_kind: Road`), its 16 pieces from the Blender pipeline, placement as a segment, clearing.
- [ ] The fill stops at dykes; wear, the builders' reset; breaches, their rubble and the fill through them.
- [ ] `MissionGoal::FloodsSurvived`; the debrief's and goals panel's line.
- [ ] The Flood lens's dykes; the terrain inspector's dyke line; the City book's warnings.
- [ ] Tests; `floods.jsonl`'s dyke and breach.

### Phase 4: Fertility and sluice gates (medium)

- [ ] Fertility kept per cell: its start, silt, depletion, its share of the harvest.
- [ ] The sluice gate: building kind, config, Blender art, placement beside a dyke, its reach, the dry-season crop, controlled silt.
- [ ] The Fertility lens and its glyph; the field's land and the sluice gate's inspector lines.
- [ ] Tests; `floods.jsonl`'s irrigated and fallow fields.

### Phase 5: Documentation (small)

- [ ] AGENTS.md (Farms and Floods entries under Runtime architecture, the area tool, the dyke and sluice gate, `farms.jsonl`, `floods.jsonl`, `farms` and `floods` in their lists), the generator's README (`blocks_floods`), the roadmap's M7 row and save-version log, known-issues for what's left, the terrain-height plan's Stage 6 hook.

## Risks

- **Food comes in lumps.** A field gives one or two harvests a year instead of a steady trickle, so the granaries must hold a season's food and a city can starve between harvests. That's the game the design asks for (Pharaoh's), but it needs room in the granaries and a warning before they run low (the City book's food card counts months). M13 balances the harvest against what the houses eat. Missions 1 to 3 farm this way too ([Q7](#open-decisions)): their goals are small, but M13 must check them.
- **Fields' bookkeeping.** Ownership has edges: undo and redo of an area or a farmstead, a farmstead that burns down, two farmsteads' reaches overlapping, a field cleared mid-crop. The farmstead owns the truth (its list of cells), the tiles only show it, and `tests/farms.rs` covers each edge.
- **Many tiles.** A city of 20 farmsteads has 500 field tiles. They're plain objects-layer tiles without game objects, as rubble and trees are, so the world's pools don't grow; the farmsteads update their fields once a month, not every step.
- **The rice farm's users.** Presets, five tests and four scenarios build rice farms; Phase 1 moves them all, and `determinism.jsonl`'s checksum changes. Phases 0 and 1 land together so nothing runs between them without a rice producer.
- **A farmstead's runner rework.** A runner carrying a load at a time is new; it's limited to farmsteads (the production cycle's runners are unchanged), and `goods.jsonl` and `smoke.jsonl` keep passing.
- **Water on flat maps.** Without height, the reach is a distance, so a flood is a band along the river of even width. It reads well on the Flood lens but not as a landscape. T fixes it; until then the missions' maps should put their low land by the river.
- **A terrain tile that's a little raised.** Terrain tiles are drawn flat and back to front; a dyke drawn taller than about 41 pixels would show its join with the tile behind it. Its pieces stay low (an embankment, not a wall), and the Blender recipe checks the height.
- **Dykes as roads.** A dyke's top joins the road network, so houses may be placed along it and a road meeting it joins it. That's the delta's way, but it makes dykes cheap roads: their cost per cell keeps them for the river.
- **Determinism and saves.** The fill, the breaches and the farms draw from the RNG in a fixed order (if at all); `determinism.jsonl` runs with floods off. The fertility grid makes saves of flooded games larger (one byte a cell, encoded), so it's allocated only while floods are on.
- **Builders' reach.** A dyke far from any road walked by a builder wears out. That's the intended tension, but the Flood lens and the warnings must make it plain.
- **The custom game's maps.** The blank map has no water and the debug presets none either, so the switch changes nothing on them; the test maps with water show floods.

## Open decisions

Each has a default; proceed with it unless the user decides otherwise.

- **Q1. What a dyke is.** *Answered 2026-11-17: a road-like terrain tile, walkable on top, its wear kept per cell and reset by a builder walking it* (the default). The other choice is a 1 × 1 building per section, which reuses M6's collapse risk and the risk lens as they are, but blocks walkers, so every crossing needs a gap or a gate, and a dyke far from a road is never visited by a builder.
- **Q2. Where a flood reaches, on flat maps.** *Answered 2026-11-17: a distance from the river's shore, drawn each year from the mission's range, through anything but dykes* (the default). The other choice is a floodplain painted on the map (a terrain kind or a mask) that a flood fills; it's exact but needs an editor tool and every map painted, and still needs a reach for dykes to matter.
- **Q3. Which water floods.** *Default: water bodies of at least `min_source_cells` (30), or the mission's `sources`.*
- **Q4. Damage.** *Default: collapse risk (and disease risk for houses) added through M6, so floods destroy what was already weak.* The other choice is destroying every building the water reaches, which the GDD's "destroy anything unprotected" reads as, but leaves no room between "protected" and "lost".
- **Q5. Walkers and the water.** *Default: walkers ignore it.* Blocking flooded cells would change the path graph twice a year and strand walkers.
- **Q6. Harvests.** *Answered 2026-11-17: discrete harvests:* a crop sown and reaped in one lump, once a year or twice when irrigated (§2). The default was production going on as now, scaled by season, fertility, irrigation and the water.
- **Q7. Harvests in every game.** *Default since 2026-11-18: yes, flood or not.* With the rice farm gone (Q19), a farmstead has only one way to work; without floods its fields are reaped once a year on land of even fertility. This changes missions 1 to 3's food, which M13 balances. (The default before the farm redesign was harvests only while floods are on.)
- **Q8. The sluice gate.** *Answered 2026-11-17: it works by itself while staffed: it waters its reach in the dry season and lets in the silt, not the damage, in a flood* (the default). The other choice is a gate the player opens and closes, with the flood coming through an open one.
- **Q9. Fertility and harvests.** *Default: per cell, 0 to 100; 70 within the usual reach at the start and 40 beyond; a flood's silt adds 30; a harvest takes 10; a field's harvest is 4 rice × (0.5 + fertility / 100) × staffing, after four months' growth (a full farmstead, 24 fields, about 96 a harvest; a 3 × 3 rice farm made about 80 a year).*
- **Q10. Breaches.** *Default: a dyke cell the flood reaches breaches with chance (wear / 100)² × severity, and the water goes on through it.*
- **Q11. The goal.** *Default: `FloodsSurvived { min }`, floods that breached no dyke, counted since the mission's start, not necessarily in a row.* The GDD's `SeasonsSurvived { kind, count }` is the general form; M7 needs the flood's only.
- **Q12. Faith.** *Default: favour moves floods only through the gentle and severe flood events; Kambara's full reservoirs waits for its barays.* A continuous link (the Water Lord's favour scaling every flood) is a knob M13 can add.
- **Q13. Timing.** *Default: one flood a year, in month 7, for two months, with the wet season months 5 to 10.*
- **Q14. Not in M7.** *Default: drought, storms, Kambara's lake pulse and barays, the event popup and pastures for the livestock farm wait for their milestones.*
- **Q15. Units of reach.** *Default: cells along the grid, four neighbours.* Diagonal steps would let water slip between two dyke sections that only touch at a corner.
- **Q16. Building on flooded land.** *Default: refused while it's under water.*
- **Q17. The save.** *Default: M7 takes v11; T's elevation map becomes v12.*
- **Q18. A field's look.** *Default since 2026-11-18: four stage sprites (fallow, sown, growing, ripe) from the Blender pipeline, as the field tile's variations.* Fields are 1 × 1, so stages cost four small renders; before the redesign the default was no stage art.
- **Q19. The farm.** *Answered 2026-11-18: a farmstead that gives the workers, with fields the player lays around it.* The other choice was a much larger farm building (5 × 5 or 6 × 6) with its fields drawn in: little new code, but a rigid square on the river land, a flood or a sluice gate's reach acting on the whole farm at once, and large sprites for every stage.
- **Q20. Fields and walkers.** *Default: walkers go around fields, as around buildings.* Walkable fields (paddy bunds are walked) would let responders cut across but make fields part of the path graph's walkable land.
- **Q21. What a field is.** *Default: an objects-layer tile with no game object, as rubble is, in its own `fields` category; its crop kept by its farmstead.* The other choice, a building per field, would give each one risks, workers and a pool entry it doesn't need.
- **Q22. Fields and their farmstead.** *Default: within 4 cells of its footprint; a field joins the nearest farmstead in reach; fields without one lie fallow until another claims them; 4 fields per worker, the nearest tended first.*
- **Q23. Old saves' rice farms.** *Answered 2026-11-18: turned into livestock farms by the migration*, so the saves keep loading.
