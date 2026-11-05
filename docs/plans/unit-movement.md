# Eight-direction movement for off-road walkers

**Status (2026-11-05): Draft. Not started.** This plan lets walkers that can leave the roads (settlers and animals today; caravans, raiders and militia in Phase 2) move diagonally on the grid as well as straight. That removes the zigzag paths they trace across open ground now. Walkers on roads keep their four directions. It is milestone U of the [roadmap](roadmap.md). The new walk angles need art, and making it in Blender is tried in [placeholder-art.md](placeholder-art.md) Phase 6. It was researched against `d0696fe` on `main`; re-check the file and line references before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Tick the checkboxes and add dated notes as tasks land.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise.
- Follow [AGENTS.md](../../AGENTS.md): keep both targets compiling, run `cargo test --workspace` and the scenarios, and keep the simulation deterministic.

## Goals

1. Units marked for it path through 8 neighbours. Diagonal steps cost √2, and walkers can't cut corners past buildings or other blockers.
2. Walkers that only use roads keep exactly today's paths and look.
3. `UnitDirection` covers 8 directions. A unit without art for a direction falls back to the nearest one it has, so code and art can land separately.
4. Speed stays even: a diagonal step takes √2 times as long as a straight one.
5. It works with [terrain height](terrain-height.md): diagonal steps only between flat cells on the same level.

## What exists today

**Directions:**
- `UnitDirection { Idle, NE, NW, SE, SW }` (`unit/navigation.rs:21`) is saved as part of `UnitNavigation`. On screen, those four are the moves along the grid axes.
- `direction_between(a, b)` (`:61`) picks the larger of the x and y differences. `same_axis` is used by the patrol code (`unit/task/patrol.rs:62`, `:124`).
- `anim_set_for_direction` and `idle_anim_set_for_direction` map directions to the anim set names `walk_ne`/`nw`/`se`/`sw` and `idle_*` (`unit/anim.rs:25–33`). `Unit::update` uses them (`unit/mod.rs:654`, `:668`).

**Art:**
- Only `ne` and `se` are drawn. `nw` and `sw` set `mirror: true`.
- Human walkers have 8 walk frames and 2 idle frames per direction. The dog has 5 walk frames, the buffalo 5 to 8, and the bird 5 (`walk_se` only).

**Pathfinding:**
- `Node::neighbors() -> [Node; 4]` (`pathfind/mod.rs:200`) and `Graph::neighbors(node, kinds) -> ArrayVec<Node, 4>` (`:541`).
- `Graph::neighbors` is used by `find_path` (`:1051`), `find_paths_internal` (`:1212`) and `find_path_to_node_internal` (`:1280`), and also by patrol and context code that looks up neighbouring buildings. Those last two callers stay 4-way.
- `AStarUniformCostHeuristic`: Manhattan estimate, every step costs 1 (`:681`).

**Who walks off-road** (`assets/configs/units/configs.json`, `traversable_node_kinds`):
- Settler: EmptyLand, Road, VacantLot, SettlersSpawnPoint;
- Dog and Buffalo: EmptyLand, Road;
- Bird: everything; it flies.

The other ten units walk on roads only.

**Movement:** `UnitNavigation` blends iso positions between cells over `segment_duration` (`unit/navigation.rs:246`, `unit/mod.rs:401`). Units sort by their starting cell while moving.

## Design

### 1. Directions and animation

- **New directions:** `UnitDirection` gains `N`, `S`, `E` and `W`, the screen directions of diagonal grid moves:

  | Grid step | Screen direction |
  |---|---|
  | (+1, +1) | N (up) |
  | (−1, −1) | S (down) |
  | (+1, −1) | E (right) |
  | (−1, +1) | W (left) |

  The new variants go at the end, so existing saves still load. New saves can't be read by older builds, which is fine; no version bump is needed.
- **`direction_between`:** returns the 8-way direction when the step is diagonal, and keeps today's answer when it isn't.
- **`same_axis` and the `is_*` helpers:** extended so patrol logic treats N and S as both north and south halves. Patrols are road walkers, so in practice nothing changes.
- **New anim set keys:** `walk_n`, `walk_s`, `walk_e`, `walk_w` and the matching `idle_*`. W is normally `mirror: true` of E in `tile_set.json`.
- **Fallback when an anim set is missing:**
  - N falls back to NE or NW, choosing the one on the same side as the unit's last direction (E side means NE);
  - S falls back to SE or SW the same way;
  - E falls back to NE or SE, and W to NW or SW, by the same rule.

  `UnitAnimSets::set_anim` already returns false when a set is missing. The fallback chain goes in `anim_set_for_direction`, and the anim set lookup caches the result per unit def, so there's no repeated hashing per frame.

### 2. Pathfinding

