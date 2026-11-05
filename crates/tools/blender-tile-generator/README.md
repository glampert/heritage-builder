# Heritage Builder — Blender tile pipeline

How building and terrain tiles are generated as pre-rendered isometric sprites
with Blender, as worked out in the fortress test (Nov 2026). Companion to
`hb_tile_gen.py` in this folder.

## 1. What the pipeline produces

A transparent PNG sprite, rendered from a procedurally built 3D model at the
engine's 2:1 isometric angle, with procedural textures, ink outlines and a
colour grade. For a building with an N×N-cell footprint:

- width = N × 64 × scale px (the ground diamond spans the full width)
- height = whatever the model needs (empty sky is trimmed; tall buildings
  overhang the tiles behind them, which the engine supports)
- the bottom corner of the diamond sits a few px above the bottom edge

`scale` is 1 for logical resolution, 2 or 4 to render oversampled and
downsample in the asset pipeline for crisper edges.

## 2. Running it

Headless, from a terminal on the Mac:

    /Applications/Blender.app/Contents/MacOS/Blender -b -P hb_tile_gen.py -- \
        --building fortress --palette viet --cells 15 --scale 1 \
        --out ~/Desktop/fortress.png --blend ~/Desktop/fortress.blend

Inside a running Blender (Scripting tab, or Claude via the Blender MCP):

    exec(open('/path/hb_tile_gen.py').read())
    generate(building='fortress', palette='viet', cells=15, out='/path/out.png')

Flags: `--height` renders a taller frame if a model overflows the square,
`--no-trim` keeps the untrimmed square render.

## 3. How the script is organised

- `PALETTES` — one dict per civilisation, sRGB hex strings. Every material
  takes its colours from here, so a new civ is a new dict, nothing else.
- `LIGHTING`, `OUTLINE`, `GRADE`, `EEVEE` — the tuned look (see §5).
- `Materials` — procedural node materials: brick (stone/pave), wood grain,
  noise (grass/leaf/trunk/earth), tiled roof, water. All driven by Object
  coordinates so they don't need UVs and stay consistent across buildings.
- `Builder` — geometry primitives: box, frustum (hipped roof), pagoda_roof
  (eave skirt + roof + ridge cap + corner tips), lantern, tree, ground_diamond.
- `BUILDINGS` — recipe functions. Each builds with a Builder and returns the
  footprint size in cells. `build_fortress` is the reference implementation.
- `setup_camera / setup_lighting / setup_render / setup_outlines / setup_grade`
  — scene setup. `generate()` wires it all together; `trim_top()` crops the PNG.

To add a building: write `build_<name>(B)` using the Builder primitives, add it
to `BUILDINGS`. Keep 1 world unit = 1 grid cell and centre the footprint on the
origin. Front faces are +X and −Y (screen bottom-right and bottom-left).

## 4. Camera and grid mapping (the part that must not drift)

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

1. Opaque columns span 0 → width−1 (diamond fills the width).
2. Lowest opaque row is a few px above the bottom edge.
3. Median luminance ≈ 0.29, median saturation ≈ 0.46 (same palette).
4. Width is cells·64·scale exactly.
5. Open the PNG over a checker background and confirm outlines are closed and
   the moat/grass edge isn't clipped.

## 8. Next steps

- Mesh polish on the fortress: curved upturned eaves, arched gate openings,
  paving detail in the courtyard (all per-asset; the pipeline is unaffected).
- Palettes for the Khmer and Bagan civilisations.
- Terrain tiles: a `build_terrain_*` family using `ground_diamond` with no
  island, rendered at 1×1 cells (64×32 plus whatever elevation needs).
- Oversampled renders (`--scale 2`) with a downsample + palette-quantise step in
  the asset pipeline if the 1× output looks too soft in-engine.
- Move `hb_tile_gen.py` and this doc into the repo once the format is settled.

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

Render one, from the repository root:

    /Applications/Blender.app/Contents/MacOS/Blender -b \
        -P crates/tools/blender-tile-generator/hb_tile_gen.py -- \
        --building roof_tile_kiln \
        --out assets/tiles/objects/buildings/roof_tile_kiln/roof_tile_kiln.png

The last line it prints starts with `HB_RESULT` and gives the `logical_size` and
`draw_size` for `assets/tiles/objects/tile_set.json`. Until the catalog and
`sync_tileset.py` exist (placeholder-art plan, Phase 2), add or update the tile
def there by hand. Then run `cargo run -p web-builder` and commit
`web/asset_manifest.json`. The desktop atlas cache repacks by itself.

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
