use std::path::{Path, PathBuf};

use engine::ui::{self, UiStyleSource, sound};
use game::{
    menu::{self, skin::{self, UiSkinDef}},
    realm::Realm,
    tile::minimap::style::HexColor,
};
use serde_json::Value;
use strum::IntoEnumIterator;

mod test_utils;

// ----------------------------------------------
// The realm skins (docs/plans/realm-ui.md, Skins; assets/configs/ui/skins/<realm>.json): every
// skin loads and merges over Đại Long's, every sprite, 9-slice and sound it names exists and fits,
// every colour it names resolves, and its fonts draw every Vietnamese letter. The skin module's own
// unit tests (`cargo test -p game --lib skin`) cover the merge rules, dialog sizes, the dialog
// entries and that the menu code names no sprite path.
// ----------------------------------------------

// ----------------------------------------------
// Glyph coverage of the UI styles (docs/plans/realm-ui.md, Fonts).
//
// Every text style of every style the game loads draws Đại Long's names and every Vietnamese
// letter (ui::GLYPH_TEST_TEXTS) with its font or, glyph by glyph, its own fallbacks, not the
// style's monospace or icon font; the icon text style draws the UI's icons
// (ui::check_glyph_coverage). The styles are the developer UI's and each realm skin's `style`
// block (assets/configs/ui/skins/<realm>.json, merged over Đại Long's).
//
// The skins' font sets follow these rules. The font names are the same in every realm: `title`
// (headings and buttons), `body` (body and small text), their Vietnamese fallbacks `title_vi` and
// `body_vi` where the realm font lacks Vietnamese letters, `mono` and `icons`.
// - Metrics are the ones egui reads: OS/2's typo ascender and descender if the font sets
//   USE_TYPO_METRICS, else hhea's, with the descent negative as stored; none of these fonts has a
//   line gap. Scales are units_per_em / (ascent - descent), the style files' rule. A font entry
//   gives `file`, `scale` and `y_offset_factor` together, since a skin is merged over Đại Long's
//   key by key.
// - A fallback takes its primary font's scale, so both have the same em, as the mockups draw them.
// - egui centres a fallback glyph's line height on the primary font's instead of lining up their
//   baselines, so a fallback's `y_offset_factor` moves it back onto the baseline: with equal
//   scales, ((ascent + descent) of the primary - (ascent + descent) of the fallback) / 2, in ems.
//   Kambara: Jaini 1048 / -266, Spectral SemiBold 1059 / -463, so 0.093. Tampadesa: Amita
//   1292 / -650, Alegreya 1016 / -345, so -0.015.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("UI skins", &[
        test_utils::test_fn!(test_every_skin_loads),
        test_utils::test_fn!(test_realm_skins_inherit_dai_long),
        test_utils::test_fn!(test_every_sprite_exists_and_slices_fit),
        test_utils::test_fn!(test_world_map_art_fits),
        test_utils::test_fn!(test_every_colour_and_widget_part_resolves),
        test_utils::test_fn!(test_every_sound_exists),
        test_utils::test_fn!(test_game_styles_cover_vietnamese),
        test_utils::test_fn!(test_realm_fonts_need_their_fallbacks),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}

// Every realm's skin, merged over Đại Long's as the game merges it.
fn skins() -> Vec<(Realm, UiSkinDef)> {
    let files = skin::read_skin_files().unwrap_or_else(|err| panic!("{err}"));
    let file = |realm: Realm| files.iter().find(|(file_realm, _)| *file_realm == realm).unwrap().1.clone();
    Realm::iter().map(|realm| (realm, skin::merge_skin(realm, &file).unwrap_or_else(|err| panic!("{err}")))).collect()
}

fn ui_asset(path: &str) -> PathBuf {
    repo_path("assets/ui").join(path)
}

fn ui_asset_exists(path: &str) -> bool {
    ui_asset(path).is_file()
}

const BUTTON_STATES: [&str; 4] = ["idle", "hovered", "pressed", "disabled"];

