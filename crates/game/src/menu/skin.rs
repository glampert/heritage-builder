// Skins (docs/plans/realm-ui.md, Skins): everything realm-specific about the retail UI, as data.
// One file per realm, assets/configs/ui/skins/<realm>.json; a realm's file holds only what it
// changes and is merged over dailong.json (engine::config::merge_json: objects key by key, anything
// else replaced). A skin holds:
// - `style`: the realm's UI style (a UiStyleDef: fonts, text styles, colours, widget visuals,
//   spacing), loaded at boot as the theme named after the realm ("dailong", ...);
// - `text`: named text colours (ink, accent, on_accent, ...), which the other blocks name;
// - `headings`, `panels`, `buttons`, `separators`, `controls`, `emblems`, `ornaments`, `icons`,
//   `icon_sets`, `cards`, `backdrops`, `dialogs`: the sprites, 9-slice borders, insets and sizes
//   the screens use;
// - `world_map`: the world map screen's art (its base without markers, the map's frame, the
//   screen's background) and where the map's units fall on it;
// - `intermission`, `briefing`: the debrief's and the campaign-complete screen's art (the stele,
//   its niche, the newest line's mark, the screen's background, the carved text's looks) and the
//   briefing's (its panel);
// - `toast`: the panel toasts draw on (the caption panel), at a scale, and their text colour;
// - `meters`, `rows`: the colours (`text` names or "#RRGGBB(AA)") of painted meters and step
//   selectors, and of list rows, key-value pairs and tables;
// - `sounds`: each UI sound role's file, applied with ui::sound::set_mapping.
//
// Every file is read once, at boot (the web build can read an asset only once), and kept; the
// textures a skin names are loaded through the texture cache, which keeps them. ui.reload_skin
// reads the files again (desktop; a changed image under the same path needs a restart). The skin
// shown follows realm::ui_realm_for(): follow() switches it, applies its sounds and bumps
// version(), which the dialogs (dialog::sync) and the HUD (InGameMenus) watch to rebuild
// themselves.
//
// The helpers below are how screens are built: dialog, button, panel, tooltip, heading, separator,
// popup, icon; and for the realm UI's widgets tab_bar, toggle, step_selector, menu_item,
// list_row_look, icon_button, framed_image, emblem, ornament, card, key_values, table, meter and
// rich_text. No sprite path is written in the menu code (see the test at the end).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use strum::IntoEnumIterator;

use common::{Color, Vec2};
use engine::{
    config,
    file_sys::{self, paths::PathRef},
    log,
    render::texture::TextureHandle,
    sound::SoundSystem,
    ui::{
        DEV_UI_THEME,
        UiFontScale,
        UiLabelLook,
        UiNineSlice,
        UiNineSliceCenter,
        UiStyleSource,
        UiString,
        UiSystem,
        UiThemeId,
        UiToastLook,
        UiTooltipBackground,
        UiTooltipPlacement,
        sound::{self, UiSoundMapping},
        widgets::*,
    },
};

use crate::{
    realm::Realm,
    tile::minimap::style::HexColor,
    ui_context::GameUiContext,
};

// The developer UI's style file, under assets/configs/.
const DEV_STYLE_FILE: &str = "ui/styles/dev.json";

// A skin file, under assets/configs/.
pub fn skin_file(realm: Realm) -> String {
    format!("ui/skins/{realm}.json")
}

// The UI theme of a realm's skin: its style, loaded under the realm's name.
pub fn theme_name(realm: Realm) -> String {
    realm.to_string()
}

// ----------------------------------------------
// Skin definition (the JSON)
// ----------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSkinDef {
    pub sprite_scale: f32, // Pixels per point of the skin's art (2: drawn at 2×).
    pub style: Value,      // A UiStyleDef, checked by the engine when loaded.
    pub text: BTreeMap<String, HexColor>,
    pub headings: BTreeMap<String, SkinHeadingDef>,
    pub panels: BTreeMap<String, SkinPanelDef>,
    pub buttons: BTreeMap<String, SkinButtonDef>,
    pub icon_sets: BTreeMap<String, String>, // Sprite-button sets: name → folder of state sprites.
    pub icons: BTreeMap<String, String>,     // Single sprites by name.
    pub controls: SkinControlsDef,
    pub meters: SkinMetersDef,
    pub rows: SkinRowsDef,
    pub separators: BTreeMap<String, SkinSeparatorDef>,
    // The home menu's campaign row, the campaign picker, Load / Save's rows and Custom Game's realm
    // rows and preview draw them (skin::emblem).
    pub emblems: BTreeMap<String, String>,   // Realm → stem (idle, locked, current states).
    pub ornaments: BTreeMap<String, String>, // Decorations by name: `crest`, the home menu's.
    pub cards: SkinCardsDef,
    pub toast: SkinToastDef,
    pub backdrops: SkinBackdropsDef,
    pub world_map: SkinWorldMapDef,
    pub intermission: SkinIntermissionDef,
    pub briefing: SkinBriefingDef,
    pub dialogs: BTreeMap<String, SkinDialogDef>, // "default", then by dialog kind name.
    pub sounds: BTreeMap<String, Option<String>>,
}

impl Default for UiSkinDef {
    fn default() -> Self {
        Self {
            sprite_scale: 2.0,
            style: Value::Object(Default::default()),
            text: BTreeMap::new(),
            headings: BTreeMap::new(),
            panels: BTreeMap::new(),
            buttons: BTreeMap::new(),
            icon_sets: BTreeMap::new(),
            icons: BTreeMap::new(),
            controls: SkinControlsDef::default(),
            meters: SkinMetersDef::default(),
            rows: SkinRowsDef::default(),
            separators: BTreeMap::new(),
            emblems: BTreeMap::new(),
            ornaments: BTreeMap::new(),
            cards: SkinCardsDef::default(),
            toast: SkinToastDef::default(),
            backdrops: SkinBackdropsDef::default(),
            world_map: SkinWorldMapDef::default(),
            intermission: SkinIntermissionDef::default(),
            briefing: SkinBriefingDef::default(),
            dialogs: BTreeMap::new(),
            sounds: BTreeMap::new(),
        }
    }
}

// How a 9-slice's middle and edges fill their room (UiNineSliceCenter).
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinCenter {
    #[default]
    Stretch,
    Tile,
    Round,
}

impl SkinCenter {
    fn nine_slice(self) -> UiNineSliceCenter {
        match self {
            Self::Stretch => UiNineSliceCenter::Stretch,
            Self::Tile => UiNineSliceCenter::Tile,
            Self::Round => UiNineSliceCenter::Round,
        }
    }
}

// A panel: a 9-slice (`slice` given: borders in pixels, left, top, right, bottom) or a sprite
// stretched over the rect (no `slice`, the old HUD art). `content`: the contents' insets in points.
// `text`: the colour of text drawn on it, a `text` colour's name or "#RRGGBB" (the style's if
// none), e.g. light text on Tampadesa's dark lacquer tooltip.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinPanelDef {
    pub sprite: String,
    pub slice: Option<[f32; 4]>,
    pub center: SkinCenter,
    pub content: Option<[f32; 4]>,
    pub text: Option<String>,
}

// A button role. Framed (`sprite`: a stem with _idle, _hovered, _pressed and _disabled sprites,
// cut by `slice`), or plain text that shows the `hover` panel (grown by `hover_grow` points) or
// the `hover_sprite` underline while hovered. `font`: large, normal, small or extra_small.
// `text`, `text_hovered`: colours as panels name them. `min_width`: the narrowest a framed button
// gets, in points. The menu item role (UiMenuItem) also takes `marker`, a sprite at its left
// while hovered, `subline`, its subline's colour, and `padding`'s second value, the room above and
// below its text; the icon role (UiIconButton) `selected`, the selected frame (cut by
// `selected_slice`, else by `slice`), and `size`, the button's in points.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinButtonDef {
    pub sprite: Option<String>,
    pub slice: [f32; 4],
    pub center: SkinCenter,
    pub padding: Option<[f32; 2]>,
    pub font: Option<SkinFontSize>,
    pub text: Option<String>,
    pub text_hovered: Option<String>,
    pub hover: Option<String>,
    pub hover_grow: [f32; 2],
    pub hover_sprite: Option<String>,
    pub min_width: Option<f32>,
    pub marker: Option<String>,
    pub subline: Option<String>,
    pub selected: Option<String>,
    pub selected_slice: Option<[f32; 4]>,
    pub size: Option<f32>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinFontSize {
    Large,
    Normal,
    Small,
    ExtraSmall,
}

impl SkinFontSize {
    fn button_size(self) -> UiTextButtonSize {
        match self {
            Self::Large => UiTextButtonSize::Large,
            Self::Normal => UiTextButtonSize::Normal,
            Self::Small => UiTextButtonSize::Small,
            Self::ExtraSmall => UiTextButtonSize::ExtraSmall,
        }
    }
}

// A heading level: its lines' font scale, the separator under it, its margins and colour.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHeadingDef {
    pub font_scale: f32,
    pub separator: Option<String>,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub color: Option<String>,
}

// A separator: a 9-slice rule (or a stretched sprite without `slice`), `height` points tall, with
// an optional ornament at its middle.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSeparatorDef {
    pub sprite: String,
    pub slice: Option<[f32; 4]>,
    pub height: Option<f32>,
    pub ornament: Option<String>,
    pub ornament_size: [f32; 2],
}

// A 9-slice sprite of a control.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSlicedSpriteDef {
    pub sprite: String,
    pub slice: [f32; 4],
}

// The controls' art: the toggle's draws UiToggle (skin::toggle), the checkbox's, slider's and
// dropdown's draw theirs (skin::checkbox, slider, dropdown). Sizes are the sprites' at the skin's
// sprite scale unless given.
// TODO(realm-ui): the scrollbar's sprites aren't drawn: egui paints dialogs' and lists' scroll
// bars in the style's colours (`style.widgets`). Drawing them needs a scroll area of the engine's
// own, or egui's scroll bar painted over.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinControlsDef {
    pub toggle: SkinToggleDef,
    pub checkbox: SkinCheckboxDef,
    pub slider: SkinSliderDef,
    pub dropdown: SkinDropdownDef,
    pub scrollbar: SkinScrollbarDef,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinToggleDef {
    pub off: String,
    pub on: String,
    pub knob: String,
    pub size: [f32; 2],         // The pill's, in points; the knob's is its sprite's.
    pub text: Option<String>,       // The label's colour.
    pub state_text: Option<String>, // The "On" / "Off" text's colour.
}

// The colours of painted meters (UiMeter), step selectors and toggles without sprites: the filled
// part, the empty part, outlines, a bar's goal tick and a ring's text.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinMetersDef {
    pub fill: Option<String>,
    pub track: Option<String>,
    pub outline: Option<String>,
    pub goal: Option<String>,
    pub text: Option<String>,
}

