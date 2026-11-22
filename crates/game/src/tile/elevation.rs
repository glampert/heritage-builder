// Terrain height (docs/plans/terrain-height.md): each cell's level and shape, saved with the tile
// map (save v15), from which cliffs and ramps' pieces are worked out as they're drawn.
//
// A cell has a level, 0 to MAX_LEVEL, and a shape: flat, or half of a ramp. A ramp climbs one level
// over two cells towards its uphill side: its low half, at level L, rises from L to L + ½ and its
// high half, also at level L, from L + ½ to L + 1. So the ramp joins ground at L below it to ground
// at L + 1 above it; a longer climb is a chain of ramps facing the same way. A ramp is two cells
// long because one level in one cell (32 px, about 39°) is steeper than the camera looks down (30°):
// a ramp climbing towards the viewer would face away from it and vanish (found making its art). At
// two cells a level (about 22°) every ramp shows. Cliffs aren't stored: there's one wherever
// neighbours' ground differs in height.
//
// Directions are the terrain notes' (.claude/rules/terrain-tiles.md): north is +x on the grid (the
// upper-right edge on screen), east -y, south -x, west +y. A cell (x, y) spans [x - 0.5, x + 0.5] ×
// [y - 0.5, y + 0.5] in continuous cell space: its corners are at ±0.5 from its centre.
//
// Only the developer editor changes heights (Q6), through TilePlacement::set_elevation, which keeps
// the minimap in step and records undo. The map is a byte a cell: bits 0-2 the level, bits 3-5 the
// shape (0 flat, 1-4 a ramp up to the north, east, south or west), bit 6 a ramp's high half, bit 7
// reserved; saved as runs.

use common::{
    Size,
    coords::{Cell, CellF32},
};
use serde::{Deserialize, Serialize};

// ----------------------------------------------
// Constants
// ----------------------------------------------

// How far up a level is drawn, in logical px: one tile height (Q1, decided 2026-11-22).
pub const LEVEL_STEP_PX: f32 = 32.0;

// The highest level a cell can have (3 bits).
pub const MAX_LEVEL: u8 = 7;

// The drop between neighbours the smooth brush works down to and the overlay flags above (Q13):
// taller drops are allowed, but they're usually a mistake.
pub const MAX_CLIFF_LEVELS: u8 = 2;

const LEVEL_MASK: u8 = 0b0000_0111;
const SHAPE_SHIFT: u8 = 3;
const SHAPE_MASK: u8 = 0b0011_1000;
const HIGH_HALF: u8 = 0b0100_0000;
const RESERVED_MASK: u8 = 0b1000_0000;

// ----------------------------------------------
// Dir / Shape
// ----------------------------------------------

// A side of a cell, and the way a ramp climbs.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Dir {
    N, // +x
    E, // -y
    S, // -x
    W, // +y
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    // The neighbour on this side: (dx, dy).
    #[inline]
    pub const fn offset(self) -> (i32, i32) {
        match self {
            Dir::N => (1, 0),
            Dir::E => (0, -1),
            Dir::S => (-1, 0),
            Dir::W => (0, 1),
        }
    }

    #[inline]
    pub fn neighbour(self, cell: Cell) -> Cell {
        let (dx, dy) = self.offset();
        Cell::new(cell.x + dx, cell.y + dy)
    }

    #[inline]
    pub const fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::E => Dir::W,
            Dir::S => Dir::N,
            Dir::W => Dir::E,
        }
    }

    // The lowercase letter used in automation and configs: n, e, s, w.
    pub const fn letter(self) -> &'static str {
        match self {
            Dir::N => "n",
            Dir::E => "e",
            Dir::S => "s",
            Dir::W => "w",
        }
    }

    pub fn parse(text: &str) -> Option<Dir> {
        match text {
            "n" | "N" | "north" => Some(Dir::N),
            "e" | "E" | "east" => Some(Dir::E),
            "s" | "S" | "south" => Some(Dir::S),
            "w" | "W" | "west" => Some(Dir::W),
            _ => None,
        }
    }

    const fn code(self) -> u8 {
        match self {
            Dir::N => 1,
            Dir::E => 2,
            Dir::S => 3,
            Dir::W => 4,
        }
    }

    const fn from_code(code: u8) -> Option<Dir> {
        match code {
            1 => Some(Dir::N),
            2 => Some(Dir::E),
            3 => Some(Dir::S),
            4 => Some(Dir::W),
            _ => None,
        }
    }
}

// Which half of a ramp a cell is (see the module's notes).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Half {
    Low,  // Rises from its level to half a level up: the ramp's foot.
    High, // Rises from half a level up to one level up: the ramp's head.
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Shape {
    #[default]
    Flat,
    Ramp(Dir, Half), // Half of a ramp climbing towards this side.
}

