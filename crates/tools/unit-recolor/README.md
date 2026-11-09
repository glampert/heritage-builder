# Unit recolour

Makes a unit's sprites by recolouring another unit's, for walkers that need to look different
before they have art of their own. The fire watcher is the guard with a red jacket and cap
([city-risks plan](../../../docs/plans/city-risks.md), decision Q7).

    python3 crates/tools/unit-recolor/recolor.py [UNIT ...] [--list] [--force]

Needs Pillow (`pip install Pillow`).

## What it does

For each recipe in `recipes.json` (or only the units named):

1. Recolours every frame under `assets/tiles/objects/units/<source>/` into
   `assets/tiles/objects/units/<name>/`, at the same paths.
2. Adds a `<name>` tile def to `assets/tiles/objects/tile_set.json`: a copy of the source unit's
   def, placed right after it. A def that's there already is left as it is, so edit it by hand
   if the source's changes. The rest of the file keeps its formatting.
3. Records the frames it wrote in `manifest.json`. It skips an existing file the manifest doesn't
   list, so it never overwrites hand-made art, unless `--force`.

Then, by hand:

- a `UnitConfigKey` variant (`crates/game/src/unit/config.rs`) and a unit config in
  `assets/configs/units/configs.json`, both named after the tile def;
- the web build's asset list: `cargo run -p web-builder -- manifest`.

## Recipes

```json
{
  "name": "fire_watcher",
  "source": "guard",
  "outline": "#1a0b03",
  "colors": { "#765729": "#9a221a", "#8e7644": "#c43a26", "#887951": "#d06048" },
  "keep": ["#503b1a", "#40381f", "#393016", "#2a2313", "#776f47", "#776c47"]
}
```

- `outline`: the dark colour the source art is outlined in.
- `colors`: each flat fill to change, and its new colour.
- `keep`: colours to leave alone even though they look like a blend of the outline and a fill.

The source art is flat fills with a dark outline, antialiased: an edge pixel is a blend of the
outline and a fill. A pixel that's such a blend of the outline and one of `colors` (within a small
error, at least 30% of the way to the fill) gets the same blend of its new colour. A pixel closer
to a `keep` colour, or to its blend with the outline, stays as it is.

To find the colours, count the source frame's pixels by colour, and look at where the changed ones
sit. The guard's spearhead (`#776f47`, `#776c47`) lies close to the jacket's blend line, so it's in
`keep`; without it the spearhead turns red.
