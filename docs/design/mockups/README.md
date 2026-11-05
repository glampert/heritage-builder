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

## Regenerating

The generator lives in [generator/](generator/): `common.py` holds the shared geography and layout, `content.py` what each realm's map shows, and `world_map.py` the three styles. It needs Python 3, Node with Playwright and Chromium, and network access to Google Fonts the first time.

```bash
cd docs/design/mockups/generator
python3 fonts.py        # downloads the fonts into build/ (git-ignored)
python3 world_map.py    # writes build/world-map-<realm>.html
node shot.js world-map-dailong world-map-kambara world-map-tampadesa
```

`shot.js` saves `../<name>.jpg` at 2x (3840 x 2160); `SCALE` and `QUALITY` change that.
