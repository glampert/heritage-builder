# Eight-direction movement for off-road walkers

**Status (2026-11-27): Draft, brought up to date with the code at `60f5d956` (after terrain height, M6's responders and M10's caravans). Not started.** This plan lets walkers that leave the roads move diagonally on the grid as well as straight, which removes the zigzag paths they trace across open ground now. Today those are settlers, harvesters, fire watchers and medics on a call, caravans, and the dogs and buffalo that wander from houses and farms; Phase 2's raiders and militia will be too. Trade ships sail diagonally over open water as well (Q6). Walkers on roads keep their four directions. The defaults below were accepted on 2026-11-27, with ships sailing diagonally. It is milestone U of the [roadmap](roadmap.md). The new walk angles need art, and making it in Blender is tried in [placeholder-art.md](placeholder-art.md) Phase 6.

## How to resume this work

- Work through the [phases](#phases) in order. Tick the checkboxes and add dated notes as tasks land.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise.
- Follow [AGENTS.md](../../AGENTS.md): keep both targets compiling, run `cargo test --workspace` and the scenarios, and keep the simulation deterministic.

## Goals

1. Searches over open land go through 8 neighbours. A diagonal step costs about √2 a straight one, and walkers can't cut corners past buildings or other blockers.
2. Road-only searches keep exactly today's paths, on flat maps and on raised ground.
3. `UnitDirection` covers 8 directions. A unit without art for a direction falls back to the nearest one it has, so code and art can land separately.
4. Speed stays even: a diagonal step takes √2 times as long as a straight one, and a ramp still takes 1.5 times as long.
5. It follows [terrain height](terrain-height.md): a diagonal step only over flat ground at one level.
6. Trade ships sail through 8 neighbours over open water, without cutting past land, with N, S and E sprites from the Blender generator.
7. Ships keep off the shore: a ship's path never runs through a water cell next to land when a way through open water exists, since a shore cell's sprite is partly land and a ship there looks as if it sails over it (Q8). This is visual only.

## What exists today

**Directions** ([unit/navigation.rs](../../crates/game/src/unit/navigation.rs)):
- `UnitDirection { Idle, NE, NW, SE, SW }` is saved in `UnitNavigation`. On screen, those four are the moves along the grid axes.
- `direction_between(a, b)` picks the larger of the x and y differences. `UnitNavigation::update` sets the direction from it as a segment starts, and `Unit` does too when its tile moves cell (`unit/mod.rs`, by `try_move_tile_with_stacking`).
- `is_north`, `is_south`, `is_east`, `is_west` and `same_axis` serve the patrols' directional bias (`unit/task/patrol.rs`, twice), which also picks the dogs' and buffalo's wandering waypoints.
- `anim_set_for_direction` and `idle_anim_set_for_direction` (in navigation.rs, not anim.rs any more) map directions to `walk_ne`/`nw`/`se`/`sw` and `idle_*` (`UnitAnimSets`, [unit/anim.rs](../../crates/game/src/unit/anim.rs)).

**Art** (`assets/tiles/objects/tile_set.json`):
- Only `ne` and `se` are drawn; `nw` and `sw` copy them with `mirror: true`.
- People (settler, peasant, fire watcher, medic, trader) have 8 walk frames and 2 idle frames per direction. The dog has 5 walk frames, the buffalo 5 (`ne`) and 8 (`se`), and the bird 5 (`walk_se` only).
- The fire watcher and the trader are recoloured hand-made walkers (`crates/tools/unit-recolor`: the guard and the vendor).
- The trade ship is a Blender unit (`catalog/trade.json`, recipe `build_trade_ship` in `hbgen/recipes/trade.py`) in a 2 × 2 frame: `idle_ne` and `idle_se`, two frames each (`bob` 0 and 1), turned by `SHIP_FACINGS` (`se` 0°, `ne` 90°); its `walk_*` sets copy the idle ones, and `nw` and `sw` mirror.

**Pathfinding** ([pathfind/mod.rs](../../crates/game/src/pathfind/mod.rs)):
- `Node::neighbors() -> [Node; 4]` and `Graph::neighbors(node, kinds) -> ArrayVec<Node, 4>`, which since terrain height keeps only neighbours whose ground joins (`ElevationMap::joins`, skipped on a flat map). `Graph::steps(from, to)` is the same check for one step.
- Costs (terrain height's Q7): on a flat map every step costs 1; on a map with raised ground a flat step costs `FLAT_STEP_COST` (2) and a step onto, along or off a ramp `RAMP_STEP_COST` (3). `Graph::cost_scale` (1 or 2) scales the A* estimate (`AStarUniformCostHeuristic`, Manhattan × scale) and the patrols' bias, and `Graph::step_time_scale` makes a ramp step 1.5 times as long to walk.
- The searches that walk the neighbours: `find_paths_internal` (behind `find_path` and `find_paths`), `find_path_to_node_internal` (`find_path_to_node`, `find_paths_to_node`, `find_buildings`), `find_waypoints` (patrols, with a Manhattan `max_distance`) and `find_path_next_to` (responders). Other callers of `Graph::neighbors` look up buildings beside a cell (`sim/context.rs`, `unit/task/patrol.rs`) and stay 4-way.
- Ships don't use the graph's searches: `WaterSearch` ([trade/ships.rs](../../crates/game/src/trade/ships.rs)) is a breadth-first search of its own over open water (`is_sailable`: water nodes that are open water, no canal), through the four `SIDES`, behind `find_water_path` (a ship's way to a berth, its waiting place or its entry), the berth search, `ship_entry` and `reaches_working_dock`. A dock's berth is a cell beside it (`dock_beside`, four neighbours). The ship then follows its path through `UnitNavigation`, as walkers do.

**Who walks off-road.** A unit's `traversable_node_kinds` come from its config (`assets/configs/units/configs.json`) and are saved in `UnitNavigation`, but the tasks that send walkers off the road widen them as they start:
- the settler (`UnitTaskSettler`) and a harvester, the peasant a lumberyard sends out (`UnitTaskHarvest`): `EmptyLand | Road | VacantLot | SettlersSpawnPoint`;
- a fire watcher or medic on a call (`UnitTaskRespond`, `RESPONDER_NODE_KINDS`: the same four kinds);
- the caravan (`UnitTaskTrade`, `TRADER_NODE_KINDS`; its config: `EmptyLand | Road | SettlersSpawnPoint`);
- the dog and the buffalo, by config (`EmptyLand | Road`), on the ambient patrols houses and farms send out;
- the bird (every kind), which doesn't search: it follows a path made by `system/ambient_effects.rs` with one animation set for the whole flight;
- automation's `world.walk` sets the kinds it's given (`automation/commands/world.rs`).

The other units walk on roads only. A unit's config flag, as this plan first proposed, wouldn't reach the peasant, fire watcher or medic, whose configs say `Road`.

**Movement:** `UnitNavigation` blends iso positions between cells over `segment_duration` (1 / `movement_speed`) times the step's time scale; it reports `PathBlocked` when the next cell's kind no longer fits or its ground no longer joins (`Graph::steps`), and the task paths again. On raised maps walkers stand on `surface_height` where they are and sort by their nearest cell.

## Design

### 1. Directions and animation

- **New directions:** `UnitDirection` gains `N`, `S`, `E` and `W`, the screen directions of diagonal grid moves (north is +x, the upper-right edge on screen; east is −y):

  | Grid step | Screen direction |
  |---|---|
  | (+1, +1) | N (up) |
  | (−1, −1) | S (down) |
  | (+1, −1) | E (right) |
  | (−1, +1) | W (left) |

  The new variants go at the end, so existing saves still load. New saves can't be read by older builds, which is fine; no version bump is needed.
- **`direction_between`:** returns the 8-way direction when the step is diagonal (both differences nonzero and equal in size), and keeps today's answer otherwise.
- **`is_*` and `same_axis`:** each helper takes the screen half it names, so N is north (with NE and NW), S south (SE, SW), E east (NE, SE) and W west (NW, SW); a unit test holds the table. `same_axis` then pairs N with NE and NW, and so on. The patrol bias is the only user, and it also steers the dogs' and buffalo's waypoints, so check their wandering still spreads out.
- **New anim set keys:** `walk_n`, `walk_s`, `walk_e`, `walk_w` and the matching `idle_*`, as constants in `UnitAnimSets`. W is normally E with `mirror: true`.
- **Fallback when an anim set is missing:**
  - N falls back to NE or NW, choosing the one on the same side as the unit's last direction (E side means NE);
  - S falls back to SE or SW the same way;
  - E falls back to NE or SE, and W to NW or SW, by the same rule.

  `UnitAnimSets::set_anim` already returns false when a set is missing. The fallback chain goes beside `anim_set_for_direction`, which now takes the previous direction, and whether a unit def has the 8-way sets is cached per def, so there's no repeated hashing per frame.

### 2. Pathfinding

- **Which searches go 8-way:** a search is 8-way when its node kinds include `EmptyLand` (`Connectivity::for_kinds`), so the tasks that already widen a walker's kinds onto open land get diagonals with no new field, config or save data, and every road-only search stays 4-way (Q1). Ships' own water search goes 8-way too (below, Q6).
- **8-way neighbours:** `Node::neighbors8` and `Graph::neighbors_with(node, kinds, connectivity) -> ArrayVec<Node, 8>`. The four searches above take the connectivity from their kinds. Other callers keep `neighbors`, which stays 4-way.
- **No corner-cutting:** a diagonal step is allowed only when **both** cells beside it, at (x+dx, y) and (x, y+dy), are passable for the search's kinds. That stops walkers slipping between two buildings, and it keeps draw order right, because nothing tall stands beside a diagonal move.
- **Heights:** on a map with raised ground a diagonal step also needs its start, its end and both side cells flat (`Shape::Flat`) at one level. A diagonal never climbs: ramps are taken straight (`Graph::steps_diagonally`, beside `steps`).
- **Costs:** one integer scale for every map, so diagonals fit beside terrain height's ramps: a straight flat step costs 10, a diagonal 14 and a ramp step 15 (today's 2 : 3, scaled by 5). `cost_scale` becomes 10 on every map, so the estimate and the patrols' bias scale with it. A road-only search on a flat map sees every step cost the same, as now, and one on raised ground keeps today's 2 : 3, so its paths are unchanged (Goal 2, checked by a test).
- **The estimate:** octile distance for 8-way searches, `10 × max + 4 × min` of the x and y differences; Manhattan × 10 for 4-way ones, as now.
- **Ties:** among equal-cost paths, 8-way searches prefer straight runs: a turn costs 1 more. Without it, A* can zigzag between equally short diagonals.
- **Distance limits:** `find_waypoints`' `max_distance` (the ambient patrols' 40) stays a Manhattan distance in cells. A building search's `radius` (`find_path_next_to`'s responders) stays in cells too; only path costs change scale.
- **Ships (Q6):** `WaterSearch` takes the 8 neighbours with the same corner rule (a diagonal only when both cells beside it are sailable, so a ship never cuts past a point of land or a dock's corner). A breadth-first search counts a diagonal as one step, which would make zigzags and diagonals look equally short, so it becomes a uniform-cost search (Dijkstra's, a binary heap) at the same costs, 10 straight and 14 diagonal, with the cost of 1 for turning; it still visits cells in order of distance, so "the nearest" berth, dock, waiting place and map edge keep their meaning. Every ship search is over level-0 water, so the heights rule doesn't arise. A berth stays a cell beside the dock (four neighbours): a ship moors side on, not across a corner.
- **Keeping off the shore (Q8):** a sailable cell is on the shore when any of its 8 neighbours has a terrain tile that isn't water (its path kind; the rule the shore sprites follow, `water::update_transitions`, so a canal's mouth and the map's edge count as water and a cell with no tile, past the map or in the playable area's margins, isn't land). Shore cells aren't forbidden, since a berth is beside a dock at the shore, a ship starts and ends where it must, and a river one or two cells wide is shore from bank to bank: the search's cost is a pair compared in order, the shore cells entered, then the distance (10 and 14, plus turning), so a path takes no shore cell it can avoid, however much longer the way round, and only the fewest it can't (the last cells into a berth, a narrow channel). "The nearest" berth, waiting place and map edge become the one reached through the fewest shore cells, then the nearest; for berths, which are all on the shore, that's still the nearest unless one's approach hugs the bank. `is_shore` is worked out from the tile map as the search runs (8 lookups a cell), not stored. Flooded land stays land: ships keep to the river.

