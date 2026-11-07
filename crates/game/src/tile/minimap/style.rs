// Minimap styles: one JSON file per realm under assets/configs/ui/minimap/, with the palette, bake
// settings, shape sizes, frame and button sprites. Every field has a default (Đại Long's, from
// the mockups); an unknown field is an error. Loaded on first use, and again by the
// minimap.reload_style command.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};

use common::Color;
use engine::{config, file_sys, log};

use super::model::LandmarkKind;
use crate::tile::sets::TileSector;

// ----------------------------------------------
// HexColor
// ----------------------------------------------

// A colour written "#RRGGBB" or "#RRGGBBAA" in style files.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HexColor(pub Color);

impl HexColor {
    pub const fn rgb(hex: u32) -> Self {
        Self::rgba((hex << 8) | 0xff)
    }

    pub const fn rgba(hex: u32) -> Self {
        Self(Color::new(
            ((hex >> 24) & 0xff) as f32 / 255.0,
            ((hex >> 16) & 0xff) as f32 / 255.0,
            ((hex >> 8) & 0xff) as f32 / 255.0,
            (hex & 0xff) as f32 / 255.0,
        ))
    }

    #[inline]
    pub fn color(self) -> Color {
        self.0
    }

    // 0..255 RGB, as the bake takes them.
    #[inline]
    pub fn rgb255(self) -> [f32; 3] {
        [self.0.r * 255.0, self.0.g * 255.0, self.0.b * 255.0]
    }

    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        Some(if hex.len() == 6 { Self::rgb(value) } else { Self::rgba(value) })
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("expected a colour as \"#RRGGBB\" or \"#RRGGBBAA\", got \"{text}\"")))
    }
}

// ----------------------------------------------
// MinimapRealm
// ----------------------------------------------

// Which realm's style the minimap uses. The game doesn't track a current realm yet, so it's Đại
// Long's unless minimap.reload_style picks another.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum MinimapRealm {
    #[default]
    DaiLong,
    Kambara,
    Tampadesa,
}

impl MinimapRealm {
    pub fn file_name(self) -> String {
        format!("ui/minimap/{self}.json")
    }

    pub fn names() -> Vec<String> {
        Self::iter().map(|realm| realm.to_string()).collect()
    }
}

