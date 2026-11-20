// The river's floods and the land's fertility (docs/plans/seasons-floods.md). Saved in `Simulation`
// and read through `SimContext::floods()`.
//
// A game's floods are off unless its mission's `floods` block turns them on, or its custom game's
// "Seasons and floods" switch. While they're on, the state keeps a fertility grid (a byte a cell),
// the dykes' wear, the flood under way and a log of the floods; while they're off, every cell reads
// FloodRules::neutral_fertility and nothing floods.

use std::collections::VecDeque;

use common::{coords::Cell, hash};
use serde::{Deserialize, Serialize};

use crate::{
    campaign::config::MissionFloodsDef,
    config::FloodRules,
    tile::{TileKind, TileMap, TileMapLayerKind, water},
    world::stats::WorldStats,
};

pub const MAX_FERTILITY: u8 = 100;

// ----------------------------------------------
// FloodProfile
// ----------------------------------------------

// The game's flood: the mission's block over FloodRules, its sources resolved from the map.
#[derive(Clone, Serialize, Deserialize)]
pub struct FloodProfile {
    // The flood's month, 1 to 12.
    pub month: u32,
    // The least and most cells it reaches from its water, drawn each year.
    pub reach: [u32; 2],
    // How many months the water stays.
    pub months: u32,
    // A cell of each water body that floods.
    pub sources: Vec<Cell>,
    // A Realm text key naming the river; empty: "the river".
    pub river: String,
    // The land's starting fertility within the usual reach and beyond it.
    pub fertility: [u8; 2],
}

impl FloodProfile {
    fn from_rules(rules: &FloodRules) -> Self {
        Self {
            month: rules.month,
            reach: [rules.reach_min, rules.reach_max.max(rules.reach_min)],
            months: rules.months.max(1),
            sources: Vec::new(),
            river: String::new(),
            fertility: [to_fertility(rules.fertility_in_reach), to_fertility(rules.fertility_beyond)],
        }
    }

    fn with_mission(mut self, mission: &MissionFloodsDef) -> Self {
        if let Some(month) = mission.month {
            self.month = month;
        }
        if let Some([min, max]) = mission.reach {
            self.reach = [min.max(1), max.max(min.max(1))];
        }
        if let Some(months) = mission.months {
            self.months = months.max(1);
        }
        if let Some(river) = &mission.river {
            self.river = river.clone();
        }
        if let Some([within, beyond]) = mission.fertility {
            self.fertility = [to_fertility(within), to_fertility(beyond)];
        }
        self
    }
}

#[inline]
fn to_fertility(value: u32) -> u8 {
    value.min(MAX_FERTILITY as u32) as u8
}

// ----------------------------------------------
// FloodUnderWay, FloodRecord, DykeWear
// ----------------------------------------------

// The water on the land: the cells it covers (in row order), the months it stays and how severe
// it was (docs/plans/seasons-floods.md, §3).
#[derive(Clone, Serialize, Deserialize)]
pub struct FloodUnderWay {
    pub cells: Vec<Cell>,
    pub months_left: u32,
    pub severity: f32,
}

impl FloodUnderWay {
    #[inline]
    pub fn covers(&self, cell: Cell) -> bool {
        self.cells.binary_search_by_key(&row_order(cell), |cell| row_order(*cell)).is_ok()
    }
}

#[inline]
fn row_order(cell: Cell) -> (i32, i32) {
    (cell.y, cell.x)
}

// A flood that came: for the goal, the messages and M9's "Floods answered".
#[derive(Clone, Serialize, Deserialize)]
pub struct FloodRecord {
    // The calendar's ordinal month it came in (year x 12 + month - 1).
    pub ordinal_month: u32,
    pub reach: u32,
    pub severity: f32,
    pub cells: u32,
    pub buildings_damaged: u32,
    pub fields_flooded: u32,
    pub breaches: u32,
}