const RAMP_NAMES: [[&str; 2]; 4] =
    [["ramp_n_low", "ramp_n_high"], ["ramp_e_low", "ramp_e_high"], ["ramp_s_low", "ramp_s_high"], [
        "ramp_w_low",
        "ramp_w_high",
    ]];

impl Shape {
    // "flat", or "ramp_n_low", "ramp_n_high", ...: automation's and the presets' spelling.
    pub fn name(self) -> &'static str {
        match self {
            Shape::Flat => "flat",
            Shape::Ramp(dir, half) => RAMP_NAMES[usize::from(dir.code() - 1)][usize::from(half == Half::High)],
        }
    }

    pub fn parse(text: &str) -> Option<Shape> {
        if text == "flat" {
            return Some(Shape::Flat);
        }
        let rest = text.strip_prefix("ramp_")?;
        let (dir, half) = rest.split_once('_')?;
        let half = match half {
            "low" => Half::Low,
            "high" => Half::High,
            _ => return None,
        };
        Dir::parse(dir).map(|dir| Shape::Ramp(dir, half))
    }

    #[inline]
    pub fn is_ramp(self) -> bool {
        matches!(self, Shape::Ramp(..))
    }

    // How far above its level the ground starts rising on this cell, in levels: half a level for a
    // ramp's high half.
    #[inline]
    pub fn base_rise(self) -> f32 {
        match self {
            Shape::Ramp(_, Half::High) => 0.5,
            _ => 0.0,
        }
    }

    // How much the ground rises across the cell, in levels: half a level for a ramp's half.
    #[inline]
    pub fn climb(self) -> f32 {
        if self.is_ramp() { 0.5 } else { 0.0 }
    }
}

// A cell's height as the map stores it.
#[inline]
pub const fn encode(level: u8, shape: Shape) -> u8 {
    let level = if level > MAX_LEVEL { MAX_LEVEL } else { level };
    match shape {
        Shape::Flat => level,
        Shape::Ramp(dir, Half::Low) => level | (dir.code() << SHAPE_SHIFT),
        Shape::Ramp(dir, Half::High) => level | (dir.code() << SHAPE_SHIFT) | HIGH_HALF,
    }
}

// A stored byte's level and shape; None for a byte no edit writes (a bad shape code or reserved
// bits set), which load reports.
#[inline]
pub const fn decode(byte: u8) -> Option<(u8, Shape)> {
    if byte & RESERVED_MASK != 0 {
        return None;
    }
    let level = byte & LEVEL_MASK;
    let code = (byte & SHAPE_MASK) >> SHAPE_SHIFT;
    let high = byte & HIGH_HALF != 0;
    if code == 0 {
        // A flat cell has no half.
        return if high { None } else { Some((level, Shape::Flat)) };
    }
    match Dir::from_code(code) {
        Some(dir) => Some((level, Shape::Ramp(dir, if high { Half::High } else { Half::Low }))),
        None => None,
    }
}

// The highest point of a cell's ground, in whole levels (rounded up): its level, one more for a
// ramp's half (which tops out half or one level up).
#[inline]
const fn top_of(byte: u8) -> usize {
    match decode(byte) {
        Some((level, Shape::Flat)) => level as usize,
        Some((level, Shape::Ramp(..))) => level as usize + 1,
        None => (byte & LEVEL_MASK) as usize,
    }
}

// ----------------------------------------------
// ElevationMap
// ----------------------------------------------

// The levels and shapes of a tile map's cells, row by row. Every cell outside it reads flat at 0.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ElevationMap {
    width: i32,
    height: i32,
    #[serde(with = "crate::byte_runs")]
    cells: Vec<u8>,

    // Runtime: how many cells' ground tops out at each height (0 to MAX_LEVEL + 1), for the highest
    // level and the count of raised cells. Rebuilt on load (recount).
    #[serde(skip)]
    tops: [usize; MAX_LEVEL as usize + 2],
}

impl ElevationMap {
    pub fn new(size: Size) -> Self {
        let mut map = Self::default();
        map.reset(size);
        map
    }

    // Every cell flat at level 0.
    pub fn reset(&mut self, size: Size) {
        let count = (size.width.max(0) * size.height.max(0)) as usize;
        self.width = size.width.max(0);
        self.height = size.height.max(0);
        self.cells = vec![0; count];
        self.tops = Default::default();
        self.tops[0] = count;
    }

    // After a load: the runtime counts, from the cells.
    pub fn recount(&mut self) {
        self.tops = Default::default();
        for &byte in &self.cells {
            self.tops[top_of(byte)] += 1;
        }
    }

    #[inline]
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    // Whether the stored cells match the size (a broken save's don't).
    #[inline]
    pub fn is_consistent(&self) -> bool {
        self.cells.len() == (self.width.max(0) * self.height.max(0)) as usize
    }