// The colours of rows: template list rows' titles (`selected`: a selected row's), subtitles and
// right-hand texts; key-value pairs' labels and values; table headers; the rules under rows; and
// a table's status marks (`done`, `pending`, `failed`: a goal met, still open, missed).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinRowsDef {
    pub title: Option<String>,
    pub selected: Option<String>,
    pub subtitle: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub header: Option<String>,
    pub rule: Option<String>,
    pub done: Option<String>,
    pub pending: Option<String>,
    pub failed: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinCheckboxDef {
    pub off: String,
    pub on: String,
    pub size: [f32; 2], // In points; the sprite's if zero.
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSliderDef {
    pub rail: SkinSlicedSpriteDef,
    pub fill: SkinSlicedSpriteDef,
    pub knob: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinDropdownDef {
    pub field: String, // A panel's name.
    pub arrow: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinScrollbarDef {
    pub track: SkinSlicedSpriteDef,
    pub thumb: SkinSlicedSpriteDef,
}

// Cards (UiCard, skin::card), the panels the player picks by clicking, as the campaign picker's:
// the panel one's drawn on and the one while it's selected (panel names), the room between the
// panel's edges and the contents, in points, the fill over a hovered card, and the tints of a
// pressed card's panel and of a disabled one's (`text` names or "#RRGGBB(AA)").
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinCardsDef {
    pub panel: String,
    pub selected: String,
    pub padding: [f32; 2],
    pub hover: String,
    pub pressed: String,
    pub disabled: String,
}

impl Default for SkinCardsDef {
    fn default() -> Self {
        Self {
            panel: "well".into(),
            selected: "row_selected".into(),
            padding: [20.0, 20.0],
            hover: "#FFFFFF1C".into(),
            pressed: "#E1E1E1".into(),
            disabled: "#FFFFFF96".into(),
        }
    }
}

// How toasts look (UiSystem::set_toast_look): the panel behind their text (the caption panel if
// none) drawn at `scale` times its size (its borders and content insets alike), the text's colour
// (the panel's, else ink) and font scale.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinToastDef {
    pub panel: String,
    pub scale: f32,
    pub text: Option<String>,
    pub font_scale: f32,
}

impl Default for SkinToastDef {
    fn default() -> Self {
        Self { panel: "caption".into(), scale: 1.0, text: None, font_scale: 1.0 }
    }
}

// The home menu's backdrop (one frame, or frames of an animation `home_frame_secs` apart, the last
// two looping), the colour dialogs dim the screen with, in the menus and in game, and the one
// popups dim it with.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinBackdropsDef {
    pub home: Vec<String>,
    pub home_frame_secs: f32,
    pub dim_home: HexColor,        // Behind the home menu's own dialog, Home.
    pub dim_home_dialog: HexColor, // Behind a dialog opened over it (the mockups also blur it).
    pub dim_game: HexColor,
    pub dim_popup: HexColor, // Popups', over the dialog under them and the dim behind it.
}

impl Default for SkinBackdropsDef {
    fn default() -> Self {
        Self {
            home: Vec::new(),
            home_frame_secs: 0.3,
            dim_home: HexColor::rgba(0),
            dim_home_dialog: HexColor::rgba(0x120B056B),
            dim_game: HexColor::rgba(0x00000026),
            dim_popup: HexColor::rgba(0x120B0559),
        }
    }
}

// The world map screen's art (docs/plans/realm-ui.md, Phase 2: Base art), drawn by crates/tools/ui-art
// from the mockup generator (world_map.py's base_map, map_frame and background). The game draws the
// cities, routes, labels, choice sites and compass live over the base, at the positions of the
// campaign's world file (crate::world_map::config), whose units are the generator's map space.
// The base holds the map's scenery only: sea, land, rivers, lakes, relief, the sea traders' ships
// (under the routes drawn live, where the mockup draws them over the routes) and each uncharted
// area's fill, hatching and dashed outline, so a world file's uncharted `outline` must match the
// art (the screen draws only the area's `title` and `subtitle` at its `label`). A threat's storm
// clouds aren't in it: they're `threat`, drawn over the region labels and under the routes.
// - `base`: the map without markers, a JPEG. Its top-left pixel is the map's origin, and it has
//   `pixels_per_unit` pixels per map unit on both axes (the generator's viewport, 1.29 points per
//   unit, at the art's 2 pixels per point: 2.58), so it covers `extent`, the world files' extent
//   (1000 × 672 units: 2580 × 1733.76 pixels of its 2580 × 1734). Nothing past the extent belongs
//   to the map.
// - `threat`: a threat's storm clouds, a PNG (empty: the realm's threats have none), drawn with
//   its pixel `threat_anchor` on each threat's `position` at the base's scale, cut to the map.
// - `frame`: the map panel's frame and its ornaments, a PNG drawn over the map's edges, the map
//   left transparent. `frame_map_rect` is where the map's extent lies in it, [x, y, width,
//   height] in its pixels: the frame goes on the map panel's rect grown by the frame's margins at
//   the panel's scale (the panel's width over that rect's width).
// - `backdrop`: the screen's background behind the map, the top bar and the side panel, a 1920 ×
//   1080 JPEG at 1 pixel per point.
// What the screen draws live over the base (docs/plans/realm-ui.md, Phase 2: Markers and labels),
// after the mockup generator's marks, in points on a 1920 × 1080 screen (the map's own scale at its
// fit; they keep their size as the map zooms):
// - `marks`: sprites by name (at `sprite_scale`), each with the mark's point at its centre: the
//   player's `capital`, `city` and `monument` marks (Đại Long's seals, Kambara's prangs,
//   Tampadesa's stupas), the legend's `legend` glyph for them, the `compass` and the top bar's
//   `coin`. `seal_letter`: the city's initial drawn on the player's capital and city marks (Đại
//   Long's seals: its size is a share of the place's `seal`), its look; none without it.
// - `emblems`: the side panel's emblem of a city by its owner (a realm or a power), else the
//   owner's realm emblem (`emblems` above), else `default` (a realm's file sets an owner of Đại
//   Long's to null to drop it).
// - `text`: the looks of the map's text by role (SkinMapTextDef): `capital`, `city`, `monument`,
//   `foreign`, `choice`, `region`, `region_own`, `note`, `uncharted`, `uncharted_note`, `threat`,
//   `threat_note`, `compass`; `places`: where a marker's name goes beside it (SkinMapPlaceDef), by
//   `capital`, `city`, `monument`, `foreign` and `choice`.
// - `routes`: by kind (`land`, `sea`, `river`) and `closed`, `closed_legend` (SkinMapRouteDef).
// - `colors`: named colours (a `text` name or "#RRGGBB(AA)"): the foreign cities' ring
//   (`foreign_fill`, `foreign_stroke`, `foreign_dot`), the selection ring (`selected`, its outer
//   stroke `selected_outer`, `selected_shadow`), the next mission's sites (`choice`,
//   `choice_dot`), a threat's arrow (`threat_arrow`), the relations meter (`relations_fill`,
//   `relations_track`, `relations_outline`), the legend bar (`legend_fill`, `legend_text`), the
//   side panel's text (`panel_text`, `panel_sub`), its section headings (`section`) and rules
//   (`panel_rule`), the goods' bullets (`bullet`), the court's request box (`request_fill`,
//   `request_border`, `request_inner`, `request_title`, `request_text`) and the top bar's text
//   (`bar_text`, `bar_sub`); `bullet_shape`: square, diamond or circle;
//   `request_dashed`: the request box's border in dashes; `request_seal`: the request box ends in
//   the `city` mark (a seal, drawn 32 points large) with "!" in its `seal_letter` look.
// - `choice_box`: the next mission's choice box (SkinChoiceBoxDef), its text in the `text` looks
//   `choice_title` and `choice_subtitle`, its colours `choice_box_fill`, `choice_box_border`,
//   `choice_box_inner`, the optional `choice_box_inner_fill` and `choice_box_shadow`, the dotted
//   lines to its sites `choice_line`, and a site's name under the pointer `choice_hover`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinWorldMapDef {
    pub base: String,
    pub extent: [f32; 2],
    pub pixels_per_unit: f32,
    pub threat: String,
    pub threat_anchor: [f32; 2],
    pub frame: String,
    pub frame_map_rect: [f32; 4],
    pub backdrop: String,
    pub marks: BTreeMap<String, String>,
    pub seal_letter: Option<SkinMapTextDef>,
    pub emblems: BTreeMap<String, Option<String>>,
    pub text: BTreeMap<String, SkinMapTextDef>,
    pub places: BTreeMap<String, SkinMapPlaceDef>,
    pub routes: BTreeMap<String, SkinMapRouteDef>,
    pub colors: BTreeMap<String, String>,
    pub bullet_shape: String,
    pub request_dashed: bool,
    pub request_seal: bool,
    pub choice_box: SkinChoiceBoxDef,
}

// The next mission's choice box, after the generator's choice_box, in points on a 1920 × 1080
// screen: at least `size` (wider for longer text), its border `border_width` wide, an inner rule
// `inner_inset` in and `inner_width` wide, its shadow `shadow_offset` off, its title's and
// subtitle's baselines `title_baseline` and `subtitle_baseline` down from its top and `text_x` in
// from its left, the lines to the sites `line_width` wide, and `seal`: a seal (the `city` mark)
// with two arrows at its lower right (Đại Long's).
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinChoiceBoxDef {
    pub size: [f32; 2],
    pub border_width: f32,
    pub inner_inset: f32,
    pub inner_width: f32,
    pub shadow_offset: [f32; 2],
    pub title_baseline: f32,
    pub subtitle_baseline: f32,
    pub text_x: f32,
    pub line_width: f32,
    pub seal: bool,
}

impl Default for SkinChoiceBoxDef {
    fn default() -> Self {
        Self {
            size: [292.0, 74.0],
            border_width: 1.5,
            inner_inset: 4.0,
            inner_width: 1.0,
            shadow_offset: [4.0, 5.0],
            title_baseline: 32.0,
            subtitle_baseline: 58.0,
            text_x: 18.0,
            line_width: 1.2,
            seal: false,
        }
    }
}

impl Default for SkinWorldMapDef {
    fn default() -> Self {
        Self {
            base: String::new(),
            extent: [1000.0, 672.0],
            pixels_per_unit: 2.58,
            threat: String::new(),
            threat_anchor: [0.0, 0.0],
            frame: String::new(),
            frame_map_rect: [0.0, 0.0, 2580.0, 1733.76],
            backdrop: String::new(),
            marks: BTreeMap::new(),
            seal_letter: None,
            emblems: BTreeMap::new(),
            text: BTreeMap::new(),
            places: BTreeMap::new(),
            routes: BTreeMap::new(),
            colors: BTreeMap::new(),
            bullet_shape: "square".into(),
            request_dashed: false,
            request_seal: false,
            choice_box: SkinChoiceBoxDef::default(),
        }
    }
}

// A text look: text on the world map, and the stele's carved text (`intermission.stele.text`). Its
// size and letter spacing in points (1920 × 1080; the size an em, as the mockups' CSS sizes it:
// label_look), its colour, in the heading (title) font or the body font, in capitals or as written,
// and an optional halo (the text drawn round itself, `halo_width` points out), shadow and highlight
// (copies behind it, `shadow_offset` and `highlight_offset` points off, the shadow first: carved
// text, a dark cut and a light lip). Colours are `text` names or "#RRGGBB(AA)".
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinMapTextDef {
    pub size: f32,
    pub color: String,
    pub heading: bool,
    pub capitals: bool,
    pub letter_spacing: f32,
    pub halo: Option<String>,
    pub halo_width: f32,
    pub shadow: Option<String>,
    pub shadow_offset: [f32; 2],
    pub highlight: Option<String>,
    pub highlight_offset: [f32; 2],
}

impl Default for SkinMapTextDef {
    fn default() -> Self {
        Self {
            size: 18.0,
            color: "ink".into(),
            heading: false,
            capitals: false,
            letter_spacing: 0.0,
            halo: None,
            halo_width: 2.0,
            shadow: None,
            shadow_offset: [1.2, 1.4],
            highlight: None,
            highlight_offset: [1.0, 1.0],
        }
    }
}

// A UiLabelLook of a skin's text look, `size` points large on a 1920 × 1080 screen (an em, as the
// generator's font sizes: font_em), at the screen's scale `scale` (its size over 1920 × 1080).
pub fn label_look(def: &SkinMapTextDef, size: f32, scale: f32) -> UiLabelLook {
    let offset_color = |name: &Option<String>, offset: [f32; 2]| {
        name.as_deref().and_then(color).map(|color| (color, Vec2::new(offset[0] * scale, offset[1] * scale)))
    };
    UiLabelLook {
        size: size * scale / font_em(def.heading),
        heading_font: def.heading,
        letter_spacing: def.letter_spacing * scale,
        color: color(&def.color).unwrap_or(Color::black()),
        halo: def.halo.as_deref().and_then(color),
        halo_width: def.halo_width * scale,
        shadow: offset_color(&def.shadow, def.shadow_offset),
        highlight: offset_color(&def.highlight, def.highlight_offset),
    }
}

// Where a marker's name goes, as the generator's place_label puts it: from the point `y` points
// below the marker's (negative: above), `gap` points away on the side the city's file names. `seal`:
// the size of a seal mark the city's initial goes on (seal_letter), 0 for none.
#[derive(Copy, Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinMapPlaceDef {
    pub y: f32,
    pub gap: f32,
    pub seal: f32,
}

// A route's line: its colour and width, in dashes (`dash`: length and gap) or dots (`dots`: their
// spacing; their size is the width), over an optional shadow (`shadow` colour, `shadow_width`
// wide, `shadow_offset` off).
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinMapRouteDef {
    pub color: String,
    pub width: f32,
    pub dash: [f32; 2],
    pub dots: f32,
    pub shadow: Option<String>,
    pub shadow_width: f32,
    pub shadow_offset: [f32; 2],
}

impl Default for SkinMapRouteDef {
    fn default() -> Self {
        Self { color: "ink".into(), width: 2.5, dash: [14.0, 6.0], dots: 0.0, shadow: None, shadow_width: 5.0, shadow_offset: [0.0, 0.0] }
    }
}

impl SkinWorldMapDef {
    // The pixel of `base` at a position in map units (a world file's city or route point).
    pub fn base_pixel(&self, position: [f32; 2]) -> Vec2 {
        Vec2::new(position[0] * self.pixels_per_unit, position[1] * self.pixels_per_unit)
    }

    // The part of `base` the map's extent covers, in pixels from its top left: the rest of its last
    // row or column is past the map.
    pub fn base_map_size(&self) -> Vec2 {
        self.base_pixel(self.extent)
    }
}

// The intermission's art (docs/plans/realm-ui.md, Phase 3), drawn by crates/tools/ui-art from the
// mockup generator's screens.py (stele, stele_backdrop, fresh_mark): the debrief after a mission
// and the campaign-complete screen show the campaign's stele, with its text carved live
// (menu::stele). In points on the debrief's 1920 × 1080 screen (the mockup's):
// - `backdrop`: the screen's background, a 1920 × 1080 JPEG (the world map's: the same drawing);
// - `panel`: the panel the debrief's and the campaign-complete screen's right side is on;
// - `niche`: what stands behind the stele (Đại Long's lacquer niche, Kambara's vines, Tampadesa's
//   whitewashed niche): its sprite goes on `rect` grown by `bleed` (left, top, right, bottom), as
//   ui-art draws it;
// - `stele`: the stele (SkinSteleDef);
// - `colors`: the debrief's panel's colours by role (`text`, `sub`, `heading`, `accent` (the
//   kicker), `section`, `rule`, `check`, `check_mark`, `check_open`, `note_fill`, `note_border`),
//   each a `text` name (the realm's own, so a realm's file names only the ones it changes) or
//   "#RRGGBB(AA)" (intermission_color);
// - `complete`: where the campaign-complete screen puts them (SkinCampaignCompleteDef).
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinIntermissionDef {
    pub backdrop: String,
    pub panel: String,
    pub niche: SkinPlacedSpriteDef,
    pub stele: SkinSteleDef,
    pub colors: BTreeMap<String, String>,
    pub complete: SkinCampaignCompleteDef,
}

