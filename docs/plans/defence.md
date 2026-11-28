# Defence and attacks

**Status (2026-11-28): in progress. Phase 0 (the data, the state and save v20) done.** This plan adds the city's defences and the attacks they answer: walls drawn like roads with gatehouses through them, towers that shoot, a watchtower that warns early, a garrison or fortress whose militia sally out, weapons from a workshop; and raiders who come from the map's edge after a warning, break through, loot and burn, kill the city's walkers and militia, and leave or die. It is the first milestone of the roadmap's Phase 2 (D, [roadmap](roadmap.md#phase-2-outline)), and covers GDD 5.16 for Campaign I's missions 6 and 8; mission 9's moats, river stakes and naval raiders are its last phase, with wooden bridges over narrow water. Its decisions' defaults were accepted on 2026-11-28, with the user's own on civilians (Q14 to Q16), bridges (raiders never target them) and ships under bridges (Q13).

## How to resume this work

- Work through the [phases](#phases) in order. Tick the checkboxes and add dated notes as tasks land.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise.
- Follow [AGENTS.md](../../AGENTS.md): keep both targets compiling, run `cargo test --workspace` and the scenarios, keep the simulation deterministic (the simulation's RNG only), and bump the save version with a migration script when the saved layout changes.

## Goals

1. Walls block attackers. The player draws them with the road tool in three tiers (palisade, rampart, wall), and a gatehouse lets the city's own walkers through where a road crosses one.
2. Attacks come with warning: a message and event popup name the enemy, the direction, the rough size and the months until they arrive (GDD 5.16), earlier with a watchtower.
3. Attackers act on their own: they come from the map's edge, go for loot (stores, granaries, the treasury through a tax office), the palace and houses, break walls and gates in their way, attack the city's walkers and militia they meet, set fires, and leave once they've looted or lost too many.
4. Defence is automatic: towers and militia archers shoot arrows at attackers in range (further from high ground), and a garrison or fortress sends its militia to fight attackers within its radius and brings them back.
5. Attacks cost people: a walker or militiaman killed is a resident lost, and his building loses a worker; a house a raider destroys loses all its residents.
6. Attacks matter: damage through M6's fires and collapses, goods and gold lost, the Security rating's defence drivers and Legitimacy, a `SurviveAttacks` goal, and a mission lost when its palace falls (in the missions that say so).
7. Wooden bridges carry roads over canals and narrow rivers, as in Pharaoh and Caesar (with the water defences, Phase 6).
8. Every unit, building and projectile has Blender art: the wall pieces, gatehouses, towers, the watchtower, garrison, fortress, weapons workshop and bridges, the raiders and militia in all eight directions ([unit-movement.md](unit-movement.md) Q5), and the arrow.

## What exists today

- **Barriers:** the dyke (`crate::floods`, [tile/road.rs](../../crates/game/src/tile/road.rs)) is a terrain tile drawn with the road tool (`RoadKind::Dyke`), its 16 pieces joining only other dykes (`road::dyke_junction_mask`), a road along its crest; `CityFloods` keeps each dyke cell's wear (`DykeWear`), a builders' guild walker standing on it resets it, and a breach clears the tile and lays dirt and rubble. Walls can follow this model: the road tool, junction pieces from Blender, per-cell condition kept in the simulation's state, repair by builders, a breach to rubble.
- **Buildings:** `BuildingKind` is a `u32` of flags with 27 of 32 bits used ([building/mod.rs](../../crates/game/src/building/mod.rs)); a `Citadel` and a `GovernorPalace` exist as services with no defence role. The `PoliceStation` sends a `Guard` patrol; Security reads its coverage. A tile def's `requires_dyke` (the sluice gate) is the model for a gatehouse that needs a wall.
- **Walkers:** units have no health and nothing fights. Off-road searches are eight-way ([unit-movement.md](unit-movement.md)), and new units get Blender art in every direction (`hbgen/recipes/walkers.py`: `person` with a spear or bag, `quadruped`). Paths follow heights (terrain height Stage 5), so attackers climb by ramps.
- **Damage:** M6's `Building::ignite_now`, `destroy_building` and collapse risk (`add_risk`); the risks' notices and minimap alerts.
- **Warnings and messages:** M11's typed messages with places and event popups (`messages::popups`); the failure warnings' model (`failure::notice_message`).
- **Ratings:** Security has `police` (coverage), `safety`, `calm_streets` and `disasters` drivers ([ratings/dailong.json](../../assets/configs/game/ratings/dailong.json)); a failure is a `FailureReason` (debt, Legitimacy, time); goals are `MissionGoal`s with no `SurviveAttacks`.
- **Custom games:** the dialog's "Raids and invasions" switch is stored (`CustomGameSettings::raids`) but disabled, waiting for this.
- **Pathfinding:** `NodeKind` flags (`EmptyLand`, `Road`, `Water`, `Building`, ... `Field`) decide what a search walks; a wall or a gate needs kinds of its own.
- **Saves:** version 19.

## Design

### 1. Walls and gatehouses

- **Walls** are terrain tiles drawn as segments with the road tool, like the dyke: `RoadKind::Palisade`, `Rampart` and `Wall` (wood, earth, then brick or stone: GDD 5.16's three tiers; Đại Long's earthen ramparts and citadel walls), each a tile def with 16 junction pieces from Blender, joining walls of any tier (a tier's pieces meet another's). Their path kind is a new `NodeKind::Wall`: no one walks it, the city's walkers path round, attackers bash through (§3). Flat ground only, as dykes; cliffs are walls already.
- **Condition:** `CityDefence` keeps each wall cell's condition (0 to its tier's strength), synced with the map when walls are drawn or cleared, as `CityFloods` keeps dyke wear. Attackers lower it; at 0 the cell breaches: the tile goes, rubble is laid (`rubble::place_pile`), and a message says where. A builders' guild walker standing on a damaged wall cell repairs it over time (Q4). A damaged cell is drawn with a cracked overlay, and the Defence lens colours walls by condition.
- **Gatehouse:** a building placed on a road cell in a wall line (`requires_wall`: walls on both sides along the line, as `requires_dyke`), its variation the line's axis, a tier per wall tier (Q3). Its node kind is `Road | Gate`: the city's walkers pass, attackers treat it as a wall with more strength, and it closes (blocks all) while attackers are within a few cells (Q5).

### 2. Raids: warning, arrival and the raid's end

- **Data:** a mission's `defence` block in `campaigns.json` lists its `raids` (id, month after the start, `from`: a map-edge cell or a named entry, enemy `kind`, `size`, `warning_months`), checked as missions load; I-6 has two (Vijara pirates), I-8 four waves (Kambara raiders). A custom game with Raids on gets raids drawn from the simulation's RNG every few years, sized by the city (Q9). `DefenceRules` (`GameConfigs::sim.defence`) holds the knobs.
- **State:** `sim::defence::CityDefence` (saved in `Simulation`, save v20): the raids scheduled and under way (their units), the log (each raid's outcome: losses on both sides, goods and gold taken, buildings damaged), and the walls' condition.
- **Warning:** `warning_months` before arrival (more with a working watchtower facing the raid's side: §4), a Warning message and event popup: the enemy in the realm's words, the side of the map it comes from, its rough size ("a band of about 20"), the months left. Again a month before. The minimap pings the edge it comes from.
- **Arrival:** each raider spawns at the raid's entry over a few seconds, with `UnitTaskRaid` (§3).
- **End:** the raid ends when every raider has left the map or fallen. Its outcome is logged and posted (a message, and an event popup for a city that lost its palace or a lot), Security's and Legitimacy's drivers read the log (§6), and a repelled raid (no building lost, no goods taken) counts for `RaidsRepelled`.

### 3. Attackers

- **Units:** `UnitConfigKey::Raider` (a pirate in I-6, a Kambara soldier in I-8: art by realm and kind, Q11), eight-way over open land (`EmptyLand | Road`, never `Wall` or `Gate`).
- **Health:** every unit gains `health` (saved), full on spawn by its config (`max_health`); raiders, militia and the city's walkers can lose it (not the bird, ships or animals). At 0 a unit falls: a brief fallen sprite, then it despawns (§3a).
- **`UnitTaskRaid`:** pick a target by the raid's kind (Q6: raiders go for the nearest store, granary or tax office holding something, then houses; war parties also the palace), path to it; with no path (walls in the way) path again letting `Wall` and `Gate` cells cost by their condition (`find_path` with a weighted kind), walk to the first wall or gate cell on that path and strike it until it breaches; at the target, take a load (goods from a store, gold from the treasury through a tax office: finance.md's plunder) and set it alight with a chance (`ignite_now`), then pick the next target, or retreat to its entry once it carries enough or the raid has lost half its number (Q7). On its way it attacks any of the city's units within reach (§3a): militia (who fight back, §5) and civilian walkers (who don't).

### 3a. The city's walkers under attack, and what a death costs

- **Civilians as targets:** a raider attacks any of the city's walkers that comes within reach (Q14: two cells), any kind (runners, patrols, settlers, traders' caravans, dockers, responders). A walker under attack **stops where it is** (its navigation paused, its task waiting) until the raider falls or moves on, or it falls: no fleeing and no chasing (Q15), so a raider strikes a walker it has reached until one of them is gone, then goes back to its own task.
- **A walker or militiaman killed is a resident lost:** the city's population drops by one. The resident comes from the dead walker's household: the house that supplies his building's workers (`Employer`'s households, the nearest first), else the house he came from (a settler's population never arrives: the settler's whole group is lost), else the largest house (Q16).
- **And his building loses a worker:** if the walker came from a building with workers (his task's origin, `Employer`), that building loses one from its staff at once (`labour`'s allocation then hands out the workers left at its next pass, from a smaller workforce).
- **A house a raider destroys loses all its residents:** a house collapsed or burnt down by a raider's strikes or fire is a `destroy_building` (M6), and its residents are lost with it, counted as deaths in the raid's log and M9's disasters (`CityRisks::record_disaster`).
- **Notices:** a walker's or militiaman's death is a log line (one message a raid, its count rising, not one per death), and the raid's end message sums the dead.
- **Pathing:** walls and gates block the ordinary searches; the weighted search for a way through is new (a cost per kind on top of `Connectivity`), and terrain height already funnels attackers through ramps.

### 4. Towers and the watchtower

- **Tower:** a 1×1 building on or beside a wall (Q8), staffed (2 to 4 workers), with a `range` in cells, plus one per level of height above the ground around (terrain height's defence hook). Every `fire_secs` it shoots the nearest raider in range: an arrow (§4a). Weapons: Q2.
- **Watchtower:** a 1×1 staffed building with no weapons. A working one adds `watch_months` to the warning of raids from its side of the map, and shows the raid's approach on the minimap (the raiders' route, drawn once they're on the map).

### 4a. Projectiles

Arrows from towers and militia archers are new game logic and new art, not units:
- **State:** `CityDefence` keeps the arrows in flight (saved, so a save mid-volley loads the same): each its origin and target unit, start and land points, flight time (`arrow_speed`), damage and who fired it.
- **Flight:** an arrow flies on a short arc from its archer to where its target stood when it was loosed, advanced with the simulation's clock. On landing it hits the target if the target is still within `hit_radius` of that point (a walker that moved on is missed, Q17), for its damage; otherwise it falls and is gone.
- **Drawing:** the renderer draws each arrow in flight at its point on the arc, lifted by the arc's height and the ground's, sorted with the objects at its nearest cell, its sprite picked by its screen direction (eight rendered, or one rotated: Q18). No unit, tile or path search.
- **Art:** the arrow from Blender (`catalog/defence.json`), and an archer's draw-and-loose frames for the tower's look and the militia archer's.

### 5. Garrison, fortress and militia

- **Garrison** (I-6) and **fortress** (I-8): service buildings (2×2 and 3×3) that train militia from their staff, up to a number each, one every `train_secs`, each needing a weapon from their stock (Q2). Militia (`UnitConfigKey::Militia`: spearmen; archers come with the fortress) stay inside until a raider comes within the building's `defence_radius`; then each walks out (`UnitTaskDefend`, eight-way over land and roads) to the nearest raider, fights it (alternating strikes, `damage` and `strike_secs` from their configs, the simulation's RNG for who strikes first only), goes on to the next within the radius, and walks back when none is left. A militiaman who falls is trained again.
- **Archers:** the fortress also trains archers, who stand off at `range` and shoot arrows (§4a) at the nearest raider instead of closing in.
- **Combat** (Q1): no formations, no morale; a fight is two units within one cell striking in turn until one falls or leaves. Towers' and archers' arrows and militia damage raiders; raiders damage militia, the city's walkers (who don't strike back, §3a), walls, gates and buildings (a building takes collapse risk, so M6 collapses it at 100). A militiaman who falls is a resident lost and his garrison's worker (§3a).

