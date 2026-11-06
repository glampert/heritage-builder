# Heritage Builder — Blender tile pipeline

Placeholder sprites for the game, rendered in Blender 5.x from procedurally
built 3D models at the engine's 2:1 isometric angle, with procedural textures,
ink outlines and a colour grade. The catalog lists the tiles the generator owns;
one command renders what changed, checks it, and updates the game's
`tile_set.json` and the web build's asset list. The fortress studies (§9) were
the first test and remain the reference for the look. Plan:
[docs/plans/placeholder-art.md](../../../docs/plans/placeholder-art.md).

## 1. Quick start: adding a game tile

1. **Recipe.** Write `build_<name>(B, **params)` in a module under
   `hbgen/recipes/` and list it in `RECIPES` (`hbgen/recipes/__init__.py`).
   `test.py` is the smallest example; §10 has the rules for the engine camera.
2. **Catalog entry.** Add it to `catalog/buildings_daiLong.json` (§11): the
   tile name, the recipe, its variations, and the `tile_set` fields (cost,
   sector, materials, construction).
3. **Iterate.** `./tilegen.sh NAME --fast` renders at 32 samples. Look at
   `build/contact_<catalog>.png` and at the sprite next to its hand-made
   neighbours.
4. **Final render.** `./tilegen.sh NAME` renders at full quality, checks the
   sprite (§11), merges its tile def into `assets/tiles/<layer>/tile_set.json`,
   and refreshes `web/asset_manifest.json`.
5. **Commit** the sprites, the catalog, `catalog/manifest.json`, the tile set
   and the asset list. CI fails if they disagree.

The tile's gameplay side (its building config in
`assets/configs/buildings/configs.json`, and its sector, so the build menu
shows it) belongs to the gameplay milestone. Until then, a local config lets
you place it with `world.place` to see it in the game.

## 2. Running it

The pipeline, from anywhere in the repository (`BLENDER` overrides the path
to Blender, by default `/Applications/Blender.app/Contents/MacOS/Blender`):

    crates/tools/blender-tile-generator/tilegen.sh [TILE ...] [--fast] [--force] [--dry-run]

One tile by hand, without the catalog (prints `HB_RESULT {...}` with its sizes):

    Blender -b -P hb_tile_gen.py -- --building roof_tile_kiln --out /tmp/x.png \
        [--density 8] [--seed N] [--params '{"k": 1}'] [--blend /tmp/x.blend]

A fortress study, with the legacy camera (§4):

    Blender -b -P hb_tile_gen.py -- --building fortress --palette viet \
        --cells 15 --scale 1 --out ~/Desktop/fortress.png

Inside a running Blender (Scripting tab, or Claude through the Blender MCP),
tell it where the package is, then call the functions. Each `exec` reloads
`hbgen`, so edits take effect without restarting Blender:

    HB_TILE_GEN_DIR = '/path/to/crates/tools/blender-tile-generator'
    exec(open(HB_TILE_GEN_DIR + '/hb_tile_gen.py').read())
    generate_engine('roof_tile_kiln', out='/tmp/x.png', blend='/tmp/x.blend')
    generate(building='fortress', palette='viet', cells=15, out='/tmp/f.png')

Legacy flags: `--height` renders a taller frame if a model overflows the
square, `--no-trim` keeps the untrimmed square render.

## 3. How the code is organised

Blender modules (they import `bpy`; use them only inside Blender):

- `hbgen/palettes.py`: `PALETTES`, one dict of sRGB hex strings per
  civilisation, and the tuned look: `LIGHTING`, `OUTLINE`, `GRADE`, `EEVEE`
  (see §5).
- `hbgen/materials.py`: `Materials`, procedural node materials (brick, wood,
  noise, tiled roofs, water, plaster, fire). They're driven by object
  coordinates, so they need no UVs and stay consistent across buildings.
  Palettes that lack a newer key borrow the viet value.
- `hbgen/builder.py`: `Builder`, the geometry primitives: box, frustum,
  pagoda_roof, gable_roof, lantern, tree, cone, prang, shikhara, cylinder,
  dome, jar, tile_stack, ground_diamond. `B.rng` is seeded per variation.
- `hbgen/scene.py`: the legacy and engine cameras, lights, render settings,
  outlines, the compositor grade, and `trim_top`.
- `hbgen/recipes/`: the recipes, registered in `RECIPES`. `fortress.py`
  holds the studies, `workshop.py` the game's workshops, `overlays.py` the
  construction scaffolds and renovation overlays, `test.py` the template.
