---
name: drive-game
description: Launch, drive, inspect and screenshot the running Heritage Builder game, through the heritage-builder MCP tools, hb-ctl or --script mode. Use it to check a change in the real game, reproduce or debug a gameplay bug, take screenshots, or write an automation scenario.
---

# Driving the game

The game runs automation commands; `help` lists them with their parameters. There are three ways in; pick by task:

| Way in | Best for |
| --- | --- |
| MCP tools (`heritage-builder` server, `.mcp.json`) | Interactive work in this session: look, act, look again. |
| `hb-ctl` (`cargo run -q -p hb-ctl -- ...`) | The same from bash, e.g. when the MCP server isn't loaded. |
| `--script FILE` | Repeatable checks with a pass/fail exit status; CI. |

All three run the same commands with the same JSON parameters and results.

## Start a game

- MCP: `game_launch` with `args`, e.g. `["--map", "preset:2", "--fixed-dt", "0.016667"]`. It builds the game first (slow the first time). `game_connect` attaches to one already running with `--automation`.
- Bash: `cargo run -q -p hb-ctl -- launch --map preset:2 --fixed-dt 0.016667`. It prints the pid, port and log file (`logs/game-<pid>.log`).
- Start options: `--map preset:N` (debug presets, 9x9 to 15x15; `session.list_presets`), `--map empty:WxH`, `--load-save NAME`, `--seed N`, `--fixed-dt SECONDS`, `--window-size WxH`. `--render-api Wgpu` (or `OpenGl`) picks the renderer without editing `configs.json`.
- In a running game: `session.new_map size=[16,16]`, `session.load_preset preset=2`, `session.load_save name=...`.

## Act and look

- Tiles: `tiles.list` gives the names; `world.place tile=house0 cell=[4,4]`, `world.road from=[0,5] to=[8,5]`, `world.clear cell=[4,4]`, `undo`. Placement goes through the player's own code, so it pays the cost and fails where a player would. `cheats.set name=ignore_tile_cost value=true` lifts the cost.
- Settlers turn `vacant_lot` tiles into houses. One arrives every 20 simulated seconds while a lot is free, from the map's spawn point: cell `[width/2 - 1, height - 1]` unless the map sets one. `crates/game/tests/scenarios/smoke.jsonl` runs a road from there to the lots.
- State: `session.state`, `sim.state`, `world.stats`, `world.list type=building`, `world.building_at cell=[x,y]`, `tile.at cell=[x,y]`, `world.inspect id=...` (the full saved state of one object).
- Time: `time.fast_forward sim_secs=120` runs the simulation ahead without drawing; `frame.hold` / `frame.step frames=N` hold it still for close inspection; `wait.until query=sim.state path=/population op=gt value=0`.
- Screenshots: the MCP `screenshot` tool returns the image at window-point resolution. `hb-ctl screenshot out/x.png`, or `screenshot path=out/x.png` in a script. `include_ui=false` shows the scene only. `camera.teleport cell=[x,y]` and `camera.zoom zoom=2` frame the shot.
- Coordinates: cells are `[x, y]` map cells. `camera.cell_to_screen` and `camera.screen_to_cell` convert to and from window points (the units of point-resolution screenshots).

## Play through the UI

