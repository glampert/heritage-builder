// Minimap lenses: one data layer over a dimmed base. A lens has a heat field (a value per cell,
// drawn as a small smoothly filtered texture) and colours for the buildings it cares about.
//
// Each lens comes from a provider, a game system that owns its data: City (the plain map), Sectors,
// Water, Risks (M6), Faith (M8), and Flood and Fertility (M7, offered only while the game's river
// floods).

use std::collections::BTreeMap;

use common::{
    Color,
    Size,
    coords::{Cell, CellRange},
};
use strum::{Display, EnumIter, EnumString};

use super::{model::MinimapFeatures, style::MinimapStyle};
use crate::{
    building::{
        BuildingKind,
        config::BuildingConfigs,
        footprints_within_reach,
        risks::{MAX_RISK, RiskKind},
    },
    faith,
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
    pub const WITH_PROVIDERS: &'static [MinimapLens] =
        &[Self::City, Self::Sectors, Self::Water, Self::Risk, Self::Faith, Self::Flood, Self::Fertility];

    // Whether the game offers it now: the Flood and Fertility lenses only while its river floods.
    pub fn is_offered(self, floods_enabled: bool) -> bool {
        Self::WITH_PROVIDERS.contains(&self) && (!matches!(self, Self::Flood | Self::Fertility) || floods_enabled)
    }

    // The lenses the game offers now, in the lens bar's order.
    pub fn offered(floods_enabled: bool) -> Vec<MinimapLens> {
        Self::WITH_PROVIDERS.iter().copied().filter(|lens| lens.is_offered(floods_enabled)).collect()
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
    pub cell_colors: Vec<(Cell, Color)>,              // Marks on single cells, over the roads (dykes).
    pub sector_colors: bool,                          // Colour every building by its sector instead.
    pub legend: Vec<(Color, String)>,                 // Swatches and their labels, under the map.
    pub legend_max_columns: usize,                    // At most this many columns of them (0: as many as fit).
    pub version: u64,
}

impl LensData {
    pub fn clear(&mut self, lens: MinimapLens) {
        self.lens = lens;
        self.values.clear();
        self.building_colors.clear();
        self.cell_colors.clear();
        self.sector_colors = false;
        self.legend.clear();
        self.legend_max_columns = 0;
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
            out.extend_from_slice(&[
                channel(low.r, high.r),
                channel(low.g, high.g),
                channel(low.b, high.b),
                channel(low.a, high.a),
            ]);
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
        MinimapLens::Faith => Some(&FaithLens),
        MinimapLens::Flood => Some(&FloodLens),
        MinimapLens::Fertility => Some(&FertilityLens),
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
        out.legend = legend(&[
            (colors.housing.color(), "Housing"),
            (colors.food_and_farming.color(), "Food"),
            (colors.industry_and_resources.color(), "Industry"),
            (colors.services.color(), "Services"),
            (colors.infrastructure.color(), "Infrastructure"),
            (colors.culture_and_religion.color(), "Culture"),
            (colors.trade_and_economy.color(), "Trade"),
            (colors.beautification.color(), "Beauty"),
        ]);
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

        out.legend = legend(&[(access, "Has water"), (no_access, "No water"), (out.ramp[1], "Well reach")]);
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

        out.legend = legend(&[
            (low, "Low risk"),
            (fire, "Fire risk"),
            (collapse, "Collapse risk"),
            (disease, "Disease risk"),
            (burning, "On fire"),
            (diseased, "Sickness"),
        ]);
    }
}

// ----------------------------------------------
// Faith
// ----------------------------------------------

// The patrons (docs/plans/faith.md, §5): each shrine and temple in its patron's colour (the realm's
// faith file), each house from faith_none to faith_all by how many patrons' working shrines and
// temples reach it, and the heat field their reach, fading to their effect radius as the wells'
// does. The legend lists the patrons.
pub struct FaithLens;

impl LensProvider for FaithLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Faith
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        let style = &context.style.lenses;
        let (width, height) = (context.map_size.width, context.map_size.height);
        out.values = vec![0; (width.max(0) * height.max(0)) as usize];
        out.ramp = [style.faith_ramp[0].color(), style.faith_ramp[1].color()];
        let (none, all) = (style.faith_none.color(), style.faith_all.color());

        let realm = context.sim.realm();
        let Some(def) = faith::FaithConfigs::get().for_realm(realm) else {
            return;
        };
        let world = context.sim.world();

        // Shrines and temples in their patron's colour; the working ones' reach.
        let mut tenders: Vec<(usize, CellRange, i32)> = Vec::new();
        world.for_each_building(faith::DEDICATED_KINDS, |building| {
            let Some(index) = faith::dedicated_patron(context.sim, building)
                .and_then(|patron| def.patrons.iter().position(|other| other.id == patron.id))
            else {
                return true;
            };
            let base = building.base_cell();
            out.building_colors.insert((base.y, base.x), def.patrons[index].colour());
            if building.is_under_construction() || !building.is_operational() {
                return true;
            }
            let radius = BuildingConfigs::get().find_service_config(building.kind()).effect_radius.max(1);
            let range = building.cell_range();
            tenders.push((index, range, radius));

            let (cx, cy) = ((range.start.x + range.end.x) as f32 * 0.5, (range.start.y + range.end.y) as f32 * 0.5);
            let reach = radius as f32 + 0.5;
            for y in ((cy - reach).floor() as i32).max(0)..=((cy + reach).ceil() as i32).min(height - 1) {
                for x in ((cx - reach).floor() as i32).max(0)..=((cx + reach).ceil() as i32).min(width - 1) {
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

        // Houses by how many patrons tend them.
        let count = def.patrons.len().max(1) as f32;
        world.for_each_building(BuildingKind::House, |house| {
            let mut tended = vec![false; def.patrons.len()];
            for (index, footprint, radius) in &tenders {
                if footprints_within_reach(*footprint, house.cell_range(), *radius) {
                    tended[*index] = true;
                }
            }
            let share = tended.iter().filter(|tended| **tended).count() as f32 / count;
            let base = house.base_cell();
            out.building_colors.insert((base.y, base.x), mix(none, all, share));
            true
        });

        out.legend = def
            .patrons
            .iter()
            .map(|patron| (patron.colour(), faith::config::patron_name(realm, &patron.id)))
            .chain([(none, "No patron".to_string()), (all, "Every patron".to_string())])
            .collect();
        out.legend_max_columns = 2; // The patrons' names are long.
    }
}

// ----------------------------------------------
// Flood
// ----------------------------------------------

// The river's flood (docs/plans/seasons-floods.md, §6): the heat field is where an ordinary flood
// would reach now (the fill at the profile's largest reach), strongest by the water, with the cells
// under water at the top; buildings and fields under water take flood_under_water, those in that
// reach flood_at_risk; each dyke cell is marked from dyke_sound to dyke_worn by its wear (the reach
// stops at the dykes as they stand). Offered only while the game's river floods
// (MinimapLens::is_offered).
pub struct FloodLens;

// The heat of the ordinary reach, by the water and at its edge (a share of the ramp).
const FLOOD_REACH_HEAT: [f32; 2] = [0.65, 0.3];

impl LensProvider for FloodLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Flood
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        let style = &context.style.lenses;
        let (width, height) = (context.map_size.width.max(0), context.map_size.height.max(0));
        out.values = vec![0; (width * height) as usize];
        out.ramp = [style.flood_ramp[0].color(), style.flood_ramp[1].color()];
        let (under_water, at_risk) = (style.flood_under_water.color(), style.flood_at_risk.color());

        let floods = context.sim.floods();
        let index = |cell: Cell| {
            (cell.x >= 0 && cell.y >= 0 && cell.x < width && cell.y < height).then(|| (cell.y * width + cell.x) as usize)
        };

        // Where an ordinary flood would reach, fading away from the water; then the water now.
        let farthest = floods.profile().reach[1].max(1) as f32;
        for (cell, distance) in floods.ordinary_reach(context.sim.tile_map()) {
            if let Some(index) = index(cell) {
                let t = ((distance.max(1) - 1) as f32 / farthest).clamp(0.0, 1.0);
                let heat = FLOOD_REACH_HEAT[0] + (FLOOD_REACH_HEAT[1] - FLOOD_REACH_HEAT[0]) * t;
                out.values[index] = (heat * 255.0).round() as u8;
            }
        }
        if let Some(flood) = floods.flood() {
            for &cell in &flood.cells {
                if let Some(index) = index(cell) {
                    out.values[index] = u8::MAX;
                }
            }
        }

        // Buildings and fields: under water, or in the reach.
        let colour = |cells: CellRange| -> Option<Color> {
            let mut reached = false;
            for cell in cells.iter() {
                if floods.is_flooded(cell) {
                    return Some(under_water);
                }
                reached |= index(cell).is_some_and(|index| out.values[index] > 0);
            }
            reached.then_some(at_risk)
        };
        let mut colours = Vec::new();
        context.sim.world().for_each_building_of_any_kind(|building| {
            if let Some(color) = colour(building.cell_range()) {
                colours.push((building.base_cell(), color));
            }
            true
        });
        for cell in crate::farms::field_cells(context.sim.tile_map()) {
            if let Some(color) = colour(CellRange::new(cell, cell)) {
                colours.push((cell, color));
            }
        }
        for (base, color) in colours {
            out.building_colors.insert((base.y, base.x), color);
        }

        // The dykes, from sound to worn.
        let (sound, worn) = (style.dyke_sound.color(), style.dyke_worn.color());
        let worn_wear = crate::config::GameConfigs::get().sim.floods.worn_wear.max(1.0);
        for cell in crate::sim::floods::dyke_cells(context.sim.tile_map()) {
            let share = (floods.wear_at(cell) / worn_wear).clamp(0.0, 1.0);
            out.cell_colors.push((cell, mix(sound, worn, share)));
        }

        out.legend = legend(&[
            (under_water, "Under water"),
            (mix(out.ramp[0], out.ramp[1], 0.55), "Floods in an ordinary year"),
            (at_risk, "At risk"),
            (sound, "Dyke sound"),
            (worn, "Dyke worn"),
        ]);
        out.legend_max_columns = 2; // "Floods in an ordinary year" is long.
    }
}

// ----------------------------------------------
// Fertility
// ----------------------------------------------

// The land (docs/plans/seasons-floods.md, §6): the heat field is each land cell's fertility, from
// barren to rich; fields take the colour of their stage (growing, ripe, fallow), and those a working
// sluice gate waters, and the gates themselves, fertility_irrigated. Offered only while the game's
// river floods (MinimapLens::is_offered): without it the land is the same everywhere.
pub struct FertilityLens;

// The heat of barren and of the richest land (a share of the ramp): barren land still shows.
const FERTILITY_HEAT: [f32; 2] = [0.25, 1.0];

impl LensProvider for FertilityLens {
    fn lens(&self) -> MinimapLens {
        MinimapLens::Fertility
    }

    fn compute(&self, context: &LensContext, out: &mut LensData) {
        use crate::tile::{TileKind, TileMapLayerKind};

        let style = &context.style.lenses;
        let (width, height) = (context.map_size.width.max(0), context.map_size.height.max(0));
        out.values = vec![0; (width * height) as usize];
        out.ramp = [style.fertility_ramp[0].color(), style.fertility_ramp[1].color()];
        let (growing, ripe, fallow, irrigated) = (
            style.fertility_growing.color(),
            style.fertility_ripe.color(),
            style.fertility_fallow.color(),
            style.fertility_irrigated.color(),
        );

        let floods = context.sim.floods();
        let tile_map = context.sim.tile_map();
        let max = crate::sim::floods::MAX_FERTILITY as f32;
        let heat = |fertility: f32| FERTILITY_HEAT[0] + (FERTILITY_HEAT[1] - FERTILITY_HEAT[0]) * (fertility / max);
        for y in 0..height {
            for x in 0..width {
                let cell = Cell::new(x, y);
                let is_land = tile_map
                    .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
                    .is_some_and(|tile| !tile.path_kind().is_water());
                if is_land {
                    out.values[(y * width + x) as usize] = (heat(floods.fertility_at(cell) as f32) * 255.0).round() as u8;
                }
            }
        }

        // The fields by their stage (their tile's variation), or watered by a sluice gate.
        let gates = crate::farms::working_sluice_gates(context.sim.world());
        for cell in crate::farms::field_cells(tile_map) {
            let color = if gates.iter().any(|gate| gate.reaches(cell)) {
                irrigated
            } else {
                let stage = tile_map
                    .try_tile_from_layer(cell, TileMapLayerKind::Objects)
                    .filter(|tile| tile.is(TileKind::Field))
                    .map_or(0, |tile| tile.variation_index());
                match stage {
                    s if s == crate::farms::FieldStage::Ripe.variation() => ripe,
                    s if s == crate::farms::FieldStage::Fallow.variation() => fallow,
                    _ => growing,
                }
            };
            out.building_colors.insert((cell.y, cell.x), color);
        }
        for gate in crate::farms::sluice_gates(context.sim.world()) {
            let base = gate.cells.start;
            out.building_colors.insert((base.y, base.x), if gate.working { irrigated } else { fallow });
        }

        let poor = mix(out.ramp[0], out.ramp[1], heat(30.0));
        out.legend = legend(&[
            (out.ramp[1], "Fertile"),
            (poor, "Poor"),
            (growing, "Growing"),
            (ripe, "Ripe"),
            (fallow, "Fallow"),
            (irrigated, "Irrigated"),
        ]);
        out.legend_max_columns = 3;
    }
}

// A legend from fixed labels.
fn legend(entries: &[(Color, &str)]) -> Vec<(Color, String)> {
    entries.iter().map(|(color, label)| (*color, label.to_string())).collect()
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