    // Whether any cell is raised: a flat map draws, picks and paths as before.
    #[inline]
    pub fn is_flat(&self) -> bool {
        self.raised_count() == 0
    }

    // The cells whose ground stands above level 0: raised, or a ramp.
    #[inline]
    pub fn raised_count(&self) -> usize {
        self.cells.len() - self.tops[0]
    }

    // The highest ground on the map, in levels (a ramp's top counts).
    pub fn max_height(&self) -> u8 {
        self.tops.iter().rposition(|&count| count != 0).unwrap_or(0) as u8
    }

    #[inline]
    fn index(&self, cell: Cell) -> Option<usize> {
        if cell.x < 0 || cell.y < 0 || cell.x >= self.width || cell.y >= self.height {
            return None;
        }
        Some((cell.y * self.width + cell.x) as usize)
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        self.index(cell).is_some()
    }

    // The stored byte (0 outside the map).
    #[inline]
    pub fn byte(&self, cell: Cell) -> u8 {
        self.index(cell).map_or(0, |index| self.cells[index])
    }

    // Sets a cell's stored byte. Returns whether it changed. Use TileMap::set_elevation, which keeps
    // the minimap in step.
    pub(super) fn set_byte(&mut self, cell: Cell, byte: u8) -> bool {
        let Some(index) = self.index(cell) else {
            return false;
        };
        let old = self.cells[index];
        if old == byte {
            return false;
        }
        self.tops[top_of(old)] -= 1;
        self.tops[top_of(byte)] += 1;
        self.cells[index] = byte;
        true
    }

    // A cell's level and shape (flat at 0 outside the map, or for a bad byte: problems() says).
    #[inline]
    pub fn get(&self, cell: Cell) -> (u8, Shape) {
        decode(self.byte(cell)).unwrap_or((self.byte(cell) & LEVEL_MASK, Shape::Flat))
    }

    #[inline]
    pub fn level(&self, cell: Cell) -> u8 {
        self.get(cell).0
    }

    #[inline]
    pub fn shape(&self, cell: Cell) -> Shape {
        self.get(cell).1
    }

    // Whether a cell's ground stands above level 0.
    #[inline]
    pub fn is_raised(&self, cell: Cell) -> bool {
        self.byte(cell) != 0
    }

    // ----------------------
    // Heights:
    // ----------------------

    // The ground's height at a corner of `cell` (dx and dy ±0.5), in levels: a ramp half's uphill
    // side stands half a level above its downhill one.
    pub fn corner_height(&self, cell: Cell, dx: f32, dy: f32) -> f32 {
        let (level, shape) = self.get(cell);
        let base = f32::from(level) + shape.base_rise();
        match shape {
            Shape::Flat => base,
            Shape::Ramp(dir, _) => {
                let up = match dir {
                    Dir::N => dx > 0.0,
                    Dir::S => dx < 0.0,
                    Dir::W => dy > 0.0,
                    Dir::E => dy < 0.0,
                };
                if up { base + shape.climb() } else { base }
            }
        }
    }

    // The ground's height under a point in continuous cell space (a walker's place), in levels.
    pub fn surface_height(&self, point: CellF32) -> f32 {
        let cell = Cell::new(point.0.x.round() as i32, point.0.y.round() as i32);
        let (level, shape) = self.get(cell);
        let base = f32::from(level) + shape.base_rise();
        match shape {
            Shape::Flat => base,
            Shape::Ramp(dir, _) => {
                let fx = (point.0.x - (cell.x as f32 - 0.5)).clamp(0.0, 1.0);
                let fy = (point.0.y - (cell.y as f32 - 0.5)).clamp(0.0, 1.0);
                let across = match dir {
                    Dir::N => fx,
                    Dir::S => 1.0 - fx,
                    Dir::W => fy,
                    Dir::E => 1.0 - fy,
                };
                base + across * shape.climb()
            }
        }
    }

    // A cell's ground at its centre, in levels: a ramp's low half's a quarter level up, its high
    // half's three quarters.
    #[inline]
    pub fn centre_height(&self, cell: Cell) -> f32 {
        let (level, shape) = self.get(cell);
        f32::from(level) + shape.base_rise() + shape.climb() * 0.5
    }

    // ----------------------
    // Checks:
    // ----------------------