impl Default for SkinIntermissionDef {
    fn default() -> Self {
        Self {
            backdrop: String::new(),
            panel: "card".into(),
            niche: SkinPlacedSpriteDef::default(),
            stele: SkinSteleDef::default(),
            colors: BTreeMap::new(),
            complete: SkinCampaignCompleteDef::default(),
        }
    }
}

// The campaign-complete screen's stele, wider than the debrief's for every line of a finished
// campaign, as the mockup generator's dialogs.py campaign_complete() draws it (points on the
// 1920 × 1080 screen): `stele`, the stele's rect (its sprite stretched to it, its text as wide and
// as low as it grows: menu::stele::SteleArt::stretched); `niche_stretch`, [factor, x], the niche
// stretched sideways by that factor about that x; `panel`, the right side's panel's rect;
// `rule_clearance`, points the lines move down from the rule under the title (which stays where
// the debrief's is): the debrief's lines touch it, as the mockup's do, but the mockup sets the
// campaign-complete screen's smaller, clear of it, where a short campaign's are full size.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinCampaignCompleteDef {
    pub stele: [f32; 4],
    pub niche_stretch: [f32; 2],
    pub panel: [f32; 4],
    pub rule_clearance: f32,
}

impl Default for SkinCampaignCompleteDef {
    fn default() -> Self {
        Self {
            stele: [80.0, 40.0, 780.0, 1010.0],
            niche_stretch: [1.12, 460.0],
            panel: [900.0, 60.0, 980.0, 960.0],
            rule_clearance: 10.0,
        }
    }
}

// A colour of the intermission's panel by its role (SkinIntermissionDef::colors), else the `text`
// colour `fallback` names.
pub fn intermission_color(role: &str, fallback: &str) -> Color {
    intermission().colors.get(role).and_then(|name| color(name)).or_else(|| color(fallback)).unwrap_or(Color::black())
}

// A sprite drawn over `rect` (x, y, width, height in points on a 1920 × 1080 screen) grown by
// `bleed` (left, top, right, bottom points; negative crops into it): ui-art's rect and bleed.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinPlacedSpriteDef {
    pub sprite: String,
    pub rect: [f32; 4],
    pub bleed: [f32; 4],
}

impl SkinPlacedSpriteDef {
    // Where the sprite goes, [x, y, width, height] in points on the 1920 × 1080 screen.
    pub fn sprite_rect(&self) -> [f32; 4] {
        let [x, y, width, height] = self.rect;
        let [left, top, right, bottom] = self.bleed;
        [x - left, y - top, width + left + right, height + top + bottom]
    }
}

// The stele and how its text is carved, after the generator's stele_lines, in points from its
// rect's top left (`rect` and `bleed` as SkinPlacedSpriteDef's, the generator's stele rect):
// - `text`: [x, y, width]: the text's left edge, the title's first baseline and the width lines
//   wrap at. The title is centred on that width, its lines `title_gap` points apart beyond its
//   size; the first line's baseline is `title_space` points below the title's last; the rule
//   under the title (`rule`: inset, height above that first baseline, width) runs `rule[0]` in
//   from each side; each line's own lines are `line_height` apart, and `entry_gap` more between
//   lines;
// - `bottom`: the lowest baseline the text may reach (the slab's lower rule), so the lines are
//   laid out smaller to fit when there are many (menu::stele);
// - `rows`, `row_gap`, `row_dash` and the `row` colour: the empty rows for the missions to come,
//   dashed lines `row_gap` apart after the last line, as many as fit above `bottom`, at most
//   `rows`;
// - `mark`: the newest line's mark, `mark_size` points large, its centre `mark_offset` points from
//   that line's start on its first baseline;
// - `looks`: the carved text's looks (SkinMapTextDef: realm font, colour, shadow and highlight) by
//   role: `title`, `line` and `newest` (the line freshly carved);
// - `colors`: `rule` and `row`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSteleDef {
    pub sprite: String,
    pub rect: [f32; 4],
    pub bleed: [f32; 4],
    pub text: [f32; 3],
    pub bottom: f32,
    pub title_gap: f32,
    pub title_space: f32,
    pub rule: [f32; 3],
    pub line_height: f32,
    pub entry_gap: f32,
    pub rows: usize,
    pub row_gap: f32,
    pub row_dash: [f32; 2],
    pub mark: String,
    pub mark_size: [f32; 2],
    pub mark_offset: [f32; 2],
    pub looks: BTreeMap<String, SkinMapTextDef>,
    pub colors: BTreeMap<String, String>,
}

impl Default for SkinSteleDef {
    fn default() -> Self {
        Self {
            sprite: String::new(),
            rect: [140.0, 70.0, 640.0, 950.0],
            bleed: [0.0; 4],
            text: [124.0, 218.0, 400.0],
            bottom: 735.0,
            title_gap: 8.0,
            title_space: 34.0,
            rule: [40.0, 20.0, 1.5],
            line_height: 32.0,
            entry_gap: 16.0,
            rows: 3,
            row_gap: 34.0,
            row_dash: [2.0, 8.0],
            mark: String::new(),
            mark_size: [24.0, 24.0],
            mark_offset: [-30.0, -8.0],
            looks: BTreeMap::new(),
            colors: BTreeMap::new(),
        }
    }
}

impl SkinSteleDef {
    // Where the sprite goes, [x, y, width, height] in points on the 1920 × 1080 screen.
    pub fn sprite_rect(&self) -> [f32; 4] {
        SkinPlacedSpriteDef { sprite: String::new(), rect: self.rect, bleed: self.bleed }.sprite_rect()
    }
}

// The briefing's art (docs/plans/realm-ui.md, Phase 3: Briefing): the scroll or tablet it's on
// (`panel`, the generator's mpanel: the dialogs' panel), the outline round its image
// (`image_outline` colour, `image_outline_width` points), and the mark before each of its "New in
// this mission" items (`bullet`: square, diamond or circle, the generator's good_bullet; in the
// `bullet_color` colour).
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinBriefingDef {
    pub panel: String,
    pub image_outline: String,
    pub image_outline_width: f32,
    pub bullet: SkinBullet,
    pub bullet_color: String,
}

impl Default for SkinBriefingDef {
    fn default() -> Self {
        Self { panel: "dialog".into(), image_outline: "ink".into(), image_outline_width: 3.0, bullet: SkinBullet::Square, bullet_color: "accent".into() }
    }
}

// A list's bullet (engine::ui::widgets::UiBullet), as skins spell it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinBullet {
    Square,
    Diamond,
    Circle,
}

impl SkinBullet {
    pub fn bullet(self) -> UiBullet {
        match self {
            Self::Square => UiBullet::Square,
            Self::Diamond => UiBullet::Diamond,
            Self::Circle => UiBullet::Circle,
        }
    }
}

// A dialog's window: its panel, size and placement, and whether the dialog under it shows.
// - `size`: [width, height] in points on a 1920 × 1080 window, scaled down to fit smaller ones
//   (dialog_size); a width or height of zero or less is the reference window's minus that much.
//   Or "fullscreen": the whole window, for full-screen screens (the world map, the intermission,
//   the comic viewer).
// - `align`: left (centred on the left), center or right; the menus mode's if none.
// - `offset`: [x, y] in points on a 1920 × 1080 window, scaled down with the window as the size
//   is but not grown by the UI scale (a larger offset would push a grown dialog off the window):
//   how far the dialog sits in from the edge it's aligned to (or right of the centre), and down
//   from the middle.
// - `show_below`: whether the dialog under this one stays visible, drawn inert under it (false if
//   none: the mockups hide Home's panel under Settings). A full-screen dialog hides it anyway.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinDialogDef {
    pub panel: Option<String>,
    pub size: Option<SkinDialogSize>,
    pub align: Option<SkinAlign>,
    pub offset: Option<[f32; 2]>,
    pub show_below: Option<bool>,
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SkinDialogSize {
    Points([f32; 2]),
    Mode(SkinDialogSizeMode),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinDialogSizeMode {
    Fullscreen,
}

impl SkinDialogSize {
    // A dialog's size when neither its own entry nor `default` gives one: today's dialogs' width,
    // and the reference window's height minus 120 points.
    pub const DEFAULT: Self = Self::Points([550.0, -120.0]);

    #[inline]
    pub fn is_fullscreen(self) -> bool {
        self == Self::Mode(SkinDialogSizeMode::Fullscreen)
    }
}

// The window size, in points, that dialog sizes are given for.
pub const DIALOG_REFERENCE_WINDOW: Vec2 = Vec2::new(1920.0, 1080.0);
// The smallest window the dialogs are made to fit: on a smaller one they scale no further (and are
// cut to the window).
pub const DIALOG_MIN_WINDOW: Vec2 = Vec2::new(1280.0, 720.0);

// The size in points of a dialog of `size` on a window of `window` points: its size on the
// reference window, scaled down by as much as the window is smaller than it (the smaller of the
// width and height ratios, so its proportions stay), but no further than on the minimum window,
// and never larger than the window. Whole points, so its contents stay on the pixel grid.
// The player's UI scale (ui_scale) multiplies it, the dialog still no larger than the window, nor
// smaller than at 100% on the minimum window: below that a small scale would shrink the dialog
// under the sizes its widgets keep in points (fields, toggles), and its contents would scroll.
pub fn dialog_size(size: SkinDialogSize, window: Vec2) -> Vec2 {
    dialog_size_at(size, window, ui_scale())
}

// dialog_size at a UI scale of `ui_scale`.
pub fn dialog_size_at(size: SkinDialogSize, window: Vec2, ui_scale: f32) -> Vec2 {
    let window = Vec2::new(window.x.max(1.0), window.y.max(1.0));
    let [width, height] = match size {
        SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen) => return window,
        SkinDialogSize::Points(points) => points,
    };

    let reference = DIALOG_REFERENCE_WINDOW;
    let fit = |size: f32, reference: f32| if size > 0.0 { size } else { (reference + size).max(1.0) };
    let size = Vec2::new(fit(width, reference.x), fit(height, reference.y));

    let factor = (dialog_scale(window) * ui_scale).max(dialog_scale(DIALOG_MIN_WINDOW));
    Vec2::new((size.x * factor).min(window.x).round().max(1.0), (size.y * factor).min(window.y).round().max(1.0))
}

// The player's UI scale (the Settings dialog's, GameConfigs::display): every realm theme's text
// and every dialog's size are this many times as large (1 at 100%). Widgets inside the dialogs
// keep their sizes in points, so a dialog's contents grow only as far as their text does.
// TODO(realm-ui): a full UI scale (egui's zoom factor, so the HUD, the minimap and every size in
// points grow too) needs the engine's window points and the UI's points told apart: the cursor,
// ui.tree's rects and the automation's clicks, and the HUD's layout from the window size.
static UI_SCALE_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000); // 1.0

