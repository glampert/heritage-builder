// Terrain height (docs/plans/terrain-height.md): each cell's level and shape, saved with the tile
// map (save v15), from which cliffs and ramps' pieces are worked out as they're drawn.
//
// A cell has a level, 0 to MAX_LEVEL, and a shape: flat, or a ramp that climbs one level towards
// its uphill side. A ramp at level L joins its downhill neighbour, at L, to its uphill neighbour, at
// L + 1; a longer climb is a chain of ramps facing the same way, each one level up. Cliffs aren't
// stored: there's one wherever neighbours' ground differs in height.
//
// Directions are the terrain notes' (.claude/rules/terrain-tiles.md): north is +x on the grid (the
// upper-right edge on screen), east -y, south -x, west +y. A cell (x, y) spans [x - 0.5, x + 0.5] ×
// [y - 0.5, y + 0.5] in continuous cell space: its corners are at ±0.5 from its centre.
//
// Only the developer editor changes heights (Q6), through TilePlacement::set_elevation, which keeps
// the minimap in step and records undo. The map is a byte a cell: bits 0-2 the level, bits 3-5 the
// shape (0 flat, 1-4 a ramp up to the north, east, south or west), bits 6-7 reserved; saved as runs.

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
const RESERVED_MASK: u8 = 0b1100_0000;

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

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Shape {
    #[default]
    Flat,
    Ramp(Dir), // Climbs one level towards this side.
}

impl Shape {
    // "flat", or "ramp_n", "ramp_e", "ramp_s", "ramp_w": automation's and the presets' spelling.
    pub fn name(self) -> &'static str {
        match self {
            Shape::Flat => "flat",
            Shape::Ramp(Dir::N) => "ramp_n",
            Shape::Ramp(Dir::E) => "ramp_e",
            Shape::Ramp(Dir::S) => "ramp_s",
            Shape::Ramp(Dir::W) => "ramp_w",
        }
    }

    pub fn parse(text: &str) -> Option<Shape> {
        if text == "flat" {
            return Some(Shape::Flat);
        }
        text.strip_prefix("ramp_").and_then(Dir::parse).map(Shape::Ramp)
    }

    #[inline]
    pub fn is_ramp(self) -> bool {
        matches!(self, Shape::Ramp(_))
    }
}

// A cell's height as the map stores it.
#[inline]
pub const fn encode(level: u8, shape: Shape) -> u8 {
    let level = if level > MAX_LEVEL { MAX_LEVEL } else { level };
    let code = match shape {
        Shape::Flat => 0,
        Shape::Ramp(dir) => dir.code(),
    };
    level | (code << SHAPE_SHIFT)
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
    if code == 0 {
        return Some((level, Shape::Flat));
    }
    match Dir::from_code(code) {
        Some(dir) => Some((level, Shape::Ramp(dir))),
        None => None,
    }
}

