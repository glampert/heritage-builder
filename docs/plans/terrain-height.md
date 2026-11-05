# Terrain height: analysis and plan

**Status (2026-11-05): Analysis and draft plan. Not started.** This plan looks at how to add terrain elevation (hills, plateaus, cliffs and ramps) for highland maps such as Đại Long's northern mountains and valleys. It compares the options, recommends one, and breaks it down into stages. It is milestone T of the [roadmap](roadmap.md). It was researched against `d0696fe` on `main`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Start with **Stage 0 (the spike)**. It decides whether the recommended approach holds up before anything is committed to the save format.
- Tick the checkboxes and add dated notes under each stage as tasks land.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise.
- Follow [AGENTS.md](../../AGENTS.md):
  - keep both targets compiling;
  - route edits through `TilePlacement` and the `TileMap` setters;
  - follow the Saves workflow, since Stage 1 bumps the save version.

## Goals

1. Maps can have **raised terrain**: plateaus at a few discrete levels, cliffs between levels that can't be crossed, and ramps that can.
2. **Height matters in play:**
   - cliffs are barriers;
   - ramps are chokepoints for attackers;
   - high ground escapes floods;
   - towers on high ground reach further.
3. **Map designers author height in the developer editor.** Players don't sculpt terrain (Q6).
4. **Flat maps don't change:** they look, perform and save the same as now apart from the format bump, and existing missions keep working.
5. **Art comes from the Blender pipeline:** cliff faces and ramps ([placeholder-art.md](placeholder-art.md), Phase 5). Raised cells reuse the existing terrain sprites.

**Not a goal:** matching Zeus exactly. The model below takes Zeus as its inspiration (raised plateaus, cliff faces and access by ramps), but the rules are our own.

## What exists today (and why height touches so much)

Nothing in the code has a notion of height. Every screen position comes from the cell's (x, y) alone.

**Projection** (`common/src/coords.rs:415`):
```rust
iso_x = (cell.x - cell.y) *  32;
iso_y = (cell.x + cell.y) * -16;   // larger x+y is higher on screen
```
- One step of the diagonal `d = x + y` moves 16 logical px up the screen.
- `WorldToScreenTransform` is only a 2D scale and offset.

**Tiles:**
- `TerrainTile` and `ObjectTile` cache `iso_coords_f32`, and that value is serialized (`tile/mod.rs:392`, `:492`).
- Terrain defs must be exactly 64×32 (`sets.rs:826`).
- Terrain sprites are drawn at 65×36: the diamond plus about 4 px of side face.

**Rendering** (`tile/rendering.rs`):
- `draw_map` (`:144`) draws **all terrain first, unsorted** (reverse row-major, `:180`), then all objects.
- Objects are sorted by one float, the screen Y of the footprint's centre (`:568`), with overrides for topmost and bottommost.
- The engine is pure 2D, with **no depth buffer** (`opengl/mod.rs:229`; Wgpu passes have no depth attachment). It's a painter's algorithm, in submission order.

**Culling:** `calc_visible_cells_range` (`camera.rs:950`) → `selection::bounds` (`selection.rs:321`): the screen corners go to flat iso, then to a cell bounding box padded by one tile.

**Picking:** `find_exact_cell_for_point` (`tile/mod.rs:1809`) projects to a flat cell, then tests that cell's diamond and its 8 neighbours'. `topmost_tile_at_cursor` (`:2965`) tests base-cell diamonds.

**Pathfinding:** `Graph` is one `NodeKind` per cell (`pathfind/mod.rs:313`). Passability is decided per node in `neighbors` (`:541`), with no data per edge. `Heuristic::movement_cost(from, to)` (`:668`) is a ready hook for slope costs.

**Units:** `Unit::update_navigation` (`unit/mod.rs:401`) blends flat iso positions between cells. While moving, they sort by their starting cell.

**Placement:** there are no flatness rules (`placement.rs:114`, `:297`). Every edit goes through `TilePlacement::place`/`clear` (`menu/mod.rs:538`). Undo stores def, flags and variation per cell.

**Saves:**
- Version 2.
- Terrain takes about 320 bytes per cell (`test_map_128x128` is 13.6 MB).
- New serialized fields need a version bump and a migration script, with no `serde(default)` shims.