// ----------------------------------------------
// MinimapStyle
// ----------------------------------------------

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MinimapStyle {
    // The base texture's largest side in pixels: sets the pixels per cell, 2 to 8.
    pub max_texture_side: u32,
    // Cells baked per frame after edits; more stays for the next frames. Loads bake everything.
    pub bake_budget_cells: u32,

    // The map area in points, [width, height]: the widget grows around it. The world is 2:1, so
    // another shape shows a crop of the playable area at zoom 1.
    pub map_size: [f32; 2],

    pub palette: MinimapPalette,
    pub sector_colors: SectorColors,

    // The bake (see crates/minimap_bake).
    pub blur_cells: BlurCells,
    pub max_depth_cells: i32,
    pub foam_width: f32,
    pub foam_strength: f32,
    pub hill_shade: [f32; 2],
    pub hill_strength: f32,
    pub elevation_cells: f32,
    pub forest_texture: f32,
    pub land_texture: f32,
    pub vacant_lot_alpha: f32,

    // Shapes, in cells.
    pub building_inset_cells: f32,
    pub shadow_offset_cells: [f32; 2],
    pub outline_cells: f32,
    pub road_width_cells: f32,
    pub road_casing_cells: f32,
    pub field_stripes_per_cell: f32,

    // Sprites, under assets/ui.
    pub field_texture: String,
    pub frame: String,
    pub tooltip_frame: String, // Behind the minimap's tooltips; empty for `frame`.
    pub buttons: String, // Folder of the tool and lens buttons under buttons/, e.g. "minimap".
    pub landmark_icons: BTreeMap<LandmarkKind, String>,
    pub alert_icon: String,

    pub landmark: LandmarkStyle,
    pub camera: CameraStyle,
    pub ping: HexColor,
    pub ping_opacity: f32,    // Of alert pings: their rings and icon, 1 opaque.
    pub tooltip_opacity: f32, // Of the minimap's tooltips (buttons and the map), 1 opaque.
    pub badge: HexColor,
    pub badge_text: HexColor,
    pub lenses: LensStyle,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MinimapPalette {
    pub land_low: HexColor,
    pub land_high: HexColor,
    pub forest: HexColor,
    pub rocks: HexColor,
    pub water_shallow: HexColor,
    pub water_deep: HexColor,
    pub foam: HexColor,
    pub vacant_lot: HexColor,
    pub void: HexColor,
    pub road: HexColor,
    pub paved_road: HexColor,
    pub road_casing: HexColor,
    pub shadow: HexColor,
    pub outline: HexColor,
    pub field: HexColor,
    pub background: HexColor, // Behind the map, where the diamond doesn't reach.
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SectorColors {
    #[serde(rename = "Housing")]
    pub housing: HexColor,
    #[serde(rename = "FoodAndFarming")]
    pub food_and_farming: HexColor,
    #[serde(rename = "IndustryAndResources")]
    pub industry_and_resources: HexColor,
    #[serde(rename = "Services")]
    pub services: HexColor,
    #[serde(rename = "Infrastructure")]
    pub infrastructure: HexColor,
    #[serde(rename = "CultureAndReligion")]
    pub culture_and_religion: HexColor,
    #[serde(rename = "TradeAndEconomy")]
    pub trade_and_economy: HexColor,
    #[serde(rename = "Beautification")]
    pub beautification: HexColor,
    #[serde(rename = "None")]
    pub none: HexColor,
}

impl SectorColors {
    pub fn color(&self, sector: TileSector) -> Color {
        match sector {
            TileSector::Housing => self.housing,
            TileSector::FoodAndFarming => self.food_and_farming,
            TileSector::IndustryAndResources => self.industry_and_resources,
            TileSector::Services => self.services,
            TileSector::Infrastructure => self.infrastructure,
            TileSector::CultureAndReligion => self.culture_and_religion,
            TileSector::TradeAndEconomy => self.trade_and_economy,
            TileSector::Beautification => self.beautification,
            TileSector::None | TileSector::Roads => self.none,
        }
        .color()
    }
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlurCells {
    pub water: f32,
    pub vegetation: f32,
    pub rocks: f32,
    pub vacant_lot: f32,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LandmarkStyle {
    pub fill: HexColor,
    pub ink: HexColor,
    pub size: f32,         // Diameter in points.
    pub min_cell_px: f32,  // Shown once a cell is this many points across.
    pub opacity: f32,      // 1 opaque.
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraStyle {
    pub fill: HexColor,
    pub outline: HexColor,
    pub width: f32,
    pub ticks: f32, // Length of the corner ticks in points.
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LensStyle {
    pub veil: HexColor,          // Over the base, to dim it under a lens.
    pub neutral: HexColor,       // Buildings a lens says nothing about.
    pub sector_colors: SectorColors,
    pub water_ramp: [HexColor; 2], // The heat field, from no value to full.
    pub water_access: HexColor,
    pub water_no_access: HexColor,
    pub refresh_secs: f32,
}

// ----------------------------------------------
// Defaults: Đại Long, from the mockups
// ----------------------------------------------

impl Default for MinimapStyle {
    fn default() -> Self {
        let landmark_icons = LandmarkKind::iter().map(|kind| (kind, format!("minimap/icons/{}.png", kind.file_stem()))).collect();
        Self {
            max_texture_side: 1024,
            bake_budget_cells: 4096,
            map_size: [300.0, 150.0],
            palette: MinimapPalette::default(),
            sector_colors: SectorColors::default(),
            blur_cells: BlurCells::default(),
            max_depth_cells: 6,
            foam_width: 0.09,
            foam_strength: 0.85,
            hill_shade: [0.75, 1.2],
            hill_strength: 2.2,
            elevation_cells: 14.0,
            forest_texture: 0.2,
            land_texture: 0.04,
            vacant_lot_alpha: 0.7,
            building_inset_cells: 0.14,
            shadow_offset_cells: [0.32, 0.32],
            outline_cells: 0.1,
            road_width_cells: 0.45,
            road_casing_cells: 0.75,
            field_stripes_per_cell: 1.6,
            field_texture: "minimap/field_stripes.png".into(),
            frame: "misc/square_page_bg.png".into(),
            tooltip_frame: String::new(),
            buttons: "minimap".into(),
            landmark_icons,
            alert_icon: "icons/alert_icon.png".into(),
            landmark: LandmarkStyle::default(),
            camera: CameraStyle::default(),
            ping: HexColor::rgb(0xe8432f),
            ping_opacity: 1.0,
            tooltip_opacity: 1.0,
            badge: HexColor::rgb(0xa8261c),
            badge_text: HexColor::rgb(0xfff4dc),
            lenses: LensStyle::default(),
        }
    }
}

impl Default for MinimapPalette {
    fn default() -> Self {
        Self {
            land_low: HexColor::rgb(0xa7bd72),
            land_high: HexColor::rgb(0xcdbb83),
            forest: HexColor::rgb(0x5d8c48),
            rocks: HexColor::rgb(0xa19584),
            water_shallow: HexColor::rgb(0x6db3c2),
            water_deep: HexColor::rgb(0x2f6f8c),
            foam: HexColor::rgb(0xf3f0dc),
            vacant_lot: HexColor::rgb(0xd8c99a),
            void: HexColor::rgb(0x1a1611),
            road: HexColor::rgb(0xf4e8c8),
            paved_road: HexColor::rgb(0xe2d6c0),
            road_casing: HexColor::rgba(0x6b5a40cc),
            shadow: HexColor::rgba(0x1f1a1259),
            outline: HexColor::rgba(0x2b24188c),
            field: HexColor::rgb(0xb7c560),
            background: HexColor::rgb(0x2f6f8c),
        }
    }
}

impl Default for SectorColors {
    fn default() -> Self {
        Self {
            housing: HexColor::rgb(0xe7c98b),
            food_and_farming: HexColor::rgb(0xa9b85a),
            industry_and_resources: HexColor::rgb(0xb8553f),
            services: HexColor::rgb(0x5b8fc9),
            infrastructure: HexColor::rgb(0x8d8a83),
            culture_and_religion: HexColor::rgb(0x9a5aa8),
            trade_and_economy: HexColor::rgb(0xe0a93a),
            beautification: HexColor::rgb(0x6fbf7a),
            none: HexColor::rgb(0xd9d3c4),
        }
    }
}

impl SectorColors {
    // The Sectors lens: brighter, so sectors read at a glance.
    fn lens() -> Self {
        Self {
            housing: HexColor::rgb(0xf2c45a),
            food_and_farming: HexColor::rgb(0x9cc24a),
            industry_and_resources: HexColor::rgb(0xd4463a),
            services: HexColor::rgb(0x3f8fe0),
            infrastructure: HexColor::rgb(0x7d8794),
            culture_and_religion: HexColor::rgb(0x9b59c2),
            trade_and_economy: HexColor::rgb(0xf08c2a),
            beautification: HexColor::rgb(0x3dbf8a),
            none: HexColor::rgb(0xd9d3c4),
        }
    }
}

impl Default for BlurCells {
    fn default() -> Self {
        Self { water: 0.3, vegetation: 0.4, rocks: 0.4, vacant_lot: 0.15 }
    }
}

impl Default for LandmarkStyle {
    fn default() -> Self {
        Self { fill: HexColor::rgb(0xfffaf0), ink: HexColor::rgb(0x2b2418), size: 15.0, min_cell_px: 3.0, opacity: 1.0 }
    }
}

impl Default for CameraStyle {
    fn default() -> Self {
        Self { fill: HexColor::rgba(0xffffff1f), outline: HexColor::rgb(0xfffaf0), width: 1.5, ticks: 7.0 }
    }
}

impl Default for LensStyle {
    fn default() -> Self {
        Self {
            veil: HexColor::rgba(0xd8d2c48c),
            neutral: HexColor::rgb(0xd9d3c4),
            sector_colors: SectorColors::lens(),
            water_ramp: [HexColor::rgba(0x3d8fd100), HexColor::rgba(0x3d8fd1b0)],
            water_access: HexColor::rgb(0x2f78c4),
            water_no_access: HexColor::rgb(0xe0893a),
            refresh_secs: 1.0,
        }
    }
}

impl MinimapStyle {
    pub fn load(realm: MinimapRealm) -> Result<Self, String> {
        let path = config::configs_path().join(realm.file_name());
        let json = file_sys::load_string(&path).map_err(|err| format!("Can't read minimap style {path}: {err}"))?;
        serde_json::from_str(&json).map_err(|err| format!("Invalid minimap style {path}: {err}"))
    }

    // The sprite behind tooltips.
    pub fn tooltip_frame(&self) -> &str {
        if self.tooltip_frame.is_empty() { &self.frame } else { &self.tooltip_frame }
    }

    // The map area's size, kept large enough for the lens buttons.
    pub fn map_size(&self) -> common::Vec2 {
        common::Vec2::new(self.map_size[0].max(160.0), self.map_size[1].max(80.0))
    }

    // The bake's settings.
    pub fn bake_style(&self) -> minimap_bake::BakeStyle {
        let palette = &self.palette;
        minimap_bake::BakeStyle {
            land_low: palette.land_low.rgb255(),
            land_high: palette.land_high.rgb255(),
            forest: palette.forest.rgb255(),
            rocks: palette.rocks.rgb255(),
            water_shallow: palette.water_shallow.rgb255(),
            water_deep: palette.water_deep.rgb255(),
            foam: palette.foam.rgb255(),
            vacant_lot: palette.vacant_lot.rgb255(),
            vacant_lot_alpha: self.vacant_lot_alpha,
            void: palette.void.rgb255(),
            blur_water: self.blur_cells.water,
            blur_vegetation: self.blur_cells.vegetation,
            blur_rocks: self.blur_cells.rocks,
            blur_vacant_lot: self.blur_cells.vacant_lot,
            max_depth_cells: self.max_depth_cells,
            foam_width: self.foam_width,
            foam_strength: self.foam_strength,
            hill_shade: self.hill_shade,
            hill_strength: self.hill_strength,
            elevation_cells: self.elevation_cells,
            forest_texture: self.forest_texture,
            land_texture: self.land_texture,
        }
    }
}

// ----------------------------------------------
// MinimapStyles: the current style
// ----------------------------------------------

pub struct MinimapStyles {
    current: Option<MinimapStyle>,
    realm: MinimapRealm,
    version: u32, // Goes up each time the style changes, so the minimap rebuilds what it baked.
}

impl MinimapStyles {
    const fn new() -> Self {
        Self { current: None, realm: MinimapRealm::DaiLong, version: 0 }
    }
}

common::singleton! { MINIMAP_STYLES, MinimapStyles }

// The current style, loaded on first use. A file that fails to load logs an error and leaves
// the defaults.
pub fn current() -> &'static MinimapStyle {
    let styles = MinimapStyles::get_mut();
    if styles.current.is_none() {
        let style = MinimapStyle::load(styles.realm).unwrap_or_else(|err| {
            log::error!(log::channel!("minimap"), "{err}");
            MinimapStyle::default()
        });
        styles.current = Some(style);
        styles.version += 1;
    }
    styles.current.as_ref().unwrap()
}

pub fn current_realm() -> MinimapRealm {
    MinimapStyles::get().realm
}

pub fn version() -> u32 {
    current();
    MinimapStyles::get().version
}

// Loads the style of `realm` (the current one if None) again. Keeps the current style if the
// file fails to load.
pub fn reload(realm: Option<MinimapRealm>) -> Result<MinimapRealm, String> {
    let styles = MinimapStyles::get_mut();
    let realm = realm.unwrap_or(styles.realm);
    let style = MinimapStyle::load(realm)?;
    styles.current = Some(style);
    styles.realm = realm;
    styles.version += 1;
    log::info!(log::channel!("minimap"), "Loaded minimap style '{realm}'.");
    Ok(realm)
}
