# ui-art: the realm UI skins' placeholder sprites

Draws every sprite of the realm UI skins (panels, buttons, controls, separators, emblems) from
the mockup generator's own drawing code, so the game's UI looks like the
[mockups](../../../docs/design/mockups/README.md) until hand-made art replaces it. Plan:
[docs/plans/realm-ui.md](../../../docs/plans/realm-ui.md), Art pipeline.

- `art.json`: the catalog. One entry per sprite: the generator call that draws it, its size,
  9-slice borders and states, per realm.
- `make_ui_art.py`: renders what changed, checks (`--check`), measures (`--measure`).
- `manifest.json`: what the script wrote, with hashes. Commit it with the sprites.
- `uiart/catalog.py` (catalog, manifest, checks; standard library only), `uiart/render.py`
  (the generator adapter and the browser), `render.js` (Playwright), `tests/`.

Output: `assets/ui/skins/<realm>/<id>.png`, or `<id>_<state>.png` for an entry with states, at
2 pixels per point, RGBA on a transparent background. An entry with `"format": "jpeg"` writes an
opaque `<id>.jpg` instead, and one with a `scale` of its own is resized to it: the home menu's
backdrops are 1920 × 1080 JPEGs at 1 pixel per point.

## 1. Running it

    python3 crates/tools/ui-art/make_ui_art.py                    # render what changed
    python3 crates/tools/ui-art/make_ui_art.py --realm kambara panel_dialog
    python3 crates/tools/ui-art/make_ui_art.py --measure button_primary
    python3 crates/tools/ui-art/make_ui_art.py --list
    python3 crates/tools/ui-art/make_ui_art.py --check            # what CI runs
    python3 -m unittest discover -s crates/tools/ui-art/tests     # and this

Rendering needs:
- Python 3 with Pillow and numpy (the generator imports them);
- Node.js with Playwright and its Chromium, installed outside the repository so no
  `node_modules/` or `package.json` lands in it: `npm i -g playwright && npx playwright install
  chromium`, with `NODE_PATH="$(npm root -g)"` if Node doesn't find global packages by itself.
  Cloud sessions have both.

`--check` and the tests need only the standard library.

After adding or removing sprites, refresh the web build's preload list:
`cargo run -p web-builder -- manifest` (then `git checkout -- web/assets` if it re-linked that
symlink). `--check` fails while a sprite is missing from `web/asset_manifest.json`.

A whole run takes about 15 s: one browser shoots every sprite.

## 2. How a sprite is drawn

1. A fresh realm theme: `dialogs.<Realm>Dialogs` from `docs/design/mockups/generator/`, whose
   class hierarchy has every drawing method of the mockups (panels, buttons, form widgets,
   emblems). The generator's text helper `T()` is replaced by one that draws nothing, so no
   words end up in a sprite. The generator needs `build/fonts.css` at import; without it, an
   empty one is read in its place (the sprites have no text).
2. The entry's `attrs` are set on the theme (e.g. Tampadesa's lacquer `box_style`), helpers in
   `omit` draw nothing, and one generator call (`render.CALLS`) draws the entry's `size` rect at
   (`pad`, `pad`) on a page with `pad` points all round.
3. `elements` keeps only some top-level SVG elements of what the call drew (a toggle's track
   or knob, a slider's rail, fill or knob); `capture` keeps only what one helper drew during the
   call (Kambara's vines, a crest). `rotate: 90` turns the drawing a quarter clockwise (the
   scrollbar from the slider).
4. The page is shot with Chromium at the catalog's `scale` (2) on a transparent background, with
   each state's CSS filter on it (hovered, pressed and disabled are derived from the idle
   drawing), then cropped to the rect plus its `bleed`, resized if the entry has a `scale` of its
   own, and saved as an optimised PNG (or, for `"format": "jpeg"`, flattened on black and saved as
   a progressive JPEG at quality 85). A drawing that reaches past the crop is a warning: raise the
   bleed.

For a 9-slice whose middle repeats (`center: tile`), the theme's material helpers are swapped
for ones that repeat cleanly with the middle's size and line up with its borders, since the
engine repeats both the middle and the edges:
- noise (paper, stone, teak grain) is made as a stitched tile at the filter's origin, where
  Chromium stitches it correctly, moved onto the middle and repeated (`feTile`), with at least
  5 periods of its lowest octave per tile (`MIN_TILE_PERIODS`);
- noise layers of large features (stains, mottling, moss: base frequency under `BLOTCH_FREQ`)
  keep only `TILE_BLOTCH_CONTRAST` (45%) of their contrast about their middle tone. At full
  strength they repeat as a lattice on a large panel; the fine grain repeats unseen. So a tiled
  panel's paper or stone is plainer than the mockups' broad blotches;
- Đại Long's rough paper edge repeats the same way;
- the paper's vignette (a radial gradient) is left out, and horizontal gradients (teak) become
  their middle colour;
- Tampadesa's gilt gradient repeats along the diagonal with a period that divides the middle's width
  and height (their greatest common divisor); sides that share no factor of 16 points or more are
  an error, unless the middle is square.