- **`Connectivity { Four, Eight }`**, chosen per search. Unit configs gain `diagonal_movement: bool`, defaulting to false; settler, dog, buffalo and bird set it to true. `UnitNavigation` saves it next to `traversable_node_kinds`.
- **8-way neighbours:** `Node::neighbors8` and `Graph::neighbors_with(node, kinds, connectivity) -> ArrayVec<Node, 8>`. The three path searches take the connectivity. Other callers keep `neighbors`, which stays 4-way.
- **No corner-cutting:** a diagonal step is allowed only when **both** cells next to it, at (x+dx, y) and (x, y+dy), are passable for the unit. That stops walkers slipping between two buildings, and it also keeps draw order right, because nothing tall stands beside a diagonal move.
- **Costs:** costs become whole numbers in tenths, 10 for a straight step and 14 for a diagonal. The estimate becomes octile distance: `10 × max + 4 × min` of the x and y differences. Costs are only compared within one search, so scaling them doesn't change any road-only path. Search limits by distance (`HAS_MAX_DIST`) are scaled to match.
- **Ties:** among equal-cost paths, prefer straight runs (a small extra cost for turning) when the unit uses 8-way movement. Without it, A* can still zigzag between two equally short diagonals.
- **Terrain height** (once T lands): a diagonal step needs the start cell, the end cell and both side cells to be flat and on the same level. This joins the step rule in [terrain-height.md](terrain-height.md) §6.

### 3. Navigation and drawing

- **Even speed:** `segment_duration` is multiplied by √2 for diagonal segments, so walking speed is the same in every direction.
- **Interpolation** between iso positions works as it does now. Sorting by the starting cell is safe because of the corner rule.
- **Moving between cells:** a diagonal step moves the unit's tile between cells that only share a corner (`try_move_tile_with_stacking`). The occupancy code doesn't care whether cells are side by side, but this needs a check in a unit test.

### 4. Art

Each 8-way unit needs `walk_n`, `walk_s` and `walk_e` (with `walk_w` mirrored), plus idle frames for those directions. For the four units that use it today:

| Unit | Frames needed |
|---|---|
| Settler | 3 × (8 walk + 2 idle) |
| Dog | 3 × 5 |
| Buffalo | 3 × 5–8 |
| Bird | `walk_n`, `walk_s` and `walk_e`, 5 frames each |

The fallback covers any gap. Whether this art comes from Blender is decided by the experiment in [placeholder-art.md](placeholder-art.md) Phase 6. If it does, Phase 2's new walkers (militia, raiders, caravans) are made in Blender in all 8 directions from the start.

## Phases

### Phase 1: Directions and animation fallback (small)

- [ ] `UnitDirection` N, S, E and W; `direction_between`, `same_axis` and the `is_*` helpers; `current_anim_name`.
- [ ] Anim set keys and the fallback chain, with a per-def cache.
- [ ] Unit test: each direction maps to the expected anim set for a unit that has 4 directions of art, and for a mock unit that has 8.

Done when: nothing changes in play. Every unit still moves 4-way and looks the same.

### Phase 2: 8-way pathfinding (medium)

- [ ] `Connectivity`, `neighbors8` and `neighbors_with`; the three searches take the connectivity.
- [ ] The corner rule, costs of 10 and 14, the octile estimate, scaled distance limits, and the cost for turning.
- [ ] `diagonal_movement` in unit configs, saved in `UnitNavigation`. Settler, dog, buffalo and bird set it.
- [ ] Tests in `search_graph`:
  - a diagonal path's length across open ground;
  - no corner-cutting between two buildings;
  - a road-only unit's path is identical to before;
  - the same path on repeated runs (determinism).

### Phase 3: Navigation (small)

- [ ] √2 segment duration for diagonals; check moving tiles between cells that only share a corner.
- [ ] `unit_tasks` test: a settler walks to a vacant lot diagonally, in fewer segments than the 4-way path, and arrives.
- [ ] Scenario `offroad.jsonl`: a settler crosses open ground; check the path's segment count and that the settler arrives. Add it to CI. `determinism.jsonl` still passes twice in a row.

### Phase 4: Art (small to medium; depends on placeholder-art Phase 6)

- [ ] N, S and E walk and idle frames for settler, dog, buffalo and bird, from Blender if the experiment succeeds, otherwise by hand. Add them to `tile_set.json` with W mirrored.
- [ ] Review in the game at 1× zoom and at full zoom, next to the hand-made directions.

## Risks

| Risk | Mitigation |
|---|---|
| New directions don't match the hand-made directions in style | Fallback until art lands. If Blender art can't match, regenerate all 8 directions for that unit in Blender (Q5) |
| Search changes alter road walkers' behaviour | Road-only units keep 4-way searches; a test checks identical paths |
| The cost for turning changes which paths are chosen in unexpected ways | Keep it small (1 tenth) and only for 8-way units; covered by tests |
| Terrain height lands first and conflicts | Both edit the same step check; whichever lands second adds its rule there, and both plans point at each other |

## Open decisions

**Q1. How does a unit get 8-way movement?**
*Default: an explicit `diagonal_movement` flag* in the unit config. The alternative is to infer it from `EmptyLand` in its traversable kinds.

**Q2. Is cutting corners never allowed?**
*Default: never;* both side cells must be passable.

**Q3. Do 8-way units move diagonally on roads too?**
*Default: yes, wherever both side cells allow it.* It rarely happens, since roads run along the grid.

**Q4. What do the costs look like?**
*Default: 10 and 14, with an octile estimate,* plus a cost of 1 for turning on 8-way units.

**Q5. How do units get their new direction art?**
*Default: decided by the Blender walker experiment.* If Blender walkers look right, new units get all 8 directions from Blender; for existing hand-made units, only the missing N, S and E are generated, unless the styles clash.
