# Trade and court requests

**Status (2026-11-21): Plan written and revised with the user; not started.** This plan is milestone M10 of the [roadmap](roadmap.md). It covers GDD 5.12 and the requests half of 5.14:
- land, river and sea routes the player opens to partner cities, and the buy and sell limits they set per good and per partner;
- caravans that walk in from the map's edge and trade at the storage yards and granaries themselves, store by store, as in Pharaoh: there's no trade post;
- docks, and trade ships that sail in from the map's edge, moor at a dock and wait while its dockers carry their goods to the stores and the city's exports back to them;
- imports, construction materials included, delivered to the stores, never bought instantly (GDD 5.1.1);
- prices that move with supply and demand;
- court requests: "deliver N goods within M months", met or failed.

Mission I-5, The Mountain Market, teaches "the trade post and the world map; opening a route and setting buy and sell limits; importing construction materials: a caravan brings timber or bricks to the storage yards; court requests". Its goals are "trade income 1,000 a year; fulfil 3 court requests; Mandate 30" ([campaigns.md](../design/campaigns.md#5-the-mountain-market)). The Mandate is M9's. With no trade post, I-5 teaches the world map and caravans at the storage yards; M13 rewords its line. Gifts, relations that move and tribute are Phase 2's (roadmap, Phase 2 outline); docks and ships, which that outline also held, come into M10 at the user's request (2026-11-21).

It was researched against `28a11acc` on `claude`. File and line references point at that commit, so re-check them before relying on them.

**Revisions:**
- 2026-11-21, with the user:
  - **Q1:** no trade post. Caravans go from store to store, as in Pharaoh, and buy from granaries as well as storage yards (Pharaoh's buy only from yards), for later missions that trade food.
  - **Docks and ships come into M10** (Phase 2): river and sea partners trade only once the city has a dock.
  - **The other defaults stand.** Q2's default (M11's Get and Empty orders brought forward) only served the trade post, so it goes back to M11 (Q2 below).

## How to resume this work

- Work through the [phases](#phases) in order. Each phase is one PR-sized change that is usable on its own.
- Tick the checkboxes as tasks land, and add a short dated note under the phase (commit, deviations, what was verified). This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default, and the user confirmed them on 2026-11-21 (Q1 decided otherwise, Q2 moved back with it). Record any later answer here.
- Keep this milestone's UI changes to the ones listed under [UI changes](#ui-changes), and say so in the phase notes if one grows. The retail screens follow the realm UI's skins and mockups ([realm-ui.md](realm-ui.md)). The user's UI redesign lives on `claude-ui`, so flag every UI change in the phase notes.
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Run `cargo +nightly fmt --all`, both clippy runs, `cargo test --workspace` and the scenario scripts before committing.
  - Follow the Saves workflow for the format change in Phase 0.
  - Generate the new art with the Blender pipeline, ui-art and the recolour tools (art on demand).

## Goals

1. **Trade partners and their terms are data.** A partner city's goods, prices and yearly amounts live in its campaign's world file, beside what the world map already shows. A mission says which partners trade in it, by land, river or sea, which routes are open at its start and what opening the others costs. Nothing realm-specific is hard-coded.
2. **The player opens a route, sets limits and trade happens on its own.**
   - Opening a route costs gold once. A river or sea route can be opened only once the city has a dock.
   - For each good a partner sells, the player sets "import until the city holds N"; for each good it buys, "export what's above N".
   - A caravan or ship comes every few months, trades within those limits and the partner's yearly amounts, and the gold moves as each load changes hands.
3. **Goods travel.**
   - A caravan walks in from the map's edge to the storage yards and granaries, one after another. At each it unloads what the city imports and the store takes, and loads what the city exports and the store holds, until it's sold its goods and filled up, or nothing in reach is left to trade. Then it walks out.
   - A ship sails in to a dock and moors. The dock's dockers carry its goods to the stores a load at a time, and fetch the exports from the stores to it; it sails out when they're done.
   - An import the city waits for is visible: the next caravan's or ship's month and what it will bring.
4. **Prices move.** Each partner has base prices; selling a good to it lowers what it pays, buying raises what it asks, and both drift back month by month. Faith's "easy trade" blessing improves them for a while.
5. **Court requests.**
   - The mission's schedule sends requests: a good, an amount and a time limit, from the court or a partner.
   - The player sends the goods from the City book's Goods page when the city holds them.
   - Each request met or failed is logged for M9's Legitimacy, and counts toward a new `RequestsFulfilled` goal.
   - A `TradeIncome` goal counts the exports' gold.
6. **The player can see it.**
   - The world map's side panel: live prices, the limits, Open the route.
   - A trade limits dialog.
   - The dock's inspector panel, and the caravan's and ship's panels.
   - The City book: the Goods page's Requests card and the Finances page's trade lines.
   - Messages for caravans, ships and requests.
   - Custom Game's Trade partners.
7. **Later milestones can add to it.**
   - M9's ratings read the trade income and the request log.
   - M11's advisors and event popups speak for it.
   - Phase 2 adds gifts, relations and tribute on the same request log, and warships and blockades on the same routes.

## What exists today

**Design:**
- GDD 5.12:
  - Routes cost gold to open; "land routes need a trade post, river and sea routes need a dock". This plan drops the trade post, and Phase 5 rewords the GDD.
  - Buy and sell limits are set per good and per partner.
  - Caravans, river boats and sea junks are walkers that travel to the trade building.
  - Imports always arrive by delivery to the storage yards.
  - Prices move with supply and demand, and with events.
  - Trade income feeds Prosperity and is a goal in several missions.
- GDD 5.14: requests come from the court or a foreign city, for goods, gold or labour within a time limit. Fulfilling one raises Legitimacy and relations, failing lowers them, "and the court may ask for more". Gifts, relations and tribute are Phase 2's.
- GDD 5.1.1:
  - Imports bring construction materials.
  - "Every mission must be able to bootstrap itself, from its starting stock and local production, or through an import route that is open from the start."
  - Imports arrive in I-5.
- GDD 7: the goals `TradeIncome { per_year }` ("2,000 gold a year from exports") and `RequestsFulfilled { count }`.
- The cost tiers (GDD 5.1.2) list the trade post and the dock as Civic.
- The mockups (`docs/design/mockups/generator/`):
  - The world map's side panel (`content.py`: Vijara 41–51, Hamsara 87–97, Yasonagara 134–144): relations, the route line ("Sea route · junks every 3 months"), They sell and They buy with prices, the court's request, and the buttons Set trade limits and Send a gift.
  - The Goods page's Requests card (`pages.py:354–362`): the request's name, when it's due, a have/need bar, "18 / 30" and a Send now button.
    - Đại Long's card is "Pottery for the court, 18 of 30, 4 months left" (`pages_content.py:61`).
    - The Overview has a matching warning, "The court's pottery request: 18 of 30 jars, 4 months left." (`:59`).
  - Goals in the HUD and the debrief: "Trade income 640 / 1,000 a year" and "Court requests 2 / 3" (`pages_content.py:62`, `screens_content.py:24–40`). The Mandate's drivers include "Court requests met +16" and "Late tribute −4" (`pages_content.py:43`).
  - HUD messages: "The court asks for 30 jars of pottery within 6 months." and "A caravan from the highland clans arrived with timber." (`screens_content.py:39–40`).
  - Kambara's event popup: "The king asks for bricks: send 300 bricks within 12 months. When it is met: Divine Favour +8. If it is late: Divine Favour −10." (`dialogs_content.py:89–93`, `event-kambara.jpg`). The popups are M11's.
  - The build palette's Trade sector ("Nothing to build here yet", `palette.py:44`) and a 3 × 3 trade post on the minimap mockup (`minimap.py:108`).
  - There's no dock or trade limits dialog mockup.

**World map data** (`crates/game/src/world_map/`, `assets/configs/game/world/*.json`):
- Each campaign's world file has one city with `role: "trade_partner"` and a static `trade` block (`CityTradeDef`, `config.rs:139`):
  - `relation` (0–5);
  - `route` (`TradeRouteDef`: the route id, the city it runs from, the route line and carrier keys, `every_months`);
  - `sells` and `buys` (`TradeGoodDef { good, price }`, the good a Menu key `trade_good.<id>`);
  - `request` (`TradeRequestDef { good, amount, within_months }`).
- Routes already have a `kind`: `land`, `river` or `sea` (`dragon_legacy.json:20–24`).
- Campaign I's partner is Vijara, by sea from Bến Gốm (`dragon_legacy.json:15`). The others are `neighbour`s. Among them are the Clan camps, joined to Chợ Đèo (mission 5's city, a placeholder) by the open land route `cho_deo_clans`, and Đê Son, on the river route `phu_sa_de_son`.
- The trade goods are free text keys (salt, aloeswood, cotton, silk, ceramics, rice, glazed jars, cardamom, palm sugar, iron, lacquerware, gems); only rice is a `ResourceKind`.
- `WorldMapProvider` (`provider.rs:140`) is the screen's only source, so trade can replace the static values (`city_panel`, `:321–402`, with `TODO(realm-ui)`s for M10). The panel's Set trade limits and Send a gift are disabled with "Coming with trade" (`menu/dialog/world_map/mod.rs:853–862`).

**Logistics:**
- **The map's edge.**
  - A map has one entry point, the terrain flag `TileFlags::SettlersSpawnPoint` (`tile/mod.rs:139`), cached by the path graph (`pathfind/mod.rs:317`).
  - Settlers spawn there (`system/settlers.rs:104–124`; without one, they fall back to the bottom edge's middle and flag it) and walk over land and roads to a lot.
  - The sample saves set it on their bottom edge, presets 9–11 at (4, 0), and the other presets have none.
  - Nothing marks where a river or the sea leaves the map.
- **Walkers.**
  - A unit is spawned with a task through `Unit::try_spawn_with_task_deferred_*` (`unit/mod.rs:515`, `:551`). A building's walker goes through `UnitTaskHelper` (`Runner`, `Patrol`).
  - Tasks (`unit/task/mod.rs:74–84`) include:
    - `UnitTaskDeliverToStorage` (`deliver.rs:53`): a producer's runner carries one good to the nearest store that takes it. It can go on to a `completion_task`, such as walking home.
    - `UnitTaskFetchFromStorage`: a market's runner fetches one good from a shopping list, back to its building.
  - A unit's inventory holds one good at a time, any amount (`unit/inventory.rs`). A walker despawns with an empty inventory (`task/despawn.rs:100`).
- **Water.**
  - The path graph has `NodeKind::Water`, and a search walks whatever `traversable_node_kinds` its unit's config names (`pathfind/mod.rs:946–990`). The bird already crosses water.
  - The water tile and the canal are both `path_kind: Water` (`water::is_canal_def` tells them apart), so a ship's search needs a filter that keeps it out of canals (`SimContext`'s `find_path*` take a `path_filter`, `sim/context.rs:372–460`).
  - Nothing moves on water today.
- **The fishing wharf** (`BuildingKind::FishingWharf`, a producer) is the one building placed on the water's edge. Its footprint must be water, not canal, beside land (`tile/placement.rs:128–162`). Its variation is the bank it faces (north, south, east, west), set after placement (`water::update_port_wharf_orientation`, `tile/water.rs:270–320`). `water::is_port_or_wharf` knows it by its tile's hash. It catches fish on its production timer; it has no boat.
- **Stores.**
  - `StorageBuilding` (`building/storage.rs:112`) has `StorageSlots`: 8 slots of 8, one good to a slot. The storage yards and the granaries are both stores. A store takes goods only while staffed.
  - `visited_by` (`:151–209`) handles deliveries and fetches, and panics on any other task.
  - A store's orders per good (`StorageOrder`: Refuse, Accept, Get, Empty; saved by name since v9) are set from the inspector and `storage.orders`. A store refuses a good it's set to Refuse or Empty (`StorageSlots::accepts`). Get and Empty send no runners until M11.
- **City stock.** `CityStock` (`world/city_stock.rs`) reads and withdraws across stores at once, staffed or not, keeping the world stats in step. Construction takes from the yards (`CITY_STOCK_BUILDINGS`), festivals from the yards and granaries (`FESTIVAL_STOCK_BUILDINGS`, `available_in`, `withdraw_from`).
- **Goods.** `ResourceKind` (`sim/resources.rs:32–54`):
  - foods: Rice, Meat, Fish;
  - consumer goods: Wine, Pottery;
  - raw materials: Wood, Metal, Clay, Bricks, RoofTiles;
  - and Gold.

  Per-good state is saved by name (`per_kind_map`).

**Buildings and art:**
- `BuildingKind` is a `u32` bitflag with bits 0–25 used (`building/mod.rs:92–129`). Its archetypes are Producer, Storage, Service and House (`:1525`), each with its pool in `World`. Services without a patrol exist: the sluice gate and the festival square.
- `TileSector::TradeAndEconomy` exists with its palette button, sector glyph and minimap colour, but no tile def uses it (`tile/sets.rs:113`, `menu/in_game/palette.rs:136`). The HUD has `trade` and `request` line icons (`menu/skin_hud.rs:120`, `:143`).
- No trade building, caravan, trader, dock or ship exists. There's no caravan, pack-animal or boat art. The units are the peasant, runner, settler, vendor, tax collector, water carrier, guard, fire watcher, teacher, actor, monk, medic, dog, bird and buffalo.
- The Blender pipeline makes buildings, overlays and terrain pieces, not units. Walkers wait for its walker experiment (placeholder-art Phase 6). A ship has no walk cycle, so it's a smaller step: a rigid model shot in each direction.

**Finance:** `LedgerCategory` (`sim/finance.rs:35`) has Taxes and Offerings (income), and Construction, Wages, Upkeep and Festivals (spending). It's saved by name, so new lines need no save-version bump. The Finances page lists its lines by hand (`menu/dialog/city_book/finances.rs:82–95`).

**Missions and goals** (`campaign/config.rs`):
- `MissionDef` (`:189`) has `finance`, `risks`, `faith` and `floods` blocks, each `#[serde(default, deny_unknown_fields)]`. A `trade` block goes beside them.
- `MissionGoal` (`:450`) has `Population`, `Employment`, `Treasury`, `Resource`, `FestivalsHeld` and `FloodsSurvived`, met against `WorldStats` (`campaign::tick`, `mod.rs:358`). The debrief, the briefing, the HUD's goals panel and the debug window match every variant.
- Campaign I's three missions are placeholders until M13 writes I-1 to I-5. None has a mission 5 yet.

**Waiting for trade:**
- Faith (`faith/config.rs:92–141`): `FaithEffectDef::Envoys` (Kambara's Sky Deva, "envoys bring gifts") and `TradeWindfall` (Tampadesa's River Spirit, "easy river trade") are both `requires: "trade"`. `FaithSystems` has only `floods`, so they never come.
- The City book's Requests card always says "No requests from the court yet." (`city_book/goods.rs:54–93`).
- Custom Game's Trade partners detail always shows "—" (`custom_game.rs:371`).

**Save version:** 11. The roadmap's log had 12 for terrain height (T), which hasn't started. M10 takes 12, and T moves to 13.

## Design

### 1. Partners, routes and goods as data (Phase 0)

- **Goods are `ResourceKind`s or display-only.**
  - A world file's `sells` and `buys` take `good`. A `ResourceKind` name (`"Wood"`) is traded. A `trade_good.<id>` key is shown on the panel with "not traded yet", for a good the game doesn't have (silk, salt).
  - Each traded entry gains `per_year` (how many the partner sells or buys a year) beside its `price` (the base price, gold a unit).
  - The existing keys stay, so Vijara's, Hamsara's and Yasonagara's panels read as today.
- **A partner's route** takes its `kind` from the route it names (land, river or sea), and gains `open_cost`. A land route is served by caravans, a river or sea route by ships (§5).
- **The mission's `trade` block** (`MissionTradeDef`, beside `faith` and `floods`):
  - `partners`: city ids from the campaign's world file. Each may be `open` at the start, has an `entry` cell on the map (a land cell for a caravan, an open-water cell on the map's edge for a ship), and may override `open_cost`, prices and amounts.
  - `requests` (§6).
  - `enabled`: on by default when the block lists partners.

  A mission without the block has no trade: missions 1 to 4 trade nothing, as they teach other things.
- **Campaign I's partners for I-5** (Q5):
  - The highland clans: the Clan camps, a land route from Chợ Đèo, open at the start. They sell Wood and Metal and buy Rice and Pottery.
  - The delta: the player's earlier cities, a land route opened for gold. They sell Bricks and RoofTiles and buy Wood and Metal.

  M13 writes the mission. M10 writes the world file's terms, and test missions: one by land, and one by river with a dock.
- **Custom games:** a map in `maps.json` takes an optional `trade` block like a mission's, naming partners from its realm's first campaign's world file. The Blank map and the presets list that realm's land partners, so trade can be tried outside the campaign. Custom Game's Trade partners detail lists them.
- **Checks** (`WorldMapConfigs::validate`, `CampaignConfigs::validate`):
  - every traded good is a `ResourceKind`;
  - prices and amounts are positive;
  - a mission's partners exist and have a route.

  An `entry` is checked when the mission starts. A land entry must be walkable inside the map, else the settlers' spawn point stands in, with a warning. A water entry must be open water on the map's edge, else the nearest such cell of the largest water body stands in, with a warning.

### 2. The trade state (Phase 0)

`sim::trade::CityTrade` (`sim/trade.rs`) is saved in `Simulation` beside `faith` and `floods`, and read through `SimContext::trade()`. It holds:
- per partner, by city id:
  - whether its route is open;
  - the limits per good (`TradeLimit`: off, import up to N, export above N; saved by good name with `per_kind_map`);
  - what it sold and bought this year, per good;
  - its price offsets per good;
  - the month its next caravan or ship leaves;
- the requests under way and the request log (§6);
- the caravans and ships under way, by unit id, for the panel's "on its way";
- the trade income this year and last (also in the ledger; kept here for the goal and M9).

`CityTrade::start_mission` fills it from the mission's block (or a custom game's map). `sync_partners` matches it to the configs on load, as faith's `sync_patrons` does.