    // What's wrong with a cell's height, if anything: a byte no edit writes, a ramp that doesn't
    // join its downhill and uphill neighbours, or a drop to a neighbour taller than
    // MAX_CLIFF_LEVELS (allowed, but flagged in the overlay).
    pub fn problem_at(&self, cell: Cell) -> Option<ElevationProblem> {
        let byte = self.byte(cell);
        let Some((level, shape)) = decode(byte) else {
            return Some(ElevationProblem::BadByte(byte));
        };

        if let Shape::Ramp(dir, half) = shape {
            let up = dir.neighbour(cell);
            let down = dir.opposite().neighbour(cell);
            let (uphill_ok, downhill_ok) = match half {
                // The foot: its high half above it; below it ground at its level, or the head of
                // the ramp below in a chain (the map's edge counts as ground at 0).
                Half::Low => (
                    self.is_cell_within_bounds(up) && self.get(up) == (level, Shape::Ramp(dir, Half::High)),
                    if self.is_cell_within_bounds(down) {
                        match self.get(down) {
                            (down_level, Shape::Flat) => down_level == level,
                            (down_level, Shape::Ramp(down_dir, Half::High)) => down_dir == dir && down_level + 1 == level,
                            _ => false,
                        }
                    } else {
                        level == 0
                    },
                ),
                // The head: ground one level up above it, or the foot of the next ramp up in a
                // chain; its low half below it.
                Half::High => (
                    self.is_cell_within_bounds(up)
                        && match self.get(up) {
                            (up_level, Shape::Flat) => up_level == level + 1,
                            (up_level, Shape::Ramp(up_dir, Half::Low)) => up_dir == dir && up_level == level + 1,
                            _ => false,
                        },
                    self.is_cell_within_bounds(down) && self.get(down) == (level, Shape::Ramp(dir, Half::Low)),
                ),
            };
            if !uphill_ok {
                return Some(ElevationProblem::RampUphill { dir });
            }
            if !downhill_ok {
                return Some(ElevationProblem::RampDownhill { dir });
            }
            return None;
        }

        // A flat cell: its drop to each flat neighbour.
        for dir in Dir::ALL {
            let neighbour = dir.neighbour(cell);
            if !self.is_cell_within_bounds(neighbour) {
                continue;
            }
            if let (other, Shape::Flat) = self.get(neighbour)
                && level > other
                && level - other > MAX_CLIFF_LEVELS
            {
                return Some(ElevationProblem::TallCliff { drop: level - other });
            }
        }
        None
    }

    // Every cell with a problem that matters on load (bad bytes and broken ramps; tall cliffs are
    // allowed), row by row.
    pub fn problems(&self) -> Vec<(Cell, ElevationProblem)> {
        let mut problems = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = Cell::new(x, y);
                if self.byte(cell) == 0 {
                    continue;
                }
                if let Some(problem) = self.problem_at(cell)
                    && !matches!(problem, ElevationProblem::TallCliff { .. })
                {
                    problems.push((cell, problem));
                }
            }
        }
        problems
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ElevationProblem {
    BadByte(u8),               // A byte no edit writes: a bad shape code, or reserved bits set.
    RampUphill { dir: Dir },   // A ramp half's uphill neighbour isn't its other half, or the ground above.
    RampDownhill { dir: Dir }, // Its downhill neighbour isn't its other half, or the ground below.
    TallCliff { drop: u8 },    // A drop to a neighbour taller than MAX_CLIFF_LEVELS.
}

impl ElevationProblem {
    // The short name automation reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::BadByte(_) => "bad_byte",
            Self::RampUphill { .. } => "ramp_uphill",
            Self::RampDownhill { .. } => "ramp_downhill",
            Self::TallCliff { .. } => "tall_cliff",
        }
    }
}

impl std::fmt::Display for ElevationProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadByte(byte) => write!(f, "a height byte no edit writes ({byte:#04x})"),
            Self::RampUphill { dir } => {
                write!(
                    f,
                    "a ramp climbing {} whose uphill side doesn't join its other half or the ground above",
                    dir.letter()
                )
            }
            Self::RampDownhill { dir } => {
                write!(
                    f,
                    "a ramp climbing {} whose downhill side doesn't join its other half or the ground below",
                    dir.letter()
                )
            }
            Self::TallCliff { drop } => write!(f, "a cliff {drop} levels tall"),
        }
    }
}

// ----------------------------------------------
// Brushes
// ----------------------------------------------

// The developer editor's height tools (§8): what a stroke does to the cells under the brush.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HeightTool {
    // Up to the stroke's level, one above the ground where it started: lower cells rise to it.
    Raise,
    // Down to the stroke's level, one below the ground where it started: higher cells drop to it.
    Lower,
    // To a level picked in the palette, only on cells one level from it (as Zeus's brushes do), so
    // painting a terrace's edge leaves the ground around it alone.
    Terrace(u8),
    // Brings each cell within MAX_CLIFF_LEVELS of its neighbours.
    Smooth,
    // Makes each cell a ramp climbing this way, at its level.
    // Lays a two-cell ramp climbing this way from the cell (its low half) to the next (its high
    // half), at the cell's level; a wider brush lays them side by side.
    Ramp(Dir),
    // Makes each ramp half flat, at its level.
    Flatten,
}

