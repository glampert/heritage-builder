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
- Start options: `--map preset:N` (debug presets, 9x9 to 15x15; `session.list_presets`), `--map empty:WxH`, `--load-save NAME`, `--seed N`, `--fixed-dt SECONDS`.
- In a running game: `session.new_map size=[16,16]`, `session.load_preset preset=2`, `session.load_save name=...`.

## Act and look

- Tiles: `tiles.list` gives the names; `world.place tile=house0 cell=[4,4]`, `world.road from=[0,5] to=[8,5]`, `world.clear cell=[4,4]`, `undo`. Placement goes through the player's own code, so it pays the cost and fails where a player would. `cheats.set name=ignore_tile_cost value=true` lifts the cost.
- Settlers turn `vacant_lot` tiles into houses. One arrives every 20 simulated seconds while a lot is free, from the map's spawn point: cell `[width/2 - 1, height - 1]` unless the map sets one. `crates/game/tests/scenarios/smoke.jsonl` runs a road from there to the lots.
- State: `session.state`, `sim.state`, `world.stats`, `world.list type=building`, `world.building_at cell=[x,y]`, `tile.at cell=[x,y]`, `world.inspect id=...` (the full saved state of one object).
- Time: `time.fast_forward sim_secs=120` runs the simulation ahead without drawing; `frame.hold` / `frame.step frames=N` hold it still for close inspection; `wait.until query=sim.state path=/population op=gt value=0`.
- Screenshots: the MCP `screenshot` tool returns the image at window-point resolution. `hb-ctl screenshot out/x.png`, or `screenshot path=out/x.png` in a script. `include_ui=false` shows the scene only. `camera.teleport cell=[x,y]` and `camera.zoom zoom=2` frame the shot.
- Coordinates: cells are `[x, y]` map cells. `camera.cell_to_screen` and `camera.screen_to_cell` convert to and from window points (the units of point-resolution screenshots).

## Check and debug

- Logs: `logs.tail min_level=Warning` (MCP), or `hb-ctl logs --follow` (prints `panic` and `session.changed` too).
- Crashes: each desktop panic writes `logs/crash-<timestamp>.txt` (message, backtrace, frame, recent log lines). After a crash the MCP tools report the panic and its crash file; launch again to continue.
- Reproduce a bug: put the steps in a script with `time.fixed_dt` and a fixed `--seed`. The same script and seed give the same game, byte for byte; `session.checksum` compares states, and `session.save name=...` keeps one to inspect.
- Assertions (scripts): `assert.gt query=sim.state path=/population value=0`; also `eq`, `ne`, `ge`, `lt`, `le` and `exists`.

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
