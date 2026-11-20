// Farmsteads and their rice fields (docs/plans/seasons-floods.md, Phase 1).
//
// A farmstead is a producer whose config has a `crop` block (building::producer::CropConfig): it
// runs no production cycle. The player lays rice fields around it, plain tiles of the objects tile
// set's `fields` category (TileKind::Field), within its `field_reach`. The farmstead keeps the
// fields it owns and their crops (ProducerBuilding::fields, nearest first); its workers tend
// `cells_per_worker` fields each, the nearest first. Once a month (on_new_month), each tended field
// is sown in its crop's season and reaped `grow_months` later, its harvest the crop's
// `harvest_per_cell` times its land's yield (sim::floods) and the share of its months it was
// tended; the farmstead's fields reaped that month go into its stock together, and its runner
// carries them to the granaries a load at a time. A field's tile variation is its stage.
//
// A field that water reaches (irrigation, Phase 6: open water beside it, a canal carrying water, a
// sluice gate's outlet, or a wet field next to it) is sown in the other season too, for a second
// harvest. A sluice gate (BuildingKind::SluiceGate, Phase 4), placed beside a dyke, sends water
// through its outlet while it works and has water across its dyke; when the flood stands against
// its dyke, the fields its canals reach take the flood's silt without the water (crate::floods).

pub mod irrigation;

use std::collections::HashMap;

use common::{
    coords::{Cell, CellRange},
    mem::SingleThreadStatic,
};
pub use irrigation::{GateSupply, Irrigation, WaterSource};
use serde::{Deserialize, Serialize};

use crate::{
    building::{Building, BuildingArchetypeKind, BuildingKind, BuildingKindAndId, producer::CropConfig},
    config::GameConfigs,
    sim::{
        SimContext,
        SimContextMut,
        calendar::{CalendarDate, CalendarSettings, MONTHS_PER_YEAR},
        resources::ResourceKind,
    },
    tile::{TileKind, TileMap, TileMapLayerKind, sets::TileDef},
    world::World,
};

// ----------------------------------------------
// FieldStage, GrowingCrop, FieldPlot
// ----------------------------------------------

// A field's look, its tile's variation index (the field tile's variations, in this order).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FieldStage {
    Fallow,
    Sown,
    Growing,
    Ripe,
}

impl FieldStage {
    #[inline]
    pub fn variation(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Fallow => "fallow",
            Self::Sown => "sown",
            Self::Growing => "growing",
            Self::Ripe => "ripe",
        }
    }
}

// A crop in a field: the ordinal month it was sown in, the months it has grown since, and how many
// of them it was tended.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct GrowingCrop {
    pub sown: u32,
    pub months: u32,
    pub tended_months: u32,
}

// A field a farmstead owns.
#[derive(Clone, Serialize, Deserialize)]
pub struct FieldPlot {
    pub cell: Cell,
    pub crop: Option<GrowingCrop>,
    // The ordinal month the season it was last sown in began: a field is sown once a season.
    pub sown_season: Option<u32>,
}

impl FieldPlot {
    #[inline]
    pub fn new(cell: Cell) -> Self {
        Self { cell, crop: None, sown_season: None }
    }

    // Its stage in month `now` (ordinal): fallow without a crop; sown in its first month; ripe in its
    // last before reaping; growing between.
    pub fn stage(&self, now: u32, grow_months: u32) -> FieldStage {
        match self.crop {
            None => FieldStage::Fallow,
            Some(crop) => {
                let age = now.saturating_sub(crop.sown);
                if age == 0 {
                    FieldStage::Sown
                } else if age + 1 >= grow_months {
                    FieldStage::Ripe
                } else {
                    FieldStage::Growing
                }
            }
        }
    }

    // The ordinal month its crop is reaped in, if it has one.
    #[inline]
    pub fn reaped_in(&self, grow_months: u32) -> Option<u32> {
        self.crop.map(|crop| crop.sown + grow_months)
    }
}

