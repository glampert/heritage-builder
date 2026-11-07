# Finance

**Status (2026-11-07): Done.** The open decisions were settled on 2026-11-07. This plan is milestone M4 of the [roadmap](roadmap.md). It covers GDD 5.7: one city treasury that can go into debt, monthly wages and building upkeep, a tax rate the player sets, and yearly accounts in the finances dialog. Mission I-2 teaches "the tax office, the treasury and wages" ([campaigns.md](../design/campaigns.md#2-fish-and-clay)), and M9's failure rule "the city stays in debt too long" (GDD §7) needs the debt state. It was researched against `14fbc02` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo test --workspace` and the scenario scripts before pushing.
  - Follow the Saves workflow for the format change in Phase 0.
  - New retail UI passes `ui.layout_issues`.

## Goals

1. **One treasury, and it can go into debt.** All of the city's gold is one signed balance. Taxes go into it, and spending comes out of it.
2. **Running costs.** Every month the city pays wages to its employed workers and upkeep on its finished buildings. A city without tax coverage runs down its treasury.
3. **A tax rate the player sets.** It scales what houses pay. Its effect on ratings comes in M9.
4. **Yearly accounts.** Income and spending by category, this year against last year, in the finances dialog.
5. **Debt has a clock.** The game counts the months the city has been in debt, and M9 turns too many into a mission failure.
6. **Adding a category later doesn't break saves.** Trade (M10), festivals (M8) and court levies (M10) add ledger lines, so the ledger is saved by name, as goods are since M5.

## What exists today

**Gold lives in two places:**
- `GlobalTreasury { gold_units: u32 }` (`sim/resources.rs:843`), owned by `Simulation` (`sim/mod.rs:71`). It starts at `SimConfigs::starting_gold_units` (100 in `configs.json`).
- Each tax office has its own `StockOrTreasury::Treasury { gold_units: u32 }` (`building/service.rs:674–684`). A tax collector's patrol brings the gold back to its office, which keeps it (`service.rs:644–650`).
- `subtract_gold_units_global` (`resources.rs:891`) takes from the global treasury first, then from the tax offices (`BuildingKind::treasury()`), and saturates at zero.
- **Bug:** demolishing a tax office throws its gold away. `ServiceBuilding::despawned` (`service.rs:175`) only discards its walkers' spawn promises.
- `WorldStats::treasury` (`world/stats.rs:63`) sums both places into `gold_units_total: u32` on every simulation step. Placement checks (`GlobalTreasury::can_afford`), the HUD (`menu/in_game/bars.rs:173`), the `Treasury { min_gold }` mission goal (`campaign/config.rs:86`) and the automation queries (`sim.state`, `session.state`, `world.stats`) all read that total.

**Taxes** (`building/house.rs:1113–1165`):
- Every `generate_tax_frequency_secs` (60 s, the same as `seconds_per_month`), a house adds `employed residents × base_tax_generated` to its `tax_available`. A fully employed house gets `tax_bonus` percent more. Then a tax collector's visit takes it.
- **Per house level** (`configs.json`):

  | Level | Base tax per employed resident | Bonus when fully employed |
  |---|---|---|
  | 1 | 1 | 10% |
  | 2 | 2 | 20% |
  | 3 | 3 | 30% |
  | 4 | 6 | 40% |

  A level-1 house holds 8 residents, 75% of them workers, so a fully employed one pays about 7 gold a month.
- No tax rate exists, and houses a tax collector never visits pay nothing.

**Spending:**
- The only spending is placement: a tile def's `cost` is charged by `Spawner::subtract_tile_cost` (`world/object.rs:803`). Undo and a cancelled construction site refund it (`object.rs:822`, `:840`).
- Gold costs are small next to taxes: 2–20 gold for most buildings, 500 for the governor's palace and 1,000 for the citadel.
- No wages and no upkeep.

**Workers:**
- `WorldStats::population.employed` counts the residents who hold a job, recounted on every world step, before calendar events fire.
- Buildings employ from 1 worker (apothecary) up to 15 (citadel, university).
- Construction sites have no workers.

**Calendar hooks:** `GameSystem::on_new_month` and `on_new_year` (`system/mod.rs:57–61`) run after the world step's update, with a read-only `SimContext`. Then `SimCmds` run with a mutable one (`sim/mod.rs:186–212`).

**UI:** the finances dialog (`menu/dialog/city_management/finances.rs`) is two headings of text lines: taxes generated, available and collected, and the gold in the city treasury and in the tax offices. The retail UI has a slider widget (`UiSlider`, used in the settings dialog).

## Design

### 1. One treasury (Phase 0)

- **One balance.** `Simulation` owns a new `sim::finance::Finance`, which replaces its `treasury` field. It holds a signed balance (`i64`), the player's rates, the ledger and the debt counter. `GlobalTreasury` goes away, or shrinks into `Finance`'s balance.
- **Taxes go straight to the treasury.** A tax collector's patrol deposits its gold into the balance when it returns to the office, recorded as tax income, as it lands today. Tax offices no longer hold gold:
  - `StockOrTreasury::Treasury` and `BuildingKind::treasury()` go away, and a tax office has `StockOrTreasury::None`.
  - Demolishing an office no longer loses money. A collector still out on patrol when its office is demolished loses what it carries, as any walker's goods are lost today.
  - **Plunder (user decision, 2026-11-07):** when city defence and attacks come (roadmap Phase 2, GDD 5.16), losing a tax office to an enemy attack counts as plunder and takes some gold from the city treasury, recorded in the ledger.
- **Signed stats.** `WorldStats::treasury` keeps `gold_units_total`, now an `i64` equal to the balance. `gold_units_in_buildings` goes away. The HUD, the queries and the `Treasury { min_gold }` goal read the signed total.
- **Affordability.** A placement still needs `balance ≥ cost`. In debt, nothing with a gold cost can be built, which is how debt bites in M4.
- **Construction in the ledger.** Placement charges are recorded as construction spending, and refunds (undo, a cancelled site) are taken back off it in the year they happen.

### 2. The ledger (Phase 0)

- **Categories** are an enum, `LedgerCategory`:
  - income: `Taxes`;
  - spending: `Construction`, `Wages` and `Upkeep`.

  Later milestones add `Trade` (M10), `Festivals` (M8) and `Court` levies and gifts (M10).
- **Accounts.** `Finance` keeps `this_year` and `last_year`, each a per-category array of `i64` totals, and the balance at the start of each year.
- **Saved by name.** Each year saves as `{ "Taxes": 412, "Wages": 96 }` with zero entries left out, through the same pattern as `per_kind_map`. An unknown category name is an error. Adding a category needs no save-version bump, and removing or renaming one does.
- **The new year.** At `on_new_year`, `this_year` moves to `last_year`, and a new year starts empty.
- **What isn't recorded:** cheats and debug edits to the balance. The accounts don't have to add up to the balance; the dialog shows both.

### 3. Wages and upkeep (Phase 1)

- **Yearly rates, monthly payments.** Rates are set per year, in whole gold, and charged each month as one twelfth.
  - The remainder of each charge carries to the next month, saved in `Finance`, so fractions aren't lost and the arithmetic stays in integers (determinism).
  - A worker at the default wage costs half a gold a month, which an integer monthly rate couldn't express.
- **Wages:** `employed × wage_per_year`, from `population.employed` at the month's end.
  - The default wage is 6 gold per worker per year. That is about half of what a level-1 worker pays at the default tax rate (about 13 a year), so a city with tax coverage makes money, and one without it slowly loses money.
  - `FinanceConfigs::default_wage` sets it, and the player changes it in the finances dialog.
- **Upkeep:** a new optional tile def field, `upkeep`, in gold per year next to `cost`. It is charged for finished buildings only: sites pay none.
  - Proposed values:

    | Buildings | Upkeep (gold per year) |
    |---|---|
    | Tax office, police station, apothecary | 6 |
    | Temple, hospital, theatre | 12 |
    | University | 24 |
    | Governor's palace | 120 |
    | Citadel | 240 |

  - Houses, producers, storage, wells, shrines and gardens pay none.
  - Generated tile defs take it through their catalog entry (`tile_set.upkeep`), as `cost` does.
- **When they're charged:** in `Simulation`, after a world step's `SimCmds` have run, for each new month the step crossed. The world is unlocked then, and the order is fixed: wages, then upkeep.
- **Not a game system.** The finance state lives in `Simulation` next to the calendar, rather than in a `GameSystem`, because placement, tax collectors and the HUD all reach it through `SimContext`, as they reach the treasury today.

### 4. Debt (Phase 1)

- **How the city gets into debt.** Wages and upkeep are charged even when the balance can't cover them, so the balance goes negative. Nothing else spends into debt.
- **The clock.** `months_in_debt` counts the consecutive month-ends with a negative balance, and goes back to 0 once the balance is 0 or more.
  - `FinanceConfigs::debt_limit_months` (default 12) sets the limit, and `Finance::is_debt_overdue()` reports it.
  - M9 makes an overdue debt a mission failure. M4 only reports it.
- **Telling the player.** Going into debt raises a toast, and so does each month in the last quarter before the limit. M11's advisor messages take over later.
- **What debt blocks:** building anything with a gold cost. Its effects on immigration and ratings belong to M9.

### 5. Tax rate (Phase 2)

- **The player sees five levels** (user decision, 2026-11-07): None, Low, Normal, High and Harsh.
- **Internally**, a `TaxLevel` enum, saved by name. Each level has a percentage of the base tax in `FinanceConfigs::tax_level_percent`. The defaults are 0, 50, 100, 150 and 200.
  - Normal pays exactly today's base amounts: a house pays `employed × base_tax_generated × percent / 100`, plus the full-employment bonus, rounded as today.
  - `FinanceConfigs::default_tax_level` is Normal.
- **When it applies:** to every tax a house generates after the change. Taxes already owed keep their amount.
- **Later effects:** M9's Legitimacy ("fair taxes") compares the rate with a mission's fair rate, and Phase 2's unrest reacts to high taxes. M4 only records the rate.

### 6. Missions (Phase 2)

- `MissionDef` gains an optional `finance` block: `starting_gold`, `tax_level` and `wage`. It is applied when a mission starts, as its `calendar` block is (`campaign::active_mission_calendar`).
- `SimConfigs::starting_gold_units` stays the default for a new map. It moves into `FinanceConfigs` only if that's free to do; recordings and the settings dialog carry `SimConfigs`.
- M13 sets the actual numbers for missions I-1 to I-5.

### 7. The finances dialog (Phase 2)

The dialog becomes the yearly accounts:
- **Rows:**
  - income by category, and the total;
  - spending by category, and the total;
  - net;
  - the balance at the start of the year.
- **Columns:** last year, then this year so far.
- **Below the accounts:**
  - the balance;
  - the taxes owed by houses that a collector hasn't taken yet (today's `tax_available`);
  - in debt, how many months the city has left before the limit.
- **Controls:** the tax level and the wage, as sliders or as − and + buttons, whichever fits the retail style. They take effect at once.
- **Layout:** it passes `ui.layout_issues`, and the `ui_snapshot.jsonl` and `ui_build.jsonl` paths still work.

The HUD shows a negative balance with a minus sign, in a warning color if the style has one.

### 8. Automation, tests and scenario

- **Query:** `finance.state` reports the balance, the rates, `months_in_debt`, whether the debt is overdue, and the ledger for this year and last year, by category name. `sim.state`, `session.state` and `world.stats` keep their `gold` fields, now signed.
- **Command:** `finance.set` takes any of `tax_level` (`None`, `Low`, `Normal`, `High` or `Harsh`), `wage` and `balance`. The balance is for scenarios: it puts a city into debt quickly, and it isn't recorded in the ledger. Its values are validated before use.
- **Tests** in a new `crates/game/tests/finance.rs`, with a `[[test]]` entry:
  - wages with the carry over 12 months, which total exactly one year's rate;
  - upkeep for finished buildings, and none for sites;
  - a tax collector's return records tax income;
  - a demolished tax office loses nothing;
  - each tax level scales a house's tax;
  - the new year moves the accounts;
  - the debt counter and the overdue flag;
  - placement is refused in debt;
  - the ledger saves by name and refuses unknown names.
- **Scenario** `crates/game/tests/scenarios/finance.jsonl`, added to the CI loop:
  1. Build houses, a tax office and a workplace, and run a few months: taxes come in and wages go out.
  2. Set a low balance and stop tax collection: the city goes into debt, a gold placement fails, and `months_in_debt` counts up.
  3. Restore the balance: the counter resets.
  4. Open the finances dialog and check `ui.layout_issues` is 0.

## Phases

### Phase 0: One treasury and the ledger (medium)

- [x] `sim::finance::Finance` in `Simulation`, a signed balance in place of `GlobalTreasury`; signed `gold_units_total`; the HUD, queries and goal read it.
- [x] Tax collectors deposit into the treasury; remove `StockOrTreasury::Treasury` and `BuildingKind::treasury()`.
- [x] The ledger: categories, this year and last, saved by name; the year rollover; construction charges and refunds recorded.
- [x] Save version 6 and `v5_to_v6.py`:
  - move `Simulation.treasury.gold_units`, plus every tax office's gold, into the balance;
  - rewrite tax offices to `"None"`;
  - add empty accounts and the default rates.

  Then migrate the local saves, run `--smoke-test-saves` and regenerate `saves/sample_saves.zip`.
- [x] Tests: deposits, demolition, the ledger round trip, refunds; `determinism.jsonl` twice.
- [x] Roadmap: the tax office gold bug in "Fixes the research turned up".

**Notes (2026-11-07):**
- **`sim::finance`:**
  - `Finance` holds the balance (`i64`), the tax level and wage, `months_in_debt`, the monthly carries, and `this_year` and `last_year` (`YearAccounts`: the starting balance and a total per category).
  - The methods are `earn`, `spend` and `refund`, which record in the ledger, and `add_unrecorded` and `set_balance_unrecorded` for cheats and debug.
  - `on_calendar_events` runs after a world step's `SimCmds`, with the new-year rollover only for now.
  - `per_category_map` saves the per-category arrays by name, like `per_kind_map`.
  - `LedgerCategory` is `Taxes`, `Construction`, `Wages` and `Upkeep`; `TaxLevel` is `None` to `Harsh`, saved by name.
- **Deviation: everything Phases 1 and 2 save is in version 6 already.** The tax level, the wage, the debt counter and the carries are saved now, so the later phases need no second bump. `FinanceConfigs` (the default wage and tax level, the levels' percentages, the debt limit) came forward from Phase 1 to give them defaults.
- **Affordability reads the balance.** `Finance::can_afford` and `shortfall` replace the world-stats check, so the `_global` add and subtract helpers that kept the stats in step are gone. In debt, the shortfall is the whole cost. `CityStock::can_afford` takes the `Finance`. The HUD reads the balance too.
- **Renamed:** `StockOrTreasury` is now `ServiceStock`, and its field `service_stock`, since it no longer holds a treasury. The migration renames the key.
- **Tax deposits:** `ServiceBuilding::on_patrol_completed` pays a tax office collector's gold into the treasury, as `Taxes`. It runs deferred, with a mutable context.
- **Debug and UI:**
  - World Debug's Tax/Treasury tab shows the balance and the months in debt. Its Give Gold and Subtract Gold buttons don't record in the ledger; `ui_dev_menus.jsonl` still uses them.
  - Until Phase 2, the finances dialog's treasury heading shows the balance and this year's income and spending.
- **Migration:** the sample saves' tax offices held 84 to 12,272 gold in all, now in the balance. For example, `tiny_island` has 11,343 from its 5 offices.
- **Tests:** `tests/finance.rs`, registered in `Cargo.toml`, covers:
  - a tax office holds and takes no gold;
  - the stats count a negative balance;
  - a placement is recorded as construction;
  - a cancelled site takes its cost back;
  - cheats aren't recorded;
  - debt blocks a gold placement.

  Unit tests in `sim/finance.rs` cover the ledger, debt, the year rollover, saving by name, refusing unknown categories, and the tax levels.
- **Deviation: the collector's round trip isn't in the harness.** It needs houses with employed residents. It was checked in the game instead: `tiny_island`'s balance went from 11,343 to 12,100 over two months. Phase 2's `finance.jsonl` covers it in CI.
- **Verified:**
  - `cargo test --workspace`;
  - every CI scenario;
  - `determinism.jsonl` twice, with the same output and save (checksum `168927be62deb113` with `--seed 7` at 1280x720);
  - record and replay of `ui_build` and `determinism`;
  - the web target checks;
  - `--smoke-test-saves` loads all 13 migrated saves (the stale v2 `mcp_extended_test.json` set aside).

  The local v5 saves are backed up outside the repository.

### Phase 1: Wages, upkeep and debt (medium)

- [x] `FinanceConfigs` in `SimConfigs`: default wage, debt limit, the tax levels' percentages and the default level (done in Phase 0).
- [x] Monthly wages and upkeep with carries; the tile def `upkeep` field and its values (hand-made defs in `tile_set.json`, generated ones through the catalog and `sync_tileset.py`).
- [x] The debt counter, the overdue flag, and toasts.
- [x] `finance.state` and `finance.set`.
- [x] `tests/finance.rs` for wages, upkeep and debt.

**Notes (2026-11-07):**
- **Month-end charges:** `Finance::on_calendar_events` runs each new month's `on_month_end`, then the new year's rollover. It runs from `Simulation::update` once the step's `SimCmds` have run.
  - Wages are `population.employed` (recounted by the step's update) times the yearly wage.
  - Upkeep is `finance::yearly_upkeep`: the tile def `upkeep` of every finished building, read from its tile, so houses and sites pay none. It uses a new `World::for_each_building_of_any_kind`.
  - Each is a twelfth of the yearly amount, with its carry, so twelve months pay the year exactly. December's charges land in the year that ends.
- **Upkeep values** (gold per year), on the hand-made tile defs in `assets/tiles/objects/tile_set.json`: tax office, police station and apothecary 6; temple, hospital and theater 12; university 24; governor's palace 120; citadel 240. None of them is generated. The generator's catalog passes any `tile_set` field through, so a generated building takes `tile_set.upkeep`.
- **The debt clock:**
  - `months_in_debt` counts the month-ends in debt in a row, and resets once the balance is 0 or more.
  - `is_debt_overdue` is true at `FinanceConfigs::debt_limit_months` (12; 0 turns the limit off).
  - Notices are logged on the `finance` channel and queued like construction notices; `GameLoop::notify_finances` shows each as a toast. They fire on going into debt, each of the last three months before the limit, the limit itself, and getting out of debt.
- **Commands:**
  - `finance.state` reports the balance, the debt clock, the tax level and its percentage, the wage, the yearly upkeep, and both years' accounts with every category listed.
  - `finance.set` takes `tax_level`, `wage` (0–1000) and `balance` (not recorded), validates them all before changing anything, and reports the state.
- **Scenario change:** in `construction.jsonl`, the finished temple's upkeep is charged at the first month's end, before the cancelled site's refund is checked. Those two checks now read `/this_year/categories/Construction` (43: the road, the yard and one temple) instead of the balance.
- **In the game:** `tiny_island` over three months paid 453 in wages (303 workers) and 147 in upkeep (588 a year), and took 966 in taxes, so a city with tax coverage still gains. Set to a balance of 10, it went into debt at the next month's end with the notice "The treasury is in debt: -47 gold."
- **Tests:** in `tests/finance.rs`:
  - the month's end pays wages and upkeep;
  - sites pay no upkeep;
  - December's charges count in the old year;
  - wages run an empty treasury into debt until it's overdue, with the notices, and income clears it.

  Unit tests check that twelve months pay a year exactly, and the debt clock with its notices.
- **Verified:**
  - `cargo test --workspace`;
  - every CI scenario;
  - `determinism.jsonl` twice (checksum `3801c7bcb809de76` with `--seed 7` at 1280x720);
  - record and replay;
  - the web target;
  - `sync_tileset.py --check`.

### Phase 2: Tax rate, the dialog and missions (medium)

- [x] The tax level in house tax generation.
- [x] The finances dialog: yearly accounts, the rate controls and the debt status; `ui.layout_issues` at 0.
- [x] The HUD's negative balance.
- [x] The mission `finance` block.
- [x] `finance.jsonl`, added to CI; `ui_snapshot.jsonl` still lists the dialog.

**Notes (2026-11-07):**
- **Tax level:** `HouseBuilding::generate_tax` takes `Finance::tax_percent()`, the level's percentage from `FinanceConfigs::tax_level_percent`. `calc_household_tax` scales the base tax by it before the full-employment bonus and the rounding. A unit test checks each level.
  - The house inspector and the debug panel show the house's tax at the current level.
  - **Deviation:** `WorldStats::treasury.tax_generated` counts the base tax (`BASE_TAX_PERCENT`), since a building's tally has no finance to read. Nothing but the debug window shows it.
  - Taxes a house already owes keep their amount when the level changes.
- **The dialog** (`menu/dialog/city_management/finances.rs`):
  - "This Year's Accounts": each category with last year's figure in brackets ("Wages: -453 (last year -380)"), income positive and spending negative, then the net.
  - "Treasury": the balance with the debt clock ("-4826 gold, in debt: overdue in 10 months"), and the taxes the houses owe.
  - A "Tax Level" dropdown (None to Harsh) and a "Yearly Wage" slider (0–24), which write to the treasury as they change.
  - **Deviation:** the accounts are one list with last year in brackets, not two columns. Retail text widgets have no table, and a first layout of three sections overflowed the dialog. A long wage label also widened the rows past the scroll's edge; "Yearly Wage" fits.
  - `ui.layout_issues` is 0 at 1280x720 and on a 1440x795 window; the Ok button scrolls into view.
- **HUD:** `UiSizedTextLabel::set_alert` draws a label in the style's `error` color. The top bar turns it on for a negative balance.
- **Missions:** `MissionDef::finance` (`MissionFinanceDef`: `starting_gold`, `tax_level`, `wage`, all optional, `deny_unknown_fields`). `GameSession::start_mission`, which replaces `start_mission_calendar`, applies it with the calendar through `Finance::start_mission`. That gives a new treasury with fresh accounts and no debt; the map's balance stays if `starting_gold` is left out. No mission in `campaigns.json` sets one yet; that's M13.
- **Scenario** `finance.jsonl`, in the CI loop:
  1. Two level-1 houses, a market fed from a granary, and a tax office paid for from a storage yard (the worker cheat staffs it). After 300 s, taxes, wages and upkeep are all recorded.
  2. At tax level None with an empty treasury, the city goes into debt with the notice, and the clock counts two months.
  3. Paid off, the clock resets.
  4. The dialog has no layout issues, and its dropdown and slider set the tax level and wage.
- **Tests:** `test_mission_start_sets_the_finances` (a full block, a partial one, a misspelt field refused).
- **Verified:**
  - `cargo test --workspace`;
  - every CI scenario except `minimap.jsonl`, which fails only on an uncommitted local edit to `assets/configs/ui/minimap/dailong.json` (`map_size` 300 → 250) that isn't part of this work; the committed tree passes it;
  - `determinism.jsonl` twice (checksum unchanged, `3801c7bcb809de76`);
  - record and replay;
  - the web target.

### Phase 3: Documentation (small)

- [x] AGENTS.md: the finance model (one balance, yearly rates paid monthly, the ledger saved by name, how to add a category) and the new commands.
- [x] Roadmap: the M4 row and the save-version log. `.claude/rules/known-issues.md` if anything is left open.

**Notes (2026-11-07):**
- AGENTS.md has a "Finance" entry under the runtime architecture: the model, the commands, and how to add a ledger category. The Saves section has the rule that the accounts are saved by category name, and the scenario list describes `finance.jsonl`.
- Nothing is left for known-issues: what M4 leaves to later milestones (the effects of the tax level and wage on ratings, the overdue debt's failure, mission numbers) is in the roadmap's M9 and M13 entries.

## Risks

- **Signed gold everywhere.** `gold_units_total` and costs are `u32` at many call sites. The compiler finds them, but each comparison of a cost with the balance needs a deliberate cast.
- **Balance.** Gold costs are tiny next to taxes, and starting gold is 100. The default wage and upkeep values only make running costs matter. M13's balance pass sets the real numbers, mission by mission.
- **Old saves start paying.** A sample save with many workers and no tax coverage goes into debt after the migration. That's the system working, but check the smoke test runs long enough to show it doesn't crash, and note it like M5's pottery downgrades.
- **Determinism.** The money arithmetic uses integers, and charges happen once per month boundary in a fixed order. The house tax keeps its existing float rounding. `determinism.jsonl`'s checksum changes once, with the save version.

## Open decisions

Settled with the user on 2026-11-07:

1. **Tax offices hold no gold.** Collectors deposit straight into one city treasury. Losing a tax office to an enemy attack will count as plunder and take gold from the treasury (Phase 2 defence and attacks).
2. **The tax rate is five levels** for the player: None, Low, Normal, High and Harsh. Internally each one is a percentage of the base tax (0, 50, 100, 150 and 200 by default), from `FinanceConfigs`.
3. **Wages:** gold per worker per year, 6 by default, paid monthly with a carry. A court "standard wage" to compare with belongs to M9's Prosperity.
4. **Upkeep:** a tile def `upkeep` field, on civic and grand buildings only, with the values in [section 3](#3-wages-and-upkeep-phase-1).
5. **What debt does in M4:** it blocks building anything with a gold cost and starts the clock. Nothing else until M9.
6. **The debt limit:** 12 months, set in `FinanceConfigs`. M9 decides what an overdue debt does, and may change it per mission.
7. **Wages don't affect labour.** Wages feed Prosperity in M9.
8. **Accounts for this year and last year,** as GDD 5.7 says. A longer history, for a chart, can come later without a format change if each year is saved as its own entry.
9. **Save version 6 is finance;** terrain elevation moves to 7.