impl HeightTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Terrace(_) => "Terrace",
            Self::Smooth => "Smooth",
            Self::Ramp(Dir::N) => "Ramp N",
            Self::Ramp(Dir::E) => "Ramp E",
            Self::Ramp(Dir::S) => "Ramp S",
            Self::Ramp(Dir::W) => "Ramp W",
            Self::Flatten => "Flatten",
        }
    }

    // The level a stroke works to, picked where it starts: the cell's level ± 1 for Raise and
    // Lower, the palette's for Terrace, none for the others.
    pub fn stroke_level(self, map: &ElevationMap, start: Cell) -> Option<u8> {
        let level = map.level(start);
        match self {
            Self::Raise => Some((level + 1).min(MAX_LEVEL)),
            Self::Lower => Some(level.saturating_sub(1)),
            Self::Terrace(level) => Some(level.min(MAX_LEVEL)),
            _ => None,
        }
    }
}

// The cells a brush `size` cells wide (1, 3, 5, ...) covers around `centre`, inside the map: a
// square; for a ramp, the lane across the ramp's way through `centre` (each cell a ramp's foot) and
// the cell above each (its head).
pub fn brush_cells(map: &ElevationMap, tool: HeightTool, centre: Cell, size: i32) -> Vec<Cell> {
    let radius = (size.max(1) - 1) / 2;
    let mut cells = Vec::new();
    if let HeightTool::Ramp(dir) = tool {
        for foot in ramp_lane(centre, dir, radius) {
            for cell in [foot, dir.neighbour(foot)] {
                if map.is_cell_within_bounds(cell) {
                    cells.push(cell);
                }
            }
        }
        return cells;
    }
    for y in (centre.y - radius)..=(centre.y + radius) {
        for x in (centre.x - radius)..=(centre.x + radius) {
            let cell = Cell::new(x, y);
            if map.is_cell_within_bounds(cell) {
                cells.push(cell);
            }
        }
    }
    cells
}

// The feet of a ramp brush: the cells across the ramp's way, `radius` either side of `centre`.
fn ramp_lane(centre: Cell, dir: Dir, radius: i32) -> impl Iterator<Item = Cell> {
    let (dx, dy) = dir.offset();
    let (across_x, across_y) = (dy.abs(), dx.abs());
    (-radius..=radius).map(move |k| Cell::new(centre.x + k * across_x, centre.y + k * across_y))
}

// What `tool` does with a brush `size` cells wide around `centre`, as (cell, new byte) for each cell
// it changes, in order. `stroke_level` is HeightTool::stroke_level's, from where the stroke started.
pub fn brush_edits(
    map: &ElevationMap,
    tool: HeightTool,
    centre: Cell,
    size: i32,
    stroke_level: Option<u8>,
) -> Vec<(Cell, u8)> {
    let mut edits: Vec<(Cell, u8)> = Vec::new();
    // Smooth sees the cells it already changed, as Zeus's pass does.
    let byte_now = |edits: &[(Cell, u8)], cell: Cell| -> u8 {
        edits.iter().rev().find(|(edited, _)| *edited == cell).map_or(map.byte(cell), |(_, byte)| *byte)
    };
    let push = |edits: &mut Vec<(Cell, u8)>, cell: Cell, byte: u8| {
        if map.is_cell_within_bounds(cell) && byte != byte_now(edits, cell) {
            edits.push((cell, byte));
        }
    };

    if let HeightTool::Ramp(dir) = tool {
        // A ramp's foot at each lane cell's level, its head on the cell above at the same level.
        for foot in ramp_lane(centre, dir, (size.max(1) - 1) / 2) {
            if !map.is_cell_within_bounds(foot) {
                continue;
            }
            let level = map.level(foot);
            push(&mut edits, foot, encode(level, Shape::Ramp(dir, Half::Low)));
            push(&mut edits, dir.neighbour(foot), encode(level, Shape::Ramp(dir, Half::High)));
        }
        return edits;
    }

    for cell in brush_cells(map, tool, centre, size) {
        let (level, shape) = map.get(cell);
        let new = match tool {
            HeightTool::Raise => stroke_level.filter(|&target| level < target).map(|target| encode(target, Shape::Flat)),
            HeightTool::Lower => stroke_level.filter(|&target| level > target).map(|target| encode(target, Shape::Flat)),
            HeightTool::Terrace(_) => stroke_level
                .filter(|&target| level.abs_diff(target) == 1 || (level == target && shape.is_ramp()))
                .map(|target| encode(target, Shape::Flat)),
            HeightTool::Smooth => {
                let mut lowest = u8::MAX;
                let mut highest = 0u8;
                for dir in Dir::ALL {
                    let neighbour = dir.neighbour(cell);
                    if !map.is_cell_within_bounds(neighbour) {
                        continue;
                    }
                    let other = decode(byte_now(&edits, neighbour)).map_or(0, |(other, _)| other);
                    lowest = lowest.min(other);
                    highest = highest.max(other);
                }
                if lowest == u8::MAX {
                    None
                } else {
                    let smoothed =
                        level.min(lowest.saturating_add(MAX_CLIFF_LEVELS)).max(highest.saturating_sub(MAX_CLIFF_LEVELS));
                    (smoothed != level).then(|| encode(smoothed, shape))
                }
            }
            HeightTool::Flatten => shape.is_ramp().then(|| encode(level, Shape::Flat)),
            HeightTool::Ramp(_) => unreachable!("handled above"),
        };
        if let Some(byte) = new {
            push(&mut edits, cell, byte);
        }
    }
    edits
}