pub fn ui_scale() -> f32 {
    f32::from_bits(UI_SCALE_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

// Sets the UI scale (ui_scale): the realm themes' text takes it at once, and the menus build again
// with it (a new version()).
pub fn set_ui_scale(ui_sys: &mut UiSystem, scale: f32) {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    ui_sys.set_text_scale(scale);
    if scale != ui_scale() {
        UI_SCALE_BITS.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
        SkinState::get_mut().version += 1;
        log::info!(log::channel!("ui"), "UI scale: {:.0}%.", scale * 100.0);
    }
}

// How much dialogs shrink on a window of `window` points (see dialog_size): 1 on the reference
// window or a larger one, two thirds at the minimum window and below.
pub fn dialog_scale(window: Vec2) -> f32 {
    let reference = DIALOG_REFERENCE_WINDOW;
    let min_factor = (DIALOG_MIN_WINDOW.x / reference.x).min(DIALOG_MIN_WINDOW.y / reference.y);
    (window.x.max(1.0) / reference.x).min(window.y.max(1.0) / reference.y).clamp(min_factor, 1.0)
}

// How much the contents of the dialog `kind` scale on a window of `window` points, for a tall
// dialog whose contents scale with its content area (the home menu): that area's height there, at
// the player's UI scale, over its height on the reference window at 100%, so they fill the same
// share of it at any size and UI scale. A little less than dialog_scale on a small window, since
// the panel's content insets keep their size as the dialog shrinks. (Its width shrinks more, for
// the same reason: such a dialog's contents leave room across, which ui.layout_issues checks.)
// Its sizes in points take this scale, and its text dialog_content_text_scale.
pub fn dialog_content_scale(kind: &str, window: Vec2) -> f32 {
    let (size, insets) = dialog_size_and_insets(kind);
    content_scale_at(size, insets, window, ui_scale())
}

fn content_scale_at(size: SkinDialogSize, insets: [f32; 4], window: Vec2, ui_scale: f32) -> f32 {
    content_size_at(size, insets, window, ui_scale).y / content_size_at(size, insets, DIALOG_REFERENCE_WINDOW, 1.0).y
}

// The scale such a dialog's text takes: dialog_content_scale over the UI scale, which the realm
// themes already give the text, so the text grows as much as the content area does, no more.
pub fn dialog_content_text_scale(kind: &str, window: Vec2) -> f32 {
    dialog_content_scale(kind, window) / ui_scale()
}

// How much the player's UI scale grew the dialog `kind` on a window of `window` points: ui_scale,
// or less where the window caps the dialog's size (the smaller of its width's and height's
// growth). A dialog that sizes its own contents scales them by this, and its text by
// dialog_text_fit, so they still fit it at 200% on a small window. Below 100% it's ui_scale, though
// the minimum dialog size (dialog_size) may keep the dialog itself larger: its contents then leave
// room.
pub fn dialog_ui_growth(kind: &str, window: Vec2) -> f32 {
    ui_growth_at(dialog_size_and_insets(kind).0, window, ui_scale())
}

fn ui_growth_at(size: SkinDialogSize, window: Vec2, ui_scale: f32) -> f32 {
    let at_100 = dialog_size_at(size, window, 1.0);
    let grown = dialog_size_at(size, window, ui_scale);
    (grown.x / at_100.x).min(grown.y / at_100.y).min(ui_scale)
}

// How much less than the UI scale the text of the dialog `kind` grows on a window of `window`
// points: dialog_ui_growth over ui_scale (1 where the dialog grew as much as the UI scale). A
// dialog that sizes its own contents (dialog_ui_growth) builds its text at this, so the text,
// which the themes grow by the UI scale, grows only as much as the dialog did.
pub fn dialog_text_fit(kind: &str, window: Vec2) -> f32 {
    text_fit_at(dialog_size_and_insets(kind).0, window, ui_scale())
}

fn text_fit_at(size: SkinDialogSize, window: Vec2, ui_scale: f32) -> f32 {
    (ui_growth_at(size, window, ui_scale) / ui_scale).min(1.0)
}

// The size in points of the dialog `kind`'s content area on a window of `window` points: its size
// less its panel's content insets.
pub fn dialog_content_size(kind: &str, window: Vec2) -> Vec2 {
    dialog_content_size_at(kind, window, ui_scale())
}

// dialog_content_size at a UI scale of `ui_scale`.
pub fn dialog_content_size_at(kind: &str, window: Vec2, ui_scale: f32) -> Vec2 {
    let (size, insets) = dialog_size_and_insets(kind);
    content_size_at(size, insets, window, ui_scale)
}

// The dialog `kind`'s size in the skin, and its panel's content insets in points (left, top,
// right, bottom).
fn dialog_size_and_insets(kind: &str) -> (SkinDialogSize, [f32; 4]) {
    let def = dialog_def(kind);
    let size = def.size.unwrap_or(SkinDialogSize::DEFAULT);
    let panel = def.panel.as_deref().unwrap_or("dialog");
    (size, current().panels.get(panel).and_then(|panel| panel.content).unwrap_or_default())
}

fn content_size_at(size: SkinDialogSize, insets: [f32; 4], window: Vec2, ui_scale: f32) -> Vec2 {
    let [left, top, right, bottom] = insets;
    let size = dialog_size_at(size, window, ui_scale);
    Vec2::new((size.x - left - right).max(1.0), (size.y - top - bottom).max(1.0))
}

// A dialog's offset (its `offset`, in points on the reference window) on a window of `window`
// points, scaled with the window (dialog_scale) but not by the UI scale, in whole points.
pub fn dialog_offset(offset: [f32; 2], window: Vec2) -> Vec2 {
    let factor = dialog_scale(window);
    Vec2::new((offset[0] * factor).round(), (offset[1] * factor).round())
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinAlign {
    Left,
    Center,
    Right,
}

impl SkinAlign {
    pub fn menu_flags(self) -> UiMenuFlags {
        match self {
            Self::Left => UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft,
            Self::Center => UiMenuFlags::AlignCenter,
            Self::Right => UiMenuFlags::AlignCenter | UiMenuFlags::AlignRight,
        }
    }
}

// ----------------------------------------------
// Loading and merging
// ----------------------------------------------

// A skin file's JSON.
fn parse_skin_file(realm: Realm, text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|err| format!("Invalid UI skin {}: {err}", skin_file(realm)))
}

fn read_skin_file(realm: Realm) -> Result<Value, String> {
    let path = config::configs_path().join(skin_file(realm));
    let text = file_sys::load_string(&path).map_err(|err| format!("Can't read UI skin {path}: {err}"))?;
    parse_skin_file(realm, &text)
}

// `realm`'s skin: its file merged over Đại Long's, given each realm's file as JSON.
pub fn merge_skin(realm: Realm, files: &impl Fn(Realm) -> Value) -> Result<UiSkinDef, String> {
    let mut value = files(Realm::DaiLong);
    if realm != Realm::DaiLong {
        config::merge_json(&mut value, files(realm));
    }
    serde_json::from_value(value).map_err(|err| format!("Invalid UI skin '{realm}': {err}"))
}

// The skin files as JSON, read from assets/configs/ (each call reads them again).
pub fn read_skin_files() -> Result<Vec<(Realm, Value)>, String> {
    Realm::iter().map(|realm| read_skin_file(realm).map(|value| (realm, value))).collect()
}

// Every realm's skin from the files' JSON, in Realm order.
fn merge_all(files: &[(Realm, Value)]) -> Result<Vec<UiSkinDef>, String> {
    let file = |realm: Realm| files.iter().find(|(file_realm, _)| *file_realm == realm).map(|(_, value)| value.clone()).unwrap_or_default();
    Realm::iter()
        .map(|realm| {
            let skin = merge_skin(realm, &file)?;
            check_required(&skin).map_err(|err| format!("UI skin '{realm}': {err}"))?;
            Ok(skin)
        })
        .collect()
}

// The panels the helpers fall back on or always draw, which every merged skin must have: `dialog`
// (panel() falls back to it), `popup`, `tooltip`, the toasts', the cards' and every dialog
// entry's. A skin without one fails to load, so a reload keeps the skins it had.
pub fn check_required(skin: &UiSkinDef) -> Result<(), String> {
    let dialog_panels = skin.dialogs.values().filter_map(|dialog| dialog.panel.as_deref());
    let card_panels = [skin.cards.panel.as_str(), skin.cards.selected.as_str()];
    for name in ["dialog", "popup", "tooltip", skin.toast.panel.as_str()].into_iter().chain(card_panels).chain(dialog_panels) {
        if !skin.panels.contains_key(name) {
            return Err(format!("no panel '{name}'"));
        }
    }
    Ok(())
}

// The UI styles of `skins`: the developer UI's first (its file), then each realm skin's style.
fn style_sources_of(skins: &[UiSkinDef]) -> Vec<UiStyleSource> {
    let mut sources = vec![UiStyleSource::file(DEV_UI_THEME, DEV_STYLE_FILE)];
    for (realm, skin) in Realm::iter().zip(skins) {
        sources.push(UiStyleSource::json(theme_name(realm), skin.style.clone()));
    }
    sources
}

// The UI styles the game loads (UiSystem::load_styles): the developer UI's, then one per realm
// skin, named after the realm. The loaded skins' if load() ran, else read from the files (tests).
pub fn style_sources() -> Result<Vec<UiStyleSource>, String> {
    if SkinState::get().loaded {
        return Ok(style_sources_of(&SkinState::get().skins));
    }
    Ok(style_sources_of(&merge_all(&read_skin_files()?)?))
}

// The sound mapping of a skin's `sounds` block, problems logged.
fn sound_mapping(skin: &UiSkinDef, realm: Realm) -> UiSoundMapping {
    let names = skin.sounds.clone().into_iter().collect();
    let (mapping, problems) = sound::mapping_from_names(names);
    for problem in problems {
        log::error!(log::channel!("ui"), "UI skin '{realm}': {problem}");
    }
    mapping
}

// ----------------------------------------------
// SkinState: the loaded skins and the current one
// ----------------------------------------------

struct SkinState {
    loaded: bool,
    skins: Vec<UiSkinDef>, // By Realm, merged over Đại Long's, as read at boot (or reloaded).
    current: Realm,
    version: u32, // Goes up whenever the current skin changes (realm switch or reload).
    toast_version: Option<u32>, // The version the toasts' look was set for (follow).
}

impl SkinState {
    const fn new() -> Self {
        Self { loaded: false, skins: Vec::new(), current: Realm::DaiLong, version: 0, toast_version: None }
    }
}

common::singleton! { SKIN_STATE, SkinState }

// Reads the three skin files and loads every UI style (the developer UI's and each skin's). Once,
// at boot, before any menu is built; then apply_sounds().
pub fn load(ui_sys: &mut UiSystem, pixels_per_point: f32) -> Result<(), String> {
    let files = read_skin_files()?;
    install(files, ui_sys, pixels_per_point)?;
    let names: Vec<String> = Realm::iter().map(|realm| realm.to_string()).collect();
    log::info!(log::channel!("ui"), "Loaded UI skins: {}.", names.join(", "));
    Ok(())
}

// Reads the skin files again (ui.reload_skin), e.g. after editing one, and reloads the styles; then
// apply_sounds(). If anything fails, the skins stay as they were. The texture cache keeps what it
// loaded, so a sprite image changed under the same path shows only after a restart. On web, which
// can read an asset only once (the style and font files included), nothing can have changed: the
// skins as loaded at boot stay, and only the menus rebuild.
pub fn reload(ui_sys: &mut UiSystem) -> Result<(), String> {
    if cfg!(feature = "web") {
        let state = SkinState::get_mut();
        if !state.loaded {
            return Err("The UI skins aren't loaded.".into());
        }
        state.version += 1;
        log::info!(log::channel!("ui"), "Rebuilt the UI with the skins as loaded at boot (web).");
        return Ok(());
    }
    let files = read_skin_files()?;
    let pixels_per_point = ui_sys.pixels_per_point();
    install(files, ui_sys, pixels_per_point)?;
    log::info!(log::channel!("ui"), "Reloaded the UI skins.");
    Ok(())
}

fn install(files: Vec<(Realm, Value)>, ui_sys: &mut UiSystem, pixels_per_point: f32) -> Result<(), String> {
    let skins = merge_all(&files)?;
    ui_sys.load_styles(&style_sources_of(&skins), pixels_per_point)?;

    let state = SkinState::get_mut();
    state.skins = skins;
    state.loaded = true;
    state.version += 1;
    Ok(())
}

// Gives the UI sounds Đại Long's skin's mapping as the default and the current skin's over it.
pub fn apply_sounds(sound_sys: &mut SoundSystem) {
    let state = SkinState::get();
    if !state.loaded {
        return;
    }
    sound::set_default_mapping(sound_sys, sound_mapping(&state.skins[Realm::DaiLong as usize], Realm::DaiLong));
    sound::set_mapping(sound_sys, &sound_mapping(&state.skins[state.current as usize], state.current));
}

// The skin shown now, and its realm.
pub fn current() -> &'static UiSkinDef {
    let state = SkinState::get();
    debug_assert!(state.loaded, "skin::load must run first!");
    &state.skins[state.current as usize]
}

pub fn current_realm() -> Realm {
    SkinState::get().current
}

// Goes up whenever the skin shown changes: a realm switch or a reload.
pub fn version() -> u32 {
    SkinState::get().version
}

pub fn is_loaded() -> bool {
    SkinState::get().loaded
}

// Shows `realm`'s skin if it doesn't already: its sounds, and a new version() for the menus to
// rebuild with. Retail menus call it as they're built and at the start of every frame, with
// realm::ui_realm_for() of their menus mode; apply_theme() switches the UI theme.
pub fn follow(context: &mut GameUiContext, realm: Realm) {
    let state = SkinState::get_mut();
    if !state.loaded {
        return;
    }
    if state.current != realm {
        state.current = realm;
        state.version += 1;
        let mapping = sound_mapping(&state.skins[realm as usize], realm);
        sound::set_mapping(&mut context.sound_sys(), &mapping);
        log::info!(log::channel!("ui"), "UI skin: {realm}.");
    }

    // The toasts are the engine's, drawn whatever the menus: they take the skin shown.
    if state.toast_version != Some(state.version) {
        state.toast_version = Some(state.version);
        let look = toast_look(context);
        context.ui_sys.set_toast_look(look);
    }
}

// The current skin's theme: its realm's style.
pub fn theme(ui_sys: &UiSystem) -> UiThemeId {
    ui_sys.theme_id(&theme_name(current_realm())).unwrap_or(UiThemeId::DEV)
}

// Switches the UI to the current skin's theme (cheap when it's already on).
pub fn apply_theme(ui_sys: &UiSystem) {
    ui_sys.set_ui_theme(theme(ui_sys));
}

// ----------------------------------------------
// Colours
// ----------------------------------------------

// A colour as skins write them: a `text` colour's name, or "#RRGGBB(AA)". None if neither.
pub fn color(name: &str) -> Option<Color> {
    if name.starts_with('#') {
        return HexColor::parse(name).map(HexColor::color);
    }
    current().text.get(name).map(|color| color.color())
}

fn opt_color(name: &Option<String>) -> Option<Color> {
    name.as_deref().and_then(color)
}

// ----------------------------------------------
// Panels
// ----------------------------------------------

// A panel ready for a widget: a 9-slice or a stretched sprite, with its content insets and the
// colour of the text on it.
#[derive(Copy, Clone, Debug)]
pub enum SkinPanel {
    NineSlice { slice: UiNineSlice, content: UiMenuInsets, text_color: Option<Color> },
    Sprite { texture: engine::render::texture::TextureHandle, content: Option<UiMenuInsets>, text_color: Option<Color> },
}

impl SkinPanel {
    pub fn text_color(&self) -> Option<Color> {
        match self {
            Self::NineSlice { text_color, .. } | Self::Sprite { text_color, .. } => *text_color,
        }
    }

    // `menu` on this panel.
    pub fn apply_to_menu(self, menu: UiMenu) -> UiMenu {
        match self {
            Self::NineSlice { slice, content, .. } => menu.with_panel(slice, content),
            Self::Sprite { texture, content, .. } => {
                let menu = menu.with_background_texture(texture);
                match content {
                    Some(content) => menu.with_content_insets(content),
                    None => menu,
                }
            }
        }
    }

    // A popup on this panel.
    pub fn apply_to_message_box(self, message_box: UiMessageBox) -> UiMessageBox {
        match self {
            Self::NineSlice { slice, content, .. } => message_box.with_panel(slice, content),
            Self::Sprite { texture, content, .. } => {
                let message_box = message_box.with_background_texture(texture);
                match content {
                    Some(content) => message_box.with_content_insets(content),
                    None => message_box,
                }
            }
        }
    }
}

fn insets(content: Option<[f32; 4]>) -> Option<UiMenuInsets> {
    content.map(|[left, top, right, bottom]| UiMenuInsets::points(left, top, right, bottom))
}

// A panel's content insets in points (left, top, right, bottom), None if they're relative.
fn insets_points(insets: UiMenuInsets) -> Option<[f32; 4]> {
    (!insets.relative).then_some([insets.left, insets.top, insets.right, insets.bottom])
}

// A 9-slice of a sprite under assets/ui/, at the skin's sprite scale.
pub(crate) fn nine_slice(context: &mut dyn UiWidgetContext, sprite: &str, slice_px: [f32; 4], center: SkinCenter) -> UiNineSlice {
    context.load_nine_slice(PathRef::from_str(sprite), slice_px).with_scale(art_scale()).with_center(center.nine_slice())
}

// The panel `name` of the current skin. A name the skin lacks logs an error and gives its
// `dialog` panel, so a skin error never stops the UI.
pub fn panel(context: &mut dyn UiWidgetContext, name: &str) -> SkinPanel {
    let skin = current();
    let def = match skin.panels.get(name) {
        Some(def) => def,
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no panel '{name}'.", current_realm());
            skin.panels.get("dialog").expect("Every skin has a dialog panel (Đại Long's).")
        }
    };

    let text_color = opt_color(&def.text);
    match def.slice {
        Some(slice_px) => SkinPanel::NineSlice {
            slice: nine_slice(context, &def.sprite, slice_px, def.center),
            content: insets(def.content).unwrap_or_default(),
            text_color,
        },
        None => SkinPanel::Sprite {
            texture: context.load_texture(PathRef::from_str(&def.sprite)),
            content: insets(def.content),
            text_color,
        },
    }
}

