use engine::ui::{self, UiStyleSource};
use game::{menu::{self, skin}, realm::Realm};
use serde_json::Value;
use strum::IntoEnumIterator;

mod test_utils;

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
//   Kambara: Jaini 1048 / -266, Spectral SC SemiBold 1059 / -463, so 0.093. Tampadesa: Amita
//   1292 / -650, Alegreya 1016 / -345, so -0.015.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("UI fonts", &[
        test_utils::test_fn!(test_game_styles_cover_vietnamese),
        test_utils::test_fn!(test_realm_fonts_need_their_fallbacks),
    ]);
}

fn check(sources: &[UiStyleSource]) -> Result<(), String> {
    ui::check_glyph_coverage(sources, ui::GLYPH_TEST_TEXTS, ui::GLYPH_TEST_ICONS)
}

// The realm skins' styles, as JSON, by theme name.
fn skin_styles() -> Vec<(String, Value)> {
    let files = skin::read_skin_files().unwrap_or_else(|err| panic!("{err}"));
    let file = |realm: Realm| files.iter().find(|(file_realm, _)| *file_realm == realm).unwrap().1.clone();
    Realm::iter()
        .map(|realm| {
            let skin = skin::merge_skin(realm, &file).unwrap_or_else(|err| panic!("{err}"));
            (skin::theme_name(realm), skin.style)
        })
        .collect()
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

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
