// Minimap lenses: one data layer over a dimmed base. A lens has a heat field (a value per cell,
// drawn as a small smoothly filtered texture) and colours for the buildings it cares about.
//
// Each lens comes from a provider, a game system that owns its data. City (the plain map),
// Sectors, Water and Risks (M6) exist now; Faith (M8), Floods and Fertility (M7) land with their
// milestones, each with its provider, and only then show in the lens bar.

use std::collections::BTreeMap;

use strum::{Display, EnumIter, EnumString};

use common::{Color, Size, coords::Cell};

use super::{
    model::MinimapFeatures,
    style::MinimapStyle,
};
use crate::{
    building::{
        BuildingKind,
        config::BuildingConfigs,
        risks::{MAX_RISK, RiskKind},
    },
    sim::SimContext,
    tile::sets::TileSector,
};

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum MinimapLens {
    #[default]
    City,
    Sectors,
    Water,
    Risk,
    Faith,
    Flood,
    Fertility,
}

impl MinimapLens {
    // The tooltip of its button.
    pub fn label(self) -> &'static str {
        use engine::ui::text::{UiTextCategory, get};
        match self {
            Self::City => get(UiTextCategory::Menu, "minimap.lens.city"),
            Self::Sectors => get(UiTextCategory::Menu, "minimap.lens.sectors"),
            Self::Water => get(UiTextCategory::Menu, "minimap.lens.water"),
            Self::Risk => get(UiTextCategory::Menu, "minimap.lens.risk"),
            Self::Faith => get(UiTextCategory::Menu, "minimap.lens.faith"),
            Self::Flood => get(UiTextCategory::Menu, "minimap.lens.flood"),
            Self::Fertility => get(UiTextCategory::Menu, "minimap.lens.fertility"),
        }
    }

    // Its button's sprite name, under the style's button folder.
    pub fn sprite(self) -> &'static str {
        match self {
            Self::City => "lens_city",
            Self::Sectors => "lens_sectors",
            Self::Water => "lens_water",
            Self::Risk => "lens_risk",
            Self::Faith => "lens_faith",
            Self::Flood => "lens_flood",
            Self::Fertility => "lens_fertility",
        }
    }

    // Lenses with a provider, in the lens bar's order. City has none: it's the plain map.
    pub fn available() -> &'static [MinimapLens] {
        &[Self::City, Self::Sectors, Self::Water, Self::Risk]
    }
}

// ----------------------------------------------
// LensData
// ----------------------------------------------

// What a lens shows, recomputed on a timer while it's open.
#[derive(Default)]
pub struct LensData {
    pub lens: MinimapLens,
    pub values: Vec<u8>, // 0..255 per cell, row-major; empty for no heat field.
    pub ramp: [Color; 2],
    pub building_colors: BTreeMap<(i32, i32), Color>, // By base cell (y, x); others are neutral.
    pub sector_colors: bool,                          // Colour every building by its sector instead.
    pub legend: Vec<(Color, &'static str)>,
    pub version: u64,
}

impl LensData {
    pub fn clear(&mut self, lens: MinimapLens) {
        self.lens = lens;
        self.values.clear();
        self.building_colors.clear();
        self.sector_colors = false;
        self.legend.clear();
        self.version += 1;
    }

    pub fn has_heat_field(&self) -> bool {
        !self.values.is_empty()
    }

    pub fn building_color(&self, base_cell: Cell, sector: TileSector, style: &MinimapStyle) -> Option<Color> {
        if self.sector_colors {
            return Some(style.lenses.sector_colors.color(sector));
        }
        self.building_colors.get(&(base_cell.y, base_cell.x)).copied()
    }

    // RGBA8 per cell, for the lens texture: the ramp, by value.
    pub fn texels(&self, out: &mut Vec<u8>) {
        out.clear();
        out.reserve(self.values.len() * 4);
        let [low, high] = self.ramp;
        for value in &self.values {
            let t = *value as f32 / 255.0;
            let channel = |a: f32, b: f32| ((a + (b - a) * t) * 255.0).round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[channel(low.r, high.r), channel(low.g, high.g), channel(low.b, high.b), channel(low.a, high.a)]);
        }
    }
}

