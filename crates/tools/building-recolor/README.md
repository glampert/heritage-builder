# Building recolours

`recolor.py` makes building tile variations by recolouring hand-made sprites, from
`recipes.json`. It made the patrons' shrines and temples (docs/plans/faith.md, §2a): each
Đại Long patron's shrine and temple is the hand-made art with its roofs, trims and lanterns in
the patron's colour.

    python3 crates/tools/building-recolor/recolor.py [TILE[/VARIATION] ...]   # render (needs Pillow)
    python3 crates/tools/building-recolor/recolor.py --list
    python3 crates/tools/building-recolor/recolor.py --check                   # CI: nothing out of date

## How it works

- `sources/<tile>/` holds the hand-made art, moved here from the tile's old variation folders, so
  it can be recoloured again. These files aren't game assets.
- A recipe names a `tile`, one of its `variation`s, a `source` under `sources/`, and its
  `shifts`. Each output goes to `assets/tiles/objects/buildings/<tile>/<variation>/<tile>.png`.
  A recipe without shifts copies its source unchanged.
- A shift recolours by hue, since the art is painted with gradients:
  - A pixel moves when its hue is inside the `hue` window (`[from, to]` in degrees; `from` may be
    negative, wrapping past 0) and its saturation is at least `min_saturation`.
  - It moves to around `to_hue`, keeping its offset from the window's centre, times `spread`.
  - Its saturation and value are scaled by `saturation` and `value`.
  - Membership fades over `feather` degrees past the window, and over 0.08 below the saturation
    floor, so there are no hard seams.
- The shrines' and temples' window, −10° to 26° above 0.28 saturation, takes the roofs, trims and
  lanterns. The ochre walls (30° to 40°), the stone and the greenery keep their colours.
- The tile defs' variations (names, labels, realms) are edited in
  `assets/tiles/objects/tile_set.json` by hand. Variations are saved by index, so they're
  append-only, though one can be renamed.

## Rules

- It never overwrites hand-made art. `manifest.json` records what it wrote: each output's source,
  recipe and output hashes. It skips an existing file the manifest doesn't list, unless given
  `--force`.
- `--check` needs no Pillow. It fails when an output is missing, or its source, recipe or file no
  longer match the manifest: an edit not rendered, or a sprite edited by hand. CI runs it, with
  the unit tests (`python3 -m unittest discover -s crates/tools/building-recolor/tests`).
- After rendering new files, refresh the web build's asset list:
  `cargo run -p web-builder -- manifest`.