**Things that help:** objects already sort by a per-tile screen-Y key; positions are cached per tile; graph updates go through `update_search_graph!` in one place; `movement_cost` exists; every edit goes through `TilePlacement`.

**Things that hurt:** the unsorted terrain pass; no depth buffer; picking within one cell; passability that is per node only; terrain sizes locked; cached iso coordinates stored in saves.

## Options

| | A. Decorative mountains | B. Discrete levels, cliffs and ramps (Zeus-like) | C. Per-corner heightfield (SimCity 2000 / OpenTTD) | D. A real 3D terrain mesh |
|---|---|---|---|---|
| **What it is** | Impassable mountain and rock props on flat land, as Pharaoh does | Each cell has a level from 0 to 7 and is either flat or a ramp. Cliffs appear wherever neighbouring levels differ | Each cell corner has a height, giving about 19 slope shapes per cell | Render the terrain as a 3D mesh, with sprites on top |
| **Building on high ground** | No | Yes, on plateaus | Yes, with foundations on slopes | Yes |
| **Gameplay value** | Scenery and barriers | Plateaus, chokepoints, flood refuges, tower range | Same as B, plus smooth hills | Same as C |
| **Art cost** | A few large props | Cliff faces (2 edges, stacked one level at a time) plus 4 ramp directions per terrain kind. **Top faces reuse today's sprites** | Every terrain kind and transition × about 19 slopes; roads and buildings on slopes | A new terrain art style and texturing |
| **Engine cost** | None | Medium: projection, a second sorted pass, picking, graph edges, editor | Large: B plus slope sprites everywhere, placement on slopes, and water on slopes | Very large: depth buffer, a new terrain renderer, sorting sprites against a mesh |
| **Fits the look** | Yes | Yes. Emperor and Zeus use this kind of terrain | It looks like a different genre | No |

**Recommendation: B.** It delivers what the design asks for (mountains and valleys, plateaus to build on, chokepoints, high ground in floods) at a fraction of C's art cost, because cliffs are built from stacked pieces and top faces reuse the existing sprites. It also changes nothing on flat maps. Option A is still useful alongside B, for mountains you can't enter at the map's edges.

## Design for option B

### 1. The model

- **Each cell** has a level `h` from 0 to 7, which fits in 3 bits, and a shape: `Flat` or `Ramp(dir)`, where `dir` is the uphill direction (N, E, S or W).
- **A ramp at level `L`** joins its downhill neighbour at `L` to its uphill neighbour at `L + 1`. Its top face slopes by one level. A longer climb is a chain of ramps facing the same way.
- **Cliffs** aren't stored. A cliff exists wherever two flat cells side by side have different levels, or a ramp's side edge meets a different level. They're worked out from the heights when needed.
- **Height of one level:** `LEVEL_STEP_PX = 16` logical px (Q1). That is exactly one step of the diagonal on screen, so a cell at level `h` appears where a flat cell `h` diagonals further back would. Draw order and picking both build on this (§3, §4).
  - In the Blender camera, 1 world unit of height projects to 64/√2 × sin 60° ≈ 39.2 px.
  - So one level is 16 / 39.2 ≈ **0.408 Blender units**, which the generator uses for cliff and ramp pieces.
- **Water** stays at level 0 (Q3). A plateau beside water drops to the water through a cliff face; the shore logic keeps treating any land as land.

### 2. Data and saves

- `TileMap` gains `elevation: ElevationMap { size, cells: Vec<u8> }`:
  - bits 0–2 hold the level;
  - bits 3–5 hold the shape (0 means flat; 1–4 mean a ramp facing N, E, S or W);
  - bits 6–7 are reserved.

  It is saved as a compact array, about 16 KB of JSON for 128×128, against a 13.6 MB save.
- It sits alongside the layers rather than in each `Tile`, so `Tile`'s layout and size stay the same and edits are one-byte writes.
- **Runtime only** (`serde(skip)`, rebuilt in `post_load`):
  - a cliff cache, holding the drop on each cell's two front edges;
  - the iso positions, which already sit on tiles and are recomputed on load from the cell and its height.

  The saved `iso_coords_f32` can't be trusted once a cell's height has changed.