// ----------------------------------------------
// Calendar helpers
// ----------------------------------------------

#[inline]
fn month_of(ordinal: u32) -> u32 {
    ordinal % MONTHS_PER_YEAR + 1
}

// The ordinal month the season `ordinal` is in began (looking back at most a year).
pub fn season_start(settings: &CalendarSettings, ordinal: u32) -> u32 {
    let season = settings.season_of(month_of(ordinal));
    let mut start = ordinal;
    for _ in 1..MONTHS_PER_YEAR {
        if start == 0 || settings.season_of(month_of(start - 1)) != season {
            break;
        }
        start -= 1;
    }
    start
}

// The ordinal month now.
pub fn current_month(context: &SimContext) -> u32 {
    let sim = &GameConfigs::get().sim;
    context.calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month()
}

// ----------------------------------------------
// Farmsteads and fields on the map
// ----------------------------------------------

#[inline]
pub fn is_field_def(tile_def: &TileDef) -> bool {
    tile_def.is(TileKind::Field)
}

// The farmstead's crop, if the building is one.
pub fn crop_of(building: &Building) -> Option<&'static CropConfig> {
    if building.archetype_kind() != BuildingArchetypeKind::ProducerBuilding {
        return None;
    }
    building.as_producer().crop()
}

// A farmstead, as the passes over them need it.
#[derive(Copy, Clone)]
struct Farmstead {
    id: BuildingKindAndId,
    cells: CellRange,
    crop: &'static CropConfig,
}

fn farmsteads(world: &World) -> Vec<Farmstead> {
    let mut list = Vec::new();
    world.for_each_building(BuildingKind::Farm, |building| {
        if let Some(crop) = crop_of(building) {
            list.push(Farmstead { id: building.kind_and_id(), cells: building.cell_range(), crop });
        }
        true
    });
    list
}

// How many cells lie between a footprint and a cell along the grid (1 next to it).
#[inline]
fn gap(footprint: CellRange, cell: Cell) -> i32 {
    let dx = (cell.x - footprint.end.x).max(footprint.start.x - cell.x).max(0);
    let dy = (cell.y - footprint.end.y).max(footprint.start.y - cell.y).max(0);
    dx.max(dy)
}

// Whether a field tile of the crop stands at the cell.
fn has_field(tile_map: &TileMap, cell: Cell, crop: &CropConfig) -> bool {
    tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Objects)
        .is_some_and(|tile| tile.is(TileKind::Field) && tile.tile_def().name == crop.field_tile)
}

// The farmstead nearest to a cell whose fields of `field_def` it would own, within its reach; the
// first of equals in the world's order.
pub fn farmstead_for_field(world: &World, field_def: &TileDef, cell: Cell) -> Option<BuildingKindAndId> {
    farmsteads(world)
        .into_iter()
        .filter(|farmstead| farmstead.crop.field_tile == field_def.name)
        .map(|farmstead| (gap(farmstead.cells, cell), farmstead))
        .filter(|(gap, farmstead)| *gap > 0 && *gap <= farmstead.crop.field_reach)
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, farmstead)| farmstead.id)
}

// The farmstead that owns the field at a cell, if one does.
pub fn owner_of_field(world: &World, cell: Cell) -> Option<BuildingKindAndId> {
    let mut owner = None;
    world.for_each_building(BuildingKind::Farm, |building| {
        if crop_of(building).is_some() && building.as_producer().fields().iter().any(|plot| plot.cell == cell) {
            owner = Some(building.kind_and_id());
            return false;
        }
        true
    });
    owner
}

