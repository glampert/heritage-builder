# Known issues and unfinished work

Last verified against the code on 2026-11-05. Remove an entry once it's resolved.

## Unfinished features

- Home menu **Continue** (resume last save) and **Custom Game** buttons are disabled placeholders (`Enabled = false`; `on_pressed` returns `false` with a TODO) — `crates/game/src/menu/dialog/home.rs`.
- Ambient sounds ignore game state; which sound plays is a placeholder (two TODOs) — `crates/game/src/system/ambient_sounds.rs`.
- Construction sites and renovating houses have no art of their own. A site draws its building as a faint outline with the built part rising through it, and a renovating house is tinted (`TileMapRenderer::draw_tile`, `crates/game/src/tile/rendering.rs`). The completion sound reuses the UI's tile placed sound (`UiSoundKey::ConstructionCompleted`). Scaffold sprites come from the placeholder-art pipeline (roadmap M2).
- The roof-tile kiln (`roof_tile_kiln` in `assets/tiles/objects/tile_set.json`) has generated art and a tile def but no building config, because its good (roof tiles) comes with roadmap M5. It has no `sector`, so the build menu leaves it out, but the developer editor's palette lists it, and placing it fails with "Can't find Building config".
- The construction plan's "fast build" option belongs in Custom Game settings, which don't exist yet. Until then `SimConfigs::construction.time_scale` is edited in the debug Game Configs window or with `config.set`, and the `instant_construction` cheat skips sites.

## Open bugs / design questions

- `ProducerBuilding` clears its stock as soon as it hands resources to a delivery unit, so the resources are lost if that unit fails to spawn. The TODO judges this low priority, since a spawn failure would itself be a bug — `crates/game/src/building/producer.rs`.
- House access to a service is decided by proximity to the service building, measured from the house's road-link tile, not by a service patrol unit actually visiting. A TODO asks whether to switch to visit-based access — `crates/game/src/building/house.rs`.
- `world.add_stock` checks each good's room on its own, so goods that fit one at a time but not together (20 Rice and 20 Wood for an empty yard's 32 units) succeed with only part stored, and trip a `debug_assert` in debug builds — `crates/game/src/automation/commands/world.rs`.

- Water shores are recomputed only when terrain is placed, so a save made before 2026-10-20, when the composed shore variations (channels, bays, several corners) were added, still draws plain water in a one-cell channel until the land next to it is placed again. Loading doesn't recompute them — `crates/game/src/session.rs`, see `.claude/rules/terrain-tiles.md`.
- A stock keeps the goods it accepted when it was built, since each one saves its accepted kinds. In saves made before the storage yard took pottery and bricks (`tiny_island` and `test_map_64x64`, `_100x100`, `_128x128` and `_256x256` among the sample saves), its slots accept only Rice, Meat, Fish, Wine, Wood, Metal and Clay, so walkers and the city stock skip it for pottery and bricks: a brick maker or pottery kiln built there has nowhere to deliver, and construction can't draw bricks from it. Houses and markets saved before they took pottery don't take it either: all of those in the sample saves but `test_save_1` and `test_save_2`. Loading doesn't refresh what a stock accepts from the config — `StorageBuilding::post_load`, `crates/game/src/building/storage.rs`.

## Failure handling

- Save failures, autosave included, are only logged. `GameSession::save_game` returns `false`, but `GameSessionCmdQueue::cmd_save_game` discards that, so the player is never told — `crates/game/src/session.rs`.
- Load failures (unreadable file, save-version mismatch) are handled the same way: `load_save_game` logs and returns `false`, and `cmd_load_save_game` discards the result.
- Most other code treats unexpected state as fatal (`unwrap()`, `panic!`, asserts — several hundred call sites) rather than recovering.
