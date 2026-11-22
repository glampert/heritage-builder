# Terrain height: analysis and plan

**Status (2026-11-22): Refreshed plan. Not started.** This plan adds terrain elevation (hills, plateaus, cliffs and ramps) for highland maps such as Đại Long's northern mountains and valleys. It compares the options, recommends one, and breaks it down into stages. It is milestone T of the [roadmap](roadmap.md), to be done before M13 builds the mission maps.

- It was first written on 2026-11-05 against `d0696fe`, before M3 to M10.
- It was refreshed on 2026-11-22 against `d6801146` (M10 done, save version 14):
  - the code's current state;
  - the systems added since, which all work on the grid (§10);
  - what Zeus does, from the user's reverse-engineering notes (§11);
  - the open decisions that follow from them (Q9 to Q15, and Q1 reopened).
- References name functions rather than lines, which move. Re-check them before relying on them.

## How to resume this work

- Settle the [open decisions](#open-decisions) marked **reopened** or **new** with the user first. The others each have a default: proceed with the default unless the user has decided otherwise.
- Then start with **Stage 0 (the spike)**. It decides whether the approach holds up before anything is committed to the save format. It runs on a throwaway branch off `claude` that is never merged.
- Tick the checkboxes and add dated notes under each stage as tasks land.
- Follow [AGENTS.md](../../AGENTS.md):
  - keep both targets compiling;
  - route edits through `TilePlacement` and the `TileMap` setters;
  - follow the Saves workflow, since Stage 1 bumps the save version to 15.

## Goals

1. Maps can have **raised terrain**: plateaus at a few discrete levels, cliffs between levels that can't be crossed, and ramps that can.
2. **Height matters in play:**
   - cliffs are barriers;
   - ramps are chokepoints for attackers;
   - high ground escapes floods;
   - towers on high ground reach further.
3. **Map designers author height in the developer editor.** Players don't sculpt terrain (Q6).
4. **Flat maps don't change:** they look, perform and save the same as now apart from the format bump, and existing missions and scenarios keep working.
5. **Art comes from the Blender pipeline:** cliff faces and ramps ([placeholder-art.md](placeholder-art.md), Phase 5). Raised cells reuse the existing terrain sprites for their top faces.

**Not a goal:** matching Zeus. The model takes Zeus as its inspiration (raised plateaus, cliff faces, access by ramps), and §11 records what we borrow from how it actually works, but the rules are our own.

## What exists today (and why height touches so much)

Nothing in the code has a notion of height. Every screen position comes from the cell's (x, y) alone.

**Projection** (`common::coords::cell_to_iso`):
```rust
iso_x = (cell.x - cell.y) *  32;
iso_y = (cell.x + cell.y) * -16;   // larger x+y is higher on screen
```
- One step along x or y moves 16 logical px vertically and 32 px sideways.
- Moving one cell along both, (x+1, y+1), moves exactly 32 px straight up the screen. This matters for Q1.
- `WorldToScreenTransform` is only a 2D scale and offset.

**Tiles:**
- `TerrainTile` and `ObjectTile` cache `iso_coords_f32`, and that value is **serialized** (`tile/mod.rs`, `TerrainTile`).
- Terrain defs must be exactly `BASE_TILE_SIZE`, 64 × 32 (`tile/sets.rs`). Terrain sprites are drawn at 65 × 36: the diamond plus about 4 px of side face.
- A def's `iso_offset` already lifts sprites: the dyke's pieces carry `y: -3`, and rubble's `y: 8` (terrain-tiles.md).

**Rendering** (`tile/rendering.rs`, `TileMapRenderer::draw_map`):
- `draw_terrain_layer` draws **all terrain first, unsorted**. It also draws terrain's extra layers: the flooded wash, the dry-canal wash, the canal, road and dyke previews.
- Then the grid and the terrain search-graph debug view.
- Then `draw_objects_layer`: objects and units in one list sorted by `TileDrawListEntry::depth_sort_key` (`TileDrawListEntry::compare`), the screen Y of the footprint's centre, with overrides for topmost and bottommost. Scaffolds, renovation and festival overlays and the flames are drawn with their buildings.
- The engine is pure 2D, with **no depth buffer** (neither the OpenGL nor the Wgpu passes have one). It's a painter's algorithm, in submission order.

**Culling and camera:**
- `calc_visible_cells_range` (`camera.rs`) goes through `selection::bounds`: the screen corners go to flat iso, then to a cell bounding box padded by one tile.
- `calc_map_bounds` limits the camera, and `teleport` and `teleport_iso` move it.
- The playable area is the rectangle inside the map diamond (`TileMapPlayableArea`).

**Picking:**
- `TileMap::find_exact_cell_for_point` projects to a flat cell, then tests that cell's diamond and its 8 neighbours'.
- `topmost_tile_at_cursor` tests base-cell diamonds.
- The selection, the area-drag tool (fields, the clear tool) and automation's map-cell targets (`input.click cell=`, `ui.drag`) all build on these.

**Pathfinding:**
- `Graph` is one `NodeKind` per cell. Passability is decided per node in `Graph::neighbors`, with no data per edge.
- `Heuristic::movement_cost(graph, from, to)` is a ready hook for slope costs.
- `update_search_graph!` keeps the graph in step with tile edits, in one place.
- Ships have their own breadth-first search over water (`trade::ships::find_water_path`).

**Units:** `Unit::update_navigation` blends flat iso positions between cells. While moving, they sort by their starting cell.

**Placement:** there are no flatness rules. `placement::is_placement_on_terrain_valid` and `try_place_tile_in_layer` decide validity, and every player and editor edit goes through `TilePlacement` (`menu/mod.rs`): `place`, `place_variation`, `place_area`, `place_road_segment`, `clear`. Undo stores def, flags and variation per cell.

**Minimap:**
- `tile/minimap/model.rs` keeps a grid of terrain and cover per cell.
- `crates/minimap_bake` shades hills from a **seeded noise field standing in for elevation** (`BakeStyle::hill_strength`, `elevation_cells`; `elevation_pass`). Real heights can replace the noise.

**Saves:**
- Version 14.
- Terrain takes about 320 bytes per cell: `test_map_128x128` is 13.7 MB, and `test_map_256x256` 53 MB.
- New serialized fields need a version bump and a migration script, with no `serde(default)` shims.

**Things that help:**
- objects already sort by a per-tile screen-Y key;
- positions are cached per tile;
- graph updates go through one macro;
- `movement_cost` exists;
- every edit goes through `TilePlacement`;
- the flood's reach is a fill with a "blocks" predicate (§10);
- the minimap baker already takes an elevation field.

**Things that hurt:**
- the unsorted terrain pass, which now also draws several washes and previews;
- no depth buffer;
- picking within one cell;
- passability per node only;
- terrain sizes locked;
- cached iso coordinates stored in saves;
- several systems that measure reach along the grid (§10).

## Options

| | A. Decorative mountains | B. Discrete levels, cliffs and ramps | C. Per-corner heightfield (SimCity 2000 / OpenTTD) | D. A real 3D terrain mesh |
|---|---|---|---|---|
| **What it is** | Impassable mountain and rock props on flat land, as Pharaoh does | Each cell has a level from 0 to 7 and is flat or a ramp. Cliffs appear wherever neighbouring levels differ | Each cell corner has a height, giving about 19 slope shapes per cell | Render the terrain as a 3D mesh, with sprites on top |
| **Building on high ground** | No | Yes, on plateaus | Yes, with foundations on slopes | Yes |
| **Gameplay value** | Scenery and barriers | Plateaus, chokepoints, flood refuges, tower range | Same as B, plus smooth hills | Same as C |
| **Art cost** | A few large props | Cliff faces plus 4 ramp directions per terrain kind. **Top faces reuse today's sprites** | Every terrain kind and transition × about 19 slopes; roads, fields and buildings on slopes | A new terrain art style and texturing |
| **Engine cost** | None | Medium: projection, a second sorted pass, picking, graph edges, editor | Large: B plus slope sprites everywhere, placement on slopes, and water on slopes | Very large: depth buffer, a new terrain renderer, sorting sprites against a mesh |
| **Fits the look** | Yes | Yes. Zeus uses this kind of terrain (§11) | It looks like a different genre | No |

**Recommendation: B.**
- It delivers what the design asks for (mountains and valleys, plateaus to build on, chokepoints, high ground in floods) at a fraction of C's art cost: cliffs are built from pieces and top faces reuse the existing sprites.
- It changes nothing on flat maps.
- Zeus confirms the shape of it: one height per cell, cliffs worked out from the neighbours, never stored, and two drawing passes (§11).
- Option A is still useful alongside B, for mountains you can't enter at the map's edges.

## Design for option B

### 1. The model

- **Each cell** has a level `h` from 0 to 7, which fits in 3 bits, and a shape: `Flat` or `Ramp(dir)`, where `dir` is the uphill direction (N, E, S or W).
- **A ramp at level `L`** joins its downhill neighbour at `L` to its uphill neighbour at `L + 1`. Its top face slopes by one level. A longer climb is a chain of ramps facing the same way.
- **Ramps are stored, not worked out** (Q12):
  - Zeus works its slopes out from the heights, with an ordered table of 152 neighbourhood patterns (§11).
  - We store them, because a ramp is a gameplay decision (a chokepoint, a road's way up) that the designer places.
  - The editor can still suggest them.
- **Cliffs aren't stored.** A cliff exists wherever two flat cells side by side have different levels, or a ramp's side edge meets a different level. They're worked out from the heights when needed, as in Zeus.
- **Height of one level:** `LEVEL_STEP_PX`, one constant (Q1, reopened). The plan's default was 16 px; §11 argues for 32 px, one cell straight up the screen, as Zeus does. The rest of this design is written for 32 px and notes where 16 px differs.
  - At **32 px**, a cell at level `h` appears exactly where the flat cell `(x + h, y + h)` would. Picking becomes a shift by whole cells (§4), and a raised top covers exactly one flat diamond behind it.
  - At **16 px**, a raised cell's diamond lands on the corner shared by four flat diamonds. It still sorts correctly, but picking needs the inside-diamond test at every level.
  - In the Blender camera, 1 world unit of height projects to 64/√2 × sin 60° ≈ 39.2 px. One level is 32 / 39.2 ≈ **0.816 Blender units** (0.408 at 16 px), which the generator reads from one shared constant.
- **Water** stays at level 0 (Q3). A plateau beside water drops to the water through a cliff face; the shore logic keeps treating any land as land.
- **Neighbour differences:** Zeus keeps neighbours within 2 levels of each other (§11). We allow any drop, since stacked faces draw any height, but the editor's smooth brush keeps differences within `max_cliff_levels` (Q13).

### 2. Data and saves

- `TileMap` gains `elevation: ElevationMap { size, cells: Vec<u8> }`:
  - bits 0–2 hold the level;
  - bits 3–5 hold the shape (0 means flat; 1–4 mean a ramp facing N, E, S or W);
  - bits 6–7 are reserved.

  It's saved as runs, as the floods' fertility is (`CityFloods`). An all-flat map is one run; a 128 × 128 map with hills is a few KB of JSON.
- It sits alongside the layers rather than in each `Tile`, so `Tile`'s layout and size stay the same and edits are one-byte writes.
- **Runtime only** (`serde(skip)`, rebuilt in `post_load` and on height edits):
  - a cliff cache, holding the drop on each cell's two front edges and which cells' ground is hidden (§3);
  - the projected-height lookup for picking (§4).
- **Iso positions:** they already sit on tiles and are recomputed on load from the cell and its height. The saved `iso_coords_f32` can't be trusted once a cell's height has changed.
  - Since v15 bumps the format anyway, make the cached positions `serde(skip)` and recompute them in `post_load` for every tile (Q14).
  - Zeus saves its derived image IDs, and its maps then carry art chosen for another camera view (§11). Saving only what's authoritative avoids that kind of mismatch.
  - Dropping them also shrinks every save by a good share of its terrain bytes.
- **Validation on load:** a ramp whose uphill neighbour isn't one level up, or a level above 7, is logged on a `terrain` channel and drawn flagged in the editor's overlay. The map still loads. Zeus keeps a "Found A Bad Elevation" message for the same job.
- **Saves:** bump to v15. The migration script:
  - adds an all-flat `elevation` to every save, so every existing map stays flat;
  - drops the cached positions if Q14 says so.

  Regenerate `saves/sample_saves.zip` and run `--smoke-test-saves`.

### 3. Projection and draw order

**Projection:**
- `cell_to_iso_h(cell, h) = cell_to_iso(cell) + (0, -h × LEVEL_STEP_PX)`. Ramps use `h + 0.5` at their centre.
- Every cached position uses the cell's height: `TerrainTile::new`, `calc_object_iso_coords` and `calc_unit_iso_coords` (`tile/mod.rs`), and the dyke's and rubble's `iso_offset` on top.
- Units blend height as well as (x, y) while moving between cells, so walking up a ramp looks continuous.

**Draw order:** the painter's algorithm stays, with no depth buffer. It's split in two passes, as Zeus does (§11):

1. **Pass 1, unchanged:** every level-0 terrain cell, drawn first and unsorted, as today, with its washes. A level-0 top face can never cover anything, because everything drawn after it is on top of it or in front of it. Flat maps cost exactly what they do today.
2. **Pass 2, sorted:** objects as today, **plus every raised terrain cell (h > 0, or a ramp)**, sorted by the existing screen-Y key.
   - A raised cell's key is the screen Y of its base diamond.
   - Ties put terrain first, so a building on a plateau draws after the ground under it.
   - A raised cell draws its **cliff faces first** (§5), then its top-face sprite, raised, then its washes (flooded, dry canal) and previews.

**Hidden ground:** pass 1 skips a level-0 cell whose diamond a raised neighbour in front covers whole, and pass 2 skips cliff faces that nothing can see. Zeus does the same from each cell's back and front heights (§11). It's an optimisation, not needed for correctness.
- At 32 px, the cell `(x+1, y+1)` behind a raised `(x, y)` is covered exactly.
- At 16 px, no flat diamond is covered whole by one raised cell, so the test needs two.

**Why this order is right:**
- Anything behind a raised cell (an object or lower ground) has a larger key, so it's drawn before the cell and gets covered by it.
- Anything in front has a smaller key, so it's drawn after.
- A cliff face's bottom edge is the cell's own front edges at ground level, which it shares with its front neighbours' top faces. So pass 1 never has to draw over a cliff, whatever `LEVEL_STEP_PX` is.

**Known artifact:** sprites spread over several bands (large buildings, tall cliffs beside a building's side) can be sorted wrongly where a building stands right beside a higher cell at the same depth. This is the classic isometric sorting limit, which the reference games share. A placement rule removes the worst cases: a building can't stand beside a cell more than one level higher on its left or right side (Q4).

**Cost:**
- Pass 2 grows by the number of raised cells on screen, a few thousand at most. The sort already exists.
- Mixing the terrain and object atlases in one sorted list breaks some sprite batches, so expect a few hundred more draw calls on mountain maps. The spike measures this.
- If it's too slow, the cliff, ramp and raised top sprites go into the objects atlas (a raised cell's top reusing a copy of its terrain sprite), so pass 2 stays on one texture set.

### 4. Culling, camera and picking

- **Culling:** `calc_visible_cells_range` pads the bottom of the screen rectangle by `max_level × LEVEL_STEP_PX`, because a raised cell that sits below the screen in flat terms can rise into view. `update_anims` uses the same range.
- **Camera:** `calc_map_bounds` and the camera constraints extend the top edge by `max_level × LEVEL_STEP_PX`. `teleport`, `teleport_iso` and automation's cell-to-screen conversion use the height.
- **Picking:** a **projected-height lookup**, as in Zeus (§11), built with the cliff cache.
  1. For each flat screen cell `s`, it holds the highest level of any cell drawn over it: a cell `(x, y)` at level `h` is drawn over `(x + h, y + h)` at 32 px. Scan cells front to back and keep the maximum, as Zeus does.
  2. To pick: take the flat cell `s` under the point and read its lookup `h`. The answer is `s − (h, h)` if that cell's level is `h`; otherwise go down one level and try again. A miss is rare: it happens only near a cliff's edge.
  3. At 16 px the shift is half a cell, so step 2 becomes the plan's earlier scan: for `h` from `max_level` down to 0, test the raised diamond of the flat cell under `p` moved down by `h × 16`.
  4. A point on a cliff face maps to the cliff's top cell and is marked "face". The cell is shown, but nothing can be placed there.
- **Functions that change:** `find_exact_cell_for_point`, `topmost_tile_at_cursor`, `is_screen_point_inside_base_cell` and `cell_to_screen_diamond_points`. So do automation's map-cell gestures, which aim at a cell's screen point.
- **Range selection:** the area-drag tool (fields, the clear tool) keeps working in flat cells under the dragged rectangle. Over a cliff it selects cells on both levels; placement then refuses those that don't fit (§7).
- **Overlays** draw raised diamonds:
  - the grid;
  - the search-graph debug view;
  - the hover highlight;
  - the road, canal, dyke and field previews;
  - the tile bounds.

### 5. Cliff and ramp art

- Only a cell's **two front edges** can show a cliff face, because the back edges face away from the camera.
- For each front edge, `drop = h(cell) − h(front neighbour)`, or the cell's own level at the map's edge. If the drop is above 0, the renderer draws `drop` copies of a **one-level face sprite** for that edge, stacked from the top down.
  - This needs only 2 sprites per material (left and right), plus a few variations to break up repetition.
  - The material follows the top cell's terrain: grassy rock or bare rock (Q5).
  - A variation is picked by a hash of the cell (`common::hash::splitmix64`), never the RNG and never saved, as the risks' jitter is. Zeus keeps a random byte per cell for the same job.
- **A ramp** is one sloped sprite per direction and terrain kind: grass, dirt, dirt road and paved road, giving 4 directions × 4 kinds. A ramp's side edges also show a triangular cliff piece where they meet lower ground: one sprite per side.
- These are runtime decorations computed from the heights, **not** tiles in the map, so they add nothing to saves and need no variation bookkeeping.
- **What Zeus does differently:** its cliff art is whole shaped tiles picked from the 8-neighbour pattern, some joined into 2 × 2 footprints, with each tile drawn as a ground base and a separate raised top (§11).
  - That looks better at a cliff's corners and ends than stacked edge pieces.
  - It costs a pattern table and art per pattern.
  - Start with stacked faces. If the spike's screenshots show seams at corners, add corner and end pieces picked from the neighbours, Zeus's way, one small table at a time (Q15).
- All the pieces come from the Blender pipeline (`cliff_face`, `ramp`), with one level's height read from the shared constant.

### 6. Pathfinding and units

- **The graph** stores a copy of each cell's height and shape, updated wherever `update_search_graph!` runs and on height edits.
- **`Graph::neighbors`** adds an edge test, worked out on the fly with no data per edge. A step from `a` to `b` is allowed if any of these holds:
  - both are flat at the same level;
  - one is a ramp and the other is its uphill neighbour at `L + 1`, or its downhill neighbour at `L`;
  - both are ramps facing the same way, one level apart, along that direction.
- **Every land walker follows it for free:**
  - road walkers and patrols;
  - the responders who leave the road (fire watchers, medics; `RESPONDER_NODE_KINDS`);
  - caravans;
  - settlers;
  - builders walking dykes.

  All of them path through `Graph`.
- **Ships** search water only (`find_water_path`), and water is level 0, so they're unchanged.
- **With 8-way movement** ([unit-movement.md](unit-movement.md)), a diagonal step also needs both side cells to be flat and on the same level as the two cells it joins. Ramps are only climbed straight. Doing T before U lets U write this rule once.
- **Costs:** `Heuristic::movement_cost` adds a small cost for ramps (Q7). `UnitNavigation::update`'s check on the next node also checks the edge, so a unit whose path crosses a new cliff stops and repaths.
- **Drawing:** units draw at their blended height. Their sort key stays the starting cell's, which is correct on ramps because a ramp's band is the lower cell's.

### 7. Placement rules

| Thing | Rule |
|---|---|
| Buildings, vacant lots | Every footprint cell flat and at the same level. The road link must be on the same level, or on a ramp that leads to it |
| Farmsteads and rice fields | Flat. A field must be on its farmstead's level, and `farms::farmstead_for_field`'s reach doesn't cross a cliff (Q9) |
| Roads | Flat cells, or ramps running along the ramp's direction. Junctions (`road::update_junctions`) ignore neighbours across a cliff, and a ramp road connects uphill and downhill only. This needs 4 ramp road sprites |
| Dykes | Flat, and joined only to dykes on the same level (`road::dyke_junction_mask`). No dyke ramps (Q10) |
| Canals | Level 0 only, like water (Q10): a canal is water to the shore code and the flood |
| Sluice gates | Flat; their dyke on the same level |
| Docks, wharves | Level 0, on the shore, as now |
| Water | Level 0 only |
| Trees and rocks, rubble | Flat cells at any level |
| Walls, towers | Flat; a tower on high ground gets bonus range (gameplay hook) |
| Changing a height under something | Not allowed for players (Q6). In the editor, clearing objects first is required |

These go into:
- `placement::is_placement_on_terrain_valid` and `try_place_tile_in_layer`;
- `house_upgrade::can_expand_into_cell`;
- `road::can_place_road` and `build_segment`, which also draw canals and dykes;
- `TilePlacement::place_area`, for fields.

Each gets a new `TilePlacementErrReason`.

### 8. The editor, tools and automation

- **The developer palette** gets a "Height" section:
  - **raise** and **lower** brushes (1 × 1, 3 × 3, 5 × 5);
  - a **terrace** brush, which sets the level picked only on cells one level from it, as Zeus's brushes do (§11), so painting a plateau's edge doesn't disturb the cells around it;
  - a **smooth** brush, which brings neighbours within `max_cliff_levels` (Q13), as Zeus's smoothing does;
  - setting a ramp by dragging in its uphill direction;
  - an overlay showing levels, with ramps that don't connect to the right levels flagged in red, and cliffs taller than `max_cliff_levels`.
- **Edits** go through a new `TilePlacement::set_elevation`, which:
  - updates the graph, the cliff cache, the projected-height lookup and the minimap;
  - calls `irrigation::network_changed` and the floods' reach refresh;
  - records undo. `TileSavedState` gains the cell's elevation byte.
- **Preset maps** get an optional elevation array (`debug/preset_maps.rs`).
- **Minimap:** the baker's noise elevation (`elevation_pass`) is replaced by the map's levels, smoothed, so hills shade from real heights, with darker cliff edges. The model's grid gains the level per cell. A flat map keeps the noise, so its look doesn't change.
- **Automation:**
  - `terrain.set_elevation cell=[x,y] level=2 [shape=ramp_n]` and `terrain.set_elevation from=[x,y] to=[x,y] level=N`;
  - `terrain.elevation cell=[x,y]`;
  - `inspect.at` and `world.building_at` report the level;
  - automation's cell-to-screen conversion takes height into account, so `input.click cell=` lands on a raised cell.
- **Scenario:** `elevation.jsonl` builds a plateau and a ramp. It checks:
  - pathfinding across the ramp and not across a cliff;
  - the placement rules;
  - that picking a raised cell selects it;
  - that a flood stops at high ground.

### 9. Gameplay hooks

- **Floods** (M7, done): the flood's reach is a breadth-first fill over land from the river (`sim::floods::reach_meeting`, with `reach_fill` and `reach_distances` over it) that never enters a cell its `blocks` predicate refuses (a dyke today, `sim::floods::is_dyke`).
  - Height plugs in there: a cell above the flood level blocks too, so the fill stops at high ground and the dykes it meets are only those that hold water back.
  - `MissionFloodsDef` (`campaign/config.rs`) and `FloodRules` gain `flood_level` (default 0: only level-0 land floods; Q11).
  - `CityFloods::ordinary_reach` (the Flood lens), `crate::floods::come` (the year's flood and the breaches) and the starting fertility (`CityFloods::start`) all call the fill, so they follow without other changes.
  - A severe flood (severity above 1) could raise the level by one; that's a balance question for M13.
- **Irrigation** (M7, done): canals are level 0 (§7), so `farms::irrigation`'s spread from field to field needs one rule: water passes only between neighbours on the same level. Fields on a plateau are dry unless a later feature (a water wheel, Phase 2) lifts water.
- **Reach along the grid** (Q9): several systems measure reach in grid steps without a path search:
  - services' coverage and "in reach" counts (the inspector);
  - a farmstead's field reach;
  - faith's coverage;
  - wells' water access.

  Either they ignore height (cheap, and a well's water reaching across a cliff is odd but harmless), or their fills stop at cliffs. The default is to stop the farmstead's reach and irrigation at cliffs, and leave the rest until playtesting.
- **Defence** (Phase 2): attackers path like walkers, so cliffs funnel them through ramps. Towers on high ground get bonus range.
- **Farming** (later): Đại Long terraced rice fields as a field kind that spans a run of ramps.
- **Desirability** (later): a bonus for houses on high ground.
- **Monuments** (Phase 2): Zeus's special buildings skip ordinary cliff selection and keep their own height (§11). Our monuments will flatten their footprint when placed in the editor, or refuse uneven ground.

### 10. Systems added since the first draft

Between M3 and M10 the game gained systems that work on the grid. Each needs a rule for height; the rules are in §7 and §9. In short:

| System | Where | What height means for it |
|---|---|---|
| Construction sites, renovation, festival dress | `building/construction.rs`, `tile/rendering.rs` | Drawn with their building, so raised with it. No rule |
| Rubble | `tile/rubble.rs` | Any flat level; piles join only on the same level |
| Risks' spread (fire, disease) | `building/risks.rs` | Spreads to neighbours as now; across a cliff is fine (Q9) |
| Responders off the road | `unit/task/respond.rs` | Path through `Graph`, so they take the ramps |
| Farmsteads and fields, the area tool | `farms/`, `TilePlacement::place_area` | Flat, the farmstead's level; reach stops at cliffs |
| Floods, dykes, the Flood lens | `floods/`, `sim/floods.rs` | The fill stops above the flood level; dykes flat |
| Canals, sluice gates, irrigation | `farms/irrigation.rs` | Canals level 0; water passes only on one level |
| Fertility and the Fertility lens | `sim/floods.rs` | Only flooded land gains silt, so high ground stays at its start value |
| Faith's coverage | `faith/` | Along the grid; ignores height (Q9) |
| Docks, ships | `trade/ships.rs` | Level 0, water only; unchanged |
| Caravans | `unit/task/trade.rs` | Path through `Graph`; the map's entry cell must be reachable |
| Minimap | `tile/minimap/`, `crates/minimap_bake` | Real heights replace the noise; cliffs darken |
| Inspector, City book | `inspect/`, `city/` | The terrain panel shows the level; nothing else |
| Washes and previews in pass 1 | `tile/rendering.rs` | Drawn raised, in pass 2, for raised cells |

### 11. Lessons from Zeus

The user reverse-engineered how Zeus (with Poseidon) stores and draws elevation. The notes are in `pharaoh-zeus-re/docs/zeus/` outside this repository: `terrain/zeus-elevation-notes.md`, `rendering/zeus-cliff-rendering-notes.md` and `rendering/zeus-map-rendering-notes.md`. What they establish, and what it means here:

**1. One height byte per cell is the truth.**
- Zeus stores a signed height per cell and works its cliffs and slopes out from the eight neighbours' differences.
- It picks a shape from an ordered table of 152 patterns (first match wins), which may also adjust heights to smooth them.
- **For us:** this confirms storing heights and working cliffs out (§1). We store ramps as well, because they're placed deliberately (Q12).

**2. A level is one tile height, 30 px on a 60 × 30 tile.**
- Raising a cell by one level moves it exactly one backing cell along the screen's vertical diagonal. So the inverse is an index shift (`index + h × 229` in its 228-wide grid).
- **For us:** that's 32 px on our 64 × 32 tile, one cell along (x+1, y+1). It argues for reopening Q1 (§1, §4).

**3. A projected-height lookup grid for picking.**
- Zeus keeps a second byte per cell: for each cell, the highest height projected onto it along the view's diagonal. It's built front to back, keeping maxima, with runs extended where heights rise.
- Picking reads the lookup under the cursor and shifts back by it, instead of testing every level.
- **For us:** we adopt it (§4).

**4. Two passes.**
- Zeus draws ground bases, then objects and the cliffs' raised tops. Which bases to draw is gated by each cell's back and front neighbours' heights, so covered ground is skipped.
- **For us:** this matches §3's two passes, and we take the gate as an optimisation.

**5. Cliff tiles are split.**
- A cliff tile is a ground base and a separate raised top, submitted in different passes. Some are joined into 2 × 2 footprints.
- **For us:** Zeus's corners and ends look better than stacked edges would, at the price of a pattern table and art per pattern. We start with stacked faces and borrow corner pieces if the spike shows seams (§5, Q15).

**6. Neighbours stay within 2 levels.**
- Zeus's smoothing clamps a cell between its lowest neighbour + 2 and its highest − 2, and its patterns only know differences up to 2. Taller drops are spread over several cells.
- **For us:** the editor's smooth brush, with `max_cliff_levels` (Q13).

**7. Gated editor brushes.**
- A raise brush changes only cells one (or two) below the level picked, and a lower brush only those above it. A 16-pass smoothing runs after.
- **For us:** the terrace brush (§8).

**8. Variation from a per-cell random byte, never fresh randomness.**
- **For us:** a hash of the cell (§5).

**9. Rotation.**
- Zeus has four camera rotations. The height grid stays authoritative, and the cliff art is picked per view.
- It also saves its derived image IDs, so some maps hold art picked for another view, which loaders then have to recognise.
- **For us:** we save no derived art, and with Q14 no derived positions. That leaves rotation possible later.

**10. Special buildings own their ground.**
- Monuments skip ordinary shape selection.
- **For us:** §9, Phase 2.

**11. Load-time validation.**
- Zeus keeps a "Found A Bad Elevation" message.
- **For us:** §2.

The notes don't say which Zeus shapes walkers can cross (whether every one-level slope is walkable or only some patterns), how tall its cliffs get in practice, or how its sorting handles tall buildings beside cliffs. Those questions could go back to the reverse-engineering project if they matter. None blocks the spike.

## Stages

### Stage 0: Spike (medium): go or no-go

On a throwaway branch off `claude`, with programmer art (flat-coloured cliff quads), and `LEVEL_STEP_PX` switchable between 16 and 32:
- [ ] An `ElevationMap` that isn't saved; fill a test preset with a plateau, a two-level hill and a ramp.
- [ ] Projection with height; the two-pass draw order; cliff faces as coloured quads.
- [ ] Picking with height, through the projected-height lookup.
- [ ] Measure the frame time and draw calls on a 128 × 128 map with about 30% raised cells, at the widest zoom, on macOS OpenGL, Wgpu and the web build.
- [ ] Screenshots of a building on a plateau, a building behind a cliff, a unit on a ramp and a tall building beside a two-level cliff, at 16 and 32 px, reviewed with the user.

Done when: the draw order is right in the screenshots, the frame time stays within budget (Q8), and the user picks a level height and agrees the look works.

### Stage 1: Data, saves, editor and minimap (medium)

- [ ] `ElevationMap` on `TileMap`, saved as runs; the runtime cliff cache and projected-height lookup; positions recomputed in `post_load`; load-time validation.
- [ ] Save version 15 and its migration (all flat, and the cached positions dropped if Q14 says so); regenerate the sample saves; `--smoke-test-saves`.
- [ ] `TilePlacement::set_elevation`, undo, the developer palette's Height section (raise, lower, terrace, smooth, ramps) and the validity overlay.
- [ ] The minimap from real heights; preset map support.
- [ ] Automation: `terrain.set_elevation`, `terrain.elevation`, and the level in `inspect.at` and `world.building_at`.

### Stage 2: Rendering (medium to large)

- [ ] Projection with height in every cached position; units blend height.
- [ ] The two-pass draw order (§3), with its tie rules and the hidden-ground gate; cliff faces from sprites, with placeholder sprites until Stage 5.
- [ ] Ramps: sloped top faces and side triangles.
- [ ] Washes and previews on raised cells.
- [ ] Culling padding, camera bounds, `teleport`.
- [ ] Overlays: grid, search-graph debug view, hover highlight, previews, tile bounds.

### Stage 3: Picking and placement (medium)

- [ ] Picking with height (§4) in every function listed there, and in automation's cell-to-screen conversion.
- [ ] Placement rules (§7) with new error reasons; road ramps and junctions across cliffs; fields, dykes, canals.
- [ ] Tests in `terrain_placement` for each rule.

### Stage 4: Pathfinding and units (small to medium)

- [ ] Height in the graph, the edge test in `neighbors`, the ramp cost; repath when a cliff blocks the way.
- [ ] Tests in `search_graph`: a cliff blocks, a ramp allows, a ramp chain climbs several levels, a responder takes the ramp.
- [ ] The `elevation.jsonl` scenario, added to CI.

### Stage 5: Art (small to medium; see placeholder-art Phase 5)

- [ ] Cliff faces (left and right, two materials, variations), ramps (4 directions × grass, dirt and two road kinds) and ramp side pieces from the Blender pipeline; corner pieces if Q15 asks for them.
- [ ] Contact sheet and in-game review.

### Stage 6: Gameplay hooks (small)

- [ ] Floods respect height (§9): the fill blocks cells above the flood level; `flood_level` in the mission's floods block; `floods.jsonl` and `elevation.jsonl` check it.
- [ ] Irrigation and a farmstead's reach stop at cliffs (Q9).
- [ ] Tower range bonus and attackers funnelled through ramps (Phase 2 defence).

## Scheduling

- Height isn't needed by Campaign I's missions 1 to 4, which are all lowland delta and coast. Mission 5 (a highland pass) and mission 8 (the western marches) want it.
- **Decided 2026-11-22: do T now, before M9, M11 and M13**, so the technical work is done before the missions are built. Reasons:
  - M13 then builds each map once, mission 5's with heights;
  - the save-format change lands before any mission map exists;
  - M9 and M11 are simulation and UI work and don't touch the renderer, picking, pathfinding or placement.
- If the spike is a no-go, mission 5 ships on a flat map with decorative mountains (option A).

## Risks

| Risk | Mitigation |
|---|---|
| Sorting artifacts around large buildings next to cliffs | The placement rule in §3; the spike screenshots; accept the rare remaining cases, as the reference games do |
| Mixing terrain and object atlases in pass 2 costs batching | Measured in the spike. If it's too slow, put cliff, ramp and raised top sprites into the objects atlas |
| Picking or placement bugs where heights change | Each rule has a unit test, and the scenario runs in CI |
| Grid-based systems giving odd results across cliffs (§10) | The table in §10 names each one; the scenario covers the flood and the fields |
| Stacked faces look blocky at cliff corners | Corner and end pieces, Zeus's way (Q15) |
| A level height of 16 or 32 px looks wrong | It's a constant, picked in the spike from screenshots before any art is made (Q1) |

## Open decisions

**Q1. How tall is one level, in logical px? (Reopened 2026-11-22)**
- Decided on 2026-11-05: 16, on the grounds that it kept draw order and picking exact.
- The refresh found that draw order is exact at any height (§3). At 16 px, a raised cell lands on the corner of four flat diamonds; at **32 px**, one tile height, it lands exactly on one, which makes picking a whole-cell shift and the hidden-ground test exact. Zeus uses one tile height (§11).
- 32 px makes taller, more dramatic cliffs, and doubles the camera and culling padding.
- *New default: 32, picked in the spike from screenshots of both.*

**Q2. What's the highest level?**
*Default: 7* (3 bits). Missions will probably use 0–3. At 32 px, 3 levels is 96 px, three tile heights.

**Q3. Is water only at level 0?**
*Default: yes.* Lakes on plateaus and waterfalls are out of scope. Canals too (Q10).

**Q4. Can buildings stand beside much higher cells?**
*Default: no.* A building can't stand beside a cell more than one level higher on its left or right side.

**Q5. What materials do cliff faces come in?**
*Default: two,* grassy rock and bare rock, chosen from the top cell's terrain, plus karst limestone for Đại Long later.

**Q6. Can players change terrain height?**
*Default: no,* only the editor can. That avoids handling buildings, roads and water when the ground under them changes.

**Q7. Does climbing a ramp cost walkers extra time?**
*Default: yes, a small cost* (1.5× a flat step), so paths prefer flat routes when they're about as short.

**Q8. What frame-time budget does the spike have to meet on a mountain map?**
*Default: no worse than 10% over the same map flat,* on the user's Mac at the widest zoom, on OpenGL and Wgpu.

**Q9. Do reaches measured along the grid stop at cliffs? (New)**
Services' coverage, a farmstead's field reach, faith's coverage, wells' water, irrigation's spread and risks' spread are measured in grid steps today (§9, §10).
*Default:* a farmstead's reach and irrigation stop at cliffs (a field belongs to the land its farmers walk, and water doesn't run uphill). The rest ignore height until playtesting says otherwise.

**Q10. What can stand on raised ground? (New)**
*Default:*
- buildings, houses, roads, fields, rubble, trees and rocks on any flat level;
- dykes on any flat level, joined only on one level, with no dyke ramps;
- canals, docks, wharves and water at level 0 only.

**Q11. How high do floods reach? (New)**
*Default:* only level-0 land floods (`flood_level` 0), set per mission. A severe flood rising a level is left to M13's balance.

**Q12. Are ramps stored or worked out from heights? (New)**
Zeus works its slopes out from the neighbours (§11).
*Default: stored*, since a ramp is a chokepoint and a road's way up that the designer places. The editor's overlay flags ramps that don't connect, and could suggest ramps later.

**Q13. How much can neighbours differ? (New)**
Zeus keeps them within 2 levels (§11). Stacked faces draw any drop.
*Default:* any drop is allowed; the smooth brush and the overlay use `max_cliff_levels` = 2.

**Q14. Stop saving the cached tile positions? (New)**
Since v15 bumps the format anyway, `iso_coords_f32` can become `serde(skip)` and be recomputed in `post_load`. That shrinks saves, and no derived data can then disagree with the heights.
*Default: yes*, if the smoke test and the determinism checksums hold. `session.checksum` hashes the session as a save stores it, so the checksums change once, with the format.

**Q15. Stacked faces or Zeus-style pattern pieces? (New)**
*Default: stacked faces first.* Corner and end pieces picked from the neighbours come only if the spike's screenshots show seams that matter.