// ----------------------------------------------
// Dialogs and popups
// ----------------------------------------------

// The skin's settings for the dialog `kind` (a DialogMenuKind's name): its own over `default`.
pub fn dialog_def(kind: &str) -> SkinDialogDef {
    let skin = current();
    let mut def = skin.dialogs.get("default").cloned().unwrap_or_default();
    if let Some(own) = skin.dialogs.get(kind) {
        def.panel = own.panel.clone().or(def.panel);
        def.size = own.size.or(def.size);
        def.align = own.align.or(def.align);
        def.offset = own.offset.or(def.offset);
        def.show_below = own.show_below.or(def.show_below);
    }
    def
}

// A dialog's menu: labeled `kind`, with `flags`, on its panel and at its size and offset from the
// skin for the window as it is now (the dialog framework sets them again each frame it draws the
// dialog).
pub fn dialog(context: &mut GameUiContext, kind: &str, flags: UiMenuFlags) -> UiMenu {
    let def = dialog_def(kind);
    let window = engine::ui::screen_size(context.ui_sys);
    let dialog_size_def = def.size.unwrap_or(SkinDialogSize::DEFAULT);
    let size = dialog_size(dialog_size_def, window);

    let mut menu = UiMenu::new(kind).with_flags(flags).with_size(size);
    // A full-screen dialog takes no offset (as the dialog framework's DialogFrame::offset).
    if let Some(offset) = def.offset
        && !dialog_size_def.is_fullscreen()
    {
        menu = menu.with_position(dialog_offset(offset, window));
    }
    panel(context, def.panel.as_deref().unwrap_or("dialog")).apply_to_menu(menu)
}

// A popup message box of a dialog: `lines` as a popup heading on the skin's popup panel, buttons
// to add with with_buttons. `label` names it for automation (`<label>/<button>`); it isn't drawn.
pub fn popup(context: &mut dyn UiWidgetContext, label: &str, lines: impl IntoIterator<Item = UiString>) -> UiMessageBox {
    let heading = heading(context, lines, HeadingLevel::Popup);
    let dim = current().backdrops.dim_popup.color();
    panel(context, "popup").apply_to_message_box(UiMessageBox::new(label)).with_content(heading).with_backdrop_color(dim)
}

// A popup as popup() makes it, with `body` under its heading's lines in body text: the skin's ink
// at the regular size ("A deleted save can't be brought back.").
pub fn popup_with_body(
    context: &mut dyn UiWidgetContext,
    label: &str,
    lines: impl IntoIterator<Item = UiString>,
    body: impl IntoIterator<Item = UiString>,
) -> UiMessageBox {
    popup_with_colored_body(context, label, lines, body.into_iter().map(|line| (line, "ink")))
}

// A popup as popup_with_body makes it, each line of `body` in the skin's text colour it names
// ("ink", "sub"): the Quit popup's "Saved 12 minutes ago." under what will be lost.
pub fn popup_with_colored_body<'a>(
    context: &mut dyn UiWidgetContext,
    label: &str,
    lines: impl IntoIterator<Item = UiString>,
    body: impl IntoIterator<Item = (UiString, &'a str)>,
) -> UiMessageBox {
    let body = body.into_iter().map(|(line, color_name)| {
        let text = UiText::new(line, UiFontScale::default());
        match color(color_name) {
            Some(color) => text.with_color(color),
            None => text,
        }
    });
    popup(context, label, lines).with_content(UiMenuHeading::new(body).with_margin_bottom(BODY_MARGIN_BOTTOM))
}

// The room under a popup's body text, above its buttons, in points.
const BODY_MARGIN_BOTTOM: f32 = 6.0;

// A popup's buttons, as the game-quit mockups draw them: at least this wide and this tall, in
// points on a 1920 × 1080 window.
const POPUP_BUTTON_SIZE: Vec2 = Vec2::new(220.0, 56.0);

// `button` sized as a popup's button on a window of `window` points: POPUP_BUTTON_SIZE, scaled as
// the dialogs are (dialog_scale).
pub fn size_popup_button(button: UiTextButton, window: Vec2) -> UiTextButton {
    let scale = dialog_scale(window);
    button
        .with_width(UiTextButtonWidth::Min((POPUP_BUTTON_SIZE.x * scale).round()))
        .with_min_height((POPUP_BUTTON_SIZE.y * scale).round())
}

// The colour the dialogs dim the screen with behind them, in the home menu or in game.
pub fn backdrop_dim(home_menu: bool) -> Color {
    let backdrops = &current().backdrops;
    if home_menu { backdrops.dim_home.color() } else { backdrops.dim_game.color() }
}

// The colour a dialog opened over the home menu's own dims the screen with behind it.
pub fn backdrop_dim_over_home() -> Color {
    current().backdrops.dim_home_dialog.color()
}

// The em of the current skin's title (`heading`) or body font over the size egui draws it at: its
// style font's `scale` (units per em over its line height). Text sized as the mockups' CSS sizes
// it (an em) takes that size over this.
pub fn font_em(heading: bool) -> f32 {
    let pointer = if heading { "/fonts/title/scale" } else { "/fonts/body/scale" };
    current().style.pointer(pointer).and_then(Value::as_f64).map_or(1.0, |scale| scale as f32).clamp(0.1, 2.0)
}

// The current skin's world map art.
pub fn world_map() -> &'static SkinWorldMapDef {
    &current().world_map
}

// The current skin's intermission art: the stele, its niche, the screen's background.
pub fn intermission() -> &'static SkinIntermissionDef {
    &current().intermission
}

// The current skin's briefing art.
pub fn briefing() -> &'static SkinBriefingDef {
    &current().briefing
}

// The home menu's backdrop frames (one for a still), and the time between frames.
pub fn home_backdrop() -> (&'static [String], f32) {
    let backdrops = &current().backdrops;
    (&backdrops.home, backdrops.home_frame_secs)
}

// ----------------------------------------------
// Headings and separators
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HeadingLevel {
    Title,    // The game's title, on the home menu: "Heritage Builder".
    Subtitle, // The line under it: the realm's campaign.
    Dialog,   // A dialog's title.
    Popup,    // A popup's lines.
    Section,  // A section of a dialog, e.g. the city pages' lists.
}

impl HeadingLevel {
    fn key(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Subtitle => "subtitle",
            Self::Dialog => "dialog",
            Self::Popup => "popup",
            Self::Section => "section",
        }
    }

    fn default_font_scale(self) -> f32 {
        match self {
            Self::Title => 1.9,
            Self::Subtitle => 1.0,
            Self::Dialog => 1.8,
            Self::Popup => 1.5,
            Self::Section => 1.2,
        }
    }
}

// A heading of `lines` at `level`: its font scale, colour, margins and separator from the skin.
pub fn heading(context: &mut dyn UiWidgetContext, lines: impl IntoIterator<Item = UiString>, level: HeadingLevel) -> UiMenuHeading {
    heading_at(context, lines, level, 1.0)
}

// A heading as heading() makes it, its font and margins `scale` times the skin's: for a dialog
// whose contents scale with the window (the home menu).
pub fn heading_at(context: &mut dyn UiWidgetContext, lines: impl IntoIterator<Item = UiString>, level: HeadingLevel, scale: f32) -> UiMenuHeading {
    heading_scaled(context, lines, level, scale, scale)
}

// heading_at with its margins at `scale` and its font at `text_scale`, for a dialog whose text
// takes another scale than its sizes (dialog_content_text_scale).
pub fn heading_scaled(
    context: &mut dyn UiWidgetContext,
    lines: impl IntoIterator<Item = UiString>,
    level: HeadingLevel,
    scale: f32,
    text_scale: f32,
) -> UiMenuHeading {
    let mut def = current().headings.get(level.key()).cloned().unwrap_or_default();
    def.margin_top *= scale;
    def.margin_bottom *= scale;
    let font_scale = UiFontScale(if def.font_scale > 0.0 { def.font_scale } else { level.default_font_scale() } * text_scale);
    let color = opt_color(&def.color);

    let lines = lines.into_iter().map(|line| {
        let text = UiText::new(line, font_scale);
        match color {
            Some(color) => text.with_color(color),
            None => text,
        }
    });

    let heading = UiMenuHeading::new(lines).with_margin_top(def.margin_top).with_margin_bottom(def.margin_bottom);
    with_heading_separator(context, heading, level)
}

// `heading` with the separator of headings at `level`, for a heading built with its own lines
// (e.g. placeholders filled in later).
pub fn with_heading_separator(context: &mut dyn UiWidgetContext, heading: UiMenuHeading, level: HeadingLevel) -> UiMenuHeading {
    let Some(separator) = current().headings.get(level.key()).and_then(|def| def.separator.clone()) else {
        return heading;
    };
    match separator_look(context, &separator) {
        Some(SeparatorLook::Art(art)) => heading.with_separator_art(art),
        Some(SeparatorLook::Sprite(sprite)) => heading.with_separator(context, PathRef::from_str(&sprite)),
        None => heading,
    }
}

enum SeparatorLook {
    Art(UiSeparatorArt),
    Sprite(String), // Stretched over the line.
}

