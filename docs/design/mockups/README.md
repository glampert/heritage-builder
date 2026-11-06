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

The minimal HUD of gdd.md 10 over a city: speed and date, population, treasury, food and the four ratings, the goals tracker, the build bar with a tooltip, the minimap and the message log. Đại Long uses parchment and wood, Kambara mossy sandstone, Tampadesa earthen brown with gilt trim.

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

builds every screen and serves them at http://127.0.0.1:8765/ (`--port` and `--host` change that). The index links each screen; a screen's page fits it to the window, with links to the other realms and ← / → to step through all 27. Saving a file in this folder, or a font in `assets/fonts/`, re-runs only the scripts it affects (about a second) and the open page reloads itself. A failed build shows its traceback over the page and keeps the last good screens. It's the standard library only, but the scripts it runs need Pillow. It writes only the HTML in `build/`: run `shot.js` to update the committed JPEGs.

## Regenerating

The generator lives in [generator/](generator/): `common.py` holds the shared geography and layout, `content.py` what each realm's map shows, and `world_map.py` the three styles. `screens.py` (with its text in `screens_content.py`) draws the intro, intermission and HUD screens in the same styles, and `menus.py` (text in `menus_content.py`) the main menu screens. It needs Python 3, Node with Playwright and Chromium, and network access to Google Fonts the first time.

```bash
cd docs/design/mockups/generator
python3 fonts.py        # embeds the fonts into build/fonts.css (git-ignored): Google Fonts and assets/fonts
python3 world_map.py    # writes build/world-map-<realm>.html
node shot.js world-map-dailong world-map-kambara world-map-tampadesa
python3 screens.py      # writes build/{intro,intermission,hud}-<realm>.html
node shot.js intro-dailong intermission-dailong hud-dailong   # and so on per realm
python3 menus.py        # writes build/menu-{home,load-save,settings,custom-game,about}-<realm>.html
node shot.js menu-home-dailong   # and so on
```

`node prev.js NAME...` saves quick 1x previews as `build/prev-<name>.jpg`, and `OUTDIR=DIR node shot.js ...` writes the full-size images somewhere other than this folder, e.g. to compare before replacing them.

Run `python3 backdrops.py` (needs Pillow) before `screens.py`: it crops the city art behind the HUD into `build/city_<realm>.jpg`.

`shot.js` saves `../<name>.jpg` at 2x (3840 x 2160); `SCALE` and `QUALITY` change that.
