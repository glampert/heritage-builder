# Heritage Builder — Project Overview

This file covers the game's place in the market and the shape of the project. For the game itself, see the [GDD](gdd.md).

## The game in brief

| | |
|---|---|
| **Title** | Heritage Builder (working title) |
| **Genre** | 2D isometric historical city builder, walker-based |
| **Setting** | A fictional world drawn from three civilisations of mainland Southeast Asia (see [world.md](world.md)) |
| **Content** | Three campaigns of 10 missions each (about 1 hour per mission), plus Custom Game |
| **Platforms** | macOS, Windows, Linux and web |
| **Engine** | Custom, in Rust |
| **Stage** | Playable prototype (April 2026 screenshots in `images/`), moving into full pre-production |

## What makes it different

1. **A setting the genre has barely touched.** The rice deltas, temple-mountains and plains of stupas of Southeast Asia are new ground for a city builder, which almost always goes to Egypt, Rome, Greece, China or medieval Europe.
2. **The classic Impressions formula, done properly.** Walkers, housing that evolves, staged monuments, the four ratings and campaigns built from many missions: the formula of Pharaoh, Zeus, Caesar III and Emperor, which few modern games follow closely.
3. **Three realms with their own systems.** Đại Long's flood dykes, Kambara's reservoirs and lake pulse, and Tampadesa's merit economy and drought change how each campaign plays, not only how it looks.
4. **A story about earning the right to rule.** Three governors, three questions, and one connected timeline in which the realms trade, quarrel and finally stand together.

## Target audience

### 1. The classic city-builder fan (primary)

- Aged 25–45. Played Pharaoh, Zeus, Caesar III or Emperor, and today plays Anno, Manor Lords, Farthest Frontier or Pharaoh: A New Era.
- Analytical and patient; enjoys optimising layouts and production chains.
- **What it means for the design:** deep and readable systems; statistics and advisor screens; overlays; campaigns with a variety of goals; difficulty settings; good save support.
- **Pitch:** *"The classic Impressions city builder, set in the river kingdoms of Southeast Asia."*

### 2. The creative, relaxed builder

- Aged 18–35. Plays for the look, the atmosphere and the joy of building something beautiful: Dorfromantik, Townscaper, Islanders.
- **What it means for the design:** Custom Game with failure, disasters and attacks turned off; beautiful monuments; calm music; a screenshot mode (screenshots already exist).
- **Pitch:** *"Build a city of temples, canals and rice terraces at your own pace."*

### 3. The indie enthusiast

- Aged 20–35. Follows indie developers and enjoys technical craft: Timberborn, Against the Storm, Factorio.
- **What it means for the design:** show the unusual setting and the custom Rust engine early; share development in devlogs; leave room for mods later.
- **Pitch:** *"A hand-built city sim in a custom Rust engine, with a world you haven't built in before."*

## Comparable games

| Group | Games |
|---|---|
| Direct references | Pharaoh (and Pharaoh: A New Era), Zeus: Master of Olympus, Caesar III, Emperor: Rise of the Middle Kingdom |
| Modern spiritual successors | Nebuchadnezzar, Builders of Egypt, Thriving City: Song, Augustus |
| Wider city builders | Anno 1800 and 117, Manor Lords, Farthest Frontier, Banished, Kingdoms and Castles, Against the Storm, Timberborn, Settlement Survival |

The closest neighbours in setting are Thriving City: Song (Song-dynasty China) and Emperor. Heritage Builder sets itself apart with Southeast Asia, three playable realms and automatic defence.

## Content budget (estimate)

| Content | Amount |
|---|---|
| Campaign maps | 30, plus 9 alternate maps for the map choices (missions 6–8 of each campaign), plus about 6–10 Custom Game maps |
| Realm art sets | 3 (terrain variants, about 40 shared buildings reskinned per realm, 6–10 unique buildings per realm, walkers) |
| Monuments | 8, each drawn in several stages |
| Comic sequences | 17, plus an epilogue (Campaign I: 5, Campaign II: 6, Campaign III: 6) |
| Music | About 3–4 tracks per realm, plus menu and tension cues |
| Text | Mission briefings, advisor messages, events, encyclopedia entries |

## Development phases

There are no dates. Each phase ends with something playable.

| Phase | Content | Playable result |
|---|---|---|
| **0. Prototype** (done) | Engine, OpenGL and Wgpu rendering, egui UI, saves, housing and services, production and storage, walkers, a campaign framework, automation and testing tools | A sandbox city and a 3-mission test campaign |
| **1. Core city sim** | Finance, city risks, seasons and floods, dykes, faith and festivals, ratings, the full goal set, advisor messages | Campaign I, missions 1–5 (the vertical slice) |
| **2. Advanced systems** | Trade and the world map, requests and diplomacy, elite housing, monuments and guilds, defence and attacks, story presentation (briefings, comics, steles) | All of Campaign I |
| **3. Kambara** | The realm layer as data; the Kambara art set; the water network (barays and canals); stilt housing; the lake pulse; temple estates; rest houses | Campaign II |
| **4. Tampadesa** | The Tampadesa art set; the merit economy; policies; two peoples; calling on allies; the epilogue | Campaign III and the full game |
| **5. Polish and release** | Balance, difficulty levels, Custom Game, localisation, accessibility, performance, store pages and trailer | Release candidate |

Implementation plans for each system will be written in `docs/plans/` as the phases begin. The release platforms and schedule are still to be decided.

## Business model

A premium, one-off purchase. The price and stores (itch.io, Steam, GOG) will be decided closer to release. A playable demo of Campaign I, missions 1–5, is a natural milestone for gathering feedback.