// A PNG's size in pixels, from its header.
fn png_size(path: &str) -> (f32, f32) {
    let bytes = std::fs::read(ui_asset(path)).unwrap_or_else(|err| panic!("assets/ui/{path}: {err}"));
    assert!(bytes.len() > 24 && &bytes[1..4] == b"PNG" && &bytes[12..16] == b"IHDR", "assets/ui/{path} isn't a PNG");
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as f32;
    (be(16), be(20))
}

// A 9-slice's borders (left, top, right, bottom, in pixels) leave room for its middle: on each axis
// the two borders together are narrower than the sprite.
fn check_slice(realm: Realm, what: &str, path: &str, slice: [f32; 4]) {
    assert!(ui_asset_exists(path), "{realm}: {what}: assets/ui/{path} doesn't exist");
    assert!(slice.iter().all(|border| *border >= 0.0), "{realm}: {what}: negative border in {slice:?}");
    let (width, height) = png_size(path);
    assert!(
        slice[0] + slice[2] < width && slice[1] + slice[3] < height,
        "{realm}: {what}: borders {slice:?} don't fit {path} ({width} × {height})"
    );
}

fn check(sources: &[UiStyleSource]) -> Result<(), String> {
    ui::check_glyph_coverage(sources, ui::GLYPH_TEST_TEXTS, ui::GLYPH_TEST_ICONS)
}