- **Saves:** bump the version. The migration script adds an all-zero `elevation` to every save, so every existing map stays flat. Regenerate `saves/sample_saves.zip`.

### 3. Projection and draw order

**Projection:**
- `cell_to_iso_h(cell, h) = cell_to_iso(cell) + (0, -h × 16)`. Ramps use `h + 0.5` at the centre.
- Every cached position (`TerrainTile::new`, `calc_object_iso_coords`, `calc_unit_iso_coords`) uses the cell's height.
- Units blend height as well as (x, y) while moving between cells, so walking up a ramp looks continuous.

**Draw order:** the painter's algorithm stays, with no depth buffer. It is split in two passes:

1. **Pass 1, unchanged:** every level-0 terrain cell, drawn first and unsorted, as today. A level-0 top face can never cover anything, because everything drawn after it is on top of it or in front of it. Flat maps cost exactly what they do today.
2. **Pass 2, sorted:** objects as today, **plus every raised terrain cell (h > 0, or a ramp)**, sorted by the existing screen-Y key.
   - A raised cell's key is the screen Y of its base diamond.
   - Ties put terrain first, so a building on a plateau draws after the ground under it.
   - A raised cell draws its **cliff faces first** (§5), then its top-face sprite, raised.

**Why this order is right:**
- Anything behind a raised cell (an object or lower ground) has a larger key, so it is drawn before the cell and gets covered by it.
- Anything in front has a smaller key, so it is drawn after.
- A cliff face's bottom edge falls exactly on the edges it shares with its front neighbours' top faces, because the level step equals one diagonal step. So pass 1 never has to draw over a cliff.

