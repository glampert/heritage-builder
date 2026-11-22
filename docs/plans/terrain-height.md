# Terrain height: analysis and plan

**Status (2026-11-22): Stage 0 (the spike) done: a go, at 32 px a level. Stage 1 (the data, save v15) and Stage 2 (the pieces, from Blender) done; Stage 3, rendering, next.** This plan adds terrain elevation (hills, plateaus, cliffs and ramps) for highland maps such as Đại Long's northern mountains and valleys. It compares the options, recommends one, and breaks it down into stages. It is milestone T of the [roadmap](roadmap.md), to be done before M13 builds the mission maps.

- It was first written on 2026-11-05 against `d0696fe`, before M3 to M10.
- It was refreshed on 2026-11-22 against `d6801146` (M10 done, save version 14):
  - the code's current state;
  - the systems added since, which all work on the grid (§10);
  - what Zeus does, from the user's reverse-engineering notes (§11);
  - the open decisions that follow from them (Q9 to Q15, and Q1 reopened).
- The spike (Stage 0, 2026-11-22, branch `spike/terrain-height`, never merged) changed the design in places: the draw order's two extra rules and the rims (§3), picking without a lookup grid (§4), positions kept flat and lifted as they're drawn (§2), and the art as Blender pieces made before the rendering (§5, Stage 2). Its notes are under Stage 0.
- References name functions rather than lines, which move. Re-check them before relying on them.

## How to resume this work

- The [open decisions](#open-decisions) each have a decision or a default: proceed with the default unless the user has decided otherwise.
- Stage 0 (the spike) is done. Work through the stages in order: the data (1), the pieces' art (2), rendering (3), picking and placement (4), pathfinding (5) and the gameplay hooks (6).
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

- **Each cell** has a level `h` from 0 to 7, which fits in 3 bits, and a shape: `Flat`, or half of a ramp, `Ramp(dir, half)`, where `dir` is the uphill direction (N, E, S or W) and `half` is `Low` or `High`.
- **A ramp climbs one level over two cells** (decided in Stage 2, 2026-11-22): its low half, at level `L`, rises from `L` to `L + ½`, and its high half, the next cell uphill, also at level `L`, from `L + ½` to `L + 1`. So it joins ground at `L` below it to ground at `L + 1` above it. A longer climb is a chain of ramps facing the same way.
  - Why two cells: at 32 px a level, one level in one cell climbs about 39°, steeper than the camera looks down (30°). A ramp climbing towards the viewer (south or east) then faces away from the camera and vanishes behind its own top edge, which Stage 2's first renders showed. Two cells a level climb about 22°, and every ramp shows; seen from the front, at a shallow angle. Zeus's slopes span several cells too (§11).
- **Ramps are stored, not worked out** (Q12):
  - Zeus works its slopes out from the heights, with an ordered table of 152 neighbourhood patterns (§11).
  - We store them, because a ramp is a gameplay decision (a chokepoint, a road's way up) that the designer places.
  - The editor can still suggest them.
- **Cliffs aren't stored.** A cliff exists wherever two flat cells side by side have different levels, or a ramp's side edge meets a different level. They're worked out from the heights when needed, as in Zeus.
- **Height of one level:** `LEVEL_STEP_PX` = **32 logical px**, one tile height (Q1, decided 2026-11-22), as Zeus's levels are (§11).
  - A cell at level `h` appears exactly where the flat cell `(x + h, y + h)` would: a raised top covers one flat diamond behind it.
  - It stays one constant, so it can change after playtesting. The spike drew 16 px too: the same cost and the same draw order, but its plateaus read as low terraces where 32 px reads as cliffs.
  - In the Blender camera, 1 world unit of height projects to 64/√2 × cos 30° ≈ 39.2 px, so one level is 32 / 39.2 = **√(2/3) ≈ 0.8165 Blender units**. The generator's pieces read it from one constant (§5).
- **Water** stays at level 0 (Q3). A plateau beside water drops to the water through a cliff face; the shore logic keeps treating any land as land.
- **Neighbour differences:** Zeus keeps neighbours within 2 levels of each other (§11). We allow any drop, since stacked faces draw any height, but the editor's smooth brush keeps differences within `max_cliff_levels` (Q13).

### 2. Data and saves

- `TileMap` gains `elevation: ElevationMap { size, cells: Vec<u8> }`:
  - bits 0–2 hold the level;
  - bits 3–5 hold the shape (0 means flat; 1–4 mean a ramp facing N, E, S or W);
  - bit 6 marks a ramp's high half (Stage 2: v15's format, unchanged, since no save had a ramp yet);
  - bit 7 is reserved.

  It's saved as runs, as the floods' fertility is (`CityFloods`). An all-flat map is one run; a 128 × 128 map with hills is a few KB of JSON.
- It sits alongside the layers rather than in each `Tile`, so `Tile`'s layout and size stay the same and edits are one-byte writes.
- **Nothing derived is saved.** Cliffs, rims and ramps' pieces are worked out from the heights as they're drawn (§5), and the spike found that cheap enough without a cache. Zeus saves its derived image IDs, and its maps then carry art chosen for another camera view (§11); saving only the heights avoids that kind of mismatch.
- **Iso positions stay flat** (Q14, decided by the spike): a tile's cached `iso_coords_f32` and a walker's blended position keep ignoring height, and the renderer lifts each thing by its height as it draws it (§3). So no cached position can disagree with the heights, and nothing about them changes in saves.
- **Validation on load:** a ramp whose uphill neighbour isn't one level up, or a level above 7, is logged on a `terrain` channel and drawn flagged in the editor's overlay. The map still loads. Zeus keeps a "Found A Bad Elevation" message for the same job.
- **Saves:** bump to v15. The migration script adds an all-flat `elevation` to every save's tile map, so every existing map stays flat.

  Regenerate `saves/sample_saves.zip` and run `--smoke-test-saves`.

### 3. Projection and draw order

**Projection:** a thing is drawn `lift × LEVEL_STEP_PX` higher than its flat position (§2: cached positions stay flat).
- Terrain and objects: their base cell's level; a ramp's middle is `h + 0.5`.
- A walker: the ground's height where it stands, read from its blended position (a ramp's surface rises across it), so walking up a ramp looks continuous. The spike's `unit_position` reads the position back from the tile's iso coords.
- Overlays drawn with a tile (fire, sickness, scaffolds, renovation, the festival dress, the flood and dry-canal washes, road previews) take its lift.