// Every farmstead drops the fields that are gone and claims the fields in its reach that have no
// farmstead, each field going to the nearest; then keeps its fields nearest first. Runs when the
// player lays fields or a farmstead, clears one, and at the start of each month.
pub fn claim_fields(context: &SimContextMut) {
    let farmsteads = farmsteads(context.world());
    if farmsteads.is_empty() {
        return;
    }
    let tile_map = context.tile_map();

    // Drop the fields that are gone.
    let mut owned: HashMap<Cell, usize> = HashMap::new();
    for (index, farmstead) in farmsteads.iter().enumerate() {
        let Some(building) = context.find_building_mut(farmstead.id.kind, farmstead.id.id) else {
            continue;
        };
        let fields = building.as_producer_mut().fields_mut();
        fields.retain(|plot| has_field(tile_map, plot.cell, farmstead.crop));
        for plot in fields.iter() {
            owned.entry(plot.cell).or_insert(index);
        }
    }

    // The fields in reach without a farmstead, each to its nearest.
    let size = tile_map.size_in_cells();
    let mut claims: Vec<Vec<Cell>> = vec![Vec::new(); farmsteads.len()];
    let mut seen: HashMap<Cell, ()> = HashMap::new();
    for farmstead in &farmsteads {
        let reach = farmstead.crop.field_reach;
        let (start, end) = (farmstead.cells.start, farmstead.cells.end);
        for y in (start.y - reach).max(0)..=(end.y + reach).min(size.height - 1) {
            for x in (start.x - reach).max(0)..=(end.x + reach).min(size.width - 1) {
                let cell = Cell::new(x, y);
                if owned.contains_key(&cell) || seen.insert(cell, ()).is_some() {
                    continue;
                }
                let nearest = farmsteads
                    .iter()
                    .enumerate()
                    .filter(|(_, other)| has_field(tile_map, cell, other.crop))
                    .map(|(index, other)| (gap(other.cells, cell), index, other))
                    .filter(|(gap, _, other)| *gap > 0 && *gap <= other.crop.field_reach)
                    .min_by_key(|(gap, index, _)| (*gap, *index));
                if let Some((_, index, _)) = nearest {
                    claims[index].push(cell);
                }
            }
        }
    }

    for (farmstead, cells) in farmsteads.iter().zip(claims) {
        let Some(building) = context.find_building_mut(farmstead.id.kind, farmstead.id.id) else {
            continue;
        };
        let fields = building.as_producer_mut().fields_mut();
        fields.extend(cells.into_iter().map(FieldPlot::new));
        let footprint = farmstead.cells;
        fields.sort_by_key(|plot| (gap(footprint, plot.cell), plot.cell.y, plot.cell.x));
    }
}

// ----------------------------------------------
// The month's work
// ----------------------------------------------

// A month's harvests, for the message log (GameLoop::notify_farms).
#[derive(Clone, Debug)]
pub struct HarvestNotice {
    pub farmsteads: u32,
    pub fields: u32,
    pub amount: u32,
    pub good: ResourceKind,
}

static NOTICES: SingleThreadStatic<Vec<HarvestNotice>> = SingleThreadStatic::new(Vec::new());

pub fn take_notices() -> Vec<HarvestNotice> {
    std::mem::take(NOTICES.as_mut())
}

// ----------------------------------------------
// Sluice gates
// ----------------------------------------------

// A sluice gate, as the passes over the fields need it: its footprint and whether it works now
// (finished, staffed, not burning). What it waters is its canals' reach (irrigation).
#[derive(Copy, Clone)]
pub struct SluiceGate {
    pub id: BuildingKindAndId,
    pub cells: CellRange,
    pub working: bool,
}

// The sluice gate at a building, if it's one.
pub fn sluice_gate_of(building: &Building) -> Option<SluiceGate> {
    if !building.is(BuildingKind::SluiceGate) {
        return None;
    }
    Some(SluiceGate { id: building.kind_and_id(), cells: building.cell_range(), working: building.is_operational() })
}

// Every sluice gate, working or not, in the world's order.
pub fn sluice_gates(world: &World) -> Vec<SluiceGate> {
    let mut gates = Vec::new();
    world.for_each_building(BuildingKind::SluiceGate, |building| {
        gates.extend(sluice_gate_of(building));
        true
    });
    gates
}

