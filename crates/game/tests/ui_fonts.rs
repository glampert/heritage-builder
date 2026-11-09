use engine::ui::{self, UiStyleSource};
use game::menu;
use serde_json::{Value, json};

mod test_utils;

// ----------------------------------------------
// Glyph coverage of the UI styles (docs/plans/realm-ui.md, Fonts).
//
// Every text style of every style draws Đại Long's names and every Vietnamese letter
// (ui::GLYPH_TEST_TEXTS) with its font or, glyph by glyph, its own fallbacks, not the style's
// monospace or icon font; the icon text style draws the UI's icons (ui::check_glyph_coverage).
// The styles are the game's own and the realm font sets below, which the skins step puts in each
// realm skin's `style` block.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("UI fonts", &[
        test_utils::test_fn!(test_game_styles_cover_vietnamese),
        test_utils::test_fn!(test_realm_font_sets_cover_vietnamese),
        test_utils::test_fn!(test_realm_fonts_need_their_fallbacks),
    ]);
}

// The styles to check: every style the game loads (the developer UI's first, as the engine
// requires, then the retail UI's), then the realm font sets.
// TODO(realm-ui): the skins step adds each realm skin's style to menu::ui_style_sources(), which
// this checks, or here if the skins load their styles from elsewhere.
fn style_sources() -> Vec<UiStyleSource> {
    let mut sources = menu::ui_style_sources();
    sources.extend(realm_font_sets().into_iter().map(|(name, json)| UiStyleSource::json(name, json)));
    sources
}

fn check(sources: &[UiStyleSource]) -> Result<(), String> {
    ui::check_glyph_coverage(sources, ui::GLYPH_TEST_TEXTS, ui::GLYPH_TEST_ICONS)
}

// ----------------------------------------------
// Realm font sets
// ----------------------------------------------