**Draw order:** the painter's algorithm stays, with no depth buffer. It's split in two passes, as Zeus does (§11):

1. **Pass 1, unchanged:** every level-0 terrain cell, drawn first and unsorted, as today, with its washes. A level-0 top face can never cover anything, because everything drawn after it is on top of it or in front of it. Flat maps cost exactly what they do today.
2. **Pass 2, sorted:** objects as today, **plus every raised terrain cell (h > 0, or a ramp)**, sorted by the existing screen-Y key.
   - A raised cell's key is the screen Y of its base diamond.
   - Ties put terrain first, so a building on a plateau draws after the ground under it.
   - **The ground under a building sorts with the building** (found by the spike): a raised footprint cell takes its building's key, so its top doesn't draw over the building's base. A footprint's front cells are in front of the building's own key, so without this their tops covered the building's lower part.
   - **A walker sorts by the cell it's nearest** while terrain is raised (found by the spike), not the cell it started its step from: otherwise the raised ground it steps onto, drawn after it, covers its feet.
   - A raised cell draws its **cliff faces first** (§5), then its top-face sprite, raised, then its washes (flooded, dry canal) and previews, then a **rim** along each back edge that stands above the ground behind it (§5).

**Hidden ground:** pass 1 skips a level-0 cell whose diamond a raised neighbour in front covers whole, and pass 2 skips cliff faces that nothing can see. Zeus does the same from each cell's back and front heights (§11). It's an optimisation, not needed for correctness.
- At 32 px, the cell `(x+1, y+1)` behind a raised `(x, y)` is covered exactly.
- At 16 px, no flat diamond is covered whole by one raised cell, so the test needs two.