Kambara's lotus-petal bands repeat every 22 points, so the sprite's middle width is a multiple
of 22, and its heights keep the petals on the 16-point pattern rows (`pad` 88).

Tiled 9-slices in the skins: the engine's `UiNineSliceCenter::Tile` repeats the middle and the
edges from the top left and cuts the last tile wherever the middle ends, so an edge with a motif
along it (Kambara's petals, Tampadesa's gilt rules, the teak and stone grain) would end in a cut
motif at the right or bottom corner. The skins use `center: round` instead
(`UiNineSliceCenter::Round`, like CSS `border-image-repeat: round`): each tile is resized a little
so a whole number of them fills the middle and the edges, and nothing is cut. `--check` accepts
`round` or `tile` in a skin for a sprite the catalog draws to repeat (`center: tile`), and only
`stretch` for one it doesn't.

The output is byte-identical from run to run on one machine. The source hash covers the SVG
page, the state's CSS, the crop and `RENDERER_VERSION` (bump it when the post-processing
changes), not the browser, so a Chromium update doesn't re-render anything by itself.

## 3. The catalog

    { "id": "button_primary", "group": "buttons", "call": "button", "args": { "primary": true },
      "size": [120, 56], "states": ["idle", "hovered", "pressed", "disabled"],
      "realms": {
        "dailong":   { "slice": [20, 20, 20, 20], "center": "stretch" },
        "kambara":   { "bleed": [1, 1, 3, 5], "slice": [12, 12, 16, 20], "center": "tile" },
        "tampadesa": { "bleed": [0, 0, 3, 5], "slice": [20, 20, 24, 28], "center": "stretch" } } }

| Key | Meaning |
|---|---|
| `id` | `[a-z0-9_]`, unique per realm; the file name. |
| `call`, `args` | The drawing call (`render.CALLS`, listed in `catalog.KNOWN_CALLS`) and its parameters. |
| `size` | The widget rect the call draws, in points at 1920 × 1080. |
| `bleed` | How far the sprite reaches past that rect, in points: left, top, right, bottom (shadows, roller knobs, stupas). Negative crops into it: an ornament or knob cut out of a larger drawing. The PNG is `(size + bleed) × scale` pixels. |
| `slice` | 9-slice borders in pixels of the PNG: left, top, right, bottom. They hold the whole frame, so only plain material stretches or repeats. |
| `center` | `stretch` (default) or `tile`; tiled sprites are drawn to repeat (above). |
| `states` | A list of state names, or an object of state → `{ "css", "args", "call" }`. Known: `idle`, `hovered`, `pressed`, `disabled`, `selected`, `locked`, `current`. `state_filters` gives each state's default CSS filter. |
| `scale` | The output's pixels per point (the catalog's if left out). The page is still shot at the catalog's scale, then resized. The PNG (or JPEG) is `(size + bleed) × scale` pixels. |
| `format` | `png` (default: RGBA, `.png`) or `jpeg` (opaque, `.jpg`), for full-screen pictures such as the home backdrops. |
| `realms` | Realm → overrides of any key but `id`. Without it the entry is made for every realm. |
| `elements`, `capture`, `omit`, `attrs`, `rotate` | See section 2. |
| `out` | Another output pattern under `skins/`, with `{realm}`, `{id}` and `{state}`. |

Values in the catalog are points except `slice` (pixels). `--measure` prints how far an entry's
drawing reaches past its rect, which is the bleed it needs (rounded up to half points).

## 4. What it makes

Per realm (Đại Long, Kambara, Tampadesa), about 60 sprites and 0.6-1.6 MB, plus a backdrop:

| Sprites | Call | Notes for the skins |
|---|---|---|
| `panel_dialog`, `panel_popup` | `mpanel` | The menu dialogs' frame (scroll and rollers; stone with petal bands; teak, gilt and corner stupas), tiled. The popup is the same art with a smaller tile. Kambara's vines are separate (`vine_tl`, `vine_br`). |
| `panel_side` | `panel_bg` | The world map's side panel. |
| `panel_card`, `panel_tooltip` | `box` | The HUD's boxes. Tampadesa's tooltip is the HUD's dark lacquer box (light text), stretched. |
| `panel_bar` | `bar_bg` | The world map's top bar (dark wood; mossy stone; teak and gilt). |
| `panel_caption` | `caption` | Story captions. Đại Long's cloud scroll sits in the bottom-right corner. |
| `panel_well`, `panel_row`, `panel_row_selected`, `panel_field` | `card`, `row_bg`, `text_input` | Flat shapes in the skin's colours. A row is only its bottom rule. |
| `panel_bubble`, `bubble_tail` | `speech` | The bubble's body (9-slice) and its tail, placed by the tail's bleed relative to a bubble of the catalog's size. |
| `button_primary`, `button_secondary`, `tab`, `tab_selected` | `button`, `tab` | Four states each, hovered, pressed and disabled derived by CSS filters. |
| `icon_frame` (4 states), `icon_frame_selected` | `build_button`, `icon_button` | The frame only; the glyphs stay with `crates/tools/ui-icons/`. Tampadesa's use the HUD's lacquer colours. |
| `marker` | `item_marker` | The menu item marker: a seal, a leaf, a small stupa. |
| `toggle_off`, `toggle_on`, `toggle_knob` | `toggle` | Track and knob apart, so the knob can slide; the knob's centre is 15 pt from the track's left (off) or right (on) end. |
| `checkbox_off`, `checkbox_on` | `checkbox` | |
| `slider_rail`, `slider_fill`, `slider_knob` | `slider` | Rail and fill are 9-slices along their length. |
| `dropdown_arrow` | `dropdown` | The chevron; its right edge is 12 points inside the field's, vertically centred. The field is `panel_field`. |
| `scrollbar_track`, `scrollbar_thumb` | `slider`, turned | The generator has no scrollbar: the slider's rail and fill, vertical. |
| `rule`, `ornament` | `divider` | The realm divider split in two: a rule that stretches and the ornament drawn at its middle (Đại Long's diamond sits 2 points above the line). |
| `emblem_<realm>` (`idle`, `locked`, `current`) | `realm_emblem` | Each realm's emblem drawn in this skin's colours, as the home menu shows them. |
| `vine_tl`, `vine_br` (Kambara) | `mpanel`, captured | The dialog's vines, placed by their bleed relative to a 480-point panel's top-left and bottom-right corners. |
| `crest` | `mpanel`, captured; `seal` | The home dialog's crest above the title: Kambara's prang and Tampadesa's stupa, captured from the panel (their bleed places them on a 480-point panel), and Đại Long's seal with the realm's initial, Đ (`seal`'s `letter`, in the title font). |
| `home_backdrop` (Kambara, Tampadesa) | `home_scene` | The home menu's backdrop, `menus.py`'s scene, 1920 × 1080 at 1 pixel per point, as a JPEG (about 130 KB). Đại Long keeps its painting (`misc/home_menu_static_bg.png`). |

## 5. Hand-made art

The script never overwrites a file the manifest doesn't list, nor one changed since it wrote
it (its hash differs from the manifest's): it skips them with a note, unless `--force`. To
replace a sprite with hand-made art for good, take its entry out of `art.json`, put the new
file in place and run `--prune`: it drops the entry from `manifest.json` and keeps the file,
since it's no longer what the script wrote. `--prune` deletes only files that are still
exactly what the script wrote (or any, with `--force`), so the order doesn't matter: removing
the entry and pruning first deletes the generated file, and the new file goes in after. The
skin keeps pointing at the same path.

## 6. `--check`

With the standard library only (CI has no Pillow or browser), it checks that:
- the catalog is valid: known keys, calls and states; ids unique per realm; outputs under
  `skins/`; whole-pixel sizes; borders that fit, with a middle when it tiles;
- every sprite exists at the catalog's pixel size, is in `manifest.json` with the same hash,
  and is in `web/asset_manifest.json`; the manifest lists nothing else;
- no sprite's catalog entry changed since it was rendered: the manifest records a hash of what
  `art.json` says about each output (`catalog.catalog_hash`: the call, its args and attributes,
  size, bleed, slice, the state's filter...), so an edit nobody rendered fails. A change to the
  generator itself only shows in the source hash, which takes a render to compute, so CI can't
  see that one;
- no sprite was rendered with its drawing reaching past its crop: a run records the spill in the
  manifest (and warns, with exit status 1), and every later run warns again until the bleed is
  raised and the sprite rendered again;
- for each skin file in `assets/configs/ui/skins/`, merged over `dailong.json` as the game
  merges it (a slice or `center` a realm doesn't repeat is Đại Long's): every sprite path it
  names exists under `assets/ui/` (any string with a `/` outside its `style`, `text` and
  `sounds` blocks; a path without an image extension is a folder, such as an icon set's, or a
  stem that takes a state suffix), and every panel, button or separator (an object with a `sprite`) that is a catalog
  sprite has the catalog's `slice` (a catalog 9-slice must be given one) and a `center` that
  suits it, as does a framed button's `selected` frame, cut by its `selected_slice` or else the
  button's `slice` (`catalog.check_skins`).

CI runs it with the unit tests, beside the tile generator's check.

## 7. Adding a sprite

1. Add an entry to `art.json`; a call the adapter doesn't know needs a line in `render.CALLS`
   and `catalog.KNOWN_CALLS`.
2. `make_ui_art.py --measure ID` gives the bleed; set the borders to hold the frame.
3. `make_ui_art.py ID`, then look at the PNGs (and at them as a 9-slice at a large size).
4. `cargo run -p web-builder -- manifest`, `make_ui_art.py --check`, and commit the sprites,
   `art.json` and `manifest.json`.

Keep generator changes for the mockups' sake only: this tool draws through overrides on the
theme instance (section 2) and doesn't change what the mockups render.