- `hbgen/generate.py`: `generate()` (legacy camera) and `generate_engine()`.
- `hb_tile_gen.py`: the one-tile command line, and the `exec` entry point.
- `build.py`: the batch driver (§11), run by `tilegen.sh`.

Plain-Python modules (standard library only; no Blender needed):

- `hbgen/catalog.py`: loads the catalog, expands it into render jobs, hashes
  them, and reads and writes the manifest.
- `hbgen/tileset.py`: merges catalog entries into `tile_set.json`.
- `hbgen/checks.py`: the sprite checks and the contact sheet.
- `hbgen/pngio.py`: reads and writes PNGs.
- `sync_tileset.py` and `validate.py`: their command lines.
- `tests/test_pipeline.py`: unit tests for these modules (`python3 -m unittest
  discover -s crates/tools/blender-tile-generator/tests`).

## 4. The legacy camera (fortress studies)

The fortress studies keep the camera they were tuned with; game tiles use the
engine camera (§10). Its frame is a square `cells × 64 × scale` px wide, and
the earth slab's front corner sits 6 px above the bottom edge.

- Orthographic camera, rotation (60°, 0, 45°), placed at (+18.4, −18.4, 15).
  60° tilt gives elevation 30°, so a square cell projects to exactly 2:1.
- Screen mapping: left corner = (−x, −y), right = (+x, +y), back/top = (−x, +y),
  front/bottom = (+x, −y).
- `ortho_scale = G·√2` where G is the ground size in cells, so the diamond's
  width is exactly the render width. Resolution = cells·64·scale.
- `shift_y` is computed by projecting the front-bottom corner of the earth slab
  and moving it to 6 px above the frame bottom. Positive shift_y moves the scene
  down in frame.
- `trim_top` then removes empty rows above the model.

## 5. The look, and how it was tuned

Final settings (v7) and why:

- **Colour space.** Palette colours are sRGB hex and are converted to linear
  (`lin()`) before being set on nodes. The first textured pass fed sRGB-looking
  values straight into node sockets as if linear; everything rendered pastel
  and washed out. This was the single biggest quality fix.
- **Lighting.** Warm key sun (5.4, slight orange), weak cool fill (0.8),
  neutral warm-grey ambient (0.82,0.80,0.74) at 0.37. A blue ambient was tried
  and made shadowed walls muddy and grass look neon by contrast — keep the
  ambient neutral and control mood through the palette instead.
- **Tonal range.** Each material has a dark and a light stop that are pushed
  apart (roof: near-black eaves → burnt-orange tops; stone: light block face vs
  dark mortar) rather than shifting the whole colour. This reads as depth
  without changing exposure.
- **Roof tiles.** Rows only, keyed on Z (`fract(z·10)` with a sqrt profile so
  each row has a highlight on top and a shadow line under it). A column pattern
  on x+y was tried first and streaked diagonally across slopes facing other
  directions. Roof specular is near zero; otherwise sunlit slopes go pink.
- **Outlines.** Freestyle, 1.6 px absolute, near-black brown, silhouette +
  border + crease (120°), with a Noise thickness modifier and a Perlin 1D
  geometry modifier so the lines read as inked rather than vector-clean.
- **Shading.** Eevee with shadows, fast GI (distance 1.5) for contact
  occlusion, 96 samples, Standard view transform (AgX/Filmic desaturate the
  palette).
- **Grade.** Compositor saturation 1.0 (a 1.25 boost was tried; it tipped the
  greens), contrast +0.08.