**Known artifact:** sprites spread over several bands (large buildings, tall cliffs beside a building's side) can be sorted wrongly where a building stands right beside a higher cell at the same depth. This is the classic isometric sorting limit, which the reference games share. A placement rule removes the worst cases: a building can't stand beside a cell more than one level higher on its left or right side (Q4).

**Cost:** pass 2 grows by the number of raised cells on screen, about a few thousand at most. The sort already exists. Mixing the terrain and object atlases in one sorted list breaks some sprite batches, so expect a few hundred more draw calls on mountain maps. The spike measures this.

### 4. Culling, camera and picking

- **Culling:** `calc_visible_cells_range` pads the bottom of the screen rectangle by `max_level × 16` px, because a raised cell that sits below the screen in flat terms can rise into view. `update_anims` uses the same range.
- **Camera:** `calc_map_bounds` and the camera constraints extend the top edge by `max_level × 16` px. `teleport` and `cell_to_screen` (automation) use the height.
- **Picking:** from the screen point `p`, go through `h` from `max_level` down to 0:
  1. Take the flat cell under `p` moved down by `h × 16`.
  2. If that cell's level is `h` and `p` is inside its raised diamond (or its sloped quad, for a ramp), it's a hit.

  The first hit is the right one: higher candidates are further forward on screen, so they would draw last. If `p` lands on a cliff face, it maps to the cliff's top cell and is marked "face". The cell is shown but nothing can be placed there.
- **Other functions** that change the same way: `find_exact_cell_for_point`, `topmost_tile_at_cursor`, `is_screen_point_inside_base_cell`, and `cell_to_screen_diamond_points`.
- **Range selection:** dragging a selection keeps working in flat cells, which is good enough for clearing an area.
- **Overlays** draw raised diamonds: the grid, the debug view of the search graph, the road-placement overlay and the tile bounds.

### 5. Cliff and ramp art

- Only a cell's **two front edges** can show a cliff face, because the back edges face away from the camera.
- For each front edge, `drop = h(cell) − h(front neighbour)`, or the cell's own level at the map's edge. If the drop is above 0, the renderer draws `drop` copies of a **one-level face sprite** for that edge, stacked from the top down.
  - This needs only 2 sprites per material (left and right), plus a few variations to break up repetition.
  - The material follows the top cell's terrain: grassy rock or bare rock (Q5).
- **A ramp** is one sloped sprite per direction and terrain kind: grass, dirt, dirt road and paved road, giving 4 directions × 4 kinds. A ramp's side edges also show a triangular cliff piece where they meet lower ground: one sprite per side.
- These are runtime decorations computed from the heights, **not** tiles in the map, so they add nothing to saves and need no variation bookkeeping.
- All the pieces come from the Blender pipeline (`cliff_face`, `ramp`). One level is 0.408 Blender units, read from one shared constant.

### 6. Pathfinding and units

- **The graph** stores a copy of each cell's height and shape, updated wherever `update_search_graph!` runs and on height edits. `Graph::neighbors` (`:541`) adds an edge test, worked out on the fly with no data per edge. A step from `a` to `b` is allowed if any of these holds:
  - both are flat at the same level;
  - one is a ramp and the other is its uphill neighbour at `L + 1`, or its downhill neighbour at `L`;
  - both are ramps facing the same way, one level apart, along that direction.
- `Heuristic::movement_cost` adds a small cost for ramps (Q7). `UnitNavigation::update`'s check on the next node also checks the edge, so a unit whose path crosses a new cliff stops and repaths.
- Units draw at their blended height. Their sort key stays the starting cell's, which is correct on ramps because a ramp's band is the lower cell's.

### 7. Placement rules

| Thing | Rule |
|---|---|
| Buildings, vacant lots, farms | Every footprint cell flat and at the same level. The road link must be on the same level, or on a ramp that leads to it |
| Roads | Flat cells, or ramps running along the ramp's direction. Junctions (`road::junction_mask`) ignore neighbours across a cliff, and a ramp road connects uphill and downhill only. This needs 4 ramp road sprites |
| Water | Level 0 only |
| Trees and rocks | Flat cells at any level |
| Walls, dykes, towers | Flat; a tower on high ground gets bonus range (gameplay hook) |
| Changing a height under something | Not allowed for players (Q6). In the editor, clearing objects first is required |

These go into `placement::internal::is_placement_on_terrain_valid`, `try_place_tile_in_layer`, `house_upgrade::can_expand_into_cell` and `road::can_place_road`/`build_segment`, each with a new `TilePlacementErrReason`.

### 8. The editor, tools and automation

- **The developer palette** gets a "Height" section:
  - raise, lower and flatten brushes (1×1, 3×3, 5×5), and setting a ramp by dragging in its uphill direction;
  - an overlay showing levels, with ramps that don't connect to the right levels flagged in red.

  Edits go through a new `TilePlacement::set_elevation`, which updates the graph and the cliff cache and records undo. `TileSavedState` gains the cell's elevation byte.
- **Preset maps** get an optional elevation array (`debug/preset_maps.rs`).
- **Minimap** shading by level, with darker cliff edges (`minimap.rs:169`).
- **Automation:**
  - `terrain.set_elevation cell=[x,y] level=2 shape=RampN`;
  - `tile.at` and `world.inspect` report elevation;
  - `camera.cell_to_screen` takes height into account.

  A new scenario, `elevation.jsonl`, builds a plateau and a ramp. It checks pathfinding across the ramp and not across a cliff, the placement rules, and that picking a raised cell selects it.

### 9. Gameplay hooks (later milestones use these)

- **Floods** (roadmap M7): a flood covers land cells below the mission's `flood_level`. High ground is safe, and a dyke raises the level that's protected.
- **Defence** (Phase 2): attackers path like walkers, so cliffs funnel them through ramps. Towers on high ground get bonus range.
- **Farming** (later): Đại Long terraced rice fields as a building that spans a run of ramps.
- **Desirability** (later): a bonus for houses on high ground.

## Stages

### Stage 0: Spike (medium) — go or no-go

On a throwaway branch, with programmer art (flat-coloured cliff quads):
- [ ] An `ElevationMap` that isn't saved; fill a test preset with a plateau and a ramp.
- [ ] Projection with height; the two-pass draw order; cliff faces as coloured quads.
- [ ] Picking with height.
- [ ] Measure the frame time and draw calls on a 128×128 map with about 30% raised cells, at the widest zoom, on macOS OpenGL and in the web build.
- [ ] Screenshots of a building on a plateau, a building behind a cliff, and a unit on a ramp, reviewed with the user.

Done when: the draw order is right in the screenshots, the frame time stays within budget (Q8), and the user agrees the look and scale work.

### Stage 1: Data, saves, editor and minimap (medium)

- [ ] `ElevationMap` on `TileMap`, saved; the runtime cliff cache; recompute iso positions in `post_load`.
- [ ] Save version bump and migration (all zeros); regenerate the sample saves; `--smoke-test-saves`.
- [ ] `TilePlacement::set_elevation`, undo, the developer palette's Height section, and the validity overlay.
- [ ] Minimap shading; preset map support.
- [ ] Automation: `terrain.set_elevation`, and elevation in `tile.at`.

### Stage 2: Rendering (medium to large)

- [ ] Projection with height in every cached position; units blend height.
- [ ] The two-pass draw order (§3), with tie rules; cliff faces from sprites, with placeholder sprites until Stage 5.
- [ ] Ramps: sloped top faces and side triangles.
- [ ] Culling padding, camera bounds, `teleport`.
- [ ] Overlays: grid, search graph debug view, road-placement overlay, tile bounds.

### Stage 3: Picking and placement (medium)

- [ ] Picking with height (§4) in every function listed there, and in automation's `screen_to_cell`.
- [ ] Placement rules (§7) with new error reasons; road ramps and junctions across cliffs.
- [ ] Tests in `terrain_placement` for each rule.

### Stage 4: Pathfinding and units (small to medium)

- [ ] Height in the graph, the edge test in `neighbors`, the ramp cost; repath when a cliff blocks the way.
- [ ] Tests in `search_graph`: a cliff blocks, a ramp allows, a ramp chain climbs several levels.
- [ ] The `elevation.jsonl` scenario, added to CI.

### Stage 5: Art (small to medium; see placeholder-art Phase 5)

- [ ] Cliff faces (left and right, two materials, variations), ramps (4 directions × grass, dirt and two road kinds), and ramp side pieces from the Blender pipeline.
- [ ] Contact sheet and in-game review.

### Stage 6: Gameplay hooks (with M7 and Phase 2)

- [ ] Floods respect height (M7).
- [ ] Tower range bonus and attackers funnelled through ramps (Phase 2 defence).

## Scheduling

Height is **not** on the critical path for Campaign I, missions 1–4, which are all lowland delta and coast. Mission 5 (a highland pass) and mission 8 (the western marches) want it. The suggestion:
- run Stage 0 once construction (M3) has landed;
- if the spike is a go, run Stages 1–4 during Phase 1;
- build mission 5's map last, with heights.

If height slips, mission 5 can ship on a flat map with decorative mountains (option A) and get height later.

## Risks

| Risk | Mitigation |
|---|---|
| Sorting artifacts around large buildings next to cliffs | The placement rule in §3; the spike screenshots; accept the rare remaining cases, as the reference games do |
| Mixing terrain and object atlases in pass 2 costs batching | Measured in the spike. If it's too slow, put cliff and ramp sprites into the objects atlas so pass 2 stays on one texture set |
| Picking or placement bugs where heights change | Each rule has a unit test, and the scenario runs in CI |
| The save version bump collides with other milestones' bumps | Coordinate through the roadmap's save-version log; group bumps where possible |
| A level height of 16 px looks too low or too high | It's a constant, decided in the spike before any art is made (Q1) |

## Open decisions

**Q1. How tall is one level, in logical px?**
*Default: 16,* one diagonal step. That keeps draw order and picking exact. 32 (one full tile height) gives more dramatic cliffs, but loses the exact one-step match.

**Q2. What's the highest level?**
*Default: 7* (3 bits). Missions will probably use 0–3.

**Q3. Is water only at level 0?**
*Default: yes.* Lakes on plateaus and waterfalls are out of scope.

**Q4. Can buildings stand beside much higher cells?**
*Default: no.* A building can't stand beside a cell more than one level higher on its left or right side.

**Q5. What materials do cliff faces come in?**
*Default: two,* grassy rock and bare rock, chosen from the top cell's terrain, plus karst limestone for Đại Long later.

**Q6. Can players change terrain height?**
*Default: no,* only the editor can. That avoids handling buildings, roads and water when the ground under them changes.

**Q7. Does climbing a ramp cost walkers extra time?**
*Default: yes, a small cost* (1.5× a flat step), so paths prefer flat routes when they're about as short.

**Q8. What frame-time budget does the spike have to meet on a mountain map?**
*Default: no worse than 10% over the same map flat,* on the user's Mac at the widest zoom.