fn separator_look(context: &mut dyn UiWidgetContext, kind: &str) -> Option<SeparatorLook> {
    let Some(def) = current().separators.get(kind) else {
        log::error!(log::channel!("ui"), "UI skin '{}' has no separator '{kind}'.", current_realm());
        return None;
    };

    let Some(slice_px) = def.slice else {
        return Some(SeparatorLook::Sprite(def.sprite.clone()));
    };

    let rule = nine_slice(context, &def.sprite, slice_px, SkinCenter::Stretch);
    let height = def.height.unwrap_or_else(|| rule.texture_size.y * rule.scale);
    let mut art = UiSeparatorArt::new(rule, height);
    if let Some(ornament) = &def.ornament {
        let [width, height] = def.ornament_size;
        art = art.with_ornament(context.load_texture(PathRef::from_str(ornament)), Vec2::new(width, height));
    }
    Some(SeparatorLook::Art(art))
}

// A separator line of `kind` (rule, plain, or the HUD's hud and hud_vertical), across the menu.
pub fn separator(context: &mut dyn UiWidgetContext, kind: &str) -> UiSeparator {
    match separator_look(context, kind) {
        Some(SeparatorLook::Art(art)) => {
            let height = art.total_height();
            UiSeparator::new().with_art(art).with_thickness(height)
        }
        Some(SeparatorLook::Sprite(sprite)) => UiSeparator::new().with_sprite(context, PathRef::from_str(&sprite)),
        None => UiSeparator::new(),
    }
}

// ----------------------------------------------
// Buttons and tooltips
// ----------------------------------------------

// The roles text buttons play, each looked up in the skin's `buttons`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ButtonRole {
    Primary,     // A dialog's main action: Ok, Start, Save, Yes.
    Secondary,   // Its other actions: Cancel, No.
    MenuItem,    // A dialog's list of choices: Home's, the Game menu's.
    Tab,
    TabSelected,
    HudItem,     // The HUD's text buttons (the palette's lists, the inspector), until Phase 5.
}

impl ButtonRole {
    pub fn key(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::MenuItem => "menu_item",
            Self::Tab => "tab",
            Self::TabSelected => "tab_selected",
            Self::HudItem => "hud_item",
        }
    }
}

// A text button of `role` labeled `label`, pressing which runs `on_pressed`.
pub fn button<F>(context: &mut dyn UiWidgetContext, role: ButtonRole, label: impl Into<UiString>, on_pressed: F) -> UiTextButton
where
    F: for<'a> Fn(&'a UiTextButton, &mut dyn UiWidgetContext) + 'static,
{
    let mut button = UiTextButton::new(label).on_pressed(on_pressed);
    let Some(def) = current().buttons.get(role.key()) else {
        log::error!(log::channel!("ui"), "UI skin '{}' has no button role '{}'.", current_realm(), role.key());
        return button;
    };

    if let Some(font) = def.font {
        button = button.with_size(font.button_size());
    }

    if let Some(frame) = button_frame(context, def) {
        if let Some(width) = def.min_width {
            button = button.with_width(UiTextButtonWidth::Min(width));
        }
        return button.with_frame(frame);
    }

    button = button.with_text_colors(opt_color(&def.text), opt_color(&def.text_hovered));
    if let Some(hover) = &def.hover {
        if let SkinPanel::NineSlice { slice, .. } = panel(context, hover) {
            let [x, y] = def.hover_grow;
            button = button.with_hover_panel(slice, Vec2::new(x, y));
        }
    } else if let Some(sprite) = &def.hover_sprite {
        button = button.with_hover_sprite(context, PathRef::from_str(sprite));
    }
    button
}

// A framed button role's frame: its state sprites, padding and text colour. None for a plain one.
fn button_frame(context: &mut dyn UiWidgetContext, def: &SkinButtonDef) -> Option<UiTextButtonFrame> {
    let stem = def.sprite.as_ref()?;
    let mut frame = UiTextButtonFrame::load(context, stem, def.slice).with_scale(art_scale()).with_center(def.center.nine_slice());
    if let Some([x, y]) = def.padding {
        frame = frame.with_padding(Vec2::new(x, y));
    }
    if let Some(color) = opt_color(&def.text) {
        frame = frame.with_text_color(color);
    }
    Some(frame)
}

// The button role `role`, or an empty one (and an error logged) if the skin lacks it.
pub(crate) fn button_def(role: &str) -> SkinButtonDef {
    current().buttons.get(role).cloned().unwrap_or_else(|| {
        log::error!(log::channel!("ui"), "UI skin '{}' has no button role '{role}'.", current_realm());
        SkinButtonDef::default()
    })
}

// Points per pixel of the skin's art.
fn art_scale() -> f32 {
    let scale = current().sprite_scale;
    if scale > 0.0 { 1.0 / scale } else { 1.0 }
}

// The retail tooltips' font scale.
pub const TOOLTIP_FONT_SCALE: UiFontScale = UiFontScale(0.8);

// What tooltips draw behind their text, the skin's tooltip panel with its content insets, and the
// text's colour on it (the style's if None): for tooltips not tied to a widget (ui::pointer_tooltip).
pub fn tooltip_background(context: &mut dyn UiWidgetContext) -> (UiTooltipBackground, Option<Color>) {
    let panel = panel(context, "tooltip");
    let background = match panel {
        SkinPanel::NineSlice { slice, content, .. } => UiTooltipBackground::Panel(slice, insets_points(content)),
        SkinPanel::Sprite { texture, .. } => UiTooltipBackground::Sprite(texture),
    };
    (background, panel.text_color())
}

// A tooltip on the skin's tooltip panel, in its text colour.
pub fn tooltip(context: &mut dyn UiWidgetContext, text: impl Into<UiString>) -> UiTooltipText {
    let tooltip = UiTooltipText::new(text).with_font_scale(TOOLTIP_FONT_SCALE);
    match panel(context, "tooltip") {
        SkinPanel::NineSlice { slice, content, text_color } => tooltip.with_panel(slice, insets_points(content)).with_text_color(text_color),
        SkinPanel::Sprite { texture, text_color, .. } => tooltip.with_background_texture(texture).with_text_color(text_color),
    }
}

// A tooltip beside its widget, for buttons along the screen's edges.
pub fn tooltip_beside(context: &mut dyn UiWidgetContext, text: impl Into<UiString>) -> UiTooltipText {
    tooltip(context, text).with_placement(UiTooltipPlacement::BesideWidget)
}

// ----------------------------------------------
// Toasts
// ----------------------------------------------

// The toasts' look in the current skin (its `toast` block): its panel at its scale, the text's
// insets and colour. Set by follow() whenever the skin changes.
pub fn toast_look(context: &mut dyn UiWidgetContext) -> UiToastLook {
    let def = current().toast.clone();
    let scale = if def.scale > 0.0 { def.scale } else { 1.0 };
    let mut look = UiToastLook {
        theme: context.ui_sys().theme_id(&theme_name(current_realm())), // The realm's font, in every menus mode.
        font_scale: if def.font_scale > 0.0 { def.font_scale } else { 1.0 },
        ..UiToastLook::default()
    };

    let panel = panel(context, &def.panel);
    look.text_color = opt_color(&def.text).or(panel.text_color()).or_else(|| color("ink")).unwrap_or(look.text_color);
    if let SkinPanel::NineSlice { slice, content, .. } = panel {
        look.panel = Some(slice.with_scale(slice.scale * scale));
        if let Some([left, top, right, bottom]) = insets_points(content) {
            look.insets = [left * scale, top * scale, right * scale, bottom * scale];
        }
    }
    look
}

// ----------------------------------------------
// Controls
// ----------------------------------------------

// A sprite's size in points at the skin's sprite scale, or `size` if it's given (non-zero).
fn sized_sprite(context: &mut dyn UiWidgetContext, sprite: &str, size: [f32; 2]) -> (TextureHandle, Vec2) {
    let (texture, sprite_size) = sprite_and_size(context, sprite);
    if size[0] > 0.0 && size[1] > 0.0 { (texture, Vec2::new(size[0], size[1])) } else { (texture, sprite_size) }
}

// The skin's checkbox sprites, if it names them.
pub fn checkbox_look(context: &mut dyn UiWidgetContext) -> Option<UiCheckboxLook> {
    let def = current().controls.checkbox.clone();
    if def.off.is_empty() || def.on.is_empty() {
        return None;
    }
    let (off, size) = sized_sprite(context, &def.off, def.size);
    let (on, _) = sprite_and_size(context, &def.on);
    Some(UiCheckboxLook { off, on, size })
}

// A checkbox labeled `label` in the skin's sprites. Give it on_read_value and on_update_value.
pub fn checkbox(context: &mut dyn UiWidgetContext, label: impl Into<UiString>) -> UiCheckbox {
    let checkbox = UiCheckbox::new().with_label(label);
    match checkbox_look(context) {
        Some(look) => checkbox.with_look(look),
        None => checkbox,
    }
}

// The skin's slider sprites, if it names them: the rail and fill 9-slices at their sprites'
// heights, the knob at its sprite's size, the value in ink.
pub fn slider_look(context: &mut dyn UiWidgetContext) -> Option<UiSliderLook> {
    let def = current().controls.slider.clone();
    if def.rail.sprite.is_empty() || def.fill.sprite.is_empty() || def.knob.is_empty() {
        return None;
    }
    let rail = nine_slice(context, &def.rail.sprite, def.rail.slice, SkinCenter::Stretch);
    let fill = nine_slice(context, &def.fill.sprite, def.fill.slice, SkinCenter::Stretch);
    let (knob, knob_size) = sprite_and_size(context, &def.knob);
    Some(UiSliderLook {
        rail,
        fill,
        rail_height: rail.texture_size.y * rail.scale,
        fill_height: fill.texture_size.y * fill.scale,
        knob,
        knob_size,
        text_color: color("ink"),
    })
}

// A slider over `range` labeled `label` in the skin's sprites. Give it on_read_value and
// on_update_value.
pub fn slider<T: UiSliderValue>(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, range: std::ops::RangeInclusive<T>) -> UiSlider {
    slider_at(context, label, range, 1.0)
}

// A slider whose sprites are drawn `scale` times their size (a dialog that shrinks with the window).
pub fn slider_at<T: UiSliderValue>(
    context: &mut dyn UiWidgetContext,
    label: impl Into<UiString>,
    range: std::ops::RangeInclusive<T>,
    scale: f32,
) -> UiSlider {
    let slider = UiSlider::new(range).with_label(label);
    match slider_look(context) {
        Some(mut look) => {
            look.rail_height *= scale;
            look.fill_height *= scale;
            look.knob_size = look.knob_size * scale;
            look.rail.scale *= scale;
            look.fill.scale *= scale;
            slider.with_look(look)
        }
        None => slider,
    }
}

// The skin's dropdown art, if it names it: its field panel (a 9-slice) and arrow sprite.
pub fn dropdown_look(context: &mut dyn UiWidgetContext) -> Option<UiDropdownLook> {
    let def = current().controls.dropdown.clone();
    if def.arrow.is_empty() || !current().panels.contains_key(&def.field) {
        return None;
    }
    let SkinPanel::NineSlice { slice, text_color, content } = panel(context, &def.field) else {
        return None;
    };
    let (arrow, arrow_size) = sprite_and_size(context, &def.arrow);
    let text_inset = insets_points(content).map_or(0.0, |[left, ..]| left);
    Some(UiDropdownLook { field: slice, arrow, arrow_size, text_color: text_color.or_else(|| color("ink")), text_inset })
}

// A dropdown over `items` labeled `label` in the skin's art. Give it its callbacks.
pub fn dropdown<I>(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, items: I) -> UiDropdown
where
    I: IntoIterator,
    I::Item: std::fmt::Display,
{
    let dropdown = UiDropdown::new(items).with_label(label);
    match dropdown_look(context) {
        Some(look) => dropdown.with_look(look),
        None => dropdown,
    }
}

// The look of text fields: the skin's `field` panel (the dropdown's), the text inside its content
// insets (or 12 × 8 points), in the panel's text colour or ink, outlined in the accent colour while
// it has focus (as the mockups draw the field being typed in). None if the skin has no such panel.
pub fn text_input_look(context: &mut dyn UiWidgetContext) -> Option<UiTextInputLook> {
    let def = current().controls.dropdown.clone();
    if !current().panels.contains_key(&def.field) {
        return None;
    }
    let SkinPanel::NineSlice { slice, content, text_color } = panel(context, &def.field) else {
        return None;
    };
    let padding = match insets_points(content) {
        Some([left, top, right, bottom]) if left + top + right + bottom > 0.0 => Vec2::new(left.max(right), top.max(bottom) * 0.5),
        _ => Vec2::new(12.0, 8.0),
    };
    Some(UiTextInputLook { field: slice, padding, text_color: text_color.or_else(|| color("ink")), focus_color: color("accent") })
}

// A text field named `label` in widget paths (not drawn: a heading above it says what it is), as
// wide as the room it's in, on the skin's field panel. Give it its callbacks.
pub fn text_input(context: &mut dyn UiWidgetContext, label: impl Into<UiString>) -> UiTextInput {
    let input = UiTextInput::new().with_hidden_label(label).with_width(0.0);
    match text_input_look(context) {
        Some(look) => input.with_look(look),
        None => input,
    }
}

// ----------------------------------------------
// Icons and sprite buttons
// ----------------------------------------------