// A dyke cell's wear, 0 to 100 (docs/plans/seasons-floods.md, §4): it rises each month and a
// builder walking the cell resets it; `walked` is the ordinal month one last did.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct DykeWear {
    pub cell: Cell,
    pub wear: f32,
    pub walked: Option<u32>,
}

pub const MAX_WEAR: f32 = 100.0;

// ----------------------------------------------
// FertilityGrid
// ----------------------------------------------

// A byte a cell, row by row; saved as runs ([value, count], ...), since most of the land shares a
// few values.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct FertilityGrid {
    width: i32,
    height: i32,
    #[serde(with = "runs")]
    cells: Vec<u8>,
}

impl FertilityGrid {
    fn new(width: i32, height: i32, value: u8) -> Self {
        let count = (width.max(0) * height.max(0)) as usize;
        Self { width, height, cells: vec![value; count] }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    #[inline]
    fn index(&self, cell: Cell) -> Option<usize> {
        if cell.x < 0 || cell.y < 0 || cell.x >= self.width || cell.y >= self.height {
            return None;
        }
        Some((cell.y * self.width + cell.x) as usize)
    }

    #[inline]
    pub fn get(&self, cell: Cell) -> Option<u8> {
        self.index(cell).map(|index| self.cells[index])
    }

    #[inline]
    fn set(&mut self, cell: Cell, value: u8) {
        if let Some(index) = self.index(cell) {
            self.cells[index] = value.min(MAX_FERTILITY);
        }
    }

    // The mean over the grid's cells, or None while it's empty.
    pub fn mean(&self) -> Option<f32> {
        if self.cells.is_empty() {
            return None;
        }
        Some(self.cells.iter().map(|&value| value as f32).sum::<f32>() / self.cells.len() as f32)
    }
}

mod runs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(cells: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        let mut runs: Vec<[u32; 2]> = Vec::new();
        for &value in cells {
            match runs.last_mut() {
                Some([last, count]) if *last == value as u32 => *count += 1,
                _ => runs.push([value as u32, 1]),
            }
        }
        runs.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let runs: Vec<[u32; 2]> = Vec::deserialize(deserializer)?;
        let mut cells = Vec::with_capacity(runs.iter().map(|[_, count]| *count as usize).sum());
        for [value, count] in runs {
            cells.extend(std::iter::repeat_n(value.min(u8::MAX as u32) as u8, count as usize));
        }
        Ok(cells)
    }
}

// ----------------------------------------------
// CityFloods
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct CityFloods {
    // Whether the river floods in this game (a mission's `floods` block, a custom game's switch).
    enabled: bool,
    profile: FloodProfile,
    // Kept while floods are on; empty otherwise (every cell reads `neutral_fertility`).
    fertility: FertilityGrid,
    neutral_fertility: u8,
    dyke_wear: Vec<DykeWear>,
    flood: Option<FloodUnderWay>,
    // Set by faith's gentle and severe floods for the next one; 1 for an ordinary flood.
    next_severity: f32,
    log: Vec<FloodRecord>,
}

impl CityFloods {
    pub fn new(rules: &FloodRules) -> Self {
        Self {
            enabled: false,
            profile: FloodProfile::from_rules(rules),
            fertility: FertilityGrid::default(),
            neutral_fertility: to_fertility(rules.neutral_fertility),
            dyke_wear: Vec::new(),
            flood: None,
            next_severity: 1.0,
            log: Vec::new(),
        }
    }

