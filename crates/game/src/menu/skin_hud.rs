// The skin's `hud` block (docs/plans/realm-ui.md, Phase 5): how the HUD's icon tiles, glyphs, line
// icons, gap ornaments and badges look in a realm. The art comes from crates/tools/ui-art (the
// `hud` group of art.json), drawn with the mockup generator's palette.py:
// - `tiles`: a look per tile kind (`large`, the build palette's buildings; `sector`, the sector
//   column; `small`, the top bar's and the minimap's buttons): the tile's `size` in points, its
//   frame's sprite `frame` (a stem: _idle, _hovered, _pressed and _disabled sprites) and how far
//   the frame reaches past the tile (`bleed`: left, top, right, bottom points; a crest stands over
//   the tile);
// - `glyphs`: the folder of the pictograms, `<group>/<name>_idle.png` and `_pressed.png` (groups:
//   building, sector, hud, minimap), each the tile's rect grown by `glyph_bleed` points all round,
//   so it lands where the generator draws it; `glyph_aliases` names a tile's glyph when it isn't
//   the tile's own name (dirt_road → road); `glyph_tints`, the glyph's colour per state (hovered
//   and disabled are tints of the idle sprite; pressed has its own sprite); `pressed_offset`, how
//   far a pressed glyph moves down (0: the pressed sprite is already drawn pressed);
// - `icons`: the folder of the line icons (`<name>.png`: population, coin, food and the messages'
//   kinds), each `icon_bleed` points larger than its 36-point rect;
// - `ornaments`: the gap ornaments, `wide` and `narrow`, drawn in the top bar's and the minimap's
//   empty stretches;
// - `badge`: a count badge's look (colours as `text` names or "#RRGGBB(AA)");
// - `colors`: the HUD's colours by role (`text`, `sub`, `accent`, `rule`, `warning`,
//   `rating_track`, `rating_fill`, `notch`, `notch_outline`).

use std::collections::BTreeMap;

use serde::Deserialize;

use common::{Color, Vec2};
use engine::{
    log,
    render::texture::TextureHandle,
    ui::{UiDecorationAnchor, widgets::*},
};

use super::skin::{self, sprite_and_size};

