# Placeholder art pipeline

**Status (2026-11-05): Phases 0–2 done; Phase 3 next.** The pipeline is set up: the `hbgen` package, the engine camera, the catalog, `tilegen.sh` (render, check, sync `tile_set.json`, refresh the web asset list) and a CI check. The first generated building, the roof-tile kiln, is in the game. The generator's [README](../../crates/tools/blender-tile-generator/README.md) is the user guide. This plan turns the Blender tile generator in `crates/tools/blender-tile-generator/` (`hb_tile_gen.py` and its README) into the game's source of placeholder art. It renders every building, terrain tile, overlay and prop that the design calls for and the hand-made art doesn't cover yet, and writes them into `assets/tiles/` with their `tile_set.json` entries. It is milestone M2 of the [roadmap](roadmap.md), and runs alongside the gameplay milestones. It was researched against `d0696fe` on `main`, plus the generator that commit `d414cc1` added to the repo.

## How to resume this work

- Work through the [phases](#phases) in order. Tick the checkboxes and add dated notes under each phase as tasks land.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise.
- **Never overwrite hand-made art.** The generator writes only the sprites the catalog marks as generated (§4).
- The generator runs on the Mac with Blender 5.x: `crates/tools/blender-tile-generator/tilegen.sh [TILE ...]` for the catalog, or through the Blender MCP server, which the desktop app has connected. Cloud sessions can edit the scripts and the catalog, and run `sync_tileset.py`, `validate.py` and the unit tests, but can't render.
- `tilegen.sh` refreshes `web/asset_manifest.json` (`cargo run -p web-builder -- manifest`); commit it with the sprites. The desktop atlas cache repacks by itself (see [terrain-tiles.md](../../.claude/rules/terrain-tiles.md)).

## Goals

1. **A complete placeholder set** for Campaign I, missions 1–5, then for later milestones. Placeholders are judged by how readable they are, not how finished they look.
2. **One command** renders a catalog of tiles and keeps the game's `tile_set.json` in step, adding entries and appending variations, never reordering.
3. **One look:** the tuned v7 lighting, outlines and grade from the README, at a fixed pixel density. The palette per realm is the only thing that changes from realm to realm.
4. **Terrain too:** base land tiles, and transitions (shores, cliffs, ramps) generated from neighbour masks. This also supplies the cliff and ramp art for [terrain-height.md](terrain-height.md).
5. **Automatic checks:** size, anchor and brightness checks run outside Blender, plus an in-game contact-sheet scenario.

## What exists today

### The generator (`hb_tile_gen.py`, 630 lines; README; in git since `d414cc1`)

- **Pieces:**
  - `PALETTES` for `viet`, `burma` and `khmer`, as sRGB hex values converted to linear before use;
  - `Materials`: procedural brick, wood, noise, roof and water, all in object coordinates, with no UVs;
  - `Builder` primitives: box, frustum, pagoda_roof, lantern, tree, cone, prang, shikhara, ground_diamond;
  - three fortress recipes, built on a shared `enclosure()` and `halls()`;
  - scene setup: camera, lighting, Eevee, Freestyle outlines, compositor grade;
  - `trim_top()` and the `generate()` and `_cli()` entry points.
- **Camera:** orthographic, rotated (60°, 0, 45°), at a fixed position (18.4, −18.4, 15). The output width is `cells × 64 × scale`, and `ortho_scale = G·√2`.
  - `shift_y` puts the **front corner of the earth slab** (z = −0.45) **6 px above** the frame bottom.
  - `cells` (image size) and `G` (the size the recipe returns) are separate arguments that have to agree.
- **Only fortresses so far.** There are no terrain recipes and no generic building recipes, and there is no batch mode. Outputs are written by hand to a path given on the command line.

### The game's sprite format

See `tile/sets.rs` and `assets/tiles/*/tile_set.json`.

- **Paths:** `assets/tiles/<layer>/<category>/<tile>/[<variation>/]<frame>.png`. A tile with one unnamed variation has no variation folder.
- **Footprint:** `logical_size`, a multiple of 64×32 (checked at `sets.rs:813`). Terrain must be exactly 64×32 (`sets.rs:826`).
- **`draw_size`** is the on-screen size in logical pixels, and the PNG is scaled to it. **No check ties PNG pixels to the footprint.**
- **Today's art is high-resolution and its density varies:**
  - terrain is 369×237 drawn at 65×36, about 5.7× logical;
  - 1×1 buildings are about 900 px wide drawn at 64–72, about 13×;
  - 3×3 buildings are about 1000 px drawn at 192, about 5×;
  - units are about 340×650 drawn at 22×40.
- **Anchor:**
  - a building's sprite **bottom edge** sits on the footprint's bottom vertex (`calc_object_iso_coords`, `tile/mod.rs:615`: `y -= draw_h - 32`);
  - terrain draws at `cell_to_iso(cell)` with a few pixels of side face below the diamond (65×36: 32 px of diamond plus about 4 px of side);
  - `iso_offset` per variation adjusts the position.
- **Saves:**
  - tile defs are found by the hash of their category and name;
  - variations are stored **by index**, so new variations are appended and never reordered;
  - frame names are limited to 24 characters (`ArrayString<24>`).
- **Packing:** the atlas packs images at source resolution on 4096² pages (`atlas.rs:711`). Terrain uses nearest filtering without mipmaps; objects use the global texture settings.
- **The game never edits `tile_set.json`.** New sprites need entries written by a tool or by hand.

### `crates/tools/water-transitions/compose.py`

This script uses the standard library only.

**Reusable parts:**
- PNG input and output;
- diamond geometry helpers (`cell_coords` and `pixel_coords`, currently fixed to 369×236);
- `Layer` shift, clip and bend;
- the chamfer distance field;
- score-based compositing with a cross-fade;
- side-face recolouring;
- the variation naming scheme that matches `game::tile::water`.

**Water-specific parts:** the colour heuristics ("landness" is red minus blue), the scores and the recipe table.

## Design

### 1. Repository layout

The generator moves into git and is split into modules. Blender runs one entry script, which puts the package folder on `sys.path`.

```
crates/tools/blender-tile-generator/
  README.md            pipeline doc (existing, updated)
  hb_tile_gen.py       entry point: CLI + generate(); unchanged call signature
  hbgen/
    palettes.py        PALETTES, LIGHTING, OUTLINE, GRADE, EEVEE
    materials.py       Materials
    builder.py         Builder primitives (+ new: gable_roof, stilts, fence, crates, field_rows, scaffold)
    scene.py           clear_scene, setup_camera/lighting/render/outlines/grade, trim
    recipes/
      fortress.py      the three existing fortress recipes
      generic.py       parametric recipes (house, hall, workshop, yard, field, well, shrine, scaffold, rubble)
      terrain.py       terrain slab + mask-driven transitions, cliffs, ramps
    catalog.py         loads catalog/*.json
  catalog/
    buildings_daiLong.json   one entry per building sprite to generate
    terrain.json             terrain tiles and transitions
    overlays.json            scaffolds, renovation overlays, rubble
  build.py             batch driver (runs inside Blender): renders catalog entries, caches by hash
  sync_tileset.py      plain Python: merges catalog entries into assets/tiles/*/tile_set.json
  validate.py          plain Python: size/anchor/brightness checks on the PNGs (README §7)
  reference/           the reference .blend and .png files (committed)
```

### 2. Fitting the game's format: camera, anchor and density

- **Fixed density** (Q1): render at **8 px per logical px** (first planned as 4), so 512 px per cell of width. Then:
  - a 1×1 building is 512 px wide and a 5×5 is 2560;
  - a terrain tile is 512×256 plus its side faces;
  - `draw_size` = PNG size ÷ 8, so the width is exactly `cells × 64` and the height is whatever the model needs.

  This is about the density of today's 2×2 art (the 3×3 and terrain art are 5–6×). It holds up next to the hand-made art at high zoom, it fits the 4096² atlas pages, and it gives one rule for every tile.
- **Bottom anchor:** the bottom vertex of the footprint diamond, at **z = 0**, goes **exactly on the bottom edge of the frame**. That matches `y -= draw_h - 32`.
  - Today's 6 px pad on the earth slab's front corner becomes 0, measured at z = 0.
  - Anything below ground, such as an earth slab or a moat, is cut off. Recipes that need visible depth (moats, wharves) report a `depth_px`, and the catalog turns it into an `iso_offset`.
- **One size argument:** `setup_camera` takes `G` from the recipe, so `cells` disappears. The camera position moves out with the size, so tall or large models don't get clipped by the near plane.
- **Trimming:** `trim_top` stays (crop the empty rows above, keep the width).
- **Oversampling** (later, optional): render at 16× and downsample to 8× with a box filter in `build.py`, for cleaner outlines.

### 3. Recipes

**Generic parametric recipes:** most placeholders are combinations of a few parametric recipes with a palette, so one recipe covers many buildings.

| Recipe | Parameters | Covers |
|---|---|---|
| `house(level, size, style)` | storeys, roof type (thatch, tile, tiered), stilts, veranda, lanterns | 6 common levels, elite estates, and Kambara stilt houses later |
| `hall(size, roof_tiers, columns)` | open sides, plinth height | communal hall, tax office, temple, academy, market hall |
| `workshop(size, kiln, chimney, yard_props)` | kiln or furnace, stacks of goods | pottery kiln, brick maker, roof-tile kiln, weapons workshop, distillery |
| `yard(size, fence, props)` | crates, sacks, jars, timber stacks | storage yard, granary (with silos), trade post, lumberyard |
| `field(size, crop, water)` | crop rows, paddy water, bunds | rice paddy, mulberry field, vegetable plots |
| `well(kind)`, `shrine(kind)`, `pond`, `statue`, `garden(size)` | — | small services and decoration |
| `linear(kind, mask)` | 4-bit junction mask, like roads | dykes, palisades, walls (later), canals |
| `scaffold(size, stage)` | 3 stages | construction sites ([construction.md](construction.md) §6) |
| `renovation(size)` | — | overlay on a house that's renovating |
| `rubble(size)` | — | ruins after fire, collapse or attack (M6) |

The fortress recipes stay, as the reference for the look.

**Terrain recipes:**
- `terrain_slab(kind, seed)`: one cell, 64×32 logical, with the 4–5 px side face the current tiles have.
  - Kinds: grass, dirt, vacant lot, dirt road, paved road, farmland and floodplain mud.
  - Different seeds give the random variations.
- `terrain_mask(kind, mask8)`: a transition from an 8-neighbour mask, such as a shore, a grass-to-dirt blend or a floodplain edge.
  - The geometry is built from the mask directly: a land polygon, the water plane, a dirt band.
  - This replaces image compositing for new transition families (Q3).
  - The water shores keep their hand-made set and `compose.py`, unless the user wants them regenerated.
- `cliff_face(edge, levels)` and `ramp(dir, kind)`: the pieces [terrain-height.md](terrain-height.md) needs.
  - One level of cliff face per visible edge (left and right), stacked in the engine.
  - Ramps in 4 directions for grass, dirt and road.
  - The height of one level comes from the engine's step constant.

### 4. The catalog and syncing `tile_set.json`

Each catalog entry describes one tile def, with everything needed to render it and to write its `tile_set.json` entry:

```json
{
  "tile": "pottery_kiln", "layer": "Objects", "category": "buildings",
  "recipe": "workshop", "params": { "size": 2, "kiln": "bottle", "yard_props": ["jars"] },
  "palette": "viet",
  "variations": [ { "name": "var0", "seed": 1 }, { "name": "var1", "seed": 2 } ],
  "tile_set": { "sector": "Industry", "logical_size": [128, 64], "cost": 12 },
  "generated": true
}
```

**`build.py`** runs inside one Blender process and renders every entry, or only the ones named on the command line:
- It writes `assets/tiles/<layer>/<category>/<tile>/<variation>/<tile>.png`.
- It takes `draw_size` from the render (PNG size ÷ density; the engine camera sizes the frame to whole logical px).
- It writes a sidecar `build/manifest.json` recording each sprite's size and its recipe hash, which covers the parameters, palette, recipe source and generator version. Unchanged entries are skipped.

**`sync_tileset.py`** is plain Python and needs no Blender:
- It reads the manifest and the catalog, then updates `tile_set.json`:
  - it adds missing tile defs to the right category;
  - it sets `draw_size` and `logical_size`;
  - it appends any new variation names to the end;
  - it never removes or reorders anything.
- An existing def without `"generated": true` in the catalog is left alone, so hand-made art always wins.
- It writes the JSON in a fixed format with a stable key order, so diffs stay small.
- `--check` reports drift without writing, for CI.

**Gameplay configs:** `buildings/configs.json` and `campaigns.json` aren't touched. Adding the gameplay side of a building is the gameplay milestone's job; the catalog only supplies the art and the tile def.

**Realms:** catalogs are per realm (`buildings_daiLong.json`). Until the realm layer exists (roadmap Phase 3), only Đại Long is synced into the game. How realm-specific tile defs are named and stored is Q4.

### 5. Checks

- **`validate.py`** checks every PNG in the manifest against README §7:
  - the width is exactly `cells × 64 × 4`;
  - the opaque columns span the full width, for tiles with a ground diamond;
  - the bottom anchor row sits at the bottom;
  - the median luminance is about 0.29 and the median saturation about 0.46, within a tolerance, per palette.
  - It writes a contact sheet, `build/contact_<catalog>.png`, of every sprite over a checkerboard.
- **In the game:** a scenario, `crates/game/tests/scenarios/art_gallery.jsonl`, starts an empty map, places every generated building in a grid with `world.place`, and takes screenshots with and without the UI. It isn't in CI, because CI has no goldens; it's a manual review aid.
- **CI:** `sync_tileset.py --check` runs as a step in the desktop job, so a catalog edit without a sync fails.

### 6. Performance

The README's settings (Eevee at 96 samples plus Freestyle) take a few seconds per sprite at 1×. The 2×2 roof-tile kiln takes about 7 s at 8×, Blender's start-up included.
- A Phase 1 catalog of about 120 sprites, counting variations, scaffolds and terrain, takes about 10–20 minutes for a full render. After that, the hash cache means only changed entries are rendered.
- If that turns out too slow, `build.py --fast` renders at 32 samples for iteration.

## Phases

### Phase 0: Bring the generator into the repo (small)

- [x] Commit `hb_tile_gen.py`, its README and `reference/` under `crates/tools/blender-tile-generator/`. Done by the user in `d414cc1` (2026-11-05), with the reference `.blend` files included, which answers Q5.
- [x] Split into the `hbgen/` package (§1). The `generate()` and command-line options keep working, and the three fortresses render identically: compare checksums, or `validate.py` statistics if Blender isn't deterministic.
- [x] Add a line to AGENTS.md's workspace layout and README instructions for running headless and through the Blender MCP server. The README already had both; it now also covers engine renders (§10).

Done when: `Blender -b -P hb_tile_gen.py -- --building fortress --palette viet --out x.png` produces the same image as before.

Notes (2026-11-05):
- Done after Phase 1 had started, so the split carried the engine camera and the kiln along. The fortress renders pixel for pixel as before the split, and the kiln within 1/255 on a few pixels (Eevee noise). Blender renders the same scene identically run to run.
- Layout as in §1, with these differences:
  - recipes are `fortress.py`, `workshop.py` (the kiln) and `test.py`, a minimal 1×1 template and pipeline test; `generic.py` and `terrain.py` come with Phases 3 and 4;
  - plain-Python modules joined the package: `catalog.py`, `tileset.py` (the sync), `checks.py` and `pngio.py` (a standard-library PNG reader and writer). `hbgen/__init__.py` imports nothing, so they load without Blender;
  - the manifest is `catalog/manifest.json`, committed, not `build/manifest.json`: CI checks the tile sets against it without Blender. `build/` holds only the contact sheets and is git-ignored;
  - `tilegen.sh` wraps `build.py` and the web asset list; `tests/test_pipeline.py` holds the unit tests.
- `hb_tile_gen.py` keeps its command line and gains `--seed` and `--params`. In a live Blender, `exec` it after setting `HB_TILE_GEN_DIR`; each `exec` drops the cached `hbgen` modules, so edits take effect without restarting.

### Phase 1: Fitting the engine and the checks (small to medium)

- [x] Camera: a single size argument `G`; the camera position scales with size; the bottom anchor at z = 0 on the frame's bottom edge (§2).
- [x] The density constant, 8 px per logical px (Q1), with `draw_size` computed from it.
- [x] `validate.py` and the contact sheet.
- [x] Re-render one existing building (for example `market`, 2×2) as a test. Swap it in locally and check its placement against the grid in the game: no gap or float at the bottom vertex, and correct overlap with neighbours. Done with a new building instead, see the notes.

Notes (2026-11-05):
- `setup_camera_engine` in `hb_tile_gen.py` frames a tile the way `calc_object_iso_coords` draws it. The PNG is `G × 64 × density` wide, and the footprint's front corner at z = 0 is the bottom-centre of the frame. The camera sits on the line through that corner, with `sensor_fit` horizontal and no shift. The height is measured from the model's vertices and rounded up to whole logical px, plus 2 px, so no trimming is needed and `draw_size` is exactly the PNG size ÷ density. A model wider than the diamond logs a warning, since the frame would clip it.
- `generate_engine()` and `--density` render in this format; the fortress recipes keep the old camera (`LEGACY_CAMERA`), and `fortress` renders pixel for pixel as before.
- **Outlines:** Freestyle multiplies the scene's line thickness by the line style's, so the legacy renders have 2.56 px lines. Engine renders scale only the style's thickness, by `density × 0.4`, about 1 logical px. The noise and jitter stay at their 1× size, which keeps the lines crisp; scaled up with the density, they turned into a soft brown halo.
- **First building:** `roof_tile_kiln`, a 2×2 recipe (`build_roof_tile_kiln`) for M5. It has a brick bottle kiln with a chimney, an open drying shed with racks and a grey-brown tile roof, tile stacks on pallets, jars, a clay heap and a bush. Its new primitives are `cylinder`, `dome`, `gable_roof`, `jar` and `tile_stack`. Its new materials are brick, tiles, jar, yard, clay, plaster and an emissive fire, plus gable-roof tiles with columns (`roof(cols_axis=...)`). Palettes lacking the new keys borrow the viet values.
- **In the game:** the sprite is 1024×760, drawn at 128×95, and the pottery kiln is 126×100. A local test config placed two of them beside the pottery kiln, brick maker, market, a house and a well. Their footprints meet the neighbours' with no gap or overlap at zoom 2 and zoom 5. The pixel checks agree: the diamond fills columns 0 to 1023, the bottom vertex is on the bottom row at the centre, and the side corners are at a quarter of the width.
- **Tile def:** `roof_tile_kiln` is in `assets/tiles/objects/tile_set.json`, entered by hand (until Phase 2's `sync_tileset.py`), with no `sector`, so the build menu leaves it out. M5 adds its good, its producer config and its sector (known-issues).
- **Look:** at a glance it sits with the hand-made workshops. Close up it reads as a clean 3D render beside painted art, with less clutter, larger shapes and a darker shed interior. That's fine for a placeholder; the user reviews it with the Phase 3 set.

Done when: a generated 2×2 sprite sits on its footprint exactly, at every zoom level.

Notes on the checks (2026-11-05):
- `validate.py` checks width, height, an empty top row, the side corners and the bottom row (README §11). It writes `build/contact_<catalog>.png` at 2 px per logical px, bottoms aligned, with a red line under each sprite.
- The README's brightness targets turned out to be luma of the gamma-encoded values (Blender's `image.pixels` returns stored bytes for a PNG), not linear luminance. On that scale the fortress measures 0.29, the kiln 0.26, and the hand-made pottery kiln and brick maker 0.29 and 0.35. The targets are 0.29 ± 0.08 luma and 0.48 ± 0.15 saturation, and a miss is only a warning.
- The corner check looks at the outermost 2 columns. A wider band failed the 1×1 test hut, because the diamond's lower edge drops 1 px per 2 columns on top of the ink.

### Phase 2: The catalog, the batch driver and syncing the tile set (medium)

- [x] The catalog format (§4) and `catalog.py`.
- [x] `build.py`: batch render, manifest, hash cache, `--only NAME`, `--fast`.
- [x] `sync_tileset.py`: merge, append-only variations, stable formatting, `--check`.
- [x] A CI step for `sync_tileset.py --check`.
- [x] A round trip: catalog a new test building, render, sync, load the game, place it with `world.place`, and check the screenshot.

Done when: one command adds a new building's art and tile def, and the game loads it.

Notes (2026-11-05):
- **Catalog:** `catalog/buildings_daiLong.json`, with the roof-tile kiln. Per-file defaults for `palette` and `density`. An entry is validated on load: unknown keys, duplicate tiles, names over 24 characters, and an unnamed variation among named ones are errors.
- **build.py:** tile names are positional rather than `--only`. Also `--force`, `--dry-run`, `--no-sync`, `--no-validate` and `--catalog FILE`.
  - **Hash:** it covers the entry's settings, the core modules, the recipe's own module, the forced height, `--fast` and `GENERATOR_VERSION`. The recipe registry isn't hashed, so registering a recipe doesn't re-render the others.
  - **Variations:** they share the tallest one's height, since a tile def has one `draw_size`.
  - **Hand-made art:** a sprite file the manifest doesn't know counts as hand-made and isn't overwritten; `--adopt TILE` takes one over, as the kiln's first build did.
  - **Fast renders:** `--fast` renders are recorded, and the sync check fails on them until they're redone.
- **Sync:** the objects tile set is hand-formatted (inline `materials`, `"x" : 2`), and re-serialising it would change about 1100 lines. So the sync parses the file with text spans and replaces only the text of the defs it owns, or appends new ones to their category. A def whose existing art the generator didn't write is refused. Besides the defs, `--check` checks each sprite's file, recorded size, `--fast` flag and listing in `web/asset_manifest.json`. A hand-corrupted kiln def was caught, and a sync restored the file byte for byte.
- **Web:** `web-builder` has a `manifest` mode that writes `web/asset_manifest.json` without building WASM. `tilegen.sh` runs it after a build, then `sync_tileset.py --check`.
- **CI:** the Linux desktop job runs `sync_tileset.py --check` and the unit tests (13: catalog, spans over the real tile set, sync rules, checks, PNG round trip) before the Rust build.
- **Round trip:** the kiln's sprite, manifest record and tile def were deleted, and a `test_hut` (1×1, two seeded variations) was catalogued. One `tilegen.sh` rendered all three, with the hut's variations at one forced height of 456 px. It restored the kiln's def byte for byte, appended the hut's, and listed the sprites for the web build. With local building configs, `world.place` put the kiln and eight huts beside hand-made buildings, and `tile.at` reported both hut variations in use. The footprints meet their neighbours'. The hut and the local configs were then removed; `test_hut` stays as a recipe, not in the catalog.
- Not done: the in-game gallery scenario (§5) needs building configs for the generated tiles, which their gameplay milestones add. It moves to Phase 3, with the review.

### Phase 3: Generic recipes and the Phase 1 building set (medium to large)

- [ ] New `Builder` primitives: gable_roof, thatch, stilts, fence, crates, jars, timber_stack, field_rows, scaffold_poles.
- [ ] Generic recipes (§3): house, hall, workshop, yard, field, well, shrine, garden, statue, pond.
- [ ] `buildings_daiLong.json` for the buildings that Campaign I, missions 1–5 need and that have no hand-made art yet. The roadmap's M5–M10 list them:
  - roof-tile kiln, communal hall, fire watch, builders' guild, festival square;
  - dyke, with junction variations, and sluice gate;
  - trade post, mulberry field, rice paddy (if it becomes a terrain-like building), common house level 5;
  - statue and pond decorations.
- [ ] Overlays: `scaffold` for 1×1 to 5×5 in 3 stages, `renovation` for each house size, and `rubble` for 1×1 to 5×5.
- [ ] `crates/game/tests/scenarios/art_gallery.jsonl` (§5): places every generated building that has a building config, with screenshots. Moved here from Phase 2.
- [ ] Review the contact sheets and the in-game gallery with the user.

Done when: every building in the Phase 1 build menus has art, either hand-made or generated.

### Phase 4: Terrain tiles and transitions (medium)

- [ ] `terrain_slab`, matched to today's terrain tiles: diamond plus side face, the 65×36 `draw_size` convention, nearest filtering. Check that a generated grass tile tiles seamlessly next to the hand-made grass.
- [ ] Generated variations for the new land types: farmland, floodplain mud, and dyke foundations if needed.
- [ ] `terrain_mask` for the 8-neighbour transition families the design needs: floodplain edges, and grass to dirt if wanted. The names and the order follow the same scheme as `water.rs` (§3), and new families get their own engine selection code, as water has.
- [ ] Turn `compose.py`'s PNG input and output and its geometry helpers into a small shared module, with W, H and the diamond as parameters, so later image-based composing can reuse them.

Done when: the new terrain types tile cleanly with the existing ones in the game.

### Phase 5: Height pieces (small to medium; scheduled with terrain-height Stage 5)

- [ ] `cliff_face(edge, levels=1)` for the left and right edges, per terrain kind or neutral rock.
- [ ] `ramp(dir, kind)` for grass, dirt and road, in 4 directions.
- [ ] The level height comes from the engine's constant (see [terrain-height.md](terrain-height.md)); both read it from one shared JSON value.

### Phase 6: Walker experiment (medium)

**Goal:** find out whether Blender can make walker sprites that look right next to the hand-made ones. It starts with the N, S and E directions that 8-way movement needs ([unit-movement.md](unit-movement.md)). If it works, Blender becomes the source for new walkers: militia, raiders, caravans.

Approach:
- **Recipes:** a low-poly `figure`, built from `Builder` primitives (head, torso, capsule limbs), with simple parenting or an armature. Parameters: build, clothing colours from the palette, headwear (a conical hat), and things carried (a shoulder pole with baskets, a scroll, a spear, a staff). Plus a `quadruped` recipe for the dog and the buffalo.
- **Walk cycle:** keyframed procedurally: legs and arms swing on a sine, and the body bobs. Frame counts match today's units (8 walk frames and 2 idle for people).
- **Directions:** turn the figure in 45° steps under the fixed isometric camera. Render 5 directions (N, NE, E, SE, S) and mirror the other 3 in `tile_set.json` (`mirror: true`), as the hand-made units already do.
- **Size and anchor:** match the existing unit draw sizes (about 22×40 logical for people) and the feet anchor (`y -= draw_h - 16`), at the 8× density. Outlines need to be thinner for small figures; the 1.6 px used for buildings may be too heavy.
- **Output:** `assets/tiles/objects/units/<unit>/<anim_set>/frameN.png` with `frames_source: {"Files": n}`. Catalog entries of a new `unit` kind; `sync_tileset.py` adds the anim sets.

Tasks:
- [ ] The `figure` and `quadruped` recipes, the walk cycle, rendering each direction, and frame-sequence output.
- [ ] Settler test: render all 8 directions (5 plus mirrors) and place it next to the hand-made settler in the game. Compare at 1× and at full zoom.
- [ ] Decide with the user whether to:
  - generate only the missing N, S and E for existing units;
  - regenerate every direction for a unit;
  - or draw them by hand instead.

  Record the answer in [unit-movement.md](unit-movement.md) Q5.
- [ ] If Blender works out: generate N, S and E for the settler, dog, buffalo and bird ([unit-movement.md](unit-movement.md) Phase 4).

Done when: the user has judged the settler test and the decision is recorded.

### Later

- Palettes for Kambara and Tampadesa, and the `khmer` and `burma` palettes tuned against references (roadmap Phases 3 and 4).
- Monument stages, as a separate render for each stage.
- New walkers for Phase 2 (militia, raiders, caravans) in all 8 directions, if the Phase 6 experiment works out.

## Risks

| Risk | Mitigation |
|---|---|
| Generated art looks unlike the hand-made art beside it | Fixed density and the v7 look; contact sheets reviewed with the user; generated tiles are always placeholders that hand-made art can replace |
| `sync_tileset.py` reorders or deletes variations, which breaks saves | Append only; existing defs are never removed; `--check` in CI; a test round-trips the real `tile_set.json` with no changes |
| Blender versions differ between machines | The README's 5.x notes; pin the version in the README; `validate.py` statistics catch drift in the look |
| Nearest filtering on terrain at high source resolution shimmers when zoomed out | Already true of today's terrain art. Mipmaps for terrain pages are a renderer question, outside this plan |
| The atlas fills up as sprites grow | 8× density puts a 5×5 at 2560 px wide, inside a 4096² page, at about four times the area of a 4× sprite; watch the page count in `atlas_meta.json`, and drop to `--density 6` if pages pile up |
| Renders only run on the Mac | Rendered PNGs are committed, so cloud sessions and CI never need Blender |

## Open decisions

**Q1. What pixel density should sprites be rendered at?**
*Default: 4 px per logical px* (256 px per cell). The alternative is to match each existing size, which varies 5–13×.
**Decided (2026-11-05): 8 px per logical px.** The user approved the kiln's 1024×760 render. At 4× the roof-tile kiln was visibly blurrier than the hand-made 2×2 workshops beside it (about 7.6×) from zoom 5 up, and the camera zooms to 10×. At 8×, a 2×2 is 1024 px wide and a 5×5 is 2560, which still fits the 4096² atlas pages. `--density 4` still works.

**Q2. Should the bottom anchor sit exactly at the frame's bottom edge, at z = 0?**
*Default: yes,* to match `calc_object_iso_coords`. Anything below ground goes through `iso_offset`.

**Q3. How should new terrain transitions be made?**
*Default: render them from masks in Blender.* Water shores keep their hand-made set and `compose.py`.

**Q4. How should realm-specific art be named and stored?**
*Default: deferred to roadmap Phase 3.* Until then the catalog writes Đại Long art under today's names.

**Q5. Should the `reference/*.blend` files (about 250 KB each) be committed?**
**Decided (2026-11-05):** yes. The user committed them with the generator in `d414cc1`.