    // A game's start: a mission's block (enabled unless it says otherwise once it's given; a mission
    // without one has none), or a custom game's switch with the defaults. Resolves the water that
    // floods from the map and lays out the land's starting fertility.
    pub fn start(&mut self, enabled: bool, mission: &MissionFloodsDef, rules: &FloodRules, tile_map: &TileMap) {
        *self = Self::new(rules);
        self.enabled = enabled;
        self.profile = FloodProfile::from_rules(rules).with_mission(mission);
        if !enabled {
            return;
        }

        self.profile.sources = match &mission.sources {
            Some(cells) => cells.iter().map(|&[x, y]| Cell::new(x, y)).filter(|&cell| is_water(tile_map, cell)).collect(),
            None => water_bodies(tile_map, rules.min_source_cells as usize),
        };

        let size = tile_map.size_in_cells();
        let [within, beyond] = self.profile.fertility;
        let mut grid = FertilityGrid::new(size.width, size.height, beyond);
        for cell in reach_fill(tile_map, &self.profile.sources, self.profile.reach[1], |_| false) {
            grid.set(cell, within);
        }
        self.fertility = grid;
    }

    // Turns floods on or off for the game (automation, the World Debug tab), as a start would.
    pub fn set_enabled(&mut self, enabled: bool, rules: &FloodRules, tile_map: &TileMap) {
        if enabled == self.enabled {
            return;
        }
        let mission = MissionFloodsDef {
            month: Some(self.profile.month),
            reach: Some(self.profile.reach),
            months: Some(self.profile.months),
            river: (!self.profile.river.is_empty()).then(|| self.profile.river.clone()),
            fertility: Some([self.profile.fertility[0] as u32, self.profile.fertility[1] as u32]),
            ..Default::default()
        };
        let log = std::mem::take(&mut self.log);
        self.start(enabled, &mission, rules, tile_map);
        self.log = log;
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn profile(&self) -> &FloodProfile {
        &self.profile
    }

    #[inline]
    pub fn neutral_fertility(&self) -> u8 {
        self.neutral_fertility
    }

    // The land's fertility at a cell, 0 to 100: the grid's while floods are on, else the neutral.
    #[inline]
    pub fn fertility_at(&self, cell: Cell) -> u8 {
        self.fertility.get(cell).unwrap_or(self.neutral_fertility)
    }

    // Sets a cell's fertility; false where it isn't kept (floods off, or off the map).
    pub fn set_fertility(&mut self, cell: Cell, value: u8) -> bool {
        if self.fertility.get(cell).is_none() {
            return false;
        }
        self.fertility.set(cell, value);
        true
    }

    // What a harvest on that cell is multiplied by: 0.5 + fertility / 100 (1 at the neutral 50).
    #[inline]
    pub fn yield_factor_at(&self, cell: Cell) -> f32 {
        0.5 + self.fertility_at(cell) as f32 / 100.0
    }

    #[inline]
    pub fn fertility(&self) -> &FertilityGrid {
        &self.fertility
    }

    #[inline]
    pub fn dyke_wear(&self) -> &[DykeWear] {
        &self.dyke_wear
    }

    #[inline]
    pub fn flood(&self) -> Option<&FloodUnderWay> {
        self.flood.as_ref()
    }

    #[inline]
    pub fn next_severity(&self) -> f32 {
        self.next_severity
    }

    #[inline]
    pub fn set_next_severity(&mut self, severity: f32) {
        self.next_severity = severity.max(0.0);
    }

    #[inline]
    pub fn log(&self) -> &[FloodRecord] {
        &self.log
    }

    // Floods since the game's start that breached no dyke (MissionGoal::FloodsSurvived).
    pub fn floods_survived(&self) -> u32 {
        self.log.iter().filter(|record| record.breaches == 0).count() as u32
    }

    // The world stats' flood figures, recounted with them (World::update, refresh_stats).
    #[inline]
    pub fn tally(&self, stats: &mut WorldStats) {
        stats.floods.floods = self.log.len() as u32;
        stats.floods.survived = self.floods_survived();
    }

    // ----------------------
    // Dykes:
    // ----------------------

    #[inline]
    fn dyke_index(&self, cell: Cell) -> Result<usize, usize> {
        self.dyke_wear.binary_search_by_key(&row_order(cell), |dyke| row_order(dyke.cell))
    }

    // A dyke cell's wear record, if the cell is a dyke the state knows.
    pub fn dyke(&self, cell: Cell) -> Option<&DykeWear> {
        self.dyke_index(cell).ok().map(|index| &self.dyke_wear[index])
    }

    // A dyke cell's wear, 0 to 100 (0 for one not yet counted).
    #[inline]
    pub fn wear_at(&self, cell: Cell) -> f32 {
        self.dyke(cell).map_or(0.0, |dyke| dyke.wear)
    }

    // Keeps a record for every dyke on the map, in row order: new dykes start sound, and the
    // records of dykes gone (cleared, breached) are dropped. Only while floods are on.
    pub fn sync_dykes(&mut self, tile_map: &TileMap) {
        if !self.enabled {
            self.dyke_wear.clear();
            return;
        }
        let mut cells = dyke_cells(tile_map);
        cells.sort_by_key(|cell| row_order(*cell));
        let old = std::mem::take(&mut self.dyke_wear);
        self.dyke_wear = cells
            .into_iter()
            .map(|cell| match old.binary_search_by_key(&row_order(cell), |dyke| row_order(dyke.cell)) {
                Ok(index) => old[index],
                Err(_) => DykeWear { cell, wear: 0.0, walked: None },
            })
            .collect();
    }

    // A month's wear on every dyke: `per_month`, scaled per cell by a factor within `jitter`
    // of 1 hashed from the cell (the same on every run, never drawn from the RNG).
    pub fn wear_dykes(&mut self, per_month: f32, jitter: f32) {
        for dyke in &mut self.dyke_wear {
            dyke.wear = (dyke.wear + per_month * wear_jitter(dyke.cell, jitter)).clamp(0.0, MAX_WEAR);
        }
    }

    // Sets a dyke cell's wear (automation, the debug tools); false where there's no dyke record.
    pub fn set_wear(&mut self, cell: Cell, wear: f32) -> bool {
        match self.dyke_index(cell) {
            Ok(index) => {
                self.dyke_wear[index].wear = wear.clamp(0.0, MAX_WEAR);
                true
            }
            Err(_) => false,
        }
    }

    // A builder walked a dyke cell in ordinal month `now`: its wear goes back to 0. The cell is
    // counted as a dyke if it wasn't yet (the caller checked the map).
    pub fn builder_walked(&mut self, cell: Cell, now: u32) {
        if !self.enabled {
            return;
        }
        match self.dyke_index(cell) {
            Ok(index) => {
                self.dyke_wear[index].wear = 0.0;
                self.dyke_wear[index].walked = Some(now);
            }
            Err(index) => self.dyke_wear.insert(index, DykeWear { cell, wear: 0.0, walked: Some(now) }),
        }
    }

    // A dyke cell breached: its record goes.
    pub fn remove_dyke(&mut self, cell: Cell) {
        if let Ok(index) = self.dyke_index(cell) {
            self.dyke_wear.remove(index);
        }
    }

    // How many dyke cells are worn to `worn` or more.
    pub fn worn_dykes(&self, worn: f32) -> usize {
        self.dyke_wear.iter().filter(|dyke| dyke.wear >= worn).count()
    }

    // Whether a cell is under water now.
    #[inline]
    pub fn is_flooded(&self, cell: Cell) -> bool {
        self.flood.as_ref().is_some_and(|flood| flood.covers(cell))
    }

    // The next flood's severity, used up: the next one after it is ordinary again.
    pub fn take_next_severity(&mut self) -> f32 {
        std::mem::replace(&mut self.next_severity, 1.0)
    }

    // The water comes over `cells` for the profile's months; the silt it leaves raises their
    // fertility (where it's kept), and the flood goes in the log.
    pub fn begin_flood(&mut self, mut cells: Vec<Cell>, severity: f32, silt: u32, record: FloodRecord) {
        cells.sort_by_key(|cell| row_order(*cell));
        cells.dedup();
        self.add_silt(&cells, silt);
        self.flood = Some(FloodUnderWay { cells, months_left: self.profile.months.max(1), severity });
        self.log.push(record);
    }

    // The flood's silt on cells: fertility + `silt`, at most 100, where it's kept. The flood's own
    // cells take it as it comes, and the fields a sluice gate waters behind a dyke (crate::floods).
    pub fn add_silt(&mut self, cells: &[Cell], silt: u32) {
        let silt = silt.min(MAX_FERTILITY as u32) as u8;
        for &cell in cells {
            if let Some(fertility) = self.fertility.get(cell) {
                self.fertility.set(cell, fertility.saturating_add(silt));
            }
        }
    }

    // A month passes under water: true once the water's months are over (it goes now).
    pub fn month_passed(&mut self) -> bool {
        let Some(flood) = &mut self.flood else {
            return false;
        };
        flood.months_left = flood.months_left.saturating_sub(1);
        flood.months_left == 0
    }

    // The water goes: the cells it covered.
    pub fn end_flood(&mut self) -> Option<FloodUnderWay> {
        self.flood.take()
    }

    // The land an ordinary flood would reach now: the fill at the profile's largest reach, stopped
    // by the dykes as they stand, with each cell's distance from the water (1 on the shore). Empty
    // while floods are off.
    pub fn ordinary_reach(&self, tile_map: &TileMap) -> Vec<(Cell, u32)> {
        if !self.enabled {
            return Vec::new();
        }
        reach_distances(tile_map, &self.profile.sources, self.profile.reach[1], |cell| is_dyke(tile_map, cell))
    }
}

// ----------------------------------------------
// The water and its reach
// ----------------------------------------------

#[inline]
fn is_water(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.path_kind().is_water())
}

