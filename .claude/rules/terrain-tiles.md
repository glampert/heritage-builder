# Terrain tiles: water shores and road junctions

Last verified against the code on 2026-10-20.

## Water shores

- A water tile draws the shore of the land around it. `water::update_transitions` ([tile/water.rs](../../crates/game/src/tile/water.rs)) picks its variation from which of its eight neighbors are land: the four sides as the digits of the name (north, east, south, west: `1000` is land to the north, `1010` land on both the north and south with a channel between the shores, `1111` a pond), then the diagonals no side's shore covers, in the order ne, nw, se, sw (`ne_corner`, `ne_se_corner`, `1000_sw_corner`). A diagonal next to a land side is ignored, except with a single land side when a one-cell channel enters beside that land, i.e. the water side between them has land at both of its diagonals: then that diagonal is listed too (`0010_nw_sw_corner`) and the shore bends from the channel's bank into the wider water, as at a lake's corner. Cells past the map's edge count as water. North is +x, the upper-right edge on screen; east is -y, the lower-right.
- The water tile has 55 shore variations: 13 hand-made (one side, two adjacent sides, one diagonal) and 42 composed from those by [crates/tools/water-transitions/compose.py](../../crates/tools/water-transitions/compose.py) (standard library only; `--list` prints the names). Rerun it after changing the hand-made ones. The decorative `rock`, `rocks`, `plant` and `plants` variations sit between the two groups.
- Hand-made geometry, in cell coordinates where the center is 0 and the edges ±1: a land side's shoreline is 0.25 past the center on the water side, a diagonal's land is the square past both of its edges, and the dirt band is 0.55 wide on the north and south shores but 0.25 on the east and west ones. A one-cell channel moves both shores 0.56 towards their land (`CELL_SHIFT`), so its banks are at ±0.31; every composed junction, bend, bay, pond and channel mouth is built from the hand-made pieces moved the same way, with a bank stopping where the bank across its end has its dirt (`DIRT_EXTENT`). Land wins over water and dirt over grass where pieces overlap.
- Saves store a tile's variation as an index, so new variations go at the end of the list in `assets/tiles/terrain/tile_set.json`; never reorder them. The sprite frame name (`TileSprite::name`) is an `ArrayString<24>`, sized for the longest composed name.
- Shores are recomputed only by `TilePlacement::place` ([menu/mod.rs](../../crates/game/src/menu/mod.rs)), which the dev editor, the player's build tools, `world.place` and undo/redo use, and by `debug::utils::refresh_cached_tile_visuals` for the preset maps. The `Spawner` / `SimCmds` tile spawns don't update shores or road junctions, and neither does loading a save or starting a new map: a save made before a variation existed keeps what it stored until the terrain next to it changes.
- After adding or changing sprites, delete `cache/atlas/` (git-ignored): the game loads the packed texture atlas from there when it exists and only repacks when it doesn't, so new sprites draw as magenta placeholders otherwise.
- Tests: `cargo test -p game --test terrain_placement` places land on a water map through `TilePlacement::place` and checks the variation names, including every one of the 256 neighbor patterns; `crates/game/tests/scenarios/water_edges.jsonl` does the same in the running game and runs in CI.

## Roads

- A road tile's variation is its junction mask, the same four side bits (`road::update_junctions`, [tile/road.rs](../../crates/game/src/tile/road.rs)), updated by `TilePlacement::place` and `TilePlacement::clear`.

## Map layout

- The playable area is the rectangle on screen that fits inside the map diamond (`TileMapPlayableArea::with_inner_rect_margin`): the triangular margins of the cell grid hold no tiles at all, so `try_tile_from_layer` returns `None` there and `TileMap::is_cell_within_playable_area` tells the two apart. Iterate tiles with `for_each_tile` rather than over the whole cell grid.
