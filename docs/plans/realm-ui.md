# Realm UI: menus, screens and HUD in each realm's style

**Status (2026-11-13): Phases 0 and 1 done (the main menus: Home, Load / Save, Settings, Custom Game, About and the in-game Game menu); Phase 2 done (the world map: its data, base art, screen, pan and zoom, and the next mission's choice); Phase 3 done (the debrief, the campaign-complete stele and the briefing); Phase 4 (story comics) in progress: the story data and panel art done.** The user reviewed it on 2026-11-07: their notes are folded in (see [Review notes](#review-notes)), the open decisions' defaults are accepted, and every mockup the phases need now exists. This plan builds and restyles every retail screen after the [UI mockups](../design/mockups/README.md): the main menus, the world map, the intermission, the story comics, the HUD and the city management pages. Each screen gets one look per realm: Đại Long, Kambara and Tampadesa. It is milestone **UI** of the [roadmap](roadmap.md). It was researched against `ccca1df` on `claude`. File and line references point at that commit, so re-check them before relying on them.

![Home menu, Đại Long](../design/mockups/menu-home-dailong.jpg)

## How to resume this work

- Work through the [phases](#phases) in order. Phase 0 builds what every later phase uses. Each later phase delivers one set of working screens, for all three realms, with its art, automation and tests.
- Tick the checkboxes as tasks land, and add a short dated note under the phase: the commit, the deviations, and what was verified. This file is the source of truth for progress.
- The [open decisions](#open-decisions) each have a default. Proceed with the default unless the user has decided otherwise, and record the answer here.
- The mockups are the visual reference. Their generator (`docs/design/mockups/generator/`) also draws the placeholder art (see [Art pipeline](#art-pipeline)), and `serve.py` shows changes to it live. Every screen in the phases has a mockup (see [Mockups](#mockups)).
- Follow [AGENTS.md](../../AGENTS.md) throughout:
  - Keep desktop and web compiling.
  - Only `engine/src/ui/immediate.rs` and the retained widgets call egui.
  - Every retail screen passes `ui.layout_issues`.
  - Run `cargo test --workspace` and the scenario scripts before pushing.
  - Regenerate `web/asset_manifest.json` after adding or removing assets.
- No save-format change is planned. The player's campaign progress goes in a profile file among the user configs, not in session saves (see [Realm and progress](#realm-and-progress)).
- New UI code leaves its sound hooks ready (see [Sounds](#sounds)), and its text goes through localized strings (see [Text](#text)).

## Goals

1. **Every retail screen in its realm's style**, as the mockups draw it: materials, frames, fonts, colours and motifs. The look follows the realm the player has reached (menus) or is playing (in game).
2. **Đại Long moves to the mockup style.** The prototype's sprites go: the parchment scrolls, page backgrounds, brush strokes, and the red menu-bar and palette buttons. Đại Long's home backdrop painting stays (see [open decisions](#open-decisions)).
3. **Skins as data.** One skin file per realm, merged over Đại Long's, names the sprites, fonts, colours and sizes. A new realm needs a skin file and an art run, not code. This follows the minimap's [style files](minimap.md#style-files).
4. **Placeholder art made by script**, from the same drawing code as the mockups, so the game and the mockups stay alike. It's never hand-placed, and hand-made art is never overwritten.
5. **The screens that don't exist yet are built now**, with the data the game has today:
   - the world map;
   - the intermission and its stele;
   - the briefing and the comic viewer;
   - the goals tracker, the rating badges and the message log;
   - the city management pages.

   Each grows as its milestone (M4, M9, M10, M11, M12) brings data, through a defined hook, not a rewrite.
6. **Testable.** Every screen opens with `dialog.open` or `ui.click`. It passes `ui.layout_issues` in all three skins and is covered by a scenario in CI.
7. **Works everywhere:** desktop and web, OpenGL and Wgpu, UI scales from 75% to 200%, and Vietnamese text in every skin.

**Not goals:**
- Final hand-drawn art. The pipeline swaps it in by file name when it exists.
- Translations. Every string moves into the localized string store, with English as its only language for now.
- Rebinding keys. The Controls tab exists and lists the bindings; editing them is Phase 5 of the game.
- Gameplay systems. Finance, ratings, trade, messages and story timing belong to their milestones. This plan builds their screens and the hooks those milestones fill.

## What exists today

From a survey of the code at `ccca1df`.

**Screens:**
- **Dialogs** (`crates/game/src/menu/dialog/`), all registered in `DialogMenuImpl` / `DialogMenuKind` (`dialog/mod.rs:66-119`):
  - Home, About, NewGame;
  - LoadGame, SaveGame and LoadOrSaveGame, all from one `SaveGameHelper` (`load_save.rs`);
  - MainSettings, with separate stacked Game, Sound and Graphics dialogs (the mockups have tabs);
  - MainGame, the in-game "pause" menu;
  - CityManagement, with Population, Resources and Finances (text only);
  - MissionComplete and CampaignComplete, which are placeholders: a title and buttons.
- **HUD** (`crates/game/src/menu/in_game/`):
  - `bars.rs`: TopBar with population, player icon, gold and date; LeftBar with Game, Load/Save, Settings and City buttons; SpeedControlsBar.
  - `palette.rs`: twelve 50-point sprite buttons, plus child menus of text buttons.
  - `inspector/`: four modal inspectors with the tile's sprite, a heading and text.
- **Toasts** (`engine/src/ui/toast.rs`): painter-drawn with hard-coded colours.
- **Not built:** no world map, briefing, intermission, comic viewer, goals tracker or message log.

**Art** (`assets/ui/`):
- **Stretched page sprites:**
  - `misc/scroll_bg.png` (every dialog);
  - `square_page_bg.png` (popups, inspector, minimap frame);
  - `tall_page_bg.png` (left bar, palette);
  - `wide_page_bg.png` (tooltips, top bar, speed bar, child menus).
- **Brush strokes:** `brush_stroke_divider.png` and its vertical twin.
- **Buttons:** `buttons/menu_bar/*` and `buttons/palette/*`, four states each.
- **Icons:** `icons/{population,gold,player,red_x}_icon.png`.
- **Home backdrop:** `misc/home_menu_static_bg.png`.
- **Unused:** `misc/home_menu_anim/` (30 frames, behind a `false` constant), `icons/app_icon_bg_large.png`, `icons/player_icon_no_bg.png` and `buttons/minimap/close_*`.

**Engine** (`crates/engine/src/ui/`):
- **Widgets:** 17 retained types, all in one file (`widgets.rs`, about 3,900 lines). Phase 0 split it into `widgets/`, one file per widget family.
- **Text:** `ui/text.rs` is a keyed string store loaded from `assets/configs/ui/text.json`, with two categories (TileDescription, UnitDialog) looked up by hashed key.
- **Sounds:** `ui/sound.rs` has seven `UiSoundKey`s, all placeholders (`buttons/default/*`, `misc/default/*`). Only text and sprite buttons play them (hovered and pressed); toggles, sliders, tabs, lists and dialogs play nothing.
- **Sprites:** every sprite is stretched over its rect (`uv_full`), with **no 9-slice**. Menu insets are clamped to 127 points.
- **Themes:** `UiTheme` is a fixed enum, `{Dev, InGame}` (`mod.rs:93`), with one style file each.
- **Style files** (`styles/game.json`): fonts, text styles, colours and egui widget visuals, but no sprites.
- **Plain egui widgets:** sliders, checkboxes, dropdowns and text inputs get their look only from the style file.
- **Sprite buttons** load `buttons/<name>_<state>.png` when built.

**Dialog framework** (`dialog/mod.rs`):
- Every dialog is built once per process (`initialize`, `:133`).
- A stacked dialog is drawn **inside the root dialog's window** (`draw_child_menu`, `:258`). It shares the root's size, background and insets, and its widgets take the root's path label (e.g. `Home/New Map Width`). That rules out per-dialog sizes and art.

**Realm:**
- The game has no current realm and no persistent campaign progress.
- `CampaignDef` has no realm (`campaign/config.rs`).
- Progress lives only inside session saves (`session.rs:707`, `:782`).
- So Continue and Custom Game are disabled TODOs, and Campaign always starts campaign 0 (`dialog/home.rs`).
- The minimap's `MinimapRealm` (`tile/minimap/style.rs:71`) is the only realm type.

**Automation that depends on today's names:**
- `ui_new_game.jsonl`: New Game, the map size inputs, Terrain Kind, Start New Game.
- `ui_build.jsonl`: the palette tooltips "Services" and "Roads", and the tile buttons.
- `ui_snapshot.jsonl`: dialog kind names, `LeftBar/settings`, `TilePaletteMenu/services` and `BuildingInspector/Close`.
- The `dialog.*` commands, by `DialogMenuKind` name.

## Design

### Realm and progress

```rust
// game/src/realm.rs: replaces MinimapRealm
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumIter, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
pub enum Realm { DaiLong, Kambara, Tampadesa }

// game/src/profile.rs: the player's progress across sessions
#[derive(Serialize, Deserialize, Default)]
pub struct Profile {
    pub version: u32,
    pub campaigns: Vec<CampaignProgress>,   // By campaign id: reached, missions finished, distinctions earned.
    pub last_save: Option<String>,          // What Continue loads.
    pub custom_realms: Vec<Realm>,          // Unlocked when their campaign is started (gdd.md 4).
}
```

- **Campaigns name their realm:** a `realm` field on `CampaignDef` in `campaigns.json`.
- **The profile file:**
  - It's `saves/user/profile.json`, beside the user configs (`saves/user/configs.json`). It's read and written with the user-file functions in `engine::save::storage` (`read_user_file`, `write_user_file`, `delete_user_file`), so on the web it lands in localStorage under `heritage_builder/saves/user/profile.json`, and `saves/user/` isn't listed as saves.
  - Unlike the configs, it isn't an overlay on defaults: the whole profile is written each time.
  - `--default-configs` leaves the user configs out but still reads the profile; a new `--fresh-profile` option starts from an empty one without touching the file, for scenarios and tests.
  - It's updated when a mission starts or ends, and on every save (`last_save`).
  - It isn't part of `GameSession`, so there's no save-version bump.
  - A missing or broken file means a fresh profile, with a logged warning.
- **`realm::ui_realm()`** decides which skin shows:
  - in game: the active campaign's realm, else the Custom Game's realm, else Đại Long;
  - in the menus: the latest realm the profile has reached;
  - an override from `ui.realm realm=...` (automation and the developer editor), so every skin can be tested without playing.
- **The minimap's style follows `ui_realm()`.** `MinimapRealm` and its reload command fold into this; `minimap.reload_style` stays as an alias.
- **Tests:**
  - a profile round-trips;
  - reaching campaign II switches the home realm;
  - a broken profile file falls back cleanly.

### Skins

A skin is everything realm-specific about the UI: one JSON file per realm, `assets/configs/ui/skins/<realm>.json`. Like the minimap styles, a realm's file holds only what it changes and is merged over `dailong.json` (move `merge_json` from `tile/minimap/style.rs` to a shared config helper).

```json
{
  "style": { "fonts": {...}, "text_styles": {...}, "colors": {...}, "widgets": {...}, "spacing": {...} },
  "text": { "title": "#2b2118", "accent": "#a8261c", "ink": "#2b2118", "sub": "#6b4f33", "on_accent": "#fff4dc", "disabled": "#8a7d68" },
  "panels": {
    "dialog":   { "sprite": "skins/dailong/panel_scroll.png", "slice": [96, 120, 96, 120], "content": [64, 84, 64, 84] },
    "popup":    { "sprite": "skins/dailong/panel_paper.png",  "slice": [40, 40, 40, 40],  "content": [28, 28, 28, 28] },
    "bar":      { ... }, "side": { ... }, "tooltip": { ... }, "card": { ... }, "well": { ... },
    "row": { ... }, "row_selected": { ... }, "field": { ... }, "caption": { ... }, "bubble": { ... }
  },
  "buttons": {
    "primary":   { "sprite": "skins/dailong/button_primary", "slice": [16, 16, 16, 16], "text": "on_accent" },
    "secondary": { ... }, "tab": { ... }, "tab_selected": { ... }, "menu_item": { "marker": "skins/dailong/marker_seal.png" },
    "icon": { "folder": "skins/dailong/icons", "size": 28 }
  },
  "controls": { "toggle": {...}, "checkbox": {...}, "slider": {...}, "dropdown": {...}, "scrollbar": {...} },
  "separators": { "rule": {...}, "ornament": {...} },
  "emblems": { "dailong": "skins/emblems/dailong.png", "locked": "skins/dailong/emblem_locked.png" },
  "backdrops": { "home": "misc/home_menu_static_bg.png", "dim": "#00000033" },
  "dialogs": { "Home": { "panel": "dialog", "size": [380, 980], "align": "left" }, "Settings": { "panel": "dialog", "size": [900, 640] } },
  "sounds": { "button_pressed": "buttons/dailong/pressed.wav", "dialog_opened": "misc/dailong/scroll_open.wav", "alert": null }
}
```

- **`style`:** today's `UiStyleDef` (fonts, text styles, colours, egui widget visuals, spacing), moved into the skin unchanged. `styles/game.json` becomes Đại Long's skin's `style` block. `dev.json` stays as it is.
- **`panels`:** the 9-slice backgrounds. `slice` is the sprite's border in pixels, left, top, right, bottom; those parts never stretch. `content` is the inset of the content in points. An optional `center: "tile"` repeats the centre instead of stretching it, for paper or stone grain.
- **`buttons`:** framed text buttons per role, each with four state sprites (the existing `_idle/_hovered/_pressed/_disabled` convention), plus the icon folder for sprite buttons.
- **`controls`:** sprites or colours for the toggle, checkbox, slider, dropdown and scrollbar, so they look like the mockups instead of egui's defaults.
- **`dialogs`:** each dialog's panel, size and placement, so a skin can lay out a screen differently without code.
- **Every sprite path in the UI code comes from the skin.** A unit test checks that `crates/game/src/menu/` has no `misc/` or `buttons/` string literals left.

**Engine changes:**
- **Data-driven themes:** `UiTheme` becomes a `UiThemeId` handle.
  - `UiSystem::load_styles` takes a list of named style definitions: `dev`, plus one per realm skin.
  - `set_ui_theme(id)` switches the theme, as today.
  - Fonts are already registered per style name (`style.rs:53-98`), so a realm's fonts sit beside the others.
- **9-slice:** `UiNineSlice { texture, slice_px: [f32; 4], scale, center: Stretch | Tile }` as 9 quads in one egui mesh.
  - Used by `UiMenu::with_panel`, `UiTooltipText::with_panel`, `UiTextButton::with_frame`, item list rows and tabs.
  - Also usable directly: `UiPainter::nine_slice(rect, &slice, tint)`.
  - Menu content insets stop going through egui's `i8` margin. The menu lays its contents out in its own inner rect, so the 127-point clamp goes away.
- **A skin version counter:**
  - Dialogs are built once per process today. `dialog::rebuild_all()` rebuilds them when the skin changes: a realm switch, entering a session, or `ui.reload_skin`.
  - The HUD menus are already rebuilt per session; they listen for the same counter.

**Game side:** `menu/skin.rs` holds the loaded `UiSkin`, with its textures loaded once per skin. It provides the helpers every screen builds with:
- `skin::dialog(kind)` and `skin::button(role, label)`;
- `skin::panel(name)` and `skin::tooltip(text)`;
- `skin::heading(text, level)` and `skin::separator(kind)`.

Today's `make_tooltip`, `make_text_button`, `make_dialog_popup` and the dialog defaults become these helpers.

### Widgets to add

First, `engine/src/ui/widgets.rs` (about 3,900 lines) becomes a folder, `engine/src/ui/widgets/`, with no change in behaviour:
- `mod.rs`: the `UiWidget` trait, `UiWidgetImpl`, the widget context and the shared helpers, re-exporting every widget so `ui::widgets::*` paths stay as they are;
- `menu.rs`: `UiMenu` and its flags, positions and insets;
- `text.rs`: text, headings and sized labels;
- `group.rs`: plain and labeled widget groups;
- `button.rs`: text and sprite buttons;
- `list.rs`, `input.rs` (sliders, checkboxes, dropdowns, text and number inputs), `tooltip.rs` and so on for the rest, one family per file;
- the new widgets below, each in its family's file or a file of its own.

The split goes in on its own, before any new widget, so its diff only moves code.

All retained widgets live in that folder. Each records itself for `ui.tree` and supports `ui.set` where it takes a value.

| Widget | For | Notes |
|---|---|---|
| `UiTabBar` | Settings, city pages, custom game options | Kind `tab`; tabs get the selected label in their paths, as the developer UI's `ui::tab_bar` does. |
| `UiToggle` | Settings | A pill switch, as in the mockups, with its on and off text. Sprites from `controls.toggle`, or painted from skin colours. |
| Framed `UiTextButton` | Every dialog's buttons | `with_frame(skin button)`, fixed or content width, primary and secondary roles. |
| `UiMenuItem` | Home's menu | A label with a subline under it ("Campaign I · mission 6 of 10"), a hover fill and the realm marker (seal, lotus, umbrella). |
| `UiListRow` template for `UiItemList` | Load/save, custom game maps, realms | Icon or emblem, title, subtitle, right-hand title and subtitle, and the selected-row panel. |
| `UiImage` | Save thumbnails, map previews, emblems, briefing art, comic panels | A texture in a rect, with an optional frame panel and fit (contain, cover, stretch). |
| `UiKeyValues` | Previews, intermission stats, city pages | Label and value pairs in two columns, values right-aligned. |
| `UiTable` | Goals (target and achieved), finance accounts | Header row, columns with alignment, an optional status icon column. |
| `UiMeter` | Goals tracker, ratings, relations | A bar, a ring (the rating badges), or a stepped meter (relations 0–5). Painted. |
| `UiColumns` | Two-column dialogs | Fixed or fractional column widths, each column a widget group. |
| `UiStepSelector` | Difficulty | Five steps, or any list. A dropdown would do, but the mockup shows steps. |
| `UiRichText` | Captions, briefing, stele lines | Wrapped text with inline emphasis and a per-span colour, e.g. a highlighted newest stele line. |

Toasts move onto the skin's `caption` panel and text colours.

### Dialog framework

- **Each dialog is its own window**, with its own panel, size and placement from the skin's `dialogs` block. This replaces drawing a stacked dialog inside the root.
  - The stack and Escape handling stay as they are.
  - A dialog flag says whether the dialog under it stays visible. Home's panel is hidden under Settings in the mockups.
- **Widget paths take the dialog's own label** (e.g. `Settings/Game/Autosave`). The scenarios and AGENTS.md's path notes are updated in the phase that changes them.
- **Dialog sizes are in points at 1920 × 1080** and scale down to fit smaller windows (minimum 1280 × 720), and with the UI scale setting.
- **Full-screen screens are dialogs with a `Fullscreen` panel:** the world map, the intermission and the comic viewer. They pause the simulation as dialogs do today.

### Fonts

The realm fonts are those of the mockups (`generator/common.py`, `REALM_FONTS`):

| Realm | Font | Vietnamese fallback |
|---|---|---|
| Đại Long | Protest Revolution | Covers Vietnamese itself |
| Kambara | Jaini | Spectral (not the mockups' Spectral SC, whose lowercase letters the game draws as small capitals; Phase 1, Home) |
| Tampadesa | Amita (regular and bold) | Alegreya |

- **Fallback fonts:** add Spectral SC and Alegreya to `assets/fonts/` with their OFL files, as Jaini and Amita were added.
- **Glyph-by-glyph fallback:** egui text styles already take a fallback list (`text_styles.*.fallbacks`), so a Kambara heading shows "Đại Long" with Spectral SC's accented letters.
- **Body text** uses the realm font too, as in the mockups. A skin can give its body style a plainer face if small text reads poorly (see [open decisions](#open-decisions)).
- **Test:** a unit test lays out "Đại Long · Thái Ninh · Hội Thủy" in every skin's text styles and checks that no glyph falls back to the replacement character.

### Art pipeline

**`crates/tools/ui-art/`** draws every skin sprite and backdrop from the mockup generator's own drawing code.

**How it renders:**
- It imports the theme classes in `docs/design/mockups/generator/` (`world_map.py`, `screens.py`, `menus.py`). Those already draw each realm's materials as SVG: Đại Long's scrolls, parchment, wood and lacquer; Kambara's sandstone, moss and vines; Tampadesa's brick, teak, lacquer and gilt.
- It wraps each sprite in a page of its own size and shoots it with Playwright's Chromium, on a transparent background (`omitBackground`) at 2×. The browser is pre-installed in cloud sessions; locally, install it outside the repository (`npm i -g playwright`, with `NODE_PATH` if Node can't find it; see the ui-art README).
- Small glyph icons stay with `crates/tools/ui-icons/` (Pillow, no browser). Its `icons.json` grows the HUD and build-bar glyphs from the mockups' `icon()` (`screens.py:302`).

**What drives it:**
- A **catalog**, `crates/tools/ui-art/art.json`. Each sprite entry gives:
  - realm, id and output path;
  - the generator call and its parameters;
  - size, slice borders and states.
- **`make_ui_art.py [--realm R] [ID...]`** renders what changed. It records each sprite's hash in `manifest.json` and never overwrites a file the manifest doesn't list, i.e. hand-made art.
- **`--check`** compares the catalog's slice borders and paths with the skin files, with no rendering. It runs in CI beside the tile generator's check.

**Output:**
- **Location:** `assets/ui/skins/<realm>/`.
- **Format:** PNG for sprites with alpha; JPEG for backdrops and comic placeholders.
- **9-slice sprites are small:** a dialog panel is about 512 × 512 at 2× however big the dialog. A realm's skin should come to 2–4 MB, plus about 0.5 MB per backdrop.

**What it makes per realm:**
- **Panels:** dialog, popup, bar, side, tooltip, card, well, row, selected row, field, caption, bubble.
- **Buttons:** primary, secondary, tab and selected tab, each in four states; menu item markers; icon button frames.
- **Controls:** toggle, checkbox, slider rail and knob, dropdown arrow, scrollbar.
- **Separators** and ornaments.
- **Emblems:** the three realm emblems, plus locked and current states.
- **HUD and page icons:** population, treasury, food, ratings, goals, message types, build categories, speed controls, the left-bar actions.
- **Home backdrops:** for Kambara and Tampadesa, from `menus.py`'s `home_scene`. Đại Long keeps its painting.
- **The world map base,** without markers (`world_map.py`'s map layers).
- **The stele**, without text (`screens.py`'s `stele`).
- **Comic placeholder panels:** the realm frame, an intro scene from `screens.py`'s `intro_scene`, and the panel's description as a dim caption. They're placeholders until real art exists.

**Old art:**
- Each phase deletes the old sprites its screens no longer use, with the web manifest regenerated.
- Phase 5 removes the last of them:
  - `misc/*_bg.png`;
  - the brush strokes;
  - `buttons/menu_bar/*` and `buttons/palette/*`;
  - the old icons.
- The unused files listed under [What exists today](#what-exists-today) go in Phase 0.

### Text

- **Every string moves to localized strings** as each screen is rebuilt, through `engine::ui::text`. No new retail UI code holds a literal the player reads.
- **The store grows into a localization layer:**
  - one file per language, `assets/configs/ui/text/<language>.json` (today's `text.json` becomes `en.json`), with the language from the Game settings tab (English only for now);
  - new categories: Menu, Hud, City, Story, Realm, Messages;
  - **arguments:** `{name}` placeholders filled at lookup (`text::format(category, key, &[("count", "3")])`), since most labels carry numbers, names or dates;
  - **plural forms:** a key may hold `one` and `other` forms, picked by a count argument;
  - a `UiText` value for widgets, either a key or literal text, so widget labels can take a key and look it up when drawn and again when the language changes;
  - **a fallback:** a key missing in a language falls back to English and is logged once; a key missing in English shows the key itself, so it's caught on screen;
  - a test that every key the code names exists in `en.json`, and that no language file has a key `en.json` lacks.
- **Tile and unit names** move into the store too, with their descriptions.
- **Realm-specific wording comes from data:**
  - era names;
  - season names;
  - the Legitimacy rating's name (Mandate, Divine Favour, Merit);
  - campaign subtitles.
- **No real-world names** appear anywhere (world.md, "Rules for the fiction").

### Sounds

The UI sounds are placeholders today (see [What exists today](#what-exists-today)) and will be redone with the art. New UI code gets the hooks now, so the sounds are a data change later:
- **Every interactive widget plays a sound key** for its events: hovered and pressed for buttons, menu items, tabs and list rows; toggled for toggles and checkboxes; changed for sliders and step selectors (rate-limited by the existing cooldown); opened and closed for dropdowns.
- **Dialogs** play an open and a close sound, and popups their own.
- **Keys are named by role**, not by file: `UiSoundKey` grows `TabSelected`, `Toggled`, `SliderChanged`, `ItemSelected`, `DialogOpened`, `DialogClosed`, `PopupOpened`, `Notification`, `Alert`, and so on. Each widget takes an optional override, as sprite buttons' `UiButtonSoundsEnabled` does today.
- **The skin maps roles to files** (its `sounds` block), so each realm can sound different. A role a skin doesn't name falls back to Đại Long's, and a role nobody names plays nothing. Until real sounds exist, every role maps to today's placeholders.
- `tests/ui_skins.rs` checks that every sound file a skin names exists.

### Automation and tests

- **New commands:**
  - `ui.realm realm=...` switches the skin for testing;
  - `ui.reload_skin` reloads the skin files and rebuilds;
  - `profile.state`, `profile.set` and `profile.reset` read and edit the profile;
  - `text.language` and `text.missing` switch the language and list the keys a screen looked up and didn't find.
- **Kept, or renamed with their scenarios in the same commit:**
  - dialog kind names;
  - button labels;
  - sprite-button names;
  - menu labels: `TopBar`, `LeftBar`, `TilePaletteMenu`, `BuildingInspector`.
- **Scenarios:** each phase adds one in CI, `ui_<phase>.jsonl`. It visits every screen of the phase in all three skins, clicks its buttons, checks `ui.layout_issues` is 0, and takes screenshots for the CI artifact.
- **Goldens:** `ui_snapshot.jsonl` loops over the three skins, and its goldens are regenerated per backend locally.
- **`tests/ui_skins.rs`** checks that:
  - every skin loads;
  - every sprite it names exists;
  - the 9-slice borders fit their sprites;
  - the fonts cover the test strings.

## Review notes

The user's notes from the 2026-11-07 review, and where each landed:

| Note | Where |
|---|---|
| Break `ui/widgets.rs` up into a folder of smaller files | [Widgets to add](#widgets-to-add), Phase 0 |
| The profile goes in the user configs folder, `saves/user/` | [Realm and progress](#realm-and-progress), open decision 4 |
| Move all text to localized strings, growing `ui/text.rs` | [Text](#text), Phase 0 |
| Save thumbnails at the save's camera position, at a configurable size | Phase 1's save metadata, open decision 10 |
| Animated home backdrops later, as video rather than still frames | [Later](#later) |
| UI sounds are placeholders: new UI code leaves the hooks ready | [Sounds](#sounds), Phase 0 |
| Record the mockups already made | [Mockups](#mockups) |

## Phases

Phases 1 to 6 follow the order the user set. [Dependencies](#dependencies-on-other-milestones) lists what each screen needs from other milestones; every phase ships with today's data and leaves a hook for the rest.

### Phase 0: Foundations

**Delivers:**
- the skins, the art pipeline and the new widgets;
- a developer **Skin Gallery** window that shows every panel, button, control and widget in the chosen realm;
- popups, tooltips and toasts restyled.

The existing dialogs keep their layouts but take their panels from the Đại Long skin.

- [x] `Realm`, `Profile` (`saves/user/profile.json`, `--fresh-profile`), `realm::ui_realm()`, a `realm` field in `campaigns.json`; `profile.*` and `ui.realm` commands; tests.
- [x] Engine: data-driven themes (`UiThemeId`), `UiNineSlice` and `UiPainter::nine_slice`, menus laying out in their own inner rect (no `i8` insets).
- [x] Skins: `assets/configs/ui/skins/{dailong,kambara,tampadesa}.json`; `menu/skin.rs`; the skin version counter and `dialog::rebuild_all()`; `ui.reload_skin`. `styles/game.json` folds into Đại Long's skin. The skin's `sounds` block, applied with `ui::sound::set_mapping` on each skin switch; `assets/configs/ui/sounds.json` folds into Đại Long's skin.
- [x] Fonts: add Spectral SC and Alegreya (and their OFL files); realm text styles with fallbacks; the Vietnamese glyph test.
- [x] Split `engine/src/ui/widgets.rs` into `engine/src/ui/widgets/`, one widget family per file, with no change in behaviour, in a commit of its own.
- [x] Localized strings: `text/<language>.json`, the new categories, arguments, plural forms, `UiText`, the fallback and its test; `text.*` commands. Move today's strings for the screens Phase 0 touches.
- [x] Sound hooks: the role keys, per-widget sound events and overrides, a placeholder role-to-file mapping in `assets/configs/ui/sounds.json` with `ui::sound::set_mapping` for the skins (the skin's `sounds` block: see Skins), dialog and popup open and close sounds.
- [x] Widgets: `UiTabBar`, `UiToggle`, framed `UiTextButton`, `UiMenuItem`, `UiListRow`, `UiImage`, `UiKeyValues`, `UiTable`, `UiMeter`, `UiColumns`, `UiStepSelector`, `UiRichText`, each in the Skin Gallery (first in the developer sample menus, since retired). Each interactive one plays its roles through a `UiWidgetSounds` (`sounds.play(context, event, Some(role))`): `UiTabBar` `tab_selected`, `UiToggle` `toggled`, `UiMenuItem` and `UiListRow` `item_hovered` and `item_selected`, `UiStepSelector` `slider_changed`.
- [x] Dialog framework: each dialog its own window, from the skin's `dialogs` block; the "keep the dialog under it visible" flag; paths take the dialog's own label (update `ui_new_game.jsonl`, `ui_snapshot.jsonl`, AGENTS.md).
- [x] `crates/tools/ui-art/`: the catalog, `make_ui_art.py` (render, manifest, `--check`), the first sprites for all three realms (panels, buttons, controls, separators, emblems); `--check` in CI.
- [x] Popups (Quit, Overwrite), tooltips and toasts on skin panels.
- [x] Remove the unused old files (see [What exists today](#what-exists-today)).
- [x] `tests/ui_skins.rs`; a `ui_skins.jsonl` scenario that opens the Skin Gallery in each realm and checks `ui.layout_issues`.
- [x] Cleanup: retire the engine's sample menus (`engine/src/ui/tests.rs`) and the art only they used; the Skin Gallery and `ui_skins.jsonl` take over what they showed and tested.

**Notes (2026-11-09):**
- The widgets split (commit `260dd32`): `engine/src/ui/widgets/mod.rs` keeps the context trait, `UiWidget`, `UiWidgetImpl`, the casts, callback types and shared helpers, and re-exports every family file (`pub use button::*;` and so on), so every `ui::widgets::<Name>` path is unchanged and nothing outside `engine/src/ui/` changed. The family files are `menu.rs`, `message_box.rs`, `text.rs`, `group.rs`, `button.rs`, `tooltip.rs`, `separator.rs`, `image.rs` (`UiSpriteIcon` and `UiSlideshow`), `input.rs` (sliders, checkboxes, number and text inputs, dropdowns) and `list.rs`. Each file keeps the old section banners.
- The only edits that aren't moves are the ones the split forces: `pub(super)` on `UiMenu`'s `background` and `content_insets`, `UiMessageBox`'s `menu`, and `UiTooltipText`'s `text` and `draw`, which other family files use; and `crate::ui::` in place of `super::` for `screen_size` (twice), `custom_tooltip` and `text`. Comments naming `widgets.rs` in `ui/mod.rs`, `immediate.rs`, `internal.rs`, `registry.rs` and AGENTS.md now name the folder.
- `widget_id` hashes the widget's type name, which now includes its file's module (`widgets::menu::UiMenu`; only `UiMenu`, `UiTextInput`, `UiDropdown` and `UiItemList` use it). That changes no behaviour: the id only has to stay the same from frame to frame and tell widgets apart, it also hashes the widget's address, and egui's memory isn't saved (no `persistence` feature).
- Verified: `cargo build -p HeritageBuilder`, `cargo test --workspace` and the web target check; `ui_build`, `ui_new_game`, `ui_dev_menus`, `minimap` and `ui_snapshot` (the `ui.tree` listing of every screen, without rects) print the same results as before the split, apart from the minimap's bake time. A sorted-lines diff of the old file against the new files, and an in-order diff with the files put back in the old order, show only the edits above.

**Notes (2026-11-09, realm and profile; commit `a94f4e1`):**
- `game/src/realm.rs`: `Realm` (lowercase in strum and serde) replaces `MinimapRealm`. `choose_ui_realm(&UiRealmInputs)` is the pure decision behind `ui_realm()`: the `ui.realm` override, else in game the active campaign's realm (Custom Game's is a `TODO(realm-ui)`), else in the home menu the latest campaign the profile reached, else Đại Long. The minimap's style follows it.
- `game/src/profile.rs`: `Profile` and `CampaignRecord` (named so to avoid `campaign::CampaignProgress`), strict serde with a version check; the user file `profile` is written whole after each change. A missing, broken or other-version file gives a fresh profile with a warning and is left alone. `distinctions` waits for M9.
- Never written by `--fresh-profile` (never read either), script mode, replays, `--smoke-test-saves` or the test harness. CI passes `--fresh-profile` to every scenario, determinism and replay run.
- `CampaignDef` has `id` and `realm`; hooks: a mission start reaches its campaign, a finished mission is queued by `campaign::tick` and recorded by `GameLoop`, a campaign's last mission reaches the next one, a successful save sets `last_save`.
- Commands: `ui.realm`, `profile.state`, `profile.set`, `profile.reset`.
- Deviations: a recording's header holds the profile and its replay uses it (an explicit `--fresh-profile` wins, with a warning). The minimap keeps each style file's JSON from its first read, so realm switches work on web, which reads an asset only once. `minimap.reload_style realm=` is now an alias that sets the override, and puts it back if the reload fails.
- Tests: `tests/realm_profile.rs`, the finished-missions queue in `tests/campaign.rs`, and `ui.realm` checks in `minimap.jsonl`.
- Verified: build, `cargo test --workspace` and the web target check; the CI scenarios with `--fresh-profile`; determinism twice with the same checksum; record and replay of `ui_build` and `determinism`; a live game writing, reloading, rejecting broken and other-version files, and resetting the profile; no `saves/user/profile.json` after scripts, replays, the smoke test or tests. Not run: the mission-finished path end to end in a live game (it needs goals met), and the web build in a browser.

**Notes (2026-11-09, localized strings; commit `b8fc255`):**
- `engine::ui::text`: `text/<language>.json` keyed by category name (`text.json` became `en.json`, without its order rule and panic), with the new categories; `get`, `format`, `format_count` (CLDR plural forms, a per-language `PluralRule`), `find_str` as before. Every file loads at init and stays; `set_language` switches by name, from `GameConfigs::engine.language` (its dropdown: a Phase 1 `TODO(realm-ui)`); a replay applies its recording's.
- Retained widgets' label and tooltip setters take `impl Into<UiString>`; call sites are unchanged. Dropdown and item-list items (`UiDropdown::new`, `reset_items`, `UiItemList::reset_items`) are still plain strings. Menu and popup labels stay strings (automation ids), but a menu without a background sprite draws its label as its title, which needs a `UiString` once localized.
- Moved: the Quit and Overwrite popups; the construction, screenshot and debt toasts.
- Commands `text.language`, `text.missing`, `text.get`; `strings.jsonl`, in CI. The key check (`cargo test -p engine --lib text`) scans every crate for `UiTextCategory::<Name>, "<key>"`.
- Deviations: `UiString`, not `UiText` (a widget's name). The store is a static that loads on first use. The web file system lists its preloaded assets. `config.set /engine/language` doesn't switch the live language; `text.language` does.
- Limit: text a menu copies or measures when built (the inspector's descriptions, the bars' and palette's sizes) follows a switch only once rebuilt; nothing rebuilds on `text::version()` yet.
- Verified: build, `cargo test --workspace`, web check; the key check fails on a missing key, a duplicate key, and a second file's extra key or placeholder. CI scenarios and `strings.jsonl` pass; `ui_snapshot`'s listings match; `ui_build` and `determinism` replay. A temporary second language switched live, fallbacks logged once. A broken or missing `en.json` logs errors and shows keys. Web in headless Chrome loads the text.
- Still literal: the other screens (Home, About, New Game, the Game menu, Load/Save, Settings, City Management, Mission and Campaign Complete), the HUD, inspector and minimap text, and names from data (tiles, units, goods, tax levels, ledger categories, seasons, eras).

**Notes (2026-11-09, sound hooks; commit `5a7b177`):**
- `engine::ui::sound`: `UiSoundKey` has 20 roles (snake_case in data); per-role flags and cooldowns; `UiSoundEvent` and `UiWidgetSounds` (per event: the widget's default role, another, or silence) replace `UiButtonSoundsEnabled`. Buttons, checkboxes, sliders, number fields' steps, dropdowns and item lists play their roles; `UiMenu` plays open and close roles it's given; dialogs (all but Home) and `UiMessageBox` popups get theirs. Text inputs and the developer UI are silent.
- Mapping: `assets/configs/ui/sounds.json`, read once at init; `set_mapping` lays overrides over it, for the skins step.
- Game: `construction_completed` once per frame, `alert` for debt warnings and new minimap alerts, `notification` for the out-of-debt and screenshot toasts; `TODO(realm-ui)` for M11's messages.
- `ui.sounds` lists the roles played (a ring of 64), with or without an audio device; `ui_sounds.jsonl` in CI; `cargo test -p engine --lib sound` checks the mapping.
- Deviations: the default mapping is a config file until skins exist. Cooldowns run on frame time, not wall-clock time, so scripts are repeatable. Debt warnings play `alert`, not `notification`. A dropdown's pick plays `item_selected` instead of `dropdown_closed`. Two roles mapped to one file starting in the same frame play it once, so a press and the dialog it opens still click once. Escape's simulated button press became `dialog_closed`.
- Seen, not changed: Escape in a two-deep stack closes both dialogs (the dialog framework step's Escape owner); the shared per-role cooldown drops a second press within 0.2 s, as before.
- Verified: build, `cargo test --workspace`, web check; CI scenarios and `ui_sounds` (twice, same output); `ui_build` record and replay. Not run: real audio (no device here), web in a browser.

**Notes (2026-11-09, engine themes, 9-slice and inner rect; commit `0c708d4`):**
- Themes: `UiSystem::load_styles(&[UiStyleSource], ppp)` takes named styles (a file under `assets/configs/`, or a JSON value or string), `dev` first; `theme_id(name)` gives a `UiThemeId` (None if unknown), `set_ui_theme(id)` switches. Unknown fields stay errors from JSON values; `UiStyleSource::validate` checks one without its fonts. The game's list is `menu::ui_style_sources()` (`dev`, `game`), the retail theme `menu::retail_ui_theme` (a `TODO(realm-ui)` for the skins). `ui.reload_styles` re-reads files and keeps JSON.
- `TextureCache::texture_size`, shared by both renderers and web.
- `ui/nine_slice.rs`: the geometry, pure and unit-tested; `UiPainter::nine_slice` and the widgets draw it as one mesh. `UiWidgetContext::load_nine_slice`; `UiMenu::with_panel`, `UiMessageBox::with_panel`, `UiTooltipText::with_panel` (`UiTooltipBackground::Panel`), `UiTextButton::with_frame(UiTextButtonFrame)`.
- Menus lay out in their inner rect: insets in whole points, no clamp; `menu_content` is that rect.
- Deviations: `UiNineSlice` also holds the texture's size. `Tile` repeats the edges along their length too, each repeat half a texel in so seams don't bleed. Borders that don't fit shrink by one factor for both axes, keeping corners square. `UiTextButtonFrame::uniform` (one panel, tints per state) since no plain four-state frame art exists yet. Item list rows are left to `UiListRow`.
- Sample: `Sample Menu 3` (panel, framed buttons, panel tooltip and popup, painter stretch, tile and shrink); `ui_dev_menus.jsonl` visits it.
- Verified: build, `cargo test --workspace`, web check; `ui_new_game`, `ui_build`, `ui_dev_menus`, `minimap`, `ui_sounds`, `strings`, `finance` at 1440 × 800; `ui_snapshot` at 1440 × 800 and 1920 × 1080: identical listings apart from Sample Menu 3, and `ui.layout_issues` 0 on every retail screen. At 1440 × 800 the retail screenshots match; at 1080p dialog contents sit 7 points lower (the clamp's end). The sample screenshotted on OpenGL and on Wgpu (lavapipe).

**Notes (2026-11-09, fonts; commit `26881e2`):**
- Added `spectral_sc_semi_bold.ttf` (google/fonts' static file), `alegreya_medium.ttf` and `alegreya_bold.ttf` (Google Fonts' static instances), their upstream OFL files, and the upstream licences of Source Code Pro 2.042 (`source_code_pro_OFL.txt`) and Font Awesome 5.15.4 (`fa_LICENSE.txt`).
- `ui::check_glyph_coverage` lays out `GLYPH_TEST_TEXTS` (Đại Long's names, every Vietnamese letter) in every text style but `icon`, which gets `GLYPH_TEST_ICONS`. It fails on tofu, and on a letter or digit the style's monospace or icon font would draw (the first in the chain that has it): Source Code Pro has all of Vietnamese, so without that rule a missing realm fallback would pass.
- `tests/ui_fonts.rs` runs it on `menu::ui_style_sources()` and on three realm font sets, the `fonts` and `text_styles` blocks for the skins step.
- Deviations: the weights the mockups load, not Regular: Spectral SC SemiBold, whose strokes match Jaini's; Alegreya Medium for titles and Bold for body. A fallback takes its font's scale and a `y_offset_factor` (Kambara 0.093, Tampadesa −0.015), since egui centres fallback glyphs instead of lining up baselines. The sets' scales and sizes are placeholders for the skins.
- Today's styles have no gap: Protest Revolution and Source Code Pro cover Vietnamese; Font Awesome draws only icons.
- Verified: build, `cargo test --workspace`, web check; the test fails with a set's fallback removed; offline renders show the fallback glyphs on the baseline; `ui_new_game` and `ui_dev_menus` pass.

**Notes (2026-11-09, ui-art; commit `3c1abb5`):**
- `crates/tools/ui-art/`: `art.json` (per realm: generator call, size and bleed in points, 9-slice borders in PNG pixels, centre, states), `make_ui_art.py` (renders what changed; hashes in `manifest.json`; never overwrites or prunes unlisted or edited files; `--measure`, `--prune`, `--check`), `render.js`, a README and stdlib unit tests, in CI beside the tile generator's check.
- 175 sprites in `assets/ui/skins/<realm>/` at 2×: twelve panels and a bubble tail; primary, secondary, tab and selected tab in four states; icon frames; marker; toggle, checkbox, slider, dropdown and scrollbar parts; rule and ornament; three emblems (idle, locked, current); Kambara's vines and crest, Tampadesa's crest. Panels at most 508 px. Đại Long 0.64 MB, Kambara 1.58 MB, Tampadesa 0.85 MB, all preloaded on web.
- Deviations: Node's Playwright. Hovered, pressed and disabled are CSS filters of idle. Tiled panels repeat through theme overrides (stitched noise moved onto the middle, no vignette, flat teak, repeating gilt, blotches at 45% contrast, so plainer than the mockups). The engine cuts the last tile, so motif edges (petals, gilt) end cut at the right and bottom corners: the skins should snap middles to whole tiles or add a `round` 9-slice mode (README §2). The scrollbar is the slider turned, the bubble the speech call's body: the generator has neither. No generator file changed.
- Verified: a second run renders nothing; `--force` is byte-identical; `--check` under `python3 -I -S`; unit tests; sprites against the mockups, as 9-slices at dialog size; build and `cargo test --workspace`.

**Notes (2026-11-09, skins; commit `85bddc0`):**
- `menu/skin.rs`: `UiSkinDef` (strict serde), the three files read once at boot, merged with `engine::config::merge_json` (moved from the minimap), styles loaded as themes `dailong`, `kambara`, `tampadesa`; `follow`, `apply_theme`, `version`, `reload`; helpers `dialog`, `button` (roles), `panel`, `tooltip`, `heading` (levels), `separator`, `popup`, `icon`, `sprite_button`. `styles/game.json` and `sounds.json` are gone (Đại Long's `style` and `sounds`; `sound::set_default_mapping`).
- Dialogs: menu items (hover panel), framed primary and secondary buttons, the realm rule under headings, skin popups and tooltips. The HUD keeps its old art through legacy entries (`hud_*` panels, `icon_sets`, `icons`). Sprite buttons take a stem (`with_sprite_stem`), no `buttons/` prefix or 64-byte limit.
- Rebuild: `GameMenusSystem::sync_skin` each frame; `dialog::sync`/`rebuild_all` reopen the stack without open sounds (sounds are set in `build_menu`); the HUD rebuilds too. `ui.reload_skin`.
- Engine: `UiNineSliceCenter::Round` (tests), `UiSeparatorArt`, `UiTextButton::with_hover_panel`/`with_text_colors`, tooltip text colour, `with_background_texture`.
- Deviations: Kambara's vines and the crests aren't drawn (no panel ornaments yet); panels ignore the art's bleed (shadows sit inside the rect); toasts, controls and emblems aren't skinned yet (data only); a popup open during a rebuild closes, and a dialog's unsaved edits are lost; on web `ui.reload_skin` only rebuilds; a widget under the cursor plays its hover again after a rebuild; tooltip content insets come from the style's `menu_margin`.
- Verified: build, `cargo test --workspace`, web check, `make_ui_art.py --check` and its tests; the CI scenarios at 1440 × 800 (`ui_sounds` now switches realm with two dialogs stacked); `ui_build` record and replay; `ui_snapshot` in all three realms: Đại Long's `ui.tree` listings equal the pre-skin ones, `ui.layout_issues` 0 on every retail screen, plus the Quit and Overwrite popups; screenshots compared with the mockups.

**Notes (2026-11-09, widgets) (commit `6f6a84f`):**
- Engine (`ui/widgets/`): `UiTabBar` (`tab_bar.rs`), `UiToggle` and `UiStepSelector` (`choice.rs`), `UiMenuItem` (`menu_item.rs`), `UiIconButton` and `UiTextButtonWidth` (`button.rs`), `UiListRow`/`UiListRowLook` rows for `UiItemList::with_rows`, `UiImage` (`image.rs`), `UiKeyValues`, `UiTable`, `UiMeter` (`data.rs`), `UiColumns` (`columns.rs`), `UiRichText` (`rich_text.rs`). Skin-agnostic; pure logic unit-tested (markup, column split, image fit, meter and step maths, knob position).
- Automation: kinds `tab_bar`, `tab`, `toggle`, `step_selector`, `step`, `menu_entry`, `icon_button`, `image`, `key_values`, `table`, `meter`, `rich_text`; `ui.set` takes toggles, tab bars and step selectors.
- Samples: `Debug/Show Widget Samples` (Sample Menus 4 and 5, Đại Long's art); `ui_dev_menus.jsonl` drives them.
- Game: `skin::tab_bar`, `toggle`, `step_selector`, `menu_item`, `list_row_look`, `icon_button`, `framed_image`, `emblem`, `key_values`, `table`, `table_status_look`, `meter`, `rich_text`; skin blocks `meters` and `rows` (colours), `controls.toggle.text`/`state_text`, buttons' `min_width` and `subline`; the marker, toggle and icon button TODOs are gone. Strings `toggle.on`/`toggle.off`.
- Deviations: menu items record as `menu_entry` (`menu_item` is the developer menu bar's). `UiIconButton` added (the skin's icon role). A labeled `UiItemList` now measures its label (no retail list has one). Step selectors have an arrows style too. Tabs play no hover sound. Emphasis is egui's faux italic, as the fonts have no italic face; the text after it starts a fifth of an em further on, as the sheared glyphs lean over the space. A skin's `min_width` on tabs is a minimum. Toggles and step selectors take the engine's font scale (TODO kept). Checkbox, slider, dropdown and scrollbar art still not drawn (TODO kept).
- Verified: build, `cargo test --workspace`, web check, `make_ui_art.py --check`; `ui_dev_menus`, `ui_new_game`, `ui_build`, `ui_sounds`, `strings`, `minimap` at 1440 × 800; `ui_snapshot` listings equal before and after apart from the new Debug checkbox, `ui.layout_issues` 0 on retail screens and on the samples; the samples screenshotted in all three realms' themes; roles checked with `ui.sounds`.

**Notes (2026-11-09, dialog framework) (commit `514e90b`):**
- `dialog/mod.rs`: each dialog is its own `UiMenu` window; the root window is gone. A `DialogFrame` per dialog (size, align, `show_below`). `draw_current` draws the top dialog modal and, bottom up, those every dialog above keeps visible: plain windows under its backdrop (one dim). Hidden ones aren't drawn or recorded.
- Skin `dialogs`: `size` in points at 1920 × 1080 (`skin::dialog_size`: the smaller window ratio, floored at 1280 × 720's, cut to the window) or `"fullscreen"`; `show_below` (default false). Đại Long's sizes follow the mockups (Home 600 × 960 left, Settings 1220 × 920, Load/Save 1540 × 940, City 1600 × 950); Tampadesa's taller fonts take Home 600 × 1000 and City 1600 × 1020; home dialogs are now centred.
- Escape: no `CloseModalOnEscape` on dialogs; the menus close the top dialog (or its popup) only; `draw_current` does when a focused text field kept the key. A held Escape closes one (repeats ignored). Closing keeps the pause while dialogs remain. `dialog.open` closes every other stacked dialog but Home.
- Engine: sized menus skip egui's fade-in and first sizing pass, so dialogs don't blink.
- Deviations: paths take the kind name (`GameSettings/Autosave`); `Settings/Game/...` waits for Phase 1's tabs. No dialog sets `show_below` yet; full-screen is unit-tested and was tried with a temporary skin. UI scale is a `TODO(realm-ui)`. The Quit popup still hides the Game menu.
- Verified: build, `cargo test --workspace`, web check, `make_ui_art.py --check`; CI scenarios at 1440 × 800 and default size (`ui_new_game` checks paths and hidden Home, `ui_sounds` Escape closing one of two); determinism twice; `ui_build`/`determinism` record and replay; `ui_snapshot` in all three realms at 1280 × 720, 1440 × 800 and 1920 × 1080 with `ui.layout_issues` 0 (widgets scrolled out of view aren't checked) and listings changed only by the root label; `show_below`, full-screen, pause and dim checked live; screenshots compared with the mockups.

**Notes (2026-11-10, popups, toasts, controls, old files, Skin Gallery) (commit `3b6408c`):**
- Popups: a modal menu with its popup open draws as a plain window, still dimming behind it, under the popup's darker backdrop (the skin's `dim_popup`), so the Game menu shows dimmed under the Quit popup as in the mockup; `HideWhenMessageBoxOpen` still hides.
- Tooltips sit inside their panel's `content` insets (`UiTooltipText::with_panel(panel, insets)`); the minimap's hover tooltip takes the skin's tooltip panel unless its style names a `tooltip_frame` (Tampadesa's no longer does).
- Toasts: `UiToastLook` (`UiSystem::set_toast_look`), set by `skin::follow` from the skin's new `toast` block: caption panel at 0.6 scale, the realm's font in every menus mode (`UiSystem::swap_ui_theme`, unlogged); Tampadesa's caption text is `light`. The M6 risk toasts go through the same path; the risk buttons are the developer inspector's.
- Controls: `UiCheckboxLook`, `UiSliderLook`, `UiDropdownLook`; `skin::checkbox`, `slider`, `dropdown` in Settings, New Game and Finances. The scrollbar stays egui's (TODO kept).
- Removed `misc/home_menu_anim/` (Sample Menu 2 cycles emblems now), `misc/scroll_bg.png`, two icons, `buttons/minimap/close_*`.
- Skin Gallery (`menu/gallery.rs`, `Debug/Show Skin Gallery`). `tests/ui_skins.rs` takes in `ui_fonts.rs` and the skin asset unit tests, and checks every 9-slice against its PNG. `ui_skins.jsonl` in CI.
- Deviations: headings record `visible`; `ui.layout_issues` ignores widgets cut by the top or bottom of a menu whose contents scroll (`menu_content` value `scrolls`).
- Verified: build, `cargo test --workspace`, web check, `make_ui_art.py --check`; the CI scenarios at 1440 × 800 and default size; determinism twice; `ui_build` record and replay; `ui_snapshot` in all realms at 1440 × 800 and 1280 × 720 (layout issues 0; listings changed only by the new checkbox and Sample Menu 1's `scrolls`); Quit popup, Settings, toasts, tooltips and the gallery screenshotted in each realm and compared with the mockups.

**Notes (2026-11-10, sample menus retired) (commit `159f368`):**
- Deleted `engine/src/ui/tests.rs` (Sample Menus 1–5), their two Debug checkboxes, and `minimap/frame_tooltip_tampadesa.png`, the only art they alone used (dropped from `ui-icons`' catalog).
- The Skin Gallery gained what only they showed: icon and locked list rows, set button widths, image fits, `UiColumns`, 9-slice centres. `ui_skins.jsonl` drives step selectors, tab bars, list rows, menu items, icon buttons and reads the data widgets.
- Widget API kept on purpose for Phase 1, though nothing calls it now: `UiTextInput`, `UiImage::new`, `UiTextButtonFrame::uniform`, `UiItemList::with_items`, the `with_margins` builders, and `with_fill_width`, `with_state_texts`, `with_icon_tint`, `with_show_tooltip_when_pressed`.
- Verified: build, tests, web check, `make_ui_art.py --check`; CI scenarios at 1440 × 800; `ui_skins` at 1280 × 720 and 1080p; `ui_snapshot`: listings changed only by the checkboxes, other screenshots identical, layout issues 0.

### Phase 1: Main menus

**Delivers:** the home menu and every dialog under it, as in the mockups (`menu-*-<realm>.jpg`), in all three realms. The in-game Game menu and its popups get the same look.

**Home:**
- [x] Tall panel on the left with the crest, title and campaign subtitle.
- [x] The **campaign row** of three emblems: reached, current, or "Locked", from the profile.
- [x] **Menu items:**
  - **Continue** loads `profile.last_save`, with the subline "<mission> · <campaign> · autosave";
  - **Campaign** starts or resumes the reached campaign at its next mission (a campaign picker when more than one is reached: `campaign-picker-<realm>.jpg`);
  - **Custom Game**, Load Game, Settings, About, Quit Game.
- [x] The realm marker on the hovered item, and the version string.
- [x] **Backdrop:** Đại Long's painting; Kambara's and Tampadesa's generated scenes. The skin decides.
- [x] `HomeItem` (was `HomeButtonKind`) loses New Game, whose options move into Custom Game (see [open decisions](#open-decisions)).

**Load / Save** (one dialog, `LoadOrSaveGame`):
- [x] **Save rows:** realm emblem, name, "Campaign I · mission 6 · <city>", the save time, and the in-game date in the realm's era.
- [x] **Preview:** thumbnail; Mission, Campaign, Date, Population, Treasury, Time played.
- [x] **Save as** field, and Delete (new, with a confirm popup), Cancel, Save and Load.
- [x] **Save metadata:**
  - The save writes a small header, beside the version the loader already probes for, plus a thumbnail (`saves/<name>.jpg`).
  - The thumbnail is a screenshot of the city at the camera position the save was taken from, with the UI left out (`Engine::request_screenshot`), scaled down.
  - Its size is a config, `SaveConfigs::thumbnail_size` (default 320 × 180), so it can be raised if it looks blurry in the preview, which shows it larger than that. The preview fits whatever size the file has.
  - Both are read without loading the session.
  - Old saves without them show a blank thumbnail and "—".
  - This is the only save-adjacent change. The header is new top-level fields read before the session, and needs a migration script only if the loader requires them. The default is optional fields with a fallback, as the version probe allows.

**Settings** (`MainSettings` with tabs; the Game, Sound and Graphics dialogs become tabs):
- [x] **Game:**
  - autosave toggle and interval;
  - default game speed;
  - edge scrolling;
  - the three camera zoom toggles;
  - UI scale (75–200%);
  - colour-blind-safe overlays (a stored flag; the overlays follow in M6 and M7);
  - language (English only for now).
  - A language or realm change rebuilds the open dialogs (`dialog::rebuild_all`), which drops what their widgets hold: Settings must carry its pending edits across (the `TODO(realm-ui)` there).
- [x] **Sound:** master, music, SFX, ambience, narration and spatial volume; mute in the background.
- [x] **Graphics:** window mode, resolution, renderer, VSync, frame rate limit, texture filtering, mipmaps, animated water. Settings the engine can't change live say "after a restart".
- [x] **Controls:** the key bindings, read-only.
- [x] Restore defaults, Cancel, Ok, through the existing `SettingsCategory` commit path.

**Custom Game:**
- [x] **Realm rows**, locked until their campaign is started ("Start Campaign N to unlock").
- [x] **Map rows** (a new `assets/configs/game/maps.json`: the preset maps, the sample saves' maps, and a **Blank map** entry with width, height and terrain, which replaces New Game).
- [x] **Preview:** the minimap bake of the map, and its stats.
- [x] **Options:**
  - starting date, from the realm's eras;
  - goals (Sandbox, or a goal set);
  - starting treasury;
  - difficulty in 5 steps (stored; applied by M4 and M9);
  - toggles: seasons and floods, fires and collapses, raids, and fast construction. Fast construction is `construction.time_scale`, which closes the construction plan's open item. The others are stored and disabled with "coming in M6/M7/…" tooltips until their systems exist.
- [x] Cancel and Start. A custom game sets the realm the in-game skin uses.

**About:**
- [x] Crest, title, the studio line, the credit sections, "Built with", the quote, version and copyright, Back.

**In-game Game menu (MainGame):**
- [x] Resume, Save, Load, Settings, Quit to menu, Exit, in the dialog style.
- [x] Quit's confirm popup.

**Wrap-up:**
- [x] Art: home backdrops for Kambara and Tampadesa; the save-row emblems; the remaining menu sprites.
- [x] Delete `misc/scroll_bg.png` once nothing uses it (done in Phase 0).
- [x] Scenarios:
  - `ui_menus.jsonl` (every dialog, every realm, Continue, Custom Game start, settings commit);
  - update `ui_new_game.jsonl` to the Blank map path.

**Notes (2026-11-12, end-of-phase review):**
- Campaign: a mission whose save map can't be loaded doesn't start (`start_campaign_mission` probes it), and a mission load that fails all the same cancels the start (`campaign::cancel_mission_start`, from `GameSession::load_save_game`), so the next load isn't taken for the mission's start. The picker forgets its pick when a session switch drops the dialogs (`dialog::reset`). Home and the picker share the emblem state and unlock text.
- Saves: names take no dot (the storage would replace what follows it with `.json`) and at most 60 characters (`session::MAX_SAVE_NAME_CHARS`, the Save as field's limit too); overwriting a save whose name differs in case saves under its name; Continue finds its save whatever its case. No autosave behind a queued session switch. Load / Save calls a save "Custom game" only by its header's `custom_game`. `--default-configs` keeps Settings' changes for the run.
- UI scale: Home, About and the Game menu size their contents by their content area at the UI scale (`skin::dialog_content_scale`) and their text by that over the UI scale (`dialog_content_text_scale`, `heading_scaled`, `menu_item_scaled`); Custom Game and the picker their text by `dialog_text_fit` and the picker its sizes by `dialog_ui_growth`, as Settings does. All fit from 75% to 200% at 1280 × 720 and 1080p; `ui_menus` checks them at 75% and 200%.
- Verified: build, tests, web, `--check`; CI scenarios at 1440 × 800; determinism twice; Home, the picker, About, Custom Game and the Game menu at 75% and 200% on 1280 × 720 and 1920 × 1080 (no scrolling, no layout issues).

**Notes (2026-11-12, about, Game menu, wrap-up) (commit `2cc728c`):**
- `dialog/about.rs`: crest, title, the realm's campaign, studio line, rule, six credit sections in two columns (the fonts' from `assets/fonts/` and their licences), then a filler, quote, version and copyright, Back at the bottom. `dialog/main_game.rs`: crest, "Game", the mission or custom game, Resume, Save, Load, Settings, Quit to menu, Exit game, and "Saved N minutes ago" at the bottom (`GameSession::last_saved`, time played; an autosave or a loaded autosave says "Autosaved"), followed each frame the menu is drawn, so a save through its own Save shows. Custom Game left the in-game menu (not in the mockup or the plan). Quit and Exit ask in popups (`skin::popup_with_colored_body`, 220 × 56 buttons) with Cancel, Save and quit (or exit), Quit: the popup closes, `GameLoop::save_and_then` writes the autosave's slot next frame and leaves once its thumbnail is written; a failed save, Cancel, a direct quit or another session drops it. Fixed: a rebuild of some dialogs no longer marks the profile dialogs built.
- Phase 1 review: no new art needed. Left, in known-issues: Kambara's vines, the blurred backdrop, Home's last-played line, quote and distinctions, italics, city names; popups' headings are the skin's (smaller than game-quit's, with a rule), and the Game menu's item under the Quit popup isn't drawn hovered.
- Verified: build, tests, web check, `--check`; CI scenarios at 1440 × 800, `ui_menus` also at 720p and 1080p; determinism twice; `ui_snapshot` per realm; `ui_build` and `determinism` record and replay; screenshots against the mockups.

**Notes (2026-11-12, custom game) (commit `3beeecf`):**
- `dialog/custom_game.rs`: `CustomGame` in three columns. Realm rows (emblem, campaign name, "Start Campaign II to unlock"), map rows from the new `maps.json` (Blank map, the sample saves' maps while present, three presets), the preview (a `minimap_bake` bake drawn as the isometric diamond in the selected realm's style; a save's map shows its realm's emblem, TODO) with size, water, woodland, trade partners (TODO M10), hazards; the Blank map's width, height and terrain; starting date (the realm's eras), goals (Sandbox, TODO M9), treasury, difficulty, four switches. New Game and its kind are gone; MainGame's button opened Custom Game (P1e removed it).
- `crate::custom_game`: Start replaces the session, then sets date, treasury, the risks switch and the active custom game: its realm (`ui.realm` source `custom_game`), map, difficulty, fast construction (`fast_construction_scale` 0.25 on top of `time_scale`) and the stored switches. Design: runtime state saved in the save header (`realm`, a `custom_game` record) and read back on load, so no save-version bump.
- Engine: hidden labels on dropdowns and lists, dropdown widths, toggle tooltips, `UiImageFit::Center`.
- Deviations: Kambara's and Tampadesa's campaigns gain one era each (Suryadeva, Pugara), for their dates; the user should confirm. Water and Woodland stand in for the mockup's Rivers and Fertile land. Presets are the debug maps (their cheats left off). Seasons, raids: disabled; difficulty: stored only.
- Verified: build, tests, web check, `--check`; CI scenarios at 1440 × 800, `ui_menus` also at 720p and 1080p; determinism twice; `ui_snapshot` per realm; `ui_build` replay; screenshots against mockups.

**Notes (2026-11-12, settings) (commit `8a99406`):**
- `dialog/settings/`: one `MainSettings` with tabs Game, Sound, Graphics, Controls; `GameSettings`, `SoundSettings`, `GraphicsSettings` open it on their tab. Rows: label left, toggle, slider or dropdown right, a rule under each, each tab's rows spread over the room. Pending values and the tab live in `settings::STATE`, so a rebuild (realm, language, window resize) keeps them. Ok writes changed values into `GameConfigs`, applies volumes, textures, autosave and language at once (UI scale next frame) and saves the user configs, except in scripts, replays and the smoke test. Choosing Wgpu also picks Winit.
- Configs: `display` (UI scale, colour-blind overlays, animated water), `camera.edge_scrolling`, `sim.default_speed`, `engine.vsync`, `frame_rate_limit`, `mute_in_background`, a master volume (Web Audio applies volumes once, on its tracks). Recordings note whether autosave may be switched on.
- Deviations: the UI scale scales the realm themes' text and dialog sizes, not the HUD or other point sizes (TODO); Settings' contents grow as far as the window lets the dialog; no dialog shrinks below its 100% size on 1280 × 720. VSync, frame rate limit, mute in background, colour-blind overlays and animated water are stored only (TODOs). Autosave interval is its own row; restart-only settings say so in their label; Controls lists the hard-coded keys.
- Verified: build, tests, web check, `--check`; CI scenarios at 1440 × 800, `ui_menus` (no scrolling on any tab, 75% and 200% scale) also at 720p and 1080p; determinism twice; `ui_snapshot` per realm; `ui_build` replay; screenshots against mockups.

**Notes (2026-11-11, load / save) (commit `f80d9bf`):**
- `dialog/load_save.rs`: one `LoadOrSaveGame`; `LoadGame`/`SaveGame` open it in load or save mode. Rows (realm emblem, "Campaign I · mission 2 · <place>", save time, era date), preview (thumbnail, details), Save as field (accent outline when focused), Delete with a popup, Overwrite popup. Rebuilt when saves change (`save_header::revision`) or the window rescales, keeping the pick; the autosave timer pauses while it's open. Home rebuilds on that revision too, so deleting Continue's save disables it.
- `save_header.rs`: a `save_header` object first in the file, all fields optional (no version bump); one this game can't read is blank. Read from the file's first and last 16 KB. Time played: a `#[serde(skip)]` session counter. Thumbnails: `saves/<name>.jpg`, the scene at `SaveGameConfigs::thumbnail_size` (`ScreenshotScale::Cover`, JPEG), deleted with the save or when a save writes none; none on web (TODO).
- Engine: `UiTextInput` look/width/hidden label, list rows' second column and `set_current_item`, rows recording their texts, save file head/tail reads. Commands `session.save_info`, `session.delete_save`.
- Deviations: scripts, recordings and replays leave the save time out, so `determinism.json` matches across runs and its replay (rows show "—"). No city or custom map names yet (the mission's name stands in; TODO for P1d's map); outside a campaign the realm is Đại Long. The title keeps the skin's dialog separator.
- Verified: build, tests, web check, `--check`; CI scenarios at 1440 × 800, `ui_menus` also at 720p and 1080p; determinism twice and replayed; `--smoke-test-saves`; `ui_snapshot` in each realm; screenshots against the mockups.

**Notes (2026-11-11, home) (commit `938eba8`):**
- `dialog/home.rs`: the crest (skin `ornaments`), title, campaign subtitle, the campaign row (an emblem per campaign, `Home/Campaign II`), menu items with sublines, the version. Continue loads `profile.last_save` (subline from `session::probe_save`); disabled with a tooltip when the save is missing, gone or of another version. Campaign resumes the playable campaign at its next mission, or opens the new `CampaignPicker` (`UiCard`, skin `cards`) when two can be played. Custom Game disabled.
- Home and the picker rebuild on a profile change, when the home menu opens and on a rescale (the picker keeps its pick). Home's contents scale with its content area (`skin::dialog_content_scale`); the picker's cards, buttons and text grow between 1280 × 720 and 1080p. A dialog over Home dims the backdrop (`dim_home_dialog`). Dialogs take an `offset`.
- Art (ui-art): Đại Long's crest, a seal with Đ; Kambara's and Tampadesa's backdrops, 1920 × 1080 JPEGs.
- Deviations: `campaigns.json` gains Campaigns II and III without missions ("coming soon"; finishing Campaign I reaches II): the user should confirm. New Game stays (P1d). Continue's subline names the campaign, not the mission number. No vines, blur, last-played line, quote or distinctions; upright where the mockups are italic (no italic faces). Kambara's Vietnamese fallback is Spectral, not Spectral SC (small capitals). Home is 1000 points tall. `ui_menus` leaves `saves/ui_menus.json`.
- Verified: build, tests, web check, `--check`; CI scenarios at 1440 × 800, `ui_menus` also at 720p and 1080p; `ui_snapshot` in each realm (layout issues 0); `ui_build` replay; screenshots against mockups.

### Phase 2: World map

**Delivers:** the world map screen (`world-map-<realm>.jpg`), opened from the minimap's World map tool (enabled at last), from the intermission, and from the HUD. Trade and diplomacy show their data read-only until M10 makes them act.

**User decisions (2026-11-12):**
- Kambara's and Tampadesa's maps are drawn now, from the mockups' content (Campaigns II and III have no missions yet), so every realm's screen is real.
- The map opens from the minimap's World map tool (enabled) and, as a stopgap, a "World map" entry in the in-game Game menu (`MainGame`); the intermission (Phase 3) and the HUD (Phase 5) get `TODO(realm-ui)` hooks.
- **Pan and zoom:** the map fits its panel by default (the whole map, as in the mockup). The wheel zooms in about the cursor and back out to the fit, never past it; zoomed in, dragging the map with the left button pans it, clamped so the map always covers the panel. (Edge panning at first, as the camera's edge scrolling does; after Phase 2 the user asked for click-and-drag instead, since the cursor easily left the map while panning at its edges.) Markers, labels and routes are drawn live at their screen size (positions follow the zoom, sizes don't). Automation reads and sets the view (zoom, centre), and the scenario checks zoom and pan.
- A **hidden test campaign** in `campaigns.json` (`hidden: true`, never listed by the menus; only automation reaches it, through `profile.set` or `session.start_mission`) holds a mission with choices until Campaign I's missions 6–8 exist. P2d added one per realm (`test_choices`, `test_choices_kambara`, `test_choices_tampadesa`).

- [x] **Data:** its own file per campaign, `assets/configs/game/world/<campaign>.json` (`crate::world_map::config`, loaded once after `campaigns.json`). It holds:
  - **cities:** id, name, position, kind (capital, city, monument, village, camp), owner realm or power, role, label side, and a trade partner's static trade data;
  - **routes:** kind (river, sea, land), points, state, the cities they join;
  - **region labels**, uncharted areas and threats, the compass and the choice box.
  - Player cities come from the profile's finished missions: each mission names the city it founds (`city` in `campaigns.json`).
- [x] **Base art:** the map without markers, rendered per campaign by `ui-art` from `world_map.py`'s layers (sea, land, rivers, relief, frame) at 2×.
- [x] **Markers and labels** drawn live over it, so a city joining the map needs no new art:
  - cities and their seals or lotus or stupa marks, in the realm font;
  - routes by kind and state;
  - the next mission's A/B choice sites;
  - the compass.
- [x] **Top bar:** title, realm subtitle, date and season, treasury, "Return to the city".
- [x] **Side panel for the selected city:**
  - emblem, name, role;
  - relations meter;
  - route line;
  - "They sell" and "They buy" with prices;
  - the court's request box;
  - "Set trade limits" and "Send a gift" (disabled, "coming with trade" until M10).
- [x] **Legend bar.**
- [x] **Openers and pan and zoom:** the minimap's World map tool (enabled) and the Game menu's World map entry open it; `TODO(realm-ui)` hooks where the intermission and the HUD will. Pan and zoom as the user decided (above): the fit at first, the wheel in about the cursor and back to the fit, dragging with the left button to pan zoomed in (edge panning at first; replaced after Phase 2's review at the user's request, since the cursor easily left the map), the map always covering its panel; marks and text at their screen size.
- [x] **Next mission choice:** the choice box with both sites. Picking one starts that mission's map. Only missions with `choices` in their config show it. Campaign I's missions 6–8 get them in Phase 2 of the game; until then a test mission in config exercises it.
- [x] **Hooks for M10:** `WorldMapProvider` gives trade data (prices, limits, requests, relations) when trade exists. Until then the panel reads the config's static values.
- [x] Scenario `ui_world_map.jsonl`: open it in each realm, select a city, pick a choice site in the test campaign, return.

**Notes (2026-11-12, end-of-phase review):**
- Fixed: the world map's next step is the one straight after the mission being played, finished or not, as `advance_to_next_mission` goes (the profile's first unfinished step only with none being played); `campaigns.json` validation requires a choice point to have a site starting it itself, which the campaign resumes at with no pick; `CampaignDef::site_mission`, `site_starting` and `step_mission` replace the copies of the site lookup; a world map file that fails is read and reported once, naming every campaign that shares it. `world_map.state`'s `next_mission` reports `open` and each site's `phrase`.
- The wheel zooms one step a wheel event at most: a browser's notch (100 pixels, 5 lines or more) jumped to the maximum. The choice box's subtitle reads `{{` and `}}` as braces, as `text::format` does (unit tests). The city isn't drawn under the full-screen world map (`dialog::covers_window`), except for a capture of the scene (a save's thumbnail). Markers' values are built only while the widget registry is on, and the map's text looks are resolved once. The minimap's tooltips follow a language switch. Popups size their buttons with `skin::size_popup_button`.
- Left: the HUD and the minimap are still updated under the world map (their widgets stay in `ui.tree`), and its rebuild key is formatted each frame; `ui_snapshot.jsonl` keeps the plain map only, since `ui_world_map.jsonl` screenshots the choice box and both popups.

**Notes (2026-11-12, next mission choice and wrap-up) (commit `012257c`):**
- The choice box, live in the view's corner where the world file puts it at the fit (it stays there zoomed), with dotted lines to the A/B sites. A site, or its name in the box, opens `Choose Site Popup`; Begin starts its mission (`campaign::start_mission_if_loadable`, shared with `session.start_mission`) and records the pick (`CampaignRecord::choices`, left out while empty: no version bump; a variant started records its site too). Until the mission being played is finished, or over another game, a site opens `Finish Mission Popup` instead; MissionComplete's Next Mission opens the map when the next step offers a choice the map shows (now the debrief's Continue to the world map, P3b).
- Hidden test campaigns per realm; a choice point may word its line (`choice_text`), a site its name in it (`phrase`). `session.state` gains `campaign_id` and `mission_id`. Under a dialog the minimap and the camera's edge scrolling ignore the pointer (`covers_hud`). The minimap's tooltips and lens names are text keys.
- Deviations: no italics; Kambara's stone and leaves are a flat fill; the box grows leftwards for longer titles; the home menu's Campaign starts a choice point itself while nothing is picked (`TODO(realm-ui)` at `Profile::next_mission`, Phase 3). Left from earlier steps: Tampadesa's rows cut, the window-only scale, the frame over the top bar, ships under routes, label angles, placeholder cities.
- Verified: build, tests, web check, `--check`; CI scenarios; `ui_world_map` at 720p, 800p and 1080p; determinism twice; `ui_snapshot` per realm; `ui_build` and determinism replays; screenshots against the mockups.

**Notes (2026-11-12, world map screen) (commit `a4eea47`):**
- `dialog/world_map/`: `WorldMap`, full screen, the 1920 × 1080 composition scaled to the window: top bar, map, side panel, legend. The map is the UI realm's campaign's (`screen_campaign`); selection and view live in `STATE`. Built in full once opened. `overlay.rs` draws labels, uncharted and threat words, routes (by kind and state), foreign rings, the player's marks with Đại Long's seal initials, A/B sites and the compass at screen size, in the skin's `world_map` looks.
- Engine: `UiMapView` (fit, wheel zoom, edge pan, overlay, a frame past the view, records), `UiFrame`, `UiCanvas`, `UiPainter::label` and `unclipped`, dashed and dotted lines, heading font and letter spacing on `UiRichText`.
- ui-art: marks, compass, coin, emblems per realm; Đại Long's request seal (`request_seal`). Openers: the in-game minimap's tool (the developer editor's stays disabled), `MainGame/World map` (closing the Game menu); TODOs in MissionComplete and the HUD bar. `world_map.state` gains `screen`; `world_map.set`. Scenario `ui_world_map.jsonl` (CI).
- Deviations: mockup sizes are ems (`skin::font_em`), so Tampadesa's tall rows are cut and the panel closes up when it would overflow; text keeps the window's scale at any UI scale; no italics; the frame overlaps the top bar's edge; non-trade cities show a note; the Game menu grows to 920 points; the choice box waits for P2d.
- Verified: build, tests, web, `--check`; CI scenarios at 1440 × 800; `ui_world_map` at 720p, 800p, 1080p (layout 0 at 75/100/200% in each realm); `ui_snapshot` per realm; `ui_build` replay; screenshots against mockups.

**Notes (2026-11-12, world map data) (commit `e6af595`):**
- `crate::world_map`: `WorldMapConfigs` loads `game/world/<name>.json` for every campaign (`world_map`, else its id) and validates it; `WorldMapProvider` and its `ConfigWorldMapProvider` give the player's cities (the file's `player` ones, finished missions', the current one's), the others, routes once their cities show, a city's panel (relations, route line, sells and buys, the court's request) and the next mission's choice. Values come from the generator, in its 1000 × 672 map space.
- `campaigns.json`: missions gain `id`, `city` and `choices` (sites A and B, each starting the choice point itself or a variant); a step is a mission that isn't a variant, and numbering, `next_mission`, finishing and advancing go by steps. Campaign I's missions found Phù Sa, Bến Gốm and Thần Khê. The hidden `test_choices` campaign, last in the file, shares Campaign I's map. The save header's `city_name` and the Game menu's place line take the mission's city. `world_map.state`.
- Deviations: the mockups' placeholder cities for unwritten missions (Đê Son, Chợ Đèo; Campaign II's four, Campaign III's two) are realm cities, not the player's, until those missions exist (only capitals, monuments and Kyautthin are `player`), and a trade route from one runs as the mockup shows (Hamsara's from Pashchima), where Vijara's waits for Bến Gốm; Kambara's and Tampadesa's choice sites wait for their missions; labels have no angles (the generator draws none).
- Verified: build, tests (new `world_map` test), web check, `--check`, CI scenarios at 1440 × 800, determinism twice, a live `world_map.state`. No screen yet.

**Notes (2026-11-12, world map base art) (commit `2f30edf`):**
- ui-art draws per realm `world_map_base` (2580 × 1734 JPEG, 0.5–0.6 MB: the generator's map viewport at 2×, from `world_map.py`'s new `base_map`: sea, land, terrain, uncharted hatching, rivers, lakes, relief, ships), Tampadesa's `world_map_threat` (the storm clouds, a PNG anchored on the threat's position), `world_map_frame` (PNG, the viewport transparent, from `map_frame`) and `world_map_backdrop` (the screen's wood, stone or bricks, 1920 × 1080). `map_layer` and `threat` draw through the same new methods; the mockups' HTML is byte for byte unchanged.
- Coordinates: a world-file unit is 2.58 base pixels (1.29 points × 2), from the base's top left; the 1000 × 672 extent covers 2580 × 1733.76 pixels. The skin's `world_map` block records it (`extent`, `pixels_per_unit`, `frame_map_rect`), with `skin::world_map()` and `SkinWorldMapDef::base_pixel`.
- Deviations: the frame is separate, so zooming moves only the map; the ships are base art, so the routes drawn live cross them (the mockup draws them over the routes); the uncharted areas' shapes are base art (their world-file `outline` must match it), their and Western Baray's text the screen's.
- Verified: each base diffed against its mockup's viewport (only labels, markers, routes, compass, choice box and frame rules differ), composites beside the mockups; tests `world_map` (a known city per map lands on the generator's pixel), `ui_skins`, ui-art's `test_world_map_art.py` (every city against the generator); build, web check, `--check`, CI scenarios at 1440 × 800.

### Phase 3: Intermission and briefing

**Delivers:**
- **The debrief** after a mission (`intermission-<realm>.jpg`), replacing MissionComplete;
- **the finished stele** at the end of a campaign, replacing CampaignComplete;
- **the briefing** before a mission.

- [x] **Stele, left:**
  - the stele art per realm (from `ui-art`), with text set live;
  - the campaign's stele title;
  - one carved line per finished mission (from mission config: `stele_line`, plus a line per distinction earned);
  - the newest line highlighted and freshly marked;
  - empty rows for the missions to come.
  - "Carved" text is the realm font drawn twice, with an offset shadow and highlight from skin colours.
- [x] **Panel, right:**
  - "MISSION N COMPLETE", the name, the city and its era year;
  - goals table, target and achieved, from the mission's requirements;
  - distinctions: hidden until M9 brings them;
  - ratings: four meters, hidden until M9;
  - stats: population, treasury, years taken;
  - the "joins the world map" note;
  - "Keep playing" and "Continue to the world map". The second goes to Phase 2's screen, or to the next mission when no world map is configured.
- [x] **Campaign complete:** the whole stele, a closing line, "Return to the menu". The profile marks the next campaign reached, which switches the home realm.
- [x] **Briefing** (`briefing-<realm>.jpg`): a scroll or tablet with the mission's name, its stele lead-in, its text (the mission's `briefing` Story key; `description` stays the developers' note, shown only for a mission without one), its goals, and an image slot. "Begin". It opens when a mission's map loads and from a HUD button during the mission.
- [x] **Flow:** `campaign::take_pending_prompt` opens the debrief. The `MissionComplete` and `CampaignComplete` kind names stay, so the snapshot scenario still finds them.
- [x] Scenario `ui_intermission.jsonl`: finish a test mission (by meeting its goals with `world.*` and `time.fast_forward`), check the debrief's goals, continue, finish the campaign, and check that the profile reached the next one.

**Notes (2026-11-12, stele art and mission data) (commit `e76e91d`):**
- ui-art draws per realm `stele` (the slab without its text, 640 × 950 points at 1.5×), `stele_niche` (`stele_backdrop`, at its screen place) and `stele_mark` (`fresh_mark`); the screen's background is the world map's backdrop and the briefing's tablet `panel_dialog` (the same drawings). Skin blocks `intermission` (sprites, rects and bleeds, the carved text's geometry, `title`, `line` and `newest` looks, `rule` and `row` colours) and `briefing` (panel, image outline).
- Carved text: `UiLabelLook` gains a `highlight` beside its `shadow` (each realm's offset copy: Đại Long's and Kambara's light lip, Tampadesa's dark cut), `skin::label_look`, and `UiPainter::wrap_label`. `menu::stele::SteleArt` lays out the title, rule, lines, the newest line's mark and the empty rows, setting lines smaller (to half) to fit above the slab's bottom.
- Data: `stele_title`, `stele_closing` per campaign; `stele_line`, `lead_in`, `briefing` per mission (Story keys, validated), `distinctions` for M9. Campaign I's three missions written after the generator's lines; test campaigns get placeholders and their realm's title.
- `campaign::stele` (lines per finished step, a variant taking its choice point's, newest, rows to come), `intermission.state`, the Skin Gallery's Intermission section (partly carved, so the empty rows show).
- Deviations: one stele size; the campaign-complete screen (wider in the mockup) will scale it. Wrapping is measured, so title breaks differ from the generator's.
- Verified: build, tests (`campaign`, `world_map`, `ui_skins`), web check, `--check`, CI scenarios, determinism twice; the gallery in each realm beside the mockups.

**Notes (2026-11-13, debrief) (commit `ccca777`):**
- `MissionComplete` is the debrief, full screen (`dialogs.MissionComplete`), laid out from `intermission()`'s 1920 × 1080 coordinates with the world map's scales, now shared in `dialog/screen.rs`: the stele (a canvas: `SteleArt` on its niche, carved from a profile copy that already has the mission, since the profile records it a frame later) and the panel on the skin's `intermission.panel`: kicker, name, city and era year, goals (`UiTable` with `UiTableHeader`, letter-spaced, per-column colours and `UiTableStatusLook::Badges`), stats (`UiKeyValues::with_columns`, rows capped as the mockup's for Amita's tall line), the "joins the world map" note, Keep playing and Continue. Colours: `intermission.colors`, mostly the realm's `text` names.
- Continue: the world map when it shows the campaign and offers the next mission (`WorldMapProvider::next_mission`: the A/B choice, or once the mission is finished "Next: <mission>" with `Begin the mission` and `Begin Mission Popup`); else the next mission at once; after the last step, CampaignComplete (P3c restyles it). `TODO(realm-ui)` for Phase 4's sequence and M9's distinctions and ratings.
- Deviations: the space for distinctions and ratings sits between the goals and the stats; goals are the requirements against the stats (not the mockup's trade, requests, ratings); the note names no role; layout follows the window only, as the world map's. The choice-point TODO at `Profile::next_mission` stays for the briefing (P3d).
- Verified: build, tests, web check, `--check` and its tests, CI scenarios, determinism twice; `ui_intermission.jsonl` (new, in CI) at 1280 × 720, 1440 × 800 and 1920 × 1080; screenshots in three realms beside the mockups.

**Notes (2026-11-13, campaign complete and briefing) (commit `2e77b12`):**
- `CampaignComplete` is full screen in the debrief's style: the stele carved in full (`SteleArt::stretched` to the mockup's wider rect, lines clear of the rule: `intermission.complete`), "CAMPAIGN I COMPLETE", name, realm and date, the `stele_closing` line, the missions (`UiTableHeader::hidden`), Missions and the last city, the next campaign's note with its emblem, Return to the menu (the home realm follows the profile, which reached Campaign II).
- `MissionBriefing` (new kind) on the skin's scroll or tablet, laid out from the 1520 × 980 mockup panel at the dialog's size: kicker, name, city and year, the world map around the city as a placeholder image, the mission's new `briefing_new` and `briefing_threats` (Story keys), lead-in, text, goals, Begin. It opens when a mission starts under the in-game menus (`campaign::request_briefing`), paused until Begin, and from the Game menu's Mission briefing (campaign missions only). A choice point with no pick offers its two sites in the opening briefing only (Escape plays the current site), resolving the `Profile::next_mission` TODO.
- Deviations: no span of years, distinctions, largest city, epilogue or Replay the story (`TODO(realm-ui)`: profile records, M9, Phase 4); dimmed, not blurred, city; stretched stele art. No scenario checks that `dialog.open MissionBriefing` fails outside a mission (script mode stops at a failure).
- Verified: build, tests, web check, `--check`, CI scenarios, determinism, record and replay; `ui_intermission.jsonl` at 1280 × 720, 1440 × 800 and 1920 × 1080 (the choice briefing too, in each realm and scale); screenshots beside the mockups.

**Notes (2026-11-13, Phase 3 wrap-up) (commit `7f8f8a3`):**
- Reviewed Phase 3 against the plan and the mockups: every item is in, Phase 4's hooks included (`TODO(realm-ui)` in `GameSession::start_mission` and the debrief's `on_continue`).
- `ui_intermission.jsonl` now plays the whole flow: Home's Campaign, Campaign I's three missions for real (briefing, debrief, world map, the next briefing; mission 3's goals met with world edits), the campaign-complete stele and the home menu in Kambara's look; then Kambara's and Tampadesa's hidden test campaigns in their own looks (briefing, debrief, the map's site A or B, the last debrief, the stele, Home).
- Left as they are (in known-issues): stele lines wrap wider than the generator's estimates; the campaign-complete stele sets few lines at full size; a mission without goals leaves the goals' room empty; M9's distinctions and ratings, Phase 4's story and epilogue, Phase 5's HUD button; no blur, vines or italics; no scenario checks that `dialog.open MissionBriefing` fails outside a mission (no assert for an error).
- Verified: build, tests, web check, `--check` and its tests, every CI scenario, determinism twice, record and replay; `ui_intermission.jsonl` at 1280 × 720, 1440 × 800 and 1920 × 1080; `ui_snapshot.jsonl` in each realm (layout issues 0); a final set at 1920 × 1080 beside the mockups: Campaign I's briefing, debrief and campaign complete under each realm's `ui.realm`, and Kambara's and Tampadesa's own flows through their test campaigns (`own_*`).

**Notes (2026-11-13, end-of-phase review fixes):**
- Deviation from the checklist: the briefing's text is the mission's `briefing` Story key, not `MissionDef.description`, which stays the developers' note (shown only for a mission without a briefing).
- Fixed: the debrief's Treasury stat reads the world stats, as its goals do; the stele's lines set smaller keep their gaps, as the generator's `stele_scale` does; the briefing records the other site's pick only once its variant has started (`Profile::mission_started`); the autosave waits while a briefing is pending or open; the campaign-complete note says a next campaign without missions is coming soon; `intermission.state` refuses `newest` with `full`. Shared: `screen::BuiltOnOpen` for the screens built on opening, one prepare hook per kind in `open_dialog`, `world_map::provider::step_choice` for the choice's wording, `Stele::canvas_value` and `state_json`.
- Tests: `test_briefing_request` (campaign), `test_intermission_art.py` (the skins' intermission geometry against the catalog), `intermission.state` in `ui_intermission.jsonl`, and no briefing (nor pause) after `ui_menus.jsonl`'s Continue.
- Left (in known-issues, `TODO(realm-ui)` at `on_keep_playing`): nothing opens the debrief again after Keep playing, so the campaign-complete screen is lost after the last mission. The picker's Replay a mission moved to Later.

### Phase 4: Story comics

**Delivers:** the comic viewer (`intro-<realm>.jpg`), with sequences I-A (before mission 1) and I-B (after mission 5) of story-script.md as data with placeholder panels. Every other sequence is data only, later.

- [x] **Data:** `assets/configs/story/<campaign>.json`. Sequences have an id, a title and a trigger ("before mission N", "after mission N", "epilogue"). Panels have:
  - an image;
  - a narration caption;
  - bubbles: speaker, text, kind (speech, whisper, group, bold), anchor and tail point;
  - an optional sepia flag for flashbacks.
- [ ] **Viewer:**
  - the full-screen panel in the realm frame;
  - the caption on the skin's `caption` panel;
  - bubbles painted (rounded panel and tail) or 9-sliced from the skin's `bubble`;
  - **bottom bar:** campaign line, sequence title, panel dots (done, current, to come), "Panel N of M", Skip, Next.
  - Keys: Space, Enter and Right go forward; Left goes back; Escape skips.
- [ ] **Triggers:** the campaign flow plays a sequence before a mission's briefing or after its debrief, as its trigger says. Each sequence plays once per profile, and Home gets "Replay the story" when one has been seen.
- [ ] **Dynamic panels:** a panel can list the player's stele lines (I-E, the epilogue).
- [x] **Placeholder art:** `ui-art` makes each panel from the realm's scene drawing and frame, with the panel's description from the script as a dim caption. Real art replaces a file of the same name.
- [ ] Scenario `ui_story.jsonl`: start campaign I, step through I-A, check the panel count and that Skip lands on the briefing.
- [ ] **Debug screens menu** (the user, 2026-11-13): a menu in the developer editor's menu bar that opens the retail screens that are otherwise hard to reach (the briefings, the debrief and campaign-complete screens, the world map's next-mission and choice states, every story sequence) for a campaign and mission picked in it, without changing the profile or starting a session unless the item says so; closing the screen returns to the editor.

**Notes (2026-11-13, story data and panel art) (commit TODO):**
- Data: `assets/configs/story/<campaign id>.json` (`game::story`: `StoryConfigs`, loaded after `campaigns.json` and checked: ids, triggers, duplicate triggers, placements on the 1800 × 870 panel, keys, pictures; errors on the `campaign` channel). Triggers (`before_mission`, `after_mission`, `epilogue`) name missions as `campaigns.json` does; one named for a choice point also plays for its variant; an index past the missions waits (I-B, after Campaign I's unwritten mission 5). I-A (6 panels) and I-B (4) are Story keys, speakers Realm keys; the hidden test campaigns have `test_before` (3 panels, one sepia) and `test_after` (2), and `test_choices` a `test_epilogue` with a `stele_lines` panel, every bubble kind among them.
- Provider: `before_mission`, `after_mission`, `epilogue`, `sequences_in_order`, `seen_sequences` (Replay's order), and `story::to_play_before/after` for P4c. Profile: `CampaignRecord::sequences_seen` (left out while empty, no version bump), `Profile::sequence_seen`, `profile::on_sequence_seen`; `story::has_replay` decides Home's Replay; `profile.set` checks the ids. A story file named for no campaign is an error. Automation: `story.state`.
- Art: ui-art's `story_panel` (the realm's `intro_scene` with a dim caption: label and the panel's `visual`), JPEGs at 1 pixel a point under `assets/ui/story/` (180-270 KB each), one per Campaign I panel and one per realm for the test campaigns; `story_frame` per skin (the frame with the panel cut out, also at 1 pixel a point: 110-300 KB) for P4b, named by no skin yet. `test_story_art.py` checks captions match `visual`.
- Verified: build, tests (new `story`, `realm_profile`, `world_map`, `ui_skins`), web check, `--check` and its tests, CI scenarios, determinism twice; the panels beside `intro-<realm>.jpg`.

### Phase 5: HUD

**Delivers:** the in-game HUD as in `hud-<realm>.jpg`, with every old sprite gone.

> **Before starting (the user, 2026-11-13):** the tile palette (the build bar's child menus) is to show a custom icon for each building instead of its text label, a deviation from the build bar item below. New mockups for it are to be designed with the user first, so Phase 5 doesn't start until they've been drawn and agreed. Remind the user of this when Phase 5 comes up.

- [ ] **Top-left bar:** speed controls (pause, play, fast, faster) and the date in the realm's era with the season under it. This replaces SpeedControlsBar and the TopBar's date.
- [ ] **Top-centre bar:**
  - population, treasury and food, each with its icon;
  - four rating badges (ring meters with the realm's Legitimacy initial), showing "—" until M9.
- [ ] **Game menu buttons:** Game, Load/Save, Settings, City and the briefing. The HUD mockup has no place for them; `hud-menus-<realm>.jpg` puts them in their own bar left of the build bar.
- [ ] **Goals tracker,** left: the mission name, each goal with its tick, text, progress and bar, live from the mission's requirements. Collapsible. It's hidden in custom games without goals.
- [ ] **Build bar,** right:
  - the mockup's column of category buttons in the realm style;
  - the hovered button's tooltip (title and subtitle);
  - child menus as skin lists with each building's icon, cost and materials, greyed out with the reason when unaffordable (the construction plan's tooltip);
  - clear, roads, undo and redo kept (see [open decisions](#open-decisions) on categories).
  - The `TilePaletteMenu/<sector>` paths and their tooltips stay, so `ui_build.jsonl` keeps working.
- [ ] **Message log,** bottom centre: the three latest messages with type icons, the newest bold with "now".
  - Until M11 it shows the notices the game already raises: construction completed, minimap alerts, saves.
  - It goes through a small `MessageLog` store that M11 replaces with its typed messages.
  - Toasts stay for screenshots and the like.
- [ ] **Inspector:** the building, unit, prop and terrain inspectors on the skin's side panel or card (`inspector-<realm>.jpg`). The `BuildingInspector/Close` paths stay.
- [ ] **Minimap:** its frame, tooltips and button folder come from the skin, so `assets/configs/ui/minimap/<realm>.json` keeps only the map's own settings.
- [ ] Delete the old sprites: `buttons/menu_bar/*`, `buttons/palette/*`, `icons/{population,gold,player}_icon.png`, `misc/{wide,tall,square}_page_bg.png`, the brush strokes.
- [ ] Scenarios:
  - `ui_build.jsonl` updated;
  - `ui_hud.jsonl`: speed controls, goals progress after placements, the message log after a building completes, all in three realms.

### Phase 6: City management pages

**Delivers:** the City book, one dialog with a tab or page per area, replacing CityManagement and its three text dialogs. Mockups: `city-<page>-<realm>.jpg` (see [Mockups](#mockups)).

- [ ] **Page framework:** the City dialog with side tabs (`UiTabBar`, vertical).
  - Each page is a `CityPage` impl: title, icon, build, refresh.
  - A page registers itself, so M4, M9 and M11 each add theirs without touching the others.
  - Pages refresh on open and once a game month.
- [ ] **Overview:** population, treasury, food months, employment, rating badges, warnings (no water, no food, unemployment).
- [ ] **Population:**
  - housing by level (counts, with the next level's needs);
  - workforce: employed, unemployed, by sector;
  - a monthly trend line.
  - A `CityHistory` ring buffer is sampled each game month. It's runtime only, rebuilt from zero after a load, until a milestone needs history in saves.
- [ ] **Goods:** storage yards and granaries by good, grouped (foods, consumer goods, raw materials) with icons, capacity bars and monthly change. This replaces the Resources dialog and fixes its overflow (goods.md).
- [ ] **Finances:** treasury, tax income, costs. M4 fills in the yearly accounts table (this year against last), wages and tax rates.
- [ ] **Heritage** (`city-heritage-<realm>.jpg`): faith and monuments (GDD 5.11 and 5.15), in three sub-tabs (`UiTabBar` inside the page; chosen over a single page on 2026-11-08 so each part has room to grow):
  - **Monument:** for missions that have one. The monument and its site, its stages (built, in progress, to come), the current stage's materials delivered and specialist workers against what it needs, the time left at the current rate, and its bonuses when finished. Missions without one show the tab disabled, with "No monument in this mission".
  - **Patrons:** the realm's 3–5 patrons, each with its domain, favour, mood, and the blessing or misfortune its favour is heading towards.
  - **Festivals:** the festival year on a month strip, the realm's festivals with their patron and status, and a planner for the selected one: venue, scale (modest, grand, royal), costs against stock, effects, and a Hold button. The mockup shows this tab.
  - The page and its tabs exist from Phase 6 with whatever data the game has; faith and festivals (M8) fill in Patrons and Festivals, and monuments (game Phase 2) fill in Monument (see [Dependencies](#dependencies-on-other-milestones)).
- [ ] **Hooks:** the pages M9 (Ratings, with what drives each), M11 (Labour, Food, advisors' notes) and later milestones (Trade, Military, the Court) add. Each milestone's plan gets a checkbox for its page.
- [ ] **Paths:** `CityManagement` stays as the dialog kind, with the page names as tabs. `PopulationManagement`, `ResourcesManagement` and `FinancesManagement` open the City dialog on their page, so `ui_snapshot.jsonl`'s `dialog.open` calls still work.
- [ ] Scenario `ui_city.jsonl`: open each page in each realm after building a small town; check the numbers against `world.stats`.

## Dependencies on other milestones

| Screen | Works now with | Filled in by |
|---|---|---|
| Home, Continue, Campaign | Profile (Phase 0) | Campaigns II and III configs (game Phases 3 and 4) |
| Custom Game options | Fast construction | Seasons and floods (M7), fires (M6), raids (game Phase 2), difficulty (M4, M9) |
| World map | Config cities, routes, choices | Trade, prices, limits, requests, relations (M10); sea routes and gifts (game Phase 2) |
| Intermission | Goals from requirements, stats | Distinctions and ratings (M9) |
| Briefing and comics | Mission text, I-A and I-B data | Story events, the other sequences (game Phase 2) |
| HUD ratings, goals | Requirements progress | Ratings (M9), new goal types (M9) |
| Message log | Existing notices | Typed messages, popups for important events (M11) |
| City pages | Population, goods, treasury | Accounts (M4), ratings (M9), labour and food advisors (M11) |
| Heritage page | The tabs, empty | Faith and festivals (M8): patrons, favour, blessings, festivals; monuments and guilds (game Phase 2) |

The roadmap's convention stands: new retail screens go through the skins and pass `ui.layout_issues`. From Phase 0 on, a milestone that adds a screen builds it with this plan's widgets and skin helpers.

## Mockups

Every screen the phases build has a mockup in all three realms, in [docs/design/mockups/](../design/mockups/README.md). The first set came with the plan's research: the world map, the intro comic, the intermission, the HUD and the main menus (`world-map`, `intro`, `intermission`, `hud`, `menu-{home,load-save,settings,custom-game,about}`). The screens it found missing were drawn on 2026-11-07 (commit `8a44c00`):

| Screen | Phase | Mockup |
|---|---|---|
| Campaign picker (more than one campaign reached) | 1 | `campaign-picker-<realm>.jpg` |
| In-game Game menu and the Quit popup | 1 | `game-menu-<realm>.jpg`, `game-quit-<realm>.jpg` |
| Briefing scroll | 3 | `briefing-<realm>.jpg` |
| Campaign complete (the finished stele) | 3 | `campaign-complete-<realm>.jpg` |
| HUD game-menu buttons, the build bar's child menu and its tooltip with costs | 5 | `hud-menus-<realm>.jpg` |
| Building inspector | 5 | `inspector-<realm>.jpg` |
| City book: Overview, Population, Goods, Finances | 6 | `city-{overview,population,goods,finances}-<realm>.jpg` |
| City book: Ratings, Labour, Food | 6, for M9 and M11 | `city-{ratings,labour,food}-<realm>.jpg` |
| City book: Heritage (monument, patrons, festivals; added 2026-11-08) | 6, for M8 and game Phase 2 | `city-heritage-<realm>.jpg` (Festivals tab) |
| Important-event popup (flood, fire, court request) | 5, for M11 | `event-<realm>.jpg` |

The HUD's menu buttons got their own bar left of the build bar, not the left end of the top-left bar (Phase 5's default): the date leaves no room there.

## Performance

- Skins load once per realm switch: tens of sprites, a few MB.
- **9-slice:** 9 quads per panel, the same draw path as today's stretched sprites.
- **World map:** one textured quad for the base, plus tens of painted markers.
- **Comics:** one panel image loaded at a time, the next prefetched.
- **City pages:** the history buffer is 120 samples per series, sampled monthly.

## Open decisions

The user accepted every default on 2026-11-07, with the changes marked below.

| # | Question | Default |
|---|---|---|
| 1 | Body text in the realm font, as the mockups have it, or a plainer face for small text? | Realm font. A skin can set a plainer body face if small text reads poorly in testing. |
| 2 | Đại Long's home backdrop | Keep the painting, as the mockup does. Only the UI sprites go. |
| 3 | New Game (map size and terrain) on Home | Remove it. A Blank map entry in Custom Game takes its options. |
| 4 | Where the profile lives | `saves/user/profile.json`, beside the user configs; localStorage on web. (Changed in review.) |
| 5 | 9-slice centres | Stretch. `center: "tile"` per panel where grain shows. |
| 6 | Dialogs: each its own window, or stacked inside the root as today | Each its own window. |
| 7 | Build bar categories: the mockup's nine (house, water, farm, industry, trade, temple, learning, defence, monument) or today's sectors | Today's sectors with the mockup's icons. New categories come only with buildings that need them. |
| 8 | Where the game menu buttons go in the HUD | Their own bar, left of the build bar (`hud-menus-<realm>.jpg`). |
| 9 | The art renderer | Playwright's Chromium on the generator's SVG; Pillow stays for glyph icons. |
| 10 | Save thumbnails | A JPEG beside each save, taken at the save's camera position; its size is a config (default 320 × 180). Old saves show none. (Changed in review.) |
| 11 | Ratings and distinctions before M9 | Hidden, not shown as placeholders. |

## Later

- Animated home backdrops per realm. Play a short looping video in a simple format (for example VP9 or AV1 in WebM, decoded to a texture) rather than a sequence of still frames, to keep the size down; the `misc/home_menu_anim/` frames, removed in Phase 0, showed the idea. The web build can use the browser's own video decoder.
- Real UI sounds per realm, mapped to the role keys the skins already name.
- Hand-made art replacing the placeholders, file by file.
- Translations, into the language files the strings already live in.
- Key rebinding on the Controls tab.
- An encyclopedia (overview.md's content budget mentions entries).
- A mission list for the campaign picker's Replay a mission (disabled, "coming soon", until then): replaying any mission of a campaign already played.