#[inline]
fn is_land(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| !tile.path_kind().is_water())
}

// What the flood passes over: land, and a canal not joined to the water it comes from (a gate's
// canals behind a dyke are no moat; crate::farms::irrigation).
#[inline]
fn is_passable(tile_map: &TileMap, cell: Cell) -> bool {
    is_land(tile_map, cell) || water::is_canal(tile_map, cell)
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

// A cell of each water body of at least `min_cells` cells of open water (four neighbours apart), in
// row order. Canals joined to a body are part of it, but don't count towards its size, and a body
// of canals alone is none (crate::farms::irrigation): a canal never makes water flood.
pub fn water_bodies(tile_map: &TileMap, min_cells: usize) -> Vec<Cell> {
    let size = tile_map.size_in_cells();
    let (width, height) = (size.width.max(0), size.height.max(0));
    let mut seen = vec![false; (width * height) as usize];
    let index = |cell: Cell| (cell.y * width + cell.x) as usize;
    let mut seeds = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let start = Cell::new(x, y);
            if seen[index(start)] || !water::is_open_water(tile_map, start) {
                continue;
            }
            seen[index(start)] = true;
            let mut queue = VecDeque::from([start]);
            let mut count = 0;
            while let Some(cell) = queue.pop_front() {
                if !water::is_canal(tile_map, cell) {
                    count += 1;
                }
                for next in neighbours(cell) {
                    if next.x < 0 || next.y < 0 || next.x >= width || next.y >= height {
                        continue;
                    }
                    if !seen[index(next)] && is_water(tile_map, next) {
                        seen[index(next)] = true;
                        queue.push_back(next);
                    }
                }
            }
            if count >= min_cells {
                seeds.push(start);
            }
        }
    }
    seeds
}