// ----------------------------------------------
// Test terrain
// ----------------------------------------------

// Rolling hills over a map `size` cells wide, for tests and timing (automation's terrain.hills): a
// byte a cell, row by row, from smooth value noise in whole levels, about `coverage` of the cells
// raised, up to `max_level`, with features about `feature_cells` across. The same `seed` gives the
// same hills.
pub fn hills(size: Size, seed: u64, coverage: f32, max_level: u8, feature_cells: f32) -> Vec<u8> {
    let count = (size.width.max(0) * size.height.max(0)) as usize;
    if count == 0 {
        return Vec::new();
    }
    let feature_cells = feature_cells.max(1.0);
    let mut values = Vec::with_capacity(count);
    for y in 0..size.height {
        for x in 0..size.width {
            values.push(value_noise(seed, x as f32 / feature_cells, y as f32 / feature_cells));
        }
    }
    let mut sorted = values.clone();
    sorted.sort_by(f32::total_cmp);
    let cut = (((1.0 - coverage.clamp(0.0, 1.0)) * count as f32) as usize).min(count - 1);
    let threshold = sorted[cut];
    let top = sorted[count - 1].max(threshold + f32::EPSILON);
    let max_level = max_level.clamp(1, MAX_LEVEL);
    values
        .into_iter()
        .map(|value| {
            if value <= threshold {
                return 0;
            }
            let t = (value - threshold) / (top - threshold);
            (1.0 + t * f32::from(max_level)).floor().min(f32::from(max_level)) as u8
        })
        .collect()
}

