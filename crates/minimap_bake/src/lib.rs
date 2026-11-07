// The minimap's painted base (docs/plans/minimap.md, "The bake"): per-cell terrain in, RGBA8
// pixels out, at `px_per_cell` pixels per cell. Soft shores with a foam line, water shaded by its
// distance from land, blurred forest and rock masses, and gentle hill shading from a seeded noise
// field standing in for elevation.
//
// It's a pure function of the grid and the style. Every pixel depends only on the cells within
// `margin_cells` of its own, and is computed the same way whatever region is baked, so baking a
// patch around an edit gives exactly the bytes a full bake would (see the tests).

// ----------------------------------------------
// Inputs
// ----------------------------------------------

// The ground of a cell: its terrain tile. Roads count as land: the minimap draws them as lines.
#[repr(u8)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum Terrain {
    #[default]
    None, // No terrain tile: drawn in the void colour.
    Land,
    Water,
    VacantLot,
}

// What stands on a cell, as far as the bake cares: buildings are drawn as shapes on top.
#[repr(u8)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum Cover {
    #[default]
    None,
    Vegetation,
    Rocks,
    Building,
}

// One entry per cell, row-major: index = x + y * width.
#[derive(Copy, Clone)]
pub struct Grid<'a> {
    pub width: i32,
    pub height: i32,
    pub terrain: &'a [Terrain],
    pub cover: &'a [Cover],
}

impl Grid<'_> {
    #[inline]
    fn index(&self, x: i32, y: i32) -> usize {
        (x + y * self.width) as usize
    }

    #[inline]
    fn clamped(&self, x: i32, y: i32) -> usize {
        self.index(x.clamp(0, self.width - 1), y.clamp(0, self.height - 1))
    }

    #[inline]
    fn is_water(&self, x: i32, y: i32) -> bool {
        // Past the map's edge counts as water, as for the shores.
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return true;
        }
        self.terrain[self.index(x, y)] == Terrain::Water
    }
}

// Colours are linear 0..255 RGB, as written to the texture.
pub type Rgb = [f32; 3];

#[derive(Clone, Debug)]
pub struct BakeStyle {
    pub land_low: Rgb,
    pub land_high: Rgb,
    pub forest: Rgb,
    pub rocks: Rgb,
    pub water_shallow: Rgb,
    pub water_deep: Rgb,
    pub foam: Rgb,
    pub vacant_lot: Rgb,
    pub vacant_lot_alpha: f32,
    pub void: Rgb,

    // Gaussian blur of each mask, in cells.
    pub blur_water: f32,
    pub blur_vegetation: f32,
    pub blur_rocks: f32,
    pub blur_vacant_lot: f32,

    pub max_depth_cells: i32, // Water this far from land, or farther, is deepest.
    pub foam_width: f32,      // Of the band around the shoreline, in mask units (0.5 is the shore).
    pub foam_strength: f32,
    pub hill_shade: [f32; 2],    // Clamp range of the shading factor.
    pub hill_strength: f32,      // Shading per unit of elevation slope.
    pub elevation_cells: f32,    // Size of the elevation noise's features, in cells.
    pub forest_texture: f32,     // How much the foliage noise varies forests (0..1).
    pub land_texture: f32,       // How much it varies open land (0..1).
}

impl Default for BakeStyle {
    // Đại Long's colours, from the mockups (docs/design/mockups/generator/minimap.py).
    fn default() -> Self {
        Self {
            land_low: hex(0xa7bd72),
            land_high: hex(0xcdbb83),
            forest: hex(0x5d8c48),
            rocks: hex(0xa19584),
            water_shallow: hex(0x6db3c2),
            water_deep: hex(0x2f6f8c),
            foam: hex(0xf3f0dc),
            vacant_lot: hex(0xd8c99a),
            vacant_lot_alpha: 0.7,
            void: hex(0x000000),
            blur_water: 0.3,
            blur_vegetation: 0.4,
            blur_rocks: 0.4,
            blur_vacant_lot: 0.15,
            max_depth_cells: 6,
            foam_width: 0.09,
            foam_strength: 0.85,
            hill_shade: [0.75, 1.2],
            hill_strength: 2.2,
            elevation_cells: 14.0,
            forest_texture: 0.2,
            land_texture: 0.04,
        }
    }
}

