# Ratings, goals and failure

**Status (2026-11-26): the open decisions confirmed; Phase 0 done (the ratings, their data and save v17), Phase 1 (the ratings on screen) and Phase 2 (goals and requests, save v18), Phase 3 (failure), Phase 4 (distinctions) and Phase 5 (custom games); Phase 6 (documentation) next.** This plan is milestone M9 of the [roadmap](roadmap.md). It covers:
- GDD 5.17: the four ratings, Prosperity, Culture, Security and Legitimacy, each with its own name for Legitimacy in each realm (the Mandate, Divine Favour, Merit);
- GDD 7's goals that are still missing, its failure conditions and its optional distinctions;
- the Legitimacy half of the court requests (GDD 5.14), which M10 left logged and not applied.

Campaign I teaches the ratings in two steps:
- I-3, The Village of Spirits, teaches "the Culture rating and the ratings panel" (goals "population 500; Culture 25; one festival held");
- I-5, The Mountain Market, teaches "the Mandate (Legitimacy) rating" ("trade income 1,000 a year; fulfil 3 court requests; Mandate 30").

I-4 asks for Prosperity 30 ([campaigns.md](../design/campaigns.md)). M13 writes those missions and balances every number here.

What this milestone leaves out:
- Defence, attacks, crime, unrest and monuments are game Phase 2's. Their drivers and the survival goals (`SurviveAttacks`, `SurviveUntil`, `MonumentComplete`) come with them.
- M11's advisors and event popups come later. Failure warnings go to the message log until then.

It was researched against `853baf3d` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase: the commit, deviations and what was verified. This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default, and the user confirmed them on 2026-11-25. Record any later answer there.
- Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows. The retail screens follow the realm UI's skins and mockups ([realm-ui.md](realm-ui.md)). List the UI changes in the phase notes.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo +nightly fmt --all`, both clippy runs, `cargo test --workspace` and the scenario scripts before committing.
  - Follow the Saves workflow for the format change in Phase 0.

## Goals

1. **Four ratings, from 0 to 100, recomputed each month.**
   - Each rating is the sum of its drivers. A driver is one thing that raises or lowers the rating: housing levels, temple coverage, fair taxes, court requests met. The player can always see why a rating is what it is: GDD 5.17's "a ratings panel explains what raises or lowers each one", and the mockups' breakdowns add up to the rating exactly.
   - Events fade. A festival, a request met or failed, a flood held back, a building burned down: each counts in full in the month it happens, then less each month until it's gone.
2. **Ratings are data, per realm.**
   - Which drivers feed which rating, their caps and their names live in one file per realm.
   - Đại Long's Mandate counts flood control (GDD §6: "virtue, fair taxes, flood control and loyalty").
   - Kambara's Divine Favour leans on the temples and the patrons' favour.
   - Tampadesa's Merit leans on temples and offerings. It gets the monasteries and donations when Campaign III brings them.
   - Nothing realm-specific is hard-coded.
3. **The missing goals.**
   - `Rating { rating, min }` ("Culture 25", "Mandate 30").
   - `HousesAtLevel { level, count }` ("15 houses at level 2").
   - A mission time limit.
   - `Resource` already stands for GDD 7's `Stockpile` (GDD 7, line 274); it's changed to count what's in storage.
4. **Court requests move Legitimacy.** A request met raises it by the request's `legitimacy`; one failed lowers it. After a failed court request, the court may ask for more: a larger request for the same good some months later.
5. **A mission can be lost.** Each of these fails the mission:
   - the debt has run its 12 months (`Finance::is_debt_overdue`);
   - Legitimacy has stayed near zero for a year ("the court recalls the governor");
   - its time limit has run out.

   Warnings come first. A failed mission's screen offers Try again, Load a save and Return to the menu.
6. **Distinctions.** A mission lists one or two optional goals, for example "Culture 60 or higher" or "never breach a dyke". Those met when the mission's goals are met are recorded in the profile, carved on the stele, and shown:
   - in the debrief;
   - on the campaign-complete screen ("8 of 12");
   - in the campaign picker.
7. **The player can see it all.**
   - The HUD's rings filled.
   - A City book Ratings page after `city-ratings-<realm>.jpg`.
   - The Overview's ratings card.
   - The debrief's distinctions and rating meters.
   - The failure screen.
   - Warnings in the message log.
   - Custom Game's goal sets, and a switch for failure.