The knobs are `GameConfigs::sim.trade` (`TradeRules`): the first arrival after a route opens, the caravan's and the ship's capacity, a caravan's most stops, a ship's longest stay, the docker's load, the price step and its cap, and the drift back.

**Gold:**
- Exports earn `LedgerCategory::Exports`.
- Imports spend `Imports`.
- Opening a route spends `Routes`.
- A gold request (later) spends `Court`.

The Finances page gains the three lines.

**What the city holds**, for the limits, is its holdings across the storage yards and the granaries (`FESTIVAL_STOCK_BUILDINGS`), staffed or not, with the goods on its own runners and dockers. A caravan's or ship's cargo isn't the city's, so neither the limits nor the world stats count it.

### 3. Routes and limits (Phase 1)

- **Opening a route:**
  - The world map's side panel shows Open the route, with its cost, for a closed route of a partner in this mission, and Set trade limits once it's open.
  - A river or sea route's Open the route is disabled until the city has a finished dock, with the tooltip "Build a dock first".
  - Opening spends `Routes` and schedules the first arrival `first_arrival_months` later.
- **Limits:** the trade limits dialog (`TradeLimits`) opens from the panel's Set trade limits, and from the dock's panel for its partners.
  - It has a row per good the partner sells (Import: off, or up to N in the city, a step selector of amounts) and per good it buys (Export: off, or keep N).
  - Each row shows the price now and this year's amount left.
  - The limits are per partner (GDD 5.12).