- **Measured targets** (Rec. 709 luma of the stored, gamma-encoded values over
  opaque pixels, as Blender's `image.pixels` returns them for a PNG): median ≈ 0.29,
  p25/p75 ≈ 0.25/0.35, median saturation ≈ 0.46. Use `world_strength` as the
  single exposure knob; it moves brightness without touching palette or contrast.

## 6. Blender 5.x gotchas found on the way

- Compositor: the tree is `scene.compositing_node_group`; output node is
  `NodeGroupOutput` with an interface socket, not `CompositorNodeComposite`.
  `CompositorNodeGamma` and `ColorBalance.correction_method` are gone; use
  `CompositorNodeHueSat` + `CompositorNodeBrightContrast`.
- Eevee: no `gtao_*` properties; AO is `use_fast_gi` / `fast_gi_distance`.
  Engine id is `BLENDER_EEVEE` (5.x) — the script falls back to
  `BLENDER_EEVEE_NEXT` for 4.2–4.4.
- Principled BSDF specular is `Specular IOR Level`.
- The MCP `render_viewport_to_path` tool ignores the path and writes under a
  temp dir; the script uses `bpy.ops.render.render(write_still=True)` with
  `scene.render.filepath` instead, which honours the path.
- Loading a PNG with `bpy.data.images.load` on a path loaded before can return
  a cached buffer — call `.reload()` or use a fresh filename when checking
  output.
- The MCP window screenshot is not a reliable way to judge a render (it showed
  a stale image more than once). Judge output by its pixel statistics, or open
  the PNG directly.

## 7. Verification checklist for a new tile

`validate.py` runs these on every game tile (§11), and `build.py` runs it after
rendering. By eye, still:

1. The contact sheet and the in-game view show closed outlines and nothing
   clipped at the sides.
2. Next to its hand-made neighbours, at zoom 2 and zoom 5, the tile reads at
   the same scale and brightness.

## 8. Next steps

See the phases in the plan. In short: generic recipes and the Phase 1
building set, scaffold and rubble overlays, terrain tiles and transitions,
cliff and ramp pieces, and the walker experiment.

## 9. Civilisation variants (added after the first test)

The script now carries three palettes and three fortress recipes:

| building          | palette | enclosure                                   | central complex                                   |
|-------------------|---------|---------------------------------------------|---------------------------------------------------|
| `vn_fortress`     | `viet`  | crenellated grey stone, pagoda gatehouses   | three-tier timber pagoda tower                    |
| `bagan_fortress`  | `burma` | red brick, stupa-topped towers, pointed gates | gilded shikhara on three stepped brick terraces, corner shrines |
| `khmer_fortress`  | `khmer` | moss-streaked grey stone, lotus parapet, gopura gates | quincunx of lotus-bud prangs over a cruciform tiled gallery |

How the variants are built: `enclosure()` is shared and parameterised by
`gate_style` (pagoda / pointed / gopura) and `tower_style` (crenel / stupa /
lotus); `halls()` places the small courtyard buildings; each `build_*` recipe
only adds its central complex. New primitives: `Builder.cone`, `Builder.prang`
(stacked frustums on a convex curve with a bud) and `Builder.shikhara` (base
block, curved spire bands, tall finial).

Palette extras: `moss` (two hexes) switches on a noise-gated moss overlay in all
brick materials (amount 0.75 on walls, 0.5 on paving); `goldStone` adds a
fine-scale gilded masonry material; `sandstone` adds a pale noise material.
Palettes without these keys simply don't get those materials.

Verified output: all three render at 960 px wide (15 cells), base 5 px above
the bottom, heights 592 / 633 / 587. Median luminance 0.29 / 0.27 / 0.28.

Known rough edges to revisit with the mesh pass: the Bagan terrace stairs read
as stacked blocks, the prangs are stepped rather than smoothly curved, and the
Khmer gallery roofs catch more sun than the reference's brown-red tiles.

## 10. Game tiles: the engine camera (added Nov 2026)

Sections 1–4 describe the fortress studies, which keep their original camera.
Every other recipe renders in the game's sprite format, set by
`calc_object_iso_coords` in `crates/game/src/tile/mod.rs`:

- The PNG is exactly `cells × 64 × density` px wide. The ground diamond spans
  the full width, because the engine centres the sprite with its logical width.
- The footprint's front corner at z = 0, world `(G/2, −G/2, 0)`, is the
  bottom-centre of the frame. The engine puts the sprite's bottom edge on the
  footprint's bottom vertex. Anything below z = 0 is cut off.
- The height is measured from the model, rounded up to whole logical px, plus
  2 px, so `draw_size` is exactly the PNG size ÷ density and nothing is trimmed.
- `density` defaults to 8 px per logical px, about the hand-made 2×2 art's.
  A model wider than the diamond prints a warning, since the frame clips it.

Game tiles go through the catalog (§1, §11), which writes the sprite and its
tile def. The desktop atlas cache repacks by itself when a sprite changes.

Writing a recipe for the engine camera:

- Centre the footprint on the origin: a 2×2 spans [−1, 1] × [−1, 1].
- Stand it on z = 0. A thin yard slab (`z` 0 to 0.05) matches the hand-made
  art's raised base.
- Keep everything inside the diamond's left and right corners.
- Front faces are +X (screen lower right) and −Y (screen lower left). Put
  doorways, kiln mouths and anything that should read on those faces.
- Scale against the hand-made neighbours: on a 2×2, a kiln about 0.7 across,
  a chimney about 1.1 tall, and goods stacks about 0.13 across.

Outlines: Freestyle multiplies the scene's line thickness by the line style's,
so the fortress renders draw 1.6 × 1.6 = 2.56 px lines. Engine renders scale
only the style's thickness, by `density × ENGINE_OUTLINE_SCALE` (0.4), which is
about 1 logical px. They keep the noise and jitter at their 1× size, because
scaled up with the density the lines turn into a soft brown halo.

Check a render against the hand-made art at the same draw size before adding it.
Then place it in the game next to its neighbours, once at zoom 2 and once at
zoom 5. A building needs a building config to be placed (`world.place`).

## 11. The pipeline: catalog, build, sync, checks

**Catalog** (`catalog/<realm>.json`; the format is documented in
`hbgen/catalog.py`). One entry per tile def:

    {
      "tile": "roof_tile_kiln",            // def name and sprite file name, at most 24 characters
      "recipe": "roof_tile_kiln",          // default: the tile name
      "params": {},                        // recipe parameters
      "variations": [{"name": "var0", "seed": 1}, {"name": "var1", "seed": 2}],
      "tile_set": {"cost": 10, "sector": "IndustryAndResources"},
      "generated": true                    // false: hand-made art, left alone
    }

`layer` (default `objects`), `category` (default `buildings`), `palette` and
`density` are optional; the file sets defaults for the last two. Without
`variations` a tile has one unnamed variation. A variation can carry its own
`params` and `tile_set` fields, such as `iso_offset`. Sprites go to
`assets/tiles/<layer>/<category>/<tile>/[<variation>/]<tile>.png`.

`frames` gives a tile several sprites in one anim set, each with its own
recipe `params`, written as `<frame>.png`. The scaffolds use them for their
stages:

    "frames": [{"name": "stage0", "params": {"stage": 0}}, {"name": "stage1", "params": {"stage": 1}}]

`"validate": {"ground": false}` marks an overlay, a sprite without a ground
diamond (the renovation scaffolds), so the checks skip the corner and
bottom-row tests. The `overlays` category is drawn by the renderer only:
`catalog/overlays.json` holds the construction site scaffolds and the
renovation overlays, which the renderer looks up by name
(`tile/rendering.rs`).

**Manifest** (`catalog/manifest.json`, written by `build.py`, committed).
For each sprite the generator wrote: its hash, PNG size, density,
`logical_size` and `draw_size`. A sprite file that isn't in the manifest is
hand-made, and `build.py` refuses to overwrite it unless given `--adopt TILE`.

**build.py** runs in Blender (`tilegen.sh` wraps it; `build.py`'s docstring
lists the options). For each catalog entry, or the tiles named:

- It renders a sprite when its hash changes. The hash covers the entry's
  settings, the core modules (`palettes`, `materials`, `builder`, `scene`,
  `generate`), the recipe's own module, the forced height, `--fast` and
  `GENERATOR_VERSION` in `hbgen/catalog.py`. Bump that to re-render
  everything, e.g. after a Blender upgrade.
- A tile def has one `draw_size`, so a tile's variations and frames all
  render at the tallest one's height.
- It saves the manifest after every render, then runs the checks, writes the
  contact sheets to `build/` (git-ignored), and syncs the tile sets.
- `--dry-run` lists what would render. `--force` ignores the hash.

**Sync** (`sync_tileset.py`, plain Python; `--check` writes nothing and
fails on any difference, which CI runs):

- A generated def follows its catalog entry: `name`, the `tile_set` fields in
  order, `logical_size`, `draw_size`, then `variations`.
- Only that def's text changes. The rest of the hand-formatted file stays
  byte for byte, and new defs go at the end of their category.
- Variations are append-only, since saves store a variation by index. One the
  catalog drops stays in place, with a note.
- A def none of whose sprites the generator wrote is hand-made, and syncing
  it is an error.
- It also checks that each sprite exists at its recorded size, isn't a
  `--fast` render, and is listed in `web/asset_manifest.json`. `tilegen.sh`
  refreshes that list with `cargo run -p web-builder -- manifest`.

**Checks** (`validate.py`, plain Python; `build.py` runs them):

- The width is the logical width × density, and the height a multiple of it.
- The top row is empty, so nothing is clipped above.
- The diamond's side corners sit a quarter of the width above the bottom,
  within one logical px.
- The bottom row holds only the front corner, at the centre.
- The median luma and saturation are within the palette's targets
  (`hbgen/checks.py`). A miss there is a warning: buildings differ.

**Round trip, by hand** (done for the kiln and the `test_hut` recipe):

1. Add a catalog entry and run `tilegen.sh`.
2. Add a local building config for the tile.
3. Place it with `world.place` next to hand-made buildings, and screenshot.
4. `tile.at` reports which variation each placed tile got.