// The sides a sluice gate can face its dyke on, as its tile's variations are named: the engine's
// north (+x), east (-y), south (-x) and west (+y).
pub(crate) const GATE_SIDES: [(&str, i32, i32); 4] = [("north", 1, 0), ("east", 0, -1), ("south", -1, 0), ("west", 0, 1)];

// A sluice gate just placed faces the dyke beside it: its tile's variation is the first side, in
// GATE_SIDES' order, with a dyke past its footprint.
pub fn face_dyke(tile_map: &mut TileMap, cell: Cell) {
    let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) else {
        return;
    };
    let def = tile.tile_def();
    let footprint = def.cell_range(tile.base_cell());
    let side = GATE_SIDES.iter().find(|(_, dx, dy)| {
        footprint.iter().any(|cell| {
            let next = Cell::new(cell.x + dx, cell.y + dy);
            !footprint.contains(next) && crate::sim::floods::is_dyke(tile_map, next)
        })
    });
    let Some(index) = side.and_then(|(name, _, _)| def.find_variation(name)) else {
        return;
    };
    let base = footprint.start;
    if let Some(tile) = tile_map.try_tile_from_layer_mut(base, TileMapLayerKind::Objects) {
        tile.set_variation_index(index);
    }
}

// The fields the flood's silt reaches through the sluice gates (crate::floods): those a working
// gate's outlet reaches through canals and fields (irrigation::fields_reached_by), not under water
// themselves, where the water stands against the gate's dyke (`met`, the dykes the flood reached)
// or covers the gate. The flood's water is the gate's intake then. In row order.
pub fn sluiced_fields(context: &SimContext, met: &[Cell], is_under_water: impl Fn(Cell) -> bool) -> Vec<Cell> {
    let irrigation = Irrigation::read(context.world(), context.tile_map());
    let mut cells = Vec::new();
    for supply in irrigation.gates().iter().filter(|supply| supply.gate.working) {
        let mut open = supply.gate.cells.iter().any(&is_under_water);
        if !open {
            crate::pathfind::for_each_surrounding_cell(supply.gate.cells, |cell| {
                open = met.contains(&cell);
                !open
            });
        }
        if open {
            let fields = irrigation::fields_reached_by(context.tile_map(), supply);
            cells.extend(fields.into_iter().filter(|&cell| !is_under_water(cell)));
        }
    }
    cells.sort_by_key(|cell| (cell.y, cell.x));
    cells.dedup();
    cells
}