// The realm skins' styles, as JSON, by theme name.
fn skin_styles() -> Vec<(String, Value)> {
    skins().into_iter().map(|(realm, skin)| (skin::theme_name(realm), skin.style)).collect()
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

// Every skin loads and merges, its style validates, and it has what the helpers need.
fn test_every_skin_loads() {
    let skins = skins();
    assert_eq!(skins.len(), Realm::iter().count());
    for (realm, skin) in skins {
        UiStyleSource::json(skin::theme_name(realm), skin.style.clone()).validate().unwrap_or_else(|err| panic!("{realm}: {err}"));
        assert!(skin.sprite_scale > 0.0, "{realm}");
        skin::check_required(&skin).unwrap_or_else(|err| panic!("{realm}: {err}"));
        assert!(skin.panels.contains_key("caption"), "{realm}: no panel 'caption'");
        for role in ["primary", "secondary", "menu_item", "hud_item"] {
            assert!(skin.buttons.contains_key(role), "{realm}: no button role '{role}'");
        }
        for level in ["title", "subtitle", "dialog", "popup", "section"] {
            assert!(skin.headings.contains_key(level), "{realm}: no heading '{level}'");
        }
        assert!(!skin.backdrops.home.is_empty(), "{realm}: no home backdrop");
        assert!(skin.ornaments.contains_key("crest"), "{realm}: no crest for the home menu");
        for name in [&skin.cards.panel, &skin.cards.selected] {
            assert!(skin.panels.get(name).is_some_and(|panel| panel.slice.is_some()), "{realm}: no 9-slice panel '{name}' for the cards");
        }
        assert!(skin.panels.contains_key(&skin.toast.panel), "{realm}: no panel '{}' for the toasts", skin.toast.panel);
        assert!(skin.toast.scale > 0.0 && skin.toast.font_scale > 0.0, "{realm}: toast scales");
    }
}

// A realm's file holds only what it changes: what it doesn't name is Đại Long's.
fn test_realm_skins_inherit_dai_long() {
    let skins = skins();
    let dailong = &skins[0].1;
    for (realm, skin) in &skins[1..] {
        assert_eq!(skin.icons, dailong.icons, "{realm}");
        assert_eq!(skin.sounds, dailong.sounds, "{realm}");
        assert_eq!(skin.style["spacing"], dailong.style["spacing"], "{realm}");
        assert_ne!(skin.panels["dialog"].sprite, dailong.panels["dialog"].sprite, "{realm}");
    }
}

// Every sprite a skin names exists, and every 9-slice's borders fit its sprite (the PNG headers'
// sizes): panels, framed buttons in each state, the icon frame's selected state, separators, the
// slider's and scrollbar's parts.
fn test_every_sprite_exists_and_slices_fit() {
    for (realm, skin) in skins() {
        for (name, panel) in &skin.panels {
            match panel.slice {
                Some(slice) => check_slice(realm, &format!("panel '{name}'"), &panel.sprite, slice),
                None => assert!(ui_asset_exists(&panel.sprite), "{realm}: panel '{name}': {} doesn't exist", panel.sprite),
            }
        }
        for (role, button) in &skin.buttons {
            if let Some(stem) = &button.sprite {
                for state in BUTTON_STATES {
                    check_slice(realm, &format!("button '{role}' {state}"), &format!("{stem}_{state}.png"), button.slice);
                }
            }
            if let Some(selected) = &button.selected {
                check_slice(realm, &format!("button '{role}' selected"), selected, button.slice);
            }
            for path in [&button.hover_sprite, &button.marker].into_iter().flatten() {
                assert!(ui_asset_exists(path), "{realm}: button '{role}': {path} doesn't exist");
            }
            if let Some(hover) = &button.hover {
                assert!(skin.panels.get(hover).is_some_and(|panel| panel.slice.is_some()), "{realm}: button '{role}': no 9-slice panel '{hover}'");
            }
        }
        for (name, separator) in &skin.separators {
            match separator.slice {
                Some(slice) => check_slice(realm, &format!("separator '{name}'"), &separator.sprite, slice),
                None => assert!(ui_asset_exists(&separator.sprite), "{realm}: separator '{name}': {} doesn't exist", separator.sprite),
            }
            if let Some(ornament) = &separator.ornament {
                assert!(ui_asset_exists(ornament), "{realm}: separator '{name}': {ornament} doesn't exist");
            }
        }
        for (level, heading) in &skin.headings {
            if let Some(separator) = &heading.separator {
                assert!(skin.separators.contains_key(separator), "{realm}: heading '{level}': no separator '{separator}'");
            }
        }
        for (set, folder) in &skin.icon_sets {
            assert!(repo_path("assets/ui").join(folder).is_dir(), "{realm}: icon set '{set}': {folder} isn't a folder");
        }
        for (name, path) in &skin.icons {
            assert!(ui_asset_exists(path), "{realm}: icon '{name}': {path} doesn't exist");
        }
        for (name, stem) in &skin.emblems {
            for state in ["idle", "locked", "current"] {
                assert!(ui_asset_exists(&format!("{stem}_{state}.png")), "{realm}: emblem '{name}' has no {state} sprite");
            }
        }
        for path in &skin.backdrops.home {
            assert!(ui_asset_exists(path), "{realm}: home backdrop {path} doesn't exist");
        }
        for (name, path) in &skin.ornaments {
            assert!(ui_asset_exists(path), "{realm}: ornament '{name}': {path} doesn't exist");
        }

        let controls = &skin.controls;
        for path in [&controls.toggle.off, &controls.toggle.on, &controls.toggle.knob, &controls.checkbox.off, &controls.checkbox.on, &controls.slider.knob, &controls.dropdown.arrow] {
            assert!(ui_asset_exists(path), "{realm}: control sprite {path} doesn't exist");
        }
        for (what, part) in [
            ("slider rail", &controls.slider.rail),
            ("slider fill", &controls.slider.fill),
            ("scrollbar track", &controls.scrollbar.track),
            ("scrollbar thumb", &controls.scrollbar.thumb),
        ] {
            check_slice(realm, what, &part.sprite, part.slice);
        }
        assert!(
            skin.panels.get(&controls.dropdown.field).is_some_and(|panel| panel.slice.is_some()),
            "{realm}: no 9-slice panel '{}' for the dropdown",
            controls.dropdown.field
        );
        for (kind, dialog) in &skin.dialogs {
            if let Some(panel) = &dialog.panel {
                assert!(skin.panels.contains_key(panel), "{realm}: dialog '{kind}': no panel '{panel}'");
            }
        }
    }
}

// A JPEG's size in pixels, from its frame header (SOF0 to SOF15 but DHT, JPG and DAC).
fn jpeg_size(path: &str) -> (f32, f32) {
    let bytes = std::fs::read(ui_asset(path)).unwrap_or_else(|err| panic!("assets/ui/{path}: {err}"));
    assert!(bytes.starts_with(&[0xFF, 0xD8]), "assets/ui/{path} isn't a JPEG");
    let mut at = 2;
    while at + 9 <= bytes.len() {
        assert_eq!(bytes[at], 0xFF, "assets/ui/{path}: broken JPEG marker at byte {at}");
        let marker = bytes[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            at += 2;
            continue;
        }
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let be = |i: usize| u16::from_be_bytes([bytes[i], bytes[i + 1]]) as f32;
            return (be(at + 7), be(at + 5));
        }
        at += 2 + u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
    }
    panic!("assets/ui/{path}: no JPEG frame header");
}