### 6. Weapons, ratings, goals, failure and the UI

- **Weapons:** a new good, `Weapons` (a raw material: stored in the storage yards), made by the weapons workshop (a producer, `Factory`) from metal and wood, and delivered to garrisons, fortresses and towers by the existing runners (Q2).
- **Ratings:** Security's `defence_coverage` (residents within a garrison's or fortress's radius or a tower's range) and `attacks` (raids repelled lift it, buildings and goods lost lower it, fading as events do: `ratings::fade`); Legitimacy's `attacks` for the Mandate (a palace lost, a city looted). Per realm in `assets/configs/game/ratings/`.
- **Goals:** `MissionGoal::SurviveAttacks { min }` (raids ended without the palace lost) and `RaidsRepelled { min }`; the debrief and goals panel read them as the others.
- **Failure:** `FailureReason::CityFallen` when the palace (or citadel) is destroyed, in missions whose `failure` block sets `city_falls` (Q10).
- **UI:** the Defence lens on the minimap (walls by condition, towers' ranges, garrisons' radii, watchtowers' sides, raiders on the map); the inspector's panels for walls (condition, last repaired), gatehouses, towers (staff, weapons, range), garrisons (militia trained and out, weapons) and raiders and militia (health, task); the build palette's Defence sector; the City book's Ratings page shows the new drivers. Messages and popups as §2.
- **Automation:** `defence.state` (raids scheduled, under way and logged; walls; towers; garrisons; units' health), `defence.set` (a wall's condition, a raid's month), `defence.raid_now [raid=...] [from=...] [size=...]`, `defence.end_raid`; `world.building_at` and `inspect.at` report the new parts.

### 7. Water defences and bridges (mission 9)

- **Moats:** water dug into the map with the canal tool (`RoadKind::Moat`: a canal that blocks attackers on land and can carry water, GDD 5.16).
- **River stakes:** placed in rivers, damaging and blocking enemy boats (Đại Long).
- **Naval raiders:** ships that sail in as trade ships do (`ships::find_water_path`) and land their raiders at the shore.
- **Wooden bridges** (as in Pharaoh and Caesar): drawn with the road tool across narrow water, a canal or a river up to `max_bridge_cells` (Q19: 4) wide, from land to land in a straight line along a grid axis; a terrain tile on the water cells (`RoadKind::Bridge`), its pieces by axis and part (each end, the middle), with path kind `Road`, so walkers, caravans and raiders cross it, and the water under it stays water for the irrigation network and the flood. Trade ships can't pass under a wooden bridge (Q13), so a bridge that would cut every working dock off from the map's edge is refused, with a clear reason. Raiders cross bridges as roads and never target or break them: a broken bridge could cut them off from the city. This also answers the known issue that nothing crosses a canal (seasons-floods Q27).

### 8. Art (Blender)

Walls in three tiers × 16 junction pieces (`catalog/defence.json`, as the dyke's), a cracked overlay; gatehouses per tier × 2 axes; tower, watchtower, garrison, fortress, weapons workshop; raiders (pirates, Kambara soldiers) and militia (spearmen, archers) with walk, idle, strike, shoot (archers) and fall animations in all eight directions (`person` with new gear), and a fall frame for the city's hand-made walkers (or the fallen sprite alone, Q20); the arrow (§4a); wooden bridges' pieces (§7); Đại Long's look first (Q11). The weapons good's icon and the palette's Defence sector glyph come from ui-art and ui-icons.

## Phases

### Phase 0: Data, state and saves (medium)

- [x] `BuildingKind` from `u32` to `u64` (it's saved by flag name; check the saves and every `as u32`), and the new kinds: `Gatehouse`, `Tower`, `Watchtower`, `Garrison`, `Fortress` (the weapons workshop is a `Factory`).
- [x] `NodeKind::Wall` and `Gate`; `RoadKind::Palisade`, `Rampart`, `Wall` (no art yet: placeholder pieces). *Deviation: the road kinds move to Phase 1, below.*
- [x] `sim::defence::CityDefence` and `DefenceRules`; a unit's `health` (`max_health` in the unit configs); save v20 with its migration script, the sample saves migrated, `--smoke-test-saves`.
- [x] A mission's `defence` block and its checks; Custom Game's Raids switch enabled. *Deviation: wired, but the toggle stays disabled until Phase 2.*
- [x] `defence.state` and `defence.set`; tests in a new `tests/defence.rs`.

**Notes (2026-11-28): done.**
- **Building kinds** ([building/mod.rs](../../crates/game/src/building/mod.rs)): `BuildingKind` is a `u64`, with `Gatehouse`, `Tower`, `Watchtower`, `Garrison` and `Fortress` (bits 27 to 31) among the services and `BuildingKind::defences()`; none has a config, tile or behaviour yet (their phases add them). Buildings save their kind by flag name, so saves need nothing for it. A tile's `TileGameObjectHandle` packed a building's kind bits in a `u32`; it holds a `u64` now, its invalid value still `u32::MAX` (a kind is a single bit and a generation stays below it), so an older save's numbers, invalid handles included, read as they did. The risks' jitter hashes the same bits (`risks.rs`, no `u64::from` any more), so no building's jitter moves.
- **Path kinds** ([pathfind/mod.rs](../../crates/game/src/pathfind/mod.rs)): `NodeKind::Wall` and `Gate` (a `u16`, 14 of 16 bits used), with debug colours; nothing sets them yet.
- **Deviation: the walls' `RoadKind`s go with Phase 1,** where they get their tile defs and Blender pieces; placeholder tiles now would only be replaced.
- **State** ([sim/defence.rs](../../crates/game/src/sim/defence.rs)): `CityDefence` in `Simulation` (`defence()`, `defence_mut()`, on `SimContext` and `SimContextMut` too): a custom game's random raids and the next one's month, the raids scheduled (`ScheduledRaid`: id, `RaidKind` (raiders, war party), `RaidFrom` (a `MapSide` or a cell), size, enemy key, arrival and first warning as ordinal months, warnings sent), those under way (`RaidUnderWay`), the log (`RaidRecord`, with `RaidLosses`: walkers and militia killed, residents lost, buildings and houses destroyed, goods and gold taken; `repelled`, `palace_lost`), `survived()` and `repelled()` for the goals, and the walls' `WallCondition`; the last three wait for their phases. `start` schedules a mission's raids from its start (`GameSession::start_mission`, `start_defence`) or a custom game's random ones (`start_custom_game`, its `raids` setting); only a game with random raids draws from the simulation's RNG, so no other game's draws move. `DefenceRules` (`GameConfigs::sim.defence`): the default warning months and random raids' years and sizes (`random_raid_size`).
- **Units' health** ([unit/mod.rs](../../crates/game/src/unit/mod.rs)): `Unit::health` (saved), full on spawn from the config's `max_health` (`DEFAULT_MAX_HEALTH`, 100, for every unit so far), `health()`, `max_health()`, `set_health()` (clamped).
- **Mission data** ([campaign/config.rs](../../crates/game/src/campaign/config.rs)): `MissionDef::defence`, a `MissionDefenceDef` of `MissionRaidDef`s (id, month, from: `"north"` ... or `[x, y]`, kind, size, warning_months, enemy), checked by `CampaignConfigs::validate` (ids `[a-z0-9_]` and unique, month and size 1 or more, no warning before the start, no negative cell); `campaign::active_mission_defence`. No mission has one yet (M13 writes I-6's and I-8's).
- **Deviation: Custom Game's Raids toggle stays disabled** until Phase 2 brings the raids it promises; its setting already reaches `CityDefence` (a custom game started by automation with `raids` on gets random raids scheduled).
- **Save v20** ([v19_to_v20.py](../../crates/tools/save-migration-scripts/v19_to_v20.py)): `sim.defence` (empty) and every unit's `health` (100); the local saves and `saves/sample_saves.zip` migrated; `--smoke-test-saves` loads all 13.
- **Automation** ([commands/defence.rs](../../crates/game/src/automation/commands/defence.rs)): `defence.state` (now, the random raids and the next, the raids scheduled with `months_left`, under way and logged, `survived`, `repelled`, the walls' count) and `defence.set` (`random_raids`; `raid` and `months` to move a raid; `unit` and `health`); `world.inspect` shows a unit's `health`.
- **Tests** (`tests/defence.rs`, 12): the block's parsing and its problems, scheduling, random raids within their years and deterministic, a mission drawing nothing from the RNG, moving a raid, random raids' sizes, the saved shape against the migration's, units' health, the kinds, handles with wide kinds and an old save's invalid one, the shipped campaigns still validating.
- **Verified:** `cargo test --workspace` (with `defence`), both clippy runs, the 33 CI scenarios on a release build, `--smoke-test-saves` on the migrated saves, and `defence.state` and `defence.set` in the running game. The saved layout grew (an empty `defence` and units' `health`), so `determinism.jsonl` (`--seed 7 --window-size 1280x720`) prints a new checksum, `4866211534a7e2f1` (it was `81a7523ee7abbe68`), the same on two runs with byte-identical saves; the `ui_build` and `determinism` runs, recorded afresh, replay to the same state.

### Phase 1: Walls and gatehouses (medium)

- [ ] `RoadKind::Palisade`, `Rampart` and `Wall` and their tile defs (moved from Phase 0).
- [ ] Wall segments with the road tool (the Defence sector), their junctions, flat ground only, the walls' condition synced; the gatehouse (`requires_wall`, its axis variation), its `Road | Gate` node, closing near raiders.
- [ ] Breaches to rubble and their message; builders repairing walls; the cracked overlay.
- [ ] Blender art: the three tiers' pieces, the gatehouses, the overlay.
- [ ] Tests: walls block searches; the city's walkers pass a gatehouse; a breached cell opens; builders repair. A scenario, `defence.jsonl`, begins.

### Phase 2: Raids and raiders (large)

- [ ] The raid schedule, warnings (message, event popup, minimap ping) and arrival; the weighted search through walls; `UnitTaskRaid`: targets, striking walls and gates, looting goods and gold, fires, retreat; falling and despawning; the raid's end, log and messages.
- [ ] Civilians under attack (§3a): raiders strike the city's walkers within reach; a walker under attack stops in place; a death costs a resident (from his household) and his building a worker; a house a raider destroys loses its residents; the deaths in the log, M9's disasters and the messages.
- [ ] Blender art: the raider (Vijara pirate) in eight directions with strike and fall; a fallen sprite for the city's walkers.
- [ ] Tests: a raid's warning months and arrival; raiders path round walls, break the cheapest way through, loot a store and leave; a runner attacked stops, falls, his house loses a resident and his workshop a worker; a house destroyed loses its residents; the log. `defence.jsonl`: a raid on a walled city.

### Phase 3: Towers and the watchtower (medium)

- [ ] Projectiles (§4a): arrows in flight in `CityDefence` (saved), their arcs, hits and misses, drawn by the renderer in the objects pass.
- [ ] The tower: staffing, range with the height bonus, shooting arrows; the watchtower's earlier warning and the approach on the minimap.
- [ ] Blender art: tower, watchtower, the arrow.
- [ ] Tests: a tower shoots the nearest raider in range, further from high ground; an arrow hits a raider that stands and misses one that moved on; arrows survive a save and load; a watchtower lengthens the warning.

### Phase 4: Garrison, fortress, militia and weapons (large)

- [ ] `Weapons` and the weapons workshop; weapons delivered to garrisons, fortresses and towers (Q2).
- [ ] Garrison and fortress: training militia (spearmen; archers at the fortress); `UnitTaskDefend`; combat; archers shooting arrows; militia falling (a resident and a worker lost) and being trained again.
- [ ] Blender art: garrison, fortress, weapons workshop, militia (spearmen, archers with their draw-and-loose frames).
- [ ] Tests: militia sally out to raiders within the radius, fight, return; archers shoot from range; a raid repelled by militia and towers.

### Phase 5: Ratings, goals, failure and the UI (medium)

- [ ] Security's and the Mandate's drivers; `SurviveAttacks` and `RaidsRepelled`; `FailureReason::CityFallen`.
- [ ] The Defence lens; the inspector's panels; the palette's Defence sector; the hidden test campaign's defence mission (as `test_goals`); the docs (AGENTS.md, known-issues).
- [ ] Tests and scenarios: the goals, the failure, a custom game's random raids.

### Phase 6: Water defences and bridges (medium to large; with mission 9)

- [ ] Wooden bridges (§7): drawn with the road tool over canals and narrow rivers, land to land along an axis, up to `max_bridge_cells`; their pieces; a `Road` path over water that keeps the water for irrigation and the flood; refused where it would cut every dock off from the sea, with its reason (Q13); raiders cross them and never target them.
- [ ] Moats with the canal tool; river stakes; naval raiders landing from boats.
- [ ] Blender art: the bridge's pieces (each axis: ends and middle), river stakes, the raiders' boat.
- [ ] Tests and scenario: walkers, caravans and raiders cross a bridge; a canal network keeps its water under one; a bridge that would cut a dock off is refused with its reason (`world.road`'s error); a moat stops raiders on land.

## Risks

| Risk | Mitigation |
|---|---|
| Many raiders and militia searching paths every frame cost too much | Searches only when a target changes or a path is blocked; raids are tens of units; time it with `terrain.hills`-sized maps |
| Widening `BuildingKind` breaks saves or code that packs it | It's saved by flag name; Phase 0 checks every cast and runs `--smoke-test-saves` |
| The weighted search finds silly ways through (bashing a wall beside an open gate) | Gates cost more than open ground but less than walls, and an open gate is a road; tests cover it |
| Combat looks like units standing in a heap | Strike animations and a fall; units in a fight face each other; keep raids small at first |
| Raids make the early missions too hard | Only missions with a `defence` block have raids (I-6 on), and M13 balances them |
| Determinism breaks with fights and targets | Every choice from the simulation's RNG or a settled order; `determinism.jsonl` gains a raid |
| The art for eight directions with strike and fall is a lot | The walker recipes exist; a fall is one frame; strike reuses the walk's arms |
| Walkers stopping in place under attack leave tasks stuck | A stopped walker's task resumes when the raider falls or moves on; a fallen one's task ends as a despawned unit's does (its errand's goods lost, as a delivery that fails to spawn) |
| Losing residents and workers by deaths leaves houses and employers inconsistent | Through the existing paths: a house's people leaving (`HouseBuilding`'s population) and `labour`'s next allocation; tests check the counts |
| Arrows in flight make saves or determinism fragile | Saved in `CityDefence`, advanced with the simulation's clock, no RNG in a flight; `determinism.jsonl` gains a volley |
| Bridges cut trade ships off from docks | Q13: refused, with its reason, where a bridge would block every dock's way to the map's edge |
| The player isn't told why a bridge was refused | The game shows no placement errors yet (a refused placement plays the failure sound; its message reaches automation only): the bridge's reason is written for the player, and showing it waits for placement errors to be displayed (`TODO(defence)` at the road tool's failure) |

## Open decisions

**Q1. How detailed is combat?**
*Default: simple.* Units have health; a fight is two units within a cell striking in turn at their configs' damage and interval; no formations, morale or flanking (GDD 5.16's out-of-scope list).

**Q2. Do militia and towers need weapons? (GDD 14's open question)**
*Default: militia need one weapon each to be trained; towers need staff only.* The weapons workshop then matters for the garrison and fortress, and a tower works once it's built and staffed.

**Q3. Do gatehouses come in tiers?**
*Default: yes, one per wall tier,* placed on its tier's wall (a palisade gate, an earthen gate, a stone gatehouse), its strength the tier's.

**Q4. Who repairs walls?**
*Default: the builders' guild's walkers,* standing on a damaged cell as they reset a dyke's wear; no repair during a raid.

**Q5. When does a gatehouse close?**
*Default: while any raider is within 6 cells of it;* the city's walkers wait or path round meanwhile.

**Q6. What do raiders go for?**
*Default: by the raid's kind.* Raiders: the nearest store, granary or tax office holding anything, then houses; war parties also the palace; siege forces (later) the walls first.

**Q7. When do raiders leave?**
*Default: once each carries a full load, or the raid has lost half its number,* walking back to their entry; any still on the map after `raid_secs` leave too.

**Q8. Where do towers stand?**
*Default: on a wall cell (replacing it, joining the wall's pieces) or beside a wall,* never on their own away from walls.

**Q9. Do custom games get raids?**
*Default: yes, with the Raids switch on:* a raid every 2 to 4 years from the RNG, its size from the population, with the usual warning.

**Q10. Can losing the palace lose the mission?**
*Default: only in missions whose `failure` block sets `city_falls`* (GDD 5.16: survival missions).

**Q11. Which realms' art?**
*Default: Đại Long's,* used in every realm until Campaigns II and III bring their own (Kambara's laterite walls and war elephants, Tampadesa's brick walls and horse archers: GDD 6).

**Q12. Do gifts or tribute avert attacks?**
*Default: not in this plan:* they belong with diplomacy (GDD 5.12, 5.14), a later Phase 2 plan.

**Q13. Can trade ships pass under a wooden bridge?**
**Decided (2026-11-28): no.** A low wooden bridge blocks boats (as Pharaoh's and Caesar's low bridges do), so placing one that would cut every working dock off from the map's edge is refused with a clear reason for the player: a `RoadSegmentError` of its own ("Ships can't pass under a wooden bridge: this one would cut the dock off from the sea."), the segment's preview marked invalid. The game doesn't show placement errors to the player yet (a refused placement plays the failure sound, and its message reaches automation only), so the reason reaches the player once placement errors are displayed (`TODO(defence)` at the road tool's failure, and a known issue). A raised ship bridge can come later.

**Q14. Which of the city's walkers do raiders attack?**
**Decided (2026-11-28): any of them,** civilian or militia, within reach (default 2 cells) of a raider on its way; buildings and walls as before.

**Q15. What does a walker do under attack?**
**Decided (2026-11-28): it stops in place** until the raider falls or moves on, or it falls; no fleeing, and raiders don't chase (to keep it simple first).

**Q16. Whose resident is lost when a walker or militiaman dies?**
**Decided (2026-11-28): one resident a death** (all of a house's when a raider destroys it), and a walker from a building with workers costs that building a worker. *Default for which house:* the one supplying the building's workers (nearest first), else the house he came from, else the largest; a settler's whole group is lost.

**Q17. Can an arrow miss?**
*Default: only a target that moved,* by flight time: it hits if its target is still within `hit_radius` of where it aimed, so no RNG and no homing.

**Q18. How is the arrow drawn?**
*Default: eight rendered directions* (as a unit's), picked by its flight's screen direction; one sprite rotated by the renderer would need rotation in the sprite batches, which they don't have.

**Q19. How wide can a bridge be?**
*Default: up to 4 water cells,* land to land in a straight line along a grid axis.

**Q20. How do the city's hand-made walkers fall?**
*Default: one fallen sprite for all of them* (a generic body under a cloth, Blender), not a fall animation per hand-made walker; raiders and militia, made in Blender, fall in their own frames.
