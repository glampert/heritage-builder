# UI mockups

Mockups of game screens in each realm's visual style. They show layout, palette and motifs, not final art: everything is drawn as SVG by a script and screenshotted.

## World map

The strategic world map (see [world.md](../world.md#the-world-map) and [gdd.md](../gdd.md) 5.12), one per realm, each at the point of its campaign just before mission 6, where the first map choice comes up.

| Realm | Style | Image |
|---|---|---|
| Đại Long | Parchment, carved wood, red seal stamps, pen-and-ink mountains and karst | [world-map-dailong.jpg](world-map-dailong.jpg) |
| Kambara | Sandstone, moss and leaves, chiselled lettering, lotus-bud towers | [world-map-kambara.jpg](world-map-kambara.jpg) |
| Tampadesa | Brick, carved teak, gold leaf, earthen tones, gilded stupas | [world-map-tampadesa.jpg](world-map-tampadesa.jpg) |

Placeholders in the mockups: the names of the cities founded in missions 1–5 (the docs don't name them yet), the realms' era names, the trade goods and prices, and the map's geography beyond the rough layout in world.md.

## Intro

The first screen of a story comic sequence (see [story-script.md](../story-script.md)), one panel per realm in the folk-art style the GDD gives its comics: a narration caption, a speech bubble, and a bar with the sequence title, panel dots and Skip / Next.

| Realm | Panel | Image |
|---|---|---|
| Đại Long | I-A "Adopted, Yet Chosen", panel 6, as a woodblock print | [intro-dailong.jpg](intro-dailong.jpg) |
| Kambara | II-A "The Lake Provinces", as a temple bas-relief | [intro-kambara.jpg](intro-kambara.jpg) |
| Tampadesa | III-A "The Captive Scholar", as a temple mural | [intro-tampadesa.jpg](intro-tampadesa.jpg) |

## Intermission

The debrief after a mission (gdd.md 4, *Mission flow*), shown after mission 5 of each campaign: the campaign's stele with a carved line per mission, the newest highlighted, beside the goals, distinctions, ratings, city stats and the city joining the world map.

| Realm | Stele | Image |
|---|---|---|
| Đại Long | Grey stone on a tortoise, in a red lacquer niche | [intermission-dailong.jpg](intermission-dailong.jpg) |
| Kambara | Pointed sandstone slab on a lotus pedestal, with moss and vines | [intermission-kambara.jpg](intermission-kambara.jpg) |
| Tampadesa | Rounded slab with a gilded border and umbrella finial, on a brick plinth | [intermission-tampadesa.jpg](intermission-tampadesa.jpg) |

## In-game HUD

The minimal HUD of gdd.md 10 over a city: speed and date, population, treasury, food and the four ratings, the goals tracker, the build bar with a tooltip, the minimap and the message log. The minimap is the [minimap concept](#minimap) A drawn by `minimap.py` (the sample city, in each realm's palette) with the game's layout: lens icons right-aligned across the top and tools down the right. Đại Long uses parchment and wood, Kambara mossy sandstone, Tampadesa earthen brown with gilt trim.

| Realm | Image |
|---|---|
| Đại Long | [hud-dailong.jpg](hud-dailong.jpg) |
| Kambara | [hud-kambara.jpg](hud-kambara.jpg) |
| Tampadesa | [hud-tampadesa.jpg](hud-tampadesa.jpg) |

The city behind the HUD is existing art: Đại Long's is cropped from `../../images/prototype_apr_2026_3.jpg`, and Kambara's and Tampadesa's are the small reference mocks in `../../images/references/` scaled up, so they look soft.

Placeholders in these screens: the stele lines, goals, numbers and distinctions, and the dialogue not given in story-script.md.

## Main menu

The home menu and its dialogs: load / save, settings, custom game and about. The game shows the home menu in the style of the realm whose campaign the player has reached, so each realm's set shows the saved progress at that point: the campaigns reached, Continue and Campaign pointing at mission 6, saves from earlier campaigns, and the realms Custom Game has unlocked.

| Screen | Đại Long | Kambara | Tampadesa |
|---|---|---|---|
| Home | [menu-home-dailong.jpg](menu-home-dailong.jpg) | [menu-home-kambara.jpg](menu-home-kambara.jpg) | [menu-home-tampadesa.jpg](menu-home-tampadesa.jpg) |
| Load / save | [menu-load-save-dailong.jpg](menu-load-save-dailong.jpg) | [menu-load-save-kambara.jpg](menu-load-save-kambara.jpg) | [menu-load-save-tampadesa.jpg](menu-load-save-tampadesa.jpg) |
| Settings (Game, Sound, Graphics tab) | [menu-settings-dailong.jpg](menu-settings-dailong.jpg) | [menu-settings-kambara.jpg](menu-settings-kambara.jpg) | [menu-settings-tampadesa.jpg](menu-settings-tampadesa.jpg) |
| Custom game | [menu-custom-game-dailong.jpg](menu-custom-game-dailong.jpg) | [menu-custom-game-kambara.jpg](menu-custom-game-kambara.jpg) | [menu-custom-game-tampadesa.jpg](menu-custom-game-tampadesa.jpg) |
| About | [menu-about-dailong.jpg](menu-about-dailong.jpg) | [menu-about-kambara.jpg](menu-about-kambara.jpg) | [menu-about-tampadesa.jpg](menu-about-tampadesa.jpg) |

Đại Long's home backdrop is the game's own painting (`assets/ui/misc/home_menu_static_bg.png`); Kambara's and Tampadesa's are flat vector stand-ins drawn by `menus.py`. Placeholders: the save names, maps, numbers and credit names.

## In-game dialogs and the campaign flow

The screens the [realm UI plan](../../plans/realm-ui.md#mockups) found missing. The in-game ones sit over each realm's HUD; the campaign picker sits over the home backdrop, the briefing over the mission's city. The briefing is each campaign's mission 6, the one after the intermission's mission 5.

| Screen | Đại Long | Kambara | Tampadesa |
|---|---|---|---|
| In-game Game menu | [game-menu-dailong.jpg](game-menu-dailong.jpg) | [game-menu-kambara.jpg](game-menu-kambara.jpg) | [game-menu-tampadesa.jpg](game-menu-tampadesa.jpg) |
| Quit popup | [game-quit-dailong.jpg](game-quit-dailong.jpg) | [game-quit-kambara.jpg](game-quit-kambara.jpg) | [game-quit-tampadesa.jpg](game-quit-tampadesa.jpg) |
| Campaign picker | [campaign-picker-dailong.jpg](campaign-picker-dailong.jpg) | [campaign-picker-kambara.jpg](campaign-picker-kambara.jpg) | [campaign-picker-tampadesa.jpg](campaign-picker-tampadesa.jpg) |
| Briefing | [briefing-dailong.jpg](briefing-dailong.jpg) | [briefing-kambara.jpg](briefing-kambara.jpg) | [briefing-tampadesa.jpg](briefing-tampadesa.jpg) |
| Campaign complete (the finished stele) | [campaign-complete-dailong.jpg](campaign-complete-dailong.jpg) | [campaign-complete-kambara.jpg](campaign-complete-kambara.jpg) | [campaign-complete-tampadesa.jpg](campaign-complete-tampadesa.jpg) |
| HUD menu buttons, build menu and its tooltip with costs | [hud-menus-dailong.jpg](hud-menus-dailong.jpg) | [hud-menus-kambara.jpg](hud-menus-kambara.jpg) | [hud-menus-tampadesa.jpg](hud-menus-tampadesa.jpg) |
| Building inspector | [inspector-dailong.jpg](inspector-dailong.jpg) | [inspector-kambara.jpg](inspector-kambara.jpg) | [inspector-tampadesa.jpg](inspector-tampadesa.jpg) |
| Important-event popup (flood, court request, fire) | [event-dailong.jpg](event-dailong.jpg) | [event-kambara.jpg](event-kambara.jpg) | [event-tampadesa.jpg](event-tampadesa.jpg) |

- The HUD's menu buttons (Game, Load / Save, Settings, City book, Briefing) get their own bar left of the build bar, not the left end of the top-left bar: the date leaves no room there.
- The campaign picker only appears once a second campaign is reached, so Đại Long's version, with Campaign I alone open, shows its look rather than a state the game would show.
- The build menu's costs are the game's own (`assets/tiles/objects/tile_set.json`); the bricks and roof tiles in storage are the Goods page's.

Placeholders: the briefing and event text (from campaigns.md where it has it), the last three stele lines of each campaign, the distinctions, and the inspector's numbers.

## Build palette icons and the HUD's buttons

The HUD's build palette (realm-ui.md Phase 5), after the user's requests of 2026-11-14: an icon per building instead of its text label, as in Pharaoh: A New Era. Clicking a sector's button opens its buildings as a grid of icons, four to a row, left of the column; hovering one shows its name and cost (gold, and each material with what's in storage, in the accent colour when short); clicking it picks the building and closes the grid. A building that can't be built yet stays in the grid, faded.

The HUD's top is one bar now, from the left edge to the build column: the goals button first, then the speed controls and the date at its left, population, treasury, food and the ratings in its middle, and the game menu at its right. The message log is centred at the bottom of the screen. Count badges (the goals', the minimap's alerts) take the realm's colours: a vermilion seal ringed in gilt, moss on a sandstone ring, gold leaf ringed in lacquer. Every button on the HUD takes the same treatment: the sector column, the speed controls, the goals button, the game-menu buttons (Game, Load / Save, Settings, City book, Briefing) and, smaller and plainer so their icons stay readable at 30 points, the minimap's lenses and tools. The minimap is laid out as the game lays it out now: the lenses (the landmarks toggle, risk, water, sectors, city) right-aligned across the top against the tools column (zoom in, zoom out, alerts, home, world map, hide).

The goals panel is no longer always open: a goals button (a banner with a tick, its badge counting the goals met) sits at the top bar's left end, before the speed controls, and the panel drops down under it, where the goals used to be. The mission's state reads together there (the date, its season and its goals), and the panel opens over the corner of the city least in the way, clear of the build palette on the right.

The pictograms are drawn by `palette_glyphs.py` in each realm's architecture (Đại Long's curved tile roofs, Kambara's naga-finialed roofs and lotus-bud prasat, Tampadesa's tiered roofs and stupa), so the temple, shrine and university differ by realm, and every roofed building takes its realm's roof; the HUD's and the minimap's icons are in `palette.py`. Each realm treats them its own way:

| Realm | Treatment | Every icon and its states | The palette open | The HUD at rest, goals open |
|---|---|---|---|---|
| Đại Long | Vermilion stamps on parchment in a frame of gilt bamboo lashed at the corners, under a small tiled roof with a gilt lotus, after the user's references; pressed is the stamp cut the other way, red ground with the building bare; the stamps are hand-pressed: uneven ink, speckled, ragged at the edges, with a faint double strike; the HUD's panels are carved red lacquered wood round parchment, every ornament outside the parchment: a top beam curling up past the corners, gilt key-pattern blocks on the corners, and on the large ones a lattice and gilt ruyi clouds carved along the top beam, a lotus medallion under a little roof at its middle and a gilt cloud at the foot (`dailong_carving.py`) | [build-icons-dailong.jpg](build-icons-dailong.jpg) | [build-palette-dailong.jpg](build-palette-dailong.jpg) | [hud-goals-dailong.jpg](hud-goals-dailong.jpg) |
| Kambara | Carved sandstone, after the user's references: a frame of weathered blocks with corner volutes, a Bayon face crowning it, moss and hanging vines, a dark cracked panel and the building in relief, lit from the top left; pressed sinks the relief, hovered glows; the HUD's panels are the same carved frames, the large ones crowned too, and its accents (icons, bars, rings, titles) are moss green on stone (`kambara_carving.py`) | [build-icons-kambara.jpg](build-icons-kambara.jpg) | [build-palette-kambara.jpg](build-palette-kambara.jpg) | [hud-goals-kambara.jpg](hud-goals-kambara.jpg) |
| Tampadesa | Gold leaf on red lacquer in an arched, gilt niche; pressed is gold ground with the building in lacquer; disabled is tarnished, the leaf worn off | [build-icons-tampadesa.jpg](build-icons-tampadesa.jpg) | [build-palette-tampadesa.jpg](build-palette-tampadesa.jpg) | [hud-goals-tampadesa.jpg](hud-goals-tampadesa.jpg) |

The sectors and buildings are the game's (`TilePaletteMainButtonDef`, the sectors and costs in `assets/tiles/objects/tile_set.json`); Services' icon is a rolled scroll. The palette screens open Industry and resources, with the mine disabled and the distillery hovered, short of bricks. These screens supersede the HUD's buttons and goals panel in `hud-<realm>.jpg` and `hud-menus-<realm>.jpg`. The palette names the shrine, temple and university in each realm's own words (Spirit shrine, Neak ta shrine, Nat shrine; Temple, Prasat, Stupa; Temple of Literature, Library, Monastery school). Placeholders: those names, and the bricks in storage.

## City book

The City book: one dialog with a page per area, opened from the HUD. Each realm's set shows its HUD city, so the population, treasury, food and ratings are the HUD's, and the pages agree with each other: the houses' residents add up to the population, the labour sectors to the workforce, the ledger to the treasury.

| Page | Đại Long | Kambara | Tampadesa |
|---|---|---|---|
| Overview | [city-overview-dailong.jpg](city-overview-dailong.jpg) | [city-overview-kambara.jpg](city-overview-kambara.jpg) | [city-overview-tampadesa.jpg](city-overview-tampadesa.jpg) |
| Population | [city-population-dailong.jpg](city-population-dailong.jpg) | [city-population-kambara.jpg](city-population-kambara.jpg) | [city-population-tampadesa.jpg](city-population-tampadesa.jpg) |
| Goods | [city-goods-dailong.jpg](city-goods-dailong.jpg) | [city-goods-kambara.jpg](city-goods-kambara.jpg) | [city-goods-tampadesa.jpg](city-goods-tampadesa.jpg) |
| Finances (M4's yearly accounts, tax level and wage) | [city-finances-dailong.jpg](city-finances-dailong.jpg) | [city-finances-kambara.jpg](city-finances-kambara.jpg) | [city-finances-tampadesa.jpg](city-finances-tampadesa.jpg) |
| Ratings (M9) | [city-ratings-dailong.jpg](city-ratings-dailong.jpg) | [city-ratings-kambara.jpg](city-ratings-kambara.jpg) | [city-ratings-tampadesa.jpg](city-ratings-tampadesa.jpg) |
| Labour (M11) | [city-labour-dailong.jpg](city-labour-dailong.jpg) | [city-labour-kambara.jpg](city-labour-kambara.jpg) | [city-labour-tampadesa.jpg](city-labour-tampadesa.jpg) |
| Food (M11) | [city-food-dailong.jpg](city-food-dailong.jpg) | [city-food-kambara.jpg](city-food-kambara.jpg) | [city-food-tampadesa.jpg](city-food-tampadesa.jpg) |
| Heritage (faith and monuments): Monument, Patrons and Festivals sub-tabs, Festivals open | [city-heritage-dailong.jpg](city-heritage-dailong.jpg) | [city-heritage-kambara.jpg](city-heritage-kambara.jpg) | [city-heritage-tampadesa.jpg](city-heritage-tampadesa.jpg) |

The Heritage page (GDD 5.11 and 5.15) groups the monument, the patrons and the festivals under sub-tabs; the mockup shows the Festivals tab. Each realm shows the mission that builds its first monument (the Temple of Learning, Suryagiri, the Gilded Stupa), so its city and date are that mission's, not the HUD's. Patron names follow world.md's rule that faiths go unnamed.

Placeholders: every number, the rating drivers, the labour sectors and priorities, and the houses' next-level needs. The ledger's categories and the tax levels are M4's; the money's scale is the finance plan's (about 13 gold a year in tax per worker, a wage of 6), not today's building costs, which wait for M13's balance pass.

## Minimap

Concepts for a more modern minimap, on one 80 × 80 sample city drawn in the minimap's real projection (an isometric diamond clipped to the playable area, as in `crates/game/src/tile/minimap.rs`).

| Sheet | Image |
|---|---|
| Today's minimap beside three directions: A, painted relief (recommended); B, an ink map like the world map; C, lenses (A with one data layer) | [minimap-concepts.jpg](minimap-concepts.jpg) |
| Concept A in Đại Long's HUD frame: lens icons across the top, tools down the right (zoom, alerts, home, world map, collapse), and the four lenses: sectors, water, fire risk, faith | [minimap-lenses.jpg](minimap-lenses.jpg) |

Drawn by `minimap.py` (needs numpy and Pillow). Its icon paths come from [crates/tools/ui-icons/icons.json](../../../crates/tools/ui-icons/icons.json), which also makes the game's button and landmark sprites, so the two stay alike. The implementation plan is [docs/plans/minimap.md](../../plans/minimap.md); the game's minimap now follows concept A with the layout above.

## Fonts

Each realm has its own typeface, set in `REALM_FONTS` in `generator/common.py`:

| Realm | Font | Source |
|---|---|---|
| Đại Long | Protest Revolution | `assets/fonts/protest_revolution_regular.ttf` (SIL OFL, `protest_revolution_OFL.txt`) |
| Kambara | Jaini, a carved-stone look | `assets/fonts/jaini_regular.ttf` (SIL OFL, `jaini_OFL.txt`) |
| Tampadesa | Amita, thin strokes and round letters | `assets/fonts/amita_regular.ttf`, `amita_bold.ttf` (SIL OFL, `amita_OFL.txt`) |

Jaini and Amita have no Vietnamese, so Đại Long's names on Kambara's and Tampadesa's screens take their accented letters from a fallback (Spectral SC and Alegreya, from Google Fonts). Protest Revolution covers Vietnamese.

## Live preview

```bash
cd docs/design/mockups/generator
python3 serve.py --open
```

builds every screen and serves them at http://127.0.0.1:8765/ (`--port` and `--host` change that). The index links each screen; a screen's page fits it to the window, with links to the other realms and ← / → to step through all of them. Saving a file in this folder, or a font in `assets/fonts/`, re-runs only the scripts it affects (about a second) and the open page reloads itself. A failed build shows its traceback over the page and keeps the last good screens. It's the standard library only, but the scripts it runs need Pillow. It writes only the HTML in `build/`: run `shot.js` to update the committed JPEGs.

## Regenerating

The generator lives in [generator/](generator/): `common.py` holds the shared geography and layout, `content.py` what each realm's map shows, and `world_map.py` the three styles. `screens.py` (with its text in `screens_content.py`) draws the intro, intermission and HUD screens in the same styles, and `menus.py` (text in `menus_content.py`) the main menu screens. `pages.py` (text in `pages_content.py`) draws the City book, `palette.py` (pictograms in `palette_glyphs.py`, Kambara's carving in `kambara_carving.py`, Đại Long's woodwork in `dailong_carving.py`) the build palette's icons and the HUD's buttons, and `dialogs.py` (text in `dialogs_content.py`) the in-game dialogs and the campaign flow screens. It needs Python 3, Node with Playwright and Chromium, and network access to Google Fonts the first time.

```bash
cd docs/design/mockups/generator
python3 fonts.py        # embeds the fonts into build/fonts.css (git-ignored): Google Fonts and assets/fonts
python3 world_map.py    # writes build/world-map-<realm>.html
node shot.js world-map-dailong world-map-kambara world-map-tampadesa
python3 screens.py      # writes build/{intro,intermission,hud}-<realm>.html
node shot.js intro-dailong intermission-dailong hud-dailong   # and so on per realm
python3 minimap.py      # writes build/minimap-concepts.html and minimap-lenses.html
python3 menus.py        # writes build/menu-{home,load-save,settings,custom-game,about}-<realm>.html
node shot.js menu-home-dailong   # and so on
python3 pages.py        # writes build/city-{overview,population,goods,finances,ratings,labour,food}-<realm>.html
python3 dialogs.py      # writes build/{game-menu,game-quit,campaign-picker,briefing,campaign-complete,hud-menus,inspector,event}-<realm>.html
python3 palette.py      # writes build/{build-icons,build-palette,hud-goals}-<realm>.html
node shot.js city-overview-dailong game-menu-dailong   # and so on
```

`node prev.js NAME...` saves quick 1x previews as `build/prev-<name>.jpg`, and `OUTDIR=DIR node shot.js ...` writes the full-size images somewhere other than this folder, e.g. to compare before replacing them.

Run `python3 backdrops.py` (needs Pillow) before `screens.py`: it crops the city art behind the HUD into `build/city_<realm>.jpg`.

`shot.js` saves `../<name>.jpg` at 2x (3840 x 2160); `SCALE` and `QUALITY` change that.