### 3. Navigation and drawing

- **Even speed:** a diagonal segment's time is × √2 (`Graph::step_time_scale` gains the case, told from the two cells, so it covers ships' paths too), so walking and sailing speed is the same in every direction.
- **Interpolation** between iso positions works as it does now. On raised maps a diagonal stays on one level, so `surface_height` doesn't change along it.
- **Moving between cells:** a diagonal step moves the unit's tile between cells that share only a corner (`try_move_tile_with_stacking`). The occupancy code doesn't care whether cells are side by side, but a unit test checks it.
- **`PathBlocked`:** `UnitNavigation::update` checks a diagonal step with the corner rule too, so a building placed beside a walker's diagonal makes it path again.
- **Sorting:** a walker sorts by its starting cell while moving (by its nearest cell on raised maps); the corner rule keeps that safe.

### 4. Art

Each 8-way unit needs `walk_n`, `walk_s` and `walk_e` (with `walk_w` mirrored), plus idle frames for those directions:

| Unit | Frames needed | Today's art |
|---|---|---|
| Settler | 3 × (8 walk + 2 idle) | Hand-made |
| Peasant (harvesting) | 3 × (8 + 2) | Hand-made |
| Fire watcher | 3 × (8 + 2) | The guard recoloured: recolour the guard's new directions the same way |
| Medic | 3 × (8 + 2) | Hand-made |
| Trader (caravan) | 3 × (8 + 2) | The vendor recoloured: recolour the vendor's new directions |
| Dog | 3 × (5 + 2) | Hand-made |
| Buffalo | 3 × (5–8 + 2) | Hand-made |
| Trade ship | 3 × 2 idle frames (`idle_n`, `idle_s`, `idle_e`; `walk_*` copy them, `w` mirrors `e`) | Blender: `SHIP_FACINGS` gains `e` (45°), `n` (135°) and `s` (−45°), and `catalog/trade.json` the three variations |