// The world map's art (`world_map`): the base is the map's extent at its pixels per unit, in whole
// pixels; a threat's clouds hold their anchor; the frame holds the map's rect with room for its
// border; the backdrop is the 1920 × 1080 screen.
fn test_world_map_art_fits() {
    for (realm, skin) in skins() {
        let art = &skin.world_map;
        let map = art.base_map_size();
        assert_eq!(jpeg_size(&art.base), (map.x.ceil(), map.y.ceil()), "{realm}: {} covers the extent {:?}", art.base, art.extent);
        if !art.threat.is_empty() {
            let (width, height) = png_size(&art.threat);
            let [x, y] = art.threat_anchor;
            assert!((0.0..=width).contains(&x) && (0.0..=height).contains(&y), "{realm}: {} holds its anchor {:?}", art.threat, art.threat_anchor);
        }
        let (width, height) = png_size(&art.frame);
        let [x, y, map_width, map_height] = art.frame_map_rect;
        assert!(x > 0.0 && y > 0.0 && x + map_width < width && y + map_height < height, "{realm}: {} holds the map at {:?}", art.frame, art.frame_map_rect);
        assert!((map_width - map.x).abs() < 0.01 && (map_height - map.y).abs() < 0.01, "{realm}: the frame's map rect is the base's map");
        assert_eq!(jpeg_size(&art.backdrop), (1920.0, 1080.0), "{realm}: {}", art.backdrop);
    }
}

// Every colour a skin's blocks name is a `text` colour or "#RRGGBB(AA)", and the widgets' helpers
// find what they need: framed tabs, the icon frame, the toggle's sprites, the row panels.
fn test_every_colour_and_widget_part_resolves() {
    for (realm, skin) in skins() {
        let resolves = |name: &str| if name.starts_with('#') { HexColor::parse(name).is_some() } else { skin.text.contains_key(name) };
        let meters = &skin.meters;
        let rows = &skin.rows;
        let toggle = &skin.controls.toggle;
        let names = [
            &meters.fill, &meters.track, &meters.outline, &meters.goal, &meters.text, &rows.title, &rows.selected, &rows.subtitle,
            &rows.label, &rows.value, &rows.header, &rows.rule, &rows.done, &rows.pending, &rows.failed, &toggle.text, &toggle.state_text,
            &skin.toast.text,
        ];
        for name in names.into_iter().flatten() {
            assert!(resolves(name), "{realm}: no colour '{name}'");
        }
        for name in [&skin.cards.hover, &skin.cards.pressed, &skin.cards.disabled] {
            assert!(resolves(name), "{realm}: cards: no colour '{name}'");
        }
        for (name, panel) in &skin.panels {
            if let Some(text) = &panel.text {
                assert!(resolves(text), "{realm}: panel '{name}': no colour '{text}'");
            }
        }
        for (role, button) in &skin.buttons {
            for name in [&button.text, &button.text_hovered, &button.subline].into_iter().flatten() {
                assert!(resolves(name), "{realm}: button '{role}': no colour '{name}'");
            }
        }
        for (level, heading) in &skin.headings {
            if let Some(name) = &heading.color {
                assert!(resolves(name), "{realm}: heading '{level}': no colour '{name}'");
            }
        }
        for name in ["ink", "accent", "sub", "light"] {
            assert!(skin.text.contains_key(name), "{realm}: no text colour '{name}', which the widget helpers fall back on");
        }

        for role in ["tab", "tab_selected", "icon"] {
            assert!(skin.buttons.get(role).is_some_and(|button| button.sprite.is_some()), "{realm}: button '{role}' isn't framed");
        }
        assert!(skin.buttons["icon"].selected.is_some() && skin.buttons["icon"].size.is_some_and(|size| size > 0.0), "{realm}: icon button");
        assert!(skin.buttons["menu_item"].marker.is_some(), "{realm}: no menu item marker");
        assert!(!toggle.off.is_empty() && !toggle.on.is_empty() && !toggle.knob.is_empty(), "{realm}: toggle sprites");
        assert!(toggle.size[0] > 0.0 && toggle.size[1] > 0.0, "{realm}: toggle size");
        for name in ["row", "row_selected", "well"] {
            assert!(skin.panels.get(name).is_some_and(|panel| panel.slice.is_some()), "{realm}: no 9-slice panel '{name}'");
        }
        for realm_name in Realm::iter() {
            assert!(skin.emblems.contains_key(&realm_name.to_string()), "{realm}: no emblem for {realm_name}");
        }
    }
}

