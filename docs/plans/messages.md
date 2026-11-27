# Messages and advisors

**Status (2026-11-27): accepted with every decision's default, but Q10: one save bump, v19, for labour, Pause work and the stores' runners together (Phase 2). Phases 0 to 3 done; Phase 4, the docs, next.** This plan is milestone M11 of the [roadmap](roadmap.md). It covers:
- GDD 5.18's advisor messages and events: typed messages with a priority, a place on the map or a page, and a popup for the important ones (the mockups' `event-<realm>.jpg`);
- GDD 5.3's labour priorities per sector, as in Pharaoh, and the advisor pages the roadmap asks for: the City book's Labour and Food pages (`city-labour-<realm>.jpg`, `city-food-<realm>.jpg`). Finances (M4) and Ratings (M9) are pages already;
- the two inspector actions realm-ui Phase 8 left disabled: Pause work, and a store's Get and Empty orders.

What this milestone leaves out:
- Advisors as characters with portraits and lines of their own (GDD 10's "one screen per area ... as in Pharaoh"): the City book's pages stand for them. The military, trade, culture and court advisors come with their systems (game Phase 2) or later.
- Attack warnings (GDD 6) are game Phase 2's; they'll be important events of their own.
- Scripted mission events (GDD 5.18) are M13's, if its missions need them; this milestone gives them the popup to use.
- Balance: M13's.

It was researched against `f053fb6a` on `claude`. File and line references point at that commit, so re-check them before relying on them.

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase: the commit, deviations and what was verified. This file is the source of truth for progress.
- The [open decisions](#open-decisions) are settled: the user took every default but Q10's (2026-11-26).
- Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows. The retail screens follow the realm UI's skins and mockups ([realm-ui.md](realm-ui.md)).
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo +nightly fmt --all`, both clippy runs, `cargo test --workspace` and the scenario scripts before committing.
  - Follow the Saves workflow for Phase 2's format change.

## Goals

1. **Messages that know what they're about.** Each notice the game raises carries its kind, a priority and, where it has one, a place: a cell or building on the map, a City book page, the world map. The HUD's log lines take the player there.
2. **Popups for what matters.** An important event (a flood, a dyke breach, a court request, a patron's blessing or misfortune, an outbreak, the last warnings before a mission is lost) opens a popup after `event-<realm>.jpg`: a picture, what happened, its consequences as lines, and a way to it (Show on map, Open the goods page). The player can turn popups off; the log keeps every event either way.
3. **Labour the player directs.** Workers go to the sectors in the player's order of priority, as in Pharaoh. The Labour page shows the workforce, each sector's jobs and workers and what it's short of, and changes the order. The inspector's workers note says where a building's sector stands.
4. **A Food page.** What the granaries hold, what was grown and eaten last month, how long each food lasts without harvests, and the granaries over the last year.
5. **The inspector's last actions.** Pause work stops a workplace and frees its workers. A store's Get fetches a good from other stores, and Empty carries out what it holds.
6. **Automation.** Every part can be read and driven by commands, for `assert.*` and `wait.until`.

## What exists today

### Design

- **GDD 5.3:** "Each house supplies workers by level; buildings need staff to run. Planned: labour priorities per sector, as in Pharaoh."
- **GDD 5.18 and 2:** "Advisors, court requests, events and attack warnings arrive as messages."
- **GDD 10:** "Advisors: one screen per area (labour, military, food, trade, culture, finance, ratings, the court), as in Pharaoh."
- **Mockups** ([README](../design/mockups/README.md)):
  - `event-<realm>.jpg`: a popup in the dialogs' frame over the city. A picture (a city scene with a line icon in a disc), a kicker ("FLOOD", "REQUEST FROM THE COURT"), the date at the right ("Month 7 of year 17 · the wet season"), a title ("The Cinnabar River floods"), a paragraph, three lines with icons (what happened, what it brings, what lateness costs), and two buttons: Show on map or Open the goods page, then Ok.
  - `city-labour-<realm>.jpg`: cards (Workers, Employed, Looking for work, Jobs not filled), a Sectors table (a priority with − and + buttons, the sector, Needed, Employed, a bar, "Full" or "N short"), the note "Priority 1 is filled first. Sectors short of workers run slower; unfilled jobs show on the map." and "Workers go to the highest priority first" under the title. Six sectors: Food and farming, Industry, Services, Culture and religion, Infrastructure, Trade.
  - `city-food-<realm>.jpg`: cards (In granaries, Grown a month, Eaten a month, Lasts without harvests), a By food table (Stored, Grown, Eaten, Change with an arrow, a bar and "N months", short ones in red), and a chart of the food in granaries over the last twelve months.

### Code

- **Messages** ([messages.rs](../../crates/game/src/messages.rs)): a runtime store of 32 messages, each a `MessageKind` (construction, warning, finance, faith, season, harvest, trade, request) and its text. `GameLoop::post_message` ([game_loop.rs](../../crates/game/src/game_loop.rs)) is the one way in; each system's notices reach it through its own `notify_*` (construction, finance, risks, faith, floods, farms, trade, failure warnings). The notices are text, or structs turned into text there; none carries a place. `notify_finances` tells the good news apart by its text (a `TODO(realm-ui)`).
- **The HUD's log** ([message_log.rs](../../crates/game/src/menu/in_game/message_log.rs)): the three latest messages on a canvas, hidden 20 s after the newest; nothing is clickable but its close tile. `messages.state` and `messages.push` drive it.
- **Minimap alerts** ([tile/minimap/](../../crates/game/src/tile/minimap/mod.rs)) have places: fires, collapses, the flood's damage and blights ping the map, and the alerts tool steps through them.
- **Labour** ([building/mod.rs](../../crates/game/src/building/mod.rs) `update_workers`, [sim/resources.rs](../../crates/game/src/sim/resources.rs) `Employer`, `Workers`): every `workers_update_frequency_secs` (20) an employer that isn't full asks for workers, as a deferred update: it takes every free worker of the nearest house with any, found by a road search within `workers_search_radius` (20). An employer keeps its households (`employee_households`), a house its employed and unemployed. There's no city-wide pool and no order between employers: whoever asks first gets the workers. A building below its minimum doesn't run; above it, it runs at `work_efficiency`.
- **Sectors:** each tile def has a `sector` (`TileSector`: housing, roads, food and farming, industry and resources, services, infrastructure, culture and religion, trade and economy, beauty), which the build palette groups by.
- **The City book** ([city_book/](../../crates/game/src/menu/dialog/city_book/mod.rs)): pages in `PAGES`, built from `CityReport` ([city/report.rs](../../crates/game/src/city/report.rs): population, employed, unemployed, workforce, jobs not filled, food stored and eaten a month) and `CityHistory` (a sample a month: population, employed, balance, the goods stored; runtime only). The Population page draws a trend.
- **The inspector** ([inspect/building.rs](../../crates/game/src/inspect/building.rs), [inspector/mod.rs](../../crates/game/src/menu/in_game/inspector/mod.rs)): Pause work is disabled ("action.pause_work_soon"); a store's orders are saved per good (`StorageSlots::orders`, save v9), and Get and Empty only change what it takes in (`StorageOrder::takes_in`). A store has no runner: `StorageBuilding::update` does nothing.
- **Unit tasks** that the stores' runners can use: `UnitTaskFetchFromStorage` and `UnitTaskDeliverToStorage` (the dock's dockers use them, [trade/ships.rs](../../crates/game/src/trade/ships.rs)).
- **Save version** 18.

## Design

### 1. Typed messages (Phase 0)

- **`Message`** gains:
  - `priority: MessagePriority`: `Info` (a building done, a caravan), `Notice` (a harvest, a festival held, the river rising), `Warning` (a fire, debt, a breach) and `Event` (a popup's: §2);
  - `target: Option<MessageTarget>`: `Cell(Cell)`, `Building(BuildingKindAndId, Cell)` (its cell kept, so it still shows where it stood once it's gone), `CityPage(&'static str)`, `WorldMap(Option<String>)` (a city to pick);
  - `popup: Option<Box<EventPopup>>` for an `Event`;
  - its date (the ordinal month), for the popup's date line and the log's age.
- **Notices carry their place.** Each system's notice gains what the message needs: a fire's or collapse's building (risks), a request's id (trade), the flood's cells and counts (floods), a patron's event (faith). `GameLoop`'s `notify_*` build `Message`s from them through one `messages::post(Message)`; finance's notices carry their severity (the `TODO(realm-ui)` closes).
- **The HUD's log** keeps its three lines; a click on one goes to its target (the camera to its cell, `camera.teleport` with heights; a page opens the City book on it) or reopens its popup. Each line is recorded as `MessageLog/line_<N>` from the newest (kind `message`, value its message's id, kind, priority, text, target and month) so `ui.click` reaches it. A line with a target is lit on hover (Phase 0's notes).
- **Runtime only,** as today: a load starts an empty log. The popups' queue (§2) is runtime too.

### 2. Event popups (Phase 1)

- **`EventPopup`**: a kicker (a Messages key: "Flood", "Request from the court"), a title, a paragraph, up to three lines (an icon from the HUD's line icons and a text), the target (`Show on map` for a cell or building, `Open the goods page` for a request, none otherwise) and its icon (the picture's disc).
- **Which events** (Q3's default):
  - the river's flood (the fields under water and for how long, the houses damaged, the silt), and a dyke breach;
  - a request opened (what's asked and by when, what meeting it brings, what lateness costs: the mockup's), and one failed;
  - a patron's blessing or misfortune (what it does and for how long);
  - an outbreak starting;
  - the failure warnings' last ones: Legitimacy's clock starting and its last month, the time limit's last 3 months and last month, the debt's last 3 months.
- **The dialog** (`EventPopup`, a `DialogMenuKind`, [dialog/event_popup.rs], new): on the dialogs' frame at the skin's `dialogs.EventPopup` size, laid out after the mockup. The picture is the realm's `event_picture` (§2a), the event's icon in a disc over it, drawn live. Escape and Ok close it. Show on map closes it and moves the camera; Open the goods page opens the City book's Goods page (its Requests card).
- **When it opens:** a popup waits in a queue while another dialog, a briefing, a story or a prompt is open, and opens when none is (as the campaign's prompts do); dialogs pause the simulation, so a popup pauses it. Two events in one month open one after another. None opens under the developer editor or in a preview; a session change empties the queue.
- **Turning them off:** Settings' Game tab gains "Event popups" (`DisplayConfigs::event_popups`, on by default). Off, events go to the log only, and a click on their line opens the popup there.
- **Text:** Messages keys with arguments (`event.flood.title`: "The {river} floods"), in each realm's words where a realm has its own (`event.<realm>.<key>` Realm keys, as the faith and failure strings do: the court's request, a patron's name, the fourth rating's name).

#### 2a. The popup's picture

- ui-art draws `event_picture` per realm: the mockup's city scene at the popup's picture size, at 2×, a JPEG, from the generator's own call (`palette.py`'s event popup). The disc and icon are drawn live, so one picture serves every event (Q4).

### 3. Labour priorities (Phase 2)

- **Sectors:** the six of the mockup, from `TileSector`: food and farming (the granary and market among them), industry and resources (the storage yard among them), services, culture and religion, infrastructure, trade and economy. A building with workers in another sector counts under infrastructure (Q6; the garden, beautification's only building, has none).
- **State:** `sim::labour::CityLabour` (saved in `Simulation`, `SimContext::labour()`): the sectors in priority order, 1 first. Strict order: − and + swap a sector with its neighbour (Q2). A new game starts in the mockup's order; a mission's `labour` block in `campaigns.json` may set another (M13).
- **Allocation** (Q1's default), `labour::allocate`, every `LabourRules::allocate_secs` (10) and at once when the order changes or a building is paused or resumed:
  1. The supply: each house's workers (employed and unemployed), as today.
  2. The demand: each employer that can work (finished, not paused, not burning, linked to a road), by sector.
  3. Sector by sector in priority order, each building first gets its minimum (so as many as possible run), then the rest up to its maximum, from houses within `workers_search_radius` along the roads (today's reach), nearest first. Within a sector, buildings are taken in a fixed order (pool order), so the same city gives the same allocation.
  4. The result replaces the employers' households and the houses' employed counts. Workers stay put when nothing changed, so the allocation is stable from one pass to the next.
- **What it changes:** a short sector runs slower (its buildings' `work_efficiency`, as now); a low-priority sector loses workers first when the workforce falls. The world stats' employment, the ratings' employment driver and the HUD read the same numbers as today.
- **Pause work** (Q7): a building's `paused` flag (saved), toggled by the inspector's Pause work (Resume while paused). A paused building gets no workers in the allocation, so it does whatever a building without workers does: it stops. Its panel says Paused.
- **Save v19:** `sim.labour`, each building's `paused` and each store's runner (§5), in one bump (Q10).
- **Automation:** `labour.state` (the order; per sector its buildings, needed, employed, short; the workforce, employed, looking for work, jobs not filled) and `labour.set sector=... priority=N`; `building.pause cell=... [paused=true|false]`.

### 4. The Labour and Food pages (Phase 3)

- **Labour** (`CityPage` `labour`, after Ratings, as `city-labour-<realm>.jpg`): the four cards from `CityReport`; the Sectors table from `labour::report` (each sector's priority, needed and employed, its bar and "Full" or "N short"), its − and + buttons a `UiStepSelector` of arrows per row or two icon buttons (`CityManagement/Labour/Food and farming/-` and `/+`), which set the order through a queued command (`labour::request`, run at the start of the next frame, as the festivals' planner does). The note under the table drops "unfilled jobs show on the map" unless Q8's lens lands.
- **Food** (`CityPage` `food`, after Labour, as `city-food-<realm>.jpg`):
  - the cards: food in granaries, grown last month, eaten last month, how long it lasts without harvests;
  - By food: a row per food in `ResourceKind::foods()` the city has stored, grown or eaten: stored, grown, eaten, the change since last month, a bar and the months it lasts (red under `FOOD_SHORT_MONTHS`, 3);
  - the chart: the food in granaries at each of the last twelve months' starts, from `CityHistory`, drawn with the painter's polyline and a fill.
- **Grown and eaten** are counted, not estimated: `city::history` keeps this month's production and consumption per good (runtime, sampled with the month): a producer's output as it's made (`ProducerBuilding`, the farmsteads' harvests) and a house's food as it eats it (`house consumption`). Imports aren't grown.
- **The inspector's workers note** gains the sector's place: "2 short · Industry is priority 2".
- **Automation:** the pages' canvases record their figures (`CityManagement/Labour/sectors`, value a row per sector; `CityManagement/Food/foods`, `chart`); `city.state` adds `labour` and `food`.

### 5. Storage orders: Get and Empty (Phase 2)

- **A store's runner:** `StorageBuilding` gains a `Runner` (saved, save v19 with the labour state), as the producers and the dock's dockers have. Every `storage_orders_secs` (10), while staffed, it looks for an errand:
  - **Get:** for a good set to Get with room for it, a runner fetches a load from the nearest other store that holds the good and doesn't Get it itself (`UnitTaskFetchFromStorage`), within `storage_reach` along the roads. Producers' runners keep delivering to the nearest store that takes the good; a Get store takes them like any other (Q9).
  - **Empty:** for a good set to Empty that it holds, a runner carries a load to the nearest other store that accepts it (`UnitTaskDeliverToStorage`); with none, nothing happens and the panel says so.
  - One errand at a time per store, a runner's load (`StorageConfig::runner_load`, new).
- **The inspector:** a store's panel shows its runner's errand ("Fetching 8 rice from the granary at ...", "Carrying out 8 clay"); the order selector's tooltip, which says Get and Empty wait for M11, goes.
- **Automation:** `storage.orders` reports each store's errand; `world.building_at`'s storage block too.

### 6. What the player sees

- **The HUD's log:** lines that take the player to their event; an Event line marked with its icon in the accent colour.
- **Event popups** over the city (§2).
- **Settings:** "Event popups" in the Game tab.
- **The City book:** Labour and Food pages, eight tabs in all.
- **The inspector:** Pause work and Resume; the workers note's priority; a store's errand.

### 7. Automation, tests and scenarios

- **Commands:** `messages.state` (each message's kind, priority, target, popup and date; the popups' queue), `messages.push` (with `priority` and `target`), `messages.open id=...` (its target or popup), `labour.state`, `labour.set`, `building.pause`, `storage.orders` with errands, `city.state`'s `labour` and `food`.
- **Tests:**
  - `tests/messages.rs` (new): each system's notice becomes its message with its place; the queue; popups off.
  - `tests/labour.rs` (new): the allocation by priority, minimums first, reach, stability, a change of order, pause and resume, a house gone.
  - `tests/goods.rs` or a new `tests/storage.rs`: Get and Empty's errands.
  - `tests/house_consumption.rs` and `tests/farms.rs`: the month's counters.
- **Scenarios:**
  - `messages.jsonl` (new): a request opened (its popup, Open the goods page), a flood's popup (Show on map moves the camera), a log line clicked, popups turned off in Settings and the event still in the log.
  - `labour.jsonl` (new): a city short of workers; Industry raised over Food on the Labour page, the workers moving; Pause work on a workshop and its workers going elsewhere; Resume.
  - `ui_city.jsonl`: the Labour and Food pages in each realm, at 200% and 75% UI scale.
  - `ui_inspector.jsonl`: Pause work; a store's Get and Empty with its runner's errand.
  - Existing scenarios whose employment numbers change with the allocation get new numbers (Risks).

## UI changes

| Where | Change | Phase | Files |
|---|---|---|---|
| HUD message log | Lines that go to their target or reopen their popup; a line per message recorded | 0 | `menu/in_game/message_log.rs` |
| New dialog | `EventPopup`, after `event-<realm>.jpg` | 1 | `menu/dialog/event_popup.rs` (new), skins' `dialogs`, ui-art's `event_picture` |
| Settings | "Event popups" in the Game tab | 1 | `menu/dialog/settings/game.rs` |
| City book | The Labour page | 3 | `menu/dialog/city_book/labour.rs` (new) |
| City book | The Food page | 3 | `menu/dialog/city_book/food.rs` (new) |
| Inspector | Pause work and Resume; the workers note's priority | 2, 3 | `inspect/building.rs`, `menu/in_game/inspector/` |
| Inspector | A store's errand; the order selector's tooltip gone | 2 | `inspect/building.rs` |

## Phases

### Phase 0: Typed messages (medium)

- [x] `MessagePriority`, `MessageTarget`, a message's date; `messages::post`.
- [x] Each system's notices carry their place; finance's carry their severity.
- [x] The HUD log's lines go to their targets; recorded per line.
- [x] `messages.state`, `messages.push`, `messages.open`; `tests/messages.rs`.

Notes (2026-11-26):
- **As built.** `messages::Message` (`id`, `kind`, `priority`, `text`, `target`, `month`), built with `Message::new(kind, text)`, `with_priority` and `with_target`, and posted by `messages::post(message, month)`; a kind gives the default priority (`MessageKind::default_priority`: construction and trade Info, warnings Warning, the rest Notice). `MessageTarget` is `Cell`, `Building(BuildingKindAndId, Cell)`, `CityPage(&'static str)` and `WorldMap(Option<String>)`, the city to pick on the map (a trade partner). Each system's notice carries what its message needs: `construction::CompletedNotice` (the building and its cell), `risks::RiskNotice` (a `RiskNoticeKind` and its target), `finance::DebtNotice` (a `DebtNoticeKind`: the out-of-debt `TODO(realm-ui)` is gone), the floods' notices an `at` cell (the first building damaged or a cell under water, the first breach, the first silted field; the river rising goes to its first source). `floods::notice_message`, `trade::notice_message` and `failure::notice_message` return a `Message` and its sound. Faith's go to the Heritage page, the harvests to the Goods page (the Food page from Phase 3), trade's to the partner on the world map or a request to the Goods page, Legitimacy's warnings to the Ratings page and the time limit's to the Overview.
- **Deviations.** The log's lines are recorded by position, `MessageLog/line_1` (the newest) to `line_3`, not by id, so scripts can click them; the id is in their value. The engine sets no OS cursor, so a line with a place is lit while hovered (`UiCanvas::with_hover_fill`, the HUD's sub colour, faint) instead of showing a pointer. The `MessageLog/messages` canvas is gone (`ui_hud.jsonl` reads the lines). A building target goes to the building's base cell, not its middle, as the inspector's Show its home does. The log still says "now" for the newest line; the month is kept for Phase 1's popups.
- **Engine.** `UiCanvas` takes clicks (`on_clicked`, playing ButtonPressed), a hover fill and a recorded kind (`with_kind`).
- **Verified.** `cargo test --workspace` (the new `tests/messages.rs`: the log, the default priorities, the JSON, and each system's notice with its place), both clippy runs, all 30 CI scenarios on a release build (`ui_hud.jsonl` now reads the lines, opens the Finances page from one and moves the camera from another), and `determinism.jsonl` twice with the same outputs and save. The hover fill was checked in screenshots in each realm.

### Phase 1: Event popups (medium)

- [x] `EventPopup` and the events that raise one (Q3), their text in each realm's words.
- [x] The `EventPopup` dialog, its queue and its buttons; the picture (see the notes: no ui-art `event_picture` yet).
- [x] Settings' "Event popups".
- [x] `messages.jsonl`.

Notes (2026-11-26):
- **As built.** `messages::EventPopup` (kicker, title, text, icon, up to 3 `EventLine`s) on a `Message` (`with_popup`, priority Event), built by `messages::popups` from each notice as `GameLoop` posts it: `flood`, `request` (opened, failed), `faith` (a blessing or misfortune, its lines from the event's effect: `effect_lines`), `outbreak`, `failure` (Legitimacy's clock starting and its last month, with its three lowest drivers; a time limit's last 3 months and last month, with the goals not met) and `debt` (3 months and 1 month before it's overdue, while the game can be lost to it). `messages::queue_popup`, `take_popup`, `queued_popups` and `clear_popups` keep the queue; `GameLoop::post_message` queues while `DisplayConfigs::event_popups` is on and not under the developer editor; `InGameMenus::end_frame` opens the oldest once no dialog, briefing or prompt is open and no preview runs. The dialog (`menu/dialog/event_popup.rs`, `DialogMenuKind::EventPopup`) follows the briefing's pattern: the skin's `dialogs.EventPopup` size (920 × 810, `offset` [0, −24] as the mockup places it), the mockup's coordinates at the panel's scale, set smaller until it fits. A click on an event's line in the log, or `messages.open`, opens its popup again (`message_log::open`; `go_to_target` for its button).
- **Deviations.**
  - **The picture.** No ui-art `event_picture`: the mockup's picture is the reference city painting, which the game doesn't ship. The skin's `event` block names a picture and a `focus` to cut it about, and every realm uses its home backdrop until real art lands (Q4 holds: one picture per realm).
  - **Breaches.** A breach is told in the flood's paragraph ("It broke through the dykes in 2 places.") rather than a popup of its own, since it only ever comes with a flood; its log line stays.
  - **Line icons.** The mockup's house, farm and star come from the City book's page icons, tinted with the accent, beside the HUD's line icons (`popups::LINE_ICONS`): no new art.
  - **Words.** Realm words where the game has them (the fourth rating's name, a patron's event, the river's name); the court is "the court" in every realm.
  - **Scenarios.** `risks.jsonl`, `floods.jsonl` and `trade.jsonl` turn event popups off (`config.set`), since an outbreak, a flood or a request now opens one over the city and pauses it; `messages.jsonl` checks the real flood's and request's popups.
- **Verified.** `cargo test --workspace` (`tests/messages.rs`: the queue, each builder's words and when it gives none; `ui_skins`: the picture exists), both clippy runs, all 31 CI scenarios on a release build (the new `messages.jsonl` also at 1280 × 720 and 1440 × 800), `determinism.jsonl` twice. Screenshots of the popup in each realm and at 200% UI scale.

### Phase 2: Labour priorities, Pause work and storage orders (large)

- [x] `CityLabour` and the sectors' order; save v19 with each building's `paused` and each store's runner, its migration script and the sample saves.
- [x] `labour::allocate` in place of each employer's own search.
- [x] Pause work and Resume in the inspector.
- [x] A store's runner and its Get and Empty errands; the inspector's errand line.
- [x] `labour.state`, `labour.set`, `building.pause`, `storage.orders`' errands; `tests/labour.rs` and the stores' tests; the scenarios whose numbers move; `ui_inspector.jsonl`'s Pause work, Get and Empty.

Notes (2026-11-27):
- **As built.** `sim::labour::CityLabour` (`order`, the six `LabourSector`s, and `allocate_timer`), saved in `Simulation` and read through `SimContext::labour()`; `LabourSector::of_tile_sector` maps a building's tile's `TileSector` (Q6: housing, roads and beautification count under Infrastructure). `labour::allocate` runs at the end of `Simulation::step_world`, after the `SimCmds`, when the clock is due (`LabourRules::allocate_secs`, 10) or a building finished in the step (`CityLabour::request_allocation`, from the completion's deferred update): the supply is every finished house's workers, the demand every employer that can work (`labour::can_work`), each sector in priority order giving each building its minimum when the houses in reach can give all of it, then the rest up to its maximum, nearest houses first (a road search from its road link within `workers_search_radius`, as the old search); the result replaces both sides' maps (`Employer::assign`, `HouseholdWorkerPool::assign`), unchanged maps left alone. `labour::set_paused` (Pause work: `Building::paused`, its workers let go at once) and `labour::set_priority` hand the workers out again at once. A store's runner (`StorageBuilding::runner`, `errand_timer`, `send_on_errand`): Empty first, carried to another store only (`Runner::try_carry_to_storage`), then Get; `find_storage_fetch_candidate` skips the origin store and the stores that Get the good when the origin is a store. Save v19 (`v18_to_v19.py`): `sim.labour`, each building's `paused` (its `workers_update_timer` dropped), each store's `runner` and `errand_timer`.
- **Deviations.**
  - **Each employer's own search is gone,** with the building's worker timer and `SimConfigs::workers_update_frequency_secs`; a house's people leaving, or its despawn, still take their workers from their employers at once, so the two sides agree between passes.
  - **Short of its minimum, none.** A building the houses in reach can't give its minimum gets no one, rather than a few who can't run it: the workers go to buildings that run, and the unfilled jobs show.
  - **The test harness freezes labour** (the `freeze_labour` cheat), as it turns construction instant and risks off: tests that staff buildings by hand keep their numbers, and `tests/labour.rs` turns it off. Every existing test passed unchanged.
  - **A mission's `labour` block** isn't read yet: every game starts in the mockup's order until M13 needs another.
  - **Paused jobs** are left out of the world stats' jobs and of the Labour report's needed, so the City book's "short of workers" warning doesn't count a paused building.
  - **The errand line** is the producers' (the walker's task in its own words: "A runner is fetching bricks from the storage yard."); a good to empty that no other store in reach takes adds "No other store in reach takes its bricks." The order selector had no tooltip to remove.
  - **Knobs.** `SimConfigs::storage_errand_secs` (10), `storage_reach` (40) and a store config's `runner_load` (8), first guesses for M13.
  - **Commands.** `labour.allocate` was added beside `labour.state`, `labour.set` (`sector` and `priority`, or a whole `order`) and `building.pause`.
- **Verified.** `cargo test --workspace` (the new `tests/labour.rs`: the sectors, the order, priority, minimums first, a building short of its minimum, reach, nearest first, stability over passes and world steps, Pause work and Resume, a house gone, the clock, the cheat, the report; and `tests/storage.rs`: Get, Get skipping stores that get the good too, Empty, Empty with nowhere to go, reach, the errand), both clippy runs, the local saves and the sample saves migrated (`v18_to_v19.py`, idempotent) and loaded by `--smoke-test-saves`, all 31 CI scenarios on a release build (`ui_city.jsonl`'s numbers moved and were checked by hand: its 3 workers go to food first, the granary and the market, leaving the storage yard and the tax office without; `ui_inspector.jsonl` gains Get, Empty and Pause work; `ui_dev_screens.jsonl` failed twice in full runs on its known macOS flake, a covered world map popup, and passed alone), `determinism.jsonl` twice with the same outputs and save, and `ui_build` and `determinism` recorded and replayed. Screenshots of a store's errand line and a paused workshop.

### Phase 3: The Labour and Food pages (medium)

- [x] The month's production and consumption counters.
- [x] The Labour page, its order set from it; the inspector's workers note.
- [x] The Food page and its chart.
- [x] `ui_city.jsonl` and `labour.jsonl`.

Notes (2026-11-27):
- **As built.** `city::history` counts what's made (`record_made`: a workshop's batch, a farmstead's harvest, a gatherer's load) and used (`record_used`: what a house eats or uses) in the month under way, and gives it to the month's sample when the next is taken (`MonthSample::flow`); a sample also keeps the granaries' food for the chart. `city::food::read` (`FoodReport`, in `CityReport::food`) reads last month's grown and eaten per food (`CityHistory::last_month_flow`, its `period`), the granaries' stock, how long each food lasts at that rate and the trend. The City book has eight pages: the Labour page (`city_book/labour.rs`, the `industry` icon) and the Food page (`city_book/food.rs`, the `food` icon) after Ratings, as the mockups place them. The Labour page's − and + are a step selector in a new engine style, `UiStepSelectorStyle::PlusMinus` (the boxed − and + of the mockup, recorded as `-` and `+`); a change queues `labour::request_priority`, run at the start of the next frame, and the book's rebuild key holds the order, so it's built again with the new allocation. The inspector's workers note adds the sector's priority when the building is short ("2 short · Industry is priority 2", "Needs at least 2 to work · …"). Harvest messages go to the Food page. `city.state` reports `food` and `labour`.
- **Deviations.**
  - **The cards and the table read crate::labour's report,** not `CityReport`'s workforce: the jobs not filled count only buildings that can work, so the cards and the sectors add up.
  - **Change** in By food is grown less eaten last month (the mockup's figures), not what the granaries gained, which trade, requests and markets also move.
  - **Last month's counts.** "Grown a month" and "Eaten a month" are the last month that's over; before one is (a new or loaded game), the cards say "this month" or "last month" for the part seen. Before anything was eaten, "Lasts without harvests" reads the houses' rates (the Overview's estimate).
  - **A sector without jobs** shows "—" where the mockup would say Full; the selected row's frame in the mockup (a hover) isn't drawn.
  - **The chart's months** are numbers, as the Population page's.
- **Verified.** `cargo test --workspace` (`tests/house_consumption.rs`: what houses eat counted, and a month's counts moving onto its sample with the Food report's period, rows, months and trend; `tests/farms.rs`: a harvest counted grown; `tests/labour.rs`: the page's queued priority), both clippy runs, all 32 CI scenarios on a release build (`ui_city.jsonl` gains the Labour and Food pages in each realm and at 200% and 75% UI scale, and the − and + moving Industry; the new `labour.jsonl`, in CI's list; `ui_skins.jsonl` clicks the gallery's new plus-minus control), `determinism.jsonl` twice with the same outputs and save, and `ui_build` and `determinism` recorded and replayed. Screenshots of both pages in each realm and at 200%, the reordered Labour page, and the kiln's panel with its sector's priority.

### Phase 4: Documentation (small)

- [ ] AGENTS.md: messages, popups, labour, the new pages and commands; the scenario list; the plan list.
- [ ] known-issues.md: what's left out (advisors' characters, attack warnings, unbalanced numbers).
- [ ] The roadmap's M11 row and save log; GDD 5.3 and 5.18's status.

## Risks

- **Labour changes every city's numbers.** Today's first-come pull and the allocation give different employment, so scenarios that assert workers, ratings or production may move (`ui_city`, `ratings`, `goods`, `trade`, `faith`). Each moved number gets checked by hand, not just updated. Keeping the allocation stable (workers stay put when nothing changed) keeps the numbers from flickering.
- **Allocation cost.** A road search per employer every 10 s, as today's searches are per employer every 20 s while short. A city of a few hundred employers is a few hundred searches every 10 s, spread over frames if it shows.
- **Popups that nag.** A flood every year, requests and blessings: Q3's list is short on purpose, and the Settings switch turns them off.
- **One large phase.** Phase 2 holds labour, Pause work and the stores' runners, so the save version is bumped once (Q10). It lands as one change; its tasks can be committed one by one on the branch as long as the save version moves only once.
- **Notices' places.** Some notices are raised where the place is no longer known (a building burnt down). The message keeps the cell from when it was raised.

## Open decisions

Each has a default. The user took every default but Q10's on 2026-11-26.

- **Q1. The labour model.** *Default: a city-wide allocation every 10 s, by sector priority, minimums first, from houses within today's road reach.*
  - Pharaoh's: workers come from a city-wide pool, reach decided by recruiters walking the roads. The default keeps today's reach (a house within 20 road cells), so where houses stand still matters, without recruiter walkers.
  - The alternative keeps today's pull and only orders it: higher-priority buildings ask first and may take workers from lower ones in reach. It changes less, but a sector's "Needed" and "Employed" don't add up to a plan the player sets.
- **Q2. Priorities.** *Default: the six sectors in a strict order, 1 to 6; − and + swap a sector with its neighbour.* Pharaoh lets several sectors share no priority; a strict order is what the mockup shows and is easier to read.
- **Q3. Which events open a popup.** *Default: floods and breaches; requests opened and failed; blessings and misfortunes; outbreaks; the failure warnings' last ones (§2).* Fires and collapses stay log lines with their minimap pings: a big city has them often. Goals met already open the debrief.
- **Q4. The popup's picture.** *Default: one city scene per realm from ui-art, the event's icon in a disc drawn over it.* A picture per kind of event waits for real art.
- **Q5. Popups off.** *Default: a Settings switch, on by default; off, events go to the log only.* The alternative is "important only" levels; one switch is enough while the list is short.
- **Q6. Buildings outside the six sectors.** *Default: counted under Infrastructure.* Beauty buildings have no workers today, so this is a rule for later ones.
- **Q7. What Pause work does.** *Default: the building gets no workers, so it stops as an unstaffed building does; a store still holds its goods.* Pharaoh's closed warehouse also refuses deliveries; here a store's orders do that already.
- **Q8. "Unfilled jobs show on the map".** *Default: not in M11; the Labour page's note leaves the clause out.* A labour lens on the minimap (buildings short of workers) could follow.
- **Q9. Get from producers.** *Default: Get fetches from other stores only.* Producers deliver to the nearest store that takes the good, Get or not.
- **Q10. Save bumps.** *Default: two, v19 (labour and pause) and v20 (stores' runners), each with its phase.* One bump is possible if Phases 2 and 4 land together. **Decided (2026-11-26): one bump, v19; the storage orders joined Phase 2.**
- **Q11. The message list.** *Default: no full list in M11: the log shows three lines, and the store keeps 32 for `messages.state`.* A list dialog (the mockups' mail button) can follow if playtesting misses one.