pub const fn hex(rgb: u32) -> Rgb {
    [((rgb >> 16) & 0xff) as f32, ((rgb >> 8) & 0xff) as f32, (rgb & 0xff) as f32]
}

// ----------------------------------------------
// CellRect
// ----------------------------------------------

// Cells from (x0, y0) up to, not including, (x1, y1).
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl CellRect {
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub const fn cell(x: i32, y: i32) -> Self {
        Self::new(x, y, x + 1, y + 1)
    }

    pub const fn map(width: i32, height: i32) -> Self {
        Self::new(0, 0, width, height)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    #[inline]
    pub fn width(&self) -> i32 {
        (self.x1 - self.x0).max(0)
    }

    #[inline]
    pub fn height(&self) -> i32 {
        (self.y1 - self.y0).max(0)
    }

    #[inline]
    pub fn area(&self) -> i64 {
        self.width() as i64 * self.height() as i64
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self::new(self.x0.min(other.x0), self.y0.min(other.y0), self.x1.max(other.x1), self.y1.max(other.y1))
    }

    pub fn grown(&self, cells: i32) -> Self {
        Self::new(self.x0 - cells, self.y0 - cells, self.x1 + cells, self.y1 + cells)
    }

    pub fn clipped(&self, width: i32, height: i32) -> Self {
        Self::new(self.x0.max(0), self.y0.max(0), self.x1.min(width), self.y1.min(height))
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

// ----------------------------------------------
// Sizes
// ----------------------------------------------

// Pixels per cell for a map: as many as fit `max_texture_side`, from 2 to 8.
pub fn px_per_cell(map_width: i32, map_height: i32, max_texture_side: u32) -> u32 {
    let side = map_width.max(map_height).max(1) as u32;
    (max_texture_side / side).clamp(2, 8)
}

// How far, in cells, an edit reaches into the baked pixels: a changed cell changes the pixels of
// the cells within this distance, and nothing farther.
pub fn margin_cells(style: &BakeStyle) -> i32 {
    let blur = [style.blur_water, style.blur_vegetation, style.blur_rocks, style.blur_vacant_lot]
        .into_iter()
        .map(kernel_radius)
        .max()
        .unwrap_or(0);
    blur.max(style.max_depth_cells.max(1) + 1)
}

// ----------------------------------------------
// Scratch
// ----------------------------------------------

// Buffers reused between bakes, so a bake allocates nothing once they've grown.
#[derive(Default)]
pub struct Scratch {
    rows: [Vec<f32>; MASK_COUNT], // Horizontal pass of each mask, per cell row and pixel column.
    depth: Vec<i32>,              // Chamfer distance from land, per cell of the depth window.
    kernels: Vec<Kernel>,
    kernels_key: Option<(u32, [u32; MASK_COUNT])>,
    lines: [Vec<f32>; MASK_COUNT],  // The masks along one pixel row.
    depth_line: Vec<f32>,           // Depth along one pixel row, per cell column.
    elevation_line: Vec<[f32; 3]>,  // Elevation along one pixel row, per cell column.
    elevation: Vec<[f32; 3]>, // Value and gradient at each cell corner of the region, grown by one.
    foliage: Vec<f32>,        // One tile of the foliage noise, FOLIAGE_PERIOD cells square, per pixel.
    foliage_key: Option<(u32, u32)>,
}

// The foliage noise repeats every this many cells, so one tile of it is computed per bake size.
const FOLIAGE_PERIOD: i32 = 24;
const FOLIAGE_LATTICE: i32 = 32; // Lattice points per period: 0.75 cells apart.

// ----------------------------------------------
// Bake
// ----------------------------------------------

const MASK_COUNT: usize = 4;
const WATER: usize = 0;
const VEGETATION: usize = 1;
const ROCKS: usize = 2;
const VACANT_LOT: usize = 3;

// Chamfer distance weights: 3 for a side step, 4 for a diagonal one.
const CHAMFER_SIDE: i32 = 3;
const CHAMFER_DIAGONAL: i32 = 4;

// Bakes the cells of `cells` (clipped to the map) into `out`, the whole texture: width × height
// cells at `px` pixels each, RGBA8, rows from cell y = 0 down. Pixels outside `cells` are left
// alone.
pub fn bake_region(grid: &Grid, style: &BakeStyle, cells: CellRect, px: u32, scratch: &mut Scratch, out: &mut [u8]) {
    assert!(grid.width > 0 && grid.height > 0);
    assert_eq!(grid.terrain.len(), (grid.width * grid.height) as usize);
    assert_eq!(grid.cover.len(), grid.terrain.len());
    assert!(px > 0);

    let tex_width = grid.width as usize * px as usize;
    assert!(out.len() >= tex_width * grid.height as usize * px as usize * 4);

    let region = cells.clipped(grid.width, grid.height);
    if region.is_empty() {
        return;
    }

    let seed = map_seed(grid);

    prepare_kernels(style, px, scratch);
    prepare_foliage(px, seed, scratch);
    horizontal_pass(grid, region, px, scratch);
    depth_pass(grid, style, region, scratch);
    elevation_pass(style, region, seed, scratch);

    let p = px as i32;
    let depth_window = depth_window(grid, style, region);
    let cap = style.max_depth_cells.max(1) * CHAMFER_SIDE;
    let light = light_direction();

    let first_row = rows_first(region, kernel_max_radius(scratch));
    let row_width = (region.width() * p) as usize;
    let columns = (region.width() + 2) as usize; // Cell columns x0 - 1 ..= x1.
    let period = (FOLIAGE_PERIOD * p) as usize;

    let Scratch { rows, depth, kernels, elevation, foliage, lines, depth_line, elevation_line, .. } = scratch;
    for line in lines.iter_mut() {
        line.clear();
        line.resize(row_width, 0.0);
    }
    depth_line.clear();
    depth_line.resize(columns, 0.0);
    elevation_line.clear();
    elevation_line.resize(columns, [0.0; 3]);

    for y in (region.y0 * p)..(region.y1 * p) {
        let cy = y / p;
        let sy = (y % p) as usize;

        // Blurred masks for the pixel row: the vertical pass over the horizontal one.
        for mask in 0..MASK_COUNT {
            let kernel = &kernels[mask];
            let line = &mut lines[mask];
            line.fill(0.0);
            for (k, weight) in kernel.weights(sy).iter().enumerate() {
                let row = (cy + k as i32 - kernel.radius).clamp(0, grid.height - 1);
                let start = (row - first_row) as usize * row_width;
                for (value, source) in line.iter_mut().zip(&rows[mask][start..start + row_width]) {
                    *value += source * weight;
                }
            }
        }

        // Depth and elevation along the row, between the cell rows above and below it: cell
        // centres are at whole numbers.
        let v = (y as f32 + 0.5) / px as f32 - 0.5;
        let (y0, fy) = (v.floor() as i32, v - v.floor());
        for column in 0..columns {
            let x = region.x0 - 1 + column as i32;

            let d = |x: i32, y: i32| -> f32 {
                let x = x.clamp(0, grid.width - 1);
                let y = y.clamp(0, grid.height - 1);
                depth[((x - depth_window.x0) + (y - depth_window.y0) * depth_window.width()) as usize].min(cap) as f32
            };
            depth_line[column] = d(x, y0) * (1.0 - fy) + d(x, y0 + 1) * fy;

            let corners = |row: i32| elevation[column + (row - region.y0 + 1) as usize * columns];
            let (top, bottom) = (corners(y0), corners(y0 + 1));
            elevation_line[column] = [0, 1, 2].map(|i| top[i] + (bottom[i] - top[i]) * fy);
        }

        let row_offset = y as usize * tex_width;
        let foliage_row = &foliage[(y as usize % period) * period..][..period];

        for x in (region.x0 * p)..(region.x1 * p) {
            let cx = x / p;
            let column = (x - region.x0 * p) as usize;
            let masks = [lines[0][column], lines[1][column], lines[2][column], lines[3][column]];

            let u = (x as f32 + 0.5) / px as f32 - 0.5;
            let (x0, fx) = (u.floor() as i32, u - u.floor());
            let left = (x0 - region.x0 + 1) as usize;

            let depth = (depth_line[left] * (1.0 - fx) + depth_line[left + 1] * fx) / CHAMFER_SIDE as f32;
            let (a, b) = (elevation_line[left], elevation_line[left + 1]);
            let elevation = a[0] + (b[0] - a[0]) * fx;
            let gradient = [a[1] + (b[1] - a[1]) * fx, a[2] + (b[2] - a[2]) * fx];
            let foliage = foliage_row[x as usize % period];

            // Cells with no terrain stay void, crisp.
            let color = if grid.terrain[grid.index(cx, cy)] == Terrain::None {
                style.void
            } else {
                shade_pixel(style, &masks, depth, elevation, gradient, light, foliage)
            };

            let offset = (row_offset + x as usize) * 4;
            out[offset..offset + 4].copy_from_slice(&[to_u8(color[0]), to_u8(color[1]), to_u8(color[2]), 255]);
        }
    }
}

// Bakes the whole map.
pub fn bake_all(grid: &Grid, style: &BakeStyle, px: u32, scratch: &mut Scratch, out: &mut [u8]) {
    bake_region(grid, style, CellRect::map(grid.width, grid.height), px, scratch, out);
}

fn shade_pixel(style: &BakeStyle, masks: &[f32; MASK_COUNT], depth: f32, elevation: f32, gradient: [f32; 2], light: [f32; 2], foliage: f32) -> Rgb {
    let water = masks[WATER].clamp(0.0, 1.0);

    // Land: low to high ground, forests and rocks, then the hill shading.
    let mut land = lerp3(style.land_low, style.land_high, ((elevation - 0.35) * 1.6).clamp(0.0, 1.0));
    land = scale3(land, 1.0 + (foliage - 0.5) * 2.0 * style.land_texture);
    let forest = (masks[VEGETATION] * 1.4).clamp(0.0, 1.0) * (1.0 - style.forest_texture + style.forest_texture * foliage);
    land = lerp3(land, style.forest, forest);
    land = lerp3(land, style.rocks, (masks[ROCKS] * 1.5).clamp(0.0, 1.0));
    land = lerp3(land, style.vacant_lot, masks[VACANT_LOT].clamp(0.0, 1.0) * style.vacant_lot_alpha);

    let slope = gradient[0] * light[0] + gradient[1] * light[1];
    let shade = (1.0 - slope * style.hill_strength).clamp(style.hill_shade[0], style.hill_shade[1]);
    land = scale3(land, shade);

    // Water: shallow by the shore, deep out from it.
    let max_depth = style.max_depth_cells.max(1) as f32;
    let deep = ((depth - 1.0) / (max_depth - 1.0).max(1.0)).clamp(0.0, 1.0);
    let sea = lerp3(style.water_shallow, style.water_deep, smoothstep(deep));

    let mut color = lerp3(land, sea, water);

    // Foam: a band around the shoreline, where the blurred water mask crosses one half.
    let width = style.foam_width.max(1e-4);
    let distance = (water - 0.5) / width;
    if distance.abs() < 4.0 {
        let foam = (-distance * distance).exp() * style.foam_strength;
        color = lerp3(color, style.foam, foam);
    }

    color
}

// ----------------------------------------------
// Blur
// ----------------------------------------------

// A cell mask blurred by a Gaussian, sampled at a pixel: each cell is a box, so a cell's weight
// is the Gaussian's integral over it, a difference of two normal CDFs. Weights depend only on the
// pixel's position within its cell, so they're tabulated per sub-pixel.
struct Kernel {
    radius: i32,
    taps: usize,
    weights: Vec<f32>, // px × taps.
}

impl Kernel {
    fn new(sigma: f32, px: u32) -> Self {
        let radius = kernel_radius(sigma);
        let taps = (radius * 2 + 1) as usize;
        let mut weights = vec![0.0; px as usize * taps];

        for sub in 0..px as usize {
            // The pixel centre's offset from its cell's centre, in cells.
            let d = (sub as f32 + 0.5) / px as f32 - 0.5;
            let row = &mut weights[sub * taps..(sub + 1) * taps];

            if sigma <= 0.0 {
                row[radius as usize] = 1.0;
                continue;
            }

            let mut total = 0.0;
            for (k, weight) in row.iter_mut().enumerate() {
                let offset = k as f32 - radius as f32; // The cell's position relative to the pixel's cell.
                let low = (offset - 0.5 - d) / sigma;
                let high = (offset + 0.5 - d) / sigma;
                *weight = normal_cdf(high) - normal_cdf(low);
                total += *weight;
            }
            for weight in row.iter_mut() {
                *weight /= total;
            }
        }

        Self { radius, taps, weights }
    }

    #[inline]
    fn weights(&self, sub: usize) -> &[f32] {
        &self.weights[sub * self.taps..(sub + 1) * self.taps]
    }
}

fn kernel_radius(sigma: f32) -> i32 {
    if sigma <= 0.0 { 0 } else { (sigma * 3.0 + 0.5).ceil() as i32 }
}

fn kernel_max_radius(scratch: &Scratch) -> i32 {
    scratch.kernels.iter().map(|kernel| kernel.radius).max().unwrap_or(0)
}

fn prepare_kernels(style: &BakeStyle, px: u32, scratch: &mut Scratch) {
    let sigmas = [style.blur_water, style.blur_vegetation, style.blur_rocks, style.blur_vacant_lot];
    let key = (px, sigmas.map(f32::to_bits));
    if scratch.kernels_key == Some(key) {
        return;
    }
    scratch.kernels = sigmas.iter().map(|sigma| Kernel::new(*sigma, px)).collect();
    scratch.kernels_key = Some(key);
}

// The first cell row the horizontal pass covers for `region`.
fn rows_first(region: CellRect, radius: i32) -> i32 {
    (region.y0 - radius).max(0)
}

#[inline]
fn mask_value(grid: &Grid, mask: usize, x: i32, y: i32) -> f32 {
    let index = grid.clamped(x, y);
    let on = match mask {
        WATER => grid.terrain[index] == Terrain::Water,
        VEGETATION => grid.cover[index] == Cover::Vegetation,
        ROCKS => grid.cover[index] == Cover::Rocks,
        _ => grid.terrain[index] == Terrain::VacantLot,
    };
    if on { 1.0 } else { 0.0 }
}

// Blurs each mask along x, for the cell rows the vertical pass reads, over the region's pixel
// columns.
fn horizontal_pass(grid: &Grid, region: CellRect, px: u32, scratch: &mut Scratch) {
    let p = px as i32;
    let radius = kernel_max_radius(scratch);
    let first_row = rows_first(region, radius);
    let last_row = (region.y1 + radius).min(grid.height); // Exclusive.
    let width = (region.width() * p) as usize;

    for mask in 0..MASK_COUNT {
        let kernel = &scratch.kernels[mask];
        let rows = &mut scratch.rows[mask];
        rows.clear();
        rows.resize(width * (last_row - first_row) as usize, 0.0);

        for row in first_row..last_row {
            let line = &mut rows[(row - first_row) as usize * width..(row - first_row + 1) as usize * width];
            for (column, value) in line.iter_mut().enumerate() {
                let x = region.x0 * p + column as i32;
                let cx = x / p;
                let sx = (x % p) as usize;
                let mut sum = 0.0;
                for (k, weight) in kernel.weights(sx).iter().enumerate() {
                    sum += mask_value(grid, mask, cx + k as i32 - kernel.radius, row) * weight;
                }
                *value = sum;
            }
        }
    }
}

// ----------------------------------------------
// Depth
// ----------------------------------------------

// The cells whose depth the region's pixels read, grown by the depth cap so their distances are
// exact (see depth_pass).
fn depth_window(grid: &Grid, style: &BakeStyle, region: CellRect) -> CellRect {
    region.grown(1 + style.max_depth_cells.max(1)).clipped(grid.width, grid.height)
}

// Chamfer distance from the nearest land cell, in thirds of a cell, over the depth window. Two
// passes give exact chamfer distances on a rectangle. A cell within the cap of land has that land
// within the window, so its distance is the same as a full bake's; a cell farther out reaches
// the cap either way.
fn depth_pass(grid: &Grid, style: &BakeStyle, region: CellRect, scratch: &mut Scratch) {
    let window = depth_window(grid, style, region);
    let width = window.width();
    let height = window.height();
    let far = i32::MAX / 4;

    let depth = &mut scratch.depth;
    depth.clear();
    depth.resize((width * height) as usize, far);

    for y in 0..height {
        for x in 0..width {
            if !grid.is_water(window.x0 + x, window.y0 + y) {
                depth[(x + y * width) as usize] = 0;
            }
        }
    }

    let at = |x: i32, y: i32| (x + y * width) as usize;

    // Forward pass: from the top left.
    for y in 0..height {
        for x in 0..width {
            let mut d = depth[at(x, y)];
            if x > 0 {
                d = d.min(depth[at(x - 1, y)] + CHAMFER_SIDE);
            }
            if y > 0 {
                d = d.min(depth[at(x, y - 1)] + CHAMFER_SIDE);
                if x > 0 {
                    d = d.min(depth[at(x - 1, y - 1)] + CHAMFER_DIAGONAL);
                }
                if x + 1 < width {
                    d = d.min(depth[at(x + 1, y - 1)] + CHAMFER_DIAGONAL);
                }
            }
            depth[at(x, y)] = d;
        }
    }

    // Backward pass: from the bottom right.
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let mut d = depth[at(x, y)];
            if x + 1 < width {
                d = d.min(depth[at(x + 1, y)] + CHAMFER_SIDE);
            }
            if y + 1 < height {
                d = d.min(depth[at(x, y + 1)] + CHAMFER_SIDE);
                if x + 1 < width {
                    d = d.min(depth[at(x + 1, y + 1)] + CHAMFER_DIAGONAL);
                }
                if x > 0 {
                    d = d.min(depth[at(x - 1, y + 1)] + CHAMFER_DIAGONAL);
                }
            }
            depth[at(x, y)] = d;
        }
    }
}

// ----------------------------------------------
// Noise
// ----------------------------------------------

// Samples the elevation noise at the cell corners the region's pixels interpolate between.
fn elevation_pass(style: &BakeStyle, region: CellRect, seed: u32, scratch: &mut Scratch) {
    let width = region.width() + 2;
    let height = region.height() + 2;
    scratch.elevation.clear();
    scratch.elevation.reserve((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let (value, gradient) = elevation((region.x0 - 1 + x) as f32, (region.y0 - 1 + y) as f32, style.elevation_cells, seed);
            scratch.elevation.push([value, gradient[0], gradient[1]]);
        }
    }
}

// One tile of the foliage noise at `px` pixels per cell. It wraps: its lattice repeats every
// FOLIAGE_PERIOD cells.
fn prepare_foliage(px: u32, seed: u32, scratch: &mut Scratch) {
    if scratch.foliage_key == Some((px, seed)) {
        return;
    }
    let period = (FOLIAGE_PERIOD * px as i32) as usize;
    let lattice_per_px = FOLIAGE_LATTICE as f32 / period as f32;
    scratch.foliage.clear();
    scratch.foliage.reserve(period * period);
    for y in 0..period {
        for x in 0..period {
            let lx = (x as f32 + 0.5) * lattice_per_px;
            let ly = (y as f32 + 0.5) * lattice_per_px;
            scratch.foliage.push(wrapped_value_noise(lx, ly, FOLIAGE_LATTICE, seed ^ 0x9e37_79b9));
        }
    }
    scratch.foliage_key = Some((px, seed));
}

fn wrapped_value_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let h = |x: i32, y: i32| hash(x.rem_euclid(period), y.rem_euclid(period), seed);
    let (a, b, c, d) = (h(ix, iy), h(ix + 1, iy), h(ix, iy + 1), h(ix + 1, iy + 1));
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn map_seed(grid: &Grid) -> u32 {
    (grid.width as u32).wrapping_mul(73_856_093) ^ (grid.height as u32).wrapping_mul(19_349_663) ^ 0x6d69_6e69
}

// Light from the upper left of the screen, which is +y in cells (north is +x, upper right).
fn light_direction() -> [f32; 2] {
    [0.0, 1.0]
}

#[inline]
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

// Value noise in [0, 1] with its gradient, smooth (quintic fade) between lattice points.
fn value_noise(x: f32, y: f32, seed: u32) -> (f32, [f32; 2]) {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ix, iy) = (ix as i32, iy as i32);

    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);

    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let fade_d = |t: f32| 30.0 * t * t * (t * (t - 2.0) + 1.0);
    let (u, v) = (fade(fx), fade(fy));
    let (du, dv) = (fade_d(fx), fade_d(fy));

    let value = a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
    let gx = du * ((b - a) + (a - b - c + d) * v);
    let gy = dv * ((c - a) + (a - b - c + d) * u);
    (value, [gx, gy])
}