// ----------------------------------------------
// LensProvider
// ----------------------------------------------

pub struct LensContext<'a> {
    pub sim: &'a SimContext<'a>,
    pub features: &'a MinimapFeatures,
    pub map_size: Size,
    pub style: &'a MinimapStyle,
}

// Implemented by the game systems that own a lens's data.
pub trait LensProvider {
    fn lens(&self) -> MinimapLens;
    fn compute(&self, context: &LensContext, out: &mut LensData);
}

pub fn provider(lens: MinimapLens) -> Option<&'static dyn LensProvider> {
    match lens {
        MinimapLens::Sectors => Some(&SectorsLens),
        MinimapLens::Water => Some(&WaterLens),
        MinimapLens::Risk => Some(&RiskLens),
        _ => None,
    }
}

// ----------------------------------------------
// Sectors
// ----------------------------------------------

// Every building in a bright colour for its sector; no heat field.
pub struct SectorsLens;

impl LensProvider for SectorsLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Sectors
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        out.sector_colors = true;
        let colors = &context.style.lenses.sector_colors;
        out.legend = vec![
            (colors.housing.color(), "Housing"),
            (colors.food_and_farming.color(), "Food"),
            (colors.industry_and_resources.color(), "Industry"),
            (colors.services.color(), "Services"),
            (colors.infrastructure.color(), "Infrastructure"),
            (colors.culture_and_religion.color(), "Culture"),
            (colors.trade_and_economy.color(), "Trade"),
            (colors.beautification.color(), "Beauty"),
        ];
    }
}

// ----------------------------------------------
// Water
// ----------------------------------------------

// Well coverage: the heat field is the wells' reach, and houses show whether they have water,
// as a house decides it (a well within its effect radius of the house's road link).
pub struct WaterLens;

impl LensProvider for WaterLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Water
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        let style = &context.style.lenses;
        let (width, height) = (context.map_size.width, context.map_size.height);
        out.values = vec![0; (width.max(0) * height.max(0)) as usize];
        out.ramp = [style.water_ramp[0].color(), style.water_ramp[1].color()];

        let wells = BuildingKind::SmallWell | BuildingKind::LargeWell;
        let world = context.sim.world();

        // The reach of each well: strongest at the well, fading to its effect radius.
        world.for_each_building(wells, |well| {
            if well.is_under_construction() {
                return true;
            }
            let radius = BuildingConfigs::get().find_service_config(well.kind()).effect_radius.max(1);
            let range = well.cell_range();
            let (cx, cy) = ((range.start.x + range.end.x) as f32 * 0.5, (range.start.y + range.end.y) as f32 * 0.5);
            let reach = radius as f32 + 0.5;
            for y in (cy - reach).floor() as i32..=(cy + reach).ceil() as i32 {
                for x in (cx - reach).floor() as i32..=(cx + reach).ceil() as i32 {
                    if x < 0 || y < 0 || x >= width || y >= height {
                        continue;
                    }
                    let distance = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                    if distance > reach {
                        continue;
                    }
                    let value = (0.45 + 0.55 * (1.0 - distance / reach)) * 255.0;
                    let cell = &mut out.values[(x + y * width) as usize];
                    *cell = (*cell).max(value.round() as u8);
                }
            }
            true
        });

        // Houses: blue with water, orange without.
        let (access, no_access) = (style.water_access.color(), style.water_no_access.color());
        world.for_each_building(BuildingKind::House, |house| {
            let has_water = house.road_link().is_some_and(|road_link| {
                [BuildingKind::SmallWell, BuildingKind::LargeWell].into_iter().any(|kind| {
                    let config = BuildingConfigs::get().find_service_config(kind);
                    context.sim.is_near_building(road_link, kind, config.requires_road_access, config.effect_radius.max(1))
                })
            });
            let base = house.base_cell();
            out.building_colors.insert((base.y, base.x), if has_water { access } else { no_access });
            true
        });

        out.legend = vec![(access, "Has water"), (no_access, "No water"), (out.ramp[1], "Well reach")];
    }
}

