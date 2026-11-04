# Heritage Builder — Game Design Document (GDD)

**Working title:** Heritage Builder
**Genre:** 2D isometric historical city builder
**Platforms:** PC (macOS, Windows, Linux) and web browser
**Engine:** custom, in Rust (OpenGL or Wgpu on desktop, Wgpu on web)

Related documents:
- [world.md](world.md): the fictional world, naming rules, realms and timeline
- [story.md](story.md): the three campaign stories and the cast
- [campaigns.md](campaigns.md): all 30 missions
- [story-script.md](story-script.md): story comics
- [overview.md](overview.md): audience, positioning and development phases

## 1. Concept

Heritage Builder is a classic walker-based city builder in the tradition of Impressions Games, set in a fictional world drawn from three civilisations of mainland Southeast Asia. In each campaign you are a governor sent out by your realm. Your job is to:
- found new cities and make them prosper;
- open trade routes to friends and rivals;
- defend your people with walls and militia;
- raise great monuments that become the realm's lasting heritage.

**Elevator pitch:** *Pharaoh and Emperor, set in the river deltas, temple plains and lake cities of Southeast Asia, across three campaigns and 30 maps.*

**Vision:** let players feel the beauty and the hard-won order of these civilisations. They should see rice paddies ripening behind the dykes, stone barges floating down to a temple-mountain, and gilded spires rising over a dry plain. They should understand why these places became heritage that people still visit today.

### The three realms