// The single sprite `name` (the HUD's icons), under assets/ui/. Empty if the skin lacks it.
pub fn icon(name: &str) -> &'static str {
    match current().icons.get(name) {
        Some(path) => path,
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no icon '{name}'.", current_realm());
            ""
        }
    }
}

// The state sprites' stem of sprite button `name` in icon set `set` (e.g. "palette", "services").
pub fn icon_stem(set: &str, name: &str) -> String {
    match current().icon_sets.get(set) {
        Some(folder) => format!("{folder}/{name}"),
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no icon set '{set}'.", current_realm());
            name.to_string()
        }
    }
}

// A sprite button labeled `label` ("settings": its name in widget paths) from icon set `set`.
pub fn sprite_button(context: &mut dyn UiWidgetContext, set: &str, label: &str, size: Vec2) -> UiSpriteButton {
    UiSpriteButton::with_sprite_stem(context, label, &icon_stem(set, label), size)
}

// ----------------------------------------------
// The realm UI's widgets
// ----------------------------------------------

// A sprite under assets/ui/ and its size in points, at the skin's sprite scale.
pub(crate) fn sprite_and_size(context: &mut dyn UiWidgetContext, sprite: &str) -> (TextureHandle, Vec2) {
    let texture = context.load_texture(PathRef::from_str(sprite));
    let size = context.render_sys().texture_cache().texture_size(texture);
    let size = size.map_or(Vec2::new(16.0, 16.0), |size| Vec2::new(size.width as f32, size.height as f32) * art_scale());
    (texture, size)
}

// A colour the skin names, else `fallback`'s if that's a name, else nothing.
fn color_or(name: &Option<String>, fallback: &str) -> Option<Color> {
    opt_color(name).or_else(|| color(fallback))
}

// A tab bar whose tabs are the skin's `tab` and `tab_selected` buttons, its tabs added with
// with_tab or with_tab_contents.
pub fn tab_bar(context: &mut dyn UiWidgetContext) -> UiTabBar {
    tab_bar_sized(context, Vec2::zero(), 1.0)
}

// A tab bar whose tabs are at least `min_size` points large (and the skin's tab min_width), their
// text `text_scale` times the skin's tab font.
pub fn tab_bar_sized(context: &mut dyn UiWidgetContext, min_size: Vec2, text_scale: f32) -> UiTabBar {
    let tab = button_def(ButtonRole::Tab.key());
    let selected = button_def(ButtonRole::TabSelected.key());
    let font_scale = tab.font.map_or(UiFontScale::default(), |font| font.button_size().font_scale());
    let font_scale = UiFontScale(font_scale.0 * text_scale);
    let min_width = min_size.x;
    let look = UiTabLook {
        tab: button_frame(context, &tab),
        selected: button_frame(context, &selected),
        font_scale,
        min_tab_width: Some(tab.min_width.unwrap_or(0.0).max(min_width)).filter(|width| *width > 0.0),
        min_tab_height: Some(min_size.y).filter(|height| *height > 0.0),
        ..UiTabLook::default()
    };
    UiTabBar::new().with_look(look)
}

// The look of a toggle: the skin's controls.toggle sprites, or painted in its meter colours if it
// names none.
pub fn toggle_look(context: &mut dyn UiWidgetContext) -> UiToggleLook {
    let def = current().controls.toggle.clone();
    let [width, height] = def.size;
    let size = Vec2::new(if width > 0.0 { width } else { 62.0 }, if height > 0.0 { height } else { 30.0 });
    if def.off.is_empty() || def.on.is_empty() || def.knob.is_empty() {
        let meters = meter_look();
        return UiToggleLook::Painted {
            on: meters.fill,
            off: meters.track,
            knob: color("on_accent").unwrap_or(Color::white()),
            outline: meters.outline.unwrap_or(meters.goal),
            size,
        };
    }
    let (off, _) = sprite_and_size(context, &def.off);
    let (on, _) = sprite_and_size(context, &def.on);
    let (knob, knob_size) = sprite_and_size(context, &def.knob);
    UiToggleLook::Sprites { off, on, knob, size, knob_size }
}

// A toggle labeled `label`, its state texts "On" and "Off", in the skin's look and colours. Give
// it on_read_value and on_update_value; with_fill_width(true) for a settings row.
pub fn toggle(context: &mut dyn UiWidgetContext, label: impl Into<UiString>) -> UiToggle {
    toggle_at(context, label, 1.0)
}

// A toggle whose pill is drawn `scale` times its size (a dialog that shrinks with the window). Its
// font is the engine's default: give it with_font_scale (the Settings dialog's rows do).
pub fn toggle_at(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, scale: f32) -> UiToggle {
    let def = &current().controls.toggle;
    let colors = (color_or(&def.text, "ink"), color_or(&def.state_text, "sub"));
    let look = match toggle_look(context) {
        UiToggleLook::Sprites { off, on, knob, size, knob_size } => UiToggleLook::Sprites { off, on, knob, size: size * scale, knob_size: knob_size * scale },
        UiToggleLook::Painted { on, off, knob, outline, size } => UiToggleLook::Painted { on, off, knob, outline, size: size * scale },
    };
    UiToggle::new().with_label(label).with_look(look).with_text_colors(colors.0, colors.1)
}

// The colours of the skin's painted meters.
pub fn meter_look() -> UiMeterLook {
    let def = &current().meters;
    let default = UiMeterLook::default();
    UiMeterLook {
        fill: color_or(&def.fill, "accent").unwrap_or(default.fill),
        track: color_or(&def.track, "light").unwrap_or(default.track),
        outline: color_or(&def.outline, "sub").or(default.outline),
        goal: color_or(&def.goal, "ink").unwrap_or(default.goal),
        text: color_or(&def.text, "ink"),
    }
}

// A meter of `kind`, `size` points large, in the skin's colours. Give it its value.
pub fn meter(kind: UiMeterKind, size: Vec2) -> UiMeter {
    UiMeter::new(kind, size).with_look(meter_look())
}

// The colours of step selectors' segments and arrows: the skin's meters'.
pub fn step_look() -> UiStepLook {
    let meters = meter_look();
    UiStepLook { fill: meters.fill, track: meters.track, outline: meters.outline, ..UiStepLook::default() }
}

// A step selector labeled `label` over `steps`, in the skin's meter colours and text colours. Its
// font is the engine's default: the dialog that sizes it gives it with_font_scale (Custom Game's
// difficulty), as Settings does its toggles.
pub fn step_selector(label: impl Into<UiString>, steps: impl IntoIterator<Item = UiString>) -> UiStepSelector {
    let rows = &current().rows;
    UiStepSelector::new(label, steps).with_look(step_look()).with_text_colors(color_or(&rows.title, "ink"), color_or(&rows.selected, "accent"))
}

// An entry of a dialog's list of choices (the home menu's), with an optional subline: the skin's
// menu_item role's font, colours, hover panel and marker.
pub fn menu_item(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, subline: Option<UiString>) -> UiMenuItem {
    menu_item_scaled(context, label, subline, 1.0, 1.0)
}

// The subline's font, as a share of the label's.
const MENU_ITEM_SUBLINE_SCALE: f32 = 0.55;
// The room above and below a menu item's text, when its role doesn't give it.
const MENU_ITEM_PADDING: f32 = 4.0;

// A menu item as menu_item() makes it, its padding, marker and hover panel's growth `scale` times
// the skin's and its fonts `text_scale` times: for a dialog whose contents scale with the window
// (the home menu), its text at dialog_content_text_scale.
pub fn menu_item_scaled(
    context: &mut dyn UiWidgetContext,
    label: impl Into<UiString>,
    subline: Option<UiString>,
    scale: f32,
    text_scale: f32,
) -> UiMenuItem {
    let def = button_def(ButtonRole::MenuItem.key());
    let font = def.font.map_or(UiTextButtonSize::Large, SkinFontSize::button_size).font_scale().0 * text_scale;
    let padding = def.padding.map_or(MENU_ITEM_PADDING, |[_, y]| y);
    let mut item = UiMenuItem::new(label)
        .with_font_scales(UiFontScale(font), UiFontScale(font * MENU_ITEM_SUBLINE_SCALE))
        .with_padding((padding * scale).round())
        .with_text_colors(opt_color(&def.text), opt_color(&def.text_hovered), color_or(&def.subline, "sub"));
    if let Some(subline) = subline {
        item = item.with_subline(subline);
    }
    if let Some(hover) = &def.hover
        && let SkinPanel::NineSlice { slice, .. } = panel(context, hover)
    {
        let [x, y] = def.hover_grow;
        item = item.with_hover_panel(slice, Vec2::new(x, y) * scale);
    }
    if let Some(marker) = &def.marker {
        let (texture, size) = sprite_and_size(context, marker);
        item = item.with_marker(texture, size * scale);
    }
    item
}

// The look of template list rows (UiItemList::with_rows): the skin's `row` and `row_selected`
// panels and row colours.
pub fn list_row_look(context: &mut dyn UiWidgetContext) -> UiListRowLook {
    let nine_slice_of = |context: &mut dyn UiWidgetContext, name: &str| match current().panels.contains_key(name) {
        true => match panel(context, name) {
            SkinPanel::NineSlice { slice, .. } => Some(slice),
            SkinPanel::Sprite { .. } => None,
        },
        false => None,
    };
    let rows = &current().rows;
    UiListRowLook {
        panel: nine_slice_of(context, "row"),
        selected_panel: nine_slice_of(context, "row_selected"),
        title_color: color_or(&rows.title, "ink"),
        selected_title_color: color_or(&rows.selected, "accent"),
        subtitle_color: color_or(&rows.subtitle, "sub"),
        ..UiListRowLook::default()
    }
}

// The look of key-value pairs' and tables' rows: the skin's row colours.
pub fn row_look() -> UiRowLook {
    let rows = &current().rows;
    UiRowLook {
        label_color: color_or(&rows.label, "sub"),
        value_color: color_or(&rows.value, "ink"),
        header_color: color_or(&rows.header, "accent"),
        rule_color: opt_color(&rows.rule).or(UiRowLook::default().rule_color),
        ..UiRowLook::default()
    }
}

// Label and value pairs in the skin's row colours; add them with with_pair.
pub fn key_values() -> UiKeyValues {
    UiKeyValues::new().with_look(row_look())
}

// A table in the skin's row colours; add its columns and rows (a status column with
// table_status_look's).
pub fn table() -> UiTable {
    UiTable::new().with_look(row_look())
}

// The painted status marks of a table's status column, in the skin's `rows` colours (the engine's
// where it names none).
pub fn table_status_look() -> UiTableStatusLook {
    let rows = &current().rows;
    let UiTableStatusLook::Painted { done, pending, failed } = UiTableStatusLook::default() else {
        unreachable!("the default status look is painted");
    };
    UiTableStatusLook::Painted {
        done: opt_color(&rows.done).unwrap_or(done),
        pending: opt_color(&rows.pending).unwrap_or(pending),
        failed: opt_color(&rows.failed).unwrap_or(failed),
    }
}

// Rich text in the skin's ink, its emphasis in the accent colour, and every colour the skin's
// `text` block names usable in its markup ("[accent]...[/]").
pub fn rich_text(markup: impl Into<UiString>) -> UiRichText {
    let mut text = UiRichText::new(markup);
    if let Some(ink) = color("ink") {
        text = text.with_text_color(ink);
    }
    if let Some(accent) = color("accent") {
        text = text.with_emphasis_color(accent);
    }
    for (name, value) in &current().text {
        text = text.with_color(name.clone(), value.color());
    }
    text
}

// `image` on the skin's panel `panel_name` (e.g. "well" for a map preview), inset by the panel's
// left content inset.
pub fn framed_image(context: &mut dyn UiWidgetContext, image: UiImage, panel_name: &str) -> UiImage {
    match panel(context, panel_name) {
        SkinPanel::NineSlice { slice, content, .. } => image.with_frame(slice, if content.relative { 0.0 } else { content.left }),
        SkinPanel::Sprite { .. } => image,
    }
}

// `image` on the skin's panel `panel_name`, `inset` points in from its edges whatever the panel's
// content insets: an image that should reach the panel's own border, such as a map preview.
pub fn framed_image_inset(context: &mut dyn UiWidgetContext, image: UiImage, panel_name: &str, inset: f32) -> UiImage {
    match panel(context, panel_name) {
        SkinPanel::NineSlice { slice, .. } => image.with_frame(slice, inset.max(0.0)),
        SkinPanel::Sprite { .. } => image,
    }
}

// The emblem states the skin draws.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EmblemState {
    Idle,
    Locked,
    Current,
}

impl EmblemState {
    fn suffix(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Locked => "locked",
            Self::Current => "current",
        }
    }
}

// The path under assets/ui/ of `realm`'s emblem in `state`; None if the skin has no emblem for it.
pub fn emblem_path(realm: Realm, state: EmblemState) -> Option<String> {
    current().emblems.get(&realm.to_string()).map(|stem| format!("{stem}_{}.png", state.suffix()))
}