Making the fire watcher's and the trader's directions means making the guard's and the vendor's first, which aren't off-road walkers themselves. The ship's sprites don't wait for the walker experiment: its recipe exists, so they're a catalog change and a render. Turned 45°, its hull runs across the screen and is about a quarter longer on screen than along a grid axis; check it still fits the 2 × 2 frame (or give the `e` facing a wider frame). The bird is left out (Q7). The fallback covers any gap. Whether this art comes from Blender is decided by the experiment in [placeholder-art.md](placeholder-art.md) Phase 6. If it does, Phase 2's new walkers (raiders, militia) are made in Blender in all 8 directions from the start.

## Phases

### Phase 1: Directions and animation fallback (small)

- [ ] `UnitDirection` N, S, E and W; `direction_between`, `same_axis` and the `is_*` helpers.
- [ ] Anim set keys and the fallback chain, with a per-def cache.
- [ ] Unit tests: each direction maps to the expected anim set for a unit that has 4 directions of art, and for a mock unit that has 8; the `is_*` table.

Done when: nothing changes in play. Every unit still moves 4-way and looks the same.

### Phase 2: 8-way pathfinding (medium)

- [ ] `Connectivity` (from the search's kinds), `neighbors8` and `neighbors_with`; the four searches take the connectivity.
- [ ] The corner rule, the heights rule, costs of 10, 14 and 15 with `cost_scale` 10 on every map, the octile estimate and the cost for turning.
- [ ] Tests in `search_graph`:
  - a diagonal path's length across open ground;
  - no corner-cutting between two buildings;
  - no diagonal onto a ramp, across a cliff's corner or between levels;
  - a road-only path is identical to before, on a flat map and on raised ground;
  - the same path on repeated runs (determinism).
- [ ] `WaterSearch` through 8 neighbours with the corner rule, as a uniform-cost search at 10 and 14 with the turning cost. Tests in `trade`: a ship's path across open water takes diagonals, never cuts past a point of land or a dock, and still finds the nearest berth, waiting place and map edge.
- [ ] Ships keeping off the shore: the pair cost (shore cells entered, then distance) and `is_shore`. Tests in `trade`: on a wide river a ship's path keeps to the cells with no land beside them, entering the shore only for its last cells into a berth; it still sails a river two cells wide, and goes round a headland's shore rather than along it.

### Phase 3: Navigation (small)

- [ ] √2 segment time for diagonals in `step_time_scale`; `PathBlocked` with the corner rule; a unit's tile moving between cells that only share a corner.
- [ ] `unit_tasks` tests: a settler walks to a vacant lot diagonally, in fewer segments than the 4-way path, and arrives; a fire watcher reaches a fire across open ground the same way.
- [ ] Scenarios: a new `offroad.jsonl` (a settler and a caravan cross open ground; the path's segment count and their arrival), added to CI. Check the scenarios that count an off-road walker's cells or time: `elevation.jsonl`'s `world.walk` (13 cells round the cliff, which may shorten), `risks.jsonl`'s fire watcher off the road, `trade.jsonl`'s caravan and its ship (it may moor sooner or later: its 4-cell river's middle two rows are clear of the banks, so it sails down them; screenshot it under way) and `smoke.jsonl`'s settlers. `determinism.jsonl` still passes twice in a row, and the recorded `ui_build` and `determinism` runs replay (their checksums change once, since walkers move differently).

### Phase 4: Art (small to medium; the walkers depend on placeholder-art Phase 6)

- [ ] The trade ship's `n`, `s` and `e` facings from its Blender recipe (`SHIP_FACINGS`, `catalog/trade.json`, `tilegen.sh trade_ship`), `w` mirrored, checked in its 2 × 2 frame. This doesn't wait for the experiment and can land with Phase 2.
- [ ] N, S and E walk and idle frames for the settler, peasant, medic, dog and buffalo, and for the guard and the vendor, recoloured into the fire watcher and the trader, from Blender if the experiment succeeds, otherwise by hand. Add them to `tile_set.json` with W mirrored.
- [ ] Review in the game at 1× zoom and at full zoom, next to the hand-made directions.

## Risks

| Risk | Mitigation |
|---|---|
| New directions don't match the hand-made directions in style | Fallback until art lands. If Blender art can't match, regenerate all 8 directions for that unit in Blender (Q5) |
| Search changes alter road walkers' behaviour | Road-only searches stay 4-way, and the cost scale keeps their step costs in the same ratio; a test checks identical paths on flat and raised maps |
| A task that widens a walker's kinds onto open land for another reason gets diagonals it shouldn't | Every such task is listed above; each one off the road is meant to cross open land |
| The cost for turning changes which paths are chosen in unexpected ways | Keep it small (1 in 10) and only for 8-way searches; covered by tests |
| A diagonal across a raised map's corner draws wrongly | Diagonals only over flat ground at one level, both side cells included |
| Scenarios that count an off-road walker's steps break | Phase 3 lists them; their numbers are updated by hand and checked |
| A ship turned 45° overflows its 2 × 2 frame | Check the render against the frame; give the `e` facing a wider frame if needed |
| Keeping off the shore sends a ship far round, or to a farther dock | Only when a nearer way would hug the bank; the shore count wins by design (Q8). If a detour looks wrong in play, cap it (a shore cell worth N straight steps) instead |
| Ships' uniform-cost search is slower than the breadth-first one | It runs a few times per ship trip; a binary heap over the map's water cells is cheap at these sizes |

## Open decisions

**Q1. What makes a search 8-way?**
*Default (changed 2026-11-27): its node kinds include `EmptyLand`.* The tasks that send walkers off the road already widen their kinds, so this reaches the harvesters and responders, whose configs say `Road`. The first draft's `diagonal_movement` flag on the unit config wouldn't, and it would have needed saving in `UnitNavigation`.

**Q2. Is cutting corners never allowed?**
*Default: never;* both side cells must be passable (and, on raised ground, flat at the step's level).

**Q3. Do 8-way walkers move diagonally on roads too?**
*Default: yes, wherever both side cells allow it.* It rarely happens, since roads run along the grid.

**Q4. What do the costs look like?**
*Default (changed 2026-11-27): 10 straight, 14 diagonal and 15 for a ramp step on every map,* with an octile estimate for 8-way searches and a cost of 1 for turning. This replaces terrain height's 1 (flat map) and 2 : 3 (raised ground) with one scale.

**Q5. How do units get their new direction art?**
*Default: decided by the Blender walker experiment.* If Blender walkers look right, new units get all 8 directions from Blender; for existing hand-made units, only the missing N, S and E are generated, unless the styles clash. The recoloured walkers follow their sources.

**Q6. Do ships sail diagonally?**
**Decided (2026-11-27): yes.** Their water search goes 8-way with the corner rule and becomes a uniform-cost search, and the Blender generator renders the ship's N, S and E facings (W mirrored).

**Q7. Does the bird fly diagonally?**
*Default: no change.* It doesn't search: its flight is a path made for it, with one animation set for the whole flight, so it gains nothing from the searches and keeps its art.

**Q8. Do ships sail beside the shore?**
**Decided (2026-11-27): not when they can avoid it,** a visual fix: shore cells (a land neighbour among their 8) are drawn partly land, so a ship there seems to sail over it. They aren't forbidden, since berths are on the shore and narrow rivers are shore throughout; the search enters as few as it can.