// Each realm's fonts and text styles, as the mockups set them (generator/common.py, REALM_FONTS),
// for the skins step to put in each skin's `style` block. The font names are the same in every
// realm: `title` (headings and buttons), `body` (body and small text), their Vietnamese fallbacks
// `title_vi` and `body_vi` where the realm font lacks Vietnamese letters, `mono` and `icons`.
//
// - Metrics are the ones egui reads: OS/2's typo ascender and descender if the font sets
//   USE_TYPO_METRICS, else hhea's, with the descent negative as stored; none of these fonts has a
//   line gap. Scales are units_per_em / (ascent - descent), the style files' rule, and sizes are
//   today's game.json's: the skins step sets both from the mockups. A font set's fonts and text
//   styles go together, and a font entry gives `file`, `scale` and `y_offset_factor` together,
//   since a skin is merged over Đại Long's key by key.
// - A fallback takes its primary font's scale, so both have the same em, as the mockups draw them.
// - egui centres a fallback glyph's line height on the primary font's instead of lining up their
//   baselines, so a fallback's `y_offset_factor` moves it back onto the baseline: with equal
//   scales, ((ascent + descent) of the primary - (ascent + descent) of the fallback) / 2, in ems,
//   i.e. each font's ascent minus its descender's depth. Change it with either font.
fn realm_font_sets() -> Vec<(&'static str, Value)> {
    let mono = json!({ "file": "source_code_pro_semi_bold.ttf", "scale": 0.7955, "y_offset_factor": 0.0 });
    let icons = json!({ "file": "fa-solid-900.ttf", "scale": 1.0, "y_offset_factor": 0.0 });

    // The text styles of a realm whose title and body fonts have `title_fallbacks` and
    // `body_fallbacks` before the shared mono and icons fonts.
    let text_styles = |title_fallbacks: &[&str], body_fallbacks: &[&str]| {
        let fallbacks =
            |first: &[&str]| first.iter().chain(&["mono", "icons"]).map(|name| name.to_string()).collect::<Vec<_>>();
        json!({
            "body": { "font": "body", "size": 20.0, "fallbacks": fallbacks(body_fallbacks) },
            "heading": { "font": "title", "size": 28.0, "fallbacks": fallbacks(title_fallbacks) },
            "button": { "font": "title", "size": 20.0, "fallbacks": fallbacks(title_fallbacks) },
            "small": { "font": "body", "size": 16.0, "fallbacks": fallbacks(body_fallbacks) },
            "monospace": { "font": "mono", "size": 14.0, "fallbacks": ["icons"] },
            "icon": { "font": "icons", "size": 16.0, "fallbacks": [] }
        })
    };

    // Đại Long: Protest Revolution, which has every Vietnamese letter.
    let dailong = json!({
        "fonts": {
            "title": { "file": "protest_revolution_regular.ttf", "scale": 0.8333, "y_offset_factor": 0.0 },
            "body": { "file": "protest_revolution_regular.ttf", "scale": 0.8333, "y_offset_factor": 0.0 },
            "mono": mono,
            "icons": icons
        },
        "text_styles": text_styles(&[], &[])
    });

    // Kambara: Jaini, a single weight; Spectral SC SemiBold for the letters it lacks (ạ, ộ, ủ, ơ,
    // ư, ...), the weight the mockups load, which matches Jaini's heavy strokes.
    // Jaini 1048 / -266, Spectral SC 1059 / -463: ((1.048 - 0.266) - (1.059 - 0.463)) / 2.
    let kambara = json!({
        "fonts": {
            "title": { "file": "jaini_regular.ttf", "scale": 0.761, "y_offset_factor": 0.0 },
            "body": { "file": "jaini_regular.ttf", "scale": 0.761, "y_offset_factor": 0.0 },
            "title_vi": { "file": "spectral_sc_semi_bold.ttf", "scale": 0.761, "y_offset_factor": 0.093 },
            "body_vi": { "file": "spectral_sc_semi_bold.ttf", "scale": 0.761, "y_offset_factor": 0.093 },
            "mono": mono,
            "icons": icons
        },
        "text_styles": text_styles(&["title_vi"], &["body_vi"])
    });

    // Tampadesa: Amita Regular for titles and Amita Bold for body text, as the mockups draw them;
    // Alegreya Medium and Bold, the weights the mockups load, for the letters Amita lacks.
    // Amita 1292 / -650, Alegreya 1016 / -345: ((1.292 - 0.650) - (1.016 - 0.345)) / 2.
    let tampadesa = json!({
        "fonts": {
            "title": { "file": "amita_regular.ttf", "scale": 0.5149, "y_offset_factor": 0.0 },
            "body": { "file": "amita_bold.ttf", "scale": 0.5149, "y_offset_factor": 0.0 },
            "title_vi": { "file": "alegreya_medium.ttf", "scale": 0.5149, "y_offset_factor": -0.015 },
            "body_vi": { "file": "alegreya_bold.ttf", "scale": 0.5149, "y_offset_factor": -0.015 },
            "mono": mono,
            "icons": icons
        },
        "text_styles": text_styles(&["title_vi"], &["body_vi"])
    });

    vec![("dailong_fonts", dailong), ("kambara_fonts", kambara), ("tampadesa_fonts", tampadesa)]
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

// The styles the game loads today: the developer UI's (Source Code Pro) and the retail UI's
// (Protest Revolution) have every Vietnamese letter themselves.
fn test_game_styles_cover_vietnamese() {
    if let Err(err) = check(&menu::ui_style_sources()) {
        panic!("{err}");
    }
}

fn test_realm_font_sets_cover_vietnamese() {
    for (name, json) in realm_font_sets() {
        if let Err(err) = UiStyleSource::json(name, json).validate() {
            panic!("{err}");
        }
    }
    if let Err(err) = check(&style_sources()) {
        panic!("{err}");
    }
}

// Without their Vietnamese fallbacks, Kambara's and Tampadesa's text styles fail, naming the
// letters their fonts lack, so the check above passes because of the fallbacks.
fn test_realm_fonts_need_their_fallbacks() {
    for (name, mut json) in realm_font_sets() {
        let mut removed = false;
        for (_, text_style) in json["text_styles"].as_object_mut().unwrap() {
            let fallbacks = text_style["fallbacks"].as_array_mut().unwrap();
            let count = fallbacks.len();
            fallbacks.retain(|font| !font.as_str().unwrap().ends_with("_vi"));
            removed |= fallbacks.len() != count;
        }

        let sources = [menu::ui_style_sources().remove(0), UiStyleSource::json(name, json)];
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