**Why this order is right:**
- Anything behind a raised cell (an object or lower ground) has a larger key, so it's drawn before the cell and gets covered by it.
- Anything in front has a smaller key, so it's drawn after.
- A cliff face's bottom edge is the cell's own front edges at ground level, which it shares with its front neighbours' top faces. So pass 1 never has to draw over a cliff, whatever `LEVEL_STEP_PX` is.

**Known artifact:** sprites spread over several bands (large buildings, tall cliffs beside a building's side) can be sorted wrongly where a building stands right beside a higher cell at the same depth. This is the classic isometric sorting limit, which the reference games share. A placement rule removes the worst cases: a building can't stand beside a cell more than one level higher on its left or right side (Q4).

**Cost** (the spike's numbers are under Stage 0):
- Pass 2 grows by the number of raised cells on screen, a few thousand at most. The sort already exists and stays well under a millisecond.
- Mixing the terrain and object atlases in one sorted list breaks sprite batches: on a dense hilly map, about 1,000 more texture switches, half a millisecond of OpenGL submit time.
- So the pieces (§5) live in the **objects atlas**, beside the buildings, and Stage 3 measures whether the raised tops should join them there too (a copy of each terrain sprite in the objects atlas).

### 4. Culling, camera and picking

- **Culling:** `calc_visible_cells_range` pads the bottom of the screen rectangle by `max_level × LEVEL_STEP_PX`, because a raised cell that sits below the screen in flat terms can rise into view. `update_anims` uses the same range.
- **Camera:** `calc_map_bounds` and the camera constraints extend the top edge by `max_level × LEVEL_STEP_PX`. `teleport`, `teleport_iso` and automation's cell-to-screen conversion use the height.
- **Picking** (as the spike built it, `elevation::pick`): the front-most column that covers the point.
  1. Take the flat cell under the point. The candidates are the cells in front of it, as far as the highest level reaches (a cell at level `h` shows `h` cells further back along the screen's vertical).
  2. For each, nearest the front first: the point is on its top if it's inside its raised top diamond (a ramp's sloped quad), on its face if it's inside the column below that top, down to its own ground diamond.
  3. The first hit is the answer: columns further front are drawn later. A point on a cliff face maps to the cliff's top cell and is marked "face". The cell is shown, but nothing can be placed there.
  - It tests a few dozen candidates at most, only when the cursor moves, so Zeus's projected-height lookup grid (§11) isn't needed: one less grid to keep in step with the heights.
- **Functions that change:** `find_exact_cell_for_point`, `topmost_tile_at_cursor`, `is_screen_point_inside_base_cell` and `cell_to_screen_diamond_points`. So do automation's map-cell gestures, which aim at a cell's screen point.
- **Range selection:** the area-drag tool (fields, the clear tool) keeps working in flat cells under the dragged rectangle. Over a cliff it selects cells on both levels; placement then refuses those that don't fit (§7).
- **Overlays** draw raised diamonds:
  - the grid;
  - the search-graph debug view;
  - the hover highlight;
  - the road, canal, dyke and field previews;
  - the tile bounds.

### 5. Cliff and ramp art

The pieces are sprites the renderer draws from the heights, **not** tiles in the map: they add nothing to saves and need no variation bookkeeping. They come from the Blender pipeline (`crates/tools/blender-tile-generator`, the recipes in `hbgen/recipes/heights.py`, the entries in `catalog/heights.json`; placeholder-art Phase 5), in the objects tile set's `overlays` category, so they share the objects atlas with the buildings (§3, Cost). Each is anchored as a 1 × 1 building on its cell and lifted to its level. The game finds them by name through `tile::height_pieces`.

- **Cliff courses.** Only a cell's **two front edges** can show a face, since the back edges face away from the camera. For each, the drop is the cell's level less the front neighbour's ground along that edge (a ramp neighbour's surface at the edge's corners; past the map's edge, 0). The renderer stacks one **one-level course** per level of the drop, from the lower ground up.
  - `cliff_left` (the south edge, lower left on screen) and `cliff_right` (the east edge, lower right): `mid_a` and `mid_b` under the top, and the top course capped by a lip of the cell's terrain, `top_a` or `top_b` (grass) or `top_dirt` (dirt and roads; `height_pieces::has_dirt_lip`). A look is picked by a hash of the cell and level (`height_pieces::cliff_course`, `common::hash::splitmix64`), never the RNG and never saved, as the risks' jitter is. Zeus keeps a random byte per cell for the same job.
  - Each course is one open mesh (no end caps, so its ends get no ink), its ledges at the same heights in every piece, and its rock's noise sampled on two circles so it wraps every cell along the edge and every level up. Stacked courses and neighbours along an edge join; only a few cracks vary, away from the ends. Each reaches a hair past its cell's ends (`OVERLAP`), so where two meet the soft edge of one lies over solid pixels of the other instead of two soft edges showing a seam.
  - Pieces stay inside their cell's column apart from that hair, so the draw order of §3 holds for them.
  - Q5's second material (bare rock) is the dirt lip for now; karst limestone for Đại Long's highlands later.
- **Rims** (the user's request, 2026-11-22): a light lip with an inked edge along **every** side of a raised cell's top where the ground on that side is lower, front or back (`cliff_rim`, variations `n`, `e`, `s`, `w`), drawn after the cell's top. Without them a terrace's top reads as the same grass as the ground below it, and its far side vanishes (the spike found the walker behind a plateau looked sunk into the grass).
- **Ramps.** Half a ramp per piece: `ramp_grass`, `ramp_dirt`, `ramp_dirt_road` and `ramp_paved_road`, variations `<dir>_low` and `<dir>_high` (8 each). Each is a solid wedge whose flanks are rock, so a ramp beside lower ground shows its side and one beside another ramp or higher ground is covered by it, drawn later. Its top is the terrain's own sprite (grass, dirt, or the road's straight piece along the ramp: `1010` climbing north or south, `0101` east or west) mapped onto the slope by its diamond's corners, unlit and shaded by the slope's facing (`SLOPE_SHADE`), so flat it would render as the sprite does and the ramp matches the ground it joins (`height_pieces::RampSurface::of_terrain`). Its ends are open, so no ink where it meets the ground, its other half and the cell above. A ramp on a raised cell stands on the cliff courses of its drop below.
- **The terrain sprites' own edge** (about 4 px of side under each terrain sprite's diamond) shows along a cliff's top under the rim. Its ends make faint notches at each cell's corner along the lip, as along the map's outer border; new terrain art would remove them.
- **What Zeus does differently:** its cliff art is whole shaped tiles picked from the 8-neighbour pattern, some joined into 2 × 2 footprints (§11). That looks better at a cliff's corners and ends than stacked courses, at the price of a pattern table and art per pattern. Stage 2's mock-up showed the courses join well at outer and inner corners, so no corner or end pieces for now (Q15).