The game uses fictional names. Real civilisations are references only; see [world.md](world.md#rules-for-the-fiction).

| Order | Campaign | Realm | Reference | Signature |
|---|---|---|---|---|
| I | *The Dragon Legacy* | Đại Long | Đại Việt (Lý and Trần) | Dykes and floods, examinations, the Mandate |
| II | *The Temple Mountain* | Kambara | The Khmer Empire (Angkor) | Reservoirs and canals, stone temple-mountains, Divine Favour |
| III | *The Plain of Ten Thousand Temples* | Tampadesa | The Pagan Kingdom (Bagan) | Dry-zone irrigation, brick temples, the Merit economy |

## 2. Reference games

| Game | What we take | Where we diverge |
|---|---|---|
| **Caesar III** (1998) | Water infrastructure (reservoirs and aqueducts become Kambara's barays and canals), fire and collapse risk, distant invasions | No legions or forts that the player commands |
| **Pharaoh** (1999) | Walker-based services, housing that evolves, the river flood cycle and floodplain farming, monuments built in stages by workers, the four ratings, a campaign that spans eras | No player-controlled army or navy |
| **Zeus: Master of Olympus** (2000) | Common and elite housing tracks, the world map with trade partners and rivals, requests and gifts, sanctuaries built in stages | No heroes and monsters (spirits and festivals instead), no conquest of other cities |
| **Emperor: Rise of the Middle Kingdom** (2002) | East Asian visual language, Feng Shui–style placement bonuses (here, favourable sites), multi-stage monuments, era progression, walls and gatehouses, scholar-officials and elite housing, the closest overall reference for look and feel | No multiplayer at launch, no military campaigns |

**The key divergence:** defence is automatic. The player builds walls, moats, towers and fortresses. Fortresses produce militia that defend the city on their own. There are no armies to command, no formations, no RTS micro and no conquest of other cities.

## 3. Pillars

1. **A living city.** Every house, walker and cart is simulated. The city works because the player's layout makes it work, and the player can see why by following a walker.
2. **Water shapes everything.** Each realm has its own relationship with water: flood dykes in Đại Long, reservoirs and the lake's pulse in Kambara, irrigating a dry plain in Tampadesa. Water is both the opportunity and the threat.
3. **Heritage worth building.** Monuments are the high point of a mission. They are slow, staged projects that need a strong economy behind them, and they leave a permanent mark: steles, bonuses, and the cities shown on the world map.
4. **Three realms, one craft.** All three campaigns share one set of core rules. Each realm adds signature systems, buildings, goods and art, so the third campaign feels as fresh as the first.
5. **Governance, not conquest.** Legitimacy comes from prosperity, fairness, faith and defending the people. Every story turns on that idea.

## 4. Game structure

### Modes

- **Campaign:** three campaigns of 10 missions each, played in order: Đại Long, then Kambara, then Tampadesa. Missions 1–5 of Campaign I teach the game. Its missions 6–10 and both later campaigns are full city-building challenges where the story unfolds. Each mission is one map, usually one city, which you found or take over.
- **Custom Game (sandbox):** pick a realm, a map, a starting year and goals (or none). Disasters, attacks and economic difficulty can be turned on or off. This mode serves the creative and relaxed player. The home menu already has a placeholder button for it.
- **Later:** a scenario editor built on the existing developer map editor, and mod support for configs and maps.

Unlocking: Campaign II unlocks when Campaign I is finished, and Campaign III when Campaign II is. Custom Game unlocks each realm when its campaign is started.

### Mission flow

1. **Briefing:** an illustrated scroll or comic panel with the story, the goals and the advisor's notes. Some missions offer a choice of two maps, as Pharaoh does.
2. **Play:** build the city. Goals are tracked live in a goals panel. Advisors, court requests, events and attack warnings arrive as messages.
3. **Victory:** when every goal is met, the player may finish now or keep playing until they choose to leave.
4. **Debrief:** the mission's lines are carved into the campaign's stele. A city founded in the mission joins the realm's world map for later missions, as a trade partner or a source of requests.

### Loops

- **Core loop (minutes):** plan, build, supply, upgrade housing, earn taxes and trade, and expand.
- **Mission loop (hours):** meet the basic needs, build production chains, add services and culture, open trade, prepare defences, raise the monument, reach the goals.
- **Meta loop:** a campaign's cities and monuments accumulate on the world map and its stele, and the story moves the realm towards its heritage. Each new realm brings new systems.

## 5. Shared systems

Status codes: **P** = in the prototype today, **P+** = in the prototype but to be expanded, **N** = new.

| # | System | Status | Summary |
|---|---|---|---|
| 5.1 | Terrain, map and construction | P+ | Isometric tile map with land, water and shores, rocks and vegetation; dirt and paved roads; placing, clearing, undo and redo; minimap. Planned: elevation-like terrain features (hills, karst, cliffs), fertility, and favourable sites, flood areas, rice paddies |
| 5.2 | Population and housing | P+ | Houses evolve when their service and goods needs are met; five levels exist today. Planned: a common track and an elite track (5.2.1) |
| 5.3 | Labour and employment | P | Each house supplies workers by level; buildings need staff to run. Planned: labour priorities per sector, as in Pharaoh |
| 5.4 | Walkers and coverage | P+ | Service walkers patrol from their buildings; runners deliver goods; settlers move in. Today, access is decided by distance to the service building. Planned: decide whether to switch to walkers actually visiting (an open item in known-issues) |
| 5.5 | Food and goods distribution | P | Farms and producers, then granary and storage yard, then market, then houses |
| 5.6 | Industry and production chains | P+ | Mines, clay pits, lumberyards and factories (kiln, brick maker, distillery) exist. Planned: per-realm chains (5.13) |
| 5.7 | Finance | P+ | Treasury and taxes collected by the tax office. Planned: wages, tax rate settings, construction and upkeep costs, debt, tribute, yearly accounts |
| 5.8 | City services | P+ | Water, market, religion, health, education, entertainment and security buildings exist. Planned: per-realm versions and coverage overlays |
| 5.9 | City risks | N | Fire, collapse, disease, crime and unrest, each countered by a service walker |
| 5.10 | Seasons and environment | N | Wet and dry seasons, floods, droughts, storms, crop cycles |
| 5.11 | Faith and festivals | N | Patron spirits or deities per realm, favour, blessings and misfortunes, festivals |
| 5.12 | Trade | N | World map, trade routes by river, sea and land, trade posts and docks, buying and selling |
| 5.13 | Realm layers | N | Signature systems, buildings, goods and art per realm (section 6) |
| 5.14 | Diplomacy and requests | N | Court and foreign requests, gifts, relations, tribute |
| 5.15 | Heritage monuments | N | Large projects built in stages from materials and specialist workers |
| 5.16 | Defence and attacks | N | Walls, gatehouses, moats, towers, fortresses, automatic militia; raids and invasions |
| 5.17 | Ratings | N | Prosperity, Culture, Security and Legitimacy |
| 5.18 | Goals, events and advisors | P+ | Campaign goals exist (population, employment, treasury). Planned: the full goal set (section 7), scripted events, advisor messages |
| 5.19 | Story presentation | N | Briefings, comics, steles, in-mission story events |

### 5.2.1 Housing

Two tracks, as in Zeus and Emperor:
- **Common housing:** about 6 levels, from hut to merchant townhouse. It houses the workers. Each level needs more goods (food, then a second food, pottery, cloth, then luxuries) and wider service coverage.
- **Elite housing:** about 4 levels, for scholar-officials, nobles and temple patrons. It is placed as a larger lot and sends no workers. It pays much higher taxes, needs luxury goods, gardens and culture, and raises Prosperity and Legitimacy. Elite housing is introduced in Campaign I, mission 6.

Today's five levels (0–4) become the first five common levels. Each realm draws the same levels differently: timber and tile houses in Đại Long, stilt houses in Kambara, teak and brick houses in Tampadesa.

### 5.9 City risks

| Risk | Cause | Countered by |
|---|---|---|
| Fire | Timber and thatch houses, kilns and workshops | A fire-watch walker; also wells and canals nearby |
| Collapse | Ageing buildings, floods | A builder walker from the builders' guild |
| Disease | Crowding, dirty water, floods | Apothecary and hospital walkers, clean water |
| Crime and unrest | Poverty, unemployment, high taxes, unmet needs | Police or guard walkers, festivals, fair taxes |

Each building shows its risk in its info panel and on a risk overlay.

### 5.10 Seasons and environment

- The year has a **wet season** and a **dry season**, with smaller monthly steps. The calendar shows that realm's own era and the season.
- **Floods** make floodplain fields fertile but destroy anything unprotected. Đại Long controls them with dykes and sluices, and Kambara lives with the lake's pulse.
- **Drought** cuts harvests unless fields are irrigated; it is Tampadesa's main threat.
- **Storms and typhoons** hit coastal maps.
- Crops have growing cycles. Some can only be planted in one season, and irrigation adds a second harvest.

### 5.11 Faith and festivals

- Each realm has 3–5 patron spirits or deities, fictionalised from its traditions (see [world.md](world.md)). Shrines and temples to a patron raise its favour.
- High favour brings **blessings**: a rich harvest, a gentle flood, a trade windfall. Neglect brings **misfortunes**: blight, a severe flood, fire.
- **Festivals** are held from a festival square or temple, cost goods and gold, raise Culture and favour, and calm unrest. Each realm has its own calendar, such as a New Year festival, a water festival or a festival of lights.
- In Đại Long, the Mountain Lord and the Water Lord (after the folk legend of the mountain and water spirits) frame the flood mechanic.

### 5.12 Trade

- The **world map** shows the player's earlier cities, the other realms and the non-playable powers.
- The player pays to **open a route**. Land routes need a trade post, river and sea routes need a dock.
- For each good and each partner, the player sets **buy and sell limits**. Caravans, river boats and sea junks are walkers that travel the map to the trade building.
- Prices move with supply and demand, and with events such as a war that closes a route or a festival that raises demand.
- Trade income feeds Prosperity and is a goal in several missions.

### 5.14 Diplomacy and requests

- **Requests:** the court or a foreign city asks for goods, gold or labour within a time limit. Fulfilling one raises Legitimacy and relations; failing lowers them, and the court may ask for more.
- **Gifts:** sending goods to a city improves relations, opens routes and can avert an attack.
- **Relations:** a value per city, from hostile to allied. It affects prices, requests and attacks.
- **Tribute:** paying a stronger power can prevent an attack, at a cost to Legitimacy.
- There is no conquest and no player-controlled war.

### 5.15 Heritage monuments

- A monument is a large, multi-tile project placed on a chosen site. Some missions require a favourable site.
- It is built in **stages**, for example foundation, terraces, walls, towers or spires, and ornament and consecration. Each stage needs materials delivered (stone, brick, timber, lacquer, gold leaf) plus specialist workers sent by guild buildings: stonemasons, carpenters, bricklayers, gilders and sculptors.
- Construction is visible step by step on the map.
- A finished monument gives **permanent bonuses** (Culture, favour, Legitimacy, trade fame), a stele line, and a place on the world map. Some missions require only certain stages.
- There are about two or three monuments per campaign. See [campaigns.md](campaigns.md).

### 5.16 Defence and attacks

**In scope:**

| Structure | Function |
|---|---|
| Palisade, rampart, wall | Blocks enemies on land. Three tiers: wood, earth, then brick or stone, by realm |
| Gatehouse | Lets the city's own walkers through and stops enemies. Its strength and the rams it can resist depend on its tier |
| Moat | Water dug into the map. Blocks or slows land enemies; it can double as a canal |
| Tower | Placed on or beside walls. Needs staff and fires at enemies in range |
| Watchtower | Gives earlier warning of attacks and shows the approach on the map |
| Fortress / garrison | Trains militia from its staff and weapons stock. Militia leave to fight enemies within the fortress's defence radius and return afterwards. Each realm has its own militia: spearmen and archers in Đại Long, war elephants in Kambara, horse archers in Tampadesa |
| River stakes (Đại Long) | Placed in rivers. Damages and blocks enemy boats |
| Weapons workshop | Turns metal and wood into weapons for towers and militia |

**Attackers:** raiding bands, war parties, siege forces with rams, and naval raiders who land from boats. They:
- come from map edges or along waterways;
- go for loot (granaries, storage, the treasury), the governor's palace and houses;
- set fires, which link to the fire risk.

**Warnings:** an advisor message names the enemy, the direction, the rough size and the months until they arrive. A gift or tribute can sometimes avert an attack.

**Consequences:** damage, lost goods, and lower Security and Legitimacy. In survival missions, losing the palace or citadel fails the mission.

**Out of scope:** recruiting or moving armies, formations, attacking other cities, and player-controlled naval battles.

### 5.17 Ratings

Each rating runs from 0 to 100 and is updated monthly. A ratings panel explains what raises or lowers each one.

| Rating | Driven by |
|---|---|
| **Prosperity** | Housing levels, treasury, trade income, employment, wages |
| **Culture** | Coverage by temples, schools, entertainment and festivals; finished monuments |
| **Security** | Defence coverage, attacks repelled, low crime and unrest |
| **Legitimacy** | Requests fulfilled, monuments, favour, fair taxes, disasters answered. Each realm has its own name for it: the **Mandate** in Đại Long, **Divine Favour** in Kambara, **Merit** in Tampadesa |

## 6. Realm layers

Each realm reskins the shared buildings and adds its own systems. The shared core teaches the game in Campaign I, and each later campaign brings a different twist.

| | Đại Long | Kambara | Tampadesa |
|---|---|---|---|
| **Water** | **Dykes and sluice gates:** the river floods every wet season. Dykes protect, sluices let water into the paddies, and a breached dyke floods the district | **Barays and canals:** reservoirs fill in the wet season and canals feed fields and moats in the dry. A bigger network means more harvests. The Breathing Lake grows and shrinks each year | **Weirs and canals:** a dry plain watered from the river and the hill streams. Drought is the main threat, and well-placed canals the main answer |
| **Society** | **Examinations:** an academy turns educated youths into scholar-officials who run the tax offices and courts and live in elite housing | **Temple estates:** temples own farms and workers, and the city's labour runs through them | **The monastic order:** monasteries teach, are given land and goods, and pay no tax |
| **Legitimacy** | **The Mandate:** virtue, fair taxes, flood control and loyalty | **Divine Favour:** temples, monuments and the cosmic layout of the city | **Merit:** temples, donations and gifts to monasteries. Balancing merit against the treasury is Tampadesa's central tension |
| **Signature buildings** | Village communal hall, Temple of Learning, water-puppet pavilion, examination hall | Stone barge quay, rest house, royal hospital, temple-mountain, dance hall | Brick kiln works, lacquer workshop, monastery, merit pagoda (small temples the elite build themselves), festival ground |
| **Signature goods** | Silk, celadon ceramics, salt, bronze, lacquer, tea | Sandstone, laterite, palm sugar, spices, iron, elephants | Bricks, lacquerware, cotton cloth, sesame oil, teak, gems |
| **Defence flavour** | Earthen ramparts, citadel walls, river stakes | Moats, laterite walls, face-carved gate towers, war elephants | Brick walls, moated city, horse archers |
| **Monuments** | Temple of Learning, Imperial Citadel of Thăng Hà | Suryagiri (temple-mountain), the walled Great City, the Temple of Four Faces | The Gilded Stupa, the Temple of Infinite Wisdom, the Plain of Ten Thousand Temples |

## 7. Missions and win conditions

Each mission draws its goals from three families:

1. **Metrics:** reach targets such as population, ratings, treasury, trade income, house levels, stockpiles, requests fulfilled or floods survived.
2. **Heritage:** finish a monument, or certain stages of it.
3. **Survival:** survive N attack waves, or hold out until a given year, without losing the palace or citadel.

Many missions mix families. Planned goal types (the first three exist today):

| Goal | Example |
|---|---|
| `Population { min }` | 1,500 citizens |
| `Employment { min_employed }` | 600 people employed |
| `Treasury { min_gold }` | 5,000 gold |
| `Rating { kind, min }` | Culture ≥ 40 |
| `HousesAtLevel { level, count }` | 30 houses at level 4 or above |
| `TradeIncome { per_year }` | 2,000 gold a year from exports |
| `Stockpile { good, amount }` | 400 bricks in storage |
| `RequestsFulfilled { count }` | 5 court requests met |
| `SeasonsSurvived { kind, count }` | 3 floods without a breached dyke |
| `MonumentComplete { id, stage }` | Suryagiri, every stage |
| `SurviveAttacks { waves }` | 4 waves repelled |
| `SurviveUntil { year }` | Hold out until the end of WY 125 |

**Failure:** the palace or citadel is destroyed in a survival mission; the city stays in debt too long; Legitimacy stays near zero for a year (the court recalls the governor); or a time limit runs out on the few timed missions.

**Difficulty:** five levels, as in Pharaoh. They scale costs, request sizes, attack strength and how fast favour fades.

**Optional distinctions:** each mission has one or two optional goals for replay, for example finishing with Culture ≥ 60 or never breaching a dyke. These earn extra lines on the stele.

## 8. Goods

| Group | Shared (all realms) | Realm-specific |
|---|---|---|
| Food | Rice*, fish*, meat* | Đại Long: vegetables, tea. Kambara: palm sugar, fruit. Tampadesa: sesame, beans |
| Raw materials | Wood*, clay*, metal ore → metal*, stone | Đại Long: mulberry, salt. Kambara: sandstone, laterite. Tampadesa: cotton, teak, gems |
| Crafted goods | Pottery*, bricks*, wine/spirits*, weapons | Đại Long: silk, celadon, bronze, lacquer. Kambara: iron tools, carvings. Tampadesa: lacquerware, cloth |
| Luxuries (elite housing, trade) | | Đại Long: silk, celadon, tea. Kambara: spices, carvings. Tampadesa: lacquerware, gems |
| Monument materials | Stone, timber, bricks | Gold leaf, lacquer, sandstone, laterite |

\* already in the prototype (`ResourceKind`: rice, meat, fish, wine, pottery, wood, metal, clay, bricks, gold).

## 9. Art direction

- **Style:** detailed, hand-painted 2D isometric pixel art, warm and readable at full zoom, in the line of Emperor and Pharaoh. The April 2026 prototype (`images/prototype_apr_2026_*.jpg`) and the civilisation references (`images/references/`) set the target.
- **One style, three materials:** all three realms share the camera, lighting, proportions and level of detail. They differ in their material palette:
  - **Đại Long:** dark timber, terracotta tile, ochre plaster, red lanterns, green paddies, karst grey.
  - **Kambara:** grey sandstone, red laterite, thatch, deep jungle green, blue moats.
  - **Tampadesa:** red brick, gold leaf, whitewash, dusty ochre plains, toddy palms.
- **Monuments** are the art showpieces, with visible construction stages.
- **People and walkers** wear realm-specific dress and are readable by role, through colour and what they carry.
- **Story comics** use flat colour with ink outlines, inspired by each realm's folk art: woodblock prints for Đại Long, temple reliefs for Kambara, temple murals for Tampadesa.

## 10. UI and UX

- Scroll and lacquer panels, with frame trims per realm: red and gold lacquer for Đại Long, carved sandstone for Kambara, gilt on red for Tampadesa. The prototype's scroll panels and build bar stay.
- A minimal HUD: speed controls, treasury, population, date and season, a goals tracker and a message log.
- **Overlays:** water and irrigation, each service, risks, desirability, defence coverage and floods.
- **Advisors:** one screen per area (labour, military, food, trade, culture, finance, ratings, the court), as in Pharaoh.
- **Accessibility:** UI scale, colour-blind-safe overlays, rebindable keys, adjustable game speed.
- **Fonts** must cover Vietnamese diacritics.

## 11. Audio

- Music per realm, built around its traditional instruments: đàn bầu, đàn tranh and court ensembles for Đại Long; gong circles and xylophone ensembles for Kambara; drum and gong circles and harp for Tampadesa. Calm building themes, rising cues for festivals and monuments, and tension for attacks.
- **Ambient sound** follows the game state: rain in the wet season, market bustle, temple bells, the river. Today's placeholder ambient system becomes driven by the game state.
- **Feedback sounds:** placing, upgrading, goal reached, a warning, a monument stage done.

## 12. Technical overview

The engine and its architecture are described in the repository's `AGENTS.md`. In short:
- **Rust workspace** with `engine` (platform, rendering, egui UI, sound, saves) and `game` (simulation, world, tiles, units, buildings, menus).
- **Deterministic simulation** with a fixed tick and a seeded RNG. Saves are serialised JSON of the whole `GameSession`, with versioning and migration scripts.
- **Content is data:** buildings, units, props, campaigns and UI styles are JSON under `assets/configs/`. New realms should be added as data wherever possible: a realm config that names its tile sets, building variants, goods, rating names, patrons and UI style.
- **Automation:** script mode, a live control server, MCP tools, and record and replay, all available for testing missions and balance.

Implementation plans for each system will live in `docs/plans/`.

## 13. Scope

**In scope:** three realms; 30 campaign missions; Custom Game; the systems in sections 5–8; automatic defence; world-map trade and diplomacy; staged monuments.

**Out of scope for this version:**
- player-controlled armies and navies;
- RTS combat;
- conquest of other cities;
- multiplayer;
- a playable fourth realm;
- a procedural world map;
- a scenario editor and mods (planned for later).

## 14. Open questions

- **Housing levels:** are 6 common and 4 elite the right number for each track?
- **Service coverage:** should coverage switch to walkers actually visiting houses, the true Impressions model, or stay distance-based?
- **Monument sites:** should favourable sites, Emperor's Feng Shui, be a real system, or only flavour for monument placement?
- **Defence economy:** do militia need a supply of weapons, or only staff?