// ----------------------------------------------
// The JSON
// ----------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHudDef {
    pub tiles: BTreeMap<String, SkinHudTileDef>,
    pub glyphs: String,
    pub glyph_bleed: f32,
    pub glyph_aliases: BTreeMap<String, String>,
    pub glyph_tints: SkinHudTintsDef,
    pub pressed_offset: f32,
    pub icons: String,
    pub icon_bleed: f32,
    pub ornaments: SkinHudOrnamentsDef,
    pub badge: SkinHudBadgeDef,
    pub colors: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHudTileDef {
    pub size: [f32; 2],
    pub frame: String,
    pub bleed: [f32; 4],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHudTintsDef {
    pub idle: Option<String>,
    pub hovered: Option<String>,
    pub pressed: Option<String>,
    pub disabled: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHudOrnamentsDef {
    pub wide: String,
    pub narrow: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHudBadgeDef {
    pub fill: String,
    pub ring: String,
    pub text: String,
    pub radius: f32,
    pub ring_width: f32,
    pub font_size: f32,
    pub offset: [f32; 2], // From the tile's top right corner.
}

impl Default for SkinHudBadgeDef {
    fn default() -> Self {
        Self {
            fill: "#b3271b".into(),
            ring: "#fff4dc".into(),
            text: "#ffffff".into(),
            radius: 9.0,
            ring_width: 1.5,
            font_size: 11.0,
            offset: [-1.0, 1.0],
        }
    }
}

// ----------------------------------------------
// Tiles and glyphs
// ----------------------------------------------

// The glyphs every skin draws, besides one per building in the build palette (`building/<tile>`,
// through `glyph_aliases`): the sector column's, the HUD's buttons', the minimap's; and its line icons.
pub const SECTOR_GLYPHS: &[&str] =
    &["clear", "housing", "roads", "food", "industry", "services", "infrastructure", "culture", "trade", "beauty", "undo", "redo"];
pub const HUD_GLYPHS: &[&str] = &["pause", "play", "fast", "faster", "goals", "menu", "save", "gear", "book", "briefing", "close"];
pub const MINIMAP_GLYPHS: &[&str] = &["pin", "fire", "water", "sectors", "city", "plus", "minus", "alert", "home", "world", "collapse"];
pub const HUD_ICONS: &[&str] = &["population", "coin", "food", "request", "trade", "warn", "monument"];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HudTileKind {
    Large,  // The build palette's buildings.
    Sector, // The sector column.
    Small,  // The top bar's and the minimap's buttons.
}

impl HudTileKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Large => "large",
            Self::Sector => "sector",
            Self::Small => "small",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HudGlyphGroup {
    Building,
    Sector,
    Hud,
    Minimap,
}

impl HudGlyphGroup {
    pub fn folder(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Sector => "sector",
            Self::Hud => "hud",
            Self::Minimap => "minimap",
        }
    }
}

fn tile_def(kind: HudTileKind) -> SkinHudTileDef {
    match skin::hud().tiles.get(kind.name()) {
        Some(def) => def.clone(),
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no HUD tile '{}'.", skin::current_realm(), kind.name());
            SkinHudTileDef { size: [48.0, 48.0], ..Default::default() }
        }
    }
}

// A tile kind's size in points, its crest and frame left out.
pub fn hud_tile_size(kind: HudTileKind) -> Vec2 {
    let [width, height] = tile_def(kind).size;
    Vec2::new(width, height)
}

// How far a tile kind's frame reaches past the tile, in points: left, top, right, bottom.
pub fn hud_tile_bleed(kind: HudTileKind) -> [f32; 4] {
    tile_def(kind).bleed
}

fn color_or(name: &Option<String>, fallback: Color) -> Color {
    name.as_deref().and_then(skin::color).unwrap_or(fallback)
}

// A colour of the HUD by role (the block's `colors`), else the `text` colour `fallback` names.
pub fn hud_color(role: &str, fallback: &str) -> Color {
    let hud = skin::hud();
    hud.colors
        .get(role)
        .and_then(|name| skin::color(name))
        .or_else(|| skin::color(fallback))
        .unwrap_or_else(Color::white)
}

// The badge's look.
pub fn hud_badge() -> UiIconTileBadge {
    let def = &skin::hud().badge;
    UiIconTileBadge {
        anchor: UiDecorationAnchor::TopRight,
        offset: Vec2::new(def.offset[0], def.offset[1]),
        radius: def.radius,
        fill: skin::color(&def.fill).unwrap_or_else(Color::red),
        ring: skin::color(&def.ring).unwrap_or_else(Color::white),
        ring_width: def.ring_width,
        text_color: skin::color(&def.text).unwrap_or_else(Color::white),
        font_size: def.font_size,
        heading_font: false,
    }
}

// The look of a tile kind: its frame by state, the glyph's room and tints, the badge.
pub fn hud_tile_look(context: &mut dyn UiWidgetContext, kind: HudTileKind) -> UiIconTileLook {
    let def = tile_def(kind);
    let hud = skin::hud();
    let mut frame = |state: &str| sprite_and_size(context, &format!("{}_{state}.png", def.frame)).0;
    let (idle, hovered, pressed, disabled) = (frame("idle"), frame("hovered"), frame("pressed"), frame("disabled"));
    let bleed = hud.glyph_bleed;
    let tints = &hud.glyph_tints;
    UiIconTileLook::new()
        .with_frames(idle, hovered, pressed, disabled)
        .with_frame_bleed(def.bleed)
        .with_glyph_inset([-bleed; 4])
        .with_glyph_tints(
            color_or(&tints.idle, Color::white()),
            color_or(&tints.hovered, Color::white()),
            color_or(&tints.pressed, Color::white()),
            color_or(&tints.disabled, Color::new(0.6, 0.6, 0.6, 0.5)),
        )
        .with_pressed_offset(hud.pressed_offset)
        .with_badge(hud_badge())
}

// The glyph `name` of `group` (a tile's name for a building, through the skin's aliases), and its
// pressed sprite.
pub fn hud_glyph(context: &mut dyn UiWidgetContext, group: HudGlyphGroup, name: &str) -> (UiIconGlyph, UiIconGlyph) {
    let hud = skin::hud();
    let name = hud.glyph_aliases.get(name).map_or(name, String::as_str);
    let mut load = |state: &str| {
        let (texture, _) = sprite_and_size(context, &format!("{}/{}/{name}_{state}.png", hud.glyphs, group.folder()));
        UiIconGlyph::new(texture)
    };
    (load("idle"), load("pressed"))
}

// A tile of `kind` labelled `label` (its automation path) with the glyph `name` of `group`.
pub fn hud_tile(context: &mut dyn UiWidgetContext, kind: HudTileKind, label: &str, group: HudGlyphGroup, name: &str) -> UiIconTile {
    hud_tile_scaled(context, kind, 1.0, label, group, name)
}

// The same, `scale` times its size (the sector column on a short window), its frame's bleed and the
// glyph's room with it.
pub fn hud_tile_scaled(context: &mut dyn UiWidgetContext, kind: HudTileKind, scale: f32, label: &str, group: HudGlyphGroup, name: &str) -> UiIconTile {
    let mut look = hud_tile_look(context, kind);
    if scale != 1.0 {
        let bleed = hud_tile_bleed(kind).map(|side| side * scale);
        let glyph = skin::hud().glyph_bleed * scale;
        look = look.with_frame_bleed(bleed).with_glyph_inset([-glyph; 4]);
    }
    let (idle, pressed) = hud_glyph(context, group, name);
    UiIconTile::new(label, hud_tile_size(kind) * scale, look).with_glyph(idle, Some(pressed))
}

// ----------------------------------------------
// Icons and ornaments
// ----------------------------------------------

// A line icon (population, coin, food, the messages' kinds), and the size its 36-point rect
// is drawn at in points (the sprite is `icon_bleed` larger all round).
pub fn hud_icon(context: &mut dyn UiWidgetContext, name: &str) -> (TextureHandle, Vec2) {
    let hud = skin::hud();
    sprite_and_size(context, &format!("{}/{name}.png", hud.icons))
}

// How much of a line icon's sprite is bleed, in points on each side.
pub fn hud_icon_bleed() -> f32 {
    skin::hud().icon_bleed
}

// A gap ornament and its size in points: the wide one, or the narrow one for a short gap.
pub fn hud_ornament(context: &mut dyn UiWidgetContext, wide: bool) -> (TextureHandle, Vec2) {
    let ornaments = &skin::hud().ornaments;
    sprite_and_size(context, if wide { &ornaments.wide } else { &ornaments.narrow })
}
