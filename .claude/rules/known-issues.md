# Known issues and unfinished work

Last verified against the code on 2026-10-08. Remove an entry once it's resolved.

## Unfinished features

- Home menu **Continue** (resume last save) and **Custom Game** buttons are disabled placeholders (`Enabled = false`; `on_pressed` returns `false` with a TODO) — `crates/game/src/menu/dialog/home.rs`.
- Ambient sounds ignore game state; which sound plays is a placeholder (two TODOs) — `crates/game/src/system/ambient_sounds.rs`.

## Open bugs / design questions

- `ProducerBuilding` clears its stock as soon as it hands resources to a delivery unit, so the resources are lost if that unit fails to spawn. The TODO judges this low priority, since a spawn failure would itself be a bug — `crates/game/src/building/producer.rs`.
- House access to a service is decided by proximity to the service building, measured from the house's road-link tile, not by a service patrol unit actually visiting. A TODO asks whether to switch to visit-based access — `crates/game/src/building/house.rs`.

## Failure handling

- Save failures, autosave included, are only logged. `GameSession::save_game` returns `false`, but `GameSessionCmdQueue::cmd_save_game` discards that, so the player is never told — `crates/game/src/session.rs`.
- Load failures (unreadable file, save-version mismatch) are handled the same way: `load_save_game` logs and returns `false`, and `cmd_load_save_game` discards the result.
- Most other code treats unexpected state as fatal (`unwrap()`, `panic!`, asserts — several hundred call sites) rather than recovering.