8. **Later milestones can add to it.** Each adds its drivers to the realm files and the `Driver` enum:
   - M11 (advisors);
   - M13 (balance and the missions' blocks);
   - Phase 2 (defence, attacks repelled, crime and unrest, monuments, tribute, elite housing);
   - Phases 3 and 4 (the baray, monasteries and donations).

## What exists today

### Design
- **GDD 5.17 (lines 241–250):**
  - "Each rating runs from 0 to 100 and is updated monthly. A ratings panel explains what raises or lowers each one."
  - Prosperity: "Housing levels, treasury, trade income, employment, wages".
  - Culture: "Coverage by temples, schools, entertainment and festivals; finished monuments".
  - Security: "Defence coverage, attacks repelled, low crime and unrest".
  - Legitimacy: "Requests fulfilled, monuments, favour, fair taxes, disasters answered", named "the **Mandate** in Đại Long, **Divine Favour** in Kambara, **Merit** in Tampadesa".
  - §6 (line 260) gives each realm's Legitimacy its own sources:
    - the Mandate: "virtue, fair taxes, flood control and loyalty";
    - Divine Favour: "temples, monuments and the cosmic layout of the city";
    - Merit: "temples, donations and gifts to monasteries".
- **GDD 7 (lines 266–295):**
  - Goals come in three families: metrics, heritage and survival.
  - The goal table names `Rating { kind, min }` (Culture ≥ 40), `HousesAtLevel { level, count }` (30 houses at level 4 or above), `Stockpile { good, amount }` (400 bricks in storage), `MonumentComplete`, `SurviveAttacks` and `SurviveUntil`.
  - Line 274 says `Resource` is the code's form of `Stockpile`.
  - **Failure (291):** "the palace or citadel is destroyed in a survival mission; the city stays in debt too long; Legitimacy stays near zero for a year (the court recalls the governor); or a time limit runs out on the few timed missions."
  - **Difficulty (293):** "five levels … They scale costs, request sizes, attack strength and how fast favour fades."
  - **Distinctions (295):** "each mission has one or two optional goals for replay, for example finishing with Culture ≥ 60 or never breaching a dyke. These earn extra lines on the stele."
- **Other GDD lines:**
  - 196: "Trade income feeds Prosperity".
  - 200: requests "Fulfilling one raises Legitimacy and relations; failing lowers them, and the court may ask for more."
  - 170: crime and unrest come from "Poverty, unemployment, high taxes, unmet needs".
  - 186: festivals "raise Culture and favour, and calm unrest".
  - 212: monuments give permanent Culture, favour and Legitimacy.
- **campaigns.md:**
  - Campaign I's goals: I-1 "population 150; 10 houses at level 1"; I-2 "population 300; treasury 1,500; 15 houses at level 2"; I-3 "population 500; Culture 25; one festival held"; I-4 "survive 3 flood seasons without a breach; population 800; Prosperity 30"; I-5 "trade income 1,000 a year; fulfil 3 court requests; Mandate 30".
  - None of missions 1–5 has a distinction, a time limit or a failure note.
  - The only distinction named anywhere is I-9's "Optional distinction: no house burned."
  - Later missions ask for every rating: Security 40 in II-4, Merit 80 in III-10.
- **The mockups** (`docs/design/mockups/generator/`):
  - **HUD (`screens.py:200–212`, `rating_badge` 260–268):** four rings labelled P, C, S and the first letter of the realm's Legitimacy, each with its value. The data (`screens_content.py:35–38`, 74–77, 113–116) gives Đại Long's as 34, 21, 12 and 38.
  - **City book Ratings page (`pages.py:421–440`, `city-ratings-<realm>.jpg`):** a 2 × 2 grid of cards. Each card has the rating's name, a ring with a goal tick and its value, "Goal 25: met", "Goal 40: 6 to go" or "No goal in this mission", a list of signed drivers and a bold "Rating" total. The drivers add up exactly to the rating. Đại Long's data (`pages_content.py:38–44`):
    - Prosperity 34: Housing levels +12, Treasury +8, Employment +9, Wages paid +5.
    - Culture 21: Temples and shrines +11, Theatre +8, Schools +4, No festival this year −2.
    - Security 12: Police coverage +8, No defences −6, Calm streets +10.
    - The Mandate 38: Court requests met +16, Fair taxes +12, Floods answered +14, Late tribute −4.
  - **Kambara's and Tampadesa's breakdowns** (`pages_content.py:72–78`, 103–109) have other names: "Dance halls", "Pwè theatre", "Temple offerings +22", "Baray kept full +16", "Monks fed +20", "Alms given +18", "High taxes −4".
  - **City book Overview (`pages.py:223–239`):** a Ratings card of four rings, each "goal N" or "no goal", noted "the line marks the goal".
  - **Debrief (`screens.py:94–137`, `screens_content.py:24–29`, 63–68, 102–107):**
    - a Distinctions section noted "each one adds a line to the stele", with ticks: "Never run short of rice" ✓, "Mandate 50 or higher" ✗;
    - a Ratings section of four bars with values.
  - **Campaign complete (`dialogs.py:232–287`):**
    - a mark per distinction beside each mission;
    - "Distinctions 8 of 12";
    - the campaign picker's "{n} of 12 distinctions" (`dialogs.py:155`).
  - **Kambara's request popup (`dialogs_content.py:89–93`):** "When it is met: Divine Favour +8. If it is late: Divine Favour −10." The popup itself is M11's.
  - **No failure screen.** There's no failure, defeat or recall mockup.
  - **Discrepancies:**
    - The campaign-complete marks `[2,1,1,2,1,0,1,2,1,1]` add up to 12, against the text's 8 of 12.
    - Đại Long's and Tampadesa's HUD initials are both M.
    - Every mockup writes "Divine Favour", and `en.json:1073` writes "Divine favour".

### Code
- **Goals** (`campaign/config.rs`):
  - `MissionGoal` (524–542) has `Population`, `Employment`, `Treasury`, `Resource`, `FestivalsHeld`, `FloodsSurvived`, `TradeIncome` and `RequestsFulfilled`.
  - `is_met(&WorldStats)` (547–558) checks one goal; `MissionRequirements::all_met` (563) checks them all.
  - `Resource` counts `stats.resources.all`, which includes what walkers and houses hold.
  - Variants are matched in `is_met`, `dialog::goal_line` (`menu/dialog/mission_complete.rs:279–322`, used by the debrief, the briefing, the HUD's goals panel and the Overview) and the campaign debug window (`debug/misc.rs:240–261`).
- **Checking goals.** `campaign::tick` (`campaign/mod.rs:374–394`) runs from `CampaignSystem::update` in the world step, while the simulation isn't paused. Once the goals are met it:
  - sets `completed`;
  - queues `CampaignPrompt::MissionComplete`;
  - pushes the mission to `finished_missions`, which `GameLoop::record_finished_missions` records in the profile (`profile.rs:279–297`; the `TODO(realm-ui)` at 289 is where distinctions will be recorded).

  `CampaignPrompt` has `MissionComplete` and `CampaignComplete` only, so there's no failure path.
- **Mission blocks.** `MissionDef` (186–254) has optional `calendar`, `finance`, `risks`, `faith`, `floods` and `trade` blocks, each `#[serde(default, deny_unknown_fields)]` with `Option` fields. `GameSession::start_mission` (`session.rs:653–694`) applies them. `CampaignConfigs::validate` (623–716) checks them.
- **Distinctions.**
  - `MissionDef::distinctions: Vec<MissionDistinctionDef { id, stele_line }>` (284–292) is empty everywhere, and its ids are checked unique per campaign (689–705).
  - `CampaignRecord::distinctions: Vec<String>` in the profile (`profile.rs:66`) is never filled.
  - The stele already carves a distinction's line when the profile holds its id (`campaign/stele.rs:143–154`).
- **Stats.**
  - `WorldStats` (`world/stats.rs:112`) is recounted every step.
  - `HousingStats` has `total`, `lowest_level` and `highest_level`, with no per-level counts.
  - `CityReport::read` (`city/report.rs:118`) counts houses per level (`HousingRow`), and hungry and sick houses (locals at 135–151).
- **The month's order** (`sim/mod.rs:340–363`):
  1. finance's month-end: wages, upkeep, the debt clock;
  2. `floods::on_new_month`;
  3. `farms::on_new_month`;
  4. `trade::on_new_month`, which runs requests, prices and caravans;
  5. `faith::events::on_new_month`, which runs the drift, festivals and events.

  Each runs on a `SimContextMut` after the world step. `GameSystem::on_new_month` exists, but no system uses it.
- **Inputs already logged for M9:**
  - **Festivals:** `FestivalRecord { festival, scale, ordinal_month, culture, legitimacy }` in `CityFaith::log` (`sim/faith.rs:36–44`), filled with the festival's `culture` and `legitimacy` times the scale's `effect`.
    - Đại Long's festivals give Culture 5 to 7 and Legitimacy 2 or 3; Kambara's 5 to 9 and 3 to 5; Tampadesa's 4 to 7 and 3 to 6.
    - The Heritage page already shows them (`city_book/heritage.rs:388–399`).
  - **Favour:** per patron, from 0 to 100 (`CityFaith::favour`), with the `Mood` bands at 65, 50, 40 and 30. `faith::coverage` gives each patron's covered residents.
  - **Requests:**
    - `TradeRequest { …, legitimacy: i32, relation: i32 }` (`sim/trade.rs:141–154`) and `RequestRecord { request, closed, met }` in `CityTrade::request_log` (179–200), with `requests_met()` and `requests_failed()`.
    - `MissionRequestDef::legitimacy` is `#[serde(default)] i32` (`campaign/config.rs:462–475`).
    - No shipped mission has requests.
  - **Floods:** `FloodRecord { ordinal_month, reach, severity, cells, buildings_damaged, fields_flooded, breaches }` in `CityFloods::log` (`sim/floods.rs:114–124`), commented "for … M9's 'Floods answered'".
  - **Finance:**
    - `TaxLevel` (None to Harsh; `sim/finance.rs:150`), the wage (default 6 a worker a year), `months_in_debt` and `is_debt_overdue` (477), and the year's accounts by category (Exports among them).
    - `FinanceConfigs::debt_limit_months` (12) is commented "(a mission failure from M9)".
  - **Risks:** `CityRisks::disease_deaths` is saved. Fires and collapses aren't counted: `building/destruction.rs` logs them and queues a toast, and nothing keeps a tally. `RiskSummary::of` reads the risks as they are now.
  - **History:** `CityHistory` (`city/history.rs`) samples population, the employed and the balance each month. It's runtime only.
- **Service access.**
  - `BuildingContext::has_access_to_service` (`building/mod.rs:1839`) is a proximity test (`SimContext::is_near_building`) from a house's road link within the service's `effect_radius`. It's what the house levels' needs use.
  - Walker visits by actors, teachers, monks and guards record nothing on a house.
  - The culture services are the Theater (its walker the Actor) and the University (the Teacher); the temples and shrines are covered by faith; the PoliceStation sends a Guard, which does nothing else yet.
  - There's no school or monastery building kind.
- **Placeholders waiting for M9:**
  - The top bar's `ratings` canvas: rings with "—", value `{placeholder: true, names}`, tooltip `rating.placeholder` (`menu/in_game/top_bar.rs:577–608`). `rating_names(realm)` is at 144–152.
  - The Overview's `standing` canvas (`city_book/overview.rs:117–155`).
  - The City book's page list, which waits for "M9's Ratings" (`city_book/mod.rs:9`).
  - The debrief's room between the goals and the stats (`mission_complete.rs:81–85`, 582–584).
  - The campaign-complete screen's distinctions (`campaign_complete.rs:229–231`) and the campaign picker's (`campaign_picker.rs:237–239`).
  - Custom Game's Goals dropdown, Sandbox only (`menu/dialog/custom_game.rs:892`), and its stored, unused `Difficulty` (`custom_game/mod.rs:39–46`, 180).
  - The skins' `hud.colors.rating_fill`, which nothing reads yet.
- **Strings:**
  - `rating.prosperity`, `rating.culture`, `rating.security` and `rating.placeholder` (Hud, `en.json:465–468`).
  - `rating.<realm>.fourth` (Realm, 1072–1074: "The Mandate", "Divine favour", "Merit").
  - `book.ratings*` (City, 816–818).
- **Automation:** nothing reports goals or ratings directly. Scenarios read the widgets' values: `ui_hud.jsonl:16–17`, 66 and 74 assert the placeholder and the names.
- **Save version:** 16. The roadmap's log has "Ratings state | M9 | Planned".

## Design

### 1. Ratings as data (Phase 0)

- **One file per realm, `assets/configs/game/ratings/<realm>.json`** (`ratings::RatingConfigs`). It's loaded at boot after the faith files and checked. Each of the four ratings lists its drivers, and each driver has:
  - `driver`, its kind;
  - `max`, its cap;
  - for a driver that can go negative, its `min`;
  - optional parameters by kind.

  Driver names are text keys:
  - `rating.driver.<id>` in Hud, the general name ("Housing levels");
  - `rating.driver.<realm>.<id>` in Realm where the realm names it its own way ("Dance halls", "Pwè theatre").
- **The kinds are code** (`ratings::Driver`), each a pure function of the city at the month's start. A realm uses the kinds that fit its rating, so Kambara's Divine Favour takes `Favour` with a higher cap than the Mandate's. Kinds whose systems don't exist yet aren't in M9's enum.
- **The kinds Phase 1 has:**
  - **Prosperity:**
    - `HousingLevels`: the residents' mean house level over the top level, times `max`.
    - `Treasury`: gold per resident against a `per_resident` mark, times `max`. In debt it's negative, down to `min`.
    - `Employment`: the share employed of the workforce, less unemployment beyond a `free_share`.
    - `Wages`: the wage against the realm's standard wage (the finance default, 6), from `min` to `max`.
    - `TradeIncome`: exports this year or last against a `per_resident` mark.
  - **Culture:**
    - `FaithCoverage`: the share of residents in reach of a working shrine or temple, as faith's coverage counts them.
    - `ServiceCoverage { service }`: the share of houses with access to a kind, the Theater or the University, through `has_access_to_service`, as the house levels test it.
    - `Festivals`: the culture of the festivals held, fading, with `none_penalty` when festivals are on and none was held in the last 12 months (the mockup's "No festival this year −2").
  - **Security:**
    - `ServiceCoverage { service: PoliceStation }`.
    - `RiskSafety`: the share of buildings with risks below Medium (`RISK_MEDIUM`, 40). It reads 1 while risks are off.
    - `CalmStreets`: `max` less unemployment, hungry houses and High or Harsh taxes. It stands in for GDD 170's causes of unrest until game Phase 2 brings crime and unrest.
    - `Disasters`: buildings burned down or collapsed and deaths from sickness, fading. It's negative.
  - **Legitimacy:**
    - `Requests`: each request met adds its `legitimacy`, and each failed takes away its `legitimacy` × `failed_scale`, fading. The mockups' "Court requests met" and "Late tribute".
    - `FairTaxes`: a value per tax level, from the file. Normal is fair, None and Low are generous, High and Harsh lose.
    - `Floods`: floods held back (a `FloodRecord` with no breach) add, and breaches and buildings damaged take away, fading. It's offered only while floods are on. Đại Long's Mandate has it.
    - `Favour`: the mean favour of the realm's patrons over 100, times `max`. Kambara's and Tampadesa's take it with larger caps, standing in for "Temple offerings" and "Monks fed" until those systems come.
    - `Festivals { legitimacy: true }`: the festivals' legitimacy, fading.
- **Fading** (Q2): an event counts its value × (1 − age / `fade_months`), from the month it happened; months are ordinal months. `RatingRules::fade_months` is 12 by default, and a driver may set its own: a request counts 24 months.
- **Rules** (`RatingRules` in `GameConfigs::sim.ratings`, the pattern of `FaithRules`): `fade_months`, the failure thresholds (§4), `request_legitimacy`, `failed_scale`, and the court asking again (§3). Like the other rules, they're "to be balanced in M13".
- **Checks** (`RatingConfigs::validate`), each logged on the `campaign` channel:
  - every realm has all four ratings;
  - the caps sum to at most 100 and the floors to at least −100;
  - every key exists;
  - no kind appears twice in one rating.

### 2. The ratings state (Phase 0)

- **`sim::ratings::CityRatings`**, saved in `Simulation` (save v17), read through `SimContext::ratings()` and changed through `ratings_mut()`. Per rating it holds its value (0–100) and the drivers of the last recompute (`DriverValue { id, value }`, whole numbers). It also holds the failure clocks (§4) and the distinctions' state (§5).
  - Drivers are rounded before they're summed, so the breakdown always adds up to the rating shown. The sum is clamped to 0–100, and a clamp shows as no driver.
- **`ratings::recompute(context)`** reads the realm's file and the city and writes the values. It runs at these points:
  - each month, last in `Simulation::update`'s month (after faith's rolls, so the month's festival and the requests opened or failed count);
  - when a mission or custom game starts, or a session is created;
  - from `ratings.recompute`.

  Loading a save doesn't recompute: the saved values stand until the next month, as the City book's figures do.
- **The disasters' log** (on `CityRisks`, saved with it in v17): a `DisasterMonth { ordinal_month, burned, collapsed, houses_lost }` for each month in which a building was destroyed, filled by `destruction::destroy_building`. With `disease_deaths` kept per month the same way, it feeds `Disasters` and the distinction "no house lost" (§5).
- **What stays out of the save:** the drivers' inputs are read afresh each month from the saved systems: finance, faith's log, floods' log, trade's request log and the disasters' log.

### 3. Goals and requests (Phase 2)

- **`MissionGoal::Rating { rating: RatingKind, min }`**. `RatingKind` is Prosperity, Culture, Security or Legitimacy, spelled `legitimacy` in data whatever the realm calls it. The goal is met while the rating's value is at least `min`, read from `WorldStats::ratings`, which `CityRatings::tally` fills. Its line in the HUD, briefing and debrief uses the realm's name for it ("The Mandate 30").
- **`MissionGoal::HousesAtLevel { level, count }`** counts finished houses at `level` or above. `level` is the number the player sees, from 1, as the City book's housing rows show it (Q5). `HousingStats` gains `at_level: [u32; HouseLevel::count()]`, filled by `update_housing_stats`.
- **`Resource` counts what's in storage** (`resources.storage_yards` and `granaries`), as GDD 7's `Stockpile` ("400 bricks in storage") does (Q6). Its label becomes "in storage". `tests/campaign.rs:223–227` changes with it.
- **A time limit:** `MissionRequirements::time_limit_months: Option<u32>`, the calendar's months elapsed since the mission's start (`GameCalendar::months_elapsed`). The goals panel shows the months left. Failure comes when they run out (§4).
- **Requests move Legitimacy** (Q4):
  - **Effect:** `MissionRequestDef::legitimacy` becomes `Option<i32>`. Left out, it takes `RatingRules::request_legitimacy` (6). It's resolved when the request opens, into `TradeRequest::legitimacy` as today. The `Requests` driver reads the request log.
  - **The court asks again:** a failed court request (`requests::is_court`) schedules another for the same good, at `ask_again_scale` (1.25) of the amount, `ask_again_months` (3) later, with the same months to meet it. It's a request like any other, logged as asked again, so the player sees the cost of failing.
- **The goals panel is already live** (realm-ui Phase 5 reads it each frame). The roadmap's "Goals panel in the HUD, live" is done. M9 adds the new goals' lines and the time limit's line.

### 4. Failure (Phase 3)

- **Three conditions,** checked at each month's end after the ratings are recomputed (`ratings::check_failure`). Each can be turned off by a mission's new `failure` block (`MissionFailureDef { debt, legitimacy }`, both on by default) or by a custom game's switch (§7):
  - **Debt:** `Finance::is_debt_overdue` (12 months in debt, `FinanceConfigs::debt_limit_months`). Finance's notices already warn in the last three months.
  - **Legitimacy:** the rating below `RatingRules::recall_below` (10) for `recall_months` (12) month-ends in a row. The months counted are saved in `CityRatings`. A Warning comes when the count starts, and again in its last three months.
  - **Time:** `time_limit_months` has passed and the goals aren't met. Warnings come at 12, 6, 3 and 1 months left.
- **A mission already finished can't fail.** Its goals were met, and the player is playing on.
- **On failure** (`campaign::fail(reason)`):
  - The active mission is marked failed (`ActiveMission::failed: Option<FailureReason>`, saved with the campaign's progress).
  - The simulation pauses.
  - `CampaignPrompt::MissionFailed` opens the failure screen.
  - The autosave stops for the session. A save made after the failure, which only the developer editor can make, loads failed and opens the screen again.
- **The failure screen** (`MissionFailed`, full screen, in the debrief's layout on the realm's backdrop; no mockup, Q8):
  - **The reason** in the realm's words, from Realm keys `failure.<realm>.<reason>`: "The court has recalled you", "The king withdraws his favour", "The sangha turns away".
  - **The mission's name, place and date.**
  - **The goals as they stood,** in the debrief's table.
  - **The stats:** population, treasury and years taken.
  - **The buttons:**
    - **Try again** (`home::start_campaign_mission` for the same mission), which plays no story again, since the profile has seen it;
    - **Load a save**, which opens Load / Save in load mode;
    - **Return to the menu**.

  There's no Keep playing (Q7). Escape opens the Return to the menu popup. Nothing is recorded in the profile.

### 5. Distinctions (Phase 4)

- **`MissionDistinctionDef` gains `condition`** (`DistinctionCondition`), checked when the mission's goals are met. Its forms:
  - `Goal(MissionGoal)`: any goal, met at that moment ("Culture 60 or higher", "Mandate 50 or higher").
  - `NoDebt`: never in debt since the mission's start.
  - `NeverBreached`: no `FloodRecord` with a breach ("never breaching a dyke").
  - `NoHouseLost`: no house burned down or collapsed (I-9's "no house burned").
  - `NeverShortOf { good }`: no month-start since the mission's start without the good in storage while people lived in the city ("Never run short of rice").
  - `Within { months }`: the goals met within that many months ("Finish before year 38").

  The "never" forms are tracked month by month in `CityRatings::spoiled`, the ids of the distinctions already out of reach. That's saved, so a loaded game remembers.
- **Award:** the debrief reads `campaign::distinctions_earned()`. The ids are recorded with the finished mission (`Profile::mission_finished`, the `TODO(realm-ui)` at `profile.rs:289`), added to what an earlier play earned, so a replay can earn the rest.
  - `CampaignRecord::distinctions` exists, so the profile's version stays 1.
  - The stele's lines already follow (`campaign/stele.rs:143`).
- **Checks:** a distinction needs a `condition` and a `stele_line` (a Story key). A mission has at most two (GDD 295).
- **Campaign I's missions get none in M9.** M13 writes them with the missions. The hidden test campaigns get two each, so the screens and scenarios have something to show.

### 6. What the player sees

- **HUD top bar:**
  - Each ring is filled to its value in `rating_fill`, with the number in place of "—".
  - A goal's tick is on the ring.
  - The tooltip gives the name, the value, the goal and the two largest drivers each way.
  - The canvas's value becomes `{ names, values, goals }`.
- **City book Ratings page** (`CityPage` `ratings`, after Overview, as `city-ratings-<realm>.jpg` lays it out): a 2 × 2 grid of cards. Each card has the rating's name, a ring with its goal tick, "Goal N: met" or "N to go" or "No goal in this mission", the drivers with their signed values in the realm's names, and the "Rating" total. Canvases are `CityManagement/Ratings/<rating>`, value `value`, `goal` and `drivers`.
- **City book Overview:** the ratings card's rings are live, each with its goal.
- **Debrief** (in the room between the goals and the stats):
  - a Distinctions table: a row per distinction with its badge, as the goals' (`UiTableStatusLook::Badges`), noted "each one adds a line to the stele";
  - four rating meters with their values.
- **Campaign complete:**
  - Distinctions "N of M" in the stats;
  - a mark per distinction earned beside each mission in the missions table.
- **Campaign picker:** the card's "N of M distinctions".
- **Briefing:** the goal lines for the new goals, and the time limit.
- **Failure screen:** §4.
- **Messages:**
  - The failure warnings are Warnings with the Alert sound.
  - A rating goal met says nothing of its own; the goals panel shows it.
  - M11's event popups come later.

### 7. Custom games (Phase 5)

- **Goal sets** in `maps.json`'s new `goal_sets` (each an id, a Menu name key and a `MissionRequirements`): Sandbox (none), "A thriving town" (population 1,000, Prosperity 40) and "A city of culture" (Culture 50, two festivals). Picking one makes the custom game's goals. Meeting them opens a short debrief: no stele and no world map, Keep playing or Return to the menu.
- **A Failure switch** in Custom Game ("Debt and the court can end the game"), off by default: overview.md's "relaxed builder" plays without failure. On, it fails the game as §4 does, with Try again restarting the custom game with the same options.
- **Difficulty stays stored** (Q10).

### 8. Automation, tests and scenarios

- **Commands:**
  - `ratings.state`: each rating's value, goal and drivers, the failure clocks and the distinctions' state.
  - `ratings.recompute`.
  - `ratings.set rating=... value=N`: until the next recompute, for tests.
  - `goals.state`: the mission's goals with target, achieved and met, the time limit's months left, its distinctions (earned, spoiled, open) and whether it's failed.
  - `campaign.fail reason=debt|legitimacy|time`, to test the failure screen.
- **The developer UI:**
  - World Debug gets a Ratings tab with the values, the drivers, Recompute and the failure clocks.
  - The campaign debug window lists the new goals and the distinctions.
- **Tests:** a new `tests/ratings.rs` checks:
  - each driver on a small city;
  - fading;
  - the clamp;
  - each realm's file;
  - requests' Legitimacy and the court asking again;
  - each failure, and that a finished mission doesn't fail;
  - each distinction condition.

  `tests/campaign.rs` checks the new goals and the award into the profile.
- **Scenarios:**
  - **A new `ratings.jsonl`:**
    - builds a small city;
    - checks each rating rises with what drives it: a shrine, a theater, a festival, a request met;
    - checks the City book's Ratings page and the HUD's rings;
    - runs the debt to its limit in a test mission and checks the failure screen and Try again.
  - **`ui_hud.jsonl`** loses the placeholder assertions.
  - **`ui_intermission.jsonl`** checks the debrief's distinctions and meters on a hidden test campaign.
  - **`ui_city.jsonl`** visits the new page in each realm.
  - **`ui_dev_screens.jsonl`** opens the failure screen from the Screens menu.

## UI changes

| Where | Change | Phase | Files |
|---|---|---|---|
| HUD top bar | Rings filled to the ratings, values, goal ticks, tooltips with the drivers. | 1 | `menu/in_game/top_bar.rs` |
| City book | A Ratings page after the mockup; the Overview's ratings card live. | 1 | `menu/dialog/city_book/ratings.rs` (new), `overview.rs`, `mod.rs` |
| HUD goals panel, briefing, debrief | The `Rating` and `HousesAtLevel` lines, "in storage" for `Resource`, the time limit. | 2 | `mission_complete.rs` (`goal_line`), `mission_briefing.rs`, `top_bar.rs` |
| New screen | `MissionFailed`: the reason, goals, stats, Try again, Load a save, Return to the menu. No mockup. | 3 | `menu/dialog/mission_failed.rs` (new) |
| Message log | Failure warnings (debt's already exist). | 3 | `game_loop.rs` |
| Debrief | The distinctions table and the four rating meters. | 4 | `mission_complete.rs` |
| Campaign complete, campaign picker | Distinctions "N of M", marks per mission. | 4 | `campaign_complete.rs`, `campaign_picker.rs` |
| Custom Game | Goal sets in Goals; a Failure switch; a short custom-game debrief. | 5 | `menu/dialog/custom_game.rs`, `custom_game/` |
| Developer UI only | World Debug's Ratings tab; the Screens menu's Mission failed. | 0, 3 | `debug/world.rs`, `debug/screens.rs` |

## Phases

### Phase 0: The ratings, their data and the save format (medium to large)

- [x] `assets/configs/game/ratings/<realm>.json` for the three realms, `RatingConfigs` loaded and checked, and the driver name keys.
- [x] `ratings::Driver` with the kinds in §1, each a function of the city; `RatingRules` in `GameConfigs::sim`.
- [x] `sim::ratings::CityRatings` in `Simulation`, its accessors on the contexts, and `recompute` monthly and at a session's or game's start.
- [x] `CityRisks`' disasters log, filled by the destruction path.
- [x] Save v17, `v16_to_v17.py` (empty ratings and log), migrated sample saves, `--smoke-test-saves`; the roadmap's log.
- [x] `ratings.state`, `ratings.recompute`, `ratings.set`; World Debug's Ratings tab.
- [x] `tests/ratings.rs`: drivers, fading, the clamp, each realm's file, the round trip.

**Notes (2026-11-25):**
- **What was built:**
  - `sim::ratings` (`CityRatings`, `Rating`, `DriverValue`, `RatingKind`, `FailureReason`, `FailureSwitches`) in `Simulation`, with `ratings()` and `ratings_mut()` on the simulation and both contexts.
  - `crate::ratings`:
    - `config`: `RatingConfigs`, `RealmRatingsDef`, `DriverDef`, `DriverKind`, its checks, `driver_name` and `rating_name`;
    - the drivers: `recompute`, `rate`, `CityFacts`, `fade`, and `on_new_month`, last in the month's order;
    - `request_recompute` for World Debug.
  - `RatingRules` in `GameConfigs::sim.ratings`.
  - The three realm files, `assets/configs/game/ratings/<realm>.json`. Each rating's caps add up to 100.
  - The driver names: `rating.driver.<id>` (Hud), and Kambara's and Tampadesa's own (Realm: "Dance halls", "Pwè theatre", "Temple offerings", "Alms given", "Guard posts").
  - `CityRisks::disasters` (`DisasterMonth`, `record_disaster`; `add_disease_deaths` takes the month), filled by `destruction::destroy_building` and the deaths from sickness. `GameCalendar::ordinal_month`.
  - The session rates the city when a mission or custom game starts (`GameSession::rate_city`, after `CityRatings::start`), and when a new map is made or a save loads while it's unrated (`rate_city_if_unrated`). A save's ratings otherwise stand until the next month.
  - Save v17 and `v16_to_v17.py`, with the local saves and `sample_saves.zip` migrated.
  - `ratings.state`, `ratings.recompute` and `ratings.set`, and World Debug's Ratings tab.
  - `tests/ratings.rs`, 17 tests; `tests/risks.rs`'s round trip checks the new layout.
- **Deviations:**
  - **Every M9 save change is in v17.** The failure switches, the Legitimacy clock, the failure and the spoiled distinctions came now, though Phases 3 and 4 use them, so those phases need no bump of their own.
  - **The failure is kept on `CityRatings::failure`, not `ActiveMission::failed`,** so a custom game can be lost too (Phase 5). A campaign mission starts with both failures on (`FailureSwitches::ON`) until Phase 3's `failure` block. Every other game starts with them off, and nothing checks them yet.
  - **Coverage is counted along the grid, not with `has_access_to_service`.** `ServiceCoverage` and `FaithCoverage` count the residents of the houses within a working building's effect radius, as faith's coverage does (`CityFacts::residents_reached`), so no path search runs per house; it ignores heights, as Q9 has it. The share is weighted by residents, not by houses.
  - **Drivers a game doesn't offer are left out of the breakdown,** where zeros would be misleading: trade income where the game doesn't trade, requests where it has none, floods where the river doesn't flood, favour without patrons. The festivals' penalty shows as its own id, `no_festival`.
  - **Parameterless drivers are empty struct variants** (`{"kind": "favour"}` reads as `Favour {}`). Serde ignores unknown fields on an internally tagged unit variant, so a typo would have passed silently.
  - **The treasury counts at least one resident,** so an empty city with gold rates its treasury in full.
  - **The realm files are whole, not merged over Đại Long's** (as faith's are).
- **UI changes:** World Debug's Ratings tab (developer UI). Nothing retail changes before Phase 1.
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - `--smoke-test-saves` on the migrated saves, each rated as it loads;
  - `ratings.state`, `ratings.set` and `ratings.recompute` in script mode on preset 2;
  - all 29 scenarios on a release build (macOS).

### Phase 1: The ratings on screen (medium)

- [x] The top bar's rings, values, goal ticks and tooltips; the canvas's new value.
- [x] The City book's Ratings page in each realm, and the Overview's live card.
- [x] `ui_hud.jsonl` and `ui_city.jsonl` updated; the start of `ratings.jsonl`.

**Notes (2026-11-25):**
- **What was built:**
  - `ratings::views` (`RatingView`): what the screens show of each rating, in a realm's words: its name, value, the mission's goal for it (`ratings::mission_goal`) and its drivers' names and values, with `largest_drivers` for the tooltip. `ratings::values` gives the four values.
  - `UiPainter::arc` (engine): a circle's arc in turns clockwise from the top, its ends rounded, for a ring filled to its value.
  - The HUD's `TopBar/ratings`: a ring per rating filled in the skin's `hud.colors.rating_fill` (which nothing read before), the goal ticked across it, the initial in it and the value beside it, "—" while unrated. Its tooltip gives each rating, with its goal, and its two largest drivers each way. The bar reads the ratings each frame with its other stats; the canvas's value is `rated`, `names`, `values` and `goals`.
  - The City book's Ratings page (`city_book/ratings.rs`), after `city-ratings-<realm>.jpg`: a card per rating (`CityManagement/Ratings/<rating>`) with its ring (`Paint::ring`, pages.py's), "Goal N: met", "Goal N: M to go" or "No goal in this mission", the breakdown in the realm's words and the "Rating" total, its rows set closer where a rating has more drivers than the mockup's four.
  - The Overview's `standing` card's rings, live, with "goal N" or "no goal" under each.
  - The book is rebuilt when the ratings' values change (`BuildKey::ratings`), so it follows `ratings.recompute` and `ratings.set` as well as the month.
  - The strings: `rating.not_rated`, `rating.tooltip`, `rating.tooltip_goal` (Hud); `book.page.ratings`, `book.note.ratings`, `book.rating.*`, `book.ratings.goal` and `book.ratings.no_goal` (City). `rating.placeholder` and `book.ratings.not_yet` are gone.
  - `ratings.jsonl` (in CI's list), `ui_hud.jsonl`'s rings, and `ui_city.jsonl`'s Ratings page in each realm and at 200% and 75% UI scale.
- **Deviations:**
  - **The Ratings tab follows Finances,** where the mockups' tab column puts it (Overview, Population, Goods, Finances, Ratings, Labour, Food, Heritage), not Overview as §6 said. Heritage moves down one.
  - **No goal ticks yet.** No mission can set a rating goal until Phase 2's `MissionGoal::Rating`. The rings, cards and tooltip draw one from `ratings::mission_goal`, which returns none until then.
  - **The clamp can show.** A rating whose drivers add up below 0 is shown at 0 over drivers adding to less (the plan's "a clamp shows as no driver"). The mockups' breakdowns always add up because their numbers were picked to.
- **UI changes:** the HUD's ratings (rings, values, tooltip); the City book's Ratings tab and page; the Overview's ratings card.
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - `ratings.jsonl` (94 steps), `ui_hud.jsonl` and `ui_city.jsonl` on a release build (macOS), and every other scenario;
  - the screenshots: the Ratings page in each realm's look and at 200%, the HUD's rings and tooltip, the Overview's card.

### Phase 2: Goals and requests (medium)

- [x] `MissionGoal::Rating` and `HousesAtLevel`; `HousingStats::at_level`; `WorldStats::ratings`; their lines everywhere `goal_line` is used, and the debug window's.
- [x] `Resource` counted in storage, labelled so.
- [x] `MissionRequirements::time_limit_months` and its goals-panel line.
- [x] Requests' Legitimacy applied (`Option` in the config, the rules' default); the court asking again after a failed court request.
- [x] `goals.state`; tests in `tests/campaign.rs` and `tests/ratings.rs`; `ratings.jsonl` meets a `Rating` goal in a test mission.

**Notes (2026-11-25):**
- **What was built:**
  - **The goals.** `MissionGoal::Rating { rating, min }` and `HousesAtLevel { level, count }` (the level from 1, at it or above). `MissionGoal::target` and `achieved`, which `is_met` and the goal lines share.
  - **`MissionRequirements`:**
    - `time_limit_months`, with `months_left` (the calendar's months since the mission's start);
    - `rating_goal`, the highest goal for a rating;
    - `problems`, which `CampaignConfigs::validate` reports: a rating goal above 100, a level outside 1 to 5, a time limit of 0, a time limit without goals.

    It now refuses unknown fields.
  - **The world stats:**
    - `HousingStats::at_level` and `at_or_above`;
    - `RatingStats`, filled by `CityRatings::tally`;
    - `GlobalResourceCounts::in_storage`, which `Resource` now counts (the storage yards and granaries).

    `World::refresh_stats` takes a `StatsSources` (`Simulation::stats_sources`, `SimContext::stats_sources`), the simulation's state the stats count, in place of four arguments.
  - **What the screens show:**
    - `ratings::mission_goal` reads the mission's rating goals (`campaign::active_requirements`), so the HUD's rings, the City book's cards and the Overview tick them.
    - `dialog::goal_line` takes the realm: a rating's line is its name in the realm's words ("The Mandate"), and a houses goal's "Houses at level 2 or above".
    - The briefing lists a time limit under the goals ("Time limit", "2 years").
    - The goals panel has a row for it, `Goals/time_left` (value `months_left`, `limit`): a clock, the months left and a bar, in the skin's warning colour in its last 3 months.
    - The developer campaign window lists the new goals and the limit.
  - **Requests' Legitimacy:** `MissionRequestDef::legitimacy` is optional. A court request without one takes `RatingRules::request_legitimacy` (6), resolved as `trade::start` schedules it. `requests.open` takes `legitimacy` too.
  - **The court asks again** (`requests::ask_again`, from the month's request handling):
    - A failed court request schedules `TradeRequest::asked_again`: the same good from the same asker, 1.25 times the amount rounded up, 3 months later, with as many months to send it.
    - Its id is `<first id>_again_<N>`, and `again` counts the times asked. A mission's request ids can't hold `_again_` (`trade_problems`).
    - It's a request like any other: failing it is asked again too, for more again.
    - Its messages: "The court's request for 20 pottery went unmet. It will ask again, for more." and "The court asks again, for more: 25 pottery within 2 months." The Goods page titles it "Pottery for the court, asked again".
  - **Save v18:** `TradeRequest::again`, with `v17_to_v18.py`; the local saves and `sample_saves.zip` migrated.
  - **Automation:** `goals.state` (each goal's `def`, line, target, achieved and met; `all_met`, `completed`, the time limit, `failure`); `ratings.state`'s `goal`; `requests.state`'s `legitimacy` and `again`.
  - **The hidden test campaign `test_goals`:** one mission on preset 2 asking Culture 60, the Mandate 20 and a house at level 1 within 24 months.
  - **Tests:**
    - `tests/campaign.rs`: the rating and houses goals, `Resource` in storage, the time limit and the goals' problems.
    - `tests/ratings.rs`: the stats' ratings and houses from a real city, and a request asked again failing again.
    - `tests/trade.rs`: the court asking again through the month, the Legitimacy defaults, and the reserved id.
  - **Scenarios:**
    - `ratings.jsonl` plays `test_goals`: the briefing's goal lines, `goals.state`, the rings' ticks, the goals panel and its time limit (a month on, a month less), the City book's cards, then `ratings.set` meeting the goals and the debrief.
    - `trade.jsonl`: the court's default Legitimacy, a court request for bricks failing and asked again for 500, the Mandate's `requests` driver below 0, and the request asked again kept through a save.
- **Deviations:**
  - **Save v18.** A request asked again had to say so, which changes `TradeRequest`; Phase 0's "every M9 save change in v17" didn't foresee it.
  - **Only the court's requests take the default Legitimacy.** Another city's take none unless their mission gives one: they move relations (game Phase 2), not the court's favour. §3 said every request.
  - **A request asked again keeps the failed one's Legitimacy.** Only its amount grows.
  - **The time limit runs out without consequence until Phase 3,** which fails the mission. Its months left show from now.
- **UI changes:**
  - the goal lines in the briefing, the debrief, the HUD's goals panel and the Overview (the rating's realm name, the houses' line);
  - the briefing's time limit row;
  - the goals panel's time left row;
  - the rings' and cards' goal ticks, now that missions set them;
  - the Goods page's title for a request asked again.
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - `--smoke-test-saves` on the migrated saves;
  - every scenario on a release build (macOS), `determinism.jsonl` twice;
  - the screenshots: the briefing's goals, the goals panel's time row, the City book's goal ticks and the debrief.

### Phase 3: Failure (medium)

- [x] `MissionFailureDef` and its checks; `ratings::check_failure` for debt, Legitimacy and time; the Legitimacy clock saved.
- [x] `campaign::fail`, `ActiveMission::failed`, `CampaignPrompt::MissionFailed`, the pause and the autosave stopped.
- [x] The `MissionFailed` screen in each realm, its three buttons, Escape's popup, the realms' failure strings.
- [x] The warnings in the message log.
- [x] The Screens menu's Mission failed preview; `campaign.fail`.
- [x] Tests for each condition and for a finished mission; `ratings.jsonl` fails a test mission on debt and tries again.

**Notes (2026-11-25):**
- **What was built:**
  - **The mission's `failure` block** (`MissionFailureDef { debt, legitimacy }`, each on unless set false; `switches()`), read as the mission starts (`campaign::active_mission_failure`, into `CityRatings::start`). A time limit can always fail the mission.
  - **`ratings::failure`** ([ratings/failure.rs](../../crates/game/src/ratings/failure.rs)), run by `ratings::on_new_month` once the ratings are recomputed:
    - `mission_terms`: whether the mission is finished, its time limit and whether its goals are met. None outside a campaign mission, where nothing is lost.
    - `check_failure`: moves the Legitimacy clock (`legitimacy_low_months`, month-starts in a row below `recall_below`; 0 while that failure is off), then loses the mission for debt (`Finance::is_debt_overdue`), Legitimacy (`recall_months` in a row) or time (the limit's months passed, the goals unmet), in that order. The failure is kept on `CityRatings::failure`. A finished or lost mission isn't checked.
    - The warnings, as `FailureNotice`s: Legitimacy below the mark as the clock starts and in its last three months, and 12, 6, 3 and 1 months of the time limit left. `GameLoop::notify_failure_warnings` posts them as Warnings with the Alert sound ("The Mandate has fallen below 10. If it stays there for 11 months, the mission is lost."). The debt's warnings stay Finance's.
  - **On failure:** `campaign::request_failure_screen` queues `CampaignPrompt::MissionFailed`; the in-game menus open the screen over any dialog open (closing them), not over a story sequence; the dialog pauses the simulation. `campaign::tick` doesn't finish a lost mission. The autosave stops while the game is lost. A save of a lost mission loads paused, its screen again.
  - **The failure screen** (`MissionFailed`, [mission_failed.rs](../../crates/game/src/menu/dialog/mission_failed.rs)): the debrief's panel, centred on its backdrop. It shows:
    - "MISSION N FAILED", the name, city and year;
    - the reason in the realm's words (`failure.<realm>.<reason>`, Realm: "The court has recalled you", "The king withdraws his favour", "The sangha turns away");
    - why, plainly ("The Mandate stayed below 10 for 12 months.");
    - the goals' table and the stats, shared with the debrief (`mission_complete::goal_rows`, `stat_rows`, `goals_table`, `stats_values`);
    - Return to the menu, Load a save (Load / Save over it: Cancel comes back) and Try again (`home::start_campaign_mission`: the briefing again, no story).

    Escape opens `Return to Menu Popup` (Cancel, Return to the menu) instead of closing it.
  - **The Screens menu:** `Screens/Failure` (debt, legitimacy, time) and `Screens/Mission failed`, a preview (`PreviewScreen::MissionFailed`) whose buttons and Escape only close it.
  - **Automation:** `campaign.fail reason=...` loses the mission being played now; `finance.set months_in_debt=N` sets the debt clock.
  - **Tests:**
    - `tests/ratings.rs`: debt (with a finished mission, the switch off and no mission), the Legitimacy clock with its reset and warnings, the time limit with its warnings, and each realm's wording.
    - `tests/campaign.rs`: a lost mission can't be won, and the failure block.
  - **Scenarios:**
    - `ratings.jsonl`: the test mission's twelfth month in debt, the screen, its popup, a save that loads lost, Try again, a loss on Legitimacy in Đại Long's and Tampadesa's words, and Return to the menu.
    - `ui_dev_screens.jsonl`: the preview in Đại Long's and Kambara's looks.
- **Deviations:**
  - **No `ActiveMission::failed` and no `campaign::fail`.** The failure stays on `CityRatings` (Phase 0's choice), and the campaign only queues the prompt (`request_failure_screen`). No save change.
  - **The screen opens over any dialog,** where the debrief waits for none to be open: a lost game shouldn't play on under the City book.
  - **The Legitimacy clock counts month starts,** after the recompute, as the debt's counts month-ends; the same months.
  - **No message when the mission is lost:** the screen says it.
  - **`campaign::active_mission` and `active_requirements` return None before the campaigns are set up,** as in the test harness, which steps months without them.
- **UI changes:** the failure screen (new, no mockup); its popup; the Screens menu's Failure and Mission failed (developer UI); the failure warnings in the message log.
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - every scenario on a release build (macOS), `determinism.jsonl` twice;
  - the screenshots: the screen in Đại Long's, Kambara's and Tampadesa's looks, and its popup.

### Phase 4: Distinctions (medium)

- [x] `DistinctionCondition` and its checks; `CityRatings::spoiled` tracked monthly; `campaign::distinctions_earned`.
- [x] The award into the profile with the finished mission; the stele's lines.
- [x] The debrief's distinctions and rating meters; the campaign-complete count and marks; the picker's count.
- [x] The hidden test campaigns' distinctions; `ui_intermission.jsonl` checks them.
- [x] Tests for each condition and the award.

**Notes (2026-11-25):**
- **What was built:**
  - **The data:** `MissionDistinctionDef` gains `condition` (`DistinctionCondition`, required): `Goal(MissionGoal)`, `NoDebt`, `NeverBreached`, `NoHouseLost`, `NeverShortOf { good }` and `Within { months }`. `CampaignConfigs::validate` refuses more than `MAX_DISTINCTIONS` (2) on a mission, a distinction without an id or stele line, and a condition with a problem (`MissionGoal::problem`, shared with the goals' checks; `Within` of 0 months).
  - **`ratings::distinctions`** ([ratings/distinctions.rs](../../crates/game/src/ratings/distinctions.rs)):
    - `is_broken`: a month's end in debt (`Finance::months_in_debt` above 0), a flood log record with a breach or a disaster month with a house lost since the mission's start (`mission_start_month`, the calendar's start), the good out of storage while people live in the city, or the months gone (`months_elapsed >= months`, as the time limit counts). A goal never is.
    - `check_spoiled`: at each month's start, after the failure's check (`ratings::on_new_month`), spoils the mission's broken distinctions into `CityRatings::spoiled` (saved since v17), not a goal, and nothing outside a mission, once it's finished or once it's lost.
    - `is_earned` and `earned`: as the goals are met, a distinction that isn't spoiled nor broken now, and whose goal is met.
    - `state`: open or spoiled while the mission goes on, earned or missed once it's finished.
  - **The award:** `campaign::tick` reads `distinctions::earned` as the goals are met and queues it with the finished mission (`FinishedMission`, `take_finished_missions`), and keeps it for the debrief (`campaign::distinctions_awarded`; runtime only, dropped by any session change). `GameLoop` records it with the mission (`profile::on_mission_finished`, `Profile::mission_finished_with`), added to an earlier play's; ids the mission doesn't have are left out. `campaign::earned_distinctions` is this play's with the profile's.
  - **The debrief:**
    - a Distinctions section after the goals: its title with "each one adds a line to the stele", and a table with a tick for each one earned (in this play or an earlier one) and a crossed ring, its line dimmed, for each one not (`UiTableRow::with_color`, new). A mission without distinctions shows none.
    - the four ratings as bars over the stats, in the UI realm's words (`MissionComplete/panel/ratings`, a canvas).
    - The stele is carved from the profile with this play's award, so a distinction's line shows freshly carved with the mission's.
  - **The campaign-complete screen:** "Distinctions N of M" in the stats and a star per distinction earned beside each mission (`marks`, a canvas beside each missions table), with "each mark is a distinction" by the Missions title, while the campaign has any (`stele::distinction_tally`: each step counted at the mission that finished it).
  - **The campaign picker:** "N of M distinctions" under a playable card's status while its campaign has any.
  - **Automation and the developer UI:** `goals.state`'s `distinctions` (`id`, `def`, `distinction`, `state`); the campaign debug window lists them.
  - **Data:** each hidden test campaign's first mission has two, one earned and one missed when played as the scenarios do: never in debt and Culture 60 (`test_choices`), within a year and 1,000 people (Kambara's), never a breach and Prosperity 90 (Tampadesa's), never in debt and Culture 80 (`test_goals`). Campaign I's missions have none until M13.
  - **Tests:**
    - `tests/ratings.rs`: each condition (from the mission's start, not before it), and the monthly spoiling with its states and save.
    - `tests/campaign.rs`: the award (queued, kept for the debrief, into the profile with a replay's, a spoiled one left out, dropped by a new start), the checks and the JSON forms.
  - **Scenarios:**
    - `ui_intermission.jsonl`: `test_choices`' debrief (the table, its note, `goals.state`, the stele's new line, the profile, the rating bars), its last debrief's stele, the campaign-complete screen's count, note and marks, and Kambara's and Tampadesa's debriefs.
    - `ratings.jsonl`: the test mission's distinctions open, then earned and missed on its debrief with the bars, and never in debt spoiled by a month's end in debt.
- **Deviations:**
  - **No `campaign::distinctions_earned`:** `distinctions_awarded` (this play's) and `earned_distinctions` (with the profile's).
  - **The debrief shows a distinction earned in an earlier play as earned,** as the stele carries its line.
  - **"Never short of" and the others are checked once more as the goals are met,** besides each month's start.
  - **The rating bars come before the stats, under the room left over,** and the distinctions after the goals; the room between them grows or shrinks.
  - **No save change:** `CityRatings::spoiled` (v17) and `CampaignRecord::distinctions` (profile version 1) were already there.
- **UI changes:** the debrief's Distinctions section and rating bars; the campaign-complete screen's distinctions stat, marks and note; the picker card's count; `UiTableRow::with_color` (engine).
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - every scenario on a release build (macOS), `determinism.jsonl` twice;
  - the screenshots: the debrief in Đại Long's, Kambara's and Tampadesa's looks, and the campaign-complete screen's marks.

### Phase 5: Custom games (small)

- [x] `maps.json`'s `goal_sets` and the Goals dropdown; the custom game's goals checked as a mission's.
- [x] The short custom-game debrief.
- [x] The Failure switch and Try again for a custom game.
- [x] `ui_menus.jsonl`'s Custom Game checks extended.

**Notes (2026-11-26):**
- **What was built:**
  - **The data:** `maps.json`'s `goal_sets` (`custom_game::GoalSetDef`: an `id`, a `name`, the Menu key `goal_set.<id>` winning, and `requirements`, a `MissionRequirements`): "A thriving town" (population 1,000, Prosperity 40) and "A city of culture" (Culture 50, two festivals held). `MapConfigs::post_load` leaves out a set with an empty or repeated id, no goals (the Sandbox is the game without them) or a problem its requirements report (`MissionRequirements::problems`, as a mission's).
  - **The custom game's state:** `CustomGameSettings` and `CustomGameRecord` gain `goal_set` and `failure`; the record also keeps `completed` (its goals met) and `start` (`CustomGameStart`: a blank map's size and terrain, the starting year and month, the gold and the fires switch). The record is the save header's, so a save keeps them all with no save-version bump.
  - **Goals:** `campaign::game_requirements()` is the game's goals, a mission's or the custom game's goal set's (`custom_game::active_requirements`): the HUD's goals button and panel, the rings' goal ticks (`ratings::mission_goal`), the City book's Overview (the goal set's name over its rows) and `goals.state` read it. `custom_game::tick`, each step after `campaign::tick` (`CampaignSystem`), completes the game once its goals are met, unless it's lost, and asks for its debrief (`CampaignPrompt::CustomGameComplete`, `campaign::request_custom_game_complete`).
  - **The short debrief** (`CustomGameComplete`, [dialog/custom_game_complete.rs](../../crates/game/src/menu/dialog/custom_game_complete.rs); no mockup): the failure screen's centred panel on the intermission backdrop: "GOALS MET", the goal set's name, the map and the year, the goals' table, the ratings' bars and the stats; Return to the menu and Keep playing (Escape keeps playing). The record's `completed` keeps it from opening again. The failure screen's panel fitting moved to `screen::fitted_panel`, which both use.
  - **Failure:** a custom game with its Failure switch on starts with `FailureSwitches::ON` (off: `OFF`), and `failure::mission_terms` gives it terms (its goal set's time limit, if one had one; completed once its goals are met), so debt and Legitimacy lose it as they lose a mission, and a save of it loads lost. The failure screen reads "CUSTOM GAME LOST", the goal set's name (the map's for a Sandbox), the map and the year, and its goals; Escape's popup says Custom Game can start another; Try again starts the game afresh from its record (`ActiveCustomGame::settings`; disabled when the record has no start, as a header from before this phase, or its map is gone).
  - **The Custom Game dialog:** the Goals dropdown offers the Sandbox, then the goal sets by name; the details list the goals picked ("Population 1,000 · Prosperity 40", or None); a fifth switch, "The city can be lost", off by default, its tooltip the limits in the realm's words ("The Mandate below 10 for 12 months, or 12 months in debt, ends the game, as in a campaign.").
  - **Automation:** `goals.state` reports a custom game's goals with `custom_game` and `goal_set`, and `completed` from its record; `session.state`'s `custom_game` adds `goal_set`, `failure`, `completed` and `can_restart`; `campaign.fail` loses a custom game with its Failure switch on.
  - **Tests:** `tests/ratings.rs`: the shipped goal sets load; a custom game's goals (the game's requirements, the ring's goal, completed once with its prompt, not lost then), its loss with the switch on and none with it off, a Sandbox's lack of goals, and Try again's settings from the record and from a save header's. `custom_game::config`'s unit test: the goal sets' checks.
  - **Scenario:** `ui_menus.jsonl`: the switch and the details' Goals for a new player; in Tampadesa's custom game, a goal set and the switch picked, the details, `session.state`, `goals.state`, the rings' goal, the HUD's goals badge, the save header's goal set and the load; then the debrief (opened by hand: a population of 1,000 is out of a scenario's reach; `tests/ratings.rs` meets the goals), Keep playing, a loss to debt (`campaign.fail`): the failure screen's words, its popup, and Try again starting the same game afresh.
- **Deviations:**
  - **The goal sets' goals are listed in the dialog's details,** not in the dropdown: a sixth row under Hazards, so the preview is a little shorter.
  - **The switch's label is "The city can be lost",** and its tooltip carries the plan's sentence with the limits, since the full sentence doesn't fit a switch's row.
  - **A goal set's time limit fails the game only with the switch on,** as debt and Legitimacy do; no shipped set has one.
  - **The debrief has no screen in the Screens menu,** which previews campaign screens only.
- **UI changes:** Custom Game's Goals dropdown, its details' Goals row and the "The city can be lost" switch; the new `CustomGameComplete` screen; the failure screen's custom game heading, popup text and Try again; the City book Overview's goals card for a goal set.
- **Verified:**
  - `cargo test --workspace`;
  - both clippy runs;
  - every scenario on a release build (macOS), `determinism.jsonl` twice;
  - in the running game: a goal set met (a throwaway set, Culture 5, with `ratings.set`) opened the debrief paused, Escape kept playing, it didn't open again a month later nor after a save and load;
  - the screenshots: Custom Game in Đại Long's and Tampadesa's looks (and at 75%), the debrief and the failure screen in both.

### Phase 6: Documentation (small)

- [ ] AGENTS.md: a Ratings section (data, state, drivers, failure, distinctions, commands); the scenario list; the plan list.
- [ ] known-issues.md: what the ratings leave out (defence, crime and unrest, monuments, the realms' own systems), unbalanced numbers, the failure screen without a mockup.
- [ ] The roadmap's M9 row and save log; GDD 7 if the goal names changed.

## Risks

- **Balance.** Ratings that sit at 90 from the first month, or that can't reach a mission's goal, would make I-3 to I-5 pointless or impossible.
  - The caps and marks are first guesses. Phase 1's scenario prints each driver on a small city.
  - M13 balances them against campaigns.md's goals: Culture 25 by I-3, Prosperity 30 by I-4, the Mandate 30 by I-5.
  - The realm files keep every number out of the code.
- **Security without defence.** Phase 1 has no attacks, crime or unrest, so Security is coverage and calm streets, and it can look arbitrary.
  - No Campaign I mission up to I-5 asks for it.
  - Phase 2 adds the defence and unrest drivers, which change its meaning less than its numbers.
- **Coverage's cost.** `ServiceCoverage` runs a proximity test per house per service each month. With a few hundred houses and three services that's about a thousand searches once a month. If it shows in a frame, it can share the houses' own needs check or spread over frames.
- **Failure's harshness.** Losing a city after an hour hurts. The warnings come early: the debt's three months, the Legitimacy clock's first month and its last three, the time limit's 12, 6, 3 and 1 months. Try again restarts at once.
- **Saved failure.** A failed mission saved by the developer editor, or a crash between the failure and the screen, must load back to the screen, not a playable city. `ActiveMission::failed` is saved for that.
- **The stele's lines.** A distinction's line carved on a replay changes the stele of a finished campaign, which the campaign-complete screen shows. That's intended (GDD: "for replay"), but `intermission.state`'s checks must allow it.

## Open decisions

Each has a default, and the user confirmed them all on 2026-11-25.

- **Q1. What a rating is.** *Default (confirmed): the sum of its drivers, rounded and clamped to 0–100, recomputed at each month's start.*
  - As the mockups' breakdowns, which add up exactly, and GDD 5.17's "updated monthly".
  - The alternative is Pharaoh's, where a rating moves a few points a month towards a target. It's smoother, but the breakdown no longer adds up to the value shown.
- **Q2. How events count.** *Default (confirmed): in full in their month, fading straight to nothing over 12 months (requests 24).* Festivals, requests, floods and disasters are remembered for a while, and the logs they come from are already saved.
- **Q3. Ratings in each realm.** *Default (confirmed): one data file per realm, the drivers code, their caps and names data.*
  - Kambara's and Tampadesa's fourth rating use favour and temple coverage until the baray, monasteries and donations exist.
  - "Divine favour" keeps en.json's sentence case. The mockups capitalize it.
- **Q4. Requests' Legitimacy.** *Default (confirmed): the request's `legitimacy`, else 6, when met; 1.5 times that lost when failed; a failed court request asked again three months later at 1.25 times the amount.* GDD 5.14 and Kambara's popup (+8, −10).
- **Q5. House levels in goals.** *Default (confirmed): the number the player sees, from 1, as the City book's housing rows; at that level or above.* I-1's "10 houses at level 1" then counts every house. M13 picks the levels it means.
- **Q6. `Resource` and `Stockpile`.** *Default (confirmed): no `Stockpile`; `Resource` counts the storage yards and granaries only.* GDD 7 says `Resource` is the code's `Stockpile`, and its example is "in storage". No shipped mission uses `Resource`, so nothing changes for players.
- **Q7. After a failure.** *Default (confirmed): Try again, Load a save, Return to the menu; no Keep playing.* As in Caesar III and Pharaoh. Keep playing as a sandbox would make the failure meaningless.
- **Q8. The failure screen's look.** *Default (confirmed): the debrief's layout on the realm's backdrop, without the stele.* Nothing is carved, and there's no mockup.
- **Q9. Ratings in the first missions.** *Default (confirmed): shown in every game from the start.*
  - The HUD and City book always show them, and I-3 and I-5's briefings introduce them (M13).
  - Legitimacy's failure is on by default; M13 turns it off for I-1 to I-4 through their `failure` blocks, before the Mandate is taught.
- **Q10. Difficulty.** *Default (confirmed): not applied in M9.* Custom Game keeps storing it. M13 decides with the balance what it scales: costs, request sizes and favour's drift (GDD 293).
- **Q11. Custom games.** *Default (confirmed): goal sets in `maps.json` and a Failure switch, off by default.* The dialog's `TODO` at `custom_game.rs:892` waits for M9's goal sets. overview.md's relaxed builder plays without failure.
- **Q12. Monuments, defence, crime, unrest and tribute.** *Default (confirmed): none in M9.* Their drivers come with game Phase 2. The mockups' "No defences −6", "Late tribute −4" and "Unfinished prasat −4" wait for those systems.
