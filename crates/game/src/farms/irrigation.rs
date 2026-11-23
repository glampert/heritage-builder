// Irrigation as connected water (docs/plans/seasons-floods.md, §9, Phase 6).
//
// Water comes from open water (tile::water::is_open_water: the river, a lake, a pond; never a
// canal) and from a sluice gate's outlet while the gate supplies it. Canals and fields carry it,
// touching side by side (four neighbours): a canal or a field that touches a source, or a wet canal
// or field, is wet. A gate supplies water while it works and has water across its dyke (its
// intake): open water, or a canal the river fills, within GATE_INTAKE_CELLS cells straight out
// past the dyke on the side the gate faces (its tile's variation). Its outlet is the cell past its
// footprint on the other side.
//
// On raised ground (docs/plans/terrain-height.md, §9, Q9) water passes only between cells whose
// ground joins (ElevationMap::joins): a field on a plateau above the river or a canal stays dry, and
// a gate draws its water and lets it out on its own level. Canals and water are at level 0 (Stage
// 4's placement), so water lifted onto high ground waits for a later feature (a water wheel).
//
// A wet field is sown in the dry season too (farms::on_new_month), and the fields a gate's outlet
// reaches take the flood's silt through it (farms::sluiced_fields). Nothing here is saved: it's
// read from the map when it's needed, and the canals no water reaches are flagged dry for the
// renderer (TileFlags::DryCanal, refresh_canals) whenever the network may have changed.

use std::collections::{HashMap, VecDeque};

use common::{
    coords::{Cell, CellRange},
    mem::SingleThreadStatic,
};

use super::{GATE_SIDES, SluiceGate, sluice_gates};
use crate::{
    sim::SimContextMut,
    tile::{TileFlags, TileKind, TileMap, TileMapLayerKind, water},
    world::World,
};

// How far past its dyke a sluice gate looks for water, in cells.
pub const GATE_INTAKE_CELLS: i32 = 3;

// The thickest dyke a gate's intake looks across.
const MAX_DYKE_CELLS: i32 = 4;

// Where a wet canal's or field's water comes from.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WaterSource {
    // Open water beside it, or beside the canals and fields it's joined to.
    River,
    // The sluice gate whose base cell this is, through its outlet.
    Gate(Cell),
}

// A sluice gate's part in the network.
#[derive(Clone)]
pub struct GateSupply {
    pub gate: SluiceGate,
    // The side it faces its dyke on (its tile's variation), as a step along the grid.
    pub side: Option<(i32, i32)>,
    // Water across its dyke.
    pub intake: bool,
    // The cells past its footprint on the side away from its dyke.
    pub outlets: Vec<Cell>,
}

impl GateSupply {
    // Whether it sends water through its outlet now: it works and has its intake.
    #[inline]
    pub fn supplies(&self) -> bool {
        self.gate.working && self.intake
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.gate.cells.start
    }
}

// The wet canals and fields, read from the map.
pub struct Irrigation {
    wet: HashMap<Cell, WaterSource>,
    canals: Vec<Cell>,
    gates: Vec<GateSupply>,
}

impl Irrigation {
    pub fn read(world: &World, tile_map: &TileMap) -> Self {
        let canals = canal_cells(tile_map);
        let mut wet = HashMap::new();

        // From open water: every canal and field beside it, and what joins them.
        let mut seeds: Vec<Cell> = canals.clone();
        seeds.extend(super::field_cells(tile_map));
        seeds.sort_by_key(|cell| (cell.y, cell.x));
        let river: Vec<Cell> = seeds
            .into_iter()
            .filter(|&cell| {
                neighbours(cell).into_iter().any(|next| water::is_open_water(tile_map, next) && joins(tile_map, cell, next))
            })
            .collect();
        spread(tile_map, &river, WaterSource::River, &mut wet);

        // Then from each gate that supplies water, in cell order, into what the river doesn't reach.
        let mut gates: Vec<GateSupply> =
            sluice_gates(world).into_iter().map(|gate| supply_of(tile_map, gate, &wet)).collect();
        gates.sort_by_key(|supply| (supply.base_cell().y, supply.base_cell().x));
        for supply in &gates {
            if supply.supplies() {
                spread(tile_map, &supply.outlets, WaterSource::Gate(supply.base_cell()), &mut wet);
            }
        }

        Self { wet, canals, gates }
    }

    // Where the water at a canal or a field comes from, if it's wet.
    #[inline]
    pub fn source(&self, cell: Cell) -> Option<WaterSource> {
        self.wet.get(&cell).copied()
    }

    #[inline]
    pub fn is_wet(&self, cell: Cell) -> bool {
        self.wet.contains_key(&cell)
    }

