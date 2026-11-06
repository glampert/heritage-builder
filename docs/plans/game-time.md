# Game time and calendar (M1)

**Status (2026-11-05): Done, pending a hand check on macOS.** Milestone M1 of the [roadmap](roadmap.md#m1-game-time-and-calendar-small). Researched against `16c2512` on `main`.

## Goal

A saved game clock with a date, the season and the realm's era, that every later system (construction timers, finance, floods, festivals, ratings) can read and react to. The simulation's speed must scale time exactly once.

## What was there

- `Simulation::update` ([sim/mod.rs](../../crates/game/src/sim/mod.rs)) runs the world and game systems on a fixed step (`update_timer`, 0.5 s of simulated time). The step's delta was `update_timer.time_since_last_secs() * speed`, but the timer already accumulates scaled time, so building timers ran at speed squared: 4× at 2× speed, 100× at 10×.
- `UpdateTimer::tick` returns `ShouldUpdate` without adding that frame's delta, so the step's delta also left out the frame it fired on: about one frame in 30 at 60 fps.
- `Simulation::elapsed_secs` is runtime only and resets on load; automation waits use it.
- `SimConfigs::seconds_per_day = 600` is the only game-time unit, used by house consumption as a rate unit.

## Decisions

- **Month length:** 60 simulated seconds per month and 30 days per month (2 s per day), 12 months per year, so a year is 12 minutes at 1× speed. Mission I-4 (WY 9–15) is then 72 minutes at 1×, less at higher speeds. `SimConfigs::seconds_per_month` and `days_per_month` hold them; balance can change them later.
- **House consumption keeps `seconds_per_day`.** It's a rate unit (units per 600 s), not a calendar day. Moving it to the calendar's day would multiply consumption by 300. M5 (goods) was to move rates to per-month units when it rebalanced them; it didn't retune consumption, so this moved to M13's balance pass (2026-11-06).
- **Where it lives:** `GameCalendar` ([sim/calendar.rs](../../crates/game/src/sim/calendar.rs)) is a field of `Simulation`, saved with it. It stores whole months elapsed and the seconds into the current month, so a later change to the month length keeps every save's date. `SimContext::calendar()` reads it.
- **Settings:** a `CalendarSettings` (start World Year and month, wet months, eras) is saved inside the calendar, so a save never depends on the campaign configs. Sandbox games use `SimConfigs::calendar`. A campaign mission's start applies its `calendar` (start year, start month, wet months) from `campaigns.json`, with the campaign's `eras`.
- **Seasons:** a month is wet if it's in `wet_months`, else dry. Default: months 5 to 10, the delta's monsoon.
- **Eras:** a list of `{name, start_year}` per campaign. The date reads as the latest era that has started and the year within it ("Long Thái 3"); before the first era, or with none, it reads as the World Year ("Year 3"). Only Campaign I's first reign era goes in the data now; M13 adds the rest with the missions.
- **Events:** the calendar advances on each world step. Crossing into a month calls `GameSystem::on_new_month`, then `on_new_season` if the season changed, then `on_new_year` in month 1, in that order. Setting the date through `calendar.set` fires none.
- **The timing fix also counts the frame the step fires on,** so the world, the game systems and the calendar see exactly the simulated time `elapsed_secs` counts. Building timers themselves (`UpdateTimer` ticked by buildings) keep their one-step lag; that's balance, left alone.
- **Save version 3** holds the calendar alone. M3's construction state moves to version 4.

## Tasks

- [x] Fix the speed-squared world step and count the firing frame. Regression test: the world steps add up to the same simulated time, and the calendar reaches the same date, at 1×, 2× and 10× speed.
- [x] `GameCalendar`: date (day, month, year), season, era; advance with events; `CalendarSettings` from configs or the mission.
- [x] `GameSystem::on_new_month`, `on_new_season`, `on_new_year`, broadcast by `GameSystems`, logged on the `calendar` channel.
- [x] Campaign data: `eras` per campaign, `calendar` per mission; applied when a mission starts.
- [x] HUD: date and season in the top bar.
- [x] Automation: `sim.state` reports `date`; `calendar.set` sets the date; `wait.until` waits on `/date/ordinal_month` or any other field.
- [x] Scenario `calendar.jsonl`, run in CI.
- [x] Save version 3: `v2_to_v3.py`, migrated sample saves, `--smoke-test-saves`.
- [x] Docs: AGENTS.md, the roadmap's M1 row and save-version log.

## Notes

- **Built:**
  - `Simulation::advance_clocks` holds the fixed-step timing; `Simulation::update` calls it, and so does the `game_time` test, which drives the world through it without an `Engine`.
  - `GameCalendar` in `sim/calendar.rs`, with unit tests for dates, events, eras and validation.
  - `MissionCalendarDef` on `MissionDef`, and `eras` on `CampaignDef`. Campaign I has one era, Long Thái from WY 1, and the three placeholder missions start in WY 1, 3 and 6, as missions I-1 to I-3 do in the design.
  - `campaign::restore_snapshot` now returns whether the load starts the active mission, and `campaign::take_mission_start` lets a preset mission map take the same flag. Before this, a preset mission map left the flag set until the next save load.
  - The calendar logs a new month as Verbose and a new season or year as Info.
  - `calendar.set` refuses a date before the calendar's start, since `months_elapsed` can't be negative.
- **Deviation:** the event broadcast passes `SimCmds` and the read-only `SimContext` of the step, like `GameSystem::update`, so handlers queue their changes like any update.
- **Verified (2026-11-05, Linux, Xvfb, Mesa software OpenGL):**
  - `cargo test --workspace` passes. With the old double scaling put back, the three `game_time` tests fail: 2× made 8 rice where 1× made 6.
  - The CI scenarios pass with `calendar.jsonl` added, and so do both determinism runs and record/replay.
  - The 12 local saves migrate to v3 and pass `--smoke-test-saves`, and the regenerated `sample_saves.zip` holds 11 v3 saves.
  - A date survives a save and load.
  - Starting the campaign from the home menu shows "Day 1, Month 1, Long Thái 1 · Dry season", and the top bar fits (`ui.layout_issues` is 0).
  - The web target builds.
- **Remaining checks:**
  - On macOS: the top bar's date at the Retina scale.
  - The first new month in a normal play session, at 1× and at 10×.