### 6. Pathfinding and units

- **The graph** stores a copy of each cell's height and shape, updated wherever `update_search_graph!` runs and on height edits.
- **`Graph::neighbors`** adds an edge test, worked out on the fly with no data per edge. A step from `a` to `b` is allowed if any of these holds:
  - both are flat at the same level;
  - they're a ramp's two halves, along its direction;
  - one is a ramp's low half (at `L`) and the other its downhill neighbour, flat at `L`, or a ramp's high half and its uphill neighbour, flat at `L + 1`;
  - in a chain, a ramp's high half (at `L`) and the next ramp's low half (at `L + 1`) above it, facing the same way.
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
- **Drawing:** units draw at the ground's height where they stand, and sort by the cell they're nearest (§3).

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
  - ramp tools (one per direction) that lay a two-cell ramp from the cell clicked (its low half) to the next cell up its way (its high half), at the clicked cell's level, a wider brush laying them side by side;
  - an overlay showing levels, with ramps that don't connect to the right levels flagged in red, and cliffs taller than `max_cliff_levels`.
- **Edits** go through a new `TilePlacement::set_elevation`, which:
  - updates the graph, the cliff cache, the projected-height lookup and the minimap;
  - calls `irrigation::network_changed` and the floods' reach refresh;
  - records undo. `TileSavedState` gains the cell's elevation byte.