// The land a flood reaches: from the shores of the water bodies the seeds are in, out to `reach`
// cells over land, four neighbours at a time, never into a cell `blocks` says stops the water
// (a dyke). In the order the water reaches them.
pub fn reach_fill(tile_map: &TileMap, seeds: &[Cell], reach: u32, blocks: impl Fn(Cell) -> bool) -> Vec<Cell> {
    reach_distances(tile_map, seeds, reach, blocks).into_iter().map(|(cell, _)| cell).collect()
}

// reach_fill, with each cell's distance from the water along the grid (1 on the shore).
pub fn reach_distances(tile_map: &TileMap, seeds: &[Cell], reach: u32, blocks: impl Fn(Cell) -> bool) -> Vec<(Cell, u32)> {
    reach_meeting(tile_map, seeds, reach, blocks).0
}

// reach_distances, and the cells that block it the fill met within its reach (each once, in the
// order it met them): the dykes the water stands against.
pub fn reach_meeting(
    tile_map: &TileMap,
    seeds: &[Cell],
    reach: u32,
    blocks: impl Fn(Cell) -> bool,
) -> (Vec<(Cell, u32)>, Vec<Cell>) {
    let size = tile_map.size_in_cells();
    let (width, height) = (size.width.max(0), size.height.max(0));
    let in_bounds = |cell: Cell| cell.x >= 0 && cell.y >= 0 && cell.x < width && cell.y < height;
    let index = |cell: Cell| (cell.y * width + cell.x) as usize;

    // The water bodies.
    let mut water = vec![false; (width * height) as usize];
    let mut queue: VecDeque<Cell> = VecDeque::new();
    for &seed in seeds {
        if in_bounds(seed) && !water[index(seed)] && is_water(tile_map, seed) {
            water[index(seed)] = true;
            queue.push_back(seed);
        }
    }
    let mut shore: VecDeque<(Cell, u32)> = VecDeque::new();
    let mut reached = vec![false; (width * height) as usize];
    let mut met = Vec::new();
    while let Some(cell) = queue.pop_front() {
        for next in neighbours(cell) {
            if !in_bounds(next) || water[index(next)] || reached[index(next)] {
                continue;
            }
            if is_water(tile_map, next) {
                water[index(next)] = true;
                queue.push_back(next);
            } else if reach > 0 && is_passable(tile_map, next) {
                reached[index(next)] = true;
                if blocks(next) {
                    met.push(next);
                } else {
                    shore.push_back((next, 1));
                }
            }
        }
    }

    // Out over the land.
    let mut cells = Vec::new();
    while let Some((cell, distance)) = shore.pop_front() {
        cells.push((cell, distance));
        if distance >= reach {
            continue;
        }
        for next in neighbours(cell) {
            if !in_bounds(next) || reached[index(next)] || water[index(next)] {
                continue;
            }
            if is_passable(tile_map, next) {
                reached[index(next)] = true;
                if blocks(next) {
                    met.push(next);
                } else {
                    shore.push_back((next, distance + 1));
                }
            }
        }
    }
    (cells, met)
}

// Whether a cell's terrain stops the flood (a dyke: TileDef::blocks_floods).
#[inline]
pub fn is_dyke(tile_map: &TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.tile_def().blocks_floods)
}

// Every dyke cell on the map.
pub fn dyke_cells(tile_map: &TileMap) -> Vec<Cell> {
    let mut cells = Vec::new();
    tile_map.for_each_tile(TileKind::Terrain, |_, tile| {
        if tile.tile_def().blocks_floods {
            cells.push(tile.base_cell());
        }
    });
    cells
}

// A factor within `jitter` of 1 for a dyke cell's wear, hashed from the cell.
fn wear_jitter(cell: Cell, jitter: f32) -> f32 {
    let jitter = jitter.clamp(0.0, 0.9);
    if jitter == 0.0 {
        return 1.0;
    }
    let key = ((cell.x as u32 as u64) | ((cell.y as u32 as u64) << 32)) ^ 0xD7CE_5EED_0000_0000;
    1.0 - jitter + 2.0 * jitter * hash::hash_to_unit_f32(hash::splitmix64(key))
}