// The start of a month (`date`): every farmstead claims its fields, then each field moves on a
// month: a crop grows (tended or not) and is reaped once it's ripe; a tended field without one is
// sown if it can be (in its crop's season, or in either where water reaches it); a field under
// water loses its crop. The fields' tiles show their stage, and the canals whether they're dry.
pub fn on_new_month(context: &SimContextMut, date: CalendarDate) {
    claim_fields(context);

    let now = date.ordinal_month();
    let settings = context.calendar().settings();
    let season = settings.season_of(date.month);
    let season_began = season_start(settings, now);
    let depletion = GameConfigs::get().sim.floods.depletion_per_harvest.min(u8::MAX as u32) as u8;
    let irrigation = Irrigation::read(context.world(), context.tile_map());

    let mut notice = HarvestNotice { farmsteads: 0, fields: 0, amount: 0, good: ResourceKind::Rice };
    for farmstead in farmsteads(context.world()) {
        let crop = farmstead.crop;
        let Some(building) = context.find_building_mut(farmstead.id.kind, farmstead.id.id) else {
            continue;
        };
        let producer = building.as_producer_mut();
        let tended = if building_is_finished(context, farmstead.id) { producer.tended_field_count() } else { 0 };
        let faith_scale = context.faith().production_scale(farmstead.id.kind);

        let mut harvest = 0.0;
        let mut reaped = Vec::new();
        let mut stages = Vec::new();
        for (index, plot) in producer.fields_mut().iter_mut().enumerate() {
            let is_tended = index < tended;
            if context.floods().is_flooded(plot.cell) {
                // The water takes the crop; the field is sown again once it goes.
                plot.crop = None;
                plot.sown_season = None;
                stages.push((plot.cell, FieldStage::Fallow));
                continue;
            }

            if let Some(growing) = &mut plot.crop {
                growing.months += 1;
                if is_tended {
                    growing.tended_months += 1;
                }
                if now.saturating_sub(growing.sown) >= crop.grow_months {
                    let share = growing.tended_months as f32 / growing.months.max(1) as f32;
                    harvest += crop.harvest_per_cell * context.floods().yield_factor_at(plot.cell) * share;
                    reaped.push(plot.cell);
                    plot.crop = None;
                }
            }

            let can_sow = crop.season == season || irrigation.is_wet(plot.cell);
            if plot.crop.is_none() && is_tended && can_sow && plot.sown_season != Some(season_began) {
                plot.crop = Some(GrowingCrop { sown: now, months: 0, tended_months: 0 });
                plot.sown_season = Some(season_began);
            }
            stages.push((plot.cell, plot.stage(now, crop.grow_months)));
        }

        if !reaped.is_empty() {
            let amount = (harvest * faith_scale).round() as u32;
            let stored = producer.store_harvest(amount);
            notice.farmsteads += 1;
            notice.fields += reaped.len() as u32;
            notice.amount += stored;
            notice.good = producer.production_output_stock.item.kind;
            for cell in reaped {
                let floods = context.floods_mut();
                let fertility = floods.fertility_at(cell);
                floods.set_fertility(cell, fertility.saturating_sub(depletion));
            }
        }

        let tile_map = context.tile_map_mut();
        for (cell, stage) in stages {
            if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects)
                && tile.is(TileKind::Field)
                && tile.variation_index() != stage.variation()
            {
                tile.set_variation_index(stage.variation());
            }
        }
    }

    if notice.farmsteads != 0 {
        NOTICES.as_mut().push(notice);
    }

    // The flood may have breached a dyke a gate drew across.
    irrigation::refresh_canals(context);
}

// The flood (crate::floods) takes the crops of the fields under water: they lie fallow until the
// water goes and may be sown again then, as the season allows. How many field tiles it covers.
pub fn flood_fields(context: &SimContextMut, is_under_water: impl Fn(Cell) -> bool) -> u32 {
    for farmstead in farmsteads(context.world()) {
        let Some(building) = context.find_building_mut(farmstead.id.kind, farmstead.id.id) else {
            continue;
        };
        for plot in building.as_producer_mut().fields_mut() {
            if is_under_water(plot.cell) {
                plot.crop = None;
                plot.sown_season = None;
            }
        }
    }

    let tile_map = context.tile_map_mut();
    let mut count = 0;
    for cell in field_cells(tile_map) {
        if !is_under_water(cell) {
            continue;
        }
        count += 1;
        if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Objects)
            && tile.variation_index() != FieldStage::Fallow.variation()
        {
            tile.set_variation_index(FieldStage::Fallow.variation());
        }
    }
    count
}

// A farmstead still a construction site tends nothing.
fn building_is_finished(context: &SimContext, id: BuildingKindAndId) -> bool {
    context.find_building(id.kind, id.id).is_some_and(|building| !building.is_under_construction())
}

// ----------------------------------------------
// Reports
// ----------------------------------------------

// A farmstead's fields at a glance (the inspector, farms.state).
#[derive(Clone, Default, Debug)]
pub struct FarmsteadReport {
    pub fields: usize,
    // Tended by its workers now; the rest lie fallow for want of them.
    pub tended: usize,
    pub growing: usize,
    // The ordinal month its next fields are reaped in, if any are growing.
    pub next_reaped: Option<u32>,
    // What its growing fields would give at their staffing so far.
    pub expected: f32,
}