- **Preset maps** get an optional elevation array (`debug/preset_maps.rs`).
- **Minimap:** the baker's noise elevation (`elevation_pass`) is replaced by the map's levels, smoothed, so hills shade from real heights, with darker cliff edges. The model's grid gains the level per cell. A flat map keeps the noise, so its look doesn't change.
- **Automation:**
  - `terrain.set_elevation cell=[x,y] level=2 [shape=ramp_n_low]` and `terrain.set_elevation from=[x,y] to=[x,y] level=N`;
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
- **For us:** not needed. The spike's picking tests the few dozen columns in front of the point directly, and it's exact (§4).

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
- [x] An `ElevationMap` that isn't saved; fill a test preset with a plateau, a two-level hill and a ramp.
- [x] Projection with height; the two-pass draw order; cliff faces as coloured quads.
- [x] Picking with height (by testing the columns in front of the point; no lookup grid).
- [x] Measure the frame time and draw calls on a 128 × 128 map with about 30% raised cells, at the widest zoom, on macOS OpenGL, Wgpu and the web build.
- [x] Screenshots of a building on a plateau, a building behind a cliff, a unit on a ramp and a tall building beside a two-level cliff, at 16 and 32 px, reviewed with the user.

Done when: the draw order is right in the screenshots, the frame time stays within budget (Q8), and the user picks a level height and agrees the look works.