fn hash01(seed: u64, x: i32, y: i32) -> f32 {
    let h = common::hash::splitmix64(seed ^ (((x as u32 as u64) << 32) | (y as u32 as u64)));
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn value_noise(seed: u64, x: f32, y: f32) -> f32 {
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for octave in 0..3u64 {
        let (fx, fy) = (x * frequency, y * frequency);
        let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
        let (tx, ty) = (smooth(fx - x0 as f32), smooth(fy - y0 as f32));
        let s = seed.wrapping_add(octave.wrapping_mul(0x9E37_79B9));
        let a = hash01(s, x0, y0);
        let b = hash01(s, x0 + 1, y0);
        let c = hash01(s, x0, y0 + 1);
        let d = hash01(s, x0 + 1, y0 + 1);
        total += amplitude * (a + (b - a) * tx + (c - a) * ty + (a - b - c + d) * tx * ty);
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    total
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn map(size: i32) -> ElevationMap {
        ElevationMap::new(Size::new(size, size))
    }

    fn set(map: &mut ElevationMap, cell: (i32, i32), level: u8, shape: Shape) {
        map.set_byte(Cell::new(cell.0, cell.1), encode(level, shape));
    }

    fn ramp(dir: Dir, half: Half) -> Shape {
        Shape::Ramp(dir, half)
    }

    #[test]
    fn test_bytes_round_trip() {
        for level in 0..=MAX_LEVEL {
            let mut shapes = vec![Shape::Flat];
            for dir in Dir::ALL {
                shapes.extend([ramp(dir, Half::Low), ramp(dir, Half::High)]);
            }
            for shape in shapes {
                assert_eq!(decode(encode(level, shape)), Some((level, shape)));
                assert_eq!(Shape::parse(shape.name()), Some(shape), "{}", shape.name());
            }
        }
        assert_eq!(encode(9, Shape::Flat), MAX_LEVEL);
        assert_eq!(decode(0b0010_1000), None); // Shape code 5.
        assert_eq!(decode(0b1000_0000), None); // The reserved bit.
        assert_eq!(decode(0b0100_0001), None); // A flat cell's half.
        assert_eq!(Shape::parse("ramp_n"), None);
    }

    #[test]
    fn test_counts_follow_edits() {
        let mut map = map(4);
        assert!(map.is_flat());
        assert_eq!(map.max_height(), 0);
        set(&mut map, (1, 1), 2, Shape::Flat);
        set(&mut map, (2, 1), 2, ramp(Dir::N, Half::High));
        assert_eq!(map.raised_count(), 2);
        assert_eq!(map.max_height(), 3);
        set(&mut map, (2, 1), 0, Shape::Flat);
        assert_eq!(map.raised_count(), 1);
        assert_eq!(map.max_height(), 2);

        let mut copy = map.clone();
        copy.tops = Default::default();
        copy.recount();
        assert_eq!(copy.tops, map.tops);
    }

    #[test]
    fn test_runs_round_trip() {
        let mut map = map(8);
        set(&mut map, (3, 4), 1, Shape::Flat);
        set(&mut map, (2, 4), 0, ramp(Dir::N, Half::High));
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"cells\":[[0,34],[72,1],[1,1],[0,28]]"), "{json}");
        let mut loaded: ElevationMap = serde_json::from_str(&json).unwrap();
        loaded.recount();
        assert!(loaded.is_consistent());
        assert_eq!(loaded.get(Cell::new(3, 4)), (1, Shape::Flat));
        assert_eq!(loaded.get(Cell::new(2, 4)), (0, ramp(Dir::N, Half::High)));
        assert_eq!(loaded.raised_count(), 2);
    }

    #[test]
    fn test_ramp_problems() {
        // A plateau at level 1 from x = 3, a ramp up to it climbing north: its foot at (1, 1), its
        // head at (2, 1).
        let mut map = map(6);
        for y in 0..6 {
            for x in 3..6 {
                set(&mut map, (x, y), 1, Shape::Flat);
            }
        }
        set(&mut map, (1, 1), 0, ramp(Dir::N, Half::Low));
        set(&mut map, (2, 1), 0, ramp(Dir::N, Half::High));
        assert_eq!(map.problem_at(Cell::new(1, 1)), None);
        assert_eq!(map.problem_at(Cell::new(2, 1)), None);
        assert!(map.problems().is_empty());

        // A head without its foot.
        set(&mut map, (1, 1), 0, Shape::Flat);
        assert_eq!(map.problem_at(Cell::new(2, 1)), Some(ElevationProblem::RampDownhill { dir: Dir::N }));
        // A foot without its head.
        set(&mut map, (1, 1), 0, ramp(Dir::N, Half::Low));
        set(&mut map, (2, 1), 0, Shape::Flat);
        assert_eq!(map.problem_at(Cell::new(1, 1)), Some(ElevationProblem::RampUphill { dir: Dir::N }));

        // Climbing the wrong way: its head's uphill side is the ground at 0.
        set(&mut map, (1, 1), 0, ramp(Dir::S, Half::High));
        set(&mut map, (2, 1), 0, ramp(Dir::S, Half::Low));
        assert_eq!(map.problem_at(Cell::new(1, 1)), Some(ElevationProblem::RampUphill { dir: Dir::S }));
        // And its foot's downhill side is the plateau, a level up.
        assert_eq!(map.problem_at(Cell::new(2, 1)), Some(ElevationProblem::RampDownhill { dir: Dir::S }));
        assert_eq!(map.problems().len(), 2);

        // A chain of two ramps, from 0 to 2: (0, 4) to (3, 4), onto ground at 2.
        let mut chain = ElevationMap::new(Size::new(6, 6));
        set(&mut chain, (0, 4), 0, ramp(Dir::N, Half::Low));
        set(&mut chain, (1, 4), 0, ramp(Dir::N, Half::High));
        set(&mut chain, (2, 4), 1, ramp(Dir::N, Half::Low));
        set(&mut chain, (3, 4), 1, ramp(Dir::N, Half::High));
        set(&mut chain, (4, 4), 2, Shape::Flat);
        set(&mut chain, (5, 4), 2, Shape::Flat);
        for x in 0..4 {
            assert_eq!(chain.problem_at(Cell::new(x, 4)), None, "({x}, 4)");
        }
    }

    #[test]
    fn test_tall_cliffs_are_flagged_not_reported() {
        let mut map = map(4);
        set(&mut map, (1, 1), 3, Shape::Flat);
        assert_eq!(map.problem_at(Cell::new(1, 1)), Some(ElevationProblem::TallCliff { drop: 3 }));
        assert!(map.problems().is_empty());
        set(&mut map, (1, 1), 2, Shape::Flat);
        assert_eq!(map.problem_at(Cell::new(1, 1)), None);
    }

    #[test]
    fn test_heights() {
        let mut map = map(4);
        set(&mut map, (1, 1), 1, ramp(Dir::N, Half::Low));
        set(&mut map, (2, 1), 1, ramp(Dir::N, Half::High));
        let (foot, head) = (Cell::new(1, 1), Cell::new(2, 1));
        // The foot from 1 to 1.5, the head from 1.5 to 2, joining along their shared edge.
        assert_eq!(map.corner_height(foot, -0.5, 0.5), 1.0);
        assert_eq!(map.corner_height(foot, 0.5, -0.5), 1.5);
        assert_eq!(map.corner_height(head, -0.5, 0.5), 1.5);
        assert_eq!(map.corner_height(head, 0.5, -0.5), 2.0);
        assert_eq!(map.centre_height(foot), 1.25);
        assert_eq!(map.centre_height(head), 1.75);
        assert_eq!(map.surface_height(CellF32(common::Vec2::new(1.5, 1.0))), 1.5);
        assert_eq!(map.surface_height(CellF32(common::Vec2::new(2.0, 1.0))), 1.75);
        assert_eq!(map.surface_height(CellF32(common::Vec2::new(5.0, 5.0))), 0.0);
    }

    #[test]
    fn test_brushes() {
        let mut map = map(7);
        let centre = Cell::new(3, 3);
        assert_eq!(brush_cells(&map, HeightTool::Raise, centre, 3).len(), 9);
        assert_eq!(brush_cells(&map, HeightTool::Raise, Cell::new(0, 0), 5).len(), 9);

        // Raise: everything under the brush up to the stroke's level, one above where it started.
        let level = HeightTool::Raise.stroke_level(&map, centre);
        assert_eq!(level, Some(1));
        let edits = brush_edits(&map, HeightTool::Raise, centre, 3, level);
        assert_eq!(edits.len(), 9);
        for (cell, byte) in edits {
            map.set_byte(cell, byte);
        }
        // Painting over it again changes nothing: a stroke doesn't stack.
        assert!(brush_edits(&map, HeightTool::Raise, centre, 3, level).is_empty());

        // Terrace at 2 over a wider brush: only the raised cells, one level below it, rise.
        assert_eq!(brush_edits(&map, HeightTool::Terrace(2), centre, 5, Some(2)).len(), 9);

        // Lower from the plateau: down to 0.
        let level = HeightTool::Lower.stroke_level(&map, centre);
        assert_eq!(level, Some(0));
        assert_eq!(brush_edits(&map, HeightTool::Lower, centre, 1, level), vec![(centre, 0)]);

        // A ramp: its foot on the cell, its head on the next cell up its way, at the foot's level.
        let edits = brush_edits(&map, HeightTool::Ramp(Dir::E), centre, 1, None);
        assert_eq!(edits, vec![
            (centre, encode(1, ramp(Dir::E, Half::Low))),
            (Cell::new(3, 2), encode(1, ramp(Dir::E, Half::High))),
        ]);
        // Three wide: a lane across its way (east climbs -y, so the lane runs along x).
        let cells = brush_cells(&map, HeightTool::Ramp(Dir::E), centre, 3);
        assert_eq!(cells, vec![
            Cell::new(2, 3),
            Cell::new(2, 2),
            Cell::new(3, 3),
            Cell::new(3, 2),
            Cell::new(4, 3),
            Cell::new(4, 2)
        ]);
        assert_eq!(brush_edits(&map, HeightTool::Ramp(Dir::E), centre, 3, None).len(), 6);

        // Flatten makes a ramp's half flat again.
        map.set_byte(centre, encode(1, ramp(Dir::E, Half::Low)));
        assert_eq!(brush_edits(&map, HeightTool::Flatten, centre, 1, None), vec![(centre, encode(1, Shape::Flat))]);
    }

    #[test]
    fn test_hills() {
        let size = Size::new(40, 30);
        let bytes = hills(size, 7, 0.3, 3, 8.0);
        assert_eq!(bytes.len(), 1200);
        let raised = bytes.iter().filter(|&&byte| byte != 0).count();
        assert!((300..=420).contains(&raised), "{raised} raised");
        assert!(bytes.iter().all(|&byte| byte <= 3));
        assert_eq!(hills(size, 7, 0.3, 3, 8.0), bytes);
        assert_ne!(hills(size, 8, 0.3, 3, 8.0), bytes);
    }

    #[test]
    fn test_smooth_brings_cells_within_two_levels() {
        let mut map = map(5);
        map.set_byte(Cell::new(2, 2), encode(5, Shape::Flat));
        let edits = brush_edits(&map, HeightTool::Smooth, Cell::new(2, 2), 1, None);
        assert_eq!(edits, vec![(Cell::new(2, 2), encode(MAX_CLIFF_LEVELS, Shape::Flat))]);

        // A pit is filled up to within two levels of the ground around it.
        let mut pit = ElevationMap::new(Size::new(3, 3));
        for y in 0..3 {
            for x in 0..3 {
                pit.set_byte(Cell::new(x, y), encode(4, Shape::Flat));
            }
        }
        pit.set_byte(Cell::new(1, 1), encode(0, Shape::Flat));
        let edits = brush_edits(&pit, HeightTool::Smooth, Cell::new(1, 1), 1, None);
        assert_eq!(edits, vec![(Cell::new(1, 1), encode(2, Shape::Flat))]);
    }
}