    // Every canal cell, in row order.
    #[inline]
    pub fn canals(&self) -> &[Cell] {
        &self.canals
    }

    // The sluice gates, in cell order.
    #[inline]
    pub fn gates(&self) -> &[GateSupply] {
        &self.gates
    }

    // The gate whose footprint holds a cell, if one does.
    pub fn gate_at(&self, cell: Cell) -> Option<&GateSupply> {
        self.gates.iter().find(|supply| supply.gate.cells.contains(cell))
    }

    // How many fields take their water from the gate at `base`.
    pub fn fields_watered_by(&self, tile_map: &TileMap, base: Cell) -> usize {
        self.wet.iter().filter(|&(&cell, &source)| source == WaterSource::Gate(base) && is_field(tile_map, cell)).count()
    }
}

// The fields a gate's outlet reaches through canals and fields, wherever else their water comes
// from: those its silt goes to in a flood (farms::sluiced_fields). In row order.
pub fn fields_reached_by(tile_map: &TileMap, supply: &GateSupply) -> Vec<Cell> {
    let mut reached = HashMap::new();
    spread(tile_map, &supply.outlets, WaterSource::Gate(supply.base_cell()), &mut reached);
    let mut fields: Vec<Cell> = reached.into_keys().filter(|&cell| is_field(tile_map, cell)).collect();
    fields.sort_by_key(|cell| (cell.y, cell.x));
    fields
}

// Every canal cell on the map, in row order.
pub fn canal_cells(tile_map: &TileMap) -> Vec<Cell> {
    let mut cells = Vec::new();
    tile_map.for_each_tile(TileKind::Terrain, |_, tile| {
        if water::is_canal_def(tile.tile_def()) {
            cells.push(tile.base_cell());
        }
    });
    cells.sort_by_key(|cell| (cell.y, cell.x));
    cells
}

// Whether a cell carries water: a canal or a field.
#[inline]
fn carries_water(tile_map: &TileMap, cell: Cell) -> bool {
    water::is_canal(tile_map, cell) || is_field(tile_map, cell)
}

#[inline]
fn is_field(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).is_some_and(|tile| tile.is(TileKind::Field))
}

#[inline]
fn neighbours(cell: Cell) -> [Cell; 4] {
    [
        Cell::new(cell.x + 1, cell.y),
        Cell::new(cell.x - 1, cell.y),
        Cell::new(cell.x, cell.y + 1),
        Cell::new(cell.x, cell.y - 1),
    ]
}

// Whether water passes between two cells side by side: their ground joins (always on a flat map).
#[inline]
fn joins(tile_map: &TileMap, a: Cell, b: Cell) -> bool {
    let elevation = tile_map.elevation();
    elevation.is_flat() || elevation.joins(a, b)
}

// Water from `starts` (those that carry it) into the canals and fields joined to them that aren't
// wet yet, marked with `source`.
fn spread(tile_map: &TileMap, starts: &[Cell], source: WaterSource, wet: &mut HashMap<Cell, WaterSource>) {
    let mut queue = VecDeque::new();
    for &start in starts {
        if !wet.contains_key(&start) && carries_water(tile_map, start) {
            wet.insert(start, source);
            queue.push_back(start);
        }
    }
    while let Some(cell) = queue.pop_front() {
        for next in neighbours(cell) {
            if !wet.contains_key(&next) && carries_water(tile_map, next) && joins(tile_map, cell, next) {
                wet.insert(next, source);
                queue.push_back(next);
            }
        }
    }
}

// The side a gate faces its dyke on, from its tile's variation (farms::face_dyke).
fn side_of(tile_map: &TileMap, gate: &SluiceGate) -> Option<(i32, i32)> {
    let tile = tile_map.try_tile_from_layer(gate.cells.start, TileMapLayerKind::Objects)?;
    let name = tile.variation_name();
    GATE_SIDES.iter().find(|(side, _, _)| *side == name).map(|&(_, dx, dy)| (dx, dy))
}

// The cells past a footprint on one side: those next to it in that direction.
fn cells_past(cells: CellRange, (dx, dy): (i32, i32)) -> Vec<Cell> {
    cells.iter().map(|cell| Cell::new(cell.x + dx, cell.y + dy)).filter(|next| !cells.contains(*next)).collect()
}

