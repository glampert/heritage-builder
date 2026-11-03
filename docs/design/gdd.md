# Heritage Builder — Game Design Document (GDD)

**Project Title:** Heritage Builder

## 1. Concept Overview

- **Genre:** 2D Isometric City Builder / Historical Management Simulation
- **Engine:** Custom Engine (Rust)
- **Platform:** PC (Windows, Linux, macOS)

**Core Idea:**

Heritage Builder is a deep historical city-building simulation inspired by Emperor: Rise of the Middle Kingdom, set in Vietnam between the 14th and 19th centuries.

Players design, build, and manage a thriving settlement shaped by the culture, architecture, and economy of ancient Vietnam - balancing agriculture, trade, and cultural harmony to sustain their dynasty’s legacy.

**Vision Statement:**

To immerse players in the beauty, complexity, and resilience of Vietnamese civilization through authentic architecture, cultural systems, and realistic city management.

### Heritage Builder - Story Synopsis

> The Emperor of Đại Việt is aging, and the kingdom stands at a turning point.
>
> You are his adopted son - born not of royal blood, but of a loyal general who died defending the throne. Out of gratitude, the Emperor raised you as his own. Yet the court still whispers: “He is not of the Dragon Line.”
>
> To silence doubt and earn your place among the rightful heirs, the Emperor sends you beyond the capital - to the borderlands, where the wild rivers meet the jungle. There, he commands you to build a city of order and prosperity, to tame the frontier and prove your virtue through creation, not conquest.
>
> Armed with ancestral wisdom and the trust of your father, you must lead settlers, craftsmen, and farmers to carve a new legacy from untouched land.
>
> Every house you raise, every harvest you reap, and every temple you consecrate will echo your worth.
>
> But the land is not gentle.
>
> Floods, famine, and whispers of rebellion test your leadership — and deep within the court, jealous princes conspire to see you fail. You must balance progress with harmony, ambition with humility, strength with mercy.
>
> Your success will shape the future of Đại Việt.
>
> Will you be remembered as a usurper who reached too high — or as the visionary who forged a new dynasty from the soil of the old?
>
> Build. Endure. Rule with wisdom. Your heritage awaits.

## 2. Key Gameplay Pillars

- **Authentic Historical Economy** – Recreate agricultural and trade systems of Vietnam’s dynastic period, from rice fields to silk trade.
- **Cultural Management** – Maintain social harmony through festivals, temples, and education influenced by Confucian and local traditions.
- **Urban Planning & Infrastructure** – Build cities with canals, villages, citadels, and trading posts following historical layouts.
- **Dynamic Citizen Simulation** – Every household contributes labor, production, and taxes; satisfaction depends on housing, food, and faith.
- **Environmental Challenge** – Manage floods, monsoon seasons, and resource depletion tied to Vietnam’s geography.

## 3. Core Gameplay Loop

- **Plan & Build** — Lay out roads, homes, farms, and workshops.
- **Produce & Distribute** — Generate goods (rice, pottery, cloth, tools) and ensure proper delivery to citizens.
- **Collect & Manage Resources** — Balance taxes, labor, and production chains.
- **Grow & Celebrate** — Host festivals, expand temples, and promote cultural progress.
- **Adapt & Survive** — Respond to disasters, political changes, and economic shifts.

## 4. Systems Overview

**Economy:**

- **Agriculture:** Rice paddies, irrigation, crop cycles.
- **Industry:** Pottery, textiles, blacksmithing, carpentry.
- **Trade:** River and overland routes; local and regional markets.

**Population:**

- **Social classes:** Peasants, Artisans, Scholars, Nobles.
- **Needs:** Food, shelter, faith, education, entertainment.

**Culture & Religion:**

- Festivals (Lunar New Year, harvest events).
- Temples, pagodas, and ancestral worship sites.

**Environment:**

- Seasonal cycles (rainy/dry seasons).
- River flooding and fertile plains management.

## 5. Art & UI Direction

**Style:** Hand-painted 2D isometric art (inspired by lacquer and Đông Hồ woodblock aesthetics).

**Color Palette:** Warm earth tones, gold, and jade hues.

**UI Theme:** Scroll-style panels, minimal HUD, clear icons for economy, culture, and construction.

**Camera:** Fixed isometric with zoom and rotation options.

## 6. Technical Overview

- **Language:** Rust
- **Engine:** Custom 2D Engine (supports tilemaps, sprite batching, event-driven simulation).
- **Rendering:** Hardware-accelerated 2D (OpenGL/Vulkan backend).

**Tools:**

- Internal editor for tiles, entities, and resource graphs.
- Save/load system via JSON or binary format.
- Modular simulation layers (economy, AI, UI).

## 7. Development Roadmap

### Phase 1: Concept & Prototyping (Now)

- Define core systems (construction, economy).
- Build minimal engine tools and map editor.
- Implement base tilemap rendering and entity system.

### Phase 2: Core Systems Implementation

- Add population simulation, trade, and resource chains.
- Integrate seasonal cycle and environmental effects.

### Phase 3: Playable Demo (for itch.io)

- Include basic build mechanics, economy, and a small campaign map.
- Prepare page assets (screenshots, logo, short trailer).

### Phase 4: Feedback & Expansion

- Gather community input on balance and UX.
- Expand content: new buildings, events, and visual polish

## 8. Publishing & Marketing

- **Platform:** itch.io (initial), potential Steam release later.
- **Monetization:** Pay-what-you-want or small fixed price.

**Promotion:**

- Developer devlog series (showing Vietnamese cultural references).
- Social media posts with short timelapse videos of city growth.
- Highlight “Inspired by Emperor: Rise of the Middle Kingdom with a unique Southeast Asian focus.”