// Two octaves of value noise, in [0, 1], and its gradient per cell.
fn elevation(u: f32, v: f32, feature_cells: f32, seed: u32) -> (f32, [f32; 2]) {
    let scale = 1.0 / feature_cells.max(1.0);
    let (n1, g1) = value_noise(u * scale, v * scale, seed);
    let (n2, g2) = value_noise(u * scale * 2.5, v * scale * 2.5, seed ^ 0x51ed_270b);
    let value = (n1 + n2 * 0.4) / 1.4;
    let gradient = [(g1[0] * scale + g2[0] * scale * 2.5 * 0.4) / 1.4, (g1[1] * scale + g2[1] * scale * 2.5 * 0.4) / 1.4];
    (value, gradient)
}

// ----------------------------------------------
// Mipmaps
// ----------------------------------------------

// Box-filters the pixels (x0, y0) up to (x1, y1) of a mip level from the level above it, RGBA8.
// Odd sizes repeat their last row or column.
pub fn downsample(parent: &[u8], parent_size: (i32, i32), target: &mut [u8], size: (i32, i32), rect: (i32, i32, i32, i32)) {
    let (x0, y0, x1, y1) = rect;
    let (pw, ph) = parent_size;
    for y in y0.max(0)..y1.min(size.1) {
        for x in x0.max(0)..x1.min(size.0) {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(pw - 1);
                let sy = (y * 2 + dy).min(ph - 1);
                let i = ((sy * pw + sx) * 4) as usize;
                for (c, total) in sum.iter_mut().enumerate() {
                    *total += parent[i + c] as u32;
                }
            }
            let i = ((y * size.0 + x) * 4) as usize;
            for (c, total) in sum.iter().enumerate() {
                target[i + c] = ((total + 2) / 4) as u8;
            }
        }
    }
}