// Every sound file a skin names exists, and Đại Long's names every role (the defaults).
fn test_every_sound_exists() {
    for (realm, skin) in skins() {
        let (mapping, problems) = sound::mapping_from_names(skin.sounds.clone().into_iter().collect());
        assert!(problems.is_empty(), "{realm}: {problems:?}");
        for role in sound::UiSoundKey::iter() {
            if realm == Realm::DaiLong {
                assert!(mapping.get(&role).is_some_and(Option::is_some), "{realm}: no file for '{role}'");
            }
            if let Some(Some(file)) = mapping.get(&role) {
                let path = repo_path("assets/sounds/sfx/ui").join(file);
                assert!(path.is_file(), "{realm}: '{role}' names {}, which doesn't exist", path.display());
            }
        }
    }
}

// Every style the game loads: the developer UI's (Source Code Pro) and each realm skin's.
fn test_game_styles_cover_vietnamese() {
    let sources = menu::ui_style_sources();
    assert_eq!(sources.len(), 1 + Realm::iter().count(), "the developer UI's style and one per realm skin");
    for source in &sources {
        if let Err(err) = source.validate() {
            panic!("{err}");
        }
    }
    if let Err(err) = check(&sources) {
        panic!("{err}");
    }
}

// Without their Vietnamese fallbacks, Kambara's and Tampadesa's text styles fail, naming the
// letters their fonts lack, so the check above passes because of the fallbacks. Đại Long's font
// has them all.
fn test_realm_fonts_need_their_fallbacks() {
    for (name, mut json) in skin_styles() {
        let mut removed = false;
        for (_, text_style) in json["text_styles"].as_object_mut().unwrap() {
            let fallbacks = text_style["fallbacks"].as_array_mut().unwrap();
            let count = fallbacks.len();
            fallbacks.retain(|font| !font.as_str().unwrap().ends_with("_vi"));
            removed |= fallbacks.len() != count;
        }

        let sources = [menu::ui_style_sources().remove(0), UiStyleSource::json(name.clone(), json)];
        match check(&sources) {
            Ok(()) => assert!(!removed, "{name} draws Vietnamese without its fallbacks"),
            Err(err) => {
                assert!(removed, "{name}: {err}");
                for text_style in ["body", "heading", "button", "small"] {
                    assert!(err.contains(&format!("{name}/{text_style} (fonts ")), "{name}: {err}");
                }
                assert!(err.contains("'ạ' (U+1EA1)") && err.contains("'ủ' (U+1EE7)"), "{name}: {err}");
                assert!(
                    !err.contains(&format!("{name}/monospace (")) && !err.contains(&format!("{name}/icon (")),
                    "{name}: {err}"
                );
            }
        }
    }
}