- **The schedule** (`trade::on_new_month`, after the floods' and the farmsteads' months, before faith's rolls):
  - Each open route sends a caravan or a ship every `every_months` (the world file's), at the start of a month, from its `entry`.
  - Nothing comes while that partner has no limit set, or nothing to trade: no import below its limit and no export above its limit, within this year's amounts.
  - For a river or sea route, nothing comes while the city has no working dock its water reaches.
  - A skipped month says why in a message, the first time.

### 4. Caravans: trading at the stores (Phase 1)

- **The caravan** (Q3): a new walker, `UnitConfigKey::Trader` (the vendor recoloured by `crates/tools/unit-recolor`, until the Blender walker experiment).
  - It walks over land and roads as settlers do (`EmptyLand | Road | SettlersSpawnPoint`), with `UnitTaskTrade`.
  - Its cargo is kept in the task, several goods, not in the unit's one-good inventory: at most `caravan_capacity` units (32, four of a store's slots) between what it brings and what it buys.
- **What it brings:** as it sets out, it loads each good the partner sells that the city imports, min(the limit − what the city holds, this year's amount left), within its capacity.
- **Store by store** (Q1). It goes to the nearest staffed storage yard or granary it can reach where it has something to do. Then it trades there:
  - **First it sells.** For each good it carries that the store takes (`StorageSlots::accepts`, so the store's orders count), it unloads min(what it carries, the store's room for it, the limit − what the city holds). The city pays the price for each unit.
  - **Then it buys.** For each good the partner buys that the city exports, it loads min(what the store holds, what the city holds − the limit, this year's amount left, its room left). The partner pays for each unit.

  Then it goes on to the next store, up to `caravan_stops` (6). Unlike Pharaoh's caravans, it buys from granaries too, so a later mission can trade food (rice, fish) both ways.
- **It leaves** once it's sold everything and is full, when no store in reach is left to trade with, or after its last stop, and walks back to the entry and despawns. What it didn't sell leaves with it, unpaid. A caravan that can't reach any store turns back, with a message.
- **At a store:** `StorageBuilding::visited_by` gains the trade task. The visit is deferred, as other store visits are, and reads the prices at the visit (§7).
- **Messages:** each caravan logs a `TradeNotice` when it leaves the city, which `GameLoop` posts as a `MessageKind::Trade` message with the `trade` line icon: "A caravan from the highland clans brought 20 wood and took 16 rice: +112 gold".
- **Saves:** a caravan is a unit with its task, saved as every unit is. The new task variant goes at the end.

### 5. Docks and ships (Phase 2)

- **The dock:**
  - `BuildingKind::Dock` (bit 26) is a 2 × 2 service without a patrol (as the sluice gate and the festival square are) in the Trade sector, with 4 to 8 workers, Civic tier (gold, wood and bricks).
  - It's placed as the fishing wharf is: its footprint on open water (not a canal) at the shore, its variation the bank it faces. `water::is_port_or_wharf` becomes a tile def flag (`on_shore`) instead of the wharf's hash.
  - Its art comes from the Blender pipeline: a catalog entry with the four bank variations, a timber quay on piles with a crane and bales. Its palette glyph comes from ui-art in each realm, and its minimap landmark is the existing `port` icon.
  - It has a berth: the open-water cells beside its footprint on the water side, where a ship moors.
- **The ship:**
  - A new unit, `UnitConfigKey::TradeShip`, a sea junk in every realm for now (Q15).
  - It's `traversable_node_kinds: "Water"`, with a path filter that refuses canals.
  - It's the Blender pipeline's first unit (Q16): a rigid model rendered in the unit sprite format, in the four directions units have today, with a slow two-frame bob as its idle and its move, with no walk cycle. The pipeline gains a unit output for it; the walker experiment stays Phase 6's.
  - It carries `ship_capacity` units (64, a storage yard's worth) in its task, `UnitTaskShip`, as the caravan does.
- **Its voyage:**
  1. It sets out from the route's water `entry` with what the city imports, as a caravan does.
  2. It sails to the nearest working dock whose berth its water reaches and no ship holds. If every such dock is busy, it waits off the nearest one.
  3. It moors at the berth and tells the dock.
  4. It sails back to the entry and despawns once the dock has no errand left for it, or after `ship_stay_months` (1), whichever comes first. Like a caravan, it takes its unsold goods home unpaid.
- **The dockers:** while a ship is moored, the dock's dockers (up to `dockers`, 2 at once, fewer with fewer workers) run its errands, a load (`docker_load`, 8 units) at a time:
  - **Unloading an import:** a docker takes a load off the ship (the city pays for it then) and carries it to the nearest store that takes it. This is `UnitTaskDeliverToStorage` with a walk home as its completion task. No docker sets out while no store has room for that good, or once the city holds its limit.
  - **Loading an export:** a docker fetches a load from the store that holds the most of it (`UnitTaskFetchFromStorage`, from yards and granaries) and brings it back, and it goes onto the ship (the partner pays for it then). The loads stay within the limit, this year's amount and the ship's room.
  - Dockers walk the roads, as runners do.
- **Messages:** the ship logs a `TradeNotice` as it sails ("A junk from Vijara brought 24 salt and took 30 pottery: +540 gold"). A ship with no dock to reach turns back, with a message.
- **The dock's panel:**
  - its berth: the ship moored and its partner, what it's unloading and loading, and the days left of its stay;
  - the dockers' errands;
  - the next ship, partner and month;
  - the routes it serves, with Set trade limits.

  The ship's panel shows its partner, cargo and errand.
- **The minimap** draws a ship as a dot on the water, as it draws walkers if it does; otherwise nothing (Phase 2 checks).

### 6. Court requests (Phase 4)

- **The schedule** is the mission's `requests`. Each request has:
  - an `id`;
  - `from`: a city id, the capital for the court or a partner;
  - `good` and `amount`: goods only in M10; gold and labour come later;
  - `month`: months after the mission's start;
  - `within_months`;
  - `legitimacy` and `relation` effects for M9 and Phase 2: logged, not applied.

  The world files' static `request` stays the panel's display until a mission's schedule names that city.
- **A request opens** at its month (`trade::on_new_month`). It brings:
  - a `Request` message ("The court asks for 30 pottery within 6 months.");
  - the Goods page's Requests card;
  - an Overview warning while it's open;
  - a HUD goal line when a goal counts requests.
- **Sending** (Q8): the card's Send now is enabled once the city holds the amount in its storage yards and granaries. The goods are taken at once (`CityStock::withdraw_from`, as festivals do), and the request is met and logged. No walker carries them in M10: GDD 5.14 allows "a dispatch building or the trade post", and Pharaoh sends from storage at once.
- **Failing:** a request not met when its months run out is failed and logged, with a Warning. "The court may ask for more" waits for M9 (Q9).
- **Goals** (Q10), with their text in the debrief, the briefing and the goals panel:
  - `MissionGoal::RequestsFulfilled { min }`: the log's met requests, `WorldStats::trade.requests_met`.
  - `MissionGoal::TradeIncome { per_year }`: the exports' gold this calendar year or last, whichever is higher, `WorldStats::trade.income`.

  M9 lists them; they come with the system they count, as M7's `FloodsSurvived` and M8's `FestivalsHeld` did.

### 7. Prices (Phase 3)

- Each traded good has a base price per partner (the world file's). The partner's price now is base × (1 + offset).
- The offset moves by `price_step` (2%) for every `price_units` (10) units traded: down for what the partner buys from the city, up for what it sells to it. It stays within ±`price_cap` (40%) and drifts a quarter of the way back to 0 each month.
- Relations (0–5) scale the price by `relation_step` (3%) a level from 3 (Q7). They don't move in M10.
- The side panel's They sell and They buy show the prices now, with an arrow when they've moved. The dialog shows them too, and `trade.state` reports them.

### 8. Faith

- `FaithSystems` gains `trade`, met when a route is open.
- The River Spirit's `TradeWindfall` raises export prices and lowers import prices by its scale for its months (`CityFaith::trade_scale`).
- The Sky Deva's `Envoys` sends a gift caravan of a partner's goods, unloaded free at the stores (Q11).

### 9. What the player sees

- The build palette's Trade sector: the dock, its first building.
- The world map's side panel:
  - the route line with the next caravan or ship;
  - Open the route (with its cost; for a river or sea route, "Build a dock first") or Set trade limits;
  - live prices and this year's amounts;
  - the request with its months left;
  - Send a gift stays disabled ("Coming later").
- The trade limits dialog.
- The dock's inspector panel; the caravan's and the ship's unit panels. A storage yard's or granary's panel says when a caravan last traded there.
- The City book:
  - the Goods page's Requests card (the mockup's: a card per request under way, with Send now);
  - the Overview's warning for a request under way;
  - the Finances page's Exports, Imports and Routes lines.
- The message log's `Trade` and `Request` kinds.
- Custom Game's Trade partners.
- The HUD's goals panel and the debrief's goals for the new goals.

### 10. Automation, tests and scenario

- **Commands:**
  - `trade.state`: partners, routes, limits, prices, amounts left, the next arrivals, caravans and ships under way with their cargo and stops, docks and their berths, requests under way and the log, income.
  - `trade.set`: a partner's limit per good, a route opened or closed, a price offset, the next arrival's month.
  - `trade.open partner=...`: the player's path, with its cost and the dock check.
  - `trade.arrive_now partner=...`: sends a caravan or ship now.
  - `requests.state`, `requests.send id=...`, and `requests.open id=...` (opens one from the mission's schedule now).
  - `world.place` of the dock.
  - World Debug gains a Trade tab.
- **Tests** in a new `tests/trade.rs`:
  - the data and its checks, and the state's round trip;
  - a caravan's stops and its trade within the limits, the stores' room and orders, and the amounts, granaries included;
  - a ship moored at a dock, its dockers' loads both ways, a busy dock, a dock out of reach, a canal refused;
  - gold in the ledger;
  - prices moving and drifting;
  - requests met and failed;
  - the goals;
  - faith's trade effects.
- **`trade.jsonl`:**
  - open a land route from the world map, set limits in the dialog, and wait for a caravan: check its stops, the yards, a granary and the treasury;
  - lay a river on a blank map (`world.place_area tile=water`), build a dock from the Trade grid, open the river route, and wait for a ship: check the dockers both ways;
  - meet one request from the Goods page and let one fail;
  - save screenshots to `out/trade/`.

## UI changes

| Where | Change | Phase | Files |
|---|---|---|---|
| World map, side panel | Open the route (its cost; "Build a dock first" for a river or sea route without a dock) and Set trade limits enabled; the route line's next arrival; live prices and amounts; "not traded yet" for display-only goods; Send a gift stays disabled with a new tooltip. | 1, 2, 3 | `menu/dialog/world_map/`, `world_map/provider.rs` |
| New dialog | `TradeLimits`: a row per good with an import or export step selector, the price and the amount left. No mockup: built in the Goods page's style. | 1 | `menu/dialog/trade_limits.rs` (new) |
| Inspector, unit | The caravan's and the ship's errand and cargo. | 1, 2 | `inspect/unit.rs` |
| Inspector, storage | "Last traded with a caravan" on a yard's and a granary's panel. | 1 | `inspect/building.rs` |
| Build palette | The dock in the Trade sector (its glyph in each realm); the sector button no longer empty. | 2 | `tile_set.json`, `configs.json`, ui-art, `menu/in_game/palette.rs` |
| Inspector, dock | The dock's panel: berth, dockers, next ship, its routes with Set trade limits. | 2 | `inspect/building.rs`, `menu/in_game/inspector/` |
| City book, Goods page | The Requests card after the mockup, with Send now. | 4 | `menu/dialog/city_book/goods.rs` |
| City book, Overview | A warning for each request under way. | 4 | `city/report.rs` |
| City book, Finances page | Exports, Imports and Routes lines. | 0 | `menu/dialog/city_book/finances.rs` |
| Message log | `Trade` and `Request` message kinds with the `trade` and `request` line icons. | 1, 4 | `messages.rs`, `menu/in_game/message_log.rs` |
| HUD goals, debrief, briefing | The `TradeIncome` and `RequestsFulfilled` goal lines. | 4 | `menu/dialog/mission_complete.rs`, `menu/in_game/top_bar.rs` |
| Custom Game | The map's trade partners in its details. | 1 | `menu/dialog/custom_game.rs` |
| Minimap | The dock's landmark. | 2 | `tile_set.json` (`minimap` block) |
| Developer UI only | World Debug's Trade tab. | 0 | `debug/world.rs` |
| Map drawing | The caravan, the dock and the ship. Not UI, but visible. | 1, 2 | tile sets, unit sprites |

## Phases

### Phase 0: Data, state and the save format (medium)

- [ ] World files: traded goods as `ResourceKind` names with `per_year`, display-only goods kept as keys; routes' `open_cost`; Campaign I's land partners (the clans, the delta) and their terms; checks.
- [ ] The mission's `trade` block (`MissionTradeDef`) with land and water entries, and a map's in `maps.json`; checks.
- [ ] `sim::trade::CityTrade` in `Simulation`, `SimContext::trade()` and `SimContextMut::trade_mut()`, started per mission and custom game, synced on load; `TradeRules` in `GameConfigs::sim`.
- [ ] `LedgerCategory::Exports`, `Imports` and `Routes`, and their Finances page lines.
- [ ] Save v12, `v11_to_v12.py` (an empty trade state), migrated sample saves, `--smoke-test-saves`; the roadmap's log moves T to 13.
- [ ] `trade.state`, `trade.set`; World Debug's Trade tab.
- [ ] Tests: the data, the round trip.

### Phase 1: Land routes, limits and caravans (large)

- [ ] Open the route from the world map's panel (`trade.open`), its cost in `Routes`.
- [ ] The `TradeLimits` dialog from the panel; `trade.set` limits.
- [ ] The caravan: `UnitConfigKey::Trader` (recoloured art), `UnitTaskTrade`, the monthly schedule from the route's entry, its stops at yards and granaries, gold in `Exports` and `Imports`, `TradeNotice` messages (`MessageKind::Trade`).
- [ ] The caravan's unit panel; a store's last caravan.
- [ ] Faith's `trade` system: `TradeWindfall` and `Envoys`.
- [ ] Custom Game's Trade partners.
- [ ] `trade.arrive_now`; tests; `trade.jsonl`'s land part.

### Phase 2: Docks and ships (large)

- [ ] `BuildingKind::Dock` in the Trade sector: its config and tile def, placed at the shore (`on_shore` in place of the wharf's hash), its four bank variations from the Blender pipeline, its palette glyph in each realm, its minimap landmark.
- [ ] The tile generator's unit output (a rigid model in the unit sprite format) and the trade ship's sprites.
- [ ] The ship: `UnitConfigKey::TradeShip` on water, kept out of canals, `UnitTaskShip`, its voyage from the water entry to a free berth and back, waiting off a busy dock.
- [ ] The dockers' errands both ways, paid per load; the ship's stay.
- [ ] River and sea routes: Open the route needs a dock; nothing sails while no working dock is in reach.
- [ ] The dock's and the ship's panels; messages.
- [ ] Tests; `trade.jsonl`'s river part.

### Phase 3: Prices (small)

- [ ] Price offsets per partner and good, moved by trade and drifting back; relations' scale.
- [ ] The panel's and the dialog's live prices.
- [ ] Tests; the scenario checks a price moving.

### Phase 4: Court requests and the goals (medium)

- [ ] The mission's request schedule, opened monthly; met (Send now, `requests.send`) or failed; the log.
- [ ] The Goods page's Requests card, the Overview's warning, `MessageKind::Request` messages.
- [ ] `MissionGoal::RequestsFulfilled` and `TradeIncome`, with `WorldStats::trade`, and their lines in the debrief, the briefing and the goals panel.
- [ ] `requests.state`, `requests.open`; tests; the scenario's requests.

### Phase 5: Documentation (small)

- [ ] AGENTS.md: a Trade entry under Runtime architecture, `trade.jsonl` in the scenario list and `trade` in the test list.
- [ ] The roadmap: the M10 and M11 rows and the save-version log.
- [ ] The GDD: 5.12 without the trade post (land routes need nothing; river and sea routes need a dock) and 5.1.2's Civic list; campaigns.md's I-5 line.
- [ ] known-issues: what's left.

## Risks

- **The map's edge.**
  - Caravans need a way in from the land, and ships from the water. The sample saves' spawn cells sit on the bottom edge, possibly just outside the trimmed playable area. The maps mark no water edge at all.
  - A mission's `entry` per partner fixes it for I-5. Test maps need entries too, and the fallbacks (§1) cover the rest.
  - Phase 1 checks a land entry is walkable and reaches a store, and Phase 2 that a water entry reaches a berth. The panel says so when one doesn't.
- **A caravan's long errand.** Store by store, a caravan walks more than a trade post's would have, and each stop is a deferred visit.
  - The stop limit and the "something to do" test (read from `CityStock` and the stores' slots, not a path search per store) keep it short.
  - Two caravans at once may race for the same goods; the visit re-reads what's there.
- **Ships on the water.** No unit has moved on water before.
  - The water tiles' shore cells, the wharves' and docks' footprints (`Building` nodes) and the canals must all path right.
  - A dock's berth must be reachable open water: a dock on a pond cut off from the edge can't trade, and its panel says so.
  - Flooded land isn't water, so ships stay off it.
- **A unit from the Blender pipeline.** The ship is the pipeline's first unit. If the unit output takes long, a flat placeholder (the wharf's boat, if any, or a drawn hull) stands in, and Phase 2 says so.
- **Multi-good cargo.** A caravan or ship carries several goods, while the unit inventory holds one. Keeping the cargo in the task, and out of the city's stats (it isn't the city's), keeps the despawn check and the tallies right. The dockers carry the city's goods in their inventories, which the stats count as they count every runner's.
- **Bootstrapping I-5.** The mission must be able to bootstrap from its starting stock or an open route (GDD 5.1.1). With no trade post to build, an open land route at the start is enough. M13 sets the starting yard.
- **Balance.** Prices, amounts, the intervals, the capacities and the routes' cost are first guesses, tuned by M13 against "trade income 1,000 a year".

## Open decisions

Each has a default, and the user confirmed them on 2026-11-21. Q1 was decided otherwise, and Q2 moved back with it.

- **Q1. Where caravans trade.** *Decided (2026-11-21): no trade post; caravans go from store to store, as in Pharaoh, and buy from granaries as well as storage yards.* Pharaoh's caravans buy only from yards; buying from granaries is for later missions that trade food. Ships trade through a dock and its dockers (Q15–Q17).
- **Q2. M11's Get and Empty orders.** *Decided with Q1: they stay in M11.* The default brought them forward because the trade post needed both; caravans trade at the stores themselves, and the dockers carry goods as producers' and markets' runners do.
- **Q3. The caravan's art and walk.** *Default (confirmed): one walker, the vendor recoloured, walking land and roads as settlers do.* A trader with pack animals (the buffalo following) or a cart waits for the Blender walker experiment (placeholder-art Phase 6); Phase 2's new walkers get eight directions then.
- **Q4. Limits.** *Default (confirmed): per partner and per good, "import until the city holds N" and "export above N", the city's holdings counted in yards and granaries.* As in Caesar III and Pharaoh; the GDD's "buy and sell limits for each good and each partner".
- **Q5. Campaign I's land partners.** *Default (confirmed):*
  - *the highland clans, open at the start: wood and metal for rice and pottery;*
  - *the delta, the player's earlier cities, opened for gold: bricks and roof tiles for wood and metal.*

  The mockup's message has a caravan from the highland clans bringing timber, and trading with the player's own earlier cities is as in Pharaoh. The values are M13's to balance.
- **Q6. Where caravans and ships enter.** *Default (confirmed): a partner's `entry` cell in the mission's trade block; else the settlers' spawn point for a caravan, and for a ship the nearest open-water cell on the map's edge of the largest water body.* A pass has two ends (I-5: the delta down one side, the clans up the other).
- **Q7. Prices.** *Default (confirmed): ±2% per 10 units traded, capped at ±40%, a quarter of the way back each month; relations ±3% a level from 3, fixed in M10.* Simple supply and demand (roadmap M10).
- **Q8. Sending a request.** *Default (confirmed): at once from the yards and granaries, from the Goods page's Send now, once the city holds the amount.* Pharaoh's dispatch; the mockup's card has Send now. A dispatch walker can follow with M11.
- **Q9. Failed requests.** *Default (confirmed): logged with a Warning; Legitimacy and "the court asks for more" wait for M9.*
- **Q10. The goals.** *Default (confirmed): `TradeIncome { per_year }` (the higher of this year's and last year's export gold) and `RequestsFulfilled { min }` come with M10, not M9.* They count what M10 builds, as M7's and M8's goals did.
- **Q11. Faith's trade events.** *Default (confirmed): easy trade scales prices by its effect for its months; envoys bring a gift caravan of a partner's goods, free.* They wait on `FaithRequirement::Trade` today.
- **Q12. Gifts, relations and tribute.** *Default (confirmed): not in M10 (Phase 2).* Relations stay the world file's values; Send a gift stays disabled.
- **Q13. How many docks.** *Default (confirmed): any number; a ship goes to the nearest working dock with a free berth its water reaches, and the limits are the city's, not a dock's.*
- **Q14. Gold requests.** *Default (confirmed): goods only in M10; the `Court` ledger line comes with the first gold request.* I-5's requests are goods (the mockups' pottery).
- **Q15. One ship or two.** *Default (confirmed): one trade ship, a sea junk, for river and sea routes alike, in every realm.* The GDD names river boats and sea junks. A smaller river boat, and each realm's own hull, can follow as art alone, since the route's kind is already in the data.
- **Q16. The ship's art.** *Default (confirmed): the Blender pipeline's first unit, a rigid model in the four directions units have, bobbing on two frames.* If unit output proves slow, a placeholder stands in for Phase 2 (Risks).
- **Q17. The dock's size and dockers.** *Default (confirmed): 2 × 2 on the shore like the fishing wharf, 4 to 8 workers, two dockers at a time carrying 8 units a load, one ship moored at a time, a ship's stay at most a month.* As in Pharaoh, where dockers carry between the ship and the storage yards while it waits.