// Whether `is_water` holds for a cell within GATE_INTAKE_CELLS of `from` along `side`, past the
// dyke cells there (a gate's cell past its footprint, facing its dyke).
fn water_across(tile_map: &TileMap, from: Cell, (dx, dy): (i32, i32), is_water: impl Fn(Cell) -> bool) -> bool {
    let mut cell = from;
    let mut dyke_cells = 0;
    while dyke_cells < MAX_DYKE_CELLS && crate::sim::floods::is_dyke(tile_map, cell) {
        cell = Cell::new(cell.x + dx, cell.y + dy);
        dyke_cells += 1;
    }
    (0..GATE_INTAKE_CELLS).any(|step| is_water(Cell::new(cell.x + dx * step, cell.y + dy * step)))
}

fn supply_of(tile_map: &TileMap, gate: SluiceGate, wet: &HashMap<Cell, WaterSource>) -> GateSupply {
    let side = side_of(tile_map, &gate);
    let (intake, outlets) = match side {
        Some(side) => {
            // Water is at level 0, so a gate on higher ground draws none.
            let level = tile_map.elevation().level(gate.cells.start);
            let is_water = |cell: Cell| {
                level == 0
                    && (water::is_open_water(tile_map, cell)
                        || (water::is_canal(tile_map, cell) && wet.get(&cell) == Some(&WaterSource::River)))
            };
            let intake = cells_past(gate.cells, side).into_iter().any(|cell| water_across(tile_map, cell, side, is_water));
            // Its outlets on its own level: none below or above a cliff from it.
            let outlets = cells_past(gate.cells, (-side.0, -side.1))
                .into_iter()
                .filter(|&outlet| joins(tile_map, Cell::new(outlet.x + side.0, outlet.y + side.1), outlet))
                .collect();
            (intake, outlets)
        }
        None => (false, Vec::new()),
    };
    GateSupply { gate, side, intake, outlets }
}

// ----------------------------------------------
// Dry canals
// ----------------------------------------------

// When the canals were last flagged: the session, and each gate's state then (its base cell,
// whether it works, and whether open water or a canal lies across its dyke), so a change shows
// without reading the whole network every frame.
struct CanalsFlagged {
    pending: bool,
    session: u64,
    gates: Vec<(Cell, bool, bool)>,
}

static CANALS_FLAGGED: SingleThreadStatic<CanalsFlagged> =
    SingleThreadStatic::new(CanalsFlagged { pending: true, session: u64::MAX, gates: Vec::new() });

// The network may have changed (a canal, a field, water, a dyke or a gate placed or cleared): the
// canals are flagged again at the start of the next frame (refresh_canals_if_changed).
pub fn network_changed() {
    CANALS_FLAGGED.as_mut().pending = true;
}

// Flags the canals no water reaches dry (TileFlags::DryCanal), and the others wet.
pub fn refresh_canals(context: &SimContextMut) {
    let irrigation = Irrigation::read(context.world(), context.tile_map());
    let tile_map = context.tile_map_mut();
    for &cell in irrigation.canals() {
        let dry = !irrigation.is_wet(cell);
        let flagged = tile_map
            .try_tile_from_layer(cell, TileMapLayerKind::Terrain)
            .is_some_and(|tile| tile.has_flags(TileFlags::DryCanal));
        if flagged != dry {
            tile_map.set_tile_flags(cell, TileKind::Terrain, TileFlags::DryCanal, dry);
        }
    }
    let flagged = CANALS_FLAGGED.as_mut();
    flagged.pending = false;
    flagged.session = crate::session::generation();
    flagged.gates = gate_states(context.world(), context.tile_map());
}

// At the start of each frame (sim::Simulation::update, paused or not): flags the canals again if
// the network changed, a gate started or stopped working or its intake changed, or the session did.
pub fn refresh_canals_if_changed(context: &SimContextMut) {
    let flagged = CANALS_FLAGGED.as_ref();
    let changed = flagged.pending
        || flagged.session != crate::session::generation()
        || flagged.gates != gate_states(context.world(), context.tile_map());
    if changed {
        refresh_canals(context);
    }
}

// Each gate's base cell, whether it works and whether open water or a canal lies across its dyke.
fn gate_states(world: &World, tile_map: &TileMap) -> Vec<(Cell, bool, bool)> {
    let mut states: Vec<(Cell, bool, bool)> = sluice_gates(world)
        .into_iter()
        .map(|gate| {
            let candidate = side_of(tile_map, &gate).is_some_and(|side| {
                let is_water = |cell: Cell| water::is_open_water(tile_map, cell) || water::is_canal(tile_map, cell);
                cells_past(gate.cells, side).into_iter().any(|cell| water_across(tile_map, cell, side, is_water))
            });
            (gate.cells.start, gate.working, candidate)
        })
        .collect();
    states.sort_by_key(|(cell, _, _)| (cell.y, cell.x));
    states
}