**Notes (2026-11-22): a go, at 32 px.** The spike is commit `078d4abb` on `spike/terrain-height` (never merged): `tile/elevation.rs` (the run's height map, the programmer art, picking), the raised pass in `tile/rendering.rs`, the culling padding in `camera.rs`, height-aware picking in `TileMapLayer::find_exact_cell_for_point`, and `terrain.*` commands (`showcase`, `hills`, `set`, `step`, `rims`, `faces`, `walk`, `pick`) for scripted scenes.
- **Draw order:** right in every scene: a storage yard on a plateau's front corner, a temple behind a cliff, a granary and a 4 × 4 palace pressed against cliffs in front, a university beside a two-level block, trees on the rim, walkers behind, on and up a ramp, and a 128 × 128 map with hills and about 2,400 objects in view. It needed two rules the plan lacked: the ground under a building sorts with the building, and walkers sort by the cell they're nearest (§3).
- **Readability:** back edges vanish, so a walker or building behind a plateau looks sunk into the grass, correct but unexplained. A light rim along raised back edges fixes it (§5). Noise heights make jagged one-cell steps; maps want broad plateaus, so the editor's terrace and smooth brushes matter.
- **Picking:** exact for tops, faces, the cell in front of a cliff and the cell a raised top hides, by testing the columns in front of the point (§4). No lookup grid.
- **Cost** at the widest zoom (0.5), 1600 × 900, 30% of the cells raised up to level 3 (release builds, the user's Mac):

  | Scene | Flat | Hills | Notes |
  |---|---|---|---|
  | Sample map, ~200 objects in view, OpenGL, whole frame | 8.6 ms | 9.2 ms (+7%) | |
  | Dense, ~2,400 objects, OpenGL, CPU per frame without the vsync wait | 6.2 ms | 7.5 ms (+22%) | Texture switches 840 → 2,380; without the placeholder faces 1,800 |
  | Dense, Wgpu, map drawing + submit | 1.4 ms | 1.8 ms | |
  | Dense, web (Chromium, WebGPU), map drawing | 0.7 ms | 0.9 ms | |

  Every backend held 60 fps. The OpenGL rise is over Q8's 10%, but about half of it is the placeholder faces (untextured quads that break sprite batches) and most of the rest the mix of terrain and object sprites: the pieces go in the objects atlas (§5), and Stage 3 measures again. 16 and 32 px cost the same.
- **Level height:** both sort and pick correctly; 16 px reads as low terraces, 32 px as cliffs. The user picked 32 (Q1).

### Stage 1: Data, saves, editor and minimap (medium)

- [x] `ElevationMap` on `TileMap` (level and shape per cell), saved as runs; load-time validation.
- [x] Save version 15 and its migration (all flat); regenerate the sample saves; `--smoke-test-saves`.
- [x] `TilePlacement::set_elevation`, undo, the developer palette's Height section (raise, lower, terrace, smooth, ramps) and the validity overlay (levels tinted, bad ramps and tall cliffs flagged), which shows the heights until Stage 3 draws them.
- [x] The minimap from real heights; preset map support.
- [x] Automation: `terrain.set_elevation`, `terrain.elevation`, and the level in `inspect.at` and `world.building_at`.

**Notes (2026-11-22): done.**
- **Data:** `tile::elevation` ([tile/elevation.rs](../../crates/game/src/tile/elevation.rs)): `Dir`, `Shape`, `encode`/`decode` (the byte of §2), `ElevationMap` (its counts by top height kept at run time, `recount` on load), the heights (`corner_height`, `surface_height`, `centre_height`) Stage 3 needs, `problem_at` and `problems`, and the brushes as pure functions (`HeightTool`, `brush_cells`, `brush_edits`). `TileMap` saves it as runs through `crate::byte_runs`, moved out of `sim::floods` so fertility and heights share it. A reset map is flat; a load whose heights don't match the map's size loads flat with an error, and broken ramps or bad bytes are logged on the `terrain` channel (tall cliffs are allowed, only flagged in the overlay).
- **Edits:** `TileMap::set_elevation` keeps the minimap in step; `TilePlacement::set_elevation` refuses buildings, fields and blockers (Q6) and anything but flat 0 on water and canals (Q3), and records one undo step (`EditAction::Elevation`, a record of each cell's byte before and after). A brush stroke gathers its edits (`TilePlacement::paint_height`) until `menu::end_height_stroke`. Raise and Lower work to one level above or below where the stroke starts, so dragging doesn't stack; Terrace sets its level only on cells one level from it, as Zeus's brushes do; Smooth works through the brush's cells in order, each seeing the ones already changed, as Zeus's pass does; there's also Flatten (a ramp back to flat).
- **Editor:** the developer palette's Height section (`Tile Selection/Height/...`, a selectable button per tool, the brush's width, the Terrace level); a tool picked shows the heights overlay (`TileMapRenderFlags::DrawElevation`, also `Debug/Draw elevation`) and the brush's cells outlined under the cursor. The overlay is drawn on the flat diamonds until Stage 3 lifts them.
- **Minimap:** the bake's `Grid` takes each cell's centre height (`heights`, empty while the map is flat, so flat maps bake byte for byte as before); it raises the noise's value by `level_value` a level and adds the heights' slope to the shading by `level_slope`, so a cliff shades as a lit or shaded band. `minimap_bake`'s new test checks a patched bake with heights equals a full one and that a plateau's edges shade.
- **Presets:** an optional `elevation` array of bytes; none of the presets uses one yet.
- **Automation:** `terrain.elevation`, `terrain.set_elevation`, `terrain.brush`; `inspect.at`'s and `world.building_at`'s `level`.
- **Saves:** v15 (`v14_to_v15.py`, an all-flat `elevation` on each save's tile map); the local saves and `sample_saves.zip` migrated, `--smoke-test-saves` loads all 13.
- **Verified:** `cargo test --workspace` (the elevation unit tests, six new `terrain_placement` tests, the bake's), the 29 scenarios in release with the new `elevation.jsonl` (added to CI), determinism twice and the two record-and-replay runs, fmt and both clippies.

### Stage 2: The pieces, from Blender (small to medium; placeholder-art Phase 5)

Before the rendering, so Stage 3 draws real sprites (batched with the buildings) and measures their cost. Starts from the prototype recipe (§5).
- [x] `heights.py`: cliff courses (`top`, `mid`) for the left and right edges, with variations; ramps as solid wedges, 4 directions × grass, dirt, dirt road and paved road, in two halves; rims for all four sides. One shared level constant (√(2/3) units, from `LEVEL_STEP_PX`).
- [x] Seams: one mesh per course (no box ends), strata that repeat per level, the ramp's grass matched to the terrain's.
- [x] Catalog entries (`catalog/heights.json`), rendered and synced into the objects tile set; the renderer's lookups by name.
- [x] Contact sheet, and a mock-up composed as the engine draws them (stacked courses, corners, a ramp) reviewed before Stage 3; corner and end pieces if Q15 asks for them.

**Notes (2026-11-22): done.**
- **Ramps over two cells.** The first renders showed ramps climbing south or east with no top: one level in one cell (39°) is steeper than the camera (30°), so such a ramp faces away from the camera. Ramps now climb one level over two cells (§1), as two halves: `Shape::Ramp(Dir, Half)`, bit 6 of a cell's byte the high half. `ElevationProblem`'s ramp checks, the heights (`corner_height`, `surface_height`, `centre_height`: a half's middle is a quarter or three quarters of a level up) and the brushes followed: the Ramp tools lay a foot and a head (`brush_cells`, `brush_edits` now take the brush's centre and width), and the shape names are `ramp_<dir>_low` and `ramp_<dir>_high`. Save v15's format is unchanged: no save had a ramp.
- **Rims on every side** (the user's request): a lip with an inked edge wherever a raised cell's ground drops, back edges too, so a terrace's whole edge reads apart from the ground below.
- **The pieces:** `catalog/heights.json`, 46 sprites in the objects tile set's `overlays` category: `cliff_left` and `cliff_right` (`top_a`, `top_b`, `top_dirt`, `mid_a`, `mid_b`, 64 × 53), `cliff_rim` (`n`, `e`, `s`, `w`, 64 × 35) and four ramp surfaces in eight halves each (64 × 67). The ramps' tops sample the terrain sprites, which the build doesn't hash (re-render with `--force` after changing them). The checks warn of the rock's low saturation (grey stone), and of the courses and rims reaching past the frame's side corners (`OVERLAP`, clipped there by design).
- **Lookups:** `tile::height_pieces` (`cliff_course`, `rim`, `ramp`, `RampSurface::of_terrain`, `has_dirt_lip`, `all_piece_names`); `terrain_placement`'s `test_height_pieces_exist` checks every piece the renderer can ask for is in the tile sets.
- **Mock-up:** composited from the sprites in `assets/` as the engine will draw them (the session's `mock3.py`): a plateau with a two-level block, cliff courses stacked over two levels, rims on every dropping edge, and ramps climbing all four ways, the two towards the viewer seen at a shallow angle. Courses join along edges and at outer and inner corners, so no corner pieces (Q15).
- **Verified:** the generator's unit tests and `sync_tileset.py --check`; `cargo test --workspace` (the elevation unit tests for halves, `terrain_placement`'s 18); the 29 scenarios in release (`elevation.jsonl` with a two-cell ramp); fmt and both clippies.

### Stage 3: Rendering (medium to large)

- [ ] Lifts as things are drawn (§3), from the spike: terrain, objects, walkers at the ground's height where they stand, and the overlays drawn with a tile.
- [ ] The two-pass draw order (§3), with its tie rules (terrain first, the ground under a building with the building, walkers by their nearest cell) and the hidden-ground gate; cliff courses, ramp halves and rims from Stage 2's sprites (`tile::height_pieces`).
- [ ] Washes and previews on raised cells.
- [ ] Culling padding, camera bounds, `teleport`.
- [ ] Overlays: grid, search-graph debug view, hover highlight, previews, tile bounds.
- [ ] Measure the dense scene again (Stage 0's script) and decide whether raised tops join the objects atlas.

### Stage 4: Picking and placement (medium)

- [ ] Picking with height (§4) in every function listed there, and in automation's cell-to-screen conversion.
- [ ] Placement rules (§7) with new error reasons; road ramps and junctions across cliffs; fields, dykes, canals.
- [ ] Tests in `terrain_placement` for each rule.

### Stage 5: Pathfinding and units (small to medium)

- [ ] Height in the graph, the edge test in `neighbors`, the ramp cost; repath when a cliff blocks the way.
- [ ] Tests in `search_graph`: a cliff blocks, a ramp allows, a ramp chain climbs several levels, a responder takes the ramp.
- [ ] The `elevation.jsonl` scenario, added to CI.

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
| Mixing terrain and object atlases in pass 2 costs batching | Measured in the spike (Stage 0's notes): the pieces go in the objects atlas; Stage 3 measures whether raised tops follow |
| Picking or placement bugs where heights change | Each rule has a unit test, and the scenario runs in CI |
| Grid-based systems giving odd results across cliffs (§10) | The table in §10 names each one; the scenario covers the flood and the fields |
| Stacked faces look blocky at cliff corners | Corner and end pieces, Zeus's way (Q15) |
| 32 px a level looks wrong in play | It's one constant (Q1); the pieces read it too, so changing it is a re-render |

## Open decisions

**Q1. How tall is one level, in logical px?**
**Decided (2026-11-22): 32**, one tile height, as Zeus's levels are (§11), from the spike's screenshots of both.
- 16 was decided on 2026-11-05, on the grounds that it kept draw order and picking exact. The refresh found that draw order is exact at any height, and the spike drew both at the same cost: 16 px reads as low terraces, 32 px as cliffs.
- At 32 px a raised cell lands exactly on one flat diamond, and the camera and culling padding is twice 16 px's.

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
*Default: no worse than 10% over the same map flat,* on the user's Mac at the widest zoom, on OpenGL and Wgpu. The spike met it on the sample map (+7%) and held 60 fps everywhere, but a dense hilly map cost OpenGL 22% more CPU, half of it the placeholder faces (Stage 0's notes); Stage 3 measures again with the real pieces.

**Q9. Do reaches measured along the grid stop at cliffs?**
Services' coverage, a farmstead's field reach, faith's coverage, wells' water, irrigation's spread and risks' spread are measured in grid steps today (§9, §10).
*Default:* a farmstead's reach and irrigation stop at cliffs (a field belongs to the land its farmers walk, and water doesn't run uphill). The rest ignore height until playtesting says otherwise.

**Q10. What can stand on raised ground?**
*Default:*
- buildings, houses, roads, fields, rubble, trees and rocks on any flat level;
- dykes on any flat level, joined only on one level, with no dyke ramps;
- canals, docks, wharves and water at level 0 only.

**Q11. How high do floods reach?**
*Default:* only level-0 land floods (`flood_level` 0), set per mission. A severe flood rising a level is left to M13's balance.

**Q12. Are ramps stored or worked out from heights?**
Zeus works its slopes out from the neighbours (§11).
*Default: stored*, since a ramp is a chokepoint and a road's way up that the designer places. The editor's overlay flags ramps that don't connect, and could suggest ramps later. A ramp is two cells long (§1, decided in Stage 2).

**Q13. How much can neighbours differ?**
Zeus keeps them within 2 levels (§11). Stacked faces draw any drop.
*Default:* any drop is allowed; the smooth brush and the overlay use `max_cliff_levels` = 2.

**Q14. Stop saving the cached tile positions?**
**Decided by the spike (2026-11-22): no, not for height.** Positions stay flat and the renderer lifts each thing as it draws it (§2, §3), so no cached position can disagree with the heights. Dropping `iso_coords_f32` from saves would still shrink them a little, but that's a save-size change of its own, not this milestone's.

**Q15. Stacked faces or Zeus-style pattern pieces?**
**Decided (Stage 2, 2026-11-22): stacked courses.** Stage 2's mock-up of the real pieces joins well at outer and inner corners; corner and end pieces can follow if play shows seams that matter.