// `realm`'s emblem in `state`, for a list row's icon or an image: its texture and size in points.
pub fn emblem(context: &mut dyn UiWidgetContext, realm: Realm, state: EmblemState) -> Option<(TextureHandle, Vec2)> {
    let path = emblem_path(realm, state);
    if path.is_none() {
        log::error!(log::channel!("ui"), "UI skin '{}' has no emblem for '{realm}'.", current_realm());
    }
    path.map(|path| sprite_and_size(context, &path))
}

// The skin's decoration `name` (its `ornaments`, e.g. "crest"): its texture and size in points.
// None if the skin has none of that name: a realm may go without.
pub fn ornament(context: &mut dyn UiWidgetContext, name: &str) -> Option<(TextureHandle, Vec2)> {
    let path = current().ornaments.get(name)?.clone();
    Some(sprite_and_size(context, &path))
}

// A card labeled `label` (its name in widget paths) holding `contents`, on the skin's card panels
// (its `cards` block) with their padding. Give it on_read_selected and on_pressed.
pub fn card(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, contents: UiWidgetGroup) -> UiCard {
    let def = current().cards.clone();
    let nine_slice_of = |context: &mut dyn UiWidgetContext, name: &str| match panel(context, name) {
        SkinPanel::NineSlice { slice, .. } => Some(slice),
        SkinPanel::Sprite { .. } => None,
    };
    let mut card = UiCard::new(label, contents).with_padding(Vec2::new(def.padding[0], def.padding[1]));
    if let (Some(hover), Some(pressed), Some(disabled)) = (color(&def.hover), color(&def.pressed), color(&def.disabled)) {
        card = card.with_tints(hover, pressed, disabled);
    }
    if let Some(panel) = nine_slice_of(context, &def.panel) {
        card = card.with_panels(panel, nine_slice_of(context, &def.selected));
    }
    card
}

// A square icon button labeled `label` (its name in widget paths) showing the skin's icon `icon`
// (see icon()), on the skin's icon frame and at its size, drawing its selected frame while selected.
pub fn icon_button(context: &mut dyn UiWidgetContext, label: &str, icon_name: &str) -> UiIconButton {
    let def = button_def("icon");
    let size = def.size.unwrap_or(28.0);
    let mut button = UiIconButton::new(label, Vec2::new(size, size));
    if let Some(frame) = button_frame(context, &def) {
        button = button.with_frame(frame);
    }
    if let Some(selected) = &def.selected {
        button = button.with_selected_panel(nine_slice(context, selected, def.selected_slice.unwrap_or(def.slice), def.center));
    }
    let path = icon(icon_name);
    if !path.is_empty() {
        button = button.with_icon(context, PathRef::from_str(path), (size * 0.18).round());
    }
    button
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

// The skin files' assets (sprites, 9-slices, colours, sounds, fonts) are checked by
// tests/ui_skins.rs; these check the rules.
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn repo_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
    }

    fn files() -> Vec<(Realm, Value)> {
        Realm::iter()
            .map(|realm| {
                let path = repo_path(&format!("assets/configs/{}", skin_file(realm)));
                let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
                (realm, parse_skin_file(realm, &text).unwrap())
            })
            .collect()
    }

    fn skins() -> Vec<(Realm, UiSkinDef)> {
        Realm::iter().zip(merge_all(&files()).unwrap_or_else(|err| panic!("{err}"))).collect()
    }

    // Dialog sizes are given for a 1920 × 1080 window and shrink with it, keeping their proportions,
    // down to the 1280 × 720 window's size, and never past the window.
    #[test]
    fn dialog_sizes_scale_to_the_window() {
        let size = SkinDialogSize::Points([1220.0, 920.0]);
        let at = |width: f32, height: f32| dialog_size(size, Vec2::new(width, height));

        assert_eq!(at(1920.0, 1080.0), Vec2::new(1220.0, 920.0));
        assert_eq!(at(3840.0, 2160.0), Vec2::new(1220.0, 920.0)); // Never larger.
        assert_eq!(at(2560.0, 1080.0), Vec2::new(1220.0, 920.0)); // The smaller ratio counts.
        assert_eq!(at(1440.0, 800.0), Vec2::new(904.0, 681.0)); // 800 / 1080 of it, in whole points.
        assert_eq!(at(1440.0, 1080.0), Vec2::new(915.0, 690.0)); // 1440 / 1920 of it.
        assert_eq!(at(1280.0, 720.0), Vec2::new(813.0, 613.0)); // Two thirds.
        assert_eq!(at(1024.0, 600.0), Vec2::new(813.0, 600.0)); // No smaller than at 1280 × 720, but cut to the window.

        // A width or height of zero or less is the reference window's minus that much, scaled.
        let from_edges = SkinDialogSize::Points([550.0, -120.0]);
        assert_eq!(dialog_size(from_edges, Vec2::new(1920.0, 1080.0)), Vec2::new(550.0, 960.0));
        assert_eq!(dialog_size(from_edges, Vec2::new(1440.0, 800.0)), Vec2::new(407.0, 711.0));

        // The UI scale multiplies it, down to the 1280 × 720 window's size at 100%.
        let at_scale = |width: f32, height: f32, scale: f32| dialog_size_at(size, Vec2::new(width, height), scale);
        assert_eq!(at_scale(1920.0, 1080.0, 0.75), Vec2::new(915.0, 690.0));
        assert_eq!(at_scale(1920.0, 1080.0, 1.5), Vec2::new(1830.0, 1080.0)); // Cut to the window.
        assert_eq!(at_scale(1440.0, 800.0, 0.75), Vec2::new(813.0, 613.0)); // No smaller than two thirds.
        assert_eq!(at_scale(1280.0, 720.0, 0.75), Vec2::new(813.0, 613.0));
        assert_eq!(at_scale(1280.0, 720.0, 1.25), Vec2::new(1017.0, 720.0));

        let fullscreen = SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen);
        assert_eq!(dialog_size(fullscreen, Vec2::new(1440.0, 800.0)), Vec2::new(1440.0, 800.0));
        assert!(fullscreen.is_fullscreen() && !size.is_fullscreen());
    }

    // How much a dialog and its contents grow with the UI scale: as much as the UI scale where the
    // window lets the dialog grow that far, less where it caps it; the text, which the themes grow
    // by the UI scale, by that over the UI scale.
    #[test]
    fn dialog_growth_follows_the_ui_scale_as_far_as_the_window_lets_it() {
        let size = SkinDialogSize::Points([600.0, 1000.0]); // Home's.
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let reference = DIALOG_REFERENCE_WINDOW;
        let small = DIALOG_MIN_WINDOW;

        assert!(close(ui_growth_at(size, reference, 1.0), 1.0));
        assert!(close(text_fit_at(size, reference, 1.0), 1.0));
        // At 200%, capped by the window's height: 1080 / 1000.
        assert!(close(ui_growth_at(size, reference, 2.0), 1.08));
        assert!(close(text_fit_at(size, reference, 2.0), 0.54));
        // On 1280 × 720 too: 720 / 667 (two thirds of 1000, in whole points).
        assert!(close(ui_growth_at(size, small, 2.0), 720.0 / 667.0));
        // A small dialog grows the whole way.
        let popup = SkinDialogSize::Points([400.0, 300.0]);
        assert!(close(ui_growth_at(popup, reference, 2.0), 2.0));
        assert!(close(text_fit_at(popup, reference, 2.0), 1.0));
        // Below 100% it's the UI scale, the text's fit 1.
        assert!(close(ui_growth_at(size, reference, 0.75), 0.75));
        assert!(close(text_fit_at(size, small, 0.75), 1.0));
    }

    // The contents of a dialog that scale with its content area: 1 on the reference window at
    // 100%, its content area's height over that elsewhere, at any UI scale.
    #[test]
    fn dialog_contents_scale_with_the_content_area() {
        let size = SkinDialogSize::Points([600.0, 1000.0]);
        let insets = [40.0, 50.0, 40.0, 50.0];
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;

        assert!(close(content_scale_at(size, insets, DIALOG_REFERENCE_WINDOW, 1.0), 1.0));
        assert!(close(content_scale_at(size, insets, Vec2::new(3840.0, 2160.0), 1.0), 1.0));
        // On 1280 × 720 the dialog is 667 tall, its insets the same: (667 - 100) / 900.
        assert!(close(content_scale_at(size, insets, DIALOG_MIN_WINDOW, 1.0), 567.0 / 900.0));
        // At 200% the dialog grows to the window's height: (1080 - 100) / 900.
        assert!(close(content_scale_at(size, insets, DIALOG_REFERENCE_WINDOW, 2.0), 980.0 / 900.0));
        // At 75%, 750 tall; on 1280 × 720 no smaller than at 100%.
        assert!(close(content_scale_at(size, insets, DIALOG_REFERENCE_WINDOW, 0.75), 650.0 / 900.0));
        assert!(close(content_scale_at(size, insets, DIALOG_MIN_WINDOW, 0.75), 567.0 / 900.0));
    }

    // A dialog's size is [width, height] or "fullscreen"; its entry merges over `default`.
    #[test]
    fn dialog_defs_parse_and_merge() {
        let def: SkinDialogDef = serde_json::from_value(serde_json::json!({ "size": "fullscreen", "show_below": true })).unwrap();
        assert_eq!(def.size, Some(SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen)));
        assert_eq!(def.show_below, Some(true));
        let def: SkinDialogDef = serde_json::from_value(serde_json::json!({ "size": [600, 960], "align": "left" })).unwrap();
        assert_eq!(def.size, Some(SkinDialogSize::Points([600.0, 960.0])));
        assert!(serde_json::from_value::<SkinDialogDef>(serde_json::json!({ "size": "huge" })).is_err());
        assert!(serde_json::from_value::<SkinDialogDef>(serde_json::json!({ "size": [600] })).is_err());
    }

    // Every skin's `dialogs` entries name a dialog (or `default`), and their sizes fit the
    // reference window.
    #[test]
    fn every_skin_dialog_entry_names_a_dialog() {
        let kinds: Vec<String> = crate::menu::dialog::DialogMenuKind::iter().map(|kind| kind.to_string()).collect();
        for (realm, skin) in skins() {
            assert!(skin.dialogs.contains_key("default"), "{realm}: no default dialog");
            for (name, def) in &skin.dialogs {
                assert!(name == "default" || kinds.contains(name), "{realm}: no dialog '{name}'");
                if let Some(SkinDialogSize::Points([width, height])) = def.size {
                    let size = dialog_size(SkinDialogSize::Points([width, height]), DIALOG_REFERENCE_WINDOW);
                    assert!(size.x <= DIALOG_REFERENCE_WINDOW.x && size.y <= DIALOG_REFERENCE_WINDOW.y, "{realm}: '{name}' too large");
                    assert!(width.abs() < DIALOG_REFERENCE_WINDOW.x && height.abs() < DIALOG_REFERENCE_WINDOW.y, "{realm}: '{name}': {width} × {height}");
                }
            }
        }
    }

    // A one-key map in a realm's file adds to Đại Long's (merge_json), it doesn't replace it.
    #[test]
    fn one_key_maps_merge() {
        let base = serde_json::json!({ "dialogs": { "default": { "panel": "dialog" } }, "sounds": { "alert": "a.wav" } });
        let overlay = serde_json::json!({ "dialogs": { "Home": { "size": [380, 980] } }, "sounds": { "toggled": null } });
        let files = |realm: Realm| if realm == Realm::DaiLong { base.clone() } else { overlay.clone() };
        let skin = merge_skin(Realm::Kambara, &files).unwrap();
        assert_eq!(skin.dialogs.len(), 2);
        assert_eq!(skin.sounds.len(), 2);
        assert_eq!(skin.sounds["toggled"], None);
    }

    // A merged skin without a panel the helpers rely on fails to load, so a reload keeps the old skins.
    #[test]
    fn skins_without_the_required_panels_fail_to_load() {
        assert!(merge_all(&files()).is_ok());
        for name in ["dialog", "popup", "tooltip", "caption"] {
            let mut files = files();
            for (_, file) in &mut files {
                if let Some(panels) = file.get_mut("panels").and_then(Value::as_object_mut) {
                    panels.remove(name);
                }
            }
            let err = merge_all(&files).unwrap_err();
            assert!(err.contains(&format!("no panel '{name}'")), "{err}");
        }
    }

    // No sprite path is written in the menu code: every one comes from the skin. A string literal
    // that starts with a sprite folder or names a .png file is one; a format string isn't.
    #[test]
    fn menu_code_has_no_sprite_path_literals() {
        const PREFIXES: [&str; 6] = ["misc/", "buttons/", "icons/", "menu_bar/", "palette/", "skins/"];
        let mut found = Vec::new();
        let mut stack = vec![repo_path("crates/game/src/menu")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("skin.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or_default();
                    // The text between quotes: every other piece, from the second.
                    let is_sprite_path = |literal: &str| {
                        PREFIXES.iter().any(|prefix| literal.starts_with(prefix)) || (literal.ends_with(".png") && !literal.contains('{'))
                    };
                    if code.split('"').skip(1).step_by(2).any(is_sprite_path) {
                        found.push(format!("{}:{}: {}", path.display(), number + 1, line.trim()));
                    }
                }
            }
        }
        assert!(found.is_empty(), "Sprite paths in the menu code (take them from the skin):\n{}", found.join("\n"));
    }
}