pub fn farmstead_report(context: &SimContext, building: &Building) -> Option<FarmsteadReport> {
    let crop = crop_of(building)?;
    let producer = building.as_producer();
    let tended = if building.is_under_construction() { 0 } else { producer.tended_field_count() };
    let faith_scale = context.faith().production_scale(building.kind());
    let mut report = FarmsteadReport { fields: producer.fields().len(), tended, ..Default::default() };
    for (index, plot) in producer.fields().iter().enumerate() {
        let Some(growing) = plot.crop else {
            continue;
        };
        report.growing += 1;
        let reaped = growing.sown + crop.grow_months;
        report.next_reaped = Some(report.next_reaped.map_or(reaped, |next| next.min(reaped)));
        let share = if growing.months == 0 {
            if index < tended { 1.0 } else { 0.0 }
        } else {
            growing.tended_months as f32 / growing.months as f32
        };
        report.expected += crop.harvest_per_cell * context.floods().yield_factor_at(plot.cell) * share * faith_scale;
    }
    Some(report)
}

// A field and what's in it (the inspector, farms.state).
#[derive(Clone)]
pub struct FieldReport {
    pub cell: Cell,
    pub farmstead: Option<BuildingKindAndId>,
    pub farmstead_cell: Option<Cell>,
    pub stage: FieldStage,
    pub tended: bool,
    pub sown: Option<u32>,
    pub reaped_in: Option<u32>,
    // Water reaches it (irrigation): it's sown in either season.
    pub irrigated: bool,
    // Where that water comes from: the river, or the sluice gate at a cell.
    pub water: Option<WaterSource>,
    pub flooded: bool,
}

pub fn field_report(context: &SimContext, cell: Cell) -> Option<FieldReport> {
    field_report_with(context, cell, &Irrigation::read(context.world(), context.tile_map()))
}

// field_report() with the irrigation read already, for a report on many fields.
pub fn field_report_with(context: &SimContext, cell: Cell, irrigation: &Irrigation) -> Option<FieldReport> {
    let tile = context.tile_map().try_tile_from_layer(cell, TileMapLayerKind::Objects)?;
    if !tile.is(TileKind::Field) {
        return None;
    }
    let now = current_month(context);
    let mut report = FieldReport {
        cell,
        farmstead: None,
        farmstead_cell: None,
        stage: FieldStage::Fallow,
        tended: false,
        sown: None,
        reaped_in: None,
        irrigated: false,
        water: None,
        flooded: context.floods().is_flooded(cell),
    };
    report.water = irrigation.source(cell);
    report.irrigated = report.water.is_some();
    if let Some(owner) = owner_of_field(context.world(), cell)
        && let Some(building) = context.find_building(owner.kind, owner.id)
        && let Some(crop) = crop_of(building)
    {
        let producer = building.as_producer();
        let tended = if building.is_under_construction() { 0 } else { producer.tended_field_count() };
        if let Some((index, plot)) = producer.fields().iter().enumerate().find(|(_, plot)| plot.cell == cell) {
            report.farmstead = Some(owner);
            report.farmstead_cell = Some(building.base_cell());
            report.stage = plot.stage(now, crop.grow_months);
            report.tended = index < tended;
            report.sown = plot.crop.map(|growing| growing.sown);
            report.reaped_in = plot.reaped_in(crop.grow_months);
        }
    }
    Some(report)
}

// Every field tile on the map, in row order.
pub fn field_cells(tile_map: &TileMap) -> Vec<Cell> {
    let mut cells = Vec::new();
    tile_map.for_each_tile(TileKind::Field, |_, tile| cells.push(tile.base_cell()));
    cells.sort_by_key(|cell| (cell.y, cell.x));
    cells
}