// ----------------------------------------------
// Maths
// ----------------------------------------------

// Abramowitz and Stegun 7.1.26: |error| < 1.5e-7.
fn erf(x: f32) -> f32 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs() as f64;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592) * t * (-x * x).exp();
    sign * y as f32
}

fn normal_cdf(x: f32) -> f32 {
    0.5 * (1.0 + erf(x / std::f32::consts::SQRT_2))
}

#[inline]
fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

#[inline]
fn scale3(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn to_u8(value: f32) -> u8 {
    (value + 0.5).clamp(0.0, 255.0) as u8
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMap {
        width: i32,
        height: i32,
        terrain: Vec<Terrain>,
        cover: Vec<Cover>,
    }

    impl TestMap {
        fn new(width: i32, height: i32, terrain: Terrain) -> Self {
            let count = (width * height) as usize;
            Self { width, height, terrain: vec![terrain; count], cover: vec![Cover::None; count] }
        }

        fn grid(&self) -> Grid<'_> {
            Grid { width: self.width, height: self.height, terrain: &self.terrain, cover: &self.cover }
        }

        fn set(&mut self, x: i32, y: i32, terrain: Terrain) {
            self.terrain[(x + y * self.width) as usize] = terrain;
        }

        fn bake(&self, px: u32) -> Vec<u8> {
            let mut out = vec![0; (self.width * self.height) as usize * (px * px) as usize * 4];
            bake_all(&self.grid(), &BakeStyle::default(), px, &mut Scratch::default(), &mut out);
            out
        }
    }

    fn pixel(out: &[u8], tex_width: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * tex_width + x) * 4;
        [out[i], out[i + 1], out[i + 2], out[i + 3]]
    }

    fn luma(p: [u8; 4]) -> f32 {
        0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32
    }

    // A pond in the middle of land: its centre is darker (deeper) than its edge.
    #[test]
    fn pond_centre_is_deeper_than_its_shore() {
        let mut map = TestMap::new(24, 24, Terrain::Land);
        for y in 4..20 {
            for x in 4..20 {
                map.set(x, y, Terrain::Water);
            }
        }
        let px = 4;
        let out = map.bake(px);
        let tex_width = 24 * px as usize;
        let centre = pixel(&out, tex_width, 12 * px as usize, 12 * px as usize);
        let near_shore = pixel(&out, tex_width, 5 * px as usize + 2, 12 * px as usize);
        assert!(luma(centre) < luma(near_shore), "centre {centre:?} should be darker than near the shore {near_shore:?}");
    }

    // Foam lies on the shoreline: the boundary between a land and a water cell is the lightest
    // spot along a line across it.
    #[test]
    fn foam_lies_on_the_shoreline() {
        let mut map = TestMap::new(16, 16, Terrain::Land);
        for y in 0..16 {
            for x in 8..16 {
                map.set(x, y, Terrain::Water);
            }
        }
        let px = 8;
        let out = map.bake(px);
        let tex_width = 16 * px as usize;
        let row = 8 * px as usize;
        let brightest = (5 * px as usize..11 * px as usize).max_by(|a, b| {
            luma(pixel(&out, tex_width, *a, row)).total_cmp(&luma(pixel(&out, tex_width, *b, row)))
        });
        let shore = 8 * px as usize;
        let brightest = brightest.unwrap();
        assert!(brightest.abs_diff(shore) <= 2, "foam peak at pixel {brightest}, shore at {shore}");
    }

    #[test]
    fn output_is_deterministic() {
        let mut map = TestMap::new(20, 20, Terrain::Land);
        map.set(3, 3, Terrain::Water);
        map.cover[50] = Cover::Vegetation;
        let a = map.bake(4);
        let b = map.bake(4);
        assert_eq!(a, b);
        let hash = a.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, byte| (h ^ *byte as u64).wrapping_mul(0x100_0000_01b3));
        let c = map.bake(4);
        let hash_c = c.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, byte| (h ^ *byte as u64).wrapping_mul(0x100_0000_01b3));
        assert_eq!(hash, hash_c);
    }

    // After edits, baking only the patch around each edit gives the same bytes as a full bake.
    #[test]
    fn patched_bake_equals_full_bake() {
        let (width, height, px) = (40, 32, 4);
        let mut map = TestMap::new(width, height, Terrain::Land);
        for y in 0..height {
            for x in 0..12 {
                map.set(x, y, Terrain::Water);
            }
        }

        let style = BakeStyle::default();
        let mut scratch = Scratch::default();
        let mut patched = vec![0; (width * height) as usize * (px * px) as usize * 4];
        bake_all(&map.grid(), &style, px, &mut scratch, &mut patched);

        let margin = margin_cells(&style);
        let mut seed = 12345u32;
        let mut next = |n: i32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 8) % n as u32) as i32
        };

        for _ in 0..25 {
            let (x, y) = (next(width), next(height));
            let index = (x + y * width) as usize;
            match next(4) {
                0 => map.terrain[index] = Terrain::Water,
                1 => map.terrain[index] = Terrain::Land,
                2 => map.cover[index] = Cover::Vegetation,
                _ => map.cover[index] = Cover::Rocks,
            }
            bake_region(&map.grid(), &style, CellRect::cell(x, y).grown(margin), px, &mut scratch, &mut patched);
        }

        let full = map.bake(px);
        assert!(patched == full, "a patched bake differs from a full bake");
    }

    #[test]
    fn px_per_cell_fits_the_texture() {
        assert_eq!(px_per_cell(128, 128, 1024), 8);
        assert_eq!(px_per_cell(256, 256, 1024), 4);
        assert_eq!(px_per_cell(1024, 1024, 1024), 2);
        assert_eq!(px_per_cell(32, 32, 1024), 8);
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    // cargo test -p minimap_bake --release -- --ignored --nocapture
    #[test]
    #[ignore]
    fn time_full_bakes() {
        for (side, max_side) in [(128, 1024u32), (256, 1024)] {
            let count = (side * side) as usize;
            let mut terrain = vec![Terrain::Land; count];
            let mut cover = vec![Cover::None; count];
            for i in 0..count {
                let (x, y) = ((i as i32) % side, (i as i32) / side);
                let river = ((x as f32 * 0.08).sin() * 10.0 + side as f32 * 0.4 - y as f32).abs() < 6.0;
                if river || (x - 20).pow(2) + (y - 30).pow(2) < 100 { terrain[i] = Terrain::Water; }
                else if ((x / 9 + y / 7) % 5 == 0) && (x * 31 + y * 17) % 3 != 0 { cover[i] = Cover::Vegetation; }
            }
            let grid = Grid { width: side, height: side, terrain: &terrain, cover: &cover };
            let px = px_per_cell(side, side, max_side);
            let mut out = vec![0; count * (px * px) as usize * 4];
            let mut scratch = Scratch::default();
            let start = std::time::Instant::now();
            bake_all(&grid, &BakeStyle::default(), px, &mut scratch, &mut out);
            let full = start.elapsed();
            let start = std::time::Instant::now();
            bake_region(&grid, &BakeStyle::default(), CellRect::cell(60, 60).grown(margin_cells(&BakeStyle::default())), px, &mut scratch, &mut out);
            println!("{side}x{side} at {px} px/cell: full {full:?}, one edit {:?}", start.elapsed());
        }
    }
}