// The highest point of a cell's ground, in levels: its level, one more for a ramp.
#[inline]
const fn top_of(byte: u8) -> usize {
    match decode(byte) {
        Some((level, Shape::Flat)) => level as usize,
        Some((level, Shape::Ramp(_))) => level as usize + 1,
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

    // The ground's height at a corner of `cell` (dx and dy ±0.5), in levels: a ramp's uphill side
    // stands one level above its downhill one.
    pub fn corner_height(&self, cell: Cell, dx: f32, dy: f32) -> f32 {
        let (level, shape) = self.get(cell);
        let level = f32::from(level);
        match shape {
            Shape::Flat => level,
            Shape::Ramp(dir) => {
                let up = match dir {
                    Dir::N => dx > 0.0,
                    Dir::S => dx < 0.0,
                    Dir::W => dy > 0.0,
                    Dir::E => dy < 0.0,
                };
                if up { level + 1.0 } else { level }
            }
        }
    }

    // The ground's height under a point in continuous cell space (a walker's place), in levels.
    pub fn surface_height(&self, point: CellF32) -> f32 {
        let cell = Cell::new(point.0.x.round() as i32, point.0.y.round() as i32);
        let (level, shape) = self.get(cell);
        let level = f32::from(level);
        match shape {
            Shape::Flat => level,
            Shape::Ramp(dir) => {
                let fx = (point.0.x - (cell.x as f32 - 0.5)).clamp(0.0, 1.0);
                let fy = (point.0.y - (cell.y as f32 - 0.5)).clamp(0.0, 1.0);
                level
                    + match dir {
                        Dir::N => fx,
                        Dir::S => 1.0 - fx,
                        Dir::W => fy,
                        Dir::E => 1.0 - fy,
                    }
            }
        }
    }

    // A cell's ground at its centre, in levels: a ramp's is half a level up.
    #[inline]
    pub fn centre_height(&self, cell: Cell) -> f32 {
        let (level, shape) = self.get(cell);
        f32::from(level) + if shape.is_ramp() { 0.5 } else { 0.0 }
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

        if let Shape::Ramp(dir) = shape {
            // Uphill: ground at L + 1 along the whole edge (flat at L + 1, or the next ramp up).
            let up = dir.neighbour(cell);
            let uphill_ok = self.is_cell_within_bounds(up)
                && match self.get(up) {
                    (up_level, Shape::Flat) => up_level == level + 1,
                    (up_level, Shape::Ramp(up_dir)) => up_dir == dir && up_level == level + 1,
                };
            if !uphill_ok {
                return Some(ElevationProblem::RampUphill { dir });
            }
            // Downhill: ground at L (flat at L, or the ramp below); the map's edge is ground at 0.
            let down = dir.opposite().neighbour(cell);
            let downhill_ok = if self.is_cell_within_bounds(down) {
                match self.get(down) {
                    (down_level, Shape::Flat) => down_level == level,
                    (down_level, Shape::Ramp(down_dir)) => down_dir == dir && down_level + 1 == level,
                }
            } else {
                level == 0
            };
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
    RampUphill { dir: Dir },   // The ramp's uphill neighbour isn't one level up.
    RampDownhill { dir: Dir }, // Its downhill neighbour isn't at its level.
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
                write!(f, "a ramp climbing {} whose uphill neighbour isn't one level up", dir.letter())
            }
            Self::RampDownhill { dir } => {
                write!(f, "a ramp climbing {} whose downhill neighbour isn't at its level", dir.letter())
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
    Ramp(Dir),
    // Makes each ramp flat, at its level.
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

// The cells a square brush `size` cells wide (1, 3, 5, ...) covers around `centre`, inside the map.
pub fn brush_cells(map: &ElevationMap, centre: Cell, size: i32) -> Vec<Cell> {
    let radius = (size.max(1) - 1) / 2;
    let mut cells = Vec::new();
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

// What `tool` does to `cells`, as (cell, new byte) for each cell it changes, in order.
// `stroke_level` is HeightTool::stroke_level's, from where the stroke started.
pub fn brush_edits(map: &ElevationMap, tool: HeightTool, cells: &[Cell], stroke_level: Option<u8>) -> Vec<(Cell, u8)> {
    let mut edits: Vec<(Cell, u8)> = Vec::new();
    // Smooth sees the cells it already changed, as Zeus's pass does.
    let byte_now = |edits: &[(Cell, u8)], cell: Cell| -> u8 {
        edits.iter().rev().find(|(edited, _)| *edited == cell).map_or(map.byte(cell), |(_, byte)| *byte)
    };

    for &cell in cells {
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
            HeightTool::Ramp(dir) => Some(encode(level, Shape::Ramp(dir))),
            HeightTool::Flatten => shape.is_ramp().then(|| encode(level, Shape::Flat)),
        };
        if let Some(byte) = new
            && byte != byte_now(&edits, cell)
        {
            edits.push((cell, byte));
        }
    }
    edits
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

    #[test]
    fn test_bytes_round_trip() {
        for level in 0..=MAX_LEVEL {
            for shape in [Shape::Flat, Shape::Ramp(Dir::N), Shape::Ramp(Dir::E), Shape::Ramp(Dir::S), Shape::Ramp(Dir::W)] {
                assert_eq!(decode(encode(level, shape)), Some((level, shape)));
                assert_eq!(Shape::parse(shape.name()), Some(shape));
            }
        }
        assert_eq!(encode(9, Shape::Flat), MAX_LEVEL);
        assert_eq!(decode(0b0010_1000), None); // Shape code 5.
        assert_eq!(decode(0b0100_0000), None); // A reserved bit.
    }

    #[test]
    fn test_counts_follow_edits() {
        let mut map = map(4);
        assert!(map.is_flat());
        assert_eq!(map.max_height(), 0);
        set(&mut map, (1, 1), 2, Shape::Flat);
        set(&mut map, (2, 1), 2, Shape::Ramp(Dir::N));
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
        set(&mut map, (2, 4), 0, Shape::Ramp(Dir::N));
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"cells\":[[0,34],[8,1],[1,1],[0,28]]"), "{json}");
        let mut loaded: ElevationMap = serde_json::from_str(&json).unwrap();
        loaded.recount();
        assert!(loaded.is_consistent());
        assert_eq!(loaded.get(Cell::new(3, 4)), (1, Shape::Flat));
        assert_eq!(loaded.get(Cell::new(2, 4)), (0, Shape::Ramp(Dir::N)));
        assert_eq!(loaded.raised_count(), 2);
    }

    #[test]
    fn test_ramp_problems() {
        // A plateau at level 1 from x = 3, a ramp at (2, 1) climbing north onto it.
        let mut map = map(6);
        for y in 0..6 {
            for x in 3..6 {
                set(&mut map, (x, y), 1, Shape::Flat);
            }
        }
        set(&mut map, (2, 1), 0, Shape::Ramp(Dir::N));
        assert_eq!(map.problem_at(Cell::new(2, 1)), None);
        assert!(map.problems().is_empty());

        // Climbing the wrong way: its uphill side is the ground at 0.
        set(&mut map, (2, 1), 0, Shape::Ramp(Dir::S));
        assert_eq!(map.problem_at(Cell::new(2, 1)), Some(ElevationProblem::RampUphill { dir: Dir::S }));
        assert_eq!(map.problems().len(), 1);

        // A ramp from level 1 on the plateau, climbing north, with nothing at level 2 above it.
        set(&mut map, (2, 1), 0, Shape::Flat);
        set(&mut map, (4, 2), 1, Shape::Ramp(Dir::N));
        assert_eq!(map.problem_at(Cell::new(4, 2)), Some(ElevationProblem::RampUphill { dir: Dir::N }));

        // A chain: (1, 4) at 0 and (2, 4) at 1, both climbing north, onto ground at 2.
        let mut chain = ElevationMap::new(Size::new(6, 6));
        set(&mut chain, (1, 4), 0, Shape::Ramp(Dir::N));
        set(&mut chain, (2, 4), 1, Shape::Ramp(Dir::N));
        set(&mut chain, (3, 4), 2, Shape::Flat);
        assert_eq!(chain.problem_at(Cell::new(1, 4)), None);
        assert_eq!(chain.problem_at(Cell::new(2, 4)), None);
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
        set(&mut map, (1, 1), 1, Shape::Ramp(Dir::N));
        let cell = Cell::new(1, 1);
        assert_eq!(map.corner_height(cell, 0.5, -0.5), 2.0);
        assert_eq!(map.corner_height(cell, -0.5, 0.5), 1.0);
        assert_eq!(map.centre_height(cell), 1.5);
        assert_eq!(map.surface_height(CellF32(common::Vec2::new(1.25, 1.0))), 1.75);
        assert_eq!(map.surface_height(CellF32(common::Vec2::new(5.0, 5.0))), 0.0);
    }

    #[test]
    fn test_brushes() {
        let mut map = map(7);
        let centre = Cell::new(3, 3);
        let cells = brush_cells(&map, centre, 3);
        assert_eq!(cells.len(), 9);
        assert_eq!(brush_cells(&map, Cell::new(0, 0), 5).len(), 9);

        // Raise: everything under the brush up to the stroke's level, one above where it started.
        let level = HeightTool::Raise.stroke_level(&map, centre);
        assert_eq!(level, Some(1));
        let edits = brush_edits(&map, HeightTool::Raise, &cells, level);
        assert_eq!(edits.len(), 9);
        for (cell, byte) in edits {
            map.set_byte(cell, byte);
        }
        // Painting over it again changes nothing: a stroke doesn't stack.
        assert!(brush_edits(&map, HeightTool::Raise, &cells, level).is_empty());

        // Terrace at 2 over a wider brush: only the raised cells, one level below it, rise.
        let wide = brush_cells(&map, centre, 5);
        let edits = brush_edits(&map, HeightTool::Terrace(2), &wide, Some(2));
        assert_eq!(edits.len(), 9);

        // Lower from the plateau: down to 0.
        let level = HeightTool::Lower.stroke_level(&map, centre);
        assert_eq!(level, Some(0));
        assert_eq!(brush_edits(&map, HeightTool::Lower, &[centre], level), vec![(centre, 0)]);

        // A ramp keeps its cell's level; flatten makes it flat again.
        assert_eq!(brush_edits(&map, HeightTool::Ramp(Dir::E), &[centre], None), vec![(
            centre,
            encode(1, Shape::Ramp(Dir::E))
        )]);
        map.set_byte(centre, encode(1, Shape::Ramp(Dir::E)));
        assert_eq!(brush_edits(&map, HeightTool::Flatten, &[centre], None), vec![(centre, encode(1, Shape::Flat))]);
    }

    #[test]
    fn test_smooth_brings_cells_within_two_levels() {
        let mut map = map(5);
        map.set_byte(Cell::new(2, 2), encode(5, Shape::Flat));
        let edits = brush_edits(&map, HeightTool::Smooth, &[Cell::new(2, 2)], None);
        assert_eq!(edits, vec![(Cell::new(2, 2), encode(MAX_CLIFF_LEVELS, Shape::Flat))]);

        // A pit is filled up to within two levels of the ground around it.
        let mut pit = ElevationMap::new(Size::new(3, 3));
        for y in 0..3 {
            for x in 0..3 {
                pit.set_byte(Cell::new(x, y), encode(4, Shape::Flat));
            }
        }
        pit.set_byte(Cell::new(1, 1), encode(0, Shape::Flat));
        let edits = brush_edits(&pit, HeightTool::Smooth, &[Cell::new(1, 1)], None);
        assert_eq!(edits, vec![(Cell::new(1, 1), encode(2, Shape::Flat))]);
    }
}