// ----------------------------------------------
// Risk
// ----------------------------------------------

// How far past its footprint a building's risk fades out in the heat field, in cells.
const RISK_FADE_CELLS: f32 = 1.5;

// City risks (docs/plans/city-risks.md, decision Q10: one lens for every risk): each building
// with risks takes the colour of its highest risk, fire (risk_fire), collapse (risk_collapse) or
// disease (risk_disease), mixed from risk_low by how high it is; ties go to fire, then collapse.
// A burning one is risk_burning and a sick house risk_diseased. The heat field is the highest
// risk over the building's cells, whichever it is, fading out a cell or two past them. Sites and
// buildings whose configs give them no risks stay neutral.
pub struct RiskLens;

impl LensProvider for RiskLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Risk
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        let style = &context.style.lenses;
        let (width, height) = (context.map_size.width, context.map_size.height);
        out.values = vec![0; (width.max(0) * height.max(0)) as usize];
        out.ramp = [style.risk_ramp[0].color(), style.risk_ramp[1].color()];

        let low = style.risk_low.color();
        let (fire, collapse, disease) = (style.risk_fire.color(), style.risk_collapse.color(), style.risk_disease.color());
        let (burning, diseased) = (style.risk_burning.color(), style.risk_diseased.color());

        context.sim.world().for_each_building_of_any_kind(|building| {
            if !building.has_risks() {
                return true;
            }
            let risks = building.risks();
            let (color, heat) = if risks.is_burning() {
                (burning, 1.0)
            } else if risks.has_outbreak() {
                (diseased, 1.0)
            } else {
                // The highest risk; ties go to the first.
                let (highest, top) = [(RiskKind::Fire, fire), (RiskKind::Collapse, collapse), (RiskKind::Disease, disease)]
                    .into_iter()
                    .map(|(kind, color)| (risks.get(kind), color))
                    .fold((0.0_f32, fire), |best, next| if next.0 > best.0 { next } else { best });
                let share = (highest / MAX_RISK).clamp(0.0, 1.0);
                (mix(low, top, share), share)
            };
            let base = building.base_cell();
            out.building_colors.insert((base.y, base.x), color);

            if heat <= 0.0 {
                return true;
            }
            // Full over the footprint, fading with the distance from it.
            let range = building.cell_range();
            let reach = RISK_FADE_CELLS.ceil() as i32;
            for y in (range.start.y - reach).max(0)..=(range.end.y + reach).min(height - 1) {
                for x in (range.start.x - reach).max(0)..=(range.end.x + reach).min(width - 1) {
                    let dx = (range.start.x - x).max(x - range.end.x).max(0) as f32;
                    let dy = (range.start.y - y).max(y - range.end.y).max(0) as f32;
                    let distance = (dx * dx + dy * dy).sqrt();
                    if distance > RISK_FADE_CELLS {
                        continue;
                    }
                    let value = heat * (1.0 - distance / (RISK_FADE_CELLS + 0.5)) * 255.0;
                    let cell = &mut out.values[(x + y * width) as usize];
                    *cell = (*cell).max(value.round() as u8);
                }
            }
            true
        });

        out.legend = vec![
            (low, "Low risk"),
            (fire, "Fire risk"),
            (collapse, "Collapse risk"),
            (disease, "Disease risk"),
            (burning, "On fire"),
            (diseased, "Sickness"),
        ];
    }
}

// `a` to `b` by `t` (0 to 1), channel by channel.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let channel = |a: f32, b: f32| a + (b - a) * t;
    Color::new(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b), channel(a.a, b.a))
}

// The lens's colour for a feature: what the provider said, or None (neutral).
pub fn feature_color(data: &LensData, features: &MinimapFeatures, base_cell: Cell, style: &MinimapStyle) -> Option<Color> {
    let feature = features.get(base_cell)?;
    data.building_color(base_cell, feature.sector, style)
}