To test what a player sees, drive the menus and the map with the simulated mouse and keyboard:
- `ui.tree` lists the widgets on screen with their paths and rects (`filter=...` narrows it, `fields=["path","value"]` trims each entry). `ui.click path="New Game"` clicks one by path, label or tooltip, and `ui.hover` moves the cursor onto it and leaves it there (tooltips); `ui.set path="New Map Width" value=64` sets inputs, checkboxes, sliders, dropdowns and lists.
- Build palette: `ui.click path=Services`, then `ui.click path=Apothecary`, then `input.click cell=[8,8]`. Roads: `ui.click path=Roads`, `ui.click path="Dirt Road"`, `input.drag from=[5,5] to=[5,12] cells=true`. `input.key key=Escape` drops the tool.
- `input.click point=[x,y]` clicks a window point (from `ui.tree` or a points screenshot); `input.key key=Slash modifiers=["ctrl"]` sends shortcuts; `input.type text=...` types into the focused field.
- Gestures ignore the real mouse and keyboard while they run (input capture). A person at the machine gets them back with Shift+Esc, or you with `input.capture on=false`.
- `dialog.open name=LoadGame` / `dialog.close` jump straight to a dialog.
- After clicks the cursor stays put, and the tile under it is highlighted. `input.move park=true` before a screenshot.
- Developer editor (`menus.mode mode=DevEditor`): open a menu bar menu before its items (`ui.click path=Debug`, then `ui.set path="Debug/Draw grid" value=true`). Pick palette tiles with `ui.set path="Tile Selection" value="Small Well"`, since most of the palette is scrolled out of view. With no tile picked, `input.click cell=[x,y]` opens the `Tile Inspector` on that tile. Its panels are collapsing headers (`ui.click path="Tile Inspector/Building"`, then `.../Building/Workers`); clicking toggles them, so check their `value` in `ui.tree` first. `ui.click path="Tile Inspector/Close"` closes it. The Debug menu's `Show ...` checkboxes open the debug windows (World Debug, Game Systems Debug, ...); click a tab by its label to select it. In the log viewer, `ui.set path="Log Viewer/Console" value=...` and `input.key key=Enter` run a console command. If `ui.click` says a widget is covered, close the menu or window over it. More in AGENTS.md, "Simulated input and UI automation".
- Examples: `crates/game/tests/scenarios/ui_new_game.jsonl`, `ui_build.jsonl` and `ui_dev_menus.jsonl`. `ui_snapshot.jsonl` visits every screen, checks each against a golden screenshot, and on the retail screens asserts `ui.layout_issues` finds nothing: no widget outside its menu's content area or over a sibling.

## Check and debug

- Logs: `logs.tail min_level=Warning` (MCP), or `hb-ctl logs --follow` (prints `panic` and `session.changed` too).
- Crashes: each desktop panic writes `logs/crash-<timestamp>.txt` (message, backtrace, frame, recent log lines). After a crash the MCP tools report the panic and its crash file; launch again to continue.
- Reproduce a bug: put the steps in a script with `time.fixed_dt` and a fixed `--seed`. The same script and seed give the same game, byte for byte; `session.checksum` compares states, and `session.save name=...` keeps one to inspect.
- Bugs found by playing: the person runs the game with `--record FILE`, and `--replay FILE` repeats that run frame for frame, including a crash. Add `--automation` (`hb-ctl launch --replay FILE`) to inspect it as it plays; commands sent then change the run, so stick to queries. Exit status 1 means the replay ended differently from the recording, e.g. in a window of another size.
- Assertions (scripts): `assert.gt query=sim.state path=/population value=0`; also `eq`, `ne`, `ge`, `lt`, `le` and `exists`. `assert.image golden=FILE` compares a screenshot with a PNG made on the same machine and render backend (`update=true` makes it). `--golden-dir DIR` resolves relative goldens against `DIR`, e.g. one per backend, and `--update-goldens` makes every `assert.image` in the run update its golden.

## Scripts

```bash
cargo run -p HeritageBuilder -- --script crates/game/tests/scenarios/smoke.jsonl 2>/dev/null
```

One command per line (`{"method": ..., "params": {...}}` or `name key=value ...`), `#` comments. Prints a JSON result line per command and a summary; exits 0 if all succeeded, 1 if not, 2 if the file is invalid. Logs go to stderr. Examples: `crates/game/tests/scenarios/`.

## Gotchas

- At the home menu, map commands fail with `NotInGame`. Load a map first.
- The preset maps have roads on many cells; check with `tile.at` before placing.
- `world.place` takes the first cell (lowest x and y) of a multi-cell building.
- Without `--fixed-dt`, frames advance in real time and runs aren't repeatable.
- Quit games you started (`quit`, or `hb-ctl quit`). A game launched through MCP quits when the MCP client disconnects.
